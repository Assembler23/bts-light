//! Quelle `badhub`: ein in badhub organisiertes Turnier, dessen Stand der
//! Turnierkern lokal aus den Ereignissen des Turnierpakets faltet (Paket L2,
//! badhub `docs/features/bts_light_steuerung.md`, ADR-0084).
//!
//! **Nur lesend.** Schreiben (Ergebnis, Feldvergabe, Offizielle …) folgt mit
//! L3; bis dahin weist `quelle` jeden Schreibweg mit
//! [`super::SCHREIBEN_FOLGT`] ab.
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
pub mod paket;
pub mod schluessel;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use crate::btp::model::BtpSnapshot;
use crate::config::{AppConfig, BadhubTurnierConfig};

use ablage::Ablage;
use paket::Paket;

/// Version der eingebundenen Kern-Crate (gleich dem Tag in `Cargo.toml`,
/// geprüft im Test `kern_version_passt_zum_cargo_tag`).
pub const KERN_VERSION: &str = "0.12.0";

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

/// Gültigkeit der zwischengespeicherten Abbildung: (`n`, Ereigniszahl, ETag).
type AbbildSchluessel = (u64, usize, Option<String>);

/// Laufzeitzustand eines gekoppelten Turniers.
#[derive(Debug)]
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
        Ok(Self {
            ablage,
            basis: cfg.basis.clone(),
            token,
            paket,
            etag,
            abbild: None,
            zugang_abgelehnt: false,
        })
    }

    /// Stand im heutigen Modell (Faltung nur bei neuen Daten).
    pub fn stand(&mut self) -> BtpSnapshot {
        let schluessel = (
            self.paket.n(),
            self.paket.ereignisse.len(),
            self.etag.clone(),
        );
        if let Some((s, snap)) = &self.abbild {
            if *s == schluessel {
                return snap.clone();
            }
        }
        let stand = turnierkern::falten_turnier(&self.paket.ereignisse);
        let snap = abbildung::abbilden(&stand, &self.paket);
        self.abbild = Some((schluessel, snap.clone()));
        snap
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
    fn zugang(&self) -> Option<(String, String, u64, Option<String>)> {
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
    match KernQuelle::laden(daten, &config.badhub_turnier) {
        Ok(q) => {
            tracing::info!(
                "Quelle badhub: Turnier {} geladen ({} Ereignisse, Kern {})",
                config.badhub_turnier.id,
                q.paket.ereignisse.len(),
                KERN_VERSION
            );
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

/// Liest den Stand des gekoppelten Turniers.
pub fn stand(config: &AppConfig) -> Result<BtpSnapshot, String> {
    if config.slave_mode {
        // Der LAN-Slave liest bei Kern-Turnieren den Stand vom Master (Spec
        // „Einrichtung“) — das kommt mit L3. BTP liest er hier bewusst nicht.
        return Err(
            "Ansage-Slave mit badhub-Turnier folgt in einer späteren Version — bitte den Slave \
             vorerst mit Quelle BTP betreiben."
                .into(),
        );
    }
    match zustand().as_mut() {
        Some(q) => Ok(q.stand()),
        None => Err(
            "Kein badhub-Turnier geladen — im Setup unter „Quelle“ einen Kopplungscode eingeben."
                .into(),
        ),
    }
}

/// Ein Schritt des Abgleichs; reine Netzarbeit außerhalb der Sperre.
async fn abgleich_schritt(paket_faellig: bool) -> bool {
    let Some((basis, token, n, etag)) = zustand().as_ref().and_then(KernQuelle::zugang) else {
        return false;
    };
    let mut paket_geholt = false;
    match api::ereignisse(&basis, &token, n).await {
        Ok(neu) if !neu.is_empty() => {
            if let Some(q) = zustand().as_mut() {
                let zahl = q.ereignisse_uebernehmen(neu);
                if zahl > 0 {
                    tracing::info!(
                        "Quelle badhub: {zahl} neue Ereignisse (bis n={})",
                        q.paket.n()
                    );
                }
            }
        }
        Ok(_) => {}
        Err(api::ApiFehler::Zugang) => {
            tracing::warn!("Quelle badhub: Zugang abgelehnt (401/403) — Abgleich angehalten");
            if let Some(q) = zustand().as_mut() {
                q.zugang_abgelehnt = true;
            }
            return false;
        }
        // Offline: letzter Stand bleibt, kein Lärm im Log.
        Err(e) => tracing::debug!("Quelle badhub: Ereignisse nicht abgeholt ({e})"),
    }
    if paket_faellig {
        match api::paket(&basis, &token, etag.as_deref()).await {
            Ok(api::PaketAntwort::Unveraendert) => paket_geholt = true,
            Ok(api::PaketAntwort::Neu { paket, etag }) => {
                paket_geholt = true;
                if let Some(q) = zustand().as_mut() {
                    match q.paket_uebernehmen(*paket, etag) {
                        Ok(()) => tracing::info!("Quelle badhub: Stammdaten aktualisiert"),
                        Err(e) => tracing::warn!("Quelle badhub: Paket verworfen ({e})"),
                    }
                }
            }
            Err(api::ApiFehler::Zugang) => {
                if let Some(q) = zustand().as_mut() {
                    q.zugang_abgelehnt = true;
                }
            }
            Err(e) => tracing::debug!("Quelle badhub: Paket nicht abgeholt ({e})"),
        }
    }
    paket_geholt
}

/// Abgleich mit badhub, solange die Übertragung läuft (läuft unter einem
/// `TaktWaechter` in `start_sync_kern`).
pub async fn abgleich_schleife() {
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
        if abgleich_schritt(faellig).await {
            letztes_paket = Some(jetzt);
        }
    }
}

/// Ergebnis einer Kopplung (für den Einrichtungsassistenten).
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct Kopplung {
    pub turnier: BadhubTurnierConfig,
    pub konkurrenzen: usize,
    pub spiele: usize,
    /// Kern-Version in badhub, wenn sie von der eingebundenen abweicht.
    pub kern_hinweis: Option<String>,
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
    let (paket, etag) = match api::paket(basis, &antwort.token, None)
        .await
        .map_err(|e| e.to_string())?
    {
        api::PaketAntwort::Neu { paket, etag } => (*paket, etag),
        api::PaketAntwort::Unveraendert => {
            return Err("badhub liefert kein Turnierpaket.".into());
        }
    };
    paket.pruefen()?;
    if !paket
        .turnier
        .uuid
        .eq_ignore_ascii_case(&antwort.turnier.uuid)
    {
        return Err("Turnierpaket und Kopplung nennen verschiedene Turniere.".into());
    }
    let ablage = Ablage::neu(daten, &antwort.turnier.uuid)?;
    ablage.token_speichern(&antwort.token)?;
    ablage.paket_speichern(&paket, etag.as_deref())?;
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
        },
        konkurrenzen: paket.konkurrenzen.len(),
        spiele,
        kern_hinweis,
    })
}

#[cfg(test)]
mod tests;
