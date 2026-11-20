//! Turnierquelle: woher bts-light den Turnierstand liest und wohin es
//! schreibt.
//!
//! Heute gibt es nur BTP. Für Turniere, die in badhub organisiert werden,
//! kommt der badhub-Turnierkern als zweite Quelle dazu (badhub
//! `docs/features/bts_light_steuerung.md`, ADR-0084). **BTP-Turniere laufen
//! dabei unverändert** — dieses Modul bündelt die Zugriffe nur an einer
//! Stelle, es ändert keinen BTP-Ablauf.

pub mod btp;

use crate::btp::client;
use crate::btp::model::BtpSnapshot;
use crate::config::{AppConfig, QuelleArt};

/// Liest den aktuellen Turnierstand aus der eingestellten Quelle.
///
/// Für BTP genau der bisherige Abruf (`client::fetch_snapshot` mit Host,
/// Port und Passwort aus `config.btp`), samt Fehlertext.
pub async fn stand(config: &AppConfig) -> Result<BtpSnapshot, String> {
    match config.quelle {
        QuelleArt::Btp => client::fetch_snapshot(
            &config.btp.host,
            config.btp.port,
            config.btp.password.as_deref(),
        )
        .await
        .map_err(|e| e.to_string()),
    }
}
