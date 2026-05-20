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
    /// Push-Endpunkt (`live_update.php`).
    pub url: String,
    /// Bearer-Token aus dem Badhub-Liveticker-Admin.
    pub password: String,
    /// Öffentliche Live-Seite, z. B. `https://badhub.de/live?t=bvbb`.
    /// Leer, wenn nicht hinterlegt. `#[serde(default)]` hält ältere
    /// Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub live_url: String,
}

impl Default for BadhubConfig {
    fn default() -> Self {
        Self {
            url: "https://badhub.de/api/live_update.php".to_string(),
            password: String::new(),
            live_url: String::new(),
        }
    }
}

/// Gesamte App-Konfiguration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub btp: BtpConfig,
    pub badhub: BadhubConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Konfiguration konnte nicht gelesen werden: {0}")]
    Read(std::io::Error),
    #[error("Konfiguration konnte nicht geschrieben werden: {0}")]
    Write(std::io::Error),
    #[error("Konfiguration ist beschädigt: {0}")]
    Parse(#[from] serde_json::Error),
}

impl AppConfig {
    /// Lädt die Konfiguration aus einer JSON-Datei. Fehlt die Datei, wird
    /// die Default-Konfiguration zurückgegeben (erster Start).
    pub fn load_from(path: &std::path::Path) -> Result<AppConfig, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(json) => Ok(serde_json::from_str(&json)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
            Err(e) => Err(ConfigError::Read(e)),
        }
    }

    /// Schreibt die Konfiguration als JSON. Fehlende Verzeichnisse werden
    /// angelegt.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(ConfigError::Write)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).map_err(ConfigError::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_default_config() {
        let path = std::env::temp_dir().join("bts-light-does-not-exist-xyz.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(AppConfig::load_from(&path).unwrap(), AppConfig::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");

        let config = AppConfig {
            btp: BtpConfig {
                host: "192.168.1.50".to_string(),
                port: 9901,
                password: Some("geheim".to_string()),
            },
            badhub: BadhubConfig {
                url: "https://badhub.de/api/live_update.php".to_string(),
                password: "token123".to_string(),
                live_url: "https://badhub.de/live?t=test".to_string(),
            },
        };
        config.save_to(&path).unwrap();
        assert_eq!(AppConfig::load_from(&path).unwrap(), config);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ kaputt").unwrap();
        assert!(matches!(
            AppConfig::load_from(&path),
            Err(ConfigError::Parse(_))
        ));
    }
}
