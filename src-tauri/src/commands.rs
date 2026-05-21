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
use crate::config::{AppConfig, ConnectionMode};
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
    /// Handle des laufenden Tablet-Servers (LAN-Modus), falls aktiv.
    pub tablet_server: Mutex<Option<JoinHandle<()>>>,
    /// Handle des laufenden Relay-Clients (Cloud-Modus), falls aktiv.
    pub relay_task: Mutex<Option<JoinHandle<()>>>,
    /// Handle des Diagnose-Log-Uploads, falls aktiv.
    pub log_task: Mutex<Option<JoinHandle<()>>>,
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
    if config.connection_mode == ConnectionMode::Cloud && config.install_id.is_empty() {
        return Err("Für den Cloud-Modus fehlt die Installations-ID.".to_string());
    }

    // Vor dem Move von `config` in den Tablet-Kontext merken.
    let upload_logs = config.upload_logs;
    let install_id = config.install_id.clone();
    let mode = config.connection_mode;

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

    // Geteilter Tablet-Kontext – je nach Modus betreibt ihn der
    // eingebettete Server (LAN) oder der Relay-Client (Cloud).
    let ctx = Arc::new(crate::tablet::server::ServerCtx::new(
        tablet,
        config,
        push::build_client(),
    ));
    match mode {
        ConnectionMode::Lan => {
            let mut server_slot = state
                .tablet_server
                .lock()
                .expect("Tablet-Server-Mutex nicht vergiftet");
            if server_slot.is_none() {
                let ctx = ctx.clone();
                *server_slot = Some(tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::tablet::server::run(ctx).await {
                        tracing::error!("Tablet-Server beendet: {e}");
                    }
                }));
            }
        }
        ConnectionMode::Cloud => {
            let mut relay_slot = state
                .relay_task
                .lock()
                .expect("Relay-Task-Mutex nicht vergiftet");
            if relay_slot.is_none() {
                let ctx = ctx.clone();
                *relay_slot = Some(tauri::async_runtime::spawn(
                    crate::tablet::relay_client::run(ctx, install_id.clone()),
                ));
            }
        }
    }

    // Optionaler Diagnose-Log-Upload (nur wenn vom Nutzer aktiviert).
    if upload_logs {
        let mut log_slot = state
            .log_task
            .lock()
            .expect("Log-Task-Mutex nicht vergiftet");
        if log_slot.is_none() {
            if let Ok(log_dir) = app.path().app_log_dir() {
                *log_slot = Some(tauri::async_runtime::spawn(crate::log_upload::upload_loop(
                    push::build_client(),
                    log_dir,
                    install_id,
                )));
            }
        }
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
    if let Some(handle) = state
        .relay_task
        .lock()
        .expect("Relay-Task-Mutex nicht vergiftet")
        .take()
    {
        handle.abort();
    }
    if let Some(handle) = state
        .log_task
        .lock()
        .expect("Log-Task-Mutex nicht vergiftet")
        .take()
    {
        handle.abort();
    }
    *state.status.lock().expect("Status-Mutex nicht vergiftet") = SyncStatus::default();
}

/// Server-Adresse + Felder-Übersicht für die Tablet-Seite der Oberfläche.
#[derive(Serialize)]
pub struct TabletInfo {
    /// LAN-Adresse `<ip>:<port>` des Tablet-Servers (nur LAN-Modus relevant).
    pub server_host: String,
    /// Verbindungsart: `"lan"` oder `"cloud"`.
    pub mode: String,
    /// Im Cloud-Modus die öffentliche Relay-Basis-URL
    /// (`https://badhub.de/bts-relay/<install_id>`), sonst leer.
    pub relay_base: String,
    /// Alle Courts mit aktuellem Match, Live-Stand und Tablet-Status.
    pub courts: Vec<crate::tablet::state::CourtOverview>,
}

/// Liefert Verbindungsart, Tablet-Adressen-Basis und die Felder-Übersicht.
#[tauri::command]
pub fn tablet_overview(state: State<'_, AppState>) -> TabletInfo {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    let (mode, relay_base) = match config.connection_mode {
        ConnectionMode::Lan => ("lan".to_string(), String::new()),
        ConnectionMode::Cloud => (
            "cloud".to_string(),
            format!("https://badhub.de/bts-relay/{}", config.install_id),
        ),
    };
    TabletInfo {
        server_host: crate::tablet::server::lan_host(),
        mode,
        relay_base,
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

// ───────────────────────────── Walkover nach Aufgabe ──────────────────────

/// Ein Walkover-Vorschlag samt der aktuell noch offenen Kandidaten-Spiele.
#[derive(Serialize)]
pub struct WalkoverProposalView {
    pub id: String,
    pub retired_team: String,
    pub draw_name: String,
    pub created_at_ms: u64,
    pub candidates: Vec<crate::tablet::state::WalkoverCandidate>,
}

/// Liefert die offenen Walkover-Vorschläge. Vorschläge, deren Spiele
/// inzwischen alle gewertet wurden, werden dabei aufgeräumt.
#[tauri::command]
pub fn walkover_proposals(state: State<'_, AppState>) -> Vec<WalkoverProposalView> {
    let mut views = Vec::new();
    for p in state.tablet.walkover_proposals() {
        let candidates = state.tablet.walkover_candidates(p.entry_id);
        if candidates.is_empty() {
            state.tablet.remove_walkover_proposal(&p.id);
            continue;
        }
        views.push(WalkoverProposalView {
            id: p.id,
            retired_team: p.retired_team,
            draw_name: p.draw_name,
            created_at_ms: p.created_at_ms,
            candidates,
        });
    }
    views
}

/// Verwirft einen Walkover-Vorschlag, ohne ihn umzusetzen.
#[tauri::command]
pub fn dismiss_walkover(state: State<'_, AppState>, proposal_id: String) {
    state.tablet.remove_walkover_proposal(&proposal_id);
}

/// Ergebnis einer Walkover-Bestätigung.
#[derive(Serialize)]
pub struct WalkoverResult {
    /// Anzahl erfolgreich nach BTP geschriebener kampfloser Wertungen.
    pub written: i64,
    /// Fehlermeldungen der nicht geschriebenen Spiele.
    pub errors: Vec<String>,
}

/// Schreibt für die ausgewählten Spiele einen kampflosen Sieg (Walkover)
/// nach BTP: die aufgebende Mannschaft verliert, der Gegner gewinnt
/// (`ScoreStatus = 1`, keine Sätze). Der Vorschlag wird nur entfernt, wenn
/// alle Spiele geschrieben wurden – sonst bleibt er für einen erneuten
/// Versuch stehen (bereits gewertete Spiele fallen von selbst heraus).
#[tauri::command]
pub async fn confirm_walkover(
    state: State<'_, AppState>,
    proposal_id: String,
    match_ids: Vec<i64>,
) -> Result<WalkoverResult, String> {
    // Ohne Auswahl nichts tun – insbesondere den Vorschlag nicht entfernen.
    if match_ids.is_empty() {
        return Ok(WalkoverResult {
            written: 0,
            errors: Vec::new(),
        });
    }
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    let tablet = state.tablet.clone();

    let proposal = tablet
        .walkover_proposals()
        .into_iter()
        .find(|p| p.id == proposal_id)
        .ok_or("Der Walkover-Vorschlag ist nicht mehr vorhanden.")?;
    let candidates = tablet.walkover_candidates(proposal.entry_id);

    let mut written = 0i64;
    let mut errors = Vec::new();
    for cand in candidates.iter().filter(|c| match_ids.contains(&c.match_id)) {
        let update = crate::btp::proto::MatchUpdate {
            btp_match_id: cand.match_id,
            draw_id: cand.draw_id,
            planning_id: cand.planning_id,
            sets: Vec::new(),
            // Sieger ist die jeweils NICHT aufgebende Mannschaft.
            team1_won: !cand.retired_is_team1,
            duration_mins: 0,
            score_status: 1, // 1 = Walkover
        };
        match crate::tablet::server::write_result_to_btp(&config, &update).await {
            Ok(()) => written += 1,
            Err(e) => errors.push(format!("{}: {e}", cand.round_name)),
        }
    }
    if errors.is_empty() {
        tablet.remove_walkover_proposal(&proposal_id);
    }
    Ok(WalkoverResult { written, errors })
}
