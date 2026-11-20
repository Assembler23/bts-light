//! LAN-Ansage-Slave bei einem badhub-Turnier (Paket L3, badhub-Spec
//! „Einrichtung“): Der Slave liest kein BTP und nimmt keine Lease, sondern
//! holt die Kern-Ereignisse vom Master (`GET /quelle/ereignisse?ab=<n>` am
//! LAN-Server des Turnier-PCs, Adresse im Feld „Adresse des Turnier-PCs“)
//! und faltet sie mit seinem eigenen Turnierpaket.
//!
//! **Zugang:** Der Slave koppelt sich selbst mit badhub (Namen, Felder) und
//! beweist mit dem **Slave-Schlüssel** im Kopf `X-Bts-Slave`, dass er zum
//! Turnier-PC gehört. Den Schlüssel (16 Zufallsbytes) erzeugt der Master beim
//! Start, zeigt ihn im Dashboard, und die Turnierleitung trägt ihn am Slave
//! ein; beide legen ihn verschlüsselt unter `turniere/<uuid>/slave.bin` ab.
//! Verglichen wird in konstanter Zeit. Die Turnier-UUID allein reicht nicht —
//! sie ist öffentlich (Security-Review L3).
//!
//! Über die Leitung gehen nur Ereignisse (Nummern, keine Namen), seitenweise
//! ab dem Stand des Slaves.

use std::time::Duration;

use turnierkern::TurnierEreignis;

use crate::btp::model::BtpSnapshot;
use crate::config::AppConfig;

/// Kopf mit dem Slave-Schlüssel.
pub const KOPF_SCHLUESSEL: &str = "x-bts-slave";
/// Pfad am LAN-Server des Masters.
pub const PFAD: &str = "/quelle/ereignisse";
/// Dateiname des Schlüssels im Turnierordner.
pub const SCHLUESSEL_DATEI: &str = "slave.bin";
/// Höchstzahl der Ereignisse je Antwort.
pub const SEITE_MAX: usize = 500;
/// Höchstzahl der Seiten je Abruf des Slaves (der Rest kommt im nächsten Takt).
const SEITEN_JE_ABRUF: usize = 20;

/// Antwort des Masters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MasterEreignisse {
    pub uuid: String,
    /// Stand des Masters (höchstes `n`).
    pub n: u64,
    /// Ereignisse mit `n > ab`, höchstens [`SEITE_MAX`].
    pub ereignisse: Vec<TurnierEreignis>,
}

/// Schlüssel in der Eingabeform normalisieren: ohne Bindestriche und
/// Leerraum, klein, nur Hex. `None` bei falscher Länge.
pub fn schluessel_normal(eingabe: &str) -> Option<String> {
    let s: String = eingabe
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    (s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())).then_some(s)
}

/// Anzeigeform `abcd-ef01-…` (8 Gruppen à 4).
pub fn schluessel_anzeige(s: &str) -> String {
    s.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// Neuer Schlüssel (16 Zufallsbytes als Hex).
pub fn neuer_schluessel() -> Result<String, String> {
    super::lease::neuer_zugang().map(|z| z[..32].to_string())
}

/// Vergleich in konstanter Zeit (bei gleicher Länge).
pub fn gleich_konstant(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Was der Master auf `GET /quelle/ereignisse` antwortet: nur bei Quelle
/// badhub am Master und nur mit dem richtigen Slave-Schlüssel.
pub fn fuer_slave(
    config: &AppConfig,
    schluessel_kopf: Option<&str>,
    ab: u64,
) -> Result<MasterEreignisse, u16> {
    if config.quelle != crate::config::QuelleArt::Badhub || config.slave_mode {
        return Err(404);
    }
    let g = super::zustand();
    let q = g.as_ref().ok_or(503u16)?;
    fuer_slave_aus(q, schluessel_kopf, ab)
}

/// Kern von [`fuer_slave`] am Zustand (testbar ohne globalen Zustand).
pub(crate) fn fuer_slave_aus(
    q: &super::KernQuelle,
    schluessel_kopf: Option<&str>,
    ab: u64,
) -> Result<MasterEreignisse, u16> {
    let erwartet = q.slave_schluessel.as_deref().ok_or(403u16)?;
    let gegeben = schluessel_kopf
        .and_then(schluessel_normal)
        .unwrap_or_default();
    if !gleich_konstant(gegeben.as_bytes(), erwartet.as_bytes()) {
        return Err(403);
    }
    Ok(MasterEreignisse {
        uuid: q.paket.turnier.uuid.to_ascii_lowercase(),
        n: q.n_lokal(),
        ereignisse: q.ereignisse_ab(ab, SEITE_MAX),
    })
}

/// Stand des Slaves: Ereignisse vom Master (seitenweise ab dem eigenen
/// Stand), gefaltet mit dem eigenen Paket.
pub async fn stand_vom_master(config: &AppConfig) -> Result<BtpSnapshot, String> {
    let host = config.btp.host.trim().to_string();
    if host.is_empty() {
        return Err("Ansage-Slave: Adresse des Turnier-PCs (Master) fehlt.".into());
    }
    let (uuid, schluessel) = {
        let g = super::zustand();
        let q = g.as_ref().ok_or(
            "Ansage-Slave mit badhub-Turnier: bitte auch diesen PC mit einem Kopplungscode aus \
             badhub koppeln.",
        )?;
        let s = q.slave_schluessel.clone().ok_or(
            "Ansage-Slave: Slave-Schlüssel vom Turnier-PC fehlt — in den Einstellungen unter \
             „Turnierquelle“ eintragen.",
        )?;
        (q.paket.turnier.uuid.clone(), s)
    };
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    for _ in 0..SEITEN_JE_ABRUF {
        let ab = super::zustand().as_ref().map_or(0, |q| q.slave_n());
        let url = format!(
            "http://{host}:{}{PFAD}?ab={ab}",
            crate::tablet::server::TABLET_PORT
        );
        let antwort = client
            .get(&url)
            .header(KOPF_SCHLUESSEL, &schluessel)
            .send()
            .await
            .map_err(|e| format!("Turnier-PC nicht erreichbar ({})", e.without_url()))?;
        if !antwort.status().is_success() {
            return Err(format!(
                "Turnier-PC liefert keinen Stand (Status {}) — Slave-Schlüssel und Turnier prüfen.",
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
        let voll = m.ereignisse.len() >= SEITE_MAX;
        if let Some(q) = super::zustand().as_mut() {
            q.slave_uebernehmen(m.n, m.ereignisse);
        }
        if !voll {
            break;
        }
    }
    match super::zustand().as_mut() {
        Some(q) => Ok(q.stand_slave()),
        None => Err("Kein badhub-Turnier geladen — bitte im Setup koppeln.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schluessel_form_und_vergleich() {
        let s = neuer_schluessel().unwrap();
        assert_eq!(s.len(), 32);
        let anzeige = schluessel_anzeige(&s);
        assert_eq!(anzeige.matches('-').count(), 7);
        assert_eq!(
            schluessel_normal(&anzeige.to_uppercase()).as_deref(),
            Some(s.as_str())
        );
        assert!(schluessel_normal("zu-kurz").is_none());
        assert!(gleich_konstant(b"abc", b"abc"));
        assert!(!gleich_konstant(b"abc", b"abd"));
        assert!(!gleich_konstant(b"abc", b"ab"));
    }
}
