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

/// Pfad zur Datei mit den Werbebild-Labels (Dateiname → Anzeigename).
fn monitor_ad_labels_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join(crate::tablet::monitor::AD_LABELS_FILE)
}

/// Pfad zur Datei mit den Monitor-Feld-Zuweisungen (Gerät → Feld).
fn monitor_assignments_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .expect("App-Config-Verzeichnis ist verfügbar")
        .join(crate::tablet::monitor::MONITOR_ASSIGN_FILE)
}

/// Pfad der expliziten Hallen-Zuordnung je Monitor-Gerät.
fn monitor_halls_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .expect("App-Config-Verzeichnis ist verfügbar")
        .join(crate::tablet::monitor::MONITOR_HALLS_FILE)
}

/// Pfad zur Datei mit dem laufenden Live-Satzstand je Feld. Übersteht einen
/// App-Neustart, damit der TV nach einem Absturz/Neustart nicht auf BTPs
/// 0:0 zurückfällt, bis das Tablet wieder verbunden ist.
fn tablet_scores_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join("live-scores.json")
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

    // Live-Stände vom letzten Lauf wiederherstellen, BEVOR der erste Sync
    // läuft – sonst pusht run_once kurzzeitig BTPs 0:0. Danach jede
    // Score-Änderung dauerhaft sichern.
    let scores_path = tablet_scores_path(&app);
    if let Some(parent) = scores_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    tablet.load_scores(&scores_path);
    tablet.set_scores_path(scores_path);
    // Gesperrte Felder aus der Config in den Laufzeit-State übernehmen.
    tablet.set_locked_courts(config.locked_courts.iter().copied());

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
    let log_dir = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let ctx = Arc::new(crate::tablet::server::ServerCtx::new(
        tablet,
        config,
        push::build_client(),
        monitor_dir,
        cfg_path,
        assignments_path,
        log_dir,
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

/// Ein Werbebild mit optionalem Anzeige-Label. `label` ist leer, wenn
/// der Operator dem Bild noch keinen Namen gegeben hat – die UI
/// rendert dann den Dateinamen als Fallback.
#[derive(Serialize)]
pub struct CourtAd {
    pub file: String,
    pub label: String,
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

/// WLAN-Status des Turnier-PCs für die Kopfzeile.
#[derive(Clone, Serialize)]
pub struct WifiStatus {
    /// Mit einem WLAN verbunden?
    pub connected: bool,
    /// SSID des verbundenen Netzes (`None` = kein WLAN / nicht ermittelbar,
    /// z. B. LAN-Kabel oder fehlendes WLAN-Tool).
    pub ssid: Option<String>,
}

/// Liefert das aktuell verbundene WLAN (SSID), damit man in der Kopfzeile auf
/// einen Blick sieht, ob der PC im richtigen Hallen-/Verleih-Netz
/// (`btsaccess`) hängt. Plattform-spezifisch ausgelesen; schlägt das Auslesen
/// fehl (kein WLAN-Adapter, LAN-Kabel), ist `ssid = None`.
#[tauri::command]
pub fn wifi_status() -> WifiStatus {
    // current_ssid() startet ein externes Tool (netsh/networksetup/iwgetid).
    // Hängt der WLAN-Dienst (gestörter Adapter), könnte output() unbegrenzt
    // blockieren. Deadline drum herum, damit weder ein Tauri-Worker dauerhaft
    // hängt noch die Kopfzeile auf eine Antwort wartet.
    let ssid = ssid_with_timeout(Duration::from_secs(3));
    WifiStatus {
        connected: ssid.is_some(),
        ssid,
    }
}

/// Ruft `current_ssid()` in einem eigenen Thread auf und gibt nach `timeout`
/// auf (dann `None`). Ein wirklich hängendes Tool blockiert so höchstens den
/// abgekoppelten Hilfsthread, nicht den Command.
fn ssid_with_timeout(timeout: Duration) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(current_ssid());
    });
    rx.recv_timeout(timeout).ok().flatten()
}

/// Parst die SSID aus der Ausgabe von `netsh wlan show interfaces`. Robust
/// gegen Lokalisierung (das Feld „SSID" bleibt in jeder Sprache so) und gegen
/// die `BSSID`-Zeile: der Schlüssel muss exakt „SSID" sein. Eigene Funktion,
/// damit das Parsing unit-testbar ist.
#[cfg(any(target_os = "windows", test))]
fn parse_netsh_ssid(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("SSID") {
            let v = value.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn current_ssid() -> Option<String> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: sonst blitzt bei JEDEM 15-s-Poll kurz ein cmd-Fenster
    // auf (eine aus der GUI-App gestartete Konsolenanwendung bekommt sonst ein
    // eigenes Konsolenfenster). 0x0800_0000 = CREATE_NO_WINDOW.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("netsh")
        .args(["wlan", "show", "interfaces"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    parse_netsh_ssid(&String::from_utf8_lossy(&out.stdout))
}

/// macOS (nur für die Entwicklung): SSID über `networksetup`.
#[cfg(target_os = "macos")]
fn current_ssid() -> Option<String> {
    let out = std::process::Command::new("networksetup")
        .args(["-getairportnetwork", "en0"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // Erfolg: "Current Wi-Fi Network: <ssid>"; sonst "… not associated …".
    // strip_prefix statt split_once(':'), damit SSIDs mit ':' nicht abgeschnitten
    // werden (der feste Präfix selbst enthält keinen Doppelpunkt).
    let v = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("Current Wi-Fi Network:"))?
        .trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Linux (nur für die Entwicklung): SSID über `iwgetid`.
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn current_ssid() -> Option<String> {
    let out = std::process::Command::new("iwgetid")
        .arg("-r")
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
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

// ───────────────────────────── Feldvergabe (BTP-Write) ────────────────────

/// Weist ein Match einem Feld zu – schreibt die Zuweisung nach BTP
/// (`SENDUPDATE`-Courts-Block). Bidirektional: beim nächsten Poll liest
/// bts-light das Match als OnCourt auf diesem Feld zurück, und BTP zeigt es
/// ebenfalls. Wird auch genutzt, um das Feld umzubelegen.
#[tauri::command]
pub async fn assign_court(
    state: State<'_, AppState>,
    match_id: i64,
    court_id: i64,
) -> Result<(), String> {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    // Court→Match verknüpfen UND die Feldzuordnung am Match selbst setzen
    // (Halle+Feld erscheinen so konsistent in den BTP-Match-Eigenschaften).
    let match_courts = match state.tablet.match_planning(match_id) {
        Some((draw_id, planning_id)) => vec![crate::btp::proto::MatchCourt {
            match_id,
            draw_id,
            planning_id,
            court_id,
        }],
        None => Vec::new(),
    };
    crate::tablet::server::write_courts_to_btp(
        &config,
        &[crate::btp::proto::CourtAssignment {
            court_id,
            match_id: Some(match_id),
        }],
        &match_courts,
    )
    .await
}

/// Gibt ein Feld frei – löst die Court-Verknüpfung (`Court` ohne `MatchID`)
/// UND löscht die Feldzuordnung am Match selbst (`Match.CourtID = 0`), damit
/// Halle + Feld auch aus den BTP-Match-Eigenschaften verschwinden.
#[tauri::command]
pub async fn free_court(state: State<'_, AppState>, court_id: i64) -> Result<(), String> {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    // Das aktuell auf dem Feld stehende Match auflösen, um dessen CourtID zu löschen.
    let match_courts = match state.tablet.match_for_court(court_id) {
        Some(m) => vec![crate::btp::proto::MatchCourt {
            match_id: m.id,
            draw_id: m.draw_id,
            planning_id: m.planning_id,
            court_id: 0, // 0 = Feldzuordnung am Match löschen
        }],
        None => Vec::new(),
    };
    crate::tablet::server::write_courts_to_btp(
        &config,
        &[crate::btp::proto::CourtAssignment {
            court_id,
            match_id: None,
        }],
        &match_courts,
    )
    .await
}

/// Feld sperren/entsperren (bts-light-seitig). Persistiert die Sperrliste in
/// die Config, damit sie einen Neustart übersteht. BTP wird NICHT geschrieben –
/// gesperrte Felder werden nur nicht (auto-)belegt und im UI rot markiert.
#[tauri::command]
pub fn set_court_locked(
    app: AppHandle,
    state: State<'_, AppState>,
    court_id: i64,
    locked: bool,
) -> Result<(), String> {
    state.tablet.set_court_locked(court_id, locked);
    // Config-Wert bauen, Mutex VOR der Datei-I/O wieder freigeben (sonst
    // blockiert ein langsamer Schreibvorgang andere config-Zugriffe).
    let config_to_save = {
        let mut cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        cfg.locked_courts = state.tablet.locked_courts();
        cfg.clone()
    };
    config_to_save
        .save_to(&config_path(&app))
        .map_err(|e| e.to_string())?;
    Ok(())
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

/// Entfernt ein Werbebild aus dem `court-ads`-Verzeichnis. Räumt ein
/// eventuell hinterlegtes Label automatisch mit auf, damit die
/// Labels-Datei nicht über die Zeit mit Karteileichen wächst.
#[tauri::command]
pub fn remove_court_ad(app: AppHandle, file: String) -> Result<(), String> {
    if !crate::tablet::monitor::is_safe_image_name(&file) {
        return Err("Ungültiger Dateiname.".to_string());
    }
    std::fs::remove_file(monitor_ad_dir(&app).join(&file))
        .map_err(|e| format!("Löschen fehlgeschlagen: {e}"))?;
    let labels_path = monitor_ad_labels_path(&app);
    let mut labels = crate::tablet::monitor::read_ad_labels(&labels_path);
    if labels.remove(&file).is_some() {
        let _ = crate::tablet::monitor::write_ad_labels(&labels_path, &labels);
    }
    tracing::info!("Court-Monitor: Werbebild '{file}' entfernt");
    Ok(())
}

/// Listet die aktuell hinterlegten Werbebilder mit ihrem optionalen
/// Anzeigenamen. Eintraege ohne hinterlegtes Label tragen ein leeres
/// `label` – die UI faellt dann auf den Dateinamen zurueck.
#[tauri::command]
pub fn list_court_ads(app: AppHandle) -> Vec<CourtAd> {
    let files = crate::tablet::monitor::list_ads(&monitor_ad_dir(&app));
    let labels = crate::tablet::monitor::read_ad_labels(&monitor_ad_labels_path(&app));
    files
        .into_iter()
        .map(|file| CourtAd {
            label: labels.get(&file).cloned().unwrap_or_default(),
            file,
        })
        .collect()
}

/// Setzt oder löscht das Anzeige-Label eines Werbebilds. Ein leerer
/// `label`-String entfernt den Eintrag aus der Labels-Datei.
#[tauri::command]
pub fn set_court_ad_label(app: AppHandle, file: String, label: String) -> Result<(), String> {
    if !crate::tablet::monitor::is_safe_image_name(&file) {
        return Err("Ungültiger Dateiname.".to_string());
    }
    // Label-Länge begrenzen — die UI rendert das in einem Dropdown,
    // ueberlanger Text wuerde nur stoeren. 80 Zeichen sind reichlich.
    let label = label.trim();
    if label.chars().count() > 80 {
        return Err("Anzeigename ist zu lang (max. 80 Zeichen).".to_string());
    }
    let labels_path = monitor_ad_labels_path(&app);
    let mut labels = crate::tablet::monitor::read_ad_labels(&labels_path);
    if label.is_empty() {
        labels.remove(&file);
    } else {
        labels.insert(file.clone(), label.to_string());
    }
    crate::tablet::monitor::write_ad_labels(&labels_path, &labels)
        .map_err(|e| format!("Labels speichern fehlgeschlagen: {e}"))
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
    let mut devices = match mode {
        ConnectionMode::Cloud => state.tablet.relay_monitor_devices(),
        ConnectionMode::Lan => lan_devices(),
        // Doppelmodus: LAN- und Cloud-Liste vereinen (Dedup über die
        // Geräte-ID, Online-Status der Quellen ge-ODER-t).
        ConnectionMode::LanAndCloud => {
            relay_proto::merge_device_lists(&lan_devices(), &state.tablet.relay_monitor_devices())
        }
    };
    // Explizite Halle je Gerät anhängen (host-seitig persistiert) – greift in
    // ALLEN Modi, auch Cloud (der Relay kennt die Hallen-Zuordnung nicht).
    let halls = crate::tablet::monitor::read_halls(&monitor_halls_path(&app));
    if !halls.is_empty() {
        for d in &mut devices {
            d.hall = halls.get(&d.id).cloned();
        }
    }
    devices
}

/// Legt für ein Monitor-Gerät explizit eine Halle (Hallenname) fest oder hebt
/// die Zuordnung auf (`None`). Damit lassen sich auch Geräte ohne Feld
/// (unzugewiesen, Info/Werbung/Kombi) bei Mehr-Hallen-Turnieren einer Halle
/// zuordnen und gruppieren.
#[tauri::command]
pub fn set_monitor_hall(
    app: AppHandle,
    device_id: String,
    hall: Option<String>,
) -> Result<(), String> {
    if device_id.is_empty() || device_id.len() > 64 {
        return Err("Ungültige Geräte-ID.".to_string());
    }
    if hall.as_deref().is_some_and(|h| h.len() > 128) {
        return Err("Hallenname zu lang.".to_string());
    }
    let path = monitor_halls_path(&app);
    let mut map = crate::tablet::monitor::read_halls(&path);
    match hall.map(|h| h.trim().to_string()).filter(|h| !h.is_empty()) {
        Some(h) => {
            map.insert(device_id, h);
        }
        None => {
            map.remove(&device_id);
        }
    }
    crate::tablet::monitor::write_halls(&path, &map).map_err(|e| e.to_string())
}

/// Weist ein Monitor-Gerät einem Target zu (Feld oder Info-Anzeige).
/// `target = None` hebt die Zuweisung auf (das Gerät zeigt dann wieder
/// die Kopplungs-Seite).
///
/// Frontend ruft so auf:
/// - Feld: `{ kind: "court", court_id: 5 }`
/// - Info-Übersicht: `{ kind: "info_overview" }`
/// - Info-Vorbereitung: `{ kind: "info_preparation" }`
/// - Aufheben: `null`
#[tauri::command]
pub fn assign_monitor(
    app: AppHandle,
    device_id: String,
    target: Option<relay_proto::MonitorTarget>,
) -> Result<(), String> {
    if device_id.is_empty() || device_id.len() > 64 {
        return Err("Ungültige Geräte-ID.".to_string());
    }
    let path = monitor_assignments_path(&app);
    let mut map = crate::tablet::monitor::read_assignments(&path);
    match target {
        Some(t) => {
            map.insert(device_id, t);
        }
        None => {
            map.remove(&device_id);
        }
    }
    crate::tablet::monitor::write_assignments(&path, &map).map_err(|e| e.to_string())
}

/// Entfernt ein **offline** Monitor-Gerät aus der Liste: vergisst den
/// Live-Eintrag und löscht eine eventuelle Zuweisung. Online-Geräte
/// werden abgelehnt (sie würden ohnehin beim nächsten Poll
/// zurückkommen, und ein versehentliches Entfernen soll ihre Zuweisung
/// nicht verlieren).
#[tauri::command]
pub fn forget_monitor_device(
    app: AppHandle,
    state: State<'_, AppState>,
    device_id: String,
) -> Result<(), String> {
    if device_id.is_empty() || device_id.len() > 64 {
        return Err("Ungültige Geräte-ID.".to_string());
    }
    let now = crate::tablet::monitor::now_ms();
    if state.tablet.is_monitor_online(&device_id, now) {
        return Err("Online-Geräte können nicht entfernt werden.".to_string());
    }
    // Live-Eintrag vergessen.
    state.tablet.forget_monitor(&device_id);
    // Zuweisung (falls vorhanden) aus der v3-Datei entfernen.
    let path = monitor_assignments_path(&app);
    let mut map = crate::tablet::monitor::read_assignments(&path);
    if map.remove(&device_id).is_some() {
        crate::tablet::monitor::write_assignments(&path, &map).map_err(|e| e.to_string())?;
    }
    // Ebenso eine explizite Hallen-Zuordnung entfernen, sonst sammeln sich
    // über viele Turniere verwaiste Einträge in der Hallen-Datei an.
    let halls_path = monitor_halls_path(&app);
    let mut halls = crate::tablet::monitor::read_halls(&halls_path);
    if halls.remove(&device_id).is_some() {
        crate::tablet::monitor::write_halls(&halls_path, &halls).map_err(|e| e.to_string())?;
    }
    tracing::info!("Court-Monitor: Gerät '{device_id}' aus der Liste entfernt");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_netsh_ssid_reads_ssid_not_bssid() {
        // Gekürzte, typische netsh-Ausgabe (englisches Windows).
        let text = "\
    Name                   : WLAN
    State                  : connected
    SSID                   : btsaccess
    BSSID                  : 00:11:22:33:44:55
    Signal                 : 92%";
        assert_eq!(parse_netsh_ssid(text), Some("btsaccess".to_string()));
    }

    #[test]
    fn parse_netsh_ssid_handles_german_locale_and_spaces() {
        // Deutsches Windows: „Status" statt „State"; das Feld „SSID" bleibt.
        let text = "\
    Name                   : WLAN
    Status                 : Verbunden
    SSID                   : BTS Access 5G
    BSSID                  : aa:bb:cc:dd:ee:ff";
        assert_eq!(parse_netsh_ssid(text), Some("BTS Access 5G".to_string()));
    }

    #[test]
    fn parse_netsh_ssid_none_when_disconnected() {
        // Kein verbundenes Interface → keine (nicht-leere) SSID-Zeile.
        let text = "    Name                   : WLAN\n    State                  : disconnected";
        assert_eq!(parse_netsh_ssid(text), None);
    }
}
