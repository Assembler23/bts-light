//! Tauri-Commands – die Brücke zwischen der WebView-Oberfläche und dem
//! Rust-Kern.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use crate::btp::client;
use crate::config::AppConfig;

/// Geteilter App-Zustand, von Tauri verwaltet.
#[derive(Default)]
pub struct AppState {
    /// Zuletzt geladene bzw. gespeicherte Konfiguration.
    pub config: Mutex<AppConfig>,
}

/// Pfad zur Konfigurationsdatei im App-Config-Verzeichnis des Betriebssystems.
fn config_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .expect("App-Config-Verzeichnis ist verfügbar")
        .join("config.json")
}

/// Lädt die gespeicherte Konfiguration (oder Defaults beim ersten Start).
#[tauri::command]
pub fn load_config(app: AppHandle, state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = AppConfig::load_from(&config_path(&app)).map_err(|e| e.to_string())?;
    *state.config.lock().expect("Config-Mutex nicht vergiftet") = config.clone();
    Ok(config)
}

/// Speichert die Konfiguration dauerhaft.
#[tauri::command]
pub fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    config
        .save_to(&config_path(&app))
        .map_err(|e| e.to_string())?;
    *state.config.lock().expect("Config-Mutex nicht vergiftet") = config;
    Ok(())
}

/// Testet die Verbindung zu BTP und liefert bei Erfolg den Turniernamen.
#[tauri::command]
pub async fn test_btp(host: String, port: u16, password: Option<String>) -> Result<String, String> {
    let snapshot = client::fetch_snapshot(&host, port, password.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(snapshot.tournament_name)
}
