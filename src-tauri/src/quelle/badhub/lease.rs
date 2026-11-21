//! Lease eines badhub-Turniers (Paket L3, badhub ADR-0084): das Schreibrecht
//! dieses PCs für alle Durchführungs-Befehle, mit **Epoche** als
//! Fencing-Token.
//!
//! Gespeichert unter `turniere/<uuid>/`:
//!
//! ```text
//! lease.json   Epoche, bestätigtes n, Zustand, Relay-Namespace, Zeiten
//! relay.bin    TL-Zugang, den bts-light für badhub ausgestellt hat (AES-256-GCM)
//! ```
//!
//! Die Lease hat kein Ablaufdatum: Ohne Netz beim Start gilt die gespeicherte
//! weiter (badhub-Spec „Offline-Szenarien“).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Dateiname der Lease im Turnierordner.
pub const DATEI: &str = "lease.json";
/// Dateiname des TL-Zugangs für badhub (verschlüsselt).
pub const ZUGANG_DATEI: &str = "relay.bin";

/// Wie es um die Lease steht.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseZustand {
    /// Dieser PC hält die Lease: Eingaben werden angenommen.
    #[default]
    Aktiv,
    /// badhub hat die Steuerung zurückgeholt (`epoche_veraltet`): Outbox
    /// steht, Eingaben gesperrt.
    Entzogen,
    /// Die Turnierleitung hat die Steuerung an badhub zurückgegeben.
    Zurueckgegeben,
}

/// Inhalt von `lease.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    /// Epoche aus `POST turnier/lease`.
    pub epoche: u64,
    /// Zuletzt von badhub bestätigtes Kern-`n` (`bis_n`).
    pub bis_n: u64,
    pub zustand: LeaseZustand,
    /// Mit welchem Relay-Namespace die Lease genommen wurde (`None` = ohne
    /// Cloud-Relay). Ändert er sich, wird die Lease neu angemeldet.
    #[serde(default)]
    pub relay_ns: Option<String>,
    /// Seit wann (Unix-ms).
    #[serde(default)]
    pub seit_ms: u64,
    /// Letzte erfolgreiche Antwort von badhub (Unix-ms), für die Anzeige.
    #[serde(default)]
    pub letzter_kontakt_ms: u64,
    /// Standby-Übernahme ohne Netz (Paket S2): Dieser PC schreibt unter der
    /// Epoche des ausgefallenen Hauptrechners; der erste Upload geht mit
    /// `standby: true` und bringt die neue Epoche.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub standby: bool,
    /// Nach der Standby-Übernahme passt der eigene Stand nicht zu badhub
    /// (abgewiesene Posten oder badhub weiter): Stand neu laden, bis dahin
    /// keine Eingaben.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub neu_laden: bool,
    /// Posten, die badhub bei der Standby-Übernahme abgewiesen hat (Anzeige).
    #[serde(default, skip_serializing_if = "ist_null")]
    pub abgewiesen: u64,
}

fn ist_null(v: &u64) -> bool {
    *v == 0
}

impl Lease {
    /// Darf dieser PC gerade schreiben?
    pub fn aktiv(&self) -> bool {
        self.zustand == LeaseZustand::Aktiv
    }
}

/// Lädt `lease.json` (`None`, wenn noch keine Lease genommen wurde).
pub fn laden(dir: &Path) -> Option<Lease> {
    let text = std::fs::read(dir.join(DATEI)).ok()?;
    match serde_json::from_slice(&text) {
        Ok(l) => Some(l),
        Err(_) => {
            tracing::warn!("badhub-Turnier: lease.json unlesbar — gilt als ohne Lease");
            None
        }
    }
}

/// Schreibt `lease.json` (über eine temporäre Datei).
pub fn speichern(dir: &Path, lease: &Lease) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Turnierordner: {e}"))?;
    let text = serde_json::to_vec_pretty(lease).map_err(|e| e.to_string())?;
    super::schluessel::schreiben_privat(&dir.join(DATEI), &text)
}

/// Löscht `lease.json` und den TL-Zugang (Steuerung neu übernehmen).
pub fn entfernen(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(DATEI));
    let _ = std::fs::remove_file(dir.join(ZUGANG_DATEI));
}

/// Erzeugt einen neuen TL-Zugang für badhub: 32 Zufallsbytes als Hex.
pub fn neuer_zugang() -> Result<String, String> {
    use ring::rand::SecureRandom;
    let mut roh = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut roh)
        .map_err(|_| "Zufallsquelle nicht verfügbar".to_string())?;
    Ok(roh.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_rundreise_und_zugang() {
        let dir = tempfile::tempdir().unwrap();
        assert!(laden(dir.path()).is_none());
        let l = Lease {
            epoche: 3,
            bis_n: 41,
            zustand: LeaseZustand::Aktiv,
            relay_ns: Some("ns-1".into()),
            seit_ms: 1,
            letzter_kontakt_ms: 2,
            ..Default::default()
        };
        speichern(dir.path(), &l).unwrap();
        assert_eq!(laden(dir.path()), Some(l));
        let text = std::fs::read_to_string(dir.path().join(DATEI)).unwrap();
        assert!(text.contains("\"aktiv\""), "{text}");
        let a = neuer_zugang().unwrap();
        let b = neuer_zugang().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        entfernen(dir.path());
        assert!(laden(dir.path()).is_none());
    }
}
