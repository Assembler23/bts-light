//! Quelle `badhub`: ein in badhub organisiertes Turnier, dessen Stand der
//! Turnierkern lokal aus den Ereignissen des Turnierpakets faltet (Paket L2,
//! badhub `docs/features/bts_light_steuerung.md`, ADR-0084).
//!
//! **Schreiben (Paket L3):** Unter der Lease übersetzt [`schreiben`] die
//! Schreibweichen in Kernbefehle; [`KernQuelle::ausfuehren`] prüft sie lokal
//! mit dem Kern und hängt Befehl + Ereignisse an das Protokoll
//! (`protokoll.jsonl`, fsync). Die Outbox ([`outbox_schleife`]) lädt die
//! Posten zu badhub hoch; ohne Netz läuft die Halle unverändert weiter.
//!
//! Ablauf:
//! - **Kopplung** ([`koppeln`]): Code aus badhub einlösen → Gerätetoken,
//!   Turnierpaket laden, beides unter `turniere/<uuid>/` ablegen.
//! - **Start** ([`starten`]): Paket von der Platte in den Prozesszustand.
//! - **Abgleich** ([`abgleich_schleife`]): online alle 5 s neue Ereignisse
//!   (`ereignisse?ab=n`), alle 60 s das Paket mit `If-None-Match`. Offline
//!   bleibt der letzte Stand stehen — die Halle merkt nichts.
//! - **Lesen** ([`stand`]): Kern-Faltung → [`abbildung::abbilden`].
//!
//! Keine Personennamen in Logs: Meldungen nennen nur Zahlen und Statuscodes.

pub mod abbildung;
pub mod ablage;
pub mod api;
pub mod lease;
pub mod paket;
pub mod protokoll;
pub mod schluessel;
pub mod schreiben;
pub mod slave;
pub mod standby;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use crate::btp::model::BtpSnapshot;
use crate::config::{AppConfig, BadhubTurnierConfig};

use ablage::Ablage;
use lease::{Lease, LeaseZustand};
use paket::Paket;
use protokoll::{Posten, Protokoll};
use turnierkern::{TurnierBefehl, TurnierEreignis, TurnierStand};

/// Version der eingebundenen Kern-Crate (gleich dem Tag in `Cargo.toml`,
/// geprüft im Test `kern_version_passt_zum_cargo_tag`).
pub const KERN_VERSION: &str = "0.23.0";

/// Takt der Ereignis-Abfrage.
const EREIGNIS_TAKT: Duration = Duration::from_secs(5);
/// Takt der Paket-Abfrage (Stammdaten).
const PAKET_TAKT: Duration = Duration::from_secs(60);

/// Prüft eine Turnier-UUID, bevor sie einen Pfad benennt: nur Hex-Ziffern
/// und Bindestriche, 8–64 Zeichen.
pub fn uuid_gueltig(uuid: &str) -> bool {
    (8..=64).contains(&uuid.len()) && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// API-Basis aus der Push-Adresse (`https://test.badhub.de/api/live_update.php`
/// → `https://test.badhub.de/api/torg/v1`). Folgt damit dem
/// Testsystem-Schalter. Klartext-HTTP nur für `localhost`/`127.0.0.1`
/// (Entwicklung): das Token reist im Kopf jeder Anfrage.
pub fn api_basis(push_url: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(push_url.trim())
        .map_err(|_| "Die badhub-Adresse in den Einstellungen ist ungültig.".to_string())?;
    let lokal = matches!(url.host_str(), Some("localhost") | Some("127.0.0.1"));
    if url.scheme() != "https" && !(lokal && url.scheme() == "http") {
        return Err("Die badhub-Adresse muss mit https:// beginnen.".into());
    }
    let origin = url.origin().ascii_serialization();
    Ok(format!("{origin}/api/torg/v1"))
}

/// Gültigkeit der zwischengespeicherten Abbildung: (`n`, Ereigniszahl, ETag,
/// Zahl der Meldungen im Personenauszug).
type AbbildSchluessel = (u64, usize, Option<String>, usize);

/// Höchstzahl der Posten je Hochladen (badhub `LeaseHochladen::MAX_POSTEN`).
pub const MAX_POSTEN: usize = 200;
/// Höchstzahl der Ereignisse je Hochladen (badhub `MAX_EREIGNISSE`).
pub const MAX_EREIGNISSE: usize = 1000;
/// Neuversuche der Outbox: 5, 10, 20, 40, 60 s, danach alle 60 s.
pub const OUTBOX_BACKOFF_S: [u64; 5] = [5, 10, 20, 40, 60];

/// Akteur eines Befehls aus der Halle bzw. aus badhub (nur lokal im Protokoll).
pub const AKTEUR_HALLE: &str = "bts-light";
pub const AKTEUR_BADHUB: &str = "badhub";

/// Warum ein Befehl nicht ausgeführt wurde.
#[derive(Debug, Clone, PartialEq)]
pub enum SchreibFehler {
    /// Der Kern lehnt ab (`grund` im Wortlaut des Kerns).
    Kern {
        text: String,
        grund: serde_json::Value,
    },
    /// Keine Schreibberechtigung (keine Lease, zurückgeholt, Kern-Version).
    Gesperrt(String),
    /// Befehl unbrauchbar oder Platte nicht schreibbar.
    Ungueltig(String),
}

impl std::fmt::Display for SchreibFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchreibFehler::Kern { text, .. } => write!(f, "{text}"),
            SchreibFehler::Gesperrt(t) | SchreibFehler::Ungueltig(t) => write!(f, "{t}"),
        }
    }
}

/// Text bei zurückgeholter Steuerung (badhub-Spec „Outbox“).
pub const TEXT_ENTZOGEN: &str = "Die Steuerung wurde in badhub zurückgeholt. Eingaben am \
     Turnier-PC gehen nicht mehr an badhub.";

pub(crate) fn jetzt_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Unix-ms → Uhrzeit `HH:MM` (lokal).
fn uhrzeit(ms: u64) -> String {
    schreiben::wandzeit(ms)
        .get(11..16)
        .unwrap_or("")
        .to_string()
}

/// Ablehnung von `POST turnier/lease` in Worten (badhub B1/B2).
pub fn lease_ablehnung_text(code: &str) -> String {
    match code {
        "uebernahme_nicht_freigegeben" => {
            "Übernahme in badhub im Bereich Turniertag freigeben (gilt 10 Minuten)".into()
        }
        "uebernehmen_nicht_verfuegbar" => {
            "ein anderes Gerät steuert das Turnier — Übernahme in badhub freigeben".into()
        }
        "relay_ungueltig" => "badhub lehnt die Relay-Angaben ab".into(),
        andere => {
            let c: String = andere
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(40)
                .collect();
            format!("badhub lehnt ab ({c})")
        }
    }
}

/// Ein Hochlade-Auftrag der Outbox.
#[derive(Debug, Clone)]
pub(crate) struct OutboxAuftrag {
    basis: String,
    token: String,
    epoche: u64,
    basis_n: u64,
    posten: Vec<serde_json::Value>,
    /// Erster Upload nach einer Standby-Übernahme (`standby: true`).
    standby: bool,
    /// Höchstes lokales `n` der mitgeschickten Posten.
    bis_lokal: u64,
}

/// Ergebnis eines Outbox-Schritts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxSchritt {
    /// Nichts zu tun (leer, keine Lease, angehalten).
    Ruhe,
    /// Hochgeladen bzw. Lücke aufgelöst — gleich weitermachen.
    Weiter,
    /// Netz/Server-Fehler — später erneut (Backoff).
    Spaeter,
    /// Endgültig angehalten (zurückgeholt, Abweichung).
    Angehalten,
}

/// Lage des Abgleichs für Hauptfenster und TL-Web.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Abgleich {
    /// `aktuell` | `wartet` | `keine_lease` | `entzogen` | `zurueckgegeben` | `angehalten`.
    pub zustand: String,
    /// Noch nicht von badhub bestätigte Ereignisse.
    pub wartend: usize,
    pub letzter_kontakt_ms: u64,
    /// Fertiger Text („badhub: aktuell“ / „5 Ereignisse warten, letzter Kontakt 10:47“).
    pub text: String,
    /// Darf „Turnier beenden“ jetzt? (Lease aktiv, Outbox leer.)
    pub kann_beenden: bool,
    /// Darf „Steuerung wieder übernehmen“? (zurückgegeben oder zurückgeholt.)
    pub kann_uebernehmen: bool,
    /// Roter Hinweis (zurückgeholt) — TL-Web und Hauptfenster zeigen ihn als Banner.
    pub rot: bool,
    /// Zusätzlicher Hinweis neben dem Abgleich (Standby-Übernahme: „k
    /// Eingaben abgewiesen“, „Hauptrechner war vor x s noch online“).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warnung: Option<String>,
}

/// Eine ausgeloste Meldung für den Rückzug im TL-Web.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RueckzugMeldung {
    /// Meldungsnummer (= `entry_id` im Modell).
    pub entry_id: i64,
    /// Name der Konkurrenz.
    pub konkurrenz: String,
    /// Namen der Meldung („Vorname Nachname / …“).
    pub label: String,
    pub zurueckgezogen: bool,
    /// `behalten` | `streichen`, nur bei zurückgezogenen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ergebnisse: Option<String>,
}

/// Laufzeitzustand eines gekoppelten Turniers.
pub struct KernQuelle {
    ablage: Ablage,
    basis: String,
    token: Option<String>,
    paket: Paket,
    etag: Option<String>,
    /// Zwischenspeicher der Abbildung, gültig für (`n`, Ereigniszahl, ETag).
    abbild: Option<(AbbildSchluessel, BtpSnapshot)>,
    /// badhub hat den Zugang abgelehnt (401/403): keine weiteren Anfragen
    /// bis zum nächsten Start bzw. zur neuen Kopplung.
    zugang_abgelehnt: bool,
    /// Lokales Protokoll (Paket L3).
    protokoll: Protokoll,
    /// Gespeicherte Lease (`None` = noch nie genommen).
    lease: Option<Lease>,
    /// TL-Zugang für badhub (Relay), gilt mit der Lease.
    tl_zugang: Option<String>,
    /// In diesem Lauf schon bei badhub angemeldet (`POST turnier/lease`)?
    lease_angemeldet: bool,
    /// Letzte Auskunft zur Lease (belegt, offline …) für die Anzeige.
    lease_hinweis: Option<String>,
    /// badhub nimmt die Posten nicht an (`422 abweichung`): Outbox steht.
    abweichung: Option<String>,
    /// Zwischenspeicher der Kern-Faltung (Schlüssel wie die Abbildung).
    kern: Option<((u64, usize), TurnierStand)>,
    /// Präfix der Befehls-IDs dieses Laufs und Zähler.
    id_praefix: String,
    id_zaehler: u64,
    /// Schlüssel zwischen Master und LAN-Ansage-Slave (`slave.bin`).
    pub(crate) slave_schluessel: Option<String>,
    /// Am Slave: die vom Master geholten Ereignisse.
    slave_ereignisse: Vec<TurnierEreignis>,
    /// Eine Rückgabe (`DELETE turnier/lease`) ist unterwegs.
    rueckgabe_laeuft: bool,
    // ── Standby (Paket S2) ──
    /// Gespiegelte Posten des Hauptrechners (`spiegel.jsonl`).
    spiegel: Protokoll,
    spiegel_stand: Option<standby::SpiegelStand>,
    /// Lease-Sicht aus badhub (`rolle.json`).
    rolle: Option<standby::Rolle>,
    /// Hinweis zur Spiegelung (Hauptrechner zu alt, nicht erreichbar).
    standby_hinweis: Option<String>,
    /// `403 kein_standby` beim Standby-Upload: Outbox angehalten.
    kein_standby: bool,
    /// Standby-Upload mit `epoche_veraltet` beantwortet: erst `GET
    /// /turnier/lease` fragen, ob die Übernahme schon galt (Antwort verloren).
    standby_pruefen: bool,
    /// `409 inhaber_aktiv`: der Hauptrechner war vor so vielen Sekunden bei badhub.
    inhaber_aktiv_s: Option<u64>,
    /// Notfall-Sicherung: zuletzt geschrieben bei `n`, zu welcher Zeit.
    notfall_n: Option<u64>,
    notfall_ms: u64,
}

// Debug von Hand: Gerätetoken und Relay-Zugang erscheinen nie im Klartext,
// der Personenauszug nicht im Ganzen (Security-Review L3).
impl std::fmt::Debug for KernQuelle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KernQuelle")
            .field("basis", &self.basis)
            .field("token", &self.token.as_ref().map(|_| "***"))
            .field("tl_zugang", &self.tl_zugang.as_ref().map(|_| "***"))
            .field("n", &self.n_lokal())
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}

impl KernQuelle {
    /// Lädt ein gekoppeltes Turnier von der Platte.
    pub fn laden(daten: &Path, cfg: &BadhubTurnierConfig) -> Result<Self, String> {
        // `basis` steht in der (von Hand änderbaren) config.json: dieselbe
        // https-Prüfung wie beim Koppeln, bevor das Token dorthin geht.
        api_basis(&cfg.basis)?;
        let ablage = Ablage::neu(daten, &cfg.uuid)?;
        let (paket, etag) = ablage.paket_laden()?.ok_or_else(|| {
            "Für das gekoppelte badhub-Turnier liegt kein Turnierpaket vor — bitte im Setup neu koppeln."
                .to_string()
        })?;
        let token = ablage.token_laden();
        let protokoll = Protokoll::laden(ablage.dir())?;
        let lease = lease::laden(ablage.dir());
        let tl_zugang = lease
            .as_ref()
            .filter(|l| l.aktiv())
            .and_then(|_| ablage.geheimnis_laden(lease::ZUGANG_DATEI));
        let ablage_dir_buf = ablage.dir().to_path_buf();
        let ablage_dir = ablage_dir_buf.as_path();
        let paket_lease = paket.turnier.lease.clone();
        let id_praefix = lease::neuer_zugang()
            .map(|z| z[..8].to_string())
            .unwrap_or_else(|_| format!("{:08x}", jetzt_ms() & 0xffff_ffff));
        Ok(Self {
            ablage,
            basis: cfg.basis.clone(),
            token,
            paket,
            etag,
            abbild: None,
            zugang_abgelehnt: false,
            protokoll,
            lease,
            tl_zugang,
            lease_angemeldet: false,
            lease_hinweis: None,
            abweichung: None,
            kern: None,
            id_praefix,
            id_zaehler: 0,
            slave_schluessel: None,
            slave_ereignisse: Vec::new(),
            rueckgabe_laeuft: false,
            spiegel: Protokoll::laden_als(ablage_dir, standby::SPIEGEL_DATEI)?,
            spiegel_stand: standby::spiegel_stand_laden(ablage_dir),
            rolle: standby::rolle_laden(ablage_dir).or_else(|| {
                paket_lease.map(|l| standby::Rolle {
                    lease: Some(l),
                    stand_ms: 0,
                })
            }),
            standby_hinweis: None,
            kein_standby: false,
            standby_pruefen: false,
            inhaber_aktiv_s: None,
            notfall_n: None,
            notfall_ms: 0,
        })
        .map(|mut q: Self| {
            q.protokoll_ohne_lease_raeumen();
            q.slave_schluessel = q
                .ablage
                .geheimnis_laden(slave::SCHLUESSEL_DATEI)
                .and_then(|s| slave::schluessel_normal(&s));
            q
        })
    }

    /// Ohne Lease kann kein Posten mehr hochgeladen werden (die Outbox
    /// braucht eine Epoche) — Posten im Protokoll sind dann von badhub
    /// bestätigte Reste. Liegen sie über einem gekappten Paket, zählten sie
    /// als „wartend“: kein Nachladen, keine Lease, kein Neu-Koppeln
    /// (v0.9.450 nach „Steuerung wieder übernehmen“ mit mehr als 2000
    /// Ereignissen). Deshalb räumen: was das Paket schon trägt, ersatzlos;
    /// was darüber liegt, mit Sicherung in `verworfen-<ms>.jsonl` — es kommt
    /// über das seitenweise Nachladen von badhub zurück.
    fn protokoll_ohne_lease_raeumen(&mut self) {
        if self.lease.is_some() || self.protokoll.posten().is_empty() {
            return;
        }
        let r = self
            .protokoll
            .kuerzen_auf(self.paket.n(), jetzt_ms())
            .and_then(|gesichert| self.protokoll.leeren().map(|()| gesichert));
        match r {
            Ok(gesichert) => tracing::info!(
                "Quelle badhub: Protokoll ohne Lease geräumt ({gesichert} Posten über dem Paket gesichert)"
            ),
            Err(e) => tracing::warn!("Quelle badhub: Protokoll ohne Lease nicht geräumt ({e})"),
        }
    }

    /// Alle Ereignisse: von badhub bestätigte (Paket) und die eigenen aus
    /// dem Protokoll, die darüber hinausgehen.
    pub fn alle_ereignisse(&self) -> Vec<TurnierEreignis> {
        let basis = self.paket.n();
        let mut alle = self.paket.ereignisse.clone();
        alle.extend(
            self.protokoll
                .posten()
                .iter()
                .flat_map(|p| p.ereignisse.iter())
                .filter(|e| e.n() > basis)
                .cloned(),
        );
        alle
    }

    /// Lokales `n` (inklusive noch nicht hochgeladener Ereignisse).
    pub fn n_lokal(&self) -> u64 {
        self.paket.n().max(self.protokoll.n())
    }

    /// Von badhub bestätigtes `n`.
    pub fn n_bestaetigt(&self) -> u64 {
        match &self.lease {
            Some(l) => l.bis_n.max(self.paket.n()),
            None => self.paket.n(),
        }
    }

    /// Posten, die noch auf badhub warten.
    pub fn wartende_posten(&self) -> usize {
        self.protokoll.offen_nach(self.n_bestaetigt()).count()
    }

    /// Ereignisse, die noch auf badhub warten.
    pub fn wartende_ereignisse(&self) -> usize {
        self.n_lokal().saturating_sub(self.n_bestaetigt()) as usize
    }

    fn faltungs_schluessel(&self) -> (u64, usize) {
        (
            self.n_lokal(),
            self.paket.ereignisse.len() + self.protokoll.posten().len(),
        )
    }

    /// Kern-Stand (Faltung nur bei neuen Daten).
    pub fn kern_stand(&mut self) -> TurnierStand {
        let schluessel = self.faltungs_schluessel();
        if let Some((s, st)) = &self.kern {
            if *s == schluessel {
                return st.clone();
            }
        }
        let st = turnierkern::falten_turnier(&self.alle_ereignisse());
        self.kern = Some((schluessel, st.clone()));
        st
    }

    /// Stand im heutigen Modell (Faltung nur bei neuen Daten).
    pub fn stand(&mut self) -> BtpSnapshot {
        let (n, zahl) = self.faltungs_schluessel();
        let schluessel = (n, zahl, self.etag.clone(), self.paket.meldungen.len());
        if let Some((s, snap)) = &self.abbild {
            if *s == schluessel {
                return snap.clone();
            }
        }
        let stand = self.kern_stand();
        let snap = abbildung::abbilden(&stand, &self.paket);
        self.abbild = Some((schluessel, snap.clone()));
        snap
    }

    /// Ereignisse mit `n > ab`, höchstens `max` — für den LAN-Slave. Klont
    /// nur die gelieferten, nicht das ganze Protokoll.
    pub fn ereignisse_ab(&self, ab: u64, max: usize) -> Vec<TurnierEreignis> {
        let basis = self.paket.n();
        self.paket
            .ereignisse
            .iter()
            .chain(
                self.protokoll
                    .posten()
                    .iter()
                    .flat_map(|p| p.ereignisse.iter())
                    .filter(move |e| e.n() > basis),
            )
            .filter(|e| e.n() > ab)
            .take(max)
            .cloned()
            .collect()
    }

    /// Am Slave: höchstes `n` der vom Master geholten Ereignisse.
    pub fn slave_n(&self) -> u64 {
        self.slave_ereignisse
            .iter()
            .map(TurnierEreignis::n)
            .max()
            .unwrap_or(0)
    }

    /// Am Slave: eine Seite vom Master übernehmen. Ist der Master hinter dem
    /// Slave (Steuerung neu übernommen, anderes Protokoll), beginnt der
    /// Slave von vorn.
    pub fn slave_uebernehmen(&mut self, master_n: u64, neu: Vec<TurnierEreignis>) {
        if master_n < self.slave_n() {
            self.slave_ereignisse.clear();
            return;
        }
        let mut bekannt: std::collections::BTreeSet<u64> = self
            .slave_ereignisse
            .iter()
            .map(TurnierEreignis::n)
            .collect();
        for e in neu {
            if bekannt.insert(e.n()) {
                self.slave_ereignisse.push(e);
            }
        }
        self.slave_ereignisse.sort_by_key(TurnierEreignis::n);
    }

    /// Am Slave: Stand aus den Ereignissen des Masters mit dem eigenen Paket.
    pub fn stand_slave(&mut self) -> BtpSnapshot {
        let n = self.slave_n();
        let schluessel = (
            n,
            self.slave_ereignisse.len(),
            self.etag.clone(),
            self.paket.meldungen.len(),
        );
        if let Some((s, snap)) = &self.abbild {
            if *s == schluessel {
                return snap.clone();
            }
        }
        let stand = turnierkern::falten_turnier(&self.slave_ereignisse);
        let snap = abbildung::abbilden(&stand, &self.paket);
        self.abbild = Some((schluessel, snap.clone()));
        snap
    }

    /// Schlüssel der lokalen Stores: die Turnier-UUID statt des Namens
    /// (zwei Turniere gleichen Namens teilen sich so keine Stores).
    pub fn turnier_schluessel(&self) -> String {
        format!("badhub:{}", self.paket.turnier.uuid.to_ascii_lowercase())
    }

    /// Darf dieser PC gerade schreiben?
    pub fn schreibrecht(&self) -> Result<(), SchreibFehler> {
        match &self.lease {
            Some(l) if l.zustand == LeaseZustand::Aktiv && l.neu_laden => {
                return Err(SchreibFehler::Gesperrt(
                    "Der Stand wird nach der Standby-Übernahme gerade von badhub neu geladen — \
                     bitte kurz warten."
                        .into(),
                ))
            }
            Some(l) if l.zustand == LeaseZustand::Aktiv => {}
            Some(l) if l.zustand == LeaseZustand::Entzogen => {
                return Err(SchreibFehler::Gesperrt(TEXT_ENTZOGEN.into()))
            }
            Some(_) => {
                return Err(SchreibFehler::Gesperrt(
                    "Die Steuerung ist an badhub zurückgegeben — Eingaben bitte in badhub.".into(),
                ))
            }
            None => {
                return Err(SchreibFehler::Gesperrt(format!(
                    "bts-light steuert dieses Turnier noch nicht (keine Lease von badhub){}.",
                    self.lease_hinweis
                        .as_deref()
                        .map(|h| format!(" — {h}"))
                        .unwrap_or_default()
                )))
            }
        }
        if !self.paket.kern_version.is_empty() && self.paket.kern_version != KERN_VERSION {
            return Err(SchreibFehler::Gesperrt(format!(
                "badhub rechnet mit Turnierkern {}, diese Version mit {KERN_VERSION} — bitte \
                 bts-light aktualisieren.",
                self.paket.kern_version
            )));
        }
        Ok(())
    }

    /// Neue, eindeutige Befehls-ID (`bts-<Lauf>-<Zeit>-<Zähler>`, nur
    /// Zeichen, die badhub annimmt).
    fn neue_id(&mut self, jetzt: u64) -> String {
        self.id_zaehler += 1;
        format!("bts-{}-{jetzt:x}-{}", self.id_praefix, self.id_zaehler)
    }

    /// Prüft die Befehle der Reihe nach mit dem Kern und hängt sie samt
    /// Ereignissen an das Protokoll — **alle oder keiner** (wie ein
    /// BTP-Schreibvorgang). Befehle ohne Kennung bekommen eine. Ein Befehl
    /// mit schon bekannter Kennung ist wirkungslos (Idempotenz des Kerns).
    /// Liefert die Zahl neuer Posten.
    pub fn ausfuehren(
        &mut self,
        befehle: Vec<TurnierBefehl>,
        akteur: &str,
    ) -> Result<usize, SchreibFehler> {
        self.schreibrecht()?;
        if befehle.is_empty() {
            return Ok(0);
        }
        let jetzt = jetzt_ms();
        let mut ereignisse = self.alle_ereignisse();
        let mut stand = self.kern_stand();
        let mut neu_posten = Vec::new();
        for b in befehle {
            let b = if b.id().is_empty() {
                let id = self.neue_id(jetzt);
                schreiben::mit_id(b, id)
            } else {
                b
            };
            match turnierkern::pruefen_turnier(&stand, &b) {
                Ok(neu) if neu.is_empty() => continue,
                Ok(neu) => {
                    ereignisse.extend(neu.iter().cloned());
                    stand = turnierkern::falten_turnier(&ereignisse);
                    neu_posten.push(Posten {
                        befehl: b,
                        ereignisse: neu,
                        akteur: akteur.to_string(),
                        zeit_ms: jetzt,
                    });
                }
                Err(a) => {
                    return Err(SchreibFehler::Kern {
                        text: schreiben::ablehnung_text(&a),
                        grund: serde_json::to_value(&a).unwrap_or(serde_json::Value::Null),
                    })
                }
            }
        }
        let zahl = neu_posten.len();
        self.protokoll
            .anhaengen(neu_posten)
            .map_err(SchreibFehler::Ungueltig)?;
        if zahl > 0 {
            self.kern = Some((self.faltungs_schluessel(), stand));
        }
        Ok(zahl)
    }

    /// Ein Kernbefehl aus badhub (Relay, `TlAction::KernBefehl`): nur die
    /// Durchführungs-Befehle, die badhub unter der Lease annimmt, mit einer
    /// Kennung, die badhub annimmt. `jetzt` setzt dieser PC (ADR-0084).
    pub fn befehl_von_badhub(&mut self, roh: serde_json::Value) -> Result<usize, SchreibFehler> {
        schreiben::befehl_begrenzt(&roh).map_err(SchreibFehler::Ungueltig)?;
        let mut roh = roh;
        let art = roh
            .get("art")
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();
        if !schreiben::von_badhub_erlaubt(&art) {
            return Err(SchreibFehler::Ungueltig(format!(
                "Dieser Befehl ist unter der Lease nicht erlaubt ({art})."
            )));
        }
        let id_ok = roh
            .get("id")
            .and_then(|i| i.as_str())
            .is_some_and(schreiben::id_gueltig);
        if !id_ok {
            return Err(SchreibFehler::Ungueltig(
                "Befehl ohne gültige Kennung (A–Z, a–z, 0–9, -; höchstens 64 Zeichen).".into(),
            ));
        }
        if schreiben::ARTEN_MIT_JETZT.contains(&art.as_str()) {
            if let Some(o) = roh.as_object_mut() {
                o.insert(
                    "jetzt".into(),
                    serde_json::Value::String(schreiben::wandzeit(jetzt_ms())),
                );
            }
        }
        let befehl: TurnierBefehl = serde_json::from_value(roh)
            .map_err(|e| SchreibFehler::Ungueltig(format!("Befehl unlesbar ({e}).")))?;
        self.ausfuehren(vec![befehl], AKTEUR_BADHUB)
    }

    /// Posten, die badhub noch nicht bestätigt hat, in der Form des Uploads
    /// (`[{befehl, ereignisse}]`), mit dem bestätigten `n` davor — für die
    /// Antwort auf einen Kernbefehl aus badhub. `None`, wenn die Outbox
    /// angehalten ist oder die Posten zu groß für eine Antwort wären; badhub
    /// wartet dann auf den Abgleich.
    pub fn wartende_fuer_badhub(&self) -> Option<(u64, serde_json::Value)> {
        if self.abweichung.is_some() || self.zugang_abgelehnt {
            return None;
        }
        let basis_n = self.n_bestaetigt();
        let posten: Vec<serde_json::Value> = self
            .protokoll
            .offen_nach(basis_n)
            .map(Posten::fuer_badhub)
            .collect();
        let wert = serde_json::Value::Array(posten);
        let laenge = serde_json::to_vec(&wert).map_or(usize::MAX, |v| v.len());
        (laenge <= schreiben::ANTWORT_POSTEN_MAX_BYTES).then_some((basis_n, wert))
    }

    /// Rückzug einer Meldung aus der Halle (TL-Web, Kern 0.13.0): `Some` =
    /// zurückziehen mit der Wahl für die Gruppe, `None` = Rückzug
    /// zurücknehmen. Die Konkurrenz kommt aus dem Kern-Stand (die
    /// Meldungsnummer ist turnierweit eindeutig).
    pub fn rueckzug(
        &mut self,
        meldung: u32,
        ergebnisse: Option<turnierkern::Rueckzug>,
    ) -> Result<usize, SchreibFehler> {
        self.schreibrecht()?;
        let stand = self.kern_stand();
        let konkurrenz = stand
            .meldung(meldung)
            .map(|m| m.konkurrenz.clone())
            .ok_or_else(|| {
                SchreibFehler::Ungueltig(format!(
                    "Meldung {meldung} gibt es in diesem Turnier nicht."
                ))
            })?;
        let befehl = match ergebnisse {
            Some(ergebnisse) => TurnierBefehl::Rueckzug {
                id: String::new(),
                konkurrenz,
                meldung,
                ergebnisse,
                jetzt: schreiben::wandzeit(jetzt_ms()),
            },
            None => TurnierBefehl::RueckzugZuruecknehmen {
                id: String::new(),
                konkurrenz,
                meldung,
            },
        };
        self.ausfuehren(vec![befehl], AKTEUR_HALLE)
    }

    /// Meldungen, die das TL-Web zurückziehen kann: ausgelost in einer
    /// Konkurrenz mit freigegebener Auslosung, samt Rückzugs-Stand. Namen aus
    /// dem Personenauszug — dieselben, die ohnehin in der Warteliste stehen.
    pub fn rueckzug_meldungen(&mut self) -> Vec<RueckzugMeldung> {
        let stand = self.kern_stand();
        let namen: std::collections::BTreeMap<u32, String> = self
            .paket
            .meldungen
            .iter()
            .map(|m| {
                let n: Vec<String> = m
                    .personen
                    .iter()
                    .map(|p| {
                        format!("{} {}", p.vorname.trim(), p.nachname.trim())
                            .trim()
                            .to_string()
                    })
                    .collect();
                (m.meldung, n.join(" / "))
            })
            .collect();
        let mut liste = Vec::new();
        for k in &self.paket.konkurrenzen {
            let Some(d) = stand
                .durchfuehrung
                .get(&k.key)
                .filter(|d| !d.spiele.is_empty())
            else {
                continue;
            };
            let Some(a) = stand.auslosungen.get(&k.key) else {
                continue;
            };
            let mut gesehen = std::collections::BTreeSet::new();
            for l in &a.auslosung.plaetze {
                if !gesehen.insert(l.meldung) {
                    continue;
                }
                let r = d.rueckzuege.get(&l.meldung);
                liste.push(RueckzugMeldung {
                    entry_id: i64::from(l.meldung),
                    konkurrenz: if k.name.trim().is_empty() {
                        k.key.clone()
                    } else {
                        k.name.clone()
                    },
                    label: namen
                        .get(&l.meldung)
                        .cloned()
                        .unwrap_or_else(|| format!("Meldung {}", l.meldung)),
                    zurueckgezogen: r.is_some(),
                    ergebnisse: r.map(|r| match r.ergebnisse {
                        turnierkern::Rueckzug::Behalten => "behalten".to_string(),
                        turnierkern::Rueckzug::Streichen => "streichen".to_string(),
                    }),
                });
            }
        }
        liste
    }

    // ── Lease ───────────────────────────────────────────────────────────

    /// Braucht es jetzt ein `POST turnier/lease`? Liefert (`n`, Relay).
    /// Ohne gespeicherte Lease immer; mit aktiver Lease einmal je Lauf und
    /// nur bei leerer Outbox (sonst stimmte `n` nicht mit badhub überein —
    /// die Outbox läuft dann mit der gespeicherten Epoche weiter).
    fn lease_bedarf(&mut self, relay_ns: Option<&str>) -> Option<(u64, Option<api::RelayAngaben>)> {
        if self.token.is_none() || self.zugang_abgelehnt {
            return None;
        }
        // Nach einer Standby-Übernahme meldet erst der Upload den neuen
        // Inhaber; ein `POST turnier/lease` vorher bekäme `lease_belegt`.
        if self.standby_offen() || self.lease.as_ref().is_some_and(|l| l.neu_laden) {
            return None;
        }
        match &self.lease {
            Some(l) if !l.aktiv() => return None,
            Some(l) if self.lease_angemeldet && l.relay_ns.as_deref() == relay_ns => return None,
            _ => {}
        }
        if self.wartende_posten() > 0 {
            return None;
        }
        let relay = match relay_ns {
            Some(ns) => {
                let token = match &self.tl_zugang {
                    Some(t) => t.clone(),
                    None => lease::neuer_zugang().ok()?,
                };
                Some(api::RelayAngaben {
                    ns: ns.to_string(),
                    tl_token: token,
                })
            }
            None => None,
        };
        Some((self.n_lokal(), relay))
    }

    /// Antwort auf `POST turnier/lease` übernehmen.
    fn lease_antwort(
        &mut self,
        antwort: Result<api::LeaseAntwort, api::ApiFehler>,
        relay: Option<api::RelayAngaben>,
    ) {
        let jetzt = jetzt_ms();
        match antwort {
            Ok(api::LeaseAntwort::Erteilt { epoche, n }) => {
                let seit = self
                    .lease
                    .as_ref()
                    .filter(|l| l.epoche == epoche && l.seit_ms > 0)
                    .map_or(jetzt, |l| l.seit_ms);
                if let Some(alt) = self.lease.as_ref().filter(|l| l.epoche != epoche) {
                    tracing::info!(
                        "Quelle badhub: Lease neu vergeben (Epoche {} → {epoche})",
                        alt.epoche
                    );
                }
                let l = Lease {
                    epoche,
                    bis_n: n,
                    zustand: LeaseZustand::Aktiv,
                    relay_ns: relay.as_ref().map(|r| r.ns.clone()),
                    seit_ms: seit,
                    letzter_kontakt_ms: jetzt,
                    // Hinweis „k abgewiesen“ bleibt bis zu einer neuen Epoche.
                    abgewiesen: self
                        .lease
                        .as_ref()
                        .filter(|l| l.epoche == epoche)
                        .map_or(0, |l| l.abgewiesen),
                    ..Default::default()
                };
                if let Err(e) = lease::speichern(self.ablage.dir(), &l) {
                    tracing::warn!("Quelle badhub: Lease nicht gesichert ({e})");
                }
                match &relay {
                    Some(r) => {
                        if let Err(e) = self
                            .ablage
                            .geheimnis_speichern(lease::ZUGANG_DATEI, &r.tl_token)
                        {
                            tracing::warn!("Quelle badhub: Relay-Zugang nicht gesichert ({e})");
                        }
                        self.tl_zugang = Some(r.tl_token.clone());
                    }
                    None => {
                        let _ = std::fs::remove_file(self.ablage.dir().join(lease::ZUGANG_DATEI));
                        self.tl_zugang = None;
                    }
                }
                tracing::info!(
                    "Quelle badhub: Lease Epoche {epoche}, n={n}{}",
                    if relay.is_some() { ", mit Relay" } else { "" }
                );
                self.lease = Some(l);
                self.lease_angemeldet = true;
                self.lease_hinweis = None;
            }
            Ok(api::LeaseAntwort::NVeraltet { n }) => {
                self.lease_hinweis = Some(format!(
                    "badhub ist bei Ereignis {n}, dieser PC bei {} — Stand wird nachgeholt",
                    self.n_lokal()
                ));
            }
            Ok(api::LeaseAntwort::Belegt { geraet }) => {
                self.lease_hinweis = Some(format!(
                    "ein anderes Gerät steuert das Turnier ({})",
                    geraet.chars().take(60).collect::<String>()
                ));
            }
            Ok(api::LeaseAntwort::Abgelehnt(code)) => {
                self.lease_hinweis = Some(lease_ablehnung_text(&code));
            }
            Err(api::ApiFehler::Zugang) => {
                // 401/403 ist kein Beweis für eine Rückholung (das wäre
                // `epoche_veraltet`, 409) — auch eine Firewall antwortet so.
                // Abgleich anhalten und es sagen; die Lease bleibt.
                self.zugang_abgelehnt = true;
                self.lease_hinweis = Some("badhub lehnt den Zugang dieses PCs ab".into());
            }
            Err(e) => {
                // Offline: eine gespeicherte Lease gilt weiter.
                if self.lease.is_none() {
                    self.lease_hinweis = Some(format!("{e}"));
                }
            }
        }
    }

    /// badhub hat die Steuerung zurückgeholt: Outbox steht, Eingaben gesperrt.
    fn entziehen(&mut self) {
        if let Some(l) = self.lease.as_mut() {
            l.zustand = LeaseZustand::Entzogen;
            let l = l.clone();
            if let Err(e) = lease::speichern(self.ablage.dir(), &l) {
                tracing::warn!("Quelle badhub: Lease nicht gesichert ({e})");
            }
        }
        self.tl_zugang = None;
        let _ = std::fs::remove_file(self.ablage.dir().join(lease::ZUGANG_DATEI));
        tracing::warn!("Quelle badhub: Steuerung in badhub zurückgeholt — Outbox angehalten");
    }

    // ── Outbox ──────────────────────────────────────────────────────────

    /// Steht der erste Upload nach einer Standby-Übernahme noch aus?
    pub fn standby_offen(&self) -> bool {
        self.lease.as_ref().is_some_and(|l| l.aktiv() && l.standby)
    }

    /// Nächster Hochlade-Auftrag.
    fn outbox_auftrag(&self) -> Option<OutboxAuftrag> {
        let l = self.lease.as_ref().filter(|l| l.aktiv())?;
        if self.abweichung.is_some()
            || self.zugang_abgelehnt
            || self.kein_standby
            || self.standby_pruefen
            || l.neu_laden
        {
            return None;
        }
        let token = self.token.clone()?;
        let basis_n = self.n_bestaetigt();
        let mut posten = Vec::new();
        let mut ereignisse = 0usize;
        for p in self.protokoll.offen_nach(basis_n) {
            if posten.len() >= MAX_POSTEN
                || (!posten.is_empty() && ereignisse + p.ereignisse.len() > MAX_EREIGNISSE)
            {
                break;
            }
            ereignisse += p.ereignisse.len();
            posten.push(p.fuer_badhub());
        }
        if posten.is_empty() {
            return None;
        }
        let bis_lokal = self
            .protokoll
            .offen_nach(basis_n)
            .take(posten.len())
            .last()
            .map_or(basis_n, Posten::n);
        Some(OutboxAuftrag {
            basis: self.basis.clone(),
            token,
            epoche: l.epoche,
            basis_n,
            posten,
            standby: l.standby,
            bis_lokal,
        })
    }

    /// Bestätigtes `n` setzen: eigene Ereignisse bis dorthin ins Paket
    /// übernehmen (sie gelten jetzt als badhub-Stand) und sichern.
    fn bestaetigen(&mut self, bis_n: u64) {
        let jetzt = jetzt_ms();
        let basis = self.paket.n();
        let neu: Vec<TurnierEreignis> = self
            .protokoll
            .posten()
            .iter()
            .flat_map(|p| p.ereignisse.iter())
            .filter(|e| e.n() > basis && e.n() <= bis_n)
            .cloned()
            .collect();
        if !neu.is_empty() {
            self.paket.ereignisse_anhaengen(neu);
            if let Err(e) = self
                .ablage
                .ereignisse_speichern(&self.paket, self.etag.as_deref())
            {
                tracing::warn!("badhub-Turnier: Ereignisse nicht gesichert ({e})");
            }
        }
        if let Some(l) = self.lease.as_mut() {
            l.bis_n = bis_n;
            l.letzter_kontakt_ms = jetzt;
            let l = l.clone();
            if let Err(e) = lease::speichern(self.ablage.dir(), &l) {
                tracing::warn!("Quelle badhub: Lease nicht gesichert ({e})");
            }
        }
    }

    /// Antwort auf ein Hochladen übernehmen.
    fn outbox_antwort(
        &mut self,
        r: Result<api::HochladenAntwort, api::ApiFehler>,
        bis_lokal: u64,
    ) -> OutboxSchritt {
        let war_standby = self.standby_offen();
        if !matches!(r, Ok(api::HochladenAntwort::InhaberAktiv { .. })) {
            self.inhaber_aktiv_s = None;
        }
        match r {
            Ok(api::HochladenAntwort::StandbyUebernommen {
                bis_n,
                epoche,
                abgewiesen,
            }) => self.standby_uebernommen(bis_n, epoche, abgewiesen, bis_lokal),
            Ok(api::HochladenAntwort::KeinStandby) => {
                tracing::warn!(
                    "Quelle badhub: badhub kennt diesen PC nicht als Standby — Outbox angehalten"
                );
                self.kein_standby = true;
                OutboxSchritt::Angehalten
            }
            Ok(api::HochladenAntwort::InhaberAktiv { vor_s }) => {
                // Hauptrechner war eben noch bei badhub: nichts ist passiert,
                // die Posten bleiben — später erneut (Backoff).
                self.inhaber_aktiv_s = Some(vor_s);
                OutboxSchritt::Spaeter
            }
            Ok(api::HochladenAntwort::EpocheVeraltet) if war_standby => {
                // Vielleicht galt die Übernahme schon und nur die Antwort ging
                // verloren: erst badhub fragen (`GET turnier/lease`).
                tracing::info!(
                    "Quelle badhub: Standby-Upload mit alter Epoche — frage badhub nach der Rolle"
                );
                self.standby_pruefen = true;
                OutboxSchritt::Angehalten
            }
            Ok(api::HochladenAntwort::Bestaetigt { bis_n }) => {
                self.bestaetigen(bis_n.min(self.n_lokal()));
                OutboxSchritt::Weiter
            }
            Ok(api::HochladenAntwort::Luecke { erwartet_n })
                if erwartet_n == self.n_bestaetigt() =>
            {
                // Kein Fortschritt möglich: nicht in einer Schleife
                // wiederholen, sondern mit Backoff.
                OutboxSchritt::Spaeter
            }
            Ok(api::HochladenAntwort::Luecke { erwartet_n }) => {
                if erwartet_n > self.n_lokal() || erwartet_n < self.paket.n() {
                    // badhub kennt Ereignisse, die dieser PC nicht hat — nicht
                    // still zusammenführen.
                    self.abweichung = Some(format!(
                        "badhub erwartet Ereignis {erwartet_n}, dieser PC ist bei {}",
                        self.n_lokal()
                    ));
                    tracing::warn!("Quelle badhub: Lücke ohne Anschluss — Outbox angehalten");
                    return OutboxSchritt::Angehalten;
                }
                tracing::info!("Quelle badhub: Lücke — weiter ab n={erwartet_n}");
                self.bestaetigen(erwartet_n);
                OutboxSchritt::Weiter
            }
            Ok(api::HochladenAntwort::EpocheVeraltet) => {
                self.entziehen();
                OutboxSchritt::Angehalten
            }
            Err(api::ApiFehler::Zugang) => {
                // Widerruf oder vorgeschaltete Sperre: Outbox anhalten (bis
                // zum Neustart), Eingaben bleiben möglich, nichts verworfen.
                tracing::warn!("Quelle badhub: Zugang abgelehnt (401/403) — Outbox angehalten");
                self.zugang_abgelehnt = true;
                OutboxSchritt::Angehalten
            }
            Ok(api::HochladenAntwort::Abweichung { grund }) => {
                let grund: String = grund
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .take(40)
                    .collect();
                tracing::warn!(
                    "Quelle badhub: badhub weist die Posten ab ({grund}) — Outbox angehalten"
                );
                self.abweichung = Some(format!("badhub weist die Ereignisse ab ({grund})"));
                OutboxSchritt::Angehalten
            }
            Err(e) => {
                tracing::debug!("Quelle badhub: Hochladen gescheitert ({e})");
                OutboxSchritt::Spaeter
            }
        }
    }

    /// Lage für die Anzeige.
    pub fn abgleich(&self) -> Abgleich {
        let wartend = self.wartende_ereignisse();
        let kontakt = self.lease.as_ref().map_or(0, |l| l.letzter_kontakt_ms);
        let kontakt_text = if kontakt > 0 {
            format!(", letzter Kontakt {}", uhrzeit(kontakt))
        } else {
            String::new()
        };
        let (zustand, text, rot) = match &self.lease {
            Some(_) if self.kein_standby => (
                "angehalten",
                format!(
                    "Abgleich mit badhub angehalten: badhub kennt diesen PC nicht als Standby der \
                     laufenden Lease (in badhub im Bereich Turniertag festlegen, dann Übertragung \
                     neu starten). {wartend} Ereignisse warten."
                ),
                true,
            ),
            Some(l) if l.aktiv() && l.neu_laden => (
                "wartet",
                "Turnier übernommen — der Stand wird von badhub neu geladen.".to_string(),
                false,
            ),
            Some(l) if l.aktiv() && l.standby => (
                "wartet",
                format!(
                    "Turnier als Standby übernommen — {wartend} Ereignisse warten auf den ersten \
                     Abgleich mit badhub{kontakt_text}"
                ),
                false,
            ),
            Some(l) if l.zustand == LeaseZustand::Entzogen => {
                ("entzogen", TEXT_ENTZOGEN.to_string(), true)
            }
            Some(l) if l.zustand == LeaseZustand::Zurueckgegeben => (
                "zurueckgegeben",
                "Steuerung an badhub zurückgegeben.".to_string(),
                false,
            ),
            Some(_) if self.zugang_abgelehnt => (
                "angehalten",
                format!(
                    "Abgleich mit badhub angehalten: badhub lehnt den Zugang dieses PCs ab \
                     (Kopplung widerrufen?). {wartend} Ereignisse warten — Übertragung neu \
                     starten oder in badhub prüfen."
                ),
                true,
            ),
            Some(_) if self.abweichung.is_some() => (
                "angehalten",
                format!(
                    "Abgleich mit badhub angehalten: {}. {wartend} Ereignisse warten.",
                    self.abweichung.as_deref().unwrap_or("")
                ),
                true,
            ),
            Some(_) if wartend == 0 => ("aktuell", "badhub: aktuell".to_string(), false),
            Some(_) => (
                "wartet",
                format!(
                    "{wartend} {} warten{kontakt_text}",
                    if wartend == 1 {
                        "Ereignis"
                    } else {
                        "Ereignisse"
                    }
                ),
                false,
            ),
            None => (
                "keine_lease",
                format!(
                    "badhub: noch keine Steuerung{}",
                    self.lease_hinweis
                        .as_deref()
                        .map(|h| format!(" — {h}"))
                        .unwrap_or_default()
                ),
                false,
            ),
        };
        let aktiv = self.lease.as_ref().is_some_and(Lease::aktiv);
        Abgleich {
            zustand: zustand.into(),
            wartend,
            letzter_kontakt_ms: kontakt,
            text,
            kann_beenden: aktiv
                && wartend == 0
                && !self.standby_offen()
                && !self.lease.as_ref().is_some_and(|l| l.neu_laden)
                && self.abweichung.is_none()
                && !self.zugang_abgelehnt,
            kann_uebernehmen: self.lease.as_ref().is_some_and(|l| !l.aktiv()),
            rot,
            warnung: self.abgleich_warnung(),
        }
    }

    /// Hinweise der Standby-Übernahme für Hauptfenster und TL-Web.
    fn abgleich_warnung(&self) -> Option<String> {
        if let Some(s) = self.inhaber_aktiv_s.filter(|_| self.standby_offen()) {
            return Some(format!(
                "Hauptrechner war vor {s} s noch online – badhub nimmt die Übernahme erst an, wenn \
                 er 2 Minuten weg ist. Eingaben hier gelten weiter."
            ));
        }
        let k = self.lease.as_ref().map_or(0, |l| l.abgewiesen);
        (k > 0).then(|| {
            format!(
                "{k} {} beim Übernehmen von badhub abgewiesen — sie stehen in badhub im Bereich \
                 Turniertag als Aufgabe.",
                if k == 1 {
                    "Eingabe wurde"
                } else {
                    "Eingaben wurden"
                }
            )
        })
    }

    /// Personenauszug löschen (Rückgabe, Frist).
    fn personen_loeschen(&mut self) {
        self.ablage.personen_loeschen();
        self.paket.meldungen.clear();
        self.paket.schiedsrichter.clear();
        self.abbild = None;
    }

    /// Übernimmt neue Ereignisse und legt sie ab. Liefert die Zahl neuer.
    pub fn ereignisse_uebernehmen(&mut self, neu: Vec<turnierkern::TurnierEreignis>) -> usize {
        let zahl = self.paket.ereignisse_anhaengen(neu);
        if zahl > 0 {
            if let Err(e) = self
                .ablage
                .ereignisse_speichern(&self.paket, self.etag.as_deref())
            {
                tracing::warn!("badhub-Turnier: Ereignisse nicht gesichert ({e})");
            }
        }
        zahl
    }

    /// Übernimmt ein neues Paket: Stammdaten ersetzen, Ereignisse vereinigen.
    pub fn paket_uebernehmen(&mut self, neu: Paket, etag: Option<String>) -> Result<(), String> {
        neu.pruefen()?;
        if !neu
            .turnier
            .uuid
            .eq_ignore_ascii_case(&self.paket.turnier.uuid)
        {
            return Err("badhub liefert ein anderes Turnier als gekoppelt.".into());
        }
        let alte = std::mem::take(&mut self.paket.ereignisse);
        let mut paket = neu;
        paket.ereignisse_anhaengen(alte);
        self.ablage.paket_speichern(&paket, etag.as_deref())?;
        self.paket = paket;
        self.etag = etag;
        Ok(())
    }

    /// Zugangsdaten für einen Abgleich, `None` ohne Token oder nach 401.
    pub(crate) fn zugang(&self) -> Option<(String, String, u64, Option<String>)> {
        if self.zugang_abgelehnt {
            return None;
        }
        let token = self.token.clone()?;
        Some((self.basis.clone(), token, self.paket.n(), self.etag.clone()))
    }
}

/// Der Zustand des laufenden Turniers (`None` = keine Kern-Quelle aktiv).
static ZUSTAND: Mutex<Option<KernQuelle>> = Mutex::new(None);

fn zustand() -> std::sync::MutexGuard<'static, Option<KernQuelle>> {
    ZUSTAND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Lädt beim Start der Übertragung das gekoppelte Turnier. Liefert, ob der
/// Abgleich mit badhub laufen soll.
pub fn starten(config: &AppConfig, daten: &Path) -> bool {
    let mut z = zustand();
    *z = None;
    if !config.badhub_turnier.gekoppelt() {
        tracing::warn!("Quelle badhub: kein Turnier gekoppelt");
        return false;
    }
    aufraeumen(daten, Some(&config.badhub_turnier.uuid), heute());
    match KernQuelle::laden(daten, &config.badhub_turnier) {
        Ok(q) => {
            tracing::info!(
                "Quelle badhub: Turnier {} geladen ({} Ereignisse, {} eigene warten, Kern {})",
                config.badhub_turnier.id,
                q.paket.ereignisse.len(),
                q.wartende_ereignisse(),
                KERN_VERSION
            );
            let mut q = q;
            if !config.slave_mode && q.slave_schluessel.is_none() {
                match slave::neuer_schluessel().and_then(|k| {
                    q.ablage
                        .geheimnis_speichern(slave::SCHLUESSEL_DATEI, &k)
                        .map(|_| k)
                }) {
                    Ok(k) => q.slave_schluessel = Some(k),
                    Err(e) => tracing::warn!("Quelle badhub: Slave-Schlüssel nicht angelegt ({e})"),
                }
            }
            let mit_token = q.token.is_some();
            *z = Some(q);
            mit_token
        }
        Err(e) => {
            tracing::warn!("Quelle badhub: {e}");
            false
        }
    }
}

/// Liest den Stand des gekoppelten Turniers (Master).
pub fn stand(config: &AppConfig) -> Result<BtpSnapshot, String> {
    let _ = config;
    match zustand().as_mut() {
        Some(q) => Ok(q.stand()),
        None => Err(
            "Kein badhub-Turnier geladen — im Setup unter „Quelle“ einen Kopplungscode eingeben."
                .into(),
        ),
    }
}

/// Der Schlüssel der lokalen Stores (UUID statt Name), `None` ohne Turnier.
pub fn turnier_schluessel() -> Option<String> {
    zustand().as_ref().map(KernQuelle::turnier_schluessel)
}

/// Darf gerade geschrieben werden? (Lease aktiv, Kern-Version passt.)
pub fn schreiben_moeglich() -> bool {
    zustand().as_ref().is_some_and(|q| q.schreibrecht().is_ok())
}

/// Führt Entwürfe aus dem jeweiligen Stand aus (Schreibweichen).
fn mit_quelle<T>(
    f: impl FnOnce(&mut KernQuelle) -> Result<T, SchreibFehler>,
) -> Result<T, SchreibFehler> {
    match zustand().as_mut() {
        Some(q) => f(q),
        None => Err(SchreibFehler::Gesperrt(
            "Kein badhub-Turnier geladen — Eingabe nicht möglich.".into(),
        )),
    }
}

/// Weiche Feldvergabe (`write_courts_to_btp`).
pub fn feld_vergabe(courts: &[crate::btp::proto::CourtAssignment]) -> Result<(), String> {
    mit_quelle(|q| {
        q.schreibrecht()?;
        let stand = q.kern_stand();
        let jetzt = schreiben::wandzeit(jetzt_ms());
        let befehle = schreiben::aufrufe(&stand, &q.paket, courts, &jetzt)
            .map_err(SchreibFehler::Ungueltig)?;
        q.ausfuehren(befehle, AKTEUR_HALLE)
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Weiche Ergebnis (`write_result_to_btp`, `write_result_settled`).
pub fn ergebnis(update: &crate::btp::proto::MatchUpdate) -> Result<(), String> {
    mit_quelle(|q| {
        q.schreibrecht()?;
        let stand = q.kern_stand();
        let jetzt = schreiben::wandzeit(jetzt_ms());
        let befehl = schreiben::ergebnis(&stand, &q.paket, update, &jetzt)
            .map_err(SchreibFehler::Ungueltig)?;
        q.ausfuehren(befehl.into_iter().collect(), AKTEUR_HALLE)
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Weiche Schiedsrichter (`write_officials_to_btp`): je Spiel ein Befehl.
/// Geliefert werden die Einträge, die erledigt sind — auch eine Ablehnung
/// des Kerns zählt dazu (sie wird gemeldet, aber nicht jeden Takt neu
/// versucht; der Kern-Stand zeigt dann weiter die alte Besetzung).
pub fn offizielle(
    entries: &[crate::btp::proto::MatchCourt],
) -> Result<Vec<crate::btp::proto::MatchCourt>, String> {
    mit_quelle(|q| {
        q.schreibrecht()?;
        let stand = q.kern_stand();
        let mut erledigt = Vec::new();
        for (e, r) in schreiben::offizielle(&stand, &q.paket, entries) {
            match r {
                Ok(None) => erledigt.push(e),
                Ok(Some(b)) => match q.ausfuehren(vec![b], AKTEUR_HALLE) {
                    Ok(_) => erledigt.push(e),
                    Err(SchreibFehler::Kern { text, .. }) => {
                        tracing::info!("Schiedsrichter für Spiel {}: {text}", e.match_id);
                        erledigt.push(e);
                    }
                    Err(f) => return Err(f),
                },
                Err(t) => tracing::info!("Schiedsrichter für Spiel {}: {t}", e.match_id),
            }
        }
        Ok(erledigt)
    })
    .map_err(|e| e.to_string())
}

/// Ein Kernbefehl aus badhub (über das Relay). Dazu die Posten, die badhub
/// noch nicht bestätigt hat ([`KernQuelle::wartende_fuer_badhub`]).
pub fn befehl_von_badhub(
    befehl: serde_json::Value,
) -> Result<(usize, Option<(u64, serde_json::Value)>), SchreibFehler> {
    mit_quelle(|q| {
        q.befehl_von_badhub(befehl)
            .map(|n| (n, q.wartende_fuer_badhub()))
    })
}

/// Rückzug bzw. Rücknahme aus dem TL-Web (Kern 0.13.0).
pub fn rueckzug(
    meldung: u32,
    ergebnisse: Option<turnierkern::Rueckzug>,
) -> Result<usize, SchreibFehler> {
    mit_quelle(|q| q.rueckzug(meldung, ergebnisse))
}

/// Meldungen für den Rückzug im TL-Web — nur, solange dieser PC schreiben
/// darf (`None` sonst: ohne Lease gibt es nichts zurückzuziehen).
pub fn rueckzug_liste() -> Option<Vec<RueckzugMeldung>> {
    let mut g = zustand();
    let q = g.as_mut()?;
    q.schreibrecht().ok()?;
    Some(q.rueckzug_meldungen())
}

/// TL-Zugang für badhub, solange die Lease aktiv ist.
pub fn tl_zugang() -> Option<String> {
    zustand()
        .as_ref()
        .filter(|q| q.lease.as_ref().is_some_and(Lease::aktiv))
        .and_then(|q| q.tl_zugang.clone())
}

// ── Standby (Paket S2) ──────────────────────────────────────────────────

/// Lage des Standby (`None` ohne geladenes badhub-Turnier).
pub fn standby_lage() -> Option<standby::StandbyLage> {
    zustand().as_ref().map(KernQuelle::standby_lage)
}

/// Notfall-Sicherung einlesen.
pub fn sicherung_einlesen(pfad: &Path) -> Result<String, String> {
    let s = standby::sicherung_lesen(pfad)?;
    match zustand().as_mut() {
        Some(q) => q.sicherung_einlesen(s),
        None => Err("Kein badhub-Turnier geladen — bitte erst koppeln.".into()),
    }
}

/// Hält dieser PC eine aktive Lease?
pub fn haelt_lease() -> bool {
    zustand()
        .as_ref()
        .is_some_and(|q| q.lease.as_ref().is_some_and(Lease::aktiv))
}

/// „Hauptrechner ausgefallen – Turnier übernehmen“ am geladenen Zustand.
/// Die Umstellung auf den Master-Betrieb (kein Slave mehr) macht der Aufrufer.
pub fn standby_uebernehmen() -> Result<String, String> {
    match zustand().as_mut() {
        Some(q) => q.als_standby_uebernehmen(),
        None => Err("Kein badhub-Turnier geladen.".into()),
    }
}

/// Schreibt die Notfall-Sicherung, wenn sich seit der letzten etwas
/// geändert hat und der Mindestabstand um ist (`zwang`: sofort, beim
/// Anhalten). Liefert, ob geschrieben wurde.
pub fn notfall_sichern(ordner: &Path, zwang: bool) -> bool {
    notfall_sichern_in(&ZUSTAND, ordner, zwang)
}

pub(crate) fn notfall_sichern_in(
    z: &Mutex<Option<KernQuelle>>,
    ordner: &Path,
    zwang: bool,
) -> bool {
    let jetzt = jetzt_ms();
    let sicherung = {
        let g = z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(q) = g.as_ref() else {
            return false;
        };
        if q.notfall_n == Some(q.protokoll.n()) {
            return false;
        }
        if !zwang && jetzt.saturating_sub(q.notfall_ms) < standby::SICHERUNG_TAKT_MS {
            return false;
        }
        match q.notfall_sicherung() {
            Some(s) => s,
            None => return false,
        }
    };
    let n = sicherung.n;
    match standby::sicherung_schreiben(ordner, &sicherung) {
        Ok(_) => {
            if let Some(q) = z
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                q.notfall_n = Some(n);
                q.notfall_ms = jetzt;
            }
            tracing::debug!("Quelle badhub: Notfall-Sicherung bis n={n} geschrieben");
            true
        }
        Err(e) => {
            // Nicht jeden Takt: erst nach dem Mindestabstand erneut.
            if let Some(q) = z
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                q.notfall_ms = jetzt;
            }
            tracing::warn!("Quelle badhub: Notfall-Sicherung nicht geschrieben ({e})");
            false
        }
    }
}

/// Schreibt die Notfall-Sicherung höchstens alle 30 s nach einer Änderung,
/// solange die Übertragung läuft (eigener Task am Hauptrechner).
pub async fn notfall_schleife(ordner: std::path::PathBuf) {
    let mut takt = tokio::time::interval(Duration::from_secs(5));
    takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        takt.tick().await;
        let o = ordner.clone();
        let _ = tokio::task::spawn_blocking(move || notfall_sichern(&o, false)).await;
    }
}

/// Slave-Schlüssel des Masters in Anzeigeform (für das Dashboard).
pub fn slave_schluessel_anzeige() -> Option<String> {
    zustand()
        .as_ref()
        .and_then(|q| q.slave_schluessel.as_deref().map(slave::schluessel_anzeige))
}

/// Slave-Schlüssel am Slave eintragen (vom Turnier-PC abgelesen). Legt ihn
/// im Turnierordner ab und übernimmt ihn in einen geladenen Zustand.
pub fn slave_schluessel_setzen(daten: &Path, uuid: &str, eingabe: &str) -> Result<(), String> {
    let k = slave::schluessel_normal(eingabe).ok_or(
        "Der Slave-Schlüssel hat 32 Zeichen (0–9, a–f), Bindestriche sind egal.".to_string(),
    )?;
    Ablage::neu(daten, uuid)?.geheimnis_speichern(slave::SCHLUESSEL_DATEI, &k)?;
    if let Some(q) = zustand()
        .as_mut()
        .filter(|q| q.paket.turnier.uuid.eq_ignore_ascii_case(uuid))
    {
        q.slave_schluessel = Some(k);
    }
    Ok(())
}

/// Lage des Abgleichs (`None` ohne geladenes badhub-Turnier).
pub fn abgleich_lage() -> Option<Abgleich> {
    zustand().as_ref().map(KernQuelle::abgleich)
}

/// Höchstzahl Seiten, die ein Nachladen am Stück holt (je [`api::SEITE`]
/// Ereignisse) — Schutz gegen ein badhub, das nie „fertig“ meldet.
const MAX_SEITEN: usize = 400;

/// Holt Ereignisse seitenweise ab `ab` (`GET turnier/ereignisse?ab=&max=`),
/// bis der Kopf `n` erreicht ist (badhub kappt das Paket auf 2000
/// Ereignisse). `ziel` = bekannter Kopf (aus dem Paket), der Kopf jeder
/// Antwort geht vor. `uebernehmen` legt eine Seite ab und liefert das neue
/// lokale `n` — jede Seite ist sofort gesichert, ein Abbruch mittendrin
/// verliert nichts (Ereignisse kommen lückenlos ab `n`).
///
/// Liefert, ob der Stand vollständig ist. Ohne Kopf (badhub vor dem Paket
/// in Teilen) gilt eine nicht volle Seite als Ende.
async fn seiten_holen(
    basis: &str,
    token: &str,
    mut ab: u64,
    ziel: Option<u64>,
    mut uebernehmen: impl FnMut(Vec<TurnierEreignis>) -> u64,
) -> Result<bool, api::ApiFehler> {
    let mut ziel = ziel;
    for _ in 0..MAX_SEITEN {
        if ziel.is_some_and(|z| ab >= z) {
            return Ok(true);
        }
        let seite = api::ereignisse(basis, token, ab).await?;
        if seite.kopf.is_some() {
            ziel = seite.kopf;
        }
        let voll = seite.ereignisse.len() >= api::SEITE;
        if seite.ereignisse.is_empty() {
            return Ok(ziel.is_none_or(|z| ab >= z));
        }
        let neu_n = uebernehmen(seite.ereignisse);
        if neu_n <= ab {
            // Kein Fortschritt (nur schon bekannte Nummern): nicht weiter
            // fragen, der nächste Takt versucht es neu.
            return Ok(ziel.is_none_or(|z| neu_n >= z));
        }
        ab = neu_n;
        match ziel {
            Some(z) if ab >= z => return Ok(true),
            Some(_) => {}
            None if !voll => return Ok(true),
            None => {}
        }
    }
    Ok(false)
}

/// Lädt ein gekapptes Paket bis `bis` nach. Ein Abbruch (offline) lässt das
/// Paket teilweise stehen — der Abgleich holt den Rest vor der Lease nach.
/// Liefert einen Hinweis, wenn es unvollständig blieb.
async fn paket_nachladen(
    basis: &str,
    token: &str,
    paket: &mut Paket,
    bis: Option<u64>,
) -> Result<Option<String>, api::ApiFehler> {
    let Some(bis) = bis else {
        return Ok(None);
    };
    let ab = paket.n();
    let r = seiten_holen(basis, token, ab, Some(bis), |neu| {
        paket.ereignisse_anhaengen(neu);
        paket.n()
    })
    .await;
    let hinweis = || {
        format!(
            "Turnierstand erst bis Ereignis {} von {bis} geladen — der Rest kommt mit der              Übertragung nach, bevor bts-light die Steuerung übernimmt.",
            paket.n()
        )
    };
    match r {
        Ok(true) => Ok(None),
        Ok(false) => Ok(Some(hinweis())),
        Err(api::ApiFehler::Zugang) => Err(api::ApiFehler::Zugang),
        Err(e) => {
            tracing::info!("Quelle badhub: Paket nur teilweise nachgeladen ({e})");
            Ok(Some(hinweis()))
        }
    }
}

/// Ein Schritt des Abgleichs gegen den globalen Zustand.
async fn abgleich_schritt(paket_faellig: bool, relay_ns: Option<&str>) -> bool {
    abgleich_schritt_in(&ZUSTAND, paket_faellig, relay_ns).await
}

/// Ein Schritt des Abgleichs; reine Netzarbeit außerhalb der Sperre.
///
/// Ohne aktive, angemeldete Lease holt er neue Ereignisse — seitenweise,
/// bis der Kopf von badhub erreicht ist — und nimmt erst dann die Lease
/// (`POST turnier/lease`, das `n` muss passen). Unter der Lease holt er
/// keine Ereignisse — neue gibt es dann nur von diesem PC —, nur alle 60 s
/// die Stammdaten.
pub(crate) async fn abgleich_schritt_in(
    z: &Mutex<Option<KernQuelle>>,
    paket_faellig: bool,
    relay_ns: Option<&str>,
) -> bool {
    let sperre = || z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((basis, token, n, etag)) = sperre().as_ref().and_then(KernQuelle::zugang) else {
        return false;
    };
    // Standby-Übernahme (Paket S2): Bis der erste Upload den neuen Inhaber
    // gemeldet hat, holt dieser PC nichts von badhub — badhub trägt dort noch
    // die Ereignisse des Hauptrechners, die sich nicht mit den eigenen
    // mischen dürfen. Nur die Outbox arbeitet.
    let (offen, pruefen, neu) = match sperre().as_ref() {
        Some(q) => (
            q.standby_offen(),
            q.standby_pruefen,
            q.lease.as_ref().is_some_and(|l| l.aktiv() && l.neu_laden),
        ),
        None => return false,
    };
    if pruefen {
        match api::lease_lesen(&basis, &token).await {
            Ok(sicht) => {
                if let Some(q) = sperre().as_mut() {
                    q.standby_pruefen_antwort(sicht.lease);
                }
            }
            Err(e) => tracing::debug!("Quelle badhub: Lease nicht abgefragt ({e})"),
        }
        return false;
    }
    if offen {
        return false;
    }
    if neu {
        if let Err(e) = neu_laden_in(z, &basis, &token).await {
            tracing::info!("Quelle badhub: Stand nach Standby-Übernahme nicht neu geladen ({e})");
        }
        return false;
    }
    let ereignisse_holen = sperre().as_ref().is_some_and(|q| {
        let unter_lease = q.lease.as_ref().is_some_and(Lease::aktiv) && q.lease_angemeldet;
        let entzogen = q.lease.as_ref().is_some_and(|l| !l.aktiv());
        !unter_lease && q.wartende_posten() == 0 && !entzogen
    });
    let mut paket_geholt = false;
    let mut vollstaendig = true;
    if ereignisse_holen {
        let r = seiten_holen(&basis, &token, n, None, |neu| match sperre().as_mut() {
            Some(q) => {
                let zahl = q.ereignisse_uebernehmen(neu);
                if zahl > 0 {
                    tracing::info!(
                        "Quelle badhub: {zahl} neue Ereignisse (bis n={})",
                        q.paket.n()
                    );
                }
                q.paket.n()
            }
            None => 0,
        })
        .await;
        match r {
            Ok(fertig) => {
                vollstaendig = fertig;
                if !fertig {
                    if let Some(q) = sperre().as_mut().filter(|q| q.lease.is_none()) {
                        q.lease_hinweis = Some(format!(
                            "Stand wird von badhub nachgeladen (bis Ereignis {})",
                            q.paket.n()
                        ));
                    }
                }
            }
            Err(api::ApiFehler::Zugang) => {
                tracing::warn!("Quelle badhub: Zugang abgelehnt (401/403) — Abgleich angehalten");
                if let Some(q) = sperre().as_mut() {
                    q.zugang_abgelehnt = true;
                }
                return false;
            }
            // Offline: letzter Stand bleibt, kein Lärm im Log. Was bis zum
            // Abbruch kam, ist schon abgelegt. Die Lease wartet, bis der
            // Stand vollständig ist — sonst stimmte `n` nicht.
            Err(e) => {
                tracing::debug!("Quelle badhub: Ereignisse nicht abgeholt ({e})");
                vollstaendig = false;
                if let Some(q) = sperre().as_mut().filter(|q| q.lease.is_none()) {
                    q.lease_hinweis = Some(format!("{e}"));
                }
            }
        }
    }
    // Lease nehmen bzw. anmelden (nicht am Slave — der startet keinen
    // Abgleich), nur mit vollständigem Stand.
    let bedarf = if vollstaendig {
        sperre().as_mut().and_then(|q| q.lease_bedarf(relay_ns))
    } else {
        None
    };
    if let Some((n, relay)) = bedarf {
        let antwort = api::lease_nehmen(&basis, &token, n, relay.as_ref()).await;
        if let Some(q) = sperre().as_mut() {
            q.lease_antwort(antwort, relay);
        }
    }
    if paket_faellig {
        // Ein gekapptes Paket braucht hier kein Nachladen: Die Ereignisse
        // werden mit den gespeicherten vereinigt, und was fehlt, holt der
        // nächste Takt ab `n` (das Paket ist immer ein Anfang ohne Lücke).
        match api::paket(&basis, &token, etag.as_deref()).await {
            Ok(api::PaketAntwort::Unveraendert) => paket_geholt = true,
            Ok(api::PaketAntwort::Neu { paket, etag, .. }) => {
                paket_geholt = true;
                if let Some(q) = sperre().as_mut() {
                    // Zurückgegeben: Der Personenauszug ist gelöscht und
                    // bleibt es — das Paket kommt erst mit „übernehmen“.
                    let zurueck = q
                        .lease
                        .as_ref()
                        .is_some_and(|l| l.zustand == LeaseZustand::Zurueckgegeben);
                    if !zurueck {
                        let sicht = paket.turnier.lease.clone();
                        if sicht.is_some() || q.rolle.is_some() {
                            q.rolle_setzen(sicht);
                        }
                        match q.paket_uebernehmen(*paket, etag) {
                            Ok(()) => tracing::info!("Quelle badhub: Stammdaten aktualisiert"),
                            Err(e) => tracing::warn!("Quelle badhub: Paket verworfen ({e})"),
                        }
                    }
                }
            }
            Err(api::ApiFehler::Zugang) => {
                if let Some(q) = sperre().as_mut() {
                    q.zugang_abgelehnt = true;
                }
            }
            Err(e) => tracing::debug!("Quelle badhub: Paket nicht abgeholt ({e})"),
        }
    }
    paket_geholt
}

/// Abgleich mit badhub, solange die Übertragung läuft (läuft unter einem
/// `TaktWaechter` in `start_sync_kern`). `relay_ns` = eigener
/// Relay-Namespace, wenn der Cloud-Weg an ist — badhub leitet seine
/// Eingaben dann dorthin.
pub async fn abgleich_schleife(relay_ns: Option<String>) {
    let mut takt = tokio::time::interval(EREIGNIS_TAKT);
    takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut letztes_paket: Option<tokio::time::Instant> = None;
    loop {
        takt.tick().await;
        // Das Paket kam schon beim Koppeln bzw. liegt auf der Platte — die
        // erste Prüfung erst nach einem vollen Takt.
        let jetzt = tokio::time::Instant::now();
        let faellig = letztes_paket.is_some_and(|t| jetzt.duration_since(t) >= PAKET_TAKT);
        if letztes_paket.is_none() {
            letztes_paket = Some(jetzt);
        }
        if abgleich_schritt(faellig, relay_ns.as_deref()).await {
            letztes_paket = Some(jetzt);
        }
    }
}

/// Ein Hochlade-Schritt der Outbox gegen den Zustand `z` (Netzarbeit
/// außerhalb der Sperre).
pub(crate) async fn outbox_schritt(z: &Mutex<Option<KernQuelle>>) -> OutboxSchritt {
    let auftrag = z
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(KernQuelle::outbox_auftrag);
    let Some(a) = auftrag else {
        return OutboxSchritt::Ruhe;
    };
    let posten = a.posten;
    let r = api::hochladen(&a.basis, &a.token, a.epoche, a.basis_n, &posten, a.standby).await;
    match z
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_mut()
    {
        Some(q) => {
            let schritt = q.outbox_antwort(r, a.bis_lokal);
            if schritt == OutboxSchritt::Weiter {
                tracing::info!(
                    "Quelle badhub: {} Posten hochgeladen, bestätigt bis n={}",
                    posten.len(),
                    q.n_bestaetigt()
                );
            }
            schritt
        }
        None => OutboxSchritt::Ruhe,
    }
}

/// Wartezeit nach dem `fehler`-ten Fehlschlag in Folge (1-basiert).
pub fn outbox_wartezeit(fehler: usize) -> Duration {
    let i = fehler.saturating_sub(1).min(OUTBOX_BACKOFF_S.len() - 1);
    Duration::from_secs(OUTBOX_BACKOFF_S[i])
}

/// Die Outbox: lädt die Posten nach `basis_n` hoch, solange die Übertragung
/// läuft (eigener Task unter einem `TaktWaechter`). Neuversuch nach 5, 10,
/// 20, 40, 60 s, danach alle 60 s.
pub async fn outbox_schleife() {
    let mut fehler = 0usize;
    loop {
        match outbox_schritt(&ZUSTAND).await {
            OutboxSchritt::Weiter => {
                fehler = 0;
                continue;
            }
            OutboxSchritt::Spaeter => {
                fehler += 1;
                tokio::time::sleep(outbox_wartezeit(fehler)).await;
            }
            OutboxSchritt::Ruhe | OutboxSchritt::Angehalten => {
                fehler = 0;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// „Turnier beenden / Steuerung an badhub zurückgeben“: nur mit leerer
/// Outbox, dann `DELETE turnier/lease`; danach wird der Personenauszug
/// gelöscht.
pub(crate) async fn zurueckgeben_in(z: &Mutex<Option<KernQuelle>>) -> Result<String, String> {
    let auftrag = {
        let mut g = z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let q = g.as_mut().ok_or("Kein badhub-Turnier geladen.")?;
        let epoche = q
            .lease
            .as_ref()
            .filter(|l| l.aktiv())
            .ok_or("bts-light hält die Steuerung dieses Turniers nicht.")?
            .epoche;
        let wartend = q.wartende_ereignisse();
        if wartend > 0 {
            return Err(text_outbox_offen(wartend));
        }
        // Höchstens eine Rückgabe zugleich (Hauptfenster, mehrere TL-Geräte,
        // Doppeltipp): Ein zweiter `DELETE` bekäme `epoche_veraltet`, und das
        // sähe aus wie eine Rückholung.
        if q.rueckgabe_laeuft {
            return Err("Die Rückgabe läuft schon — bitte kurz warten.".into());
        }
        let token = q
            .token
            .clone()
            .ok_or("Kein Gerätetoken — bitte neu koppeln.")?;
        q.rueckgabe_laeuft = true;
        (q.basis.clone(), token, epoche, q.n_bestaetigt())
    };
    // Freigabe auch, wenn der Aufruf abgebrochen wird (Gerät trennt).
    struct Frei<'a>(&'a Mutex<Option<KernQuelle>>);
    impl Drop for Frei<'_> {
        fn drop(&mut self) {
            if let Some(q) = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                q.rueckgabe_laeuft = false;
            }
        }
    }
    let _frei = Frei(z);
    let (basis, token, epoche, n) = auftrag;
    let r = api::lease_zurueckgeben(&basis, &token, epoche, n).await;
    let mut g = z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let q = g.as_mut().ok_or("Kein badhub-Turnier geladen.")?;
    match r {
        Ok(api::RueckgabeAntwort::Zurueckgegeben) => {
            if let Some(l) = q.lease.as_mut() {
                l.zustand = LeaseZustand::Zurueckgegeben;
                l.letzter_kontakt_ms = jetzt_ms();
                let l = l.clone();
                lease::speichern(q.ablage.dir(), &l)?;
            }
            q.tl_zugang = None;
            let _ = std::fs::remove_file(q.ablage.dir().join(lease::ZUGANG_DATEI));
            q.personen_loeschen();
            tracing::info!("Quelle badhub: Steuerung an badhub zurückgegeben");
            Ok("Steuerung an badhub zurückgegeben. Der Personenauszug ist gelöscht.".into())
        }
        Ok(api::RueckgabeAntwort::NOffen { n }) => Err(format!(
            "badhub ist bei Ereignis {n}, dieser PC bei {} — bitte kurz warten und erneut versuchen.",
            q.n_bestaetigt()
        )),
        Ok(api::RueckgabeAntwort::EpocheVeraltet) => {
            q.entziehen();
            Err(TEXT_ENTZOGEN.into())
        }
        Err(api::ApiFehler::Zugang) => Err(
            "badhub lehnt den Zugang dieses PCs ab (Kopplung widerrufen?) — Rückgabe nicht möglich."
                .into(),
        ),
        Err(e) => Err(format!("{e} — Rückgabe braucht Internet.")),
    }
}

/// Rückmeldung, wenn die Rückgabe an der Outbox scheitert („N Ereignisse
/// warten noch“) — gleich im Hauptfenster und im TL-Web.
pub fn text_outbox_offen(wartend: usize) -> String {
    let (wort, verb) = if wartend == 1 {
        ("Ereignis", "wartet")
    } else {
        ("Ereignisse", "warten")
    };
    format!(
        "{wartend} {wort} {verb} noch auf den Abgleich mit badhub — die Rückgabe geht erst, wenn \
         „badhub: aktuell“ steht. Bitte mit Internet kurz warten."
    )
}

/// Globaler Weg für den Knopf im Hauptfenster und im TL-Web
/// (`TlAction::BadhubZurueckgeben`).
pub async fn turnier_beenden() -> Result<String, String> {
    zurueckgeben_in(&ZUSTAND).await
}

/// Nach einer Standby-Übernahme, deren Stand nicht zu badhub passt
/// (abgewiesene Posten, verlorene Antwort): Paket frisch von badhub, eigene
/// nicht bestätigte Posten nach `verworfen-<ms>.jsonl` — sie liegen, soweit
/// abgewiesen, in badhub als Aufgabe. Die Lease (neue Epoche) bleibt; der
/// Abgleich meldet danach die Relay-Angaben nach.
pub(crate) async fn neu_laden_in(
    z: &Mutex<Option<KernQuelle>>,
    basis: &str,
    token: &str,
) -> Result<usize, String> {
    let (mut paket, etag, bis) = match api::paket(basis, token, None).await {
        Ok(api::PaketAntwort::Neu {
            paket,
            etag,
            nachladen_bis,
        }) => (*paket, etag, nachladen_bis),
        Ok(api::PaketAntwort::Unveraendert) => return Err("kein Paket".into()),
        Err(e) => return Err(e.to_string()),
    };
    paket.pruefen()?;
    if paket_nachladen(basis, token, &mut paket, bis)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("Paket nur teilweise geladen".into());
    }
    let mut g = z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let q = g.as_mut().ok_or("Kein badhub-Turnier geladen.")?;
    if !paket
        .turnier
        .uuid
        .eq_ignore_ascii_case(&q.paket.turnier.uuid)
    {
        return Err("badhub liefert ein anderes Turnier als gekoppelt.".into());
    }
    let verworfen = q.protokoll.kuerzen_auf(q.paket.n(), jetzt_ms())?;
    q.protokoll.leeren()?;
    q.ablage.paket_speichern(&paket, etag.as_deref())?;
    let n = paket.n();
    q.paket = paket;
    q.etag = etag;
    if let Some(l) = q.lease.as_mut() {
        l.neu_laden = false;
        l.bis_n = n;
        let l = l.clone();
        lease::speichern(q.ablage.dir(), &l)?;
    }
    q.lease_angemeldet = false;
    q.abbild = None;
    q.kern = None;
    tracing::info!(
        "Quelle badhub: Stand nach Standby-Übernahme neu geladen ({verworfen} Posten verworfen)"
    );
    Ok(verworfen)
}

/// „Steuerung wieder übernehmen“ nach Rückgabe oder Rückholung: verwirft
/// eigene, nicht in badhub angekommene Ereignisse (abgelegt als
/// `verworfen-<ms>.jsonl`), lädt das Paket frisch und lässt den Abgleich die
/// Lease neu nehmen.
pub(crate) async fn uebernehmen_in(z: &Mutex<Option<KernQuelle>>) -> Result<String, String> {
    let (basis, token) = {
        let g = z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let q = g.as_ref().ok_or("Kein badhub-Turnier geladen.")?;
        if q.lease.as_ref().is_some_and(Lease::aktiv) {
            return Err("bts-light steuert dieses Turnier bereits.".into());
        }
        (
            q.basis.clone(),
            q.token
                .clone()
                .ok_or("Kein Gerätetoken — bitte neu koppeln.")?,
        )
    };
    let (mut paket, etag, bis) = match api::paket(&basis, &token, None).await {
        Ok(api::PaketAntwort::Neu {
            paket,
            etag,
            nachladen_bis,
        }) => (*paket, etag, nachladen_bis),
        Ok(api::PaketAntwort::Unveraendert) => {
            return Err("badhub liefert kein Turnierpaket.".into())
        }
        Err(e) => return Err(format!("{e} — Übernehmen braucht Internet.")),
    };
    paket.pruefen()?;
    // Gekapptes Paket: Rest seitenweise. Bleibt es unvollständig (offline),
    // holt der Abgleich den Rest, bevor er die Lease neu nimmt.
    let teilweise = paket_nachladen(&basis, &token, &mut paket, bis)
        .await
        .map_err(|e| format!("{e} — Übernehmen braucht Internet."))?;
    let mut g = z.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let q = g.as_mut().ok_or("Kein badhub-Turnier geladen.")?;
    if !paket
        .turnier
        .uuid
        .eq_ignore_ascii_case(&q.paket.turnier.uuid)
    {
        return Err("badhub liefert ein anderes Turnier als gekoppelt.".into());
    }
    let bis = q.n_bestaetigt();
    let verworfen = q.protokoll.kuerzen_auf(bis, jetzt_ms())?;
    // Was bleibt, hat badhub bestätigt und kommt über Paket und Ereignisse
    // zurück. Bliebe es liegen, zählte es über einem gekappten Paket als
    // „wartend“ und hielte Nachladen und Lease an.
    q.protokoll.leeren()?;
    q.ablage.paket_speichern(&paket, etag.as_deref())?;
    q.paket = paket;
    q.etag = etag;
    lease::entfernen(q.ablage.dir());
    q.lease = None;
    q.tl_zugang = None;
    q.lease_angemeldet = false;
    q.lease_hinweis = None;
    q.abweichung = None;
    q.zugang_abgelehnt = false;
    q.abbild = None;
    q.kern = None;
    tracing::info!("Quelle badhub: Steuerung wird neu übernommen ({verworfen} Posten verworfen)");
    let text = if verworfen > 0 {
        format!(
            "Stand aus badhub geladen; {verworfen} Eingaben dieses PCs waren nicht in badhub \
             angekommen und wurden verworfen. Die Steuerung wird gleich übernommen."
        )
    } else {
        "Stand aus badhub geladen; die Steuerung wird gleich übernommen.".to_string()
    };
    Ok(match teilweise {
        Some(h) => format!("{text} {h}"),
        None => text,
    })
}

/// Globaler Weg für den Knopf im Hauptfenster.
pub async fn steuerung_uebernehmen() -> Result<String, String> {
    uebernehmen_in(&ZUSTAND).await
}

fn heute() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// Löschfrist des Personenauszugs nach Turnierende (badhub-Spec
/// „Personendaten auf dem Hallen-PC“, analog ADR-0052).
pub const FRIST_TAGE: i64 = 14;

/// Räumt `turniere/` auf: Ordner alter Kopplungen ohne wartende Ereignisse
/// werden gelöscht; der Personenauszug jedes Turniers spätestens
/// [`FRIST_TAGE`] nach Turnierende (`turnier.ende`, sonst `beginn`).
/// Ein Turnier ohne Datum behält ihn (bis Rückgabe oder neue Kopplung).
pub fn aufraeumen(daten: &Path, aktuell: Option<&str>, heute: chrono::NaiveDate) {
    let Ok(eintraege) = std::fs::read_dir(daten.join("turniere")) else {
        return;
    };
    for e in eintraege.flatten() {
        let pfad = e.path();
        let Some(uuid) = pfad
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !pfad.is_dir() || !uuid_gueltig(&uuid) {
            continue;
        }
        let gespeichert: Option<ablage::GespeichertesPaket> =
            std::fs::read(pfad.join("paket.json"))
                .ok()
                .and_then(|t| serde_json::from_slice(&t).ok());
        let ende = gespeichert.as_ref().and_then(|g| {
            g.paket
                .turnier
                .ende
                .clone()
                .or_else(|| g.paket.turnier.beginn.clone())
        });
        let abgelaufen = ende
            .as_deref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok())
            .is_some_and(|d| heute > d + chrono::Duration::days(FRIST_TAGE));
        let bestaetigt = lease::laden(&pfad).map_or(0, |l| l.bis_n);
        let basis = gespeichert
            .as_ref()
            .map_or(0, |g| g.paket.n())
            .max(bestaetigt);
        let wartend = Protokoll::laden(&pfad)
            .map(|p| p.offen_nach(basis).count())
            .unwrap_or(0);
        let ist_aktuell = aktuell.is_some_and(|a| a.eq_ignore_ascii_case(&uuid));
        if !ist_aktuell && wartend == 0 {
            match std::fs::remove_dir_all(&pfad) {
                Ok(()) => tracing::info!("badhub-Turnier: Ordner einer alten Kopplung gelöscht"),
                Err(e) => tracing::warn!("badhub-Turnier: alter Ordner nicht gelöscht ({e})"),
            }
        } else if abgelaufen && pfad.join("personen.bin").exists() {
            if let Ok(a) = Ablage::neu(daten, &uuid) {
                a.personen_loeschen();
            }
        }
    }
}

/// Wartende Ereignisse im Ordner eines Turniers (für die Kopplung: ein
/// Neukoppeln darf sie nicht stillschweigend verwerfen).
pub fn wartende_in(daten: &Path, uuid: &str) -> usize {
    let Ok(a) = Ablage::neu(daten, uuid) else {
        return 0;
    };
    let Some(l) = lease::laden(a.dir()) else {
        return 0;
    };
    if l.zustand == LeaseZustand::Entzogen {
        return 0; // kommen ohnehin nicht mehr an
    }
    Protokoll::laden(a.dir())
        .map(|p| p.offen_nach(l.bis_n).count())
        .unwrap_or(0)
}

/// Ergebnis einer Kopplung (für den Einrichtungsassistenten).
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct Kopplung {
    pub turnier: BadhubTurnierConfig,
    pub konkurrenzen: usize,
    pub spiele: usize,
    /// Kern-Version in badhub, wenn sie von der eingebundenen abweicht.
    pub kern_hinweis: Option<String>,
    /// Paket nur teilweise geladen (Netz brach beim Nachladen ab).
    pub nachlade_hinweis: Option<String>,
}

/// Löst den Code ein, lädt das Paket und legt beides ab. Setzt die
/// Konfiguration **nicht** — das macht der Aufrufer unter seiner Sperre.
pub async fn koppeln(
    basis: &str,
    daten: &Path,
    code: &str,
    install_id: &str,
    geraet_name: &str,
) -> Result<Kopplung, String> {
    let code: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if code.is_empty() || code.len() > 32 {
        return Err("Bitte den Kopplungscode aus badhub eingeben.".into());
    }
    let name = geraet_name.trim();
    let name = if name.is_empty() { "bts-light" } else { name };
    let antwort = api::koppeln(basis, &code, install_id, name)
        .await
        .map_err(|e| e.to_string())?;
    if !uuid_gueltig(&antwort.turnier.uuid) {
        return Err("badhub nennt keine gültige Turnier-UUID.".into());
    }
    let (mut paket, etag, bis) = match api::paket(basis, &antwort.token, None)
        .await
        .map_err(|e| e.to_string())?
    {
        api::PaketAntwort::Neu {
            paket,
            etag,
            nachladen_bis,
        } => (*paket, etag, nachladen_bis),
        api::PaketAntwort::Unveraendert => {
            return Err("badhub liefert kein Turnierpaket.".into());
        }
    };
    paket.pruefen()?;
    // Gekapptes Paket (höchstens 2000 Ereignisse): Rest seitenweise. Bricht
    // das Netz mittendrin ab, gilt die Kopplung trotzdem — der Code ist
    // eingelöst, das Paket ist ein lückenloser Anfang, und der Abgleich holt
    // den Rest, bevor er die Lease nimmt.
    let nachlade_hinweis = paket_nachladen(basis, &antwort.token, &mut paket, bis)
        .await
        .map_err(|e| e.to_string())?;
    if !paket
        .turnier
        .uuid
        .eq_ignore_ascii_case(&antwort.turnier.uuid)
    {
        return Err("Turnierpaket und Kopplung nennen verschiedene Turniere.".into());
    }
    let ablage = Ablage::neu(daten, &antwort.turnier.uuid)?;
    let wartend = wartende_in(daten, &antwort.turnier.uuid);
    if wartend > 0 {
        return Err(format!(
            "Für dieses Turnier warten auf diesem PC noch {wartend} Eingaben auf den Abgleich mit \
             badhub. Bitte erst die Übertragung mit Internet laufen lassen, bis „badhub: aktuell“ \
             steht."
        ));
    }
    // Neue Kopplung = neues Gerät: alte Lease und Protokoll gelten nicht mehr.
    lease::entfernen(ablage.dir());
    let _ = std::fs::remove_file(ablage.dir().join(protokoll::DATEI));
    standby::entfernen(ablage.dir());
    ablage.token_speichern(&antwort.token)?;
    ablage.paket_speichern(&paket, etag.as_deref())?;
    if let Err(e) = standby::rolle_schreiben(ablage.dir(), paket.turnier.lease.clone()) {
        tracing::warn!("Quelle badhub: Rolle nicht gesichert ({e})");
    }
    let stand = turnierkern::falten_turnier(&paket.ereignisse);
    let spiele = stand.durchfuehrung.values().map(|d| d.spiele.len()).sum();
    let kern_hinweis = (!paket.kern_version.is_empty() && paket.kern_version != KERN_VERSION)
        .then(|| {
            format!(
                "badhub rechnet mit Turnierkern {}, diese Version mit {KERN_VERSION}. Anzeigen geht; \
                 vor dem Turniertag beide auf denselben Stand bringen.",
                paket.kern_version
            )
        });
    tracing::info!(
        "Quelle badhub: Turnier {} gekoppelt ({} Konkurrenzen, {spiele} Spiele)",
        antwort.turnier.id,
        paket.konkurrenzen.len()
    );
    Ok(Kopplung {
        turnier: BadhubTurnierConfig {
            uuid: antwort.turnier.uuid.to_ascii_lowercase(),
            id: antwort.turnier.id,
            name: antwort.turnier.name,
            basis: basis.to_string(),
            geraet_name: name.to_string(),
            // Nur eine gültige GUID übernehmen; sonst bleibt es bei der UUID
            // (`BadhubTurnierConfig::turnier_guid`).
            guid: paket
                .turnier
                .guid
                .as_deref()
                .and_then(crate::config::kanonische_guid)
                .unwrap_or_default(),
        },
        konkurrenzen: paket.konkurrenzen.len(),
        spiele,
        kern_hinweis,
        nachlade_hinweis,
    })
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_l3;
#[cfg(test)]
mod tests_rueckzug;
#[cfg(test)]
mod tests_standby;
