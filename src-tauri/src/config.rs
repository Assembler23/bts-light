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

/// Verbindungsart für die Schiedsrichter-Tablets.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    /// Eingebetteter Server im Hallen-LAN (schnell, offline – braucht aber
    /// einen offenen eingehenden Port 8088).
    #[default]
    Lan,
    /// Über den Cloud-Relay auf badhub.de – funktioniert auch hinter
    /// gesperrten Firmen-Firewalls (nur ausgehende Verbindungen).
    Cloud,
}

/// Sprachmodus der gesprochenen Feld-Ansagen.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnounceLanguageMode {
    /// Immer Deutsch ansagen.
    De,
    /// Immer Englisch ansagen.
    En,
    /// Automatisch: Englisch, wenn mindestens die Hälfte der Spieler auf
    /// dem Feld international ist (Nationalität gesetzt und ≠ `GER`).
    #[default]
    Auto,
}

/// Einstellungen für die gesprochene Ansage neu aufs Feld gezogener Spiele.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AnnounceConfig {
    /// Sind Ansagen aktiv?
    pub enabled: bool,
    /// Sprachmodus (Deutsch / Englisch / Automatisch).
    pub language_mode: AnnounceLanguageMode,
    /// Bevorzugte deutsche Stimme (`voiceURI`); leer = Browser-Standard.
    pub voice_de: String,
    /// Bevorzugte englische Stimme (`voiceURI`); leer = Browser-Standard.
    pub voice_en: String,
    /// Sprech-Geschwindigkeit (sinnvoll 0,5–1,5).
    pub rate: f64,
    /// Gong vor der Ansage abspielen?
    pub gong: bool,
}

impl Default for AnnounceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            language_mode: AnnounceLanguageMode::Auto,
            voice_de: String::new(),
            voice_en: String::new(),
            rate: 0.8,
            gong: true,
        }
    }
}

/// Einstellungen der Court-Monitor-Anzeige (TV am Spielfeld, Raspberry Pi).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CourtMonitorConfig {
    /// Ist die Court-Monitor-Anzeige eingerichtet/aktiv? Steuert nur die
    /// Sichtbarkeit der Monitor-Adressen in der Oberfläche – die
    /// Anzeige-Seite selbst ist immer erreichbar.
    pub enabled: bool,
    /// Wechsel-Intervall der Werbebilder im Leerlauf (Sekunden).
    pub ad_interval_s: i64,
    /// Disziplin in der Kopfzeile anzeigen?
    pub show_discipline: bool,
    /// Runde in der Fußzeile anzeigen?
    pub show_round: bool,
    /// Spielnummer in der Fußzeile anzeigen?
    pub show_match_number: bool,
    /// Pausen-Countdown (Retro-Klappanzeige) anzeigen?
    pub show_timer: bool,
    /// Spieldauer (Minuten, mit Stoppuhr-Symbol) in der Kopfzeile anzeigen?
    pub show_match_clock: bool,
    /// Werbung im Leerlauf anzeigen? Aus → leeres Feld zeigt die neutrale
    /// Leerlauf-Seite statt der Werbebilder.
    pub show_ads: bool,
    /// Anzeige-Layout des Monitors (`split` = „A — Geteilt"). Vorbereitet
    /// für weitere Layouts.
    pub layout: String,
}

impl Default for CourtMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ad_interval_s: 10,
            show_discipline: true,
            show_round: true,
            show_match_number: true,
            show_timer: true,
            show_match_clock: true,
            show_ads: true,
            layout: "split".to_string(),
        }
    }
}

/// Gesamte App-Konfiguration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub btp: BtpConfig,
    pub badhub: BadhubConfig,
    /// Opt-in: Diagnose-Logs automatisch an badhub.de hochladen, damit
    /// Fehler über alle Installationen hinweg auswertbar sind.
    #[serde(default)]
    pub upload_logs: bool,
    /// Zufällige, dauerhafte Installations-ID (vom Frontend erzeugt) –
    /// ordnet hochgeladene Logs einer Installation zu und ist zugleich der
    /// Namespace im Cloud-Relay.
    #[serde(default)]
    pub install_id: String,
    /// Verbindungsart für die Tablets (LAN oder Cloud). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub connection_mode: ConnectionMode,
    /// Einstellungen der gesprochenen Feld-Ansagen. `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub announce: AnnounceConfig,
    /// Einstellungen der Court-Monitor-Anzeige. `#[serde(default)]` hält
    /// ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub court_monitor: CourtMonitorConfig,
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
            upload_logs: true,
            install_id: "inst-abc123".to_string(),
            connection_mode: ConnectionMode::Cloud,
            announce: AnnounceConfig {
                enabled: true,
                language_mode: AnnounceLanguageMode::En,
                voice_de: "voice-de-1".to_string(),
                voice_en: "voice-en-1".to_string(),
                rate: 1.1,
                gong: false,
            },
            court_monitor: CourtMonitorConfig {
                enabled: true,
                ad_interval_s: 8,
                show_discipline: false,
                show_round: true,
                show_match_number: false,
                show_timer: true,
                show_match_clock: false,
                show_ads: false,
                layout: "split".to_string(),
            },
        };
        config.save_to(&path).unwrap();
        assert_eq!(AppConfig::load_from(&path).unwrap(), config);
    }

    #[test]
    fn config_without_announce_key_loads_with_defaults() {
        // Ältere config.json kennt den announce-Block nicht – er muss mit
        // den Default-Werten geladen werden, statt das Laden scheitern zu
        // lassen.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.announce, AnnounceConfig::default());
        assert!(!loaded.announce.enabled);
        assert_eq!(loaded.announce.rate, 0.8);
        assert!(loaded.announce.gong);
        // Ebenso der court_monitor-Block – ältere config.json kennt ihn nicht.
        assert_eq!(loaded.court_monitor, CourtMonitorConfig::default());
        assert!(!loaded.court_monitor.enabled);
        assert_eq!(loaded.court_monitor.ad_interval_s, 10);
        assert!(loaded.court_monitor.show_timer);
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
