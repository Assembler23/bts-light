//! Tauri-Commands – die Brücke zwischen der WebView-Oberfläche und dem
//! Rust-Kern. Enthält außerdem die Hintergrund-Polling-Schleife.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
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
    ///
    /// **Bewusst `Arc<Mutex<_>>`, nicht nur `Mutex<_>`:** Dasselbe Arc wird
    /// 1:1 an `ServerCtx` gereicht (Konstruktion in `start_sync`/`run_sync`)
    /// — LAN-Server, Relay-Client UND alle Tauri-Commands (`save_config`,
    /// `tl_device_add`, … über `mutate_config`) mutieren so denselben
    /// In-Memory-Stand statt zweier getrennter, gegeneinander driftender
    /// Kopien. Vorher schrieb `ServerCtx::mutate_app_config` (Panel-Profile,
    /// ADR 0025) direkt an der Platte vorbei am In-Memory-Stand — ein Lost-
    /// Update, sobald danach `mutate_config`/`save_config` seinen eigenen
    /// (veralteten) In-Memory-Stand komplett zurückschrieb.
    pub config: Arc<Mutex<AppConfig>>,
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
    /// Cloud-Slave: vom Master geerbte Azure-TTS-Konfiguration (ADR 0003).
    /// Bewusst nur im Arbeitsspeicher – wird nie in die `config.json`
    /// geschrieben; eine vollständige lokale Azure-Config hat Vorrang.
    pub inherited_azure: Mutex<Option<relay_proto::AzureTtsShare>>,
    /// Laufende mDNS-Bekanntgabe (`bts-light.local`) – LAN-Modus ODER
    /// Slave-Monitor-Brücke.
    pub mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
    /// Handle der Slave-Monitor-Brücke (`:8088` → Cloud-Monitor des Masters,
    /// nur im `slave_mode`), falls aktiv.
    pub slave_bridge: Mutex<Option<JoinHandle<()>>>,
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
        SyncOutcome::SlaveActive => ("ok", "Ansage-Slave aktiv – nur Ansagen".to_string()),
        SyncOutcome::BtpError(e) => ("btp_error", format!("BTP nicht erreichbar: {e}")),
        // Bewusst "warn" (orange), nicht "btp_error" (rot): Der Guard
        // heilt sich im Folge-Abruf selbst — für die Turnierleitung ist
        // das kein Ausfall.
        SyncOutcome::SnapshotDiscarded => (
            "warn",
            "BTP lieferte einen leeren Turnier-Stand – verworfen, warte auf Bestätigung"
                .to_string(),
        ),
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

/// Pfad zur Datei mit den „Leisten-Sponsor"-Markierungen. Liegt im
/// `court-ads/`-Verzeichnis (wie die Bilder), damit der Tablet-/Monitor-Server
/// sie über sein `monitor_dir` erreicht.
fn monitor_ad_bar_path(app: &AppHandle) -> std::path::PathBuf {
    monitor_ad_dir(app).join(crate::tablet::monitor::AD_BAR_FILE)
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

/// Pfad zur persistenten BTP-Nachschub-Queue (ADR 0018). Übersteht einen
/// App-Neustart, damit ein noch nicht nach BTP geschriebenes Ergebnis nicht
/// verloren geht. Geladen wird die Queue erst beim ersten Snapshot
/// (turnier-gegated), geschrieben bei jedem `queue`/`clear`.
fn tablet_btp_retry_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join("btp-retry.json")
}

/// Pfad des Schiedsrichter-Rosters (ADR 0022). Bewusst **außerhalb** der
/// config.json: Sperrlisten sind Personendaten und dürfen nicht ins
/// Identitäts-Bündel wandern; der Stand gilt zudem nur für ein Turnier.
fn tablet_officials_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join("officials-state.json")
}

/// Pfad der Auto-Vergabe-Ausnahmeliste (Spec `feldvergabe-ausnahme`, Muster
/// ADR 0022). Bewusst **außerhalb** der config.json: der Stand gilt nur für
/// ein Turnier, wie beim Schiedsrichter-Roster.
fn tablet_exclusions_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join("excluded-matches.json")
}

fn tablet_queue_order_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join("queue-order.json")
}

/// Pfad der Spielzeiten-Messung (Spec `spielzeiten-prognose`, Muster
/// ADR 0022). Bewusst **außerhalb** der config.json: der Stand gilt nur
/// für ein Turnier.
fn tablet_match_times_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("App-Datenverzeichnis ist verfügbar")
        .join("match-times.json")
}

/// Lädt die gespeicherte Konfiguration (oder Defaults beim ersten Start).
#[tauri::command]
pub fn load_config(app: AppHandle, state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = AppConfig::load_from(&config_path(&app)).map_err(|e| e.to_string())?;
    *state.config.lock().expect("Config-Mutex nicht vergiftet") = config.clone();
    Ok(config)
}

/// Übernimmt Felder, die der **Host** verwaltet, aus dem aktuellen Stand —
/// statt sie aus dem Fenster-Stand zu übernehmen.
///
/// Die Einstellungsseite schickt beim Speichern die **ganze** Konfiguration
/// zurück, so wie sie beim Öffnen der Seite aussah. Für Einstellungen, die
/// dort auch bearbeitet werden, ist das richtig. Die Liste der gekoppelten
/// Turnierleitungs-Geräte wächst aber am Host (Kopplung) und wäre sonst auf
/// den Stand von vor dem Öffnen zurückgesetzt — ein gerade gekoppeltes Gerät
/// verlöre seinen Zugang, sobald jemand irgendeine Einstellung speichert.
///
/// Bewusst **nur** die Geräteliste, nicht der Schalter: Wird die Oberfläche
/// abgeschaltet, sollen die Zugänge auch wirklich verschwinden. Rein &
/// testbar.
fn keep_host_managed_fields(mut incoming: AppConfig, current: &AppConfig) -> AppConfig {
    if incoming.tl_web.enabled {
        incoming.tl_web.devices = current.tl_web.devices.clone();
    } else {
        incoming.tl_web.devices.clear();
    }
    // Der Panel-Profil-Katalog wird ausschließlich über TlAction aus
    // tl.html gepflegt (ADR 0024), nie über den Setup-Assistenten — dessen
    // Speichern darf ihn nicht zurücksetzen. Anders als bei `devices` gibt
    // es hier kein „Ausschalten löscht" (Profile bleiben auch bei
    // abgeschalteter Oberfläche erhalten, sie sind keine Zugänge).
    incoming.tl_web.profiles = current.tl_web.profiles.clone();
    incoming.tl_web.default_profile_id = current.tl_web.default_profile_id.clone();
    // Die Hallen-Anordnung wird auf der Felderübersicht gepflegt, nicht im
    // Assistenten — dessen Speichern darf sie nicht zurücksetzen.
    incoming.hall_layouts = current.hall_layouts.clone();
    incoming
}

/// Speichert die Konfiguration dauerhaft.
#[tauri::command]
pub fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    let current = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    let config = keep_host_managed_fields(config, &current);
    config
        .save_to(&config_path(&app))
        .map_err(|e| e.to_string())?;
    // Haben sich die Logo-BILDDATEN geändert, sie einmalig an den badhub-Check-In
    // schieben (Phase 3 der Sponsor-Leiste) — statt sie alle 60 s im Liveticker-
    // `tset` mitzusenden. Nur `.data` vergleichen: `mime`/`background_color`
    // reisen ohnehin nicht zum Endpunkt, eine reine Farb-Änderung soll die (bis
    // 2 MB) Bilddaten nicht erneut über die Leitung schicken. Vor dem Verschieben
    // von `config` in den State prüfen.
    let logo_changed = config.tournament_logo.data != current.tournament_logo.data;
    // Schiedsrichter-Schalter sofort wirksam machen: Der Sync-Lauf liest
    // seine Konfiguration nur beim Start, deshalb hält der Roster-Speicher
    // die globalen Schalter — sonst bliebe das Häkchen bis zum nächsten
    // Stoppen/Starten der Übertragung wirkungslos.
    state
        .tablet
        .officials_store()
        .set_enabled(config.officials.enabled);
    state
        .tablet
        .officials_store()
        .set_rotation(config.officials.rotation_sr, config.officials.rotation_ar);
    *state.config.lock().expect("Config-Mutex nicht vergiftet") = config;
    if logo_changed {
        push_logo_to_badhub(&state);
    }
    Ok(())
}

/// Master-Identität exportieren (ADR 0006): die komplette Konfiguration inkl.
/// `install_id` als JSON-Bündel, damit ein neuer Turnier-PC dieselbe Identität
/// (= Relay-Namespace) übernimmt und alle gekoppelten Geräte ohne Neu-Koppeln
/// weiterlaufen. **Passwörter werden bewusst entfernt** (BTP + Badhub) — sie
/// werden am neuen PC neu eingegeben; das Bündel enthält dennoch die
/// `install_id` (= Bearer-Token) und ist wie ein Passwort zu behandeln.
/// Entfernt ALLE Secrets aus einer Config fürs Export-Bündel (ADR 0006):
/// BTP-Passwort, Badhub-Passwort UND den Azure-Subscription-Key. Die
/// `install_id` bleibt — sie ist der Zweck des Umzugs (und selbst ein
/// Bearer-Token, daher ist das Bündel wie ein Passwort zu behandeln). Rein &
/// testbar.
fn identity_bundle(mut cfg: AppConfig) -> AppConfig {
    cfg.btp.password = None;
    cfg.badhub.password = String::new();
    // Azure-Key ist ein echtes Secret (Speech-Ressource) — NIE mitexportieren.
    cfg.azure_tts.key = String::new();
    // Turnierleitungs-Geräte wandern NICHT mit (ADR 0012): Sonst bliebe der
    // alte PC über die exportierten Tokens schreibberechtigt, und das Bündel
    // wäre zugleich ein Satz gültiger Zugänge. Die Geräte koppeln sich am
    // neuen PC neu — ein QR-Scan je Gerät. Der Schalter bleibt erhalten.
    //
    // Der Panel-Profil-KATALOG (`tl_web.profiles`) wird bewusst NICHT
    // gestrippt — anders als die Geräte ist er kein Zugang/Secret, sondern
    // reine Layout-Konfiguration, und soll den Umzug überstehen wie
    // `hall_layouts` (ADR 0025). Die GERÄTE-Zuordnung eines Profils
    // (`TlDevice.profile_id`) verschwindet trotzdem vollständig — nicht
    // durch einen eigenen Schritt, sondern automatisch, weil die Zeile
    // darüber die komplette `devices`-Liste leert.
    cfg.tl_web.devices.clear();
    cfg
}

/// Übernimmt die importierte Identität, behält aber die am aktuellen PC bereits
/// gesetzten Secrets (das Bündel enthält keine): BTP-/Badhub-Passwort +
/// Azure-Key. Rein & testbar.
fn apply_imported_identity(mut imported: AppConfig, current: &AppConfig) -> AppConfig {
    if imported.btp.password.is_none() {
        imported.btp.password = current.btp.password.clone();
    }
    if imported.badhub.password.is_empty() {
        imported.badhub.password = current.badhub.password.clone();
    }
    if imported.azure_tts.key.is_empty() {
        imported.azure_tts.key = current.azure_tts.key.clone();
    }
    // Turnierleitungs-Geräte kommen nie im Bündel (identity_bundle löscht
    // sie) — die am DIESEM PC gekoppelten müssen deshalb erhalten bleiben.
    // Der übliche Ablauf ist: neuen PC einrichten, Tablets koppeln, dann die
    // Identität des alten holen. Ohne das würde genau dieser Schritt die
    // frisch gekoppelten Geräte wieder aussperren.
    if imported.tl_web.devices.is_empty() {
        imported.tl_web.devices = current.tl_web.devices.clone();
    }
    // Raster-Anordnungen (Task 9/11) fehlen in Bündeln aus einer Version vor
    // deren Einführung — ein leeres Feld heißt dann „unbekannt", nicht „am
    // neuen PC absichtlich gelöscht". Sonst würde ein Identitäts-Import mit
    // altem Bündel die hier schon eingerichteten Raster stillschweigend wegwischen.
    if imported.hall_layouts.is_empty() {
        imported.hall_layouts = current.hall_layouts.clone();
    }
    // Derselbe Fall wie bei den Rastern (Task 9/11): Ein Bündel aus einer
    // Version vor diesem Feature — oder eins von einer Installation ohne
    // eingerichtete Profile — trägt ein leeres `profiles`. Das darf die am
    // aktuellen PC schon eingerichteten Profile nicht stillschweigend
    // löschen (ADR 0025). `default_profile_id` folgt mit derselben
    // Bedingung: Er zeigt in den jeweils geltenden Katalog — ihn mit dem
    // Katalog der anderen Quelle zu mischen ergäbe eine Kennung, die im
    // übernommenen Katalog gar nicht existiert.
    if imported.tl_web.profiles.is_empty() {
        imported.tl_web.profiles = current.tl_web.profiles.clone();
        imported.tl_web.default_profile_id = current.tl_web.default_profile_id.clone();
    }
    imported
}

#[tauri::command]
pub fn export_identity(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    if cfg.install_id.trim().is_empty() {
        return Err("Diese Installation hat noch keine Identität (install_id).".to_string());
    }
    serde_json::to_string_pretty(&identity_bundle(cfg)).map_err(|e| e.to_string())
}

/// Master-Identität importieren (ADR 0006): übernimmt `install_id` + alle
/// Einstellungen aus einem Export-Bündel. Die am aktuellen PC bereits
/// gesetzten Passwörter (BTP/Badhub) bleiben erhalten (das Bündel enthält
/// keine). Überschreibt die lokale Identität — Aufrufer bestätigt vorher, und
/// es darf nur EIN Master gleichzeitig laufen (R4).
#[tauri::command]
pub fn import_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    bundle: String,
) -> Result<AppConfig, String> {
    let imported: AppConfig =
        serde_json::from_str(&bundle).map_err(|_| "Ungültige Identitäts-Datei.".to_string())?;
    // install_id ist Relay-Namespace + Log-Kennung → gegen dasselbe Format
    // prüfen wie beim Kopplungs-Code (Hex+Bindestrich, 8–64), damit eine
    // manuell verfälschte Datei keine kaputte Kennung in URLs/Header schleust.
    if !crate::tablet::relay_client::valid_relay_namespace(imported.install_id.trim()) {
        return Err("Die Datei enthält keine gültige Identität (install_id).".to_string());
    }
    // Passwörter des aktuellen PCs behalten — das Bündel enthält keine.
    let current = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    let imported = apply_imported_identity(imported, &current);
    imported
        .save_to(&config_path(&app))
        .map_err(|e| e.to_string())?;
    *state.config.lock().expect("Config-Mutex nicht vergiftet") = imported.clone();
    // Der Hinweis gilt für Tablets, Monitore und ferne Hallen — die hängen
    // an der install_id. Turnierleitungs-Geräte hängen dagegen an eigenen
    // Tokens, die das Bündel bewusst nicht enthält (ADR 0012); am neuen PC
    // gekoppelte bleiben, vom alten PC übernommene gibt es nicht.
    tracing::info!(
        "Master-Identität importiert (install_id übernommen) — Tablets, Monitore und ferne Hallen bleiben verbunden; Turnierleitungs-Geräte des alten PCs müssen neu gekoppelt werden"
    );
    Ok(imported)
}

/// Pfad zum Offline-Cache des geteilten Aussprache-Wörterbuchs. Liegt im
/// App-Config-Verzeichnis neben der config.json.
fn pronunciations_cache_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .expect("App-Config-Verzeichnis ist verfügbar")
        .join("pronunciations_cache.json")
}

/// Ein Eintrag des geteilten Aussprache-Wörterbuchs (= `NameOverride`).
/// `ipa` ist optional (nur für den Azure-`<phoneme>`-Pfad); fehlt es in der
/// API-Antwort, bleibt es `None` und wird beim Senden weggelassen.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct SharedPronunciation {
    pub name: String,
    #[serde(default)]
    pub say: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipa: Option<String>,
}

#[derive(serde::Deserialize)]
struct PronunciationsResp {
    #[serde(default)]
    entries: Vec<SharedPronunciation>,
}

/// Basis-Origin (`https://badhub.de`) aus der konfigurierten Badhub-URL.
fn badhub_origin(url: &str) -> Option<String> {
    let base = reqwest::Url::parse(url)
        .ok()
        .map(|u| u.origin().ascii_serialization())?;
    if base == "null" {
        None
    } else {
        Some(base)
    }
}

/// Lädt das geteilte Aussprache-Wörterbuch von Badhub (öffentlicher GET).
/// Erfolgreiche Antworten werden lokal gecached; bei fehlendem Internet wird
/// der Cache geliefert, damit die Ansage auch im reinen LAN-Hallenbetrieb
/// korrekt spricht. Liefert nie einen Fehler – schlimmstenfalls eine leere Liste.
#[tauri::command]
pub async fn fetch_pronunciations(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SharedPronunciation>, String> {
    // Basis-URL aus der Config ziehen (Guard vor dem await wieder freigeben).
    let base = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        badhub_origin(&cfg.badhub.url)
    };
    let cache = pronunciations_cache_path(&app);

    if let Some(base) = base {
        let url = format!("{base}/api/v1/pronunciations");
        let fetched: Option<Vec<SharedPronunciation>> = async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .ok()?;
            let resp = client.get(&url).send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let body: PronunciationsResp = resp.json().await.ok()?;
            Some(body.entries)
        }
        .await;

        if let Some(entries) = fetched {
            // Cache schreiben (best effort – Fehler hier sind unkritisch).
            if let Ok(json) = serde_json::to_string(&entries) {
                if let Some(dir) = cache.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&cache, json);
            }
            return Ok(entries);
        }
    }

    // Offline/Fehler → zuletzt gecachte Liste (oder leer).
    match std::fs::read_to_string(&cache) {
        Ok(s) => Ok(serde_json::from_str(&s).unwrap_or_default()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Teilt lokale Aussprache-Korrekturen mit der Community-DB (POST, opt-in).
/// Wird vom Frontend nur aufgerufen, wenn `share_corrections` aktiv ist.
#[tauri::command]
pub async fn share_pronunciations(
    state: State<'_, AppState>,
    entries: Vec<SharedPronunciation>,
) -> Result<usize, String> {
    if entries.is_empty() {
        return Ok(0);
    }
    let (base, install_id) = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        (badhub_origin(&cfg.badhub.url), cfg.install_id.clone())
    };
    let Some(base) = base else {
        return Err("Badhub-URL ungültig".to_string());
    };
    let url = format!("{base}/api/v1/pronunciations");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({ "entries": entries, "install_id": install_id });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(entries.len())
}

// ── Hallen-Check-In: Sicht der Turnierleitung (Schnitt C) ────────────────────
//
// Der Abruf läuft über diese Commands und **nicht** per `fetch()` aus React
// gegen badhub (Architekturregel R1). Das ist hier keine Formalie: das
// Liveticker-Passwort bleibt damit im Backend und taucht in keinem
// WebView-Request auf.

/// Zieht die Zugangsdaten des Check-Ins aus der Config.
///
/// `None` heißt „nicht eingerichtet" — kein Häkchen, keine Turnier-GUID oder
/// eine unbrauchbare badhub-URL. Der Aufrufer liefert dann `Unsupported`, und
/// die Oberfläche blendet den Bereich aus (AK-A6, C4).
fn checkin_zugang(cfg: &AppConfig) -> Option<(String, String, String)> {
    if !cfg.checkin.is_ready() {
        return None;
    }
    let base = badhub_origin(&cfg.badhub.url)?;
    Some((
        base,
        cfg.badhub.password.clone(),
        cfg.checkin.tournament_uuid.trim().to_string(),
    ))
}

/// Der Check-In-Stand für die Turnierleitungs-Sicht (AK-C1, C5).
///
/// **Liefert nie `Err`** — auch nicht ohne Internet und nicht gegen ein
/// badhub, das den Kanal noch nicht kennt. Beides kommt als
/// [`Availability`](crate::badhub::checkin_state::Availability) zurück, damit
/// die Seite einen verständlichen Hinweis zeigen kann statt einer
/// Fehlermeldung (AK-C3, C4).
///
/// Bewusst **ohne** lokalen Zwischenspeicher: badhub speichert, bts-light
/// zeigt an (AK-C13).
#[tauri::command]
pub async fn checkin_state(
    state: State<'_, AppState>,
) -> Result<crate::badhub::checkin_state::CheckinView, String> {
    use crate::badhub::checkin_state::{fetch_state, Availability, CheckinView};

    let zugang = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        checkin_zugang(&cfg)
    };

    let Some((base, password, uuid)) = zugang else {
        return Ok(CheckinView::unavailable(
            Availability::Unsupported,
            "Der Hallen-Check-In ist für dieses Turnier nicht eingerichtet.",
        ));
    };

    let mut view = fetch_state(&checkin_client(), &base, &password, &uuid).await;
    // Links zur öffentlichen Seite und zum QR-Aushang: aus der Config
    // gebaut, sobald der Check-In eingerichtet ist — auch bei „offline",
    // denn die Adresse selbst gilt unabhängig von der Erreichbarkeit.
    view.public_url = crate::badhub::checkin_state::public_url(&base, &uuid);
    view.poster_url = crate::badhub::checkin_state::poster_url(&base, &uuid);
    Ok(view)
}

/// Einen Spieler von Hand setzen, zurücksetzen oder entsperren (AK-C2).
///
/// `action`: `check_in` · `reset` · `unlock`. Das Zurücksetzen sperrt den
/// Selbst-Check-In — entschieden wird das in badhub, nicht hier.
#[tauri::command]
pub async fn checkin_set_player(
    state: State<'_, AppState>,
    event_id: i64,
    player_id: i64,
    action: String,
) -> Result<(), String> {
    let zugang = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        if cfg.slave_mode {
            // Genau ein Master schreibt (Mehr-Hallen-Regel D7). Ein Slave
            // zeigt den Stand an, greift aber nicht ein.
            return Err("Diese Instanz läuft als Slave und kann den Check-In nur anzeigen.".into());
        }
        checkin_zugang(&cfg)
    };
    let Some((base, password, uuid)) = zugang else {
        return Err("Der Hallen-Check-In ist für dieses Turnier nicht eingerichtet.".into());
    };

    crate::badhub::checkin_state::set_player(
        &checkin_client(),
        &base,
        &password,
        &uuid,
        event_id,
        player_id,
        &action,
    )
    .await
}

/// Anfangszeit und Anmeldeschluss einer Klasse ändern (AK-C12).
///
/// Der Wert landet sofort in badhub; bts-light hält **keine** eigene Kopie
/// (AK-C13). Ohne Verbindung wird der Versuch abgelehnt statt
/// zwischengespeichert (AK-C14).
#[tauri::command]
pub async fn checkin_set_times(
    state: State<'_, AppState>,
    event_id: i64,
    starts_at: Option<String>,
    closes_at: Option<String>,
) -> Result<(), String> {
    let zugang = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        if cfg.slave_mode {
            return Err("Diese Instanz läuft als Slave und kann den Check-In nur anzeigen.".into());
        }
        checkin_zugang(&cfg)
    };
    let Some((base, password, uuid)) = zugang else {
        return Err("Der Hallen-Check-In ist für dieses Turnier nicht eingerichtet.".into());
    };

    crate::badhub::checkin_state::set_times(
        &checkin_client(),
        &base,
        &password,
        &uuid,
        event_id,
        starts_at.as_deref(),
        closes_at.as_deref(),
    )
    .await
}

/// Baut den Ansagetext für eine Klasse (AK-C6 bis C8).
///
/// `kind`: `deadline` („Noch N Minuten bis Anmeldeschluss …") oder `missing`
/// (die fehlenden Spieler). `Ok(None)` heißt „es gibt nichts anzusagen" —
/// niemand fehlt oder der Anmeldeschluss ist vorbei.
///
/// **Der Stand wird dafür frisch geholt**, nicht aus der Anzeige übernommen:
/// zwischen dem letzten Poll und dem Klick können 15 Sekunden liegen, und eine
/// Ansage, die einen bereits Eingecheckten ausruft, schickt jemanden umsonst
/// zur Turnierleitung.
///
/// Gesprochen wird ausschließlich nach diesem Aufruf, also nach einem Klick
/// (AK-C10) — die App sagt nie von selbst etwas an.
#[tauri::command]
pub async fn checkin_announcement(
    state: State<'_, AppState>,
    event_id: i64,
    kind: String,
) -> Result<Option<String>, String> {
    use crate::badhub::checkin_state::{deadline_text, fetch_state, missing_text};

    let (zugang, max_names) = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        (checkin_zugang(&cfg), cfg.checkin.missing_names_max)
    };
    let Some((base, password, uuid)) = zugang else {
        return Err("Der Hallen-Check-In ist für dieses Turnier nicht eingerichtet.".into());
    };

    let view = fetch_state(&checkin_client(), &base, &password, &uuid).await;
    let Some(klasse) = view.classes.iter().find(|k| k.event_id == event_id) else {
        return Err("Diese Spielklasse steht nicht mehr in der Meldeliste.".into());
    };

    match kind.as_str() {
        "deadline" => Ok(deadline_text(klasse, chrono::Local::now().naive_local())),
        "missing" => Ok(missing_text(klasse, max_names)),
        _ => Err("Unbekannte Ansage.".into()),
    }
}

/// Frischer Client je Aufruf — wie bei [`fetch_pronunciations`]. Der Poll-Takt
/// der Check-In-Sicht liegt bei Sekunden, nicht Millisekunden; ein
/// vorgehaltener Client im `AppState` wäre hier Zustand ohne Gegenwert.
fn checkin_client() -> reqwest::Client {
    crate::badhub::checkin_state::build_client()
}

/// Testet die Verbindung zu BTP und liefert bei Erfolg den Turniernamen.
#[tauri::command]
pub async fn test_btp(host: String, port: u16, password: Option<String>) -> Result<String, String> {
    let snapshot = client::fetch_snapshot(&host, port, password.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(snapshot.tournament_name)
}

/// Synthetisiert eine Ansage per Azure Neural TTS und liefert das MP3 als
/// Base64. Key/Region kommen aus der gespeicherten Konfiguration oder – am
/// Cloud-Slave – aus der vom Master geerbten Config (ADR 0003); beides bleibt
/// im Backend. Ergebnis wird je SSML auf Platte gecacht. Fehler → `Err`, das
/// Frontend fällt dann auf die lokale Web-Speech-Ansage zurück.
#[tauri::command]
pub async fn azure_tts_speak(
    app: AppHandle,
    state: State<'_, AppState>,
    ssml: String,
) -> Result<String, String> {
    use base64::Engine;
    let cfg = AppConfig::load_from(&config_path(&app)).map_err(|e| e.to_string())?;
    let inherited = state
        .inherited_azure
        .lock()
        .expect("inherited_azure-Mutex nicht vergiftet")
        .clone();
    let (region, key) = match effective_azure(&cfg.azure_tts, inherited.as_ref()) {
        Some(rk) => rk,
        None => return Err("Azure TTS nicht konfiguriert".to_string()),
    };
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("tts-cache");
    let bytes = crate::azure_tts::synthesize(&region, &key, &ssml, &cache_dir).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Vorrangregel der Azure-Zugangsdaten (ADR 0003): vollständige lokale Config
/// (aktiv + Key + Region) gewinnt, sonst die vom Master geerbte. `None`, wenn
/// keine von beiden nutzbar ist.
fn effective_azure(
    local: &crate::config::AzureTtsConfig,
    inherited: Option<&relay_proto::AzureTtsShare>,
) -> Option<(String, String)> {
    if local.enabled && !local.key.is_empty() && !local.region.is_empty() {
        return Some((local.region.clone(), local.key.clone()));
    }
    inherited
        .filter(|a| !a.key.is_empty() && !a.region.is_empty())
        .map(|a| (a.region.clone(), a.key.clone()))
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
    // Badhub-Zugang nur im Normalbetrieb nötig — ein Ansage-Slave pusht nie
    // nach badhub und braucht weder Passwort noch (Cloud-)Installations-ID.
    if !config.slave_mode {
        if config.badhub.password.is_empty() {
            return Err("Es ist kein Badhub-Passwort konfiguriert.".to_string());
        }
        if config.connection_mode.cloud_enabled() && config.install_id.is_empty() {
            return Err("Für den Cloud-Modus fehlt die Installations-ID.".to_string());
        }
    }

    // Die von Hand gesetzten Spielorte liegen neben der Konfiguration und
    // überleben so einen Neustart des Turnier-PCs. Ohne das wäre die Arbeit
    // eines ganzen Vormittags nach einem Absturz verloren — für
    // Vorbereitungs-Aufrufe wäre das verschmerzbar, für den Spielort nicht.
    if let Ok(dir) = app.path().app_config_dir() {
        state
            .tablet
            .use_manual_hall_file(&dir.join("spielorte.json"));
    }

    // Vor dem Move von `config` in den Tablet-Kontext merken.
    let upload_logs = config.upload_logs;
    let install_id = config.install_id.clone();
    let master_namespace = config.master_namespace.trim().to_string();
    // Halle des Slaves — filtert die Feld-Auswahlseite der Brücke.
    let announce_hall = config.announce.announce_hall.clone();
    let mode = config.connection_mode;
    // Ansage-Slave: kein Tablet-Server/mDNS/Relay (nur BTP lesen + ansagen) –
    // sonst Kollision mit dem Master (doppeltes bts-light.local, Liveticker).
    let slave_mode = config.slave_mode;

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
    // Persistente BTP-Nachschub-Queue (ADR 0018): Pfad VOR dem ersten Sync
    // setzen (gleiches Verzeichnis wie die Live-Stände, oben schon angelegt).
    // Geladen wird die Queue erst beim ersten Snapshot — dann liegt der
    // Turnier-Guard (`tournament_name`) vor.
    tablet.set_btp_retry_path(tablet_btp_retry_path(&app));
    // Schiedsrichter-Roster (ADR 0022): Pfad jetzt, das Turnier kommt mit dem
    // ersten Snapshot — passt der Datei-Kopf nicht, wird der Stand verworfen.
    tablet
        .officials_store()
        .set_path(tablet_officials_path(&app));
    tablet
        .officials_store()
        .set_enabled(config.officials.enabled);
    tablet
        .officials_store()
        .set_rotation(config.officials.rotation_sr, config.officials.rotation_ar);
    // Ausnahmeliste der automatischen Feldvergabe (Spec
    // `feldvergabe-ausnahme`, Muster ADR 0022): Pfad jetzt, das Turnier
    // kommt mit dem ersten Snapshot.
    tablet.set_auto_assign_exclusions_path(tablet_exclusions_path(&app));
    tablet.set_queue_order_path(tablet_queue_order_path(&app));
    // Spielzeiten-Messung (Spec `spielzeiten-prognose`, Muster ADR 0022):
    // Pfad jetzt, das Turnier kommt mit dem ersten Snapshot.
    tablet.set_match_times_path(tablet_match_times_path(&app));
    // Punktverlauf: dauerhafte Ablage je Turnier (ADR 0015). Verzeichnis
    // jetzt, das Turnier kommt mit dem ersten Snapshot; die GUID aus der
    // Check-In-Config wandert als badhub-Brücke in den Datei-Kopf.
    if let Ok(dir) = app.path().app_data_dir() {
        tablet.timeline_store().set_dir(dir.join("punktverlauf"));
    }
    tablet
        .timeline_store()
        .set_guid(&config.checkin.tournament_uuid);
    // Gesperrte Felder aus der Config in den Laufzeit-State übernehmen.
    tablet.set_locked_courts(config.locked_courts.iter().copied());
    // Laufzeit-Schalter (Pause der automatischen Vergabe) beim Start lösen —
    // sonst bliebe eine auf der Turnierleitungs-Seite gesetzte Pause auch
    // dann bestehen, wenn das Gerät gar nicht mehr da ist.
    tablet.reset_runtime_switches();

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
        // Dasselbe Arc wie `AppState.config` — siehe Feld-Kommentar dort:
        // `ServerCtx::mutate_app_config` (Panel-Profile) und `mutate_config`
        // (Tauri-Commands) mutieren so einen einzigen In-Memory-Stand statt
        // zweier gegeneinander driftender Kopien.
        state.config.clone(),
    ));
    // LAN und Cloud sind unabhängig voneinander schaltbar – im
    // Doppelmodus (`LanAndCloud`) laufen beide Wege für dieselbe
    // Turnierinstanz parallel. `lan_enabled()`/`cloud_enabled()` liefern
    // für die reinen Modi exakt dieselbe Wahl wie zuvor das `match`.
    if !slave_mode && mode.lan_enabled() {
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
    if !slave_mode && mode.cloud_enabled() {
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
    // Cloud-Ansage-Slave: zusätzlich die Monitor-Brücke auf `:8088` starten,
    // damit Tilos Court-Monitor-Pis der fernen Halle den Slave per
    // Subnetz-Scan finden und auf den Cloud-Monitor des Masters umgeleitet
    // werden (kein Extra-Rechner nötig). Nur bei gültigem Master-Namespace.
    // mDNS (`bts-light.local`) für neue-Image-Pis kommt oben drauf. Setzt
    // getrennte Broadcast-Domains von Slave und Master voraus (Zwei-Hallen-
    // Standard: eigene Netze) — sonst konkurrierten zwei `bts-light.local`.
    if slave_mode && crate::tablet::relay_client::valid_relay_namespace(&master_namespace) {
        let mut bridge_slot = state
            .slave_bridge
            .lock()
            .expect("Slave-Brücke-Mutex nicht vergiftet");
        if bridge_slot.is_none() {
            let ns = master_namespace.clone();
            let hall = announce_hall.clone();
            *bridge_slot = Some(tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::tablet::slave_bridge::run(ns, hall).await {
                    tracing::error!("Slave-Brücke beendet: {e}");
                }
            }));
        }
        drop(bridge_slot);
        let mut mdns_slot = state.mdns.lock().expect("mDNS-Mutex nicht vergiftet");
        if mdns_slot.is_none() {
            match crate::tablet::mdns::advertise() {
                Ok(daemon) => *mdns_slot = Some(daemon),
                Err(e) => tracing::warn!("mDNS (Slave-Brücke) fehlgeschlagen: {e}"),
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
    if let Some(handle) = state
        .slave_bridge
        .lock()
        .expect("Slave-Brücke-Mutex nicht vergiftet")
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
    /// `true`, wenn das Bild zusätzlich klein in der oberen Leiste der
    /// Anzeigeseiten (neben dem Turnierlogo) erscheinen soll.
    pub in_bar: bool,
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

/// Erwartetes lokales BTS-Netz (Verleih-Set): WLAN `btsaccess` bzw. Subnetz
/// `192.168.16.0/24`. Über dieses Netz erreichen LAN-Tablets und Pi-Monitore
/// den PC – Tablets im Cloud-Modus sind davon unabhängig.
const EXPECTED_SSID: &str = "btsaccess";
const BTS_SUBNET: [u8; 3] = [192, 168, 16];

/// Lokaler Netzwerk-Status des Turnier-PCs für die Kopfzeile.
#[derive(Clone, Serialize)]
pub struct WifiStatus {
    /// Hängt der PC im lokalen BTS-Netz? Wahr, wenn er im `btsaccess`-WLAN ist
    /// ODER eine lokale IPv4 im BTS-Subnetz hat (deckt das LAN-Kabel ab, wo es
    /// keine SSID gibt).
    pub bts_network: bool,
    /// Verbundenes WLAN (zur Anzeige); `None` = kein WLAN (z. B. LAN-Kabel oder
    /// fehlendes WLAN-Tool).
    pub ssid: Option<String>,
}

/// Liefert den lokalen Netzwerk-Status, damit man in der Kopfzeile auf einen
/// Blick sieht, ob der PC im **BTS-Netzwerk** hängt (über das LAN-Tablets/Pis
/// ihn erreichen). Erkennt sowohl das `btsaccess`-WLAN als auch das BTS-Subnetz
/// am LAN-Kabel.
#[tauri::command]
pub fn wifi_status() -> WifiStatus {
    // current_ssid() startet ein externes Tool (netsh/networksetup/iwgetid).
    // Hängt der WLAN-Dienst (gestörter Adapter), könnte output() unbegrenzt
    // blockieren. Deadline drum herum, damit weder ein Tauri-Worker dauerhaft
    // hängt noch die Kopfzeile auf eine Antwort wartet.
    let ssid = ssid_with_timeout(Duration::from_secs(3));
    let on_bts_ssid = ssid
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case(EXPECTED_SSID))
        .unwrap_or(false);
    WifiStatus {
        bts_network: on_bts_ssid || on_bts_subnet(),
        ssid,
    }
}

/// Hat der PC eine lokale IPv4 im BTS-Subnetz (`192.168.16.0/24`)? Prüft alle
/// Schnittstellen, also auch das LAN-Kabel – kein Prozess-Start, schnell.
fn on_bts_subnet() -> bool {
    let Ok(ifaces) = local_ip_address::list_afinet_netifas() else {
        return false;
    };
    ifaces.iter().any(|(_, ip)| match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == BTS_SUBNET[0] && o[1] == BTS_SUBNET[1] && o[2] == BTS_SUBNET[2]
        }
        _ => false,
    })
}

/// Internet-/Uplink-Status für die Kopfzeile.
#[derive(Clone, Serialize)]
pub struct InternetStatus {
    /// Ist die badhub-Cloud erreichbar? = Internet/LTE-Uplink aktiv und zugleich
    /// Voraussetzung für Cloud-Logs + Liveticker-Push.
    pub online: bool,
}

/// Kurzer HEAD auf badhub.de: hat der PC Internet (LTE-Uplink aktiv)? Jede
/// HTTP-Antwort – auch 4xx/Cloudflare-Challenge – zählt als „online"; nur ein
/// Verbindungs-/Timeout-Fehler ist „offline". 5-s-Deadline, damit die Kopfzeile
/// nicht hängt. Carrier-Name (z. B. Vodafone) ist vom PC aus nicht ermittelbar.
#[tauri::command]
pub async fn internet_status() -> InternetStatus {
    let online = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c.head("https://badhub.de/").send().await.is_ok(),
        Err(_) => false,
    };
    InternetStatus { online }
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

/// Öffnet eine URL im Standardbrowser – für die Mitwirkenden-Links im
/// Über-Dialog und die Court-Übersicht-Vorschau. Erlaubt nur:
/// - saubere `https://`-URLs (externe Links, z. B. badhub-Liveticker), oder
/// - die eigene **lokale** Übersicht per `http://` auf Loopback bzw. den
///   mDNS-Namen `bts-light.local` (Vorschau am Turnier-PC).
/// Kein anderes Schema und keine Steuerzeichen/Leerzeichen → es wird kein
/// präparierter String an die OS-Shell durchgereicht.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    let has_bad_chars = url.contains(|c: char| c.is_control() || c == ' ');
    let is_https = url.starts_with("https://");
    let is_local_http = url.starts_with("http://localhost:")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with("http://bts-light.local:");
    if has_bad_chars || !(is_https || is_local_http) {
        return Err("Nur https- oder lokale bts-light-Links sind erlaubt.".to_string());
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
    // Ansage-Slave schreibt nie nach BTP (Wertungen nur am Master).
    if config.slave_mode {
        return Err("Ansage-Slave-Modus: Wertungen nur am Master-PC.".to_string());
    }
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
            // Bewusst 0 (Spec `spielzeiten-prognose`, E1): kampflos wurde
            // nicht gespielt — hier keine Dauer aus dem Zeiten-Store füllen.
            duration_mins: 0,
            score_status: 1, // 1 = Walkover
            // Kampflose Spiele stehen auf keinem Feld → nichts freizugeben,
            // niemand war auf dem Feld → keine Spieler-/Endzeit-Updates.
            free_court_id: None,
            player_ids: Vec::new(),
            end_ts_ms: None,
            officials: tablet.officials_for_result(cand.match_id),
        };
        match crate::tablet::server::write_result_settled(&config, &tablet, &update).await {
            Ok(()) => {
                written += 1;
            }
            Err(e) => {
                // Nachschub-Queue (A5): Der Sync-Loop reicht den Walkover
                // nach, sobald BTP wieder antwortet.
                tablet.queue_btp_retry(update.clone(), now_ms());
                errors.push(format!(
                    "{}: {e} (wird automatisch nachgereicht)",
                    cand.round_name
                ));
            }
        }
    }
    if errors.is_empty() {
        tablet.remove_walkover_proposal(&proposal_id);
    }
    Ok(WalkoverResult { written, errors })
}

/// Ergebnis eines Spiels aus der **Turnierleitung** eintragen (Plan 12,
/// Backend-Finalisierung, Tilo 20.07.: „ein Spiel aus dem Backend beenden,
/// wenn das Finalisieren vergessen wurde oder z. B. durch einen
/// Verbindungsabbruch nicht klappte"). Deckt Spiele ab, deren Ergebnis
/// über kein Tablet kam.
///
/// Reguläres Satz-Ergebnis; Kampflos/Aufgabe laufen weiter über den
/// Walkover-Flow. Dieselbe R5-Validierung wie der Tablet-Weg
/// (`derive_result`). Steht das Spiel noch auf einem Feld, wird es im
/// selben Request freigegeben und die Spieler ausgecheckt.
#[tauri::command]
pub async fn enter_result(
    state: State<'_, AppState>,
    match_id: i64,
    sets: Vec<(i64, i64)>,
) -> Result<(), String> {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    if config.slave_mode {
        return Err("Ansage-Slave-Modus: Wertungen nur am Master-PC.".to_string());
    }
    let tablet = state.tablet.clone();
    let snapshot = tablet
        .snapshot_clone()
        .ok_or("Noch kein Turnier geladen.")?;
    let m = snapshot
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .ok_or("Spiel nicht gefunden.")?;
    // Kernlogik (Guards, R5-Validierung, Satz-Vollständigkeit,
    // MatchUpdate-Bau) ist rein & getestet in server::build_manual_result_update.
    // Annahme: der zuletzt gepollte Snapshot ist aktuell genug — dieselbe
    // Poll-Staleness-Grundlage wie assign_court/free_court/confirm_walkover
    // (R2); der `winner.is_some()`-Guard deckt den bereits-gewertet-Fall ab.
    let end_ms = now_ms();
    // Bruttostart aus dem Zeiten-Store (Spec `spielzeiten-prognose`, E1):
    // neustartfest; on_court_since bleibt Fallback. Damit sendet auch die
    // Backend-Wertung eine echte Duration statt 0. Als Ende zählt bei
    // einer Korrektur der ursprüngliche E3-Stempel, nicht „jetzt".
    let on_court_since = tablet.brutto_start_ms(m.id, m.court_id);
    let btp_end_ms = tablet.result_end_ms(m.id, end_ms);
    let officials = tablet.officials_for_result(m.id);
    let update = crate::tablet::server::build_manual_result_update(
        m,
        sets,
        on_court_since,
        btp_end_ms,
        officials,
    )?;
    let mid = update.btp_match_id;
    let free_court_id = update.free_court_id;
    // Spielende stempeln (E3): auch die Backend-Wertung hält den
    // Eingangszeitpunkt fest — aber als NICHT-regulär (E11): tablet-lose
    // Ergebnisse liefern keinen Messwert für die Prognose-Statistik.
    tablet
        .match_times_store()
        .stamp_finished(mid, false, end_ms);
    match crate::tablet::server::write_result_settled(&config, &tablet, &update).await {
        Ok(()) => {
            if let Some(cid) = free_court_id {
                tablet.clear_court(cid);
            }
            // Punktverlauf abschließen (Spec punktverlauf-graph): auch die
            // TL-Wertung beendet ein evtl. tablet-gezähltes Spiel — sonst
            // bliebe `finished=false` und der Abweichungs-Hinweis (AK-8)
            // könnte nie erscheinen. Ohne Aufzeichnung ein No-op.
            tablet.timeline_store().finalize(mid, false);
            tracing::info!("Turnierleitung: Ergebnis für Match {mid} nach BTP geschrieben");
            Ok(())
        }
        Err(e) => {
            // Nachschub-Queue (A5): der Sync-Loop reicht es nach.
            tablet.queue_btp_retry(update, end_ms);
            Err(format!("{e} (wird automatisch nachgereicht)"))
        }
    }
}

/// Disqualifikation aus der Turnierleitung (P3, ScoreStatus 3): das Team
/// `loser_team` (1 oder 2) wird disqualifiziert, der Gegner gewinnt. Bereits
/// gespielte `sets` bleiben erhalten (eine Disqualifikation kann mitten im
/// Spiel fallen — keine Satz-Vollständigkeitsprüfung). Gleicher BTP-Schreibweg
/// wie `enter_result` (write_result_to_btp + Nachschub-Queue bei Fehler).
#[tauri::command]
pub async fn disqualify_match(
    state: State<'_, AppState>,
    match_id: i64,
    loser_team: i64,
    sets: Vec<(i64, i64)>,
) -> Result<(), String> {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    if config.slave_mode {
        return Err("Ansage-Slave-Modus: Wertungen nur am Master-PC.".to_string());
    }
    let tablet = state.tablet.clone();
    let snapshot = tablet
        .snapshot_clone()
        .ok_or("Noch kein Turnier geladen.")?;
    let m = snapshot
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .ok_or("Spiel nicht gefunden.")?;
    let end_ms = now_ms();
    // Bruttostart/Ende aus dem Zeiten-Store (Spec `spielzeiten-prognose`,
    // E1/E3) — wie bei `enter_result`.
    let on_court_since = tablet.brutto_start_ms(m.id, m.court_id);
    let btp_end_ms = tablet.result_end_ms(m.id, end_ms);
    let officials = tablet.officials_for_result(m.id);
    let update = crate::tablet::server::build_manual_dq_update(
        m,
        loser_team,
        sets,
        on_court_since,
        btp_end_ms,
        officials,
    )?;
    let mid = update.btp_match_id;
    let free_court_id = update.free_court_id;
    // Spielende stempeln (E3/E11): Eingangszeitpunkt festhalten, aber eine
    // Disqualifikation ist kein regulärer Messwert.
    tablet
        .match_times_store()
        .stamp_finished(mid, false, end_ms);
    match crate::tablet::server::write_result_settled(&config, &tablet, &update).await {
        Ok(()) => {
            if let Some(cid) = free_court_id {
                tablet.clear_court(cid);
            }
            // Punktverlauf abschließen — DQ ist ein Sonderausgang mitten
            // im Satz (AK-13); ohne Aufzeichnung ein No-op.
            tablet.timeline_store().finalize(mid, true);
            tracing::info!("Turnierleitung: Disqualifikation für Match {mid} nach BTP geschrieben");
            Ok(())
        }
        Err(e) => {
            tablet.queue_btp_retry(update, end_ms);
            Err(format!("{e} (wird automatisch nachgereicht)"))
        }
    }
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
    // Ansage-Slave schreibt nie nach BTP (nur der Master vergibt Felder).
    if config.slave_mode {
        return Err("Ansage-Slave-Modus: Feldvergabe nur am Master-PC.".to_string());
    }
    // Disziplin/Klasse→Halle-Regel: manuelle Vergabe in eine nicht erlaubte
    // Halle hart verhindern (Hard-Block, gleiche Regel wie die Auto-Vergabe).
    if let Some(snap) = state.tablet.snapshot_clone() {
        if let Some(m) = snap.matches.iter().find(|m| m.id == match_id) {
            let court_hall = snap.court_location_name(court_id);
            if !config.hall_allows_match(m.discipline.as_str(), &m.draw_name, &court_hall) {
                let what = if m.draw_name.trim().is_empty() {
                    m.discipline.as_str().to_string()
                } else {
                    m.draw_name.trim().to_string()
                };
                let allowed = config
                    .allowed_hall_for(m.discipline.as_str(), &m.draw_name)
                    .unwrap_or("");
                let here = court_hall.trim();
                return Err(format!(
                    "„{what}“ darf nur in Halle „{allowed}“ vergeben werden — dieses Feld liegt in „{}“.",
                    if here.is_empty() { "—" } else { here }
                ));
            }
        }
    }
    // Court→Match verknüpfen UND die Feldzuordnung am Match selbst setzen
    // (Halle+Feld erscheinen so konsistent in den BTP-Match-Eigenschaften).
    let match_courts = match state.tablet.match_planning(match_id) {
        Some((draw_id, planning_id)) => vec![crate::btp::proto::MatchCourt {
            match_id,
            draw_id,
            planning_id,
            court_id,
            // Beim Ruf aufs Feld wandert die Besetzung mit (ADR 0021) —
            // ein Request statt zwei.
            officials: state
                .tablet
                .snapshot_match(match_id)
                .and_then(|m| state.tablet.officials_for_write(&m)),
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
    // Ansage-Slave schreibt nie nach BTP.
    if config.slave_mode {
        return Err("Ansage-Slave-Modus: Feldvergabe nur am Master-PC.".to_string());
    }
    // Das aktuell auf dem Feld stehende Match auflösen, um dessen CourtID zu löschen.
    let match_courts = match state.tablet.match_for_court(court_id) {
        Some(m) => vec![crate::btp::proto::MatchCourt {
            match_id: m.id,
            draw_id: m.draw_id,
            planning_id: m.planning_id,
            court_id: 0, // 0 = Feldzuordnung am Match löschen
            // Beim Freigeben die Besetzung nicht anfassen: Das Spiel ist
            // nicht zu Ende, es wird nur vom Feld genommen.
            officials: None,
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
    /// Name der Auslosung/Klasse (BTP `draw_name`, z. B. „HE A") – für die
    /// Disziplin/Klasse→Halle-Regel (welche Felder erlaubt sind).
    pub draw_name: String,
    /// Klassen-Kürzel („A", „B", …) für die Ansage „Herreneinzel A"
    /// (leer = keins erkennbar; aus Event-/Draw-Name extrahiert).
    pub class_label: String,
    /// Runden-/Spielbezeichnung (z. B. „G1", „Finale") für die Tabellenanzeige.
    pub round_name: String,
    /// Angesetzte Spielzeit (BTP `PlannedTime`) als `YYYYMMDDHHMM`; `null` ohne.
    pub planned_time: Option<i64>,
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
    /// Von der automatischen Feldvergabe ausgenommen (Spec
    /// `feldvergabe-ausnahme`)? Manuelles Zuweisen bleibt davon unberührt.
    pub excluded: bool,
    /// In welche Halle das Spiel gehört (leer = unbekannt) — Grundlage der
    /// Hallen-Abschnitte in der Vorbereitungs-Liste (Spec
    /// `spielliste-manuelle-reihenfolge`).
    pub hall: String,
    /// Steht dieses Spiel gerade im manuellen Präfix seiner Halle?
    pub manual: bool,
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
    let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
    preparation_candidates_for(&state.tablet, &cfg)
}

/// Kernlogik von [`preparation_candidates`] ohne den Tauri-`State`-Wrapper —
/// direkt testbar und Grundlage des Cross-Site-Regressionstests
/// (`tests/queue_order_consistency.rs`, ADR 0023): ein Vergleich gegen
/// `tl.rs::build_state` und `badhub/payload.rs::build_tset` für dieselben
/// Testdaten. `pub`, damit der Integrationstest sie erreicht.
pub fn preparation_candidates_for(
    tablet: &crate::tablet::state::TabletState,
    cfg: &AppConfig,
) -> PreparationView {
    let Some(snapshot) = tablet.snapshot_clone() else {
        return PreparationView {
            candidates: Vec::new(),
            locations: Vec::new(),
        };
    };
    let calls = tablet.preparation_calls();
    let manual_halls = tablet.manual_halls();

    // Erst nur Ordnungsschlüssel + Halle sammeln (Muster `tl.rs::build_state`)
    // — **derselbe** gemeinsame Helfer wie an den anderen vier Sortier-
    // Stellen, sonst zeigte diese Liste eine andere Reihenfolge als TL-Web.
    let mut ordered: Vec<(
        crate::tablet::assign::ManualOrderSortKey,
        &crate::btp::model::BtpMatch,
        String,
    )> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::Scheduled)
        // Nur echte Paarungen – beide Mannschaften müssen feststehen.
        .filter(|m| !m.team1.is_empty() && !m.team2.is_empty())
        .map(|m| {
            let call = calls.iter().find(|c| c.match_id == m.id);
            let manual_hall = manual_halls.get(&m.id).map(String::as_str);
            let called_hall = call.and_then(|c| c.location_id).and_then(|lid| {
                snapshot
                    .locations
                    .iter()
                    .find(|l| l.id == lid)
                    .map(|l| l.name.as_str())
            });
            let (hall, _, key) = crate::tablet::assign::resolve_and_sort_key(
                cfg,
                &snapshot,
                m,
                manual_hall,
                called_hall,
                call.is_some(),
                tablet.queue_order_store(),
            );
            (key, m, hall)
        })
        .collect();
    ordered.sort_by_key(|(key, _, _)| *key);

    let candidates: Vec<PreparationCandidate> = ordered
        .into_iter()
        .map(|(_, m, hall)| {
            let call = calls.iter().find(|c| c.match_id == m.id).map(|c| {
                let call_hall = c.location_id.and_then(|lid| {
                    snapshot
                        .locations
                        .iter()
                        .find(|l| l.id == lid)
                        .map(|l| l.name.clone())
                });
                PreparationCallInfo {
                    location_id: c.location_id,
                    hall: call_hall.unwrap_or_default(),
                    called_at_ms: c.called_at_ms,
                }
            });
            let manual = tablet.queue_order_store().rank(m.id).is_some();
            PreparationCandidate {
                match_id: m.id,
                label: format!("{} {}", m.draw_name, m.round_name)
                    .trim()
                    .to_string(),
                discipline: m.discipline.as_str().to_string(),
                draw_name: m.draw_name.clone(),
                class_label: m.class_label.clone(),
                round_name: m.round_name.clone(),
                planned_time: m.planned_time,
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
                excluded: tablet.auto_assign_excluded(m.id),
                hall,
                manual,
            }
        })
        .collect();

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

/// Eine Auslosung/Klasse des Turniers für die Disziplin/Klasse→Halle-Einstellung.
#[derive(Serialize)]
pub struct DrawInfo {
    /// Disziplin als snake_case-Schlüssel (`mens_singles` …) = Kategorie.
    pub discipline: String,
    /// Name der Auslosung/Klasse (BTP `draw_name`, z. B. „HE A").
    pub draw_name: String,
}

/// Liefert die im Turnier vorkommenden Auslosungen (Disziplin + `draw_name`),
/// dedupliziert – Grundlage der Disziplin/Klasse→Halle-Einstellung im Frontend.
#[tauri::command]
pub fn tournament_draws(state: State<'_, AppState>) -> Vec<DrawInfo> {
    let Some(snapshot) = state.tablet.snapshot_clone() else {
        return Vec::new();
    };
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out: Vec<DrawInfo> = Vec::new();
    for m in &snapshot.matches {
        let disc = m.discipline.as_str().to_string();
        let draw = m.draw_name.trim().to_string();
        if seen.insert((disc.clone(), draw.clone())) {
            out.push(DrawInfo {
                discipline: disc,
                draw_name: draw,
            });
        }
    }
    // Stabil nach Disziplin, dann Auslosung sortieren.
    out.sort_by(|a, b| {
        a.discipline
            .cmp(&b.discipline)
            .then(a.draw_name.cmp(&b.draw_name))
    });
    out
}

/// Turnier-Kennzahlen fürs Dashboard (Startseite). Aus dem aktuellen
/// BTP-Snapshot abgeleitet; `None`, solange noch kein Snapshot vorliegt
/// (Liveticker nicht gestartet bzw. erste Antwort steht noch aus).
#[derive(Serialize)]
pub struct TournamentStats {
    /// Turniername (BTP-Setting 1001).
    pub tournament_name: String,
    /// Anzahl Konkurrenzen = eindeutige Auslosungen (Disziplin + `draw_name`).
    pub n_disciplines: usize,
    /// Anzahl eindeutiger Spieler (über alle Paarungen, nach Name dedupliziert).
    pub n_players: usize,
    /// Spiele gesamt.
    pub matches_total: usize,
    /// Abgeschlossene Spiele (Sieger steht fest).
    pub matches_finished: usize,
    /// Laufende Spiele (einem Feld zugewiesen, noch ohne Sieger).
    pub matches_running: usize,
    /// Anzahl Felder (alle Courts des Turniers).
    pub n_courts: usize,
    /// Hallen-Namen (BTP `Locations`), alphabetisch.
    pub halls: Vec<String>,
}

/// Liefert die Turnier-Kennzahlen fürs Dashboard aus dem aktuellen Snapshot.
#[tauri::command]
pub fn tournament_stats(state: State<'_, AppState>) -> Option<TournamentStats> {
    let snapshot = state.tablet.snapshot_clone()?;
    let mut draws: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut players: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut finished = 0usize;
    let mut running = 0usize;
    for m in &snapshot.matches {
        draws.insert((
            m.discipline.as_str().to_string(),
            m.draw_name.trim().to_string(),
        ));
        for p in m.team1.iter().chain(m.team2.iter()) {
            let name = p.name.trim();
            if !name.is_empty() {
                players.insert(name.to_string());
            }
        }
        if m.winner.is_some() {
            finished += 1;
        } else if m.court_id.is_some() {
            running += 1;
        }
    }
    let mut halls: Vec<String> = snapshot
        .locations
        .iter()
        .map(|l| l.name.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    halls.sort_by_key(|a| a.to_lowercase());
    halls.dedup();
    Some(TournamentStats {
        tournament_name: snapshot.tournament_name.clone(),
        n_disciplines: draws.len(),
        n_players: players.len(),
        matches_total: snapshot.matches.len(),
        matches_finished: finished,
        matches_running: running,
        // court_infos = Felder mit echter CourtID (strukturiert); courts wäre
        // die Namensliste – beide zählen dieselben physischen Felder.
        n_courts: snapshot.court_infos.len(),
        halls,
    })
}

/// Eine Zeile der „Abgeschlossene Spiele"-Tabelle (Spielübersicht).
#[derive(Serialize)]
pub struct FinishedMatchRow {
    pub match_id: i64,
    /// Auslosung/Klasse (z. B. „HE A").
    pub draw_name: String,
    /// Runde (z. B. „Finale", „G1").
    pub round_name: String,
    pub match_num: Option<i64>,
    /// Angesetzte Spielzeit (`YYYYMMDDHHMM`), `null` ohne Ansetzung.
    pub planned_time: Option<i64>,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Sieger-Team (1 oder 2).
    pub winner: u8,
    /// Satz-Ergebnisse als (Team1, Team2)-Paare, z. B. [[15,9],[11,15],[14,16]].
    pub sets: Vec<(i64, i64)>,
    /// Art der Entscheidung: `normal` · `walkover` · `retired` · `disqualified`.
    pub result: String,
    /// Feldname, auf dem gespielt wurde (leer, falls nicht zugewiesen).
    pub court: String,
    /// Halle (BTP-Location-Name; leer bei Ein-Hallen-Turnieren).
    pub location: String,
    /// Zeitpunkt der Beendigung (Unix-ms) – für die Sortierung (neueste zuerst).
    pub finished_at: Option<u64>,
    /// Gibt es einen Punktverlauf zum Anzeigen (Spec punktverlauf-graph)?
    /// Papier-Ergebnisse haben keinen — die Tabelle bietet den Graph-Klick
    /// dann gar nicht erst an.
    pub has_timeline: bool,
}

/// Punktverlauf eines Matches (Spec punktverlauf-graph, R1: der Browser
/// spricht nie selbst mit dem Store). `None` = kein Verlauf aufgezeichnet
/// (Papier-Ergebnis oder Spiel vor Einführung).
#[tauri::command]
pub fn match_timeline(
    state: State<'_, AppState>,
    match_id: i64,
) -> Option<relay_proto::MatchTimeline> {
    state.tablet.timeline_store().timeline(match_id)
}

/// Abgeschlossene Spiele (mit Sieger) für die Spielübersicht-Tabelle, neueste
/// zuerst. Reiner Lesepfad aus dem aktuellen Snapshot.
#[tauri::command]
pub fn finished_matches(state: State<'_, AppState>) -> Vec<FinishedMatchRow> {
    use crate::btp::model::{MatchResult, MatchStatus};
    let Some(snapshot) = state.tablet.snapshot_clone() else {
        return Vec::new();
    };
    let mut rows: Vec<FinishedMatchRow> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Finished && m.winner.is_some())
        .map(|m| FinishedMatchRow {
            match_id: m.id,
            draw_name: m.draw_name.clone(),
            round_name: m.round_name.clone(),
            match_num: m.match_num,
            planned_time: m.planned_time,
            team1: m.team1.iter().map(|p| p.name.clone()).collect(),
            team2: m.team2.iter().map(|p| p.name.clone()).collect(),
            winner: m.winner.unwrap_or(0),
            sets: m.sets.clone(),
            result: match m.result {
                MatchResult::Normal => "normal",
                MatchResult::Walkover => "walkover",
                MatchResult::Retired => "retired",
                MatchResult::Disqualified => "disqualified",
            }
            .to_string(),
            court: m.court.clone().unwrap_or_default(),
            location: m
                .court_id
                .map(|cid| snapshot.court_location_name(cid))
                .unwrap_or_default(),
            finished_at: m.finished_at,
            has_timeline: state.tablet.timeline_store().has_timeline(m.id),
        })
        .collect();
    // Neueste zuerst. `Option::cmp` würde `None` bei absteigender Sortierung
    // nach OBEN ziehen (z. B. Walkover ohne Zeitstempel) — daher mit
    // `unwrap_or(0)` explizit ans Ende statt an den Anfang.
    rows.sort_by(|a, b| {
        b.finished_at
            .unwrap_or(0)
            .cmp(&a.finished_at.unwrap_or(0))
            .then(b.match_num.unwrap_or(0).cmp(&a.match_num.unwrap_or(0)))
            .then(b.match_id.cmp(&a.match_id))
    });
    rows
}

/// Master: eine Freitext-Ansage ablegen. `hall` = Ziel-Halle (BTP-Location-Name;
/// leer = alle Hallen). Master + Slaves pollen sie über `pending_freetext`.
#[tauri::command]
pub fn publish_freetext(state: State<'_, AppState>, hall: String, text: String) -> u64 {
    state
        .tablet
        .publish_freetext(hall.trim().to_string(), text.trim().to_string())
}

/// Neue Freitext-Ansagen (`id > since`) für die eigene Halle. Im Slave-Modus
/// vom Master (BTP-Rechner, `:8088`) geholt, sonst aus dem lokalen Stand.
#[tauri::command]
pub async fn pending_freetext(
    state: State<'_, AppState>,
    since: u64,
) -> Result<Vec<crate::tablet::state::FreetextItem>, String> {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    let hall = config.announce.announce_hall.clone();
    if config.slave_mode {
        // Cloud-Ansage-Slave: Freitexte kommen ausschließlich über
        // `cloud_announce_state` (CloudAnnounceSlave). Den LAN-Poll hier dann
        // bewusst NICHT fahren — sonst sagte beides denselben Text an.
        if !config.master_namespace.trim().is_empty() {
            return Ok(Vec::new());
        }
        // Vom Master holen – gleiches Netz vorausgesetzt (BTP-Host = Master-PC).
        // URL über `reqwest::Url` bauen (das `query`-Feature ist nicht aktiv).
        let mut url = reqwest::Url::parse(&format!(
            "http://{}:8088/info/announce/freetext",
            config.btp.host
        ))
        .map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("hall", &hall)
            .append_pair("since", &since.to_string());
        // Kurzer Timeout für den LAN-Poll (alle 3 s): der Master antwortet im
        // LAN sofort oder gar nicht — der 15-s-Internet-Timeout von build_client
        // würde bei hängender Verbindung Anfragen stauen.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .connect_timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| push::build_client());
        let resp = match client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            // Master (noch) nicht erreichbar → leer, der Poller versucht es erneut.
            _ => return Ok(Vec::new()),
        };
        resp.json::<Vec<crate::tablet::state::FreetextItem>>()
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(state.tablet.freetext_since(&hall, since))
    }
}

/// Meldet, welche Aufruf-Stufe die Desktop-Übersicht gerade **angesagt hat**.
///
/// Bewusst meldend und nicht hochzählend: Die Oberfläche weiß genau, was sie
/// gesprochen hat (ihr erster Druck ist der schlichte Aufruf ohne
/// Stufenwort). Ließe sie stattdessen hochzählen, liefe die gemeinsame
/// Zählung ihr um eins voraus — die Turnierleitungs-Seite zeigte „2. Aufruf
/// erfolgt" nach dem ersten und verlöre nach dem zweiten ihren Aufruf-Knopf.
#[tauri::command]
pub fn note_court_call(state: State<'_, AppState>, court_id: i64, match_id: i64, stage: u8) -> u8 {
    state.tablet.reached_court_call(court_id, match_id, stage)
}

/// Neue Ansage-Aufträge (`id > since`) für die eigene Halle.
///
/// Derselbe Weg wie beim Freitext: Im LAN-Slave-Betrieb vom Master geholt,
/// sonst aus dem lokalen Stand. Wer hier abholt, gilt dem Turnier-PC als
/// Ansage-Gerät seiner Halle — daran erkennt die Turnierleitung, ob ihr
/// Aufruf überhaupt irgendwo erklingen kann.
#[tauri::command]
pub async fn pending_announce_jobs(
    state: State<'_, AppState>,
    since: u64,
) -> Result<Vec<crate::tablet::state::AnnounceJob>, String> {
    let config = state
        .config
        .lock()
        .expect("Config-Mutex nicht vergiftet")
        .clone();
    let hall = config.announce.announce_hall.clone();
    if config.slave_mode {
        // Cloud-Ansage-Slave: **Noch nicht unterstützt.** Der Relay-Zustand
        // (`cloud_announce_state`) trägt bis heute keine Ansage-Aufträge; der
        // Weg dorthin entsteht mit der Cloud-Anbindung der
        // Turnierleitungs-Seite. Bis dahin bleibt die ferne Halle stumm —
        // aber ehrlich: Da sich der Slave nie als Ansage-Gerät meldet, sieht
        // die Turnierleitung die Warnung „kein Ansage-Gerät verbunden" und
        // weiß, dass sie per Funk rufen muss. Siehe docs/announcements.md.
        if !config.master_namespace.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut url = reqwest::Url::parse(&format!(
            "http://{}:8088/info/announce/jobs",
            config.btp.host
        ))
        .map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("hall", &hall)
            .append_pair("since", &since.to_string());
        // Kurzer Timeout wie beim Freitext-Poll: Der Master antwortet im LAN
        // sofort oder gar nicht.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .connect_timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| push::build_client());
        let resp = match client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return Ok(Vec::new()),
        };
        resp.json::<Vec<crate::tablet::state::AnnounceJob>>()
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(state.tablet.announce_jobs_since(&hall, since, now_ms()))
    }
}

/// Ein Feld im Cloud-Ansage-Status (frontend-freundlich, wie `CourtOverview`
/// fürs Ansagen): Feldname + aktuelle Paarung.
#[derive(serde::Serialize)]
pub struct CloudAnnounceCourt {
    pub court_id: i64,
    pub court: String,
    pub discipline: String,
    /// Klassen-Kürzel („A", „B", …) für „Herreneinzel A" (leer = keins).
    pub class_label: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    pub team1_nationalities: Vec<String>,
    pub team2_nationalities: Vec<String>,
    pub match_id: i64,
    /// Zähltafelbediener (ADR 0007) + ob er zugewiesen ist — der Slave sagt ihn
    /// nur bei `scorekeeper_assigned` mit an.
    pub scorekeeper: Vec<String>,
    pub scorekeeper_assigned: bool,
}

#[derive(serde::Serialize)]
pub struct CloudFreetext {
    pub id: u64,
    pub hall: String,
    pub text: String,
}

/// Ein aufgerufenes Spiel für die Slave-Spielübersicht + den Nachruf am Slave
/// (Cluster C Stufe 2), frontend-freundlich wie `PreparationCandidate`.
#[derive(serde::Serialize)]
pub struct CloudPrepared {
    pub match_id: i64,
    pub hall: String,
    pub discipline: String,
    pub class_label: String,
    pub round_name: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    pub team1_nationalities: Vec<String>,
    pub team2_nationalities: Vec<String>,
    pub called_at_ms: u64,
}

#[derive(serde::Serialize)]
pub struct CloudAnnounce {
    pub courts: Vec<CloudAnnounceCourt>,
    pub freetext: Vec<CloudFreetext>,
    /// Aufgerufene Spiele der eigenen Halle (Slave-Spielübersicht + Nachruf,
    /// Cluster C Stufe 2). Leer bei altem Relay/Master oder ohne Aufrufe.
    pub prepared: Vec<CloudPrepared>,
    /// Stimme der vom Master geerbten Azure-Config (ADR 0003), `None` ohne
    /// Vererbung. Bewusst nur die Stimme: der Key bleibt im Backend.
    pub azure_voice: Option<String>,
    /// Vom Master geerbte Stimme je Disziplin (Disziplin-Kürzel → Stimme).
    /// Leer ohne Vererbung/ohne Zuordnung → der Slave nutzt die Standard-Stimme.
    pub azure_discipline_voices: std::collections::HashMap<String, String>,
}

/// Cloud-Ansage-Slave (B1a): holt aus dem Cloud-Relay des Masters die Matches
/// der eigenen Halle (für die Auto-Feld-Ansage) + neue Freitext-Ansagen. Leer,
/// wenn nicht als Cloud-Slave konfiguriert (kein `slave_mode`/`master_namespace`).
#[tauri::command]
pub async fn cloud_announce_state(
    state: State<'_, AppState>,
    since: u64,
) -> Result<CloudAnnounce, String> {
    let (ns, hall, slave_id) = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        if !cfg.slave_mode || cfg.master_namespace.trim().is_empty() {
            return Ok(CloudAnnounce {
                courts: Vec::new(),
                freetext: Vec::new(),
                prepared: Vec::new(),
                azure_voice: None,
                azure_discipline_voices: std::collections::HashMap::new(),
            });
        }
        (
            cfg.master_namespace.clone(),
            cfg.announce.announce_hall.clone(),
            cfg.install_id.clone(),
        )
    };
    let fetched =
        crate::tablet::relay_client::fetch_announce_state(&ns, &hall, since, &slave_id).await;
    // Geerbte Azure-Config nur bei erfolgreichem Poll übernehmen – ein
    // Netz-Aussetzer soll die Vererbung nicht verwerfen (ADR 0003). Ein
    // erfolgreicher Poll ist dagegen autoritativ, auch wenn er `None`
    // liefert (Azure am Master deaktiviert).
    // Geerbte Stimme UND Disziplin-Stimmen aus derselben Master-Config ableiten,
    // damit die ferne Halle exakt dieselbe Zuordnung nutzt wie der Master.
    let azure_voice;
    let azure_discipline_voices;
    match &fetched {
        Some(st) => {
            azure_voice = st.azure_tts.as_ref().map(|a| a.voice.clone());
            azure_discipline_voices = st
                .azure_tts
                .as_ref()
                .map(|a| a.discipline_voices.clone())
                .unwrap_or_default();
            *state
                .inherited_azure
                .lock()
                .expect("inherited_azure-Mutex nicht vergiftet") = st.azure_tts.clone();
        }
        None => {
            let guard = state
                .inherited_azure
                .lock()
                .expect("inherited_azure-Mutex nicht vergiftet");
            azure_voice = guard.as_ref().map(|a| a.voice.clone());
            azure_discipline_voices = guard
                .as_ref()
                .map(|a| a.discipline_voices.clone())
                .unwrap_or_default();
        }
    };
    let st = fetched.unwrap_or_default();

    let names = |v: &[relay_proto::PlayerBrief]| v.iter().map(|p| p.name.clone()).collect();
    let nats = |v: &[relay_proto::PlayerBrief]| {
        v.iter()
            .map(|p| p.nationality.clone().unwrap_or_default())
            .collect()
    };
    let courts = st
        .courts
        .into_iter()
        .filter_map(|c| {
            let m = c.match_brief?;
            // Anzeige-Label ist bei Mehr-Hallen "{Halle} · {Feld}" – fürs
            // Ansagen nur den Feldteil verwenden.
            let court = c.label.rsplit(" · ").next().unwrap_or(&c.label).to_string();
            Some(CloudAnnounceCourt {
                court_id: c.court_id,
                court,
                discipline: m.discipline.clone(),
                class_label: m.class_label.clone(),
                team1: names(&m.team_a),
                team2: names(&m.team_b),
                team1_nationalities: nats(&m.team_a),
                team2_nationalities: nats(&m.team_b),
                match_id: m.match_id,
                scorekeeper: m.scorekeeper.clone(),
                scorekeeper_assigned: m.scorekeeper_assigned,
            })
        })
        .collect();
    let freetext = st
        .freetext
        .into_iter()
        .map(|f| CloudFreetext {
            id: f.id,
            hall: f.hall,
            text: f.text,
        })
        .collect();
    let prepared = st
        .prepared
        .into_iter()
        .map(|p| CloudPrepared {
            match_id: p.match_id,
            hall: p.hall,
            discipline: p.discipline,
            class_label: p.class_label,
            round_name: p.round_name,
            team1: names(&p.team_a),
            team2: names(&p.team_b),
            team1_nationalities: nats(&p.team_a),
            team2_nationalities: nats(&p.team_b),
            called_at_ms: p.called_at_ms,
        })
        .collect();
    Ok(CloudAnnounce {
        courts,
        freetext,
        prepared,
        azure_voice,
        azure_discipline_voices,
    })
}

/// Master: bekannte Cloud-Ansage-Slaves (ferne Hallen) samt Online-Status, für
/// die Kopfzeilen-Anzeige. Leer, wenn dieser PC kein Cloud-Master ist.
#[tauri::command]
pub async fn cloud_slaves(
    state: State<'_, AppState>,
) -> Result<Vec<relay_proto::SlaveInfo>, String> {
    let ns = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        // Nur ein Cloud-Master (nicht Slave) hat ferne Hallen zu zeigen.
        if cfg.slave_mode || !cfg.connection_mode.cloud_enabled() {
            return Ok(Vec::new());
        }
        cfg.install_id.clone()
    };
    Ok(crate::tablet::relay_client::fetch_slaves(&ns).await)
}

/// Master: kurzlebigen 8-stelligen Telefon-Kopplungscode beim Relay
/// anfordern (ADR 0004) — zum Durchsagen an die ferne Halle. 1 Stunde
/// gültig; ein neuer Code ersetzt den alten.
#[tauri::command]
pub async fn pairing_code(state: State<'_, AppState>) -> Result<relay_proto::PairingCode, String> {
    let ns = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        cfg.install_id.clone()
    };
    crate::tablet::relay_client::request_pairing_code(&ns).await
}

/// Slave: 8-stelligen Telefon-Kopplungscode gegen den vollen
/// Master-Kopplungs-Code einlösen (ADR 0004). Liefert den Namespace, den
/// das Frontend als `master_namespace` speichert.
#[tauri::command]
pub async fn resolve_pairing_code(code: String) -> Result<String, String> {
    crate::tablet::relay_client::resolve_pairing_code(code.trim()).await
}

// ───────────────── Turnierleitungs-Geräte (TL-Web) ─────────────────

/// Ein gekoppeltes Turnierleitungs-Gerät, **ohne** seinen Zugang.
///
/// Der Zugang verlässt die Konfiguration nur einmal: im QR-Code beim
/// Koppeln. Danach gibt es keinen Weg mehr, ihn anzuzeigen — auch nicht für
/// den Turnierleiter. Wer sein Gerät verliert, koppelt neu; das ist der
/// kürzere Weg als ein Zugang, der in jeder Geräteliste steht.
#[derive(Serialize)]
pub struct TlDeviceInfo {
    pub id: String,
    pub label: String,
    pub hall: String,
    pub created_at_ms: u64,
}

/// Die Geräteliste für die Oberfläche.
#[derive(Serialize)]
pub struct TlWebInfo {
    pub enabled: bool,
    pub devices: Vec<TlDeviceInfo>,
    /// Wie viele Kopplungen die Liste fassen kann.
    pub max_devices: usize,
    /// Wie viele Geräte die Seite **gleichzeitig** offen haben können. Die
    /// spürbare Grenze — und eine ganz andere als die Listenlänge: Alte
    /// Kopplungen zählen in der Liste mit, blockieren aber keinen Platz.
    pub max_online: usize,
}

#[tauri::command]
pub fn tl_web_info(state: State<'_, AppState>) -> TlWebInfo {
    let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
    TlWebInfo {
        enabled: cfg.tl_web.enabled,
        devices: cfg
            .tl_web
            .devices
            .iter()
            .map(|d| TlDeviceInfo {
                id: d.id.clone(),
                label: d.label.clone(),
                hall: d.hall.clone(),
                created_at_ms: d.created_at_ms,
            })
            .collect(),
        max_devices: relay_proto::MAX_TL_DEVICES_MIRRORED,
        max_online: relay_proto::MAX_TL_DEVICES_ONLINE,
    }
}

/// Ein Weg, auf dem ein Gerät die Oberfläche erreicht.
#[derive(Serialize)]
pub struct TlEntrance {
    /// Was dransteht („Im Hallennetz" / „Über das Internet").
    pub label: String,
    /// Die vollständige Adresse **mit Zugang im Fragment**. Das Fragment
    /// schickt kein Browser an einen Server — der Zugang steht damit weder
    /// im Zugriffsprotokoll des Relays noch in dem eines Zwischenservers.
    pub url: String,
    /// Derselbe Inhalt als QR-Code (SVG). Wird **hier** erzeugt, nicht über
    /// eine Bild-Route: Ein Zugang, der als Adressbestandteil an einen
    /// Server ginge, stünde in dessen Protokoll.
    pub qr_svg: String,
}

/// Was ein frisch gekoppeltes Gerät zum Anmelden braucht.
#[derive(Serialize)]
pub struct TlPairing {
    pub id: String,
    /// **Alle** Wege, auf denen dieses Gerät hereinkommt. Im
    /// LAN-und-Cloud-Betrieb sind es zwei — und beide werden gebraucht: Der
    /// Sinn dieser Betriebsart ist, dass die Halle weiterläuft, wenn die
    /// Internetverbindung ausfällt. Stünde nur die Cloud-Adresse im QR,
    /// stünde das Gerät bei einem Ausfall vor einer Seite, die es nicht mehr
    /// laden kann — und der Zugang ist nur dieses eine Mal zu sehen.
    pub entrances: Vec<TlEntrance>,
}

/// Koppelt ein neues Turnierleitungs-Gerät und liefert Adresse + QR-Code.
///
/// `token` und `id` erzeugt die Oberfläche mit `crypto.randomUUID()` —
/// derselbe Weg wie bei der `install_id`.
#[tauri::command]
pub fn tl_device_add(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    token: String,
    label: String,
    hall: String,
) -> Result<(TlPairing, AppConfig), String> {
    let device = crate::config::TlDevice {
        id: id.trim().to_string(),
        token: token.trim().to_string(),
        label: label.trim().chars().take(60).collect(),
        created_at_ms: now_ms(),
        hall: hall.trim().to_string(),
        // Neu gekoppelte Geräte starten ohne Profilbindung — sie zeigen das
        // turnierweite Standardprofil, bis eine Turnierleitung eines wählt
        // (Spec tl-web-panelsystem).
        profile_id: String::new(),
    };
    let neu = device.clone();
    let cfg = mutate_config(&app, &state, move |cfg| {
        cfg.tl_web.add_device(neu)?;
        // Ein Gerät zu koppeln heißt, die Oberfläche zu wollen.
        cfg.tl_web.enabled = true;
        Ok(())
    })?;
    Ok((
        TlPairing {
            id: device.id,
            entrances: tl_entrances(&cfg, &device.token)?,
        },
        cfg,
    ))
}

/// Entzieht einem Gerät den Zugang.
#[tauri::command]
pub fn tl_device_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<AppConfig, String> {
    mutate_config(&app, &state, move |cfg| {
        if cfg.tl_web.remove_device(id.trim()) {
            Ok(())
        } else {
            Err("Dieses Gerät ist nicht (mehr) gekoppelt.".to_string())
        }
    })
}

/// Schaltet die Turnierleitungs-Oberfläche an oder ab.
///
/// Abschalten **behält** die Geräte: Ein versehentlicher Klick soll nicht
/// bedeuten, dass alle Tablets neu gescannt werden müssen. Wirksam ist der
/// Schalter trotzdem sofort — ohne ihn erreicht keine Anfrage etwas, und im
/// Cloud-Betrieb pusht der Turnier-PC eine leere Liste.
#[tauri::command]
pub fn tl_web_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppConfig, String> {
    mutate_config(&app, &state, move |cfg| {
        cfg.tl_web.enabled = enabled;
        Ok(())
    })
}

/// Legt die Raster-Anordnung einer Halle fest (oder ersetzt sie).
///
/// Validierung + Normalisierung (Trimmen, Groß-/Kleinschreibung-unabhängiger
/// Ersatz) steckt testbar in `AppConfig::upsert_hall_layout` — der Command
/// ist nur der dünne `mutate_config`-Wrapper.
#[tauri::command]
pub fn set_hall_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    layout: crate::config::HallLayoutConfig,
) -> Result<AppConfig, String> {
    mutate_config(&app, &state, move |cfg| cfg.upsert_hall_layout(layout))
}

/// Entfernt die Anordnung einer Halle — zurück zur Fließ-Darstellung.
#[tauri::command]
pub fn remove_hall_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    hall: String,
) -> Result<AppConfig, String> {
    mutate_config(&app, &state, move |cfg| {
        cfg.remove_hall_layout(&hall);
        Ok(())
    })
}

/// Ändert die Konfiguration **unter durchgehend gehaltener Sperre** und
/// liefert den neuen Stand.
///
/// Lesen, Ändern und Schreiben in einem Zug: Wer zwischendurch loslässt,
/// überschreibt eine Änderung, die in genau diesem Fenster gespeichert
/// wurde — bei Zugängen hieße das, eine frische Kopplung verschwindet
/// wieder.
///
/// Den neuen Stand zurückzugeben ist kein Luxus: Die Oberfläche hält eine
/// eigene Kopie der Konfiguration. Bliebe die veraltet, schickte der nächste
/// Speichervorgang aus den Einstellungen den alten `tl_web`-Stand zurück —
/// und `keep_host_managed_fields` löschte alle Kopplungen, deren Zugänge
/// niemand wiederherstellen kann.
fn mutate_config<F>(
    app: &AppHandle,
    state: &State<'_, AppState>,
    aendern: F,
) -> Result<AppConfig, String>
where
    F: FnOnce(&mut AppConfig) -> Result<(), String>,
{
    mutate_config_at(&config_path(app), &state.config, aendern)
}

/// Kernlogik von [`mutate_config`], ohne `AppHandle`/`State` — dieselbe
/// Sperre (`shared`), derselbe Lesen-Ändern-Schreiben-Zyklus, nur ohne die
/// Tauri-Anbindung. Existiert, damit Tests aus anderen Modulen (namentlich
/// `tablet::server`) den **echten** Tauri-Command-Schreibpfad gegen den
/// echten `ServerCtx::mutate_app_config`-Schreibpfad testen können, statt
/// beide Male dieselbe Logik ein zweites Mal nachzubauen — genau das
/// Lost-Update-Regressionsszenario (kritischer Review-Fund, s. Feld-
/// Kommentar `AppState.config`).
pub(crate) fn mutate_config_at<F>(
    config_path: &std::path::Path,
    shared: &Arc<Mutex<AppConfig>>,
    aendern: F,
) -> Result<AppConfig, String>
where
    F: FnOnce(&mut AppConfig) -> Result<(), String>,
{
    let mut guard = shared.lock().expect("Config-Mutex nicht vergiftet");
    let mut cfg = guard.clone();
    aendern(&mut cfg)?;
    cfg.save_to(config_path).map_err(|e| e.to_string())?;
    *guard = cfg.clone();
    Ok(cfg)
}

/// Alle Wege, auf denen ein Gerät die Oberfläche erreicht.
///
/// Im Cloud-Betrieb ohne Namespace in der Adresse (der wäre die
/// `install_id` und damit zugleich der Zugang der Zähltablets); im
/// Hallennetz der eingebettete Server. Läuft **beides**, gibt es beide
/// Wege — und das ist wichtig: Der Sinn dieser Betriebsart ist, dass die
/// Halle weiterläuft, wenn die Internetverbindung ausfällt.
fn tl_entrances(cfg: &AppConfig, token: &str) -> Result<Vec<TlEntrance>, String> {
    let mut wege = Vec::new();
    if cfg.connection_mode.lan_enabled() {
        wege.push((
            "Im Hallennetz",
            format!("http://{}/tl#t={token}", crate::tablet::server::lan_host()),
        ));
    }
    if cfg.connection_mode.cloud_enabled() {
        wege.push((
            "Über das Internet",
            format!("https://badhub.de/bts-relay/tl#t={token}"),
        ));
    }
    wege.into_iter()
        .map(|(label, url)| {
            let qr_svg = qr_code_svg(&url)?;
            Ok(TlEntrance {
                label: label.to_string(),
                url,
                qr_svg,
            })
        })
        .collect()
}

/// QR-Code als SVG — lokal erzeugt, damit der Zugang den Rechner nicht
/// verlässt.
fn qr_code_svg(text: &str) -> Result<String, String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(260, 260)
        .build())
}

/// Geräte-Anschluss der fernen Halle (Slave): Relay-Basis des Masters +
/// die Felder **dieser** Halle, damit die Slave-Oberfläche je Feld den
/// Tablet-QR (`<relay_base>/qr/<id>`) und den Monitor-Link
/// (`<relay_base>/court/<id>/display`) zeigen kann. Leer, wenn dieser PC kein
/// Cloud-Slave ist (`slave_mode` + `master_namespace`).
#[derive(Serialize)]
pub struct SlaveDeviceInfo {
    /// Relay-Basis des Master-Namespace (`https://badhub.de/bts-relay/<master_ns>`).
    pub relay_base: String,
    /// Alle im Turnier erkannten Hallennamen (aus der Relay-Feldliste) —
    /// Optionen für die Hallen-Auswahl auf dem Slave. Der Cloud-Slave hat kein
    /// BTP und kann die Hallennamen nur hierüber verlässlich anbieten. Die
    /// gewählte Halle selbst liest das Frontend aus der Config (`announce_hall`).
    pub all_halls: Vec<String>,
    /// Felder der eigenen Halle: `id`, `label` (Anzeige), `hall`. Bei noch nicht
    /// gewählter Halle alle Felder (dann greift die Hallen-Auswahl davor).
    pub courts: Vec<relay_proto::CourtBrief>,
}

#[tauri::command]
pub async fn slave_devices(state: State<'_, AppState>) -> Result<SlaveDeviceInfo, String> {
    let (ns, hall) = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        let ns = cfg.master_namespace.trim().to_string();
        // Kein Slave / kein Code – oder ein syntaktisch ungültiger, vom Nutzer
        // eingegebener Code: leeren Zustand liefern statt eine URL aus
        // ungeprüftem Input zu bauen (der Code fließt in relay_base + QR-/
        // Monitor-Links; `.`/`/` würden Pfad-Confusion erlauben).
        if !cfg.slave_mode || !crate::tablet::relay_client::valid_relay_namespace(&ns) {
            return Ok(SlaveDeviceInfo {
                relay_base: String::new(),
                all_halls: Vec::new(),
                courts: Vec::new(),
            });
        }
        (ns, cfg.announce.announce_hall.clone())
    };
    let all = crate::tablet::relay_client::fetch_courts(&ns).await;
    // Hallen-Optionen aus der VOLLEN Feldliste (vor dem Filter) — damit die
    // Auswahl auf dem Slave alle Hallen zeigt, auch die noch nicht gewählte.
    let all_halls = relay_proto::distinct_halls(&all);
    // Auf die eigene Halle filtern (gleiche Logik wie die hallengefilterte
    // Ansage: leere Slave-Halle oder leere Feld-Halle = kein Filter).
    let courts: Vec<relay_proto::CourtBrief> = all
        .into_iter()
        .filter(|c| hall.is_empty() || c.hall.is_empty() || c.hall == hall)
        .collect();
    let relay_base = format!("https://badhub.de/bts-relay/{ns}");
    Ok(SlaveDeviceInfo {
        relay_base,
        all_halls,
        courts,
    })
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

// ───────────────── Zähltafelbediener-Warteschlange (ADR 0007, Phase 1) ─────

/// Aktuelle Zähltafelbediener-Warteschlange (FIFO) für die Warteliste-Anzeige.
#[tauri::command]
pub fn scorekeeper_queue(
    state: State<'_, AppState>,
) -> Vec<crate::tablet::state::ScorekeeperEntry> {
    state.tablet.scorekeeper_queue()
}

/// Einen Wartenden aus der Schlange entfernen (manuelle Pflege).
#[tauri::command]
pub fn remove_scorekeeper(state: State<'_, AppState>, key: String) {
    state.tablet.remove_scorekeeper(&key);
}

/// Einen Wartenden an den Anfang ziehen (als Nächsten dran).
#[tauri::command]
pub fn advance_scorekeeper(state: State<'_, AppState>, key: String) {
    state.tablet.advance_scorekeeper(&key);
}

/// Manuell einen Zähltafelbediener hinzufügen (nicht aus einem Spielende).
#[tauri::command]
pub fn add_scorekeeper(state: State<'_, AppState>, names: Vec<String>) {
    state.tablet.add_scorekeeper_manual(names, now_ms());
}

// ──────────────── Schiedsrichter (Spec schiedsrichter-management) ─────────

/// Ein Official für die Bedienoberfläche: BTP-Stammdaten plus die in
/// BTS Light gepflegten Zusatzdaten. Die **Inhalte** der Sperrlisten sind
/// bewusst nicht dabei (nur ihre Anzahl) — sie kommen auf gezielte Anfrage
/// über [`official_blocklists`], damit sie nicht in jeder Listen-Abfrage
/// mitreisen.
#[derive(Serialize)]
pub struct OfficialView {
    pub id: i64,
    /// Anzeigename „Vorname Nachname" aus BTP.
    pub name: String,
    /// Position in der Rotationsreihenfolge (0-basiert).
    pub position: usize,
    pub paused: bool,
    /// In BTS Light gepflegter Stammverein (BTP liefert keinen).
    pub club: String,
    /// Anzahl gesperrter Vereine + Spieler (nur die Zahl).
    pub blocked_count: usize,
    /// Feld-ID, auf der er gerade Dienst tut, plus Rolle — sonst `None`.
    pub on_duty_court_id: Option<i64>,
    pub on_duty_role: Option<String>,
    /// Zahl der bisherigen Einsätze (aus den beendeten Spielen abgeleitet).
    pub appearances: usize,
}

/// Ein abgeleiteter Einsatz für das Detail-Overlay.
#[derive(Serialize)]
pub struct AppearanceView {
    pub match_id: i64,
    /// „sr" oder „ar".
    pub role: String,
    /// Spielbezeichnung, z. B. „HE VF".
    pub match_name: String,
    /// Feldname, falls BTP das Feld noch führt.
    pub court: String,
    /// Endezeit in Unix-ms.
    pub finished_at: Option<u64>,
}

/// Die Sperrlisten eines Officials (Personendaten — nur auf Anfrage),
/// zusammen mit den Auswahllisten für die Pflege.
#[derive(Serialize)]
pub struct BlocklistView {
    pub clubs: Vec<String>,
    pub players: Vec<i64>,
    /// Alle Spieler des Turniers zur Auswahl (statt PlayerID-Tipperei).
    pub pick_players: Vec<crate::tablet::officials::PickPlayer>,
    /// Alle Vereine des Turniers zur Auswahl.
    pub pick_clubs: Vec<String>,
}

/// Feldweise Schalter für die Bedienoberfläche.
#[derive(Serialize)]
pub struct CourtSwitchesView {
    pub court_id: i64,
    pub court: String,
    pub sr: bool,
    pub ar: bool,
    pub operator: bool,
}

/// Läuft dieses Turnier mit Schiedsrichtern? Schreibende Officials-Commands
/// beginnen damit — sonst landeten Zusatzdaten (darunter Sperrlisten, also
/// Personendaten) in der Turnierdatei eines Turniers ohne Schiedsrichter.
fn officials_an(state: &State<'_, AppState>) -> Result<(), String> {
    if state.tablet.officials_store().enabled() {
        return Ok(());
    }
    Err("Dieses Turnier läuft ohne Schiedsrichter.".to_string())
}

/// „sr"/„ar" in die Rolle übersetzen; alles andere ist ein Bedienfehler.
fn parse_role(role: &str) -> Result<crate::tablet::officials::OfficialRole, String> {
    match role {
        "sr" => Ok(crate::tablet::officials::OfficialRole::Sr),
        "ar" => Ok(crate::tablet::officials::OfficialRole::Ar),
        _ => Err(format!("unbekannte Rolle: {role}")),
    }
}

fn role_str(role: crate::tablet::officials::OfficialRole) -> String {
    match role {
        crate::tablet::officials::OfficialRole::Sr => "sr".to_string(),
        crate::tablet::officials::OfficialRole::Ar => "ar".to_string(),
    }
}

/// Beendete Spiele des Snapshots in der Form, die die Einsatz-Ableitung
/// braucht.
fn officials_finished_input(
    snap: &crate::btp::model::BtpSnapshot,
) -> Vec<crate::tablet::officials::FinishedMatch> {
    snap.matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::Finished)
        .map(|m| crate::tablet::officials::FinishedMatch {
            match_id: m.id,
            btp_sr: m.official1_id,
            btp_ar: m.official2_id,
            court_id: m.court_id,
            finished_at: m.finished_at,
        })
        .collect()
}

/// Die Schiedsrichterliste des Turniers in Rotationsreihenfolge, angereichert
/// um Zusatzdaten, Dienst und Einsatz-Zähler. Leer, wenn BTP keine Officials
/// führt oder ohne Schiedsrichter gespielt wird.
#[tauri::command]
pub fn officials_roster(state: State<'_, AppState>) -> Vec<OfficialView> {
    let store = state.tablet.officials_store();
    let Some(snap) = state.tablet.snapshot_clone() else {
        return Vec::new();
    };
    let einsaetze = store.appearances(&officials_finished_input(&snap));
    // Wer tut gerade wo Dienst? Aus den laufenden Spielen.
    let mut dienst: std::collections::HashMap<i64, (i64, String)> =
        std::collections::HashMap::new();
    for m in snap
        .matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::OnCourt)
    {
        let w = store.effective(m.id, m.official1_id, m.official2_id);
        let Some(court_id) = m.court_id else { continue };
        if let Some(id) = w.sr {
            dienst.insert(id, (court_id, "sr".to_string()));
        }
        if let Some(id) = w.ar {
            dienst.insert(id, (court_id, "ar".to_string()));
        }
    }
    let reihenfolge = store.order();
    let mut out: Vec<OfficialView> = snap
        .officials
        .iter()
        .map(|o| {
            let extra = store.extra(o.id);
            let position = reihenfolge
                .iter()
                .position(|id| *id == o.id)
                .unwrap_or(usize::MAX);
            OfficialView {
                id: o.id,
                name: o.display_name(),
                position,
                paused: extra.paused,
                club: extra.club,
                blocked_count: extra.blocked_clubs.len() + extra.blocked_players.len(),
                on_duty_court_id: dienst.get(&o.id).map(|(c, _)| *c),
                on_duty_role: dienst.get(&o.id).map(|(_, r)| r.clone()),
                appearances: einsaetze.get(&o.id).map(Vec::len).unwrap_or(0),
            }
        })
        .collect();
    // In Rotationsreihenfolge ausliefern — das ist die Reihenfolge, in der
    // die Turnierleitung sie zugeteilt bekommt.
    out.sort_by_key(|v| (v.position, v.id));
    out
}

/// Einen Official einem Spiel zuweisen. Gibt die Konflikt-Kategorie zurück,
/// falls einer besteht: Die Zuweisung wird **trotzdem ausgeführt** (Spec
/// Nr. 2) — die Turnierleitung entscheidet, nicht die App.
#[tauri::command]
pub fn official_assign(
    state: State<'_, AppState>,
    match_id: i64,
    role: String,
    official_id: i64,
) -> Result<Option<String>, String> {
    officials_an(&state)?;
    let role = parse_role(&role)?;
    let store = state.tablet.officials_store();
    store.assign(match_id, role, official_id);
    let warnung = state.tablet.snapshot_clone().and_then(|snap| {
        let m = snap.matches.iter().find(|m| m.id == match_id)?;
        let spieler: Vec<crate::btp::model::BtpPlayer> =
            m.team1.iter().chain(m.team2.iter()).cloned().collect();
        crate::tablet::officials::official_conflict(&store.extra(official_id), &spieler)
            .map(|k| k.label().to_string())
    });
    Ok(warnung)
}

/// Eine Zuweisung lösen.
#[tauri::command]
pub fn official_clear(
    state: State<'_, AppState>,
    match_id: i64,
    role: String,
) -> Result<(), String> {
    officials_an(&state)?;
    state
        .tablet
        .officials_store()
        .clear_assignment(match_id, parse_role(&role)?);
    Ok(())
}

/// Einen Official pausieren oder wieder aktivieren (Pause, kommt später,
/// geht früher). Seine Position in der Reihenfolge bleibt.
#[tauri::command]
pub fn official_pause(
    state: State<'_, AppState>,
    official_id: i64,
    paused: bool,
) -> Result<(), String> {
    officials_an(&state)?;
    state
        .tablet
        .officials_store()
        .set_paused(official_id, paused);
    Ok(())
}

/// Ein Spiel von der automatischen Feldvergabe ausnehmen oder die Ausnahme
/// zurücknehmen (Spec `feldvergabe-ausnahme`). Derselbe Speicher wie der
/// TL-Web-Weg (`TlAction::ExcludeFromAutoAssign`) — beide Wege mutieren
/// `TabletState::set_auto_assign_excluded`, keine BTP-Rückschreibung.
/// Unabhängig vom Schiedsrichter-Betrieb, deshalb ohne `officials_an`-Gate.
#[tauri::command]
pub fn auto_assign_exclude(
    state: State<'_, AppState>,
    match_id: i64,
    excluded: bool,
) -> Result<(), String> {
    state.tablet.set_auto_assign_excluded(match_id, excluded);
    Ok(())
}

/// Ein noch nicht gerufenes Spiel in der manuellen Präfix-Reihenfolge
/// seiner Halle vor ein anderes ziehen (Spec
/// `spielliste-manuelle-reihenfolge`, ADR 0023). Derselbe Einstiegspunkt
/// wie der TL-Web-Weg (`TlAction::QueueReorder`) —
/// `TabletState::queue_reorder` leitet die Halle selbst aus dem Match ab.
#[tauri::command]
pub fn queue_reorder(
    state: State<'_, AppState>,
    match_id: i64,
    before_match_id: Option<i64>,
) -> Result<(), String> {
    let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
    state.tablet.queue_reorder(&cfg, match_id, before_match_id);
    Ok(())
}

/// Die manuelle Spielreihenfolge **aller** Hallen auf einmal verwerfen
/// (globaler Reset-Knopf, Spec `spielliste-manuelle-reihenfolge`).
#[tauri::command]
pub fn queue_order_reset(state: State<'_, AppState>) -> Result<(), String> {
    state.tablet.queue_order_reset();
    Ok(())
}

/// Einen Official in der Reihenfolge vor einen anderen ziehen
/// (`before_official_id` weggelassen ⇒ ans Ende).
#[tauri::command]
pub fn official_reorder(
    state: State<'_, AppState>,
    official_id: i64,
    before_official_id: Option<i64>,
) -> Result<(), String> {
    officials_an(&state)?;
    state
        .tablet
        .officials_store()
        .reorder(official_id, before_official_id);
    Ok(())
}

/// Stammverein pflegen (BTP liefert am Official keinen — Messung 13.08.2026).
#[tauri::command]
pub fn official_set_club(
    state: State<'_, AppState>,
    official_id: i64,
    club: String,
) -> Result<(), String> {
    officials_an(&state)?;
    state.tablet.officials_store().set_club(official_id, &club);
    Ok(())
}

/// Die Sperrlisten eines Officials — **nur auf gezielte Anfrage**, damit
/// diese Personendaten nicht in jeder Roster-Abfrage mitreisen.
#[tauri::command]
pub fn official_blocklists(state: State<'_, AppState>, official_id: i64) -> BlocklistView {
    let extra = state.tablet.officials_store().extra(official_id);
    // Die Auswahllisten kommen mit derselben Antwort: Der Dialog wird
    // bewusst geöffnet, ein zweiter Rundlauf brächte nichts.
    let (pick_players, pick_clubs) = state
        .tablet
        .snapshot_clone()
        .map(|snap| crate::tablet::officials::pick_lists(&snap.entries))
        .unwrap_or_default();
    BlocklistView {
        clubs: extra.blocked_clubs,
        players: extra.blocked_players,
        pick_players,
        pick_clubs,
    }
}

/// Sperrlisten setzen (ersetzt beide Listen).
#[tauri::command]
pub fn official_set_blocklists(
    state: State<'_, AppState>,
    official_id: i64,
    clubs: Vec<String>,
    players: Vec<i64>,
) -> Result<(), String> {
    officials_an(&state)?;
    state
        .tablet
        .officials_store()
        .set_blocklists(official_id, clubs, players);
    Ok(())
}

/// Die Einsätze eines Officials im Detail (Spiel, Rolle, Feld, Endezeit) —
/// abgeleitet aus den beendeten Spielen, ohne eigene Historien-Datenhaltung.
#[tauri::command]
pub fn official_appearances(state: State<'_, AppState>, official_id: i64) -> Vec<AppearanceView> {
    let Some(snap) = state.tablet.snapshot_clone() else {
        return Vec::new();
    };
    let store = state.tablet.officials_store();
    let alle = store.appearances(&officials_finished_input(&snap));
    alle.get(&official_id)
        .map(|liste| {
            liste
                .iter()
                .map(|a| {
                    let m = snap.matches.iter().find(|m| m.id == a.match_id);
                    AppearanceView {
                        match_id: a.match_id,
                        role: role_str(a.role),
                        match_name: m
                            .map(|m| {
                                format!("{} {}", m.draw_name, m.round_name)
                                    .trim()
                                    .to_string()
                            })
                            .unwrap_or_default(),
                        court: a
                            .court_id
                            .and_then(|c| snap.court_infos.iter().find(|ci| ci.id == c))
                            .map(|ci| ci.name.clone())
                            .unwrap_or_default(),
                        finished_at: a.finished_at,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Die feldweisen Schalter aller Felder (Default: alles aktiv).
#[tauri::command]
pub fn officials_court_switches(state: State<'_, AppState>) -> Vec<CourtSwitchesView> {
    let store = state.tablet.officials_store();
    let Some(snap) = state.tablet.snapshot_clone() else {
        return Vec::new();
    };
    snap.court_infos
        .iter()
        .map(|c| {
            let s = store.court_switches(c.id);
            CourtSwitchesView {
                court_id: c.id,
                court: c.name.clone(),
                sr: s.sr,
                ar: s.ar,
                operator: s.operator,
            }
        })
        .collect()
}

/// Feldweise Schalter setzen.
#[tauri::command]
pub fn officials_set_court_switches(
    state: State<'_, AppState>,
    court_id: i64,
    sr: bool,
    ar: bool,
    operator: bool,
) -> Result<(), String> {
    officials_an(&state)?;
    state.tablet.officials_store().set_court_switches(
        court_id,
        crate::tablet::officials::CourtSwitches { sr, ar, operator },
    );
    Ok(())
}

// ───────────────────────────── Siegerehrung ───────────────────────────────

/// Podien der ausgespielten Disziplinen + aktuell gewählte Disziplin – für
/// die Steuerung der Siegerehrung in der Monitor-Verwaltung.
#[derive(Serialize)]
pub struct WinnersView {
    pub disciplines: Vec<crate::tablet::winners::DisciplineResult>,
    /// Draw-ID der aktuell auf dem Sieger-Monitor gezeigten Disziplin (oder
    /// `None`, wenn nichts gewählt ist).
    pub selected: Option<i64>,
}

/// Liefert alle ausgespielten Disziplinen (mit Podium) und die aktuell für die
/// Siegerehrung gewählte Disziplin.
#[tauri::command]
pub fn winners_overview(state: State<'_, AppState>) -> WinnersView {
    WinnersView {
        disciplines: state.tablet.discipline_results(),
        selected: state.tablet.winners_selection(),
    }
}

/// Wählt die auf dem Sieger-Monitor gezeigte Disziplin (`None` = nichts/
/// Begrüßungsbild). Steuert die Siegerehrung — bewusst nicht rotierend.
#[tauri::command]
pub fn set_winners_selection(state: State<'_, AppState>, draw_id: Option<i64>) {
    state.tablet.set_winners_selection(draw_id);
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

/// Maximale Logo-Größe. Ein Logo ist klein; 2 MB sind großzügig und halten
/// den Liveticker-Payload (Base64 wandert in JEDEN vollen `tset`) schlank.
const MAX_LOGO_BYTES: u64 = 2 * 1024 * 1024;

/// Base64-Bilddaten + MIME einer gewählten Logo-Datei.
#[derive(Serialize)]
pub struct LogoData {
    pub data: String,
    pub mime: String,
}

/// Liest eine vom Operator gewählte Bilddatei und liefert sie Base64-kodiert
/// samt MIME zurück. Das Frontend legt das Ergebnis in `config.tournament_logo`
/// ab (per `save_config`); von dort schickt es der Sync im `tset`-Event an
/// badhub, wo `#live-logo` es anzeigt. BTP liefert kein Logo – daher Upload.
#[tauri::command]
pub fn read_tournament_logo(path: String) -> Result<LogoData, String> {
    let src = std::path::PathBuf::from(&path);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["jpg", "jpeg", "png", "webp", "gif", "svg"].contains(&e.as_str()))
        .ok_or("Nur Bilddateien (PNG, JPG, WEBP, GIF, SVG) sind erlaubt.")?;
    let meta = std::fs::metadata(&src).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
    if !meta.is_file() {
        return Err("Die Auswahl ist keine Datei.".to_string());
    }
    // Erst lesen, dann die tatsächlich gelesene Größe prüfen (kein TOCTOU-
    // Fenster zwischen metadata() und read()).
    let bytes = std::fs::read(&src).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
    if bytes.len() as u64 > MAX_LOGO_BYTES {
        return Err("Das Logo ist größer als 2 MB.".to_string());
    }
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };
    tracing::info!("Turnierlogo geladen ({} B, {mime})", bytes.len());
    Ok(LogoData {
        data,
        mime: mime.to_string(),
    })
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
    // Auch die „Leisten-Sponsor"-Markierung mit aufräumen — sonst zeigte die
    // Leiste ein 404-Bild für eine gelöschte Datei.
    let bar_path = monitor_ad_bar_path(&app);
    let mut bar = crate::tablet::monitor::read_ad_bar(&bar_path);
    if bar.remove(&file) {
        let _ = crate::tablet::monitor::write_ad_bar(&bar_path, &bar);
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
    let bar = crate::tablet::monitor::read_ad_bar(&monitor_ad_bar_path(&app));
    files
        .into_iter()
        .map(|file| CourtAd {
            label: labels.get(&file).cloned().unwrap_or_default(),
            in_bar: bar.contains(&file),
            file,
        })
        .collect()
}

/// Obergrenze der an den badhub-Check-In gesendeten Leisten-Sponsoren.
/// Spiegelt badhubs `checkin_sponsor_max()` (4) — mehr legt badhub ohnehin
/// nicht ab.
const MAX_BRANDING_SPONSORS: usize = 4;

/// Sammelt die als „Leiste" markierten Werbebilder als roh-Base64-Strings
/// (alphabetisch, auf [`MAX_BRANDING_SPONSORS`] gedeckelt). Genau die Form, die
/// der badhub-Endpunkt erwartet (`{"sponsors":[…]}`). Unlesbare Dateien fallen
/// still weg.
fn collect_bar_sponsors_b64(ad_dir: &std::path::Path, bar_path: &std::path::Path) -> Vec<String> {
    use base64::Engine;
    let bar = crate::tablet::monitor::read_ad_bar(bar_path);
    crate::tablet::monitor::list_ads(ad_dir)
        .into_iter()
        .filter(|name| bar.contains(name))
        .take(MAX_BRANDING_SPONSORS)
        .filter_map(|name| std::fs::read(ad_dir.join(&name)).ok())
        .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
        .collect()
}

/// Feuer-und-vergiss-Push einer Branding-Nachricht (Sponsoren und/oder Logo) an
/// den badhub-Check-In (Phase 3 der Sponsor-Leiste). `label` erscheint nur im
/// Log. **Additiv**: Fehler (inkl. „badhub kennt den Endpunkt noch nicht" =
/// 404/400) werden nur geloggt und stören das Speichern nicht. Läuft
/// asynchron, damit das synchrone Command sofort zurückkehrt. Der Aufrufer hat
/// bereits geprüft, dass ein Badhub-Passwort gesetzt ist.
fn spawn_branding_push(
    live_url: String,
    password: String,
    msg: crate::badhub::payload::CheckinBrandingMessage,
    label: &'static str,
) {
    let url = crate::badhub::push::checkin_branding_url(&live_url);
    tauri::async_runtime::spawn(async move {
        let client = crate::badhub::push::build_client();
        match crate::badhub::push::push_checkin_branding(&client, &url, &password, &msg).await {
            Ok(()) => tracing::debug!("{label} an badhub-Check-In gesendet"),
            // badhub kennt den Endpunkt noch nicht (ältere Version) — kein Fehler.
            Err(crate::badhub::push::PushError::Status(404))
            | Err(crate::badhub::push::PushError::Status(400)) => tracing::info!(
                "badhub kennt den Check-In-Branding-Endpunkt noch nicht — {label} nicht gesendet"
            ),
            Err(e) => tracing::warn!("{label} konnte nicht an badhub gesendet werden: {e}"),
        }
    });
}

/// Schiebt die aktuellen Leisten-Sponsoren an den badhub-Check-In. Ohne
/// konfiguriertes Badhub-Passwort passiert nichts. Sendet **nur** das
/// `sponsors`-Feld — das Logo bleibt badhub-seitig unberührt.
fn push_bar_sponsors_to_badhub(app: &AppHandle, state: &State<'_, AppState>) {
    let (live_url, password) = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        (cfg.badhub.url.clone(), cfg.badhub.password.clone())
    };
    // Kein Liveticker konfiguriert → kein Turnier, an das wir senden könnten.
    if password.is_empty() {
        return;
    }
    let sponsors = collect_bar_sponsors_b64(&monitor_ad_dir(app), &monitor_ad_bar_path(app));
    spawn_branding_push(
        live_url,
        password,
        crate::badhub::payload::CheckinBrandingMessage {
            sponsors: Some(sponsors),
            logo: None,
        },
        "Leisten-Sponsoren",
    );
}

/// Schiebt das Turnierlogo an den badhub-Check-In (Phase 3): einmalig beim
/// Speichern, wenn sich das Logo geändert hat — statt es alle 60 s im
/// Liveticker-`tset` mitzuschicken. Sendet **nur** das `logo`-Feld (leerer
/// String = badhub löscht das Logo); die Sponsoren bleiben unberührt. Ohne
/// Badhub-Passwort ein No-op.
fn push_logo_to_badhub(state: &State<'_, AppState>) {
    let (live_url, password, logo) = {
        let cfg = state.config.lock().expect("Config-Mutex nicht vergiftet");
        (
            cfg.badhub.url.clone(),
            cfg.badhub.password.clone(),
            cfg.tournament_logo.data.clone(),
        )
    };
    if password.is_empty() {
        return;
    }
    spawn_branding_push(
        live_url,
        password,
        crate::badhub::payload::CheckinBrandingMessage {
            sponsors: None,
            logo: Some(logo),
        },
        "Turnierlogo",
    );
}

/// Markiert ein Werbebild als „auch klein in der Leiste zeigen" (`in_bar=true`)
/// oder entfernt die Markierung. Die Leiste (neben dem Turnierlogo) zeigt genau
/// die markierten Bilder — in der Regel 1–2 Sponsoren. Nach dem Speichern
/// werden die Sponsoren zusätzlich an den badhub-Check-In geschoben (Phase 3,
/// additiv/feuer-und-vergiss).
#[tauri::command]
pub fn set_court_ad_bar(
    app: AppHandle,
    state: State<'_, AppState>,
    file: String,
    in_bar: bool,
) -> Result<(), String> {
    if !crate::tablet::monitor::is_safe_image_name(&file) {
        return Err("Ungültiger Dateiname.".to_string());
    }
    let bar_path = monitor_ad_bar_path(&app);
    let mut bar = crate::tablet::monitor::read_ad_bar(&bar_path);
    if in_bar {
        bar.insert(file);
    } else {
        bar.remove(&file);
    }
    crate::tablet::monitor::write_ad_bar(&bar_path, &bar)
        .map_err(|e| format!("Leisten-Markierung speichern fehlgeschlagen: {e}"))?;
    push_bar_sponsors_to_badhub(&app, &state);
    Ok(())
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

    fn local_az(enabled: bool, key: &str, region: &str) -> crate::config::AzureTtsConfig {
        crate::config::AzureTtsConfig {
            enabled,
            region: region.into(),
            key: key.into(),
            voice: "lokal-stimme".into(),
            discipline_voices: std::collections::HashMap::new(),
        }
    }

    fn share(key: &str, region: &str) -> relay_proto::AzureTtsShare {
        relay_proto::AzureTtsShare {
            region: region.into(),
            key: key.into(),
            voice: "master-stimme".into(),
            discipline_voices: std::collections::HashMap::new(),
        }
    }

    /// AppConfig mit gesetzter install_id + Secrets (Struct-Update, damit
    /// clippy nicht über Feld-Zuweisung nach Default::default() meckert).
    fn cfg_id(
        install_id: &str,
        btp_pw: Option<&str>,
        badhub_pw: &str,
        azure_key: &str,
    ) -> AppConfig {
        AppConfig {
            install_id: install_id.into(),
            btp: crate::config::BtpConfig {
                password: btp_pw.map(Into::into),
                ..Default::default()
            },
            badhub: crate::config::BadhubConfig {
                password: badhub_pw.into(),
                ..Default::default()
            },
            azure_tts: crate::config::AzureTtsConfig {
                key: azure_key.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn identity_bundle_strips_all_secrets_keeps_install_id() {
        // ADR 0006: Export-Bündel trägt die install_id, aber KEINE Secrets
        // (BTP-/Badhub-Passwort, Azure-Key).
        let bundle = identity_bundle(cfg_id(
            "inst-xyz",
            Some("btp-geheim"),
            "badhub-geheim",
            "azure-geheim",
        ));
        assert_eq!(bundle.install_id, "inst-xyz");
        assert_eq!(bundle.btp.password, None);
        assert!(bundle.badhub.password.is_empty());
        assert!(bundle.azure_tts.key.is_empty(), "Azure-Key darf nie raus");
    }

    #[test]
    fn saving_settings_does_not_revert_the_paired_device_list() {
        // Die Einstellungsseite schickt IHREN Stand der ganzen Config
        // zurück — aufgenommen, als die Seite geöffnet wurde. Die
        // Geräteliste wächst aber am Host (Kopplung). Ohne Schutz würde
        // folgender Ablauf das frisch gekoppelte Gerät wieder aussperren:
        // Einstellungen öffnen → Tablet koppeln → in den Einstellungen
        // irgendetwas speichern.
        let mut current = AppConfig::default();
        current.tl_web.enabled = true;
        current.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-frisch".to_string(),
            token: "tok-frisch".to_string(),
            label: "gerade gekoppelt".to_string(),
            created_at_ms: 2,
            hall: String::new(),
            profile_id: String::new(),
        });

        // Der Stand aus dem Fenster kennt das Gerät noch nicht.
        let mut from_ui = AppConfig::default();
        from_ui.tl_web.enabled = true;
        from_ui.badhub.url = "geändert".to_string();

        let merged = keep_host_managed_fields(from_ui, &current);
        assert_eq!(merged.badhub.url, "geändert", "Einstellung wird übernommen");
        assert_eq!(
            merged.tl_web.devices.len(),
            1,
            "das inzwischen gekoppelte Gerät bleibt"
        );
        assert_eq!(merged.tl_web.devices[0].token, "tok-frisch");
    }

    #[test]
    fn the_wizard_cannot_wipe_the_hall_layouts() {
        // Die Hallen-Anordnung wird auf der Felderübersicht gepflegt, nicht
        // im Assistenten — dessen Speichern darf sie nicht zurücksetzen
        // (dieselbe Falle wie bei der Geräteliste oben).
        let mut current = AppConfig::default();
        current.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "H1".into(),
            columns: 2,
            origin: crate::config::LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: true,
        });
        let incoming = AppConfig::default(); // Wizard-Stand ohne Layouts
        let ergebnis = keep_host_managed_fields(incoming, &current);
        assert_eq!(ergebnis.hall_layouts, current.hall_layouts);
    }

    #[test]
    fn turning_the_feature_off_still_clears_the_devices() {
        // Gegenprobe: Der Schutz darf keine Einbahnstraße sein. Schaltet
        // die Turnierleitung die Oberfläche aus, sollen die Zugänge auch
        // wirklich verschwinden.
        let mut current = AppConfig::default();
        current.tl_web.enabled = true;
        current.tl_web.devices.push(crate::config::TlDevice {
            id: "dev".to_string(),
            token: "tok".to_string(),
            label: "l".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        });

        let from_ui = AppConfig::default(); // tl_web aus
        let merged = keep_host_managed_fields(from_ui, &current);
        assert!(!merged.tl_web.enabled);
        assert!(
            merged.tl_web.devices.is_empty(),
            "Ausschalten entzieht die Zugänge"
        );
    }

    #[test]
    fn identity_bundle_strips_tl_device_tokens() {
        // ADR 0012: Ein Identitäts-Umzug nimmt die Turnierleitungs-Geräte
        // NICHT mit. Sonst bliebe der alte PC über die exportierten Tokens
        // schreibberechtigt, und ein weitergegebenes Bündel wäre zugleich
        // ein Satz gültiger Zugänge. Die Geräte koppeln sich am neuen PC
        // neu — ein QR-Scan je Gerät.
        let mut cfg = cfg_id("inst-xyz", None, "", "");
        cfg.tl_web.enabled = true;
        cfg.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-1".to_string(),
            token: "tok-geheim".to_string(),
            label: "Tablet TL".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        });

        let bundle = identity_bundle(cfg);
        assert!(
            bundle.tl_web.devices.is_empty(),
            "TL-Gerätetokens dürfen nie exportiert werden"
        );
        // Der Schalter selbst darf mitwandern — er ist kein Geheimnis, und
        // der neue PC soll die Oberfläche nicht erst wieder suchen müssen.
        assert!(bundle.tl_web.enabled);
        // Und die Identität bleibt, das ist der Zweck des Umzugs.
        assert_eq!(bundle.install_id, "inst-xyz");
    }

    #[test]
    fn apply_imported_identity_keeps_locally_paired_tl_devices() {
        // Das Bündel trägt nie Geräte (identity_bundle löscht sie). Ohne
        // Rückgabe des lokalen Stands würde ein Import die am NEUEN PC
        // bereits gekoppelten Geräte löschen — der typische Ablauf ist ja:
        // neuen PC einrichten, Tablets koppeln, dann die Identität holen.
        // Dieselbe Regel wie bei den Passwörtern.
        let mut current = cfg_id("inst-alt", None, "", "");
        current.tl_web.enabled = true;
        current.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-lokal".to_string(),
            token: "tok-lokal".to_string(),
            label: "Tablet TL".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        });
        let imported = cfg_id("inst-neu", None, "", "");

        let merged = apply_imported_identity(imported, &current);
        assert_eq!(merged.install_id, "inst-neu", "Identität wird übernommen");
        assert_eq!(
            merged.tl_web.devices.len(),
            1,
            "lokal gekoppelte Geräte bleiben"
        );
        assert_eq!(merged.tl_web.devices[0].token, "tok-lokal");
    }

    #[test]
    fn apply_imported_identity_keeps_current_secrets() {
        // Import übernimmt die Identität, behält aber die lokal gesetzten
        // Secrets (Bündel enthält keine).
        let current = cfg_id(
            "inst-alt",
            Some("aktuell-btp"),
            "aktuell-badhub",
            "aktuell-azure",
        );
        let imported = cfg_id("inst-neu", None, "", "");
        let merged = apply_imported_identity(imported, &current);
        assert_eq!(merged.install_id, "inst-neu");
        assert_eq!(merged.btp.password.as_deref(), Some("aktuell-btp"));
        assert_eq!(merged.badhub.password, "aktuell-badhub");
        assert_eq!(merged.azure_tts.key, "aktuell-azure");
    }

    #[test]
    fn apply_imported_identity_keeps_hall_layouts_when_bundle_has_none() {
        // Bündel aus einer Version vor Task 9/11 (oder eins ohne Raster
        // eingerichtet) trägt ein leeres `hall_layouts` — das darf die am
        // aktuellen PC eingerichteten Raster NICHT stillschweigend löschen.
        let mut current = cfg_id("inst-alt", None, "", "");
        current.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "Halle A".to_string(),
            columns: 3,
            origin: crate::config::LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: false,
        });
        let imported = cfg_id("inst-neu", None, "", "");

        let merged = apply_imported_identity(imported, &current);
        assert_eq!(merged.install_id, "inst-neu", "Identität wird übernommen");
        assert_eq!(
            merged.hall_layouts.len(),
            1,
            "lokal eingerichtete Raster bleiben, wenn das Bündel keine trägt"
        );
        assert_eq!(merged.hall_layouts[0].hall, "Halle A");
    }

    #[test]
    fn apply_imported_identity_takes_bundle_hall_layouts_when_present() {
        // Trägt das Bündel eigene Raster, gelten die (echter Umzug einer
        // Installation, die das Raster schon eingerichtet hatte) — nicht die
        // am neuen PC ggf. schon vorhandenen.
        let mut current = cfg_id("inst-alt", None, "", "");
        current.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "Halle Alt".to_string(),
            columns: 2,
            origin: crate::config::LayoutOrigin::TopRight,
            serpentine: true,
            vertical: true,
        });
        let mut imported = cfg_id("inst-neu", None, "", "");
        imported.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "Halle Neu".to_string(),
            columns: 4,
            origin: crate::config::LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: false,
        });

        let merged = apply_imported_identity(imported, &current);
        assert_eq!(
            merged.hall_layouts.len(),
            1,
            "das importierte Raster gilt, nicht das lokale"
        );
        assert_eq!(merged.hall_layouts[0].hall, "Halle Neu");
    }

    /// Minimales Panel-Profil für Identitäts-/Persistenz-Tests (Spec
    /// tl-web-panelsystem).
    fn profile(id: &str) -> crate::config::TlPanelProfile {
        crate::config::TlPanelProfile {
            id: id.to_string(),
            name: format!("Profil {id}"),
            panels: Vec::new(),
            display: crate::config::TlDisplaySettings::default(),
            updated_at_ms: 1,
            ..Default::default()
        }
    }

    #[test]
    fn identity_bundle_strips_tl_device_profile_ids() {
        // Regressionsschutz: identity_bundle löscht die komplette
        // devices-Liste (ADR 0012) — das nimmt jede darin gespeicherte
        // profile_id automatisch mit, ohne dass ein eigener Schritt nötig
        // wäre. Dieser Test verankert genau diese implizite Garantie.
        let mut cfg = cfg_id("inst-xyz", None, "", "");
        cfg.tl_web.enabled = true;
        cfg.tl_web.profiles.push(profile("profil-a"));
        cfg.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-1".to_string(),
            token: "tok-geheim".to_string(),
            label: "Tablet TL".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: "profil-a".to_string(),
        });

        let bundle = identity_bundle(cfg);
        assert!(
            bundle.tl_web.devices.is_empty(),
            "mit den Geräten verschwindet auch jede profile_id"
        );
    }

    #[test]
    fn identity_bundle_keeps_profile_catalog() {
        // Anders als die Geräte-Tokens ist der Profil-KATALOG kein
        // Zugang/Secret — er wandert beim Identitäts-Umzug mit (ADR 0025),
        // wie `hall_layouts`.
        let mut cfg = cfg_id("inst-xyz", None, "", "");
        cfg.tl_web.profiles.push(profile("profil-a"));
        cfg.tl_web.default_profile_id = "profil-a".to_string();

        let bundle = identity_bundle(cfg);
        assert_eq!(bundle.tl_web.profiles.len(), 1, "Katalog bleibt erhalten");
        assert_eq!(bundle.tl_web.profiles[0].id, "profil-a");
        assert_eq!(bundle.tl_web.default_profile_id, "profil-a");
    }

    #[test]
    fn apply_imported_identity_keeps_profiles_when_bundle_has_none() {
        // Bündel aus einer Version vor diesem Feature (oder eins ohne
        // eingerichtete Profile) trägt ein leeres `profiles` — das darf die
        // am aktuellen PC eingerichteten Profile NICHT stillschweigend
        // löschen (Muster hall_layouts, ADR 0025).
        let mut current = cfg_id("inst-alt", None, "", "");
        current.tl_web.profiles.push(profile("profil-lokal"));
        current.tl_web.default_profile_id = "profil-lokal".to_string();
        let imported = cfg_id("inst-neu", None, "", "");

        let merged = apply_imported_identity(imported, &current);
        assert_eq!(merged.install_id, "inst-neu", "Identität wird übernommen");
        assert_eq!(
            merged.tl_web.profiles.len(),
            1,
            "lokal eingerichtete Profile bleiben, wenn das Bündel keine trägt"
        );
        assert_eq!(merged.tl_web.profiles[0].id, "profil-lokal");
        assert_eq!(merged.tl_web.default_profile_id, "profil-lokal");
    }

    #[test]
    fn apply_imported_identity_takes_bundle_profiles_when_present() {
        // Trägt das Bündel eigene Profile, gelten die (echter Umzug einer
        // Installation, die den Katalog schon eingerichtet hatte) — nicht
        // die am neuen PC ggf. schon vorhandenen. `default_profile_id`
        // folgt demselben Katalog, sonst zeigte er womöglich auf eine
        // Kennung, die im übernommenen Katalog gar nicht existiert.
        let mut current = cfg_id("inst-alt", None, "", "");
        current.tl_web.profiles.push(profile("profil-alt"));
        current.tl_web.default_profile_id = "profil-alt".to_string();
        let mut imported = cfg_id("inst-neu", None, "", "");
        imported.tl_web.profiles.push(profile("profil-neu"));
        imported.tl_web.default_profile_id = "profil-neu".to_string();

        let merged = apply_imported_identity(imported, &current);
        assert_eq!(
            merged.tl_web.profiles.len(),
            1,
            "der importierte Katalog gilt, nicht der lokale"
        );
        assert_eq!(merged.tl_web.profiles[0].id, "profil-neu");
        assert_eq!(merged.tl_web.default_profile_id, "profil-neu");
    }

    #[test]
    fn keep_host_managed_fields_preserves_the_given_current_profiles() {
        // Muster `saving_settings_does_not_revert_the_paired_device_list`:
        // Die Einstellungsseite schickt IHREN (beim Öffnen aufgenommenen)
        // Stand zurück; der Profil-Katalog wächst aber währenddessen über
        // tl.html (ADR 0024/0025) — `keep_host_managed_fields` muss das
        // `current` übergebene `tl_web.profiles` unangetastet in den
        // gemergten Stand übernehmen.
        //
        // **Testet NUR diese Merge-Funktion isoliert** — mit einem von Hand
        // gebauten `current`. Das prüft NICHT, ob `current` (in der echten
        // App: `state.config.lock()`) zum Zeitpunkt des Aufrufs auch
        // tatsächlich das gerade in tl.html gespeicherte Profil kennt — das
        // war der eigentliche kritische Review-Fund: Vorher lief
        // `ServerCtx::mutate_app_config` (TL-Profil-Speichern) komplett
        // ohne den `AppState.config`-In-Memory-Stand zu berühren, sodass
        // `current` hier in der Praxis veraltet gewesen wäre — dieser Test
        // hätte den Fehler NICHT gefunden. Der echte End-zu-Ende-
        // Regressionstest für dieses Szenario (beide Schreibpfade über
        // denselben `Arc<Mutex<AppConfig>>`, echtes Temp-Verzeichnis, echtes
        // Schreiben) liegt in `tablet::server::tests
        // ::profile_save_survives_a_later_settings_save_lost_update_regression`.
        let mut current = AppConfig::default();
        current.tl_web.profiles.push(profile("frisch-angelegt"));
        current.tl_web.default_profile_id = "frisch-angelegt".to_string();

        // Der Stand aus dem Fenster kennt das Profil noch nicht.
        let mut from_ui = AppConfig::default();
        from_ui.badhub.url = "geändert".to_string();

        let merged = keep_host_managed_fields(from_ui, &current);
        assert_eq!(merged.badhub.url, "geändert", "Einstellung wird übernommen");
        assert_eq!(
            merged.tl_web.profiles.len(),
            1,
            "das inzwischen angelegte Profil bleibt"
        );
        assert_eq!(merged.tl_web.profiles[0].id, "frisch-angelegt");
        assert_eq!(merged.tl_web.default_profile_id, "frisch-angelegt");
    }

    #[test]
    fn export_bundle_json_roundtrips_install_id() {
        let json = serde_json::to_string(&identity_bundle(cfg_id(
            "inst-roundtrip",
            Some("x"),
            "",
            "",
        )))
        .unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.install_id, "inst-roundtrip");
        assert_eq!(parsed.btp.password, None);
    }

    #[test]
    fn effective_azure_prefers_complete_local_config() {
        // Vollständige lokale Config gewinnt gegen die geerbte (ADR 0003).
        let got = effective_azure(
            &local_az(true, "lokal", "germanywestcentral"),
            Some(&share("geerbt", "westeurope")),
        );
        assert_eq!(got, Some(("germanywestcentral".into(), "lokal".into())));
    }

    #[test]
    fn effective_azure_falls_back_to_inherited_when_local_incomplete() {
        // Genau der Turnier-Bug: Schalter an, Key fehlt → geerbte Config zieht.
        let got = effective_azure(
            &local_az(true, "", "westeurope"),
            Some(&share("geerbt", "westeurope")),
        );
        assert_eq!(got, Some(("westeurope".into(), "geerbt".into())));
        // Auch bei lokal komplett aus.
        let got = effective_azure(
            &local_az(false, "", ""),
            Some(&share("geerbt", "westeurope")),
        );
        assert_eq!(got, Some(("westeurope".into(), "geerbt".into())));
    }

    #[test]
    fn effective_azure_none_without_usable_config() {
        assert_eq!(effective_azure(&local_az(true, "", ""), None), None);
        // Unvollständig geerbte Config (defensiv) zählt nicht.
        assert_eq!(
            effective_azure(&local_az(false, "", ""), Some(&share("", "westeurope"))),
            None
        );
    }

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

    #[test]
    fn parse_netsh_ssid_does_not_match_bssid_alone() {
        // Nur eine BSSID-Zeile (MAC) → das ist KEINE SSID. Guard gegen die
        // dokumentierte BSSID/SSID-Verwechslung.
        let text = "    BSSID                  : 00:11:22:33:44:55";
        assert_eq!(parse_netsh_ssid(text), None);
    }

    #[test]
    fn parse_netsh_ssid_skips_empty_value() {
        // Leerer SSID-Wert (Übergangszustand) zählt nicht als verbunden.
        let text = "    SSID                   : \n    BSSID                  : 00:11:22:33:44:55";
        assert_eq!(parse_netsh_ssid(text), None);
    }

    #[test]
    fn parse_netsh_ssid_preserves_colon_in_name() {
        // Doppelpunkt im Netznamen bleibt erhalten (Split nur am ersten ':').
        let text = "    SSID                   : Halle:2 5G";
        assert_eq!(parse_netsh_ssid(text), Some("Halle:2 5G".to_string()));
    }

    #[test]
    fn bar_sponsors_are_collected_marked_only_sorted_and_capped() {
        use base64::Engine;
        let dir = tempfile::tempdir().unwrap();
        let ad_dir = dir.path();
        // Fünf Bilder mit unterschiedlichem Inhalt; nur die markierten zählen.
        for name in ["a.png", "b.png", "c.png", "d.png", "e.png"] {
            std::fs::write(ad_dir.join(name), name.as_bytes()).unwrap();
        }
        // Ein Nicht-Bild wird von list_ads ohnehin gefiltert (Sicherheitsnetz).
        std::fs::write(ad_dir.join("court-ad-bar.json"), b"[]").unwrap();
        let bar_path = ad_dir.join("court-ad-bar.json");
        // Markiere 5 (mehr als der Deckel) — erwartet: a..d (alphabetisch, 4).
        let marked: std::collections::HashSet<String> =
            ["e.png", "d.png", "c.png", "b.png", "a.png"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        crate::tablet::monitor::write_ad_bar(&bar_path, &marked).unwrap();

        let got = collect_bar_sponsors_b64(ad_dir, &bar_path);
        assert_eq!(got.len(), MAX_BRANDING_SPONSORS, "auf 4 gedeckelt");
        let expect: Vec<String> = ["a.png", "b.png", "c.png", "d.png"]
            .iter()
            .map(|n| base64::engine::general_purpose::STANDARD.encode(n.as_bytes()))
            .collect();
        assert_eq!(got, expect, "alphabetisch, roh-Base64 des Dateiinhalts");

        // Ohne Markierungen → leer (nichts zu senden).
        let empty: std::collections::HashSet<String> = std::collections::HashSet::new();
        crate::tablet::monitor::write_ad_bar(&bar_path, &empty).unwrap();
        assert!(collect_bar_sponsors_b64(ad_dir, &bar_path).is_empty());
    }
}
