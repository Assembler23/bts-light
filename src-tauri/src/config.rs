//! Verbindungs-Konfiguration der App: BTP-Quelle und Badhub-Ziel.

use serde::{Deserialize, Serialize};

/// Verbindungsdaten für das lokale BTP (TP-Network).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BtpConfig {
    pub host: String,
    pub port: u16,
    /// TP-Network-Passwort, falls in BTP gesetzt.
    pub password: Option<String>,
}

impl Default for BtpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9901,
            password: None,
        }
    }
}

/// Verbindungsdaten für den Badhub-Liveticker-Endpunkt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadhubConfig {
    pub url: String,
    /// Bearer-Token aus dem Badhub-Liveticker-Admin.
    pub password: String,
}

impl Default for BadhubConfig {
    fn default() -> Self {
        Self {
            url: "https://badhub.de/api/live_update.php".to_string(),
            password: String::new(),
        }
    }
}

/// Gesamte App-Konfiguration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub btp: BtpConfig,
    pub badhub: BadhubConfig,
}
