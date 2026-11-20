//! LAN-Ansage-Slave bei einem badhub-Turnier (Paket L3, badhub-Spec
//! „Einrichtung“): Der Slave liest kein BTP und nimmt keine Lease, sondern
//! holt die Kern-Ereignisse vom Master (`GET /quelle/ereignisse` am
//! LAN-Server des Turnier-PCs, Adresse wie bisher im Feld „BTP-Host“) und
//! faltet sie mit seinem eigenen Turnierpaket.
//!
//! Der Slave koppelt sich dafür selbst mit badhub (eigener Code, kein
//! Schreibrecht): So hat er Namen und Feldnamen und beweist mit der
//! Turnier-UUID im Kopf `X-Bts-Turnier`, dass er zum Turnier gehört. Über
//! die Leitung gehen nur Ereignisse — Nummern, keine Namen.

use std::time::Duration;

use turnierkern::TurnierEreignis;

use crate::btp::model::BtpSnapshot;
use crate::config::AppConfig;

/// Kopf mit der Turnier-UUID.
pub const KOPF_TURNIER: &str = "x-bts-turnier";
/// Pfad am LAN-Server des Masters.
pub const PFAD: &str = "/quelle/ereignisse";

/// Antwort des Masters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MasterEreignisse {
    pub uuid: String,
    pub ereignisse: Vec<TurnierEreignis>,
}

/// Was der Master auf `GET /quelle/ereignisse` antwortet: nur bei Quelle
/// badhub und nur für die passende Turnier-UUID.
pub fn fuer_slave(config: &AppConfig, uuid_kopf: Option<&str>) -> Result<MasterEreignisse, u16> {
    if config.quelle != crate::config::QuelleArt::Badhub || config.slave_mode {
        return Err(404);
    }
    let eigene = config.badhub_turnier.uuid.trim();
    match uuid_kopf {
        Some(u) if !eigene.is_empty() && u.trim().eq_ignore_ascii_case(eigene) => {}
        _ => return Err(403),
    }
    let ereignisse = super::zustand()
        .as_ref()
        .map(super::KernQuelle::alle_ereignisse)
        .ok_or(503u16)?;
    Ok(MasterEreignisse {
        uuid: eigene.to_ascii_lowercase(),
        ereignisse,
    })
}

/// Stand des Slaves: Ereignisse vom Master, gefaltet mit dem eigenen Paket.
pub async fn stand_vom_master(config: &AppConfig) -> Result<BtpSnapshot, String> {
    let uuid = config.badhub_turnier.uuid.trim().to_string();
    if uuid.is_empty() {
        return Err(
            "Ansage-Slave mit badhub-Turnier: bitte auch diesen PC mit einem Kopplungscode \
             aus badhub koppeln."
                .into(),
        );
    }
    let host = config.btp.host.trim();
    if host.is_empty() {
        return Err("Ansage-Slave: Adresse des Turnier-PCs (Master) fehlt.".into());
    }
    let url = format!("http://{host}:{}{PFAD}", crate::tablet::server::TABLET_PORT);
    let antwort = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?
        .get(&url)
        .header(KOPF_TURNIER, &uuid)
        .send()
        .await
        .map_err(|e| format!("Turnier-PC nicht erreichbar ({})", e.without_url()))?;
    if !antwort.status().is_success() {
        return Err(format!(
            "Turnier-PC liefert keinen Stand (Status {}) — gleiches badhub-Turnier gekoppelt?",
            antwort.status().as_u16()
        ));
    }
    let m: MasterEreignisse = antwort
        .json()
        .await
        .map_err(|_| "Stand vom Turnier-PC unlesbar".to_string())?;
    if !m.uuid.eq_ignore_ascii_case(&uuid) {
        return Err("Der Turnier-PC steuert ein anderes Turnier.".into());
    }
    match super::zustand().as_mut() {
        Some(q) => Ok(q.stand_aus(m.ereignisse)),
        None => Err("Kein badhub-Turnier geladen — bitte im Setup koppeln.".into()),
    }
}
