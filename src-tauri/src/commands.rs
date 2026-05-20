//! Tauri-Commands – die Brücke zwischen der WebView-Oberfläche und dem
//! Rust-Kern. Enthält außerdem die Hintergrund-Polling-Schleife.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager, State};

use crate::badhub::push;
use crate::btp::client;
use crate::config::AppConfig;
use crate::sync::{SyncEngine, SyncOutcome};

/// Abstand zwischen zwei Poll-Push-Zyklen.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Status der Sync-Schleife, wie ihn das Dashboard anzeigt.
#[derive(Clone, Serialize)]
pub struct SyncStatus {
    /// Läuft die Hintergrund-Schleife?
    pub running: bool,
    /// Grobkategorie: `idle` | `ok` | `btp_error` | `push_error`.
    pub kind: String,
    /// Menschenlesbare Meldung.
    pub message: String,
    /// Zeitpunkt des letzten Zyklus (Unix-Millisekunden).
    pub updated_at_ms: u64,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            running: false,
            kind: "idle".to_string(),
            message: "Nicht verbunden".to_string(),
            updated_at_ms: 0,
        }
    }
}

/// Geteilter App-Zustand, von Tauri verwaltet.
#[derive(Default)]
pub struct AppState {
    /// Zuletzt geladene bzw. gespeicherte Konfiguration.
    pub config: Mutex<AppConfig>,
    /// Aktueller Status der Sync-Schleife.
    pub status: Mutex<SyncStatus>,
    /// Handle der laufenden Polling-Schleife, falls aktiv.
    pub sync_task: Mutex<Option<JoinHandle<()>>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Übersetzt das Ergebnis eines Sync-Zyklus in einen Anzeige-Status.
fn status_from(outcome: &SyncOutcome) -> SyncStatus {
    let (kind, message) = match outcome {
        SyncOutcome::PushedFull => ("ok", "Verbunden – kompletter Stand gesendet".to_string()),
        SyncOutcome::PushedUpdate => ("ok", "Verbunden – Punktestand aktualisiert".to_string()),
        SyncOutcome::Idle => ("ok", "Verbunden – keine Änderung".to_string()),
        SyncOutcome::BtpError(e) => ("btp_error", format!("BTP nicht erreichbar: {e}")),
        SyncOutcome::PushError(e) => ("push_error", format!("Push fehlgeschlagen: {e}")),
    };
    SyncStatus {
        running: true,
        kind: kind.to_string(),
        message,
        updated_at_ms: now_ms(),
    }
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

/// Startet die Hintergrund-Polling-Schleife (BTP → Badhub, alle 5 s).
#[tauri::command]
pub fn start_sync(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut slot = state.sync_task.lock().expect("Task-Mutex nicht vergiftet");
    if slot.is_some() {
        return Ok(()); // läuft bereits
    }

    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    if config.badhub.password.is_empty() {
        return Err("Es ist kein Badhub-Passwort konfiguriert.".to_string());
    }

    let app_handle = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let http = push::build_client();
        let mut engine = SyncEngine::new();
        loop {
            let outcome = engine.run_once(&config, &http).await;
            let mut status = status_from(&outcome);
            status.running = true;
            *app_handle
                .state::<AppState>()
                .status
                .lock()
                .expect("Status-Mutex nicht vergiftet") = status;
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });

    *slot = Some(handle);
    *state.status.lock().expect("Status-Mutex nicht vergiftet") = SyncStatus {
        running: true,
        kind: "idle".to_string(),
        message: "Verbindung wird aufgebaut …".to_string(),
        updated_at_ms: now_ms(),
    };
    Ok(())
}

/// Stoppt die Hintergrund-Polling-Schleife.
#[tauri::command]
pub fn stop_sync(state: State<'_, AppState>) {
    if let Some(handle) = state
        .sync_task
        .lock()
        .expect("Task-Mutex nicht vergiftet")
        .take()
    {
        handle.abort();
    }
    *state.status.lock().expect("Status-Mutex nicht vergiftet") = SyncStatus::default();
}

/// Liefert den aktuellen Sync-Status für das Dashboard.
#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> SyncStatus {
    state
        .status
        .lock()
        .expect("Status-Mutex nicht vergiftet")
        .clone()
}
