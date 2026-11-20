//! Standby-Übernahme ohne Netz (badhub-Spec „Standby-Übernahme ohne Netz“,
//! Nachtrag ADR-0084, Paket S2).
//!
//! Fällt der Hallen-PC mit der Lease aus und ist kein Netz da, führt ein
//! vorher in badhub als **Standby** eingetragener zweiter PC das Turnier
//! weiter:
//!
//! - **Spiegeln:** Der Standby läuft als LAN-Slave des Hauptrechners und holt
//!   über `GET /quelle/ereignisse?ab=<n>&posten=1` dessen **Posten** (Befehl +
//!   Ereignisse) in ein eigenes Protokoll `spiegel.jsonl`. Was der Hauptrechner
//!   schon von badhub bestätigt hat (`bestaetigt_n`), übernimmt der Standby
//!   als bestätigte Ereignisse ins eigene Paket.
//! - **Notfall-Sicherung:** Der Hauptrechner schreibt höchstens alle 30 s
//!   (und beim Anhalten) `notfall-<uuid>.btsk` in einen einstellbaren Ordner:
//!   UUID, Epoche, `n` und alle Posten — keine Namen, keine Tokens. Der
//!   Standby kann sie einlesen, wenn die LAN-Spiegelung fehlt oder älter ist.
//! - **Übernehmen:** Auf Knopfdruck wird der Spiegel zum Protokoll und eine
//!   Lease unter der **bisherigen Epoche** mit dem Merker `standby` angelegt.
//!   Der erste Upload geht mit `standby: true`; badhub antwortet mit der neuen
//!   Epoche (siehe `KernQuelle::outbox_antwort`).
//!
//! Gespeichert unter `turniere/<uuid>/`: `spiegel.jsonl`, `spiegel.json`
//! (Epoche, `n`, Zeit, Herkunft), `rolle.json` (Lease-Sicht aus badhub).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use turnierkern::{TurnierBefehl, TurnierEreignis};

use super::lease::{self, Lease, LeaseZustand};
use super::paket::PaketLease;
use super::protokoll::Posten;
use super::slave::MasterEreignisse;
use super::{jetzt_ms, KernQuelle};

/// Gespiegelte Posten des Hauptrechners.
pub const SPIEGEL_DATEI: &str = "spiegel.jsonl";
/// Stand der Spiegelung.
pub const SPIEGEL_STAND: &str = "spiegel.json";
/// Lease-Sicht aus badhub (Rolle dieses PCs).
pub const ROLLE_DATEI: &str = "rolle.json";
/// Kennung im Kopf der Notfall-Sicherung.
pub const SICHERUNG_FORMAT: &str = "bts-light-notfall";
/// Version des Sicherungsformats.
pub const SICHERUNG_VERSION: u32 = 1;
/// Größte Sicherung, die eingelesen wird (Schutz vor einer falschen Datei).
pub const SICHERUNG_MAX: u64 = 64 * 1024 * 1024;
/// Mindestabstand zweier Sicherungen.
pub const SICHERUNG_TAKT_MS: u64 = 30_000;

/// Akteur gespiegelter Posten (nur lokal).
pub const AKTEUR_SPIEGEL: &str = "hauptrechner";

/// Ein Posten ohne lokale Felder — so reist er über das LAN und steht in der
/// Sicherung (wie [`Posten::fuer_badhub`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostenWire {
    pub befehl: TurnierBefehl,
    pub ereignisse: Vec<TurnierEreignis>,
}

impl PostenWire {
    pub fn von(p: &Posten) -> Self {
        Self {
            befehl: p.befehl.clone(),
            ereignisse: p.ereignisse.clone(),
        }
    }
    fn n(&self) -> u64 {
        self.ereignisse
            .iter()
            .map(TurnierEreignis::n)
            .max()
            .unwrap_or(0)
    }
    fn erstes_n(&self) -> Option<u64> {
        self.ereignisse.iter().map(TurnierEreignis::n).min()
    }
    fn als_posten(self, zeit_ms: u64) -> Posten {
        Posten {
            befehl: self.befehl,
            ereignisse: self.ereignisse,
            akteur: AKTEUR_SPIEGEL.into(),
            zeit_ms,
        }
    }
}

/// Inhalt von `spiegel.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpiegelStand {
    /// Epoche der Lease des Hauptrechners.
    pub epoche: u64,
    /// Gespiegelt bis `n`.
    pub n: u64,
    /// Letzter Erfolg (Unix-ms).
    pub stand_ms: u64,
    /// `lan` | `sicherung`.
    pub quelle: String,
}

/// Inhalt von `rolle.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rolle {
    #[serde(default)]
    pub lease: Option<PaketLease>,
    #[serde(default)]
    pub stand_ms: u64,
}

/// Die Notfall-Sicherung `notfall-<uuid>.btsk` (JSON). Bewusst nur Posten,
/// Epoche und `n`: keine Namen (Posten tragen nur Nummern), keine Tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sicherung {
    pub format: String,
    pub version: u32,
    pub uuid: String,
    pub epoche: u64,
    pub n: u64,
    #[serde(default)]
    pub geschrieben_ms: u64,
    pub posten: Vec<PostenWire>,
}

/// Dateiname der Sicherung eines Turniers.
pub fn sicherung_dateiname(uuid: &str) -> String {
    format!("notfall-{}.btsk", uuid.to_ascii_lowercase())
}

/// Schreibt die Sicherung atomar (temporäre Datei + Umbenennen) in `ordner`.
pub fn sicherung_schreiben(ordner: &Path, s: &Sicherung) -> Result<PathBuf, String> {
    if !super::uuid_gueltig(&s.uuid) {
        return Err("ungültige Turnier-UUID".into());
    }
    if !ordner.is_dir() {
        return Err("Der Ordner für die Notfall-Sicherung ist nicht erreichbar.".into());
    }
    let ziel = ordner.join(sicherung_dateiname(&s.uuid));
    let text = serde_json::to_vec(s).map_err(|e| e.to_string())?;
    super::schluessel::schreiben_privat(&ziel, &text)?;
    Ok(ziel)
}

/// Liest und prüft eine Sicherung: Format, Version, UUID, lückenlose Posten,
/// `n` = letzter Posten. Eine abgeschnittene Datei ist kein gültiges JSON.
pub fn sicherung_lesen(pfad: &Path) -> Result<Sicherung, String> {
    let groesse = std::fs::metadata(pfad)
        .map_err(|_| "Die Sicherung ist nicht lesbar.".to_string())?
        .len();
    if groesse > SICHERUNG_MAX {
        return Err("Die Datei ist zu groß für eine Notfall-Sicherung.".into());
    }
    let roh = std::fs::read(pfad).map_err(|_| "Die Sicherung ist nicht lesbar.".to_string())?;
    let s: Sicherung = serde_json::from_slice(&roh).map_err(|_| {
        "Die Sicherung ist unvollständig oder keine Notfall-Sicherung von bts-light.".to_string()
    })?;
    if s.format != SICHERUNG_FORMAT {
        return Err("Die Datei ist keine Notfall-Sicherung von bts-light.".into());
    }
    if s.version != SICHERUNG_VERSION {
        return Err(format!(
            "Die Sicherung hat Version {}, diese bts-light-Version liest Version {SICHERUNG_VERSION}.",
            s.version
        ));
    }
    if !super::uuid_gueltig(&s.uuid) {
        return Err("Die Sicherung nennt keine gültige Turnier-UUID.".into());
    }
    let mut vorher: Option<u64> = None;
    for p in &s.posten {
        let (Some(a), b) = (p.erstes_n(), p.n()) else {
            return Err("Die Sicherung enthält einen Posten ohne Ereignisse.".into());
        };
        if vorher.is_some_and(|v| a != v + 1) {
            return Err("Die Posten der Sicherung sind nicht lückenlos.".into());
        }
        vorher = Some(b);
    }
    if vorher.unwrap_or(0) != s.n && !s.posten.is_empty() {
        return Err("Die Sicherung ist in sich nicht stimmig (n).".into());
    }
    Ok(s)
}

fn json_laden<T: for<'a> Deserialize<'a>>(dir: &Path, name: &str) -> Option<T> {
    std::fs::read(dir.join(name))
        .ok()
        .and_then(|t| serde_json::from_slice(&t).ok())
}

fn json_speichern<T: Serialize>(dir: &Path, name: &str, wert: &T) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Turnierordner: {e}"))?;
    let text = serde_json::to_vec_pretty(wert).map_err(|e| e.to_string())?;
    super::schluessel::schreiben_privat(&dir.join(name), &text)
}

/// Rolle beim Koppeln aus dem Paket ablegen (ohne geladenen Zustand).
pub(super) fn rolle_schreiben(dir: &Path, lease: Option<PaketLease>) -> Result<(), String> {
    json_speichern(
        dir,
        ROLLE_DATEI,
        &Rolle {
            lease,
            stand_ms: jetzt_ms(),
        },
    )
}

/// Spiegel und Rolle entfernen (neue Kopplung = neues Gerät).
pub(super) fn entfernen(dir: &Path) {
    for d in [SPIEGEL_DATEI, SPIEGEL_STAND, ROLLE_DATEI] {
        let _ = std::fs::remove_file(dir.join(d));
    }
}

pub(super) fn rolle_laden(dir: &Path) -> Option<Rolle> {
    json_laden(dir, ROLLE_DATEI)
}

pub(super) fn spiegel_stand_laden(dir: &Path) -> Option<SpiegelStand> {
    json_laden(dir, SPIEGEL_STAND)
}

/// Lage des Standby für das Hauptfenster des Standby-PCs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandbyLage {
    /// `standby` | `inhaber` | `keine` (laut letzter Auskunft von badhub).
    pub rolle: String,
    /// Name des Hauptrechners in badhub.
    pub hauptrechner: String,
    pub epoche: u64,
    /// Gespiegelt bis `n` (0 = nichts).
    pub gespiegelt_n: u64,
    pub gespiegelt_ms: u64,
    /// `lan` | `sicherung` | leer.
    pub quelle: String,
    /// Fertiger Text („Standby für Halle 1 PC, gespiegelt bis n = 412, vor 3 s“).
    pub text: String,
    /// Darf „Hauptrechner ausgefallen – Turnier übernehmen“?
    pub kann_uebernehmen: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hinweis: Option<String>,
}

impl KernQuelle {
    /// Lease-Sicht von badhub übernehmen und ablegen (Paket, `GET
    /// /turnier/lease`).
    pub fn rolle_setzen(&mut self, lease: Option<PaketLease>) {
        let r = Rolle {
            lease,
            stand_ms: jetzt_ms(),
        };
        if let Err(e) = json_speichern(self.ablage.dir(), ROLLE_DATEI, &r) {
            tracing::warn!("Quelle badhub: Rolle nicht gesichert ({e})");
        }
        self.rolle = Some(r);
    }

    /// Ist dieser PC laut badhub der Standby der laufenden Lease?
    pub fn ist_standby(&self) -> bool {
        self.rolle
            .as_ref()
            .and_then(|r| r.lease.as_ref())
            .is_some_and(PaketLease::ist_standby)
    }

    /// Gespiegelt bis (bestätigt im Paket oder im Spiegel).
    pub fn spiegel_n(&self) -> u64 {
        self.paket.n().max(self.spiegel.n())
    }

    /// Eine Antwort des Hauptrechners in den Spiegel übernehmen. `Err`, wenn
    /// der Hauptrechner keine Posten liefert (ältere Version).
    pub fn spiegel_aufnehmen(&mut self, m: MasterEreignisse) -> Result<(), String> {
        let (Some(posten), Some(bestaetigt)) = (m.posten, m.bestaetigt_n) else {
            return Err(
                "Der Hauptrechner liefert keine Posten (ältere bts-light-Version) — Standby-Spiegelung \
                 nicht möglich, bitte dort aktualisieren."
                    .into(),
            );
        };
        let jetzt = jetzt_ms();
        if m.n < self.spiegel_n() {
            // Der Hauptrechner ist hinter dem Spiegel (Steuerung neu
            // übernommen, anderes Protokoll): nicht still zusammenführen —
            // den Spiegel sichern und von vorn.
            let weg = self.spiegel.kuerzen_auf(self.paket.n(), jetzt)?;
            tracing::warn!(
                "Quelle badhub: Hauptrechner hinter dem Spiegel — {weg} gespiegelte Posten gesichert"
            );
        }
        // Vom Hauptrechner bestätigt = von badhub bestätigt: lückenlos ins Paket.
        let mut neu_bestaetigt = Vec::new();
        let mut naechstes = self.paket.n() + 1;
        // Schon gespiegelte Posten, die der Hauptrechner inzwischen bestätigt
        // hat, zuerst (der Master schickt nur Ereignisse über `ab`).
        let gespiegelt: Vec<TurnierEreignis> = self
            .spiegel
            .posten()
            .iter()
            .flat_map(|p| p.ereignisse.iter().cloned())
            .collect();
        for e in gespiegelt.into_iter().chain(m.ereignisse) {
            if e.n() > bestaetigt || e.n() < naechstes {
                continue;
            }
            if e.n() != naechstes {
                break;
            }
            naechstes += 1;
            neu_bestaetigt.push(e);
        }
        if !neu_bestaetigt.is_empty() {
            self.ereignisse_uebernehmen(neu_bestaetigt);
        }
        // Posten darüber in den Spiegel, nur lückenlos.
        let mut anhaengen = Vec::new();
        let mut bis = self.spiegel_n();
        for p in posten {
            if p.n() <= bis {
                continue;
            }
            if p.erstes_n() != Some(bis + 1) {
                break;
            }
            bis = p.n();
            anhaengen.push(p.als_posten(jetzt));
        }
        self.spiegel.anhaengen(anhaengen)?;
        let stand = SpiegelStand {
            epoche: m
                .epoche
                .or(self.spiegel_stand.as_ref().map(|s| s.epoche))
                .unwrap_or(0),
            n: self.spiegel_n(),
            stand_ms: jetzt,
            quelle: "lan".into(),
        };
        json_speichern(self.ablage.dir(), SPIEGEL_STAND, &stand)?;
        self.spiegel_stand = Some(stand);
        self.standby_hinweis = None;
        Ok(())
    }

    /// Alle Ereignisse am Standby: bestätigte (Paket) und gespiegelte.
    fn standby_ereignisse(&self) -> Vec<TurnierEreignis> {
        let basis = self.paket.n();
        let mut alle = self.paket.ereignisse.clone();
        alle.extend(
            self.spiegel
                .posten()
                .iter()
                .flat_map(|p| p.ereignisse.iter())
                .filter(|e| e.n() > basis)
                .cloned(),
        );
        alle
    }

    /// Stand am Standby (Paket + Spiegel).
    pub fn stand_standby(&mut self) -> crate::btp::model::BtpSnapshot {
        let schluessel = (
            self.spiegel_n(),
            self.paket.ereignisse.len() + self.spiegel.posten().len(),
            self.etag.clone(),
            self.paket.meldungen.len(),
        );
        if let Some((s, snap)) = &self.abbild {
            if *s == schluessel {
                return snap.clone();
            }
        }
        let stand = turnierkern::falten_turnier(&self.standby_ereignisse());
        let snap = super::abbildung::abbilden(&stand, &self.paket);
        self.abbild = Some((schluessel, snap.clone()));
        snap
    }

    /// Die Notfall-Sicherung des Hauptrechners — nur unter aktiver Lease.
    pub fn notfall_sicherung(&self) -> Option<Sicherung> {
        let l = self.lease.as_ref().filter(|l| l.aktiv())?;
        Some(Sicherung {
            format: SICHERUNG_FORMAT.into(),
            version: SICHERUNG_VERSION,
            uuid: self.paket.turnier.uuid.to_ascii_lowercase(),
            epoche: l.epoche,
            n: self.protokoll.n(),
            geschrieben_ms: jetzt_ms(),
            posten: self
                .protokoll
                .posten()
                .iter()
                .map(PostenWire::von)
                .collect(),
        })
    }

    /// Eine Notfall-Sicherung am Standby einlesen (oder an einem frisch
    /// gekoppelten PC, den badhub als Standby kennt).
    pub fn sicherung_einlesen(&mut self, s: Sicherung) -> Result<String, String> {
        if !s.uuid.eq_ignore_ascii_case(&self.paket.turnier.uuid) {
            return Err("Die Sicherung gehört zu einem anderen Turnier.".into());
        }
        if self.lease.as_ref().is_some_and(Lease::aktiv) {
            return Err(
                "Dieser PC steuert das Turnier selbst — eine Sicherung braucht er nicht.".into(),
            );
        }
        let basis = self.paket.n();
        let rest: Vec<PostenWire> = s.posten.into_iter().filter(|p| p.n() > basis).collect();
        if let Some(erstes) = rest.first().and_then(PostenWire::erstes_n) {
            if erstes != basis + 1 {
                return Err(format!(
                    "Die Sicherung schließt nicht an den Stand dieses PCs an (sie beginnt bei \
                     Ereignis {erstes}, dieser PC kennt bis {basis}) — bitte diesen PC einmal mit \
                     Internet abgleichen lassen."
                ));
            }
        }
        if s.n <= self.spiegel_n() {
            return Err(format!(
                "Die Sicherung reicht bis Ereignis {}, dieser PC hat schon bis {} gespiegelt — nichts \
                 Neues.",
                s.n,
                self.spiegel_n()
            ));
        }
        let jetzt = jetzt_ms();
        // Den bisherigen Spiegel nicht wegwerfen, sondern daneben legen.
        if !self.spiegel.posten().is_empty() {
            self.spiegel.kuerzen_auf(0, jetzt)?;
        }
        self.spiegel
            .ersetzen(rest.into_iter().map(|p| p.als_posten(jetzt)).collect())?;
        let stand = SpiegelStand {
            epoche: s.epoche,
            n: self.spiegel_n(),
            stand_ms: jetzt,
            quelle: "sicherung".into(),
        };
        json_speichern(self.ablage.dir(), SPIEGEL_STAND, &stand)?;
        self.spiegel_stand = Some(stand);
        self.abbild = None;
        tracing::info!(
            "Quelle badhub: Notfall-Sicherung eingelesen (bis n={})",
            self.spiegel_n()
        );
        Ok(format!(
            "Sicherung eingelesen: Stand bis Ereignis {}.",
            self.spiegel_n()
        ))
    }

    /// Lage für das Hauptfenster des Standby-PCs.
    pub fn standby_lage(&self) -> StandbyLage {
        let lease = self.rolle.as_ref().and_then(|r| r.lease.as_ref());
        let rolle = match lease.and_then(|l| l.du_bist.as_deref()) {
            Some("standby") => "standby",
            Some("inhaber") => "inhaber",
            _ => "keine",
        };
        let hauptrechner = lease.map(|l| l.geraet.clone()).unwrap_or_default();
        let st = self.spiegel_stand.clone().unwrap_or_default();
        let jetzt = jetzt_ms();
        let alter = if st.stand_ms > 0 {
            let s = jetzt.saturating_sub(st.stand_ms) / 1000;
            if s < 120 {
                format!("vor {s} s")
            } else {
                format!("vor {} min", s / 60)
            }
        } else {
            "noch nie".into()
        };
        let epoche = if st.epoche > 0 {
            st.epoche
        } else {
            lease.map_or(0, |l| l.epoche)
        };
        let text = match rolle {
            "standby" => format!(
                "Standby für {}, gespiegelt bis n = {}, {alter}{}",
                if hauptrechner.is_empty() {
                    "den Hauptrechner"
                } else {
                    hauptrechner.as_str()
                },
                self.spiegel_n(),
                if st.quelle == "sicherung" {
                    " (aus der Notfall-Sicherung)"
                } else {
                    ""
                }
            ),
            _ => "Dieser PC ist in badhub nicht als Standby eingetragen.".to_string(),
        };
        let haelt = self.lease.as_ref().is_some_and(Lease::aktiv);
        StandbyLage {
            rolle: rolle.into(),
            hauptrechner,
            epoche,
            gespiegelt_n: self.spiegel_n(),
            gespiegelt_ms: st.stand_ms,
            quelle: st.quelle,
            text,
            kann_uebernehmen: rolle == "standby" && epoche > 0 && !haelt,
            hinweis: self.standby_hinweis.clone(),
        }
    }

    /// „Hauptrechner ausgefallen – Turnier übernehmen“: Der Spiegel wird zum
    /// Protokoll, die Lease gilt unter der bisherigen Epoche mit dem Merker
    /// `standby`. Danach schreibt dieser PC wie ein Hauptrechner.
    pub fn als_standby_uebernehmen(&mut self) -> Result<String, String> {
        if self.lease.as_ref().is_some_and(Lease::aktiv) {
            return Err("Dieser PC steuert das Turnier bereits.".into());
        }
        let lage = self.standby_lage();
        if lage.rolle != "standby" {
            return Err(
                "badhub hat diesen PC nicht als Standby bestätigt. In badhub im Bereich Turniertag \
                 „Als Standby festlegen“ wählen und diesen PC einmal mit Internet abfragen lassen."
                    .into(),
            );
        }
        if lage.epoche == 0 {
            return Err(
                "Kein gespiegelter Stand und keine Notfall-Sicherung — die Epoche des Hauptrechners \
                 ist unbekannt."
                    .into(),
            );
        }
        let jetzt = jetzt_ms();
        // Was ohne Lease im eigenen Protokoll läge, ist bestätigt (beim Laden
        // geräumt) — ersetzt wird es durch den Spiegel.
        let posten = self.spiegel.posten().to_vec();
        self.protokoll.ersetzen(posten)?;
        let l = Lease {
            epoche: lage.epoche,
            bis_n: self.paket.n(),
            zustand: LeaseZustand::Aktiv,
            relay_ns: None,
            seit_ms: jetzt,
            letzter_kontakt_ms: 0,
            standby: true,
            ..Default::default()
        };
        lease::speichern(self.ablage.dir(), &l)?;
        self.spiegel.ersetzen(Vec::new())?;
        let _ = std::fs::remove_file(self.ablage.dir().join(SPIEGEL_STAND));
        self.spiegel_stand = None;
        self.lease = Some(l);
        self.lease_angemeldet = false;
        self.lease_hinweis = None;
        self.kein_standby = false;
        self.kern = None;
        self.abbild = None;
        tracing::warn!(
            "Quelle badhub: Turnier als Standby übernommen (Epoche {}, bis n={})",
            lage.epoche,
            self.n_lokal()
        );
        Ok(format!(
            "Turnier übernommen (Stand bis Ereignis {}). Dieser PC ist jetzt der Turnier-PC: \
             Tablets und Monitore bitte neu koppeln (QR-Codes dieses PCs; mit Cloud-Verbindung ein \
             anderer Namensraum). Die Eingaben gehen an badhub, sobald Internet da ist.",
            self.n_lokal()
        ))
    }

    /// Antwort `200 {bis_n, epoche, abgewiesen}` auf den Standby-Upload:
    /// dieser PC hält jetzt die Lease unter der neuen Epoche. Passt der
    /// eigene Stand zu badhub, geht es normal weiter (Relay-Angaben meldet
    /// der Abgleich per `POST turnier/lease` nach); sonst wird der Stand neu
    /// geladen (`neu_laden`).
    pub(super) fn standby_uebernommen(
        &mut self,
        bis_n: u64,
        epoche: u64,
        abgewiesen: u64,
        bis_lokal: u64,
    ) -> super::OutboxSchritt {
        let jetzt = jetzt_ms();
        // Alles mitgeschickt und nichts abgewiesen: Der eigene Stand ist ein
        // Anfang des badhub-Stands (liegt `bis_n` darüber, hatte der
        // Hauptrechner noch mehr hochgeladen — der Abgleich holt den Rest).
        let passt = abgewiesen == 0
            && (bis_n == bis_lokal || (bis_n > bis_lokal && bis_lokal == self.n_lokal()));
        if let Some(l) = self.lease.as_mut() {
            l.epoche = epoche;
            l.standby = false;
            l.abgewiesen = abgewiesen;
            l.letzter_kontakt_ms = jetzt;
            l.neu_laden = !passt;
        }
        self.lease_angemeldet = false;
        self.tl_zugang = None;
        let _ = std::fs::remove_file(self.ablage.dir().join(lease::ZUGANG_DATEI));
        tracing::warn!(
            "Quelle badhub: Standby-Übernahme bestätigt (Epoche {epoche}, bis n={bis_n}, {abgewiesen} abgewiesen)"
        );
        if passt {
            self.bestaetigen(bis_lokal);
            super::OutboxSchritt::Weiter
        } else {
            if let Some(l) = self.lease.clone() {
                if let Err(e) = lease::speichern(self.ablage.dir(), &l) {
                    tracing::warn!("Quelle badhub: Lease nicht gesichert ({e})");
                }
            }
            super::OutboxSchritt::Angehalten
        }
    }

    /// Antwort auf `GET turnier/lease` nach `epoche_veraltet` beim
    /// Standby-Upload: Hält dieser PC die Lease inzwischen (die Antwort auf
    /// den Upload ging verloren), gilt die neue Epoche und der Stand wird neu
    /// geladen; sonst ist die Steuerung entzogen.
    pub(super) fn standby_pruefen_antwort(&mut self, sicht: Option<PaketLease>) {
        self.standby_pruefen = false;
        match sicht {
            Some(l) if l.du_bist.as_deref() == Some("inhaber") => {
                if let Some(me) = self.lease.as_mut() {
                    me.epoche = l.epoche;
                    me.standby = false;
                    me.neu_laden = true;
                    me.letzter_kontakt_ms = jetzt_ms();
                    let me = me.clone();
                    if let Err(e) = lease::speichern(self.ablage.dir(), &me) {
                        tracing::warn!("Quelle badhub: Lease nicht gesichert ({e})");
                    }
                }
                self.lease_angemeldet = false;
                tracing::info!(
                    "Quelle badhub: Standby-Übernahme galt schon (Epoche {})",
                    l.epoche
                );
                self.rolle_setzen(Some(l));
            }
            andere => {
                self.rolle_setzen(andere);
                self.entziehen();
            }
        }
    }
}
