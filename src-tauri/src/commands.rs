//! Tauri-Commands – die Brücke zwischen der WebView-Oberfläche und dem
//! Rust-Kern. Enthält außerdem die Hintergrund-Polling-Schleife.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::badhub::push;
use crate::btp::client;
use crate::config::AppConfig;
use crate::sync::{SyncEngine, SyncOutcome};
use crate::tablet::state::TabletState;

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
    /// Geteilter Zustand zwischen Sync-Loop und Tablet-Server.
    pub tablet: Arc<TabletState>,
    /// Handle des laufenden Tablet-Servers, falls aktiv.
    pub tablet_server: Mutex<Option<JoinHandle<()>>>,
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

    let tablet = state.tablet.clone();

    // Poll-Push-Schleife BTP → Badhub.
    let app_handle = app.clone();
    let sync_config = config.clone();
    let sync_tablet = tablet.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let http = push::build_client();
        let mut engine = SyncEngine::new();
        loop {
            let outcome = engine.run_once(&sync_config, &http, &sync_tablet).await;
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
    drop(slot); // sync_task-Lock freigeben, bevor tablet_server gelockt wird

    // Eingebetteter Tablet-Server (Spielzettel) – läuft mit der Sync-Schleife.
    let mut server_slot = state
        .tablet_server
        .lock()
        .expect("Tablet-Server-Mutex nicht vergiftet");
    if server_slot.is_none() {
        let ctx = Arc::new(crate::tablet::server::ServerCtx::new(
            tablet,
            config,
            push::build_client(),
        ));
        let server_handle = tauri::async_runtime::spawn(async move {
            if let Err(e) = crate::tablet::server::run(ctx).await {
                tracing::error!("Tablet-Server beendet: {e}");
            }
        });
        *server_slot = Some(server_handle);
    }
    *state.status.lock().expect("Status-Mutex nicht vergiftet") = SyncStatus {
        running: true,
        kind: "idle".to_string(),
        message: "Verbindung wird aufgebaut …".to_string(),
        updated_at_ms: now_ms(),
    };
    Ok(())
}

/// Stoppt die Hintergrund-Polling-Schleife und den Tablet-Server.
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
    if let Some(handle) = state
        .tablet_server
        .lock()
        .expect("Tablet-Server-Mutex nicht vergiftet")
        .take()
    {
        handle.abort();
    }
    *state.status.lock().expect("Status-Mutex nicht vergiftet") = SyncStatus::default();
}

/// Server-Adresse + Felder-Übersicht für die Tablet-Seite der Oberfläche.
#[derive(Serialize)]
pub struct TabletInfo {
    /// LAN-Adresse `<ip>:<port>` des Tablet-Servers.
    pub server_host: String,
    /// Alle Courts mit aktuellem Match, Live-Stand und Tablet-Status.
    pub courts: Vec<crate::tablet::state::CourtOverview>,
}

/// Liefert die Felder-Übersicht für die Turnierleitung.
#[tauri::command]
pub fn tablet_overview(state: State<'_, AppState>) -> TabletInfo {
    TabletInfo {
        server_host: crate::tablet::server::lan_host(),
        courts: state.tablet.overview(),
    }
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

/// Öffnet die öffentliche Live-Seite im Standard-Browser.
///
/// `display` wählt die Ansicht: `None` = Liveticker, `Some("monitor")` =
/// Hallen-Monitor, `Some("next")` = Aufruf-Anzeige.
#[tauri::command]
pub fn open_live_view(
    app: AppHandle,
    state: State<'_, AppState>,
    display: Option<String>,
) -> Result<(), String> {
    let live_url = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .badhub
        .live_url
        .clone();
    if live_url.is_empty() {
        return Err("Für dieses Turnier ist keine Live-Seite hinterlegt.".to_string());
    }
    let url = match display {
        Some(view) => format!("{live_url}&display={view}"),
        None => live_url,
    };
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|e| e.to_string())
}
