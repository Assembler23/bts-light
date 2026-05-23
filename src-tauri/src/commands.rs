//! Tauri-Commands – die Brücke zwischen der WebView-Oberfläche und dem
//! Rust-Kern. Enthält außerdem die Hintergrund-Polling-Schleife.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use relay_proto::{MonitorCommandKind, MonitorDeviceInfo};

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
    /// Laufende mDNS-Bekanntgabe (`bts-light.local`, nur LAN-Modus).
    pub mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
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

/// Verzeichnis der hochgeladenen Court-Monitor-Werbebilder im
/// App-Datenverzeichnis des Betriebssystems.
fn monitor_ad_dir(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join(crate::tablet::monitor::AD_DIR_NAME)
}

/// Pfad zur Datei mit den Monitor-Feld-Zuweisungen (Gerät → Feld).
fn monitor_assignments_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .expect("App-Config-Verzeichnis ist verfügbar")
        .join(crate::tablet::monitor::MONITOR_ASSIGN_FILE)
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
    if config.connection_mode.cloud_enabled() && config.install_id.is_empty() {
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
    let monitor_dir = monitor_ad_dir(&app);
    let _ = std::fs::create_dir_all(&monitor_dir);
    let cfg_path = config_path(&app);
    let assignments_path = monitor_assignments_path(&app);
    let ctx = Arc::new(crate::tablet::server::ServerCtx::new(
        tablet,
        config,
        push::build_client(),
        monitor_dir,
        cfg_path,
        assignments_path,
    ));
    // LAN und Cloud sind unabhängig voneinander schaltbar – im
    // Doppelmodus (`LanAndCloud`) laufen beide Wege für dieselbe
    // Turnierinstanz parallel. `lan_enabled()`/`cloud_enabled()` liefern
    // für die reinen Modi exakt dieselbe Wahl wie zuvor das `match`.
    if mode.lan_enabled() {
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
        drop(server_slot);
        // mDNS-Bekanntgabe (`bts-light.local`) – damit Tablets und
        // Monitore den PC ohne feste IP finden. Fehler ist unkritisch.
        let mut mdns_slot = state.mdns.lock().expect("mDNS-Mutex nicht vergiftet");
        if mdns_slot.is_none() {
            match crate::tablet::mdns::advertise() {
                Ok(daemon) => *mdns_slot = Some(daemon),
                Err(e) => tracing::warn!("mDNS-Bekanntgabe fehlgeschlagen: {e}"),
            }
        }
    }
    if mode.cloud_enabled() {
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
    if let Some(daemon) = state
        .mdns
        .lock()
        .expect("mDNS-Mutex nicht vergiftet")
        .take()
    {
        let _ = daemon.shutdown();
    }
    *state.status.lock().expect("Status-Mutex nicht vergiftet") = SyncStatus::default();
}

/// Server-Adresse + Felder-Übersicht für die Tablet-Seite der Oberfläche.
#[derive(Serialize)]
pub struct TabletInfo {
    /// LAN-Adresse `<ip>:<port>` des Tablet-Servers – gesetzt, sobald der
    /// LAN-Pfad aktiv ist (`Lan` oder `LanAndCloud`), sonst leer.
    pub server_host: String,
    /// Verbindungsart: `"lan"`, `"cloud"` oder `"lan+cloud"`.
    pub mode: String,
    /// Öffentliche Relay-Basis-URL (`https://badhub.de/bts-relay/<install_id>`)
    /// – gesetzt, sobald der Cloud-Pfad aktiv ist (`Cloud` oder
    /// `LanAndCloud`), sonst leer.
    pub relay_base: String,
    /// Ist der LAN-Pfad aktiv? Im Doppelmodus zeigt die Oberfläche LAN- und
    /// Cloud-Adresse parallel.
    pub lan_enabled: bool,
    /// Ist der Cloud-Pfad aktiv?
    pub cloud_enabled: bool,
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
    let lan_enabled = config.connection_mode.lan_enabled();
    let cloud_enabled = config.connection_mode.cloud_enabled();
    // LAN- und Cloud-Adresse werden unabhängig befüllt: im Doppelmodus
    // sind beide gesetzt, im reinen Modus genau eine – wie bisher.
    let server_host = if lan_enabled {
        crate::tablet::server::lan_host()
    } else {
        String::new()
    };
    let relay_base = if cloud_enabled {
        format!("https://badhub.de/bts-relay/{}", config.install_id)
    } else {
        String::new()
    };
    let mode = match config.connection_mode {
        ConnectionMode::Lan => "lan",
        ConnectionMode::Cloud => "cloud",
        ConnectionMode::LanAndCloud => "lan+cloud",
    }
    .to_string();
    TabletInfo {
        server_host,
        mode,
        relay_base,
        lan_enabled,
        cloud_enabled,
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

/// Öffnet eine externe `https`-URL im Standardbrowser – für die
/// Mitwirkenden-Links im Über-Dialog. Erlaubt nur saubere `https`-URLs
/// (Schema-Prefix, keine Steuerzeichen/Leerzeichen), damit kein
/// präparierter String an die OS-Shell durchgereicht wird.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    let is_clean_https =
        url.starts_with("https://") && !url.contains(|c: char| c.is_control() || c == ' ');
    if !is_clean_https {
        return Err("Nur reguläre https-Links sind erlaubt.".to_string());
    }
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
    for cand in candidates
        .iter()
        .filter(|c| match_ids.contains(&c.match_id))
    {
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

// ───────────────────────────── Spiele in Vorbereitung ─────────────────────

/// Daten zu einem bereits ausgesprochenen „in Vorbereitung"-Aufruf.
#[derive(Serialize)]
pub struct PreparationCallInfo {
    /// LocationID der Halle, für die gerufen wurde; `null` bei einem
    /// hallenunabhängigen Aufruf (Ein-Hallen-Turnier).
    pub location_id: Option<i64>,
    /// Aufgelöster Hallenname; leer, wenn ohne Halle gerufen wurde.
    pub hall: String,
    /// Zeitpunkt des Aufrufs (Unix-Millisekunden).
    pub called_at_ms: u64,
}

/// Ein eingeplantes Spiel, das „in Vorbereitung" gerufen werden kann –
/// für die Frontend-Liste auf dem „In Vorbereitung"-Tab.
#[derive(Serialize)]
pub struct PreparationCandidate {
    /// BTP-Match-ID.
    pub match_id: i64,
    /// Anzeigename, z. B. "HE G1".
    pub label: String,
    /// Disziplin als snake_case-Schlüssel (`mens_singles`, `mixed`, …;
    /// leer = unbekannt) – das Frontend lokalisiert für die Ansage selbst.
    pub discipline: String,
    /// Spieler-Namen Team 1 (1 bei Einzel, 2 bei Doppel).
    pub team1: Vec<String>,
    /// Spieler-Namen Team 2.
    pub team2: Vec<String>,
    /// Nationalitäten Team 1, parallel zu `team1` (leerer String, wenn
    /// unbekannt) – Grundlage der automatischen DE/EN-Sprachwahl.
    pub team1_nationalities: Vec<String>,
    /// Nationalitäten Team 2, parallel zu `team2`.
    pub team2_nationalities: Vec<String>,
    /// Spielnummer (BTP `MatchNr`), falls vergeben.
    pub match_num: Option<i64>,
    /// Aufruf-Daten, falls das Match bereits gerufen wurde; sonst `null`.
    pub call: Option<PreparationCallInfo>,
}

/// Rückgabe von [`preparation_candidates`]: die Kandidaten-Spiele und die
/// Hallen des Turniers (für das hallenweise Aufrufen im Frontend).
#[derive(Serialize)]
pub struct PreparationView {
    /// Eingeplante, ruf-bare Spiele – gerufene zuerst, dann nach Spielnummer.
    pub candidates: Vec<PreparationCandidate>,
    /// Hallen des Turniers (BTP `Locations`). Ab zwei Einträgen blendet das
    /// Frontend die Hallen-Auswahl ein.
    pub locations: Vec<PreparationLocation>,
}

/// Eine Halle des Turniers für die Frontend-Auswahl.
#[derive(Serialize)]
pub struct PreparationLocation {
    pub id: i64,
    pub name: String,
}

/// Liefert die ruf-baren Spiele und die Hallen des Turniers. Kandidaten
/// sind alle eingeplanten Matches mit zwei feststehenden Mannschaften;
/// bereits gerufene stehen vorn, danach nach Spielnummer (ohne Nummer
/// zuletzt). Reiner Lesepfad – nicht mehr ruf-bare Matches erscheinen
/// einfach nicht in der Liste, ihre Aufrufe räumt der Sync-Lauf
/// (`apply_preparation_calls` in `run_once`) auf.
#[tauri::command]
pub fn preparation_candidates(state: State<'_, AppState>) -> PreparationView {
    let tablet = &state.tablet;
    let Some(snapshot) = tablet.snapshot_clone() else {
        return PreparationView {
            candidates: Vec::new(),
            locations: Vec::new(),
        };
    };
    let calls = tablet.preparation_calls();

    let mut candidates: Vec<PreparationCandidate> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::Scheduled)
        // Nur echte Paarungen – beide Mannschaften müssen feststehen.
        .filter(|m| !m.team1.is_empty() && !m.team2.is_empty())
        .map(|m| {
            let call = calls.iter().find(|c| c.match_id == m.id).map(|c| {
                let hall = c.location_id.and_then(|lid| {
                    snapshot
                        .locations
                        .iter()
                        .find(|l| l.id == lid)
                        .map(|l| l.name.clone())
                });
                PreparationCallInfo {
                    location_id: c.location_id,
                    hall: hall.unwrap_or_default(),
                    called_at_ms: c.called_at_ms,
                }
            });
            PreparationCandidate {
                match_id: m.id,
                label: format!("{} {}", m.draw_name, m.round_name)
                    .trim()
                    .to_string(),
                discipline: m.discipline.as_str().to_string(),
                team1: m.team1.iter().map(|p| p.name.clone()).collect(),
                team2: m.team2.iter().map(|p| p.name.clone()).collect(),
                team1_nationalities: m
                    .team1
                    .iter()
                    .map(|p| p.nationality.clone().unwrap_or_default())
                    .collect(),
                team2_nationalities: m
                    .team2
                    .iter()
                    .map(|p| p.nationality.clone().unwrap_or_default())
                    .collect(),
                match_num: m.match_num,
                call,
            }
        })
        .collect();
    // Gerufene zuerst, danach nach Spielnummer (ohne Nummer zuletzt).
    candidates.sort_by_key(|c| {
        (
            c.call.is_none(),
            c.match_num.unwrap_or(i64::MAX),
            c.match_id,
        )
    });

    let locations = snapshot
        .locations
        .iter()
        .map(|l| PreparationLocation {
            id: l.id,
            name: l.name.clone(),
        })
        .collect();

    PreparationView {
        candidates,
        locations,
    }
}

/// Ruft die ausgewählten Spiele „in Vorbereitung". `location_id` bindet den
/// Aufruf an eine Halle (oder `None` bei einem hallenunabhängigen Aufruf).
#[tauri::command]
pub fn call_preparation(state: State<'_, AppState>, match_ids: Vec<i64>, location_id: Option<i64>) {
    let now = now_ms();
    for match_id in match_ids {
        state
            .tablet
            .add_preparation_call(crate::tablet::state::PreparationCall {
                match_id,
                location_id,
                called_at_ms: now,
            });
    }
}

/// Nimmt den „in Vorbereitung"-Aufruf eines Spiels zurück.
#[tauri::command]
pub fn retract_preparation(state: State<'_, AppState>, match_id: i64) {
    state.tablet.remove_preparation_call(match_id);
}

// ───────────────────────────── Court-Monitor-Werbung ──────────────────────

/// Obergrenze für ein einzelnes Werbebild (8 MB).
const MAX_AD_BYTES: u64 = 8 * 1024 * 1024;

/// Übernimmt ein im Datei-Dialog gewähltes Werbebild in das
/// `court-ads`-Verzeichnis. `path` ist der absolute Pfad der Quelldatei;
/// der Zielname wird mit Zeitstempel selbst vergeben (kein Pfad-Traversal
/// über den Originalnamen). Liefert den vergebenen Dateinamen zurück.
#[tauri::command]
pub fn add_court_ad(app: AppHandle, path: String) -> Result<String, String> {
    let src = std::path::PathBuf::from(&path);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["jpg", "jpeg", "png", "webp", "gif"].contains(&e.as_str()))
        .ok_or("Nur Bilddateien (JPG, PNG, WEBP, GIF) sind erlaubt.")?;
    let meta = std::fs::metadata(&src).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
    if !meta.is_file() {
        return Err("Die Auswahl ist keine Datei.".to_string());
    }
    if meta.len() > MAX_AD_BYTES {
        return Err("Das Bild ist größer als 8 MB.".to_string());
    }
    let dir = monitor_ad_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Verzeichnis fehlt: {e}"))?;
    let name = format!("ad-{}.{ext}", now_ms());
    std::fs::copy(&src, dir.join(&name)).map_err(|e| format!("Kopieren fehlgeschlagen: {e}"))?;
    tracing::info!("Court-Monitor: Werbebild '{name}' hinzugefügt");
    Ok(name)
}

/// Entfernt ein Werbebild aus dem `court-ads`-Verzeichnis.
#[tauri::command]
pub fn remove_court_ad(app: AppHandle, file: String) -> Result<(), String> {
    if !crate::tablet::monitor::is_safe_image_name(&file) {
        return Err("Ungültiger Dateiname.".to_string());
    }
    std::fs::remove_file(monitor_ad_dir(&app).join(&file))
        .map_err(|e| format!("Löschen fehlgeschlagen: {e}"))?;
    tracing::info!("Court-Monitor: Werbebild '{file}' entfernt");
    Ok(())
}

/// Listet die aktuell hinterlegten Werbebild-Dateinamen.
#[tauri::command]
pub fn list_court_ads(app: AppHandle) -> Vec<String> {
    crate::tablet::monitor::list_ads(&monitor_ad_dir(&app))
}

// ───────────────────────────── Court-Monitor-Geräte ───────────────────────

/// Liefert die Court-Monitor-Geräte für die Verwaltungsseite. Im LAN-Modus
/// lokal aus Zuweisungen + Live-Pollzeiten gebaut, im Cloud-Modus die vom
/// Relay gemeldete Liste, im Doppelmodus beide vereint.
#[tauri::command]
pub fn monitor_devices(app: AppHandle, state: State<'_, AppState>) -> Vec<MonitorDeviceInfo> {
    let mode = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .connection_mode;
    // LAN-Liste: lokal aus Feld-Zuweisungen + Live-Pollzeiten.
    let lan_devices = || {
        let assignments = crate::tablet::monitor::read_assignments(&monitor_assignments_path(&app));
        let court_names = state.tablet.court_name_map();
        let seen = state.tablet.monitor_live_seen();
        relay_proto::build_device_list(&assignments, &court_names, &seen, now_ms())
    };
    match mode {
        ConnectionMode::Cloud => state.tablet.relay_monitor_devices(),
        ConnectionMode::Lan => lan_devices(),
        // Doppelmodus: LAN- und Cloud-Liste vereinen (Dedup über die
        // Geräte-ID, Online-Status der Quellen ge-ODER-t).
        ConnectionMode::LanAndCloud => {
            relay_proto::merge_device_lists(&lan_devices(), &state.tablet.relay_monitor_devices())
        }
    }
}

/// Weist ein Monitor-Gerät einem Feld (per CourtID) zu. `court_id` =
/// `None` hebt die Zuweisung auf (das Gerät zeigt dann wieder die
/// Kopplungs-Seite).
#[tauri::command]
pub fn assign_monitor(
    app: AppHandle,
    device_id: String,
    court_id: Option<i64>,
) -> Result<(), String> {
    if device_id.is_empty() || device_id.len() > 64 {
        return Err("Ungültige Geräte-ID.".to_string());
    }
    let path = monitor_assignments_path(&app);
    let mut map = crate::tablet::monitor::read_assignments(&path);
    match court_id {
        Some(cid) => {
            map.insert(device_id, cid);
        }
        None => {
            map.remove(&device_id);
        }
    }
    crate::tablet::monitor::write_assignments(&path, &map).map_err(|e| e.to_string())
}

/// Schickt einem Monitor-Gerät einen Fernbefehl: `kind` ist `"reload"`
/// (Seite neu laden) oder `"identify"` (Feldnummer groß einblenden).
#[tauri::command]
pub fn monitor_command(
    state: State<'_, AppState>,
    device_id: String,
    kind: String,
) -> Result<(), String> {
    let cmd = match kind.as_str() {
        "reload" => MonitorCommandKind::Reload,
        "identify" => MonitorCommandKind::Identify,
        _ => return Err("Unbekannter Befehl.".to_string()),
    };
    state.tablet.set_monitor_command(&device_id, cmd);
    Ok(())
}
