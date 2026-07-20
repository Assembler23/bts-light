//! bts-relay – Cloud-Relay für den digitalen Tablet-Spielzettel.
//!
//! Auf IT-verwalteten Turnier-PCs blockiert die Windows-Firewall eingehende
//! Verbindungen; manche Hallen-WLANs isolieren die Geräte. Dann erreichen
//! die Tablets bts-light nicht direkt. Dieser Relay löst das: bts-light
//! **und** die Tablets verbinden sich nur noch *nach außen* zu badhub.de –
//! eine ausgehende Verbindung lässt jede Firma-IT durch.
//!
//! Der Relay ist ein reiner Broker ohne Persistenz. Jede bts-light-
//! Installation hat über ihre `install_id` einen eigenen **Namespace** –
//! Turniere kollidieren nicht. Pro Namespace gibt es genau einen „Host"
//! (bts-light) und beliebig viele Tablets, je an einen Court gebunden.
//!
//! Läuft als systemd-Dienst auf dem Hetzner-Server hinter nginx
//! (`https://badhub.de/bts-relay/` → `127.0.0.1:8090`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use include_dir::{include_dir, Dir};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, Mutex};

use relay_proto::{
    device_code, html_escape, path_encode, CourtBrief, HostFrame, MatchBrief, MonitorConfig,
    MonitorControl, MonitorDeviceInfo, MonitorMatch, MonitorPlayer, MonitorState, MonitorUpload,
    PlayerBrief, RelayFrame, ResultBody, ResultResponse, ServerMsg, SetAb, TabletMsg,
};

/// Die Tablet-Spielzettel-UI – dieselbe Datei wie in der bts-light-App.
const TABLET_HTML: &str = include_str!("../../src-tauri/assets/tablet.html");

/// Die Court-Monitor-Anzeige – dieselbe Datei wie in der bts-light-App.
const MONITOR_HTML: &str = include_str!("../../src-tauri/assets/monitor.html");

/// Gebündelte SVG-Länderflaggen (IOC-Code → `<code>.svg`), ausgeliefert
/// unter `/{ns}/flags/{file}`.
static FLAGS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../src-tauri/assets/flags");

/// Obergrenze gleichzeitiger Werbebilder je Namespace.
const MAX_ADS: usize = 24;

/// Obergrenze der Gesamtgröße aller Werbebilder eines Namespace (12 MB).
const MAX_ADS_TOTAL: usize = 12 * 1024 * 1024;

/// Body-Limit der Werbe-Upload-Route – Base64 bläht die Rohdaten ~+33 % auf.
const MONITOR_UPLOAD_LIMIT: usize = 20 * 1024 * 1024;

/// Obergrenze gleichzeitiger Tablets je Namespace (einfacher Missbrauchs-
/// Schutz – ein reales Turnier hat höchstens ~30 Felder).
const MAX_TABLETS_PER_NS: usize = 64;

/// WebSocket-Ping-Intervall – hält Verbindungen über NAT/LB offen.
const HEARTBEAT: Duration = Duration::from_secs(30);

/// Ping-Intervall der HOST-Verbindung – bewusst enger als [`HEARTBEAT`],
/// damit `host_last_seen` (Pong-Stempel) höchstens ~5 s alt ist und die
/// Stale-Erkennung schnell und sicher entscheiden kann.
const HOST_PING: Duration = Duration::from_secs(5);

/// Nach so viel Empfangs-Stille gilt eine Host-Verbindung als tot
/// (= 3 verpasste Pongs bei [`HOST_PING`]): Die Verbindung beendet sich
/// selbst, und ein neu verbindender Host darf den Slot übernehmen.
/// Ein LEBENDIGER Host antwortet binnen ~5 s auf Pings — die Übernahme
/// kann also nie einen gesunden Host verdrängen (R4 bleibt gewahrt).
const HOST_STALE: Duration = Duration::from_secs(15);

/// Wie lange der Relay auf die `ResultAck` von bts-light wartet.
const RESULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Obergrenze gleichzeitiger Namespaces (Speicher-Schutz – jede echte
/// Installation ist ein Namespace, real also höchstens ein paar Hundert).
const MAX_NAMESPACES: usize = 2000;

/// Obergrenze offener Ergebnis-Übermittlungen je Namespace.
const MAX_PENDING_PER_NS: usize = 16;

/// Maximale Größe eines gespiegelten Spielzustands (Schutz gegen Missbrauch).
const MAX_STATE_LEN: usize = 64 * 1024;

type Tx = mpsc::UnboundedSender<Message>;

/// Ein hochgeladenes Werbebild im Speicher (Content-Type + Rohbytes).
struct AdImage {
    content_type: String,
    bytes: Vec<u8>,
}

/// Court-Monitor-Datensatz eines Namespace: Anzeige-Konfiguration und
/// Werbebilder, vom bts-light-Host hochgeladen.
struct MonitorBundle {
    config: MonitorConfig,
    tournament_name: String,
    ads: Vec<AdImage>,
    /// Aufruf-Timer-Schwellen (vom Host hochgeladen) für die Monitor-Anzeige.
    call_timer: relay_proto::CallTimerView,
}

/// Ein Namespace: ein bts-light-Host und seine Tablets.
struct Namespace {
    /// Sende-Ende zur Host-WebSocket (bts-light), falls verbunden.
    host: Option<Tx>,
    /// Zeitpunkt (Unix-ms) des letzten Lebenszeichens der Host-Verbindung
    /// (Frame oder Pong). Grundlage der Zombie-Host-Ablösung: ein neuer
    /// Host darf einen seit [`HOST_STALE`] stummen alten ersetzen
    /// (Turnier-Befund 19.07.: tote TCP-Verbindung nach Netzwechsel hielt
    /// den Slot 17 Minuten — der Master war so lange ausgesperrt).
    host_last_seen: u64,
    /// CourtID → Sende-Ende zur Tablet-WebSocket.
    tablets: HashMap<i64, Tx>,
    /// CourtID → Geräte-Kennung des aktiven Tablets (leer bei alten
    /// Tablet-Seiten). Reconnect-Erkennung: dasselbe Gerät darf seine
    /// eigene, tote Session nahtlos ablösen (kein „Feld belegt").
    tablet_devices: HashMap<i64, String>,
    /// CourtID → zuletzt gespiegelter Spielzustand (JSON) des aktiven
    /// Tablets – wird einem übernehmenden Gerät übergeben.
    court_state: HashMap<i64, String>,
    /// CourtID → aktuelles Match (für die Court-Monitor-Anzeige).
    court_matches: HashMap<i64, MatchBrief>,
    /// CourtID → Satzstand in Team-Koordinaten (für die Monitor-Anzeige).
    court_scores: HashMap<i64, Vec<SetAb>>,
    /// CourtID → Zeitpunkt (Unix-ms), seit dem das aktuelle Spiel auf dem Feld
    /// steht (1. Aufruf). Wird beim ersten `MatchAssigned` eines neuen Matches
    /// gestempelt – Grundlage der Aufruf-Uhr am Cloud-Monitor.
    court_on_court_since: HashMap<i64, u64>,
    /// CourtID → Feldname (Anzeige) – vom Host mit jedem `MatchAssigned`/
    /// `MatchCleared`-Frame mitgeliefert, für die Monitor-Anzeige.
    court_labels: HashMap<i64, String>,
    /// CourtID → Hallenname (BTP-Location) – für die hallengefilterte
    /// Cloud-Ansage der fernen Halle (B1a).
    court_hall: HashMap<i64, String>,
    /// Freitext-Ansagen (Master → Slave), dedupliziert nach id, Cap 50.
    freetext: Vec<relay_proto::FreetextItem>,
    /// Cloud-Ansage-Slaves: id → (Halle, letzter Poll Unix-ms). Für die
    /// „ferne Halle online?"-Anzeige am Master. Rein informativ.
    slaves: HashMap<String, (String, u64)>,
    /// Vollständige Feld-Liste (vom Host via `HostFrame::Courts` gepusht) für
    /// das Cloud-Feldwechsel-Menü des Tablets (`/{ns}/courts`).
    courts: Vec<CourtBrief>,
    /// Aufgerufene (in Vorbereitung gerufene) Spiele der fernen Hallen – für
    /// die Slave-Spielübersicht + den Nachruf am Slave (Cluster C Stufe 2).
    /// Vom Host via `HostFrame::Prepared` gepusht; ersetzt jeweils die Liste.
    prepared: Vec<relay_proto::PreparedMatch>,
    /// Azure-TTS-Konfiguration des Masters für die Vererbung an Cloud-Ansage-
    /// Slaves (ADR 0003). Kommt mit jedem `HostFrame::Courts`-Push; `None`
    /// überschreibt bewusst — Azure am Master aus = Vererbung endet. Enthält
    /// den Subscription-Key → niemals loggen.
    azure_tts: Option<relay_proto::AzureTtsShare>,
    /// Court-Monitor-Konfiguration + Werbebilder, falls hochgeladen.
    monitor: Option<MonitorBundle>,
    /// Geräte-Steuerung (Feld-Zuweisungen + Fernbefehle), vom Host gepusht.
    monitor_control: MonitorControl,
    /// Geräte-ID → Zeitpunkt des letzten Monitor-Polls (Unix-ms) – für den
    /// Online-Status in der „Court-Monitore"-Seite des Tools.
    monitor_seen: HashMap<String, u64>,
    /// Offene Ergebnis-Übermittlungen: `req_id` → wartender HTTP-Handler.
    pending: HashMap<u64, oneshot::Sender<ResultResponse>>,
    /// Fortlaufende Request-ID für Ergebnis-Übermittlungen.
    next_req: u64,
}

impl Namespace {
    fn new() -> Self {
        Self {
            host: None,
            host_last_seen: 0,
            tablets: HashMap::new(),
            tablet_devices: HashMap::new(),
            court_state: HashMap::new(),
            court_matches: HashMap::new(),
            court_scores: HashMap::new(),
            court_on_court_since: HashMap::new(),
            court_labels: HashMap::new(),
            court_hall: HashMap::new(),
            prepared: Vec::new(),
            freetext: Vec::new(),
            slaves: HashMap::new(),
            courts: Vec::new(),
            azure_tts: None,
            monitor: None,
            monitor_control: MonitorControl::default(),
            monitor_seen: HashMap::new(),
            pending: HashMap::new(),
            next_req: 1,
        }
    }

    /// Leer = kann aus der Namespace-Tabelle entfernt werden. Der
    /// Court-Monitor-Datensatz (`monitor`) zählt hier bewusst NICHT mit:
    /// ohne Host gibt es nichts anzuzeigen, und der Host lädt ihn nach
    /// einem Reconnect binnen 30 s erneut hoch. Ihn zu behalten würde nur
    /// Speicher belegen, falls ein Host endgültig weg ist.
    fn is_empty(&self) -> bool {
        self.host.is_none() && self.tablets.is_empty() && self.pending.is_empty()
    }
}

/// Geteilter Broker-Zustand aller Handler.
#[derive(Clone)]
struct Broker {
    namespaces: Arc<Mutex<HashMap<String, Namespace>>>,
    /// Telefon-Kopplungscodes (ADR 0004): Code → (Namespace, Ablauf Unix-ms).
    /// Nur im RAM; ein Relay-Neustart macht offene Codes ungültig.
    pairings: Arc<Mutex<HashMap<String, PairingEntry>>>,
    /// Fehlversuchs-Zähler fürs Einlösen (globales Sliding Window gegen
    /// Durchprobieren): (Fensterbeginn Unix-ms, Fehlversuche im Fenster).
    pair_fails: Arc<Mutex<(u64, u32)>>,
    /// Öffentliche Basis-URL für QR-Codes, z. B. `https://badhub.de/bts-relay`.
    public_base: String,
}

/// Ein ausgestellter Telefon-Kopplungscode (ADR 0004).
struct PairingEntry {
    namespace: String,
    expires_ms: u64,
}

/// Gültigkeit eines Telefon-Kopplungscodes (Nutzerwunsch 19.07.2026:
/// 1 Stunde statt 15 Minuten — bequemer beim Turnier-Aufbau).
const PAIRING_TTL_MS: u64 = 60 * 60 * 1000;
/// Fehlversuchs-Fenster + -Limit fürs Einlösen (danach 429). Großzügig für
/// vertippte Menschen, viel zu knapp für 10⁸ Kombinationen.
const PAIR_FAIL_WINDOW_MS: u64 = 60_000;
const PAIR_FAIL_LIMIT: u32 = 100;

impl Broker {
    fn new(public_base: String) -> Self {
        Self {
            namespaces: Arc::new(Mutex::new(HashMap::new())),
            pairings: Arc::new(Mutex::new(HashMap::new())),
            pair_fails: Arc::new(Mutex::new((0, 0))),
            public_base,
        }
    }
}

/// Erzeugt einen 8-stelligen Zahlen-Code (führende Nullen möglich) aus
/// OS-Zufall. Modulo-Bias bei u64 → 10⁸ ist vernachlässigbar (~10⁻¹¹).
fn gen_pairing_code() -> Result<String, String> {
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).map_err(|e| e.to_string())?;
    Ok(format!("{:08}", u64::from_le_bytes(buf) % 100_000_000))
}

/// Sieht `code` wie ein Telefon-Kopplungscode aus (genau 8 Ziffern)?
fn valid_pairing_code(code: &str) -> bool {
    code.len() == 8 && code.bytes().all(|b| b.is_ascii_digit())
}

/// Serialisiert einen Wert zu einem WebSocket-Text-Frame.
fn text<T: Serialize>(value: &T) -> Message {
    Message::Text(Utf8Bytes::from(
        serde_json::to_string(value).unwrap_or_default(),
    ))
}

/// Prüft, ob `ns` wie eine kanonische UUID aussieht (Form `8-4-4-4-12`,
/// nur Hex und Bindestriche). Die `install_id` ist immer eine
/// `crypto.randomUUID()` – frei erfundene oder überlange Namespaces
/// werden so abgewiesen, bevor sie Speicher belegen.
fn valid_namespace(ns: &str) -> bool {
    let bytes = ns.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Richtet das Logging ein. Ist `RELAY_LOG_DIR` gesetzt, schreibt der Relay
/// ZUSÄTZLICH zu stdout (journald) in eine **täglich rotierende Datei**
/// `bts-relay.log.YYYY-MM-DD` in diesem Verzeichnis — auf dem Hetzner-Server
/// nach `storage/relay-logs/`, das der `badhub`-User direkt lesen darf (kein
/// journalctl-Recht nötig). Ohne die Env-Var bleibt es bei stdout-only (lokal/
/// `cargo run`). Der zurückgegebene Guard muss für die Programmlaufzeit leben
/// (sonst flusht der non-blocking Writer nicht).
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::prelude::*;
    // INFO-Default: unsere Diagnose-Zeilen (info!) bleiben, der TRACE-/DEBUG-
    // Verbindungsspam von axum/hyper wird gefiltert → lesbare, kompakte Datei.
    let level = LevelFilter::INFO;
    let stdout_layer = tracing_subscriber::fmt::layer().with_ansi(false);
    match std::env::var("RELAY_LOG_DIR") {
        Ok(dir) if !dir.is_empty() => {
            // Relay läuft als `badhub` und darf in storage/ schreiben. Scheitert
            // das (falsche Rechte/Quota), warnen wir nach stdout/journald — sonst
            // bliebe die erwartete Datei stumm leer (im Ernstfall der falsche
            // Moment, das zu merken). Der stdout-Fallback greift weiterhin.
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("WARN: RELAY_LOG_DIR '{dir}' nicht anlegbar: {e} — nur stdout");
            }
            let (non_blocking, guard) = tracing_appender::non_blocking(
                tracing_appender::rolling::daily(&dir, "bts-relay.log"),
            );
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking);
            tracing_subscriber::registry()
                .with(level)
                .with(stdout_layer)
                .with(file_layer)
                .init();
            Some(guard)
        }
        _ => {
            tracing_subscriber::registry()
                .with(level)
                .with(stdout_layer)
                .init();
            None
        }
    }
}

#[tokio::main]
async fn main() {
    // Guard bis Programmende halten → der Datei-Writer flusht zuverlässig.
    let _log_guard = init_tracing();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);
    let public_base =
        std::env::var("PUBLIC_BASE").unwrap_or_else(|_| "https://badhub.de/bts-relay".to_string());

    let broker = Broker::new(public_base.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route("/{ns}/court/{id}", get(court_page))
        .route("/{ns}/courts", get(courts_list))
        .route("/{ns}/court/{id}/display", get(monitor_page))
        .route("/{ns}/court/{id}/state", get(monitor_state))
        .route("/{ns}/monitor", get(monitor_device_page))
        .route("/{ns}/monitor/state", get(monitor_device_state))
        .route("/{ns}/monitor/control", post(monitor_control_upload))
        .route("/{ns}/monitor-devices", get(monitor_devices_list))
        .route("/{ns}/info/announce/state", get(announce_state))
        .route("/{ns}/pairing-code", post(pairing_code_create))
        .route("/pair/{code}", get(pairing_resolve))
        .route("/{ns}/slaves", get(slaves_list))
        .route("/{ns}/qr/{id}", get(qr_svg))
        .route("/{ns}/flags/{file}", get(flag_route))
        .route("/{ns}/ads/{idx}", get(ad_image))
        .route(
            "/{ns}/monitor",
            post(monitor_upload).layer(DefaultBodyLimit::max(MONITOR_UPLOAD_LIMIT)),
        )
        .route("/{ns}/ws", get(tablet_ws))
        .route("/{ns}/host-ws", get(host_ws))
        .route("/{ns}/result", post(result))
        .with_state(broker);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bts-relay konnte den Port nicht binden");
    tracing::info!("bts-relay lauscht auf 127.0.0.1:{port} (öffentlich: {public_base})");
    axum::serve(listener, app)
        .await
        .expect("bts-relay-Server beendet");
}

// ─────────────────────────────── HTTP-Routen ──────────────────────────────

/// Status-Schnappschuss.
async fn health(State(broker): State<Broker>) -> Json<serde_json::Value> {
    let map = broker.namespaces.lock().await;
    Json(serde_json::json!({
        "ok": true,
        "namespaces": map.len(),
        "tablets": map.values().map(|n| n.tablets.len()).sum::<usize>(),
    }))
}

/// Liefert die Tablet-UI für ein Feld (per CourtID; kein Caching – immer
/// frisch). Der Feldname für die Anzeige stammt – falls bekannt – aus dem
/// Namespace; sonst bleibt er leer und wird vom ersten Server-Frame
/// nachgeliefert.
async fn court_page(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    tracing::info!("Tablet-Seite ausgeliefert für Feld {court_id}");
    let label = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.court_labels.get(&court_id).cloned())
            .unwrap_or_default()
    };
    let body = TABLET_HTML
        .replace("__COURT_ID__", &court_id.to_string())
        .replace("__COURT_LABEL__", &html_escape(&label))
        // Der Relay kennt den Host-PIN nicht → leer lassen; tablet.html fällt
        // dann defensiv auf „0000" zurück. Die Feldwechsel-Liste liefert
        // `/{ns}/courts` (vom Host gepusht).
        .replace("__TABLET_PIN__", "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// Feld-Liste fürs Feldwechsel-PIN-Menü des Tablets (Cloud-Modus). Liefert die
/// vom Host via `HostFrame::Courts` gepushte Liste; leer, solange kein Push kam.
async fn courts_list(State(broker): State<Broker>, Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let courts = {
        let map = broker.namespaces.lock().await;
        map.get(&ns).map(|n| n.courts.clone()).unwrap_or_default()
    };
    let items: Vec<serde_json::Value> = courts
        .into_iter()
        .map(|c| serde_json::json!({ "id": c.id, "label": c.label, "hall": c.hall }))
        .collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::Value::Array(items)),
    )
        .into_response()
}

/// QR-Code (SVG), der auf die öffentliche Tablet-URL des Felds (per
/// CourtID) zeigt.
async fn qr_svg(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let url = format!(
        "{}/{}/court/{}",
        broker.public_base,
        path_encode(&ns),
        court_id
    );
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(220, 220)
                .build();
            (
                [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
                svg,
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "QR-Erzeugung fehlgeschlagen",
        )
            .into_response(),
    }
}

// ─────────────────────────────── Court-Monitor ────────────────────────────

/// Obergrenze gepollter Monitor-Geräte je Namespace (Missbrauchs-Schutz).
const MAX_MONITOR_DEVICES: usize = 128;

/// Aktuelle Unix-Zeit in Millisekunden.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Baut den URL-Basis-Pfad für `monitor.html`: der **Pfad-Teil** der
/// öffentlichen Relay-Basis plus der Namespace, z. B. `/bts-relay/<ns>/`.
/// Wichtig: der Relay läuft hinter nginx unter `/bts-relay/` – ohne
/// diesen Präfix zeigen die absoluten Asset-/State-URLs ins Leere.
fn monitor_base(public_base: &str, ns: &str) -> String {
    let after = public_base
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(public_base);
    let path = after.find('/').map(|i| &after[i..]).unwrap_or("");
    format!("{}/{}/", path.trim_end_matches('/'), ns)
}

/// Rendert `monitor.html` mit den Platzhaltern. `base` ist der
/// absolute URL-Basis-Pfad ([`monitor_base`]) – so lösen sich Flaggen,
/// Werbung und State-Abfrage korrekt auf.
fn render_monitor_html(mode: &str, base: &str, court_label: &str) -> String {
    MONITOR_HTML
        .replace("__MODE__", mode)
        .replace("__BASE__", base)
        .replace("__COURT_LABEL__", &html_escape(court_label))
}

/// Liefert die Court-Monitor-Anzeige fest für ein Feld
/// (`/court/{id}/display`, per CourtID).
async fn monitor_page(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let label = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.court_labels.get(&court_id).cloned())
            .unwrap_or_default()
    };
    let body = render_monitor_html("fixed", &monitor_base(&broker.public_base, &ns), &label);
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// Liefert die Court-Monitor-Anzeige im Geräte-Modus (`/{ns}/monitor`).
async fn monitor_device_page(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let body = render_monitor_html("device", &monitor_base(&broker.public_base, &ns), "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// Anzeige-Zustand eines fest verdrahteten Feldes (per CourtID), im
/// Sekundentakt gepollt.
async fn monitor_state(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let map = broker.namespaces.lock().await;
    let state = match map.get(&ns) {
        Some(namespace) => build_monitor_state(namespace, court_id),
        // Kein Host verbunden: leerer Zustand → neutrale Leerlauf-Seite.
        None => empty_monitor_state(court_id, String::new()),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

/// Query-Parameter der Geräte-Modus-Abfrage: die Geräte-ID.
#[derive(serde::Deserialize)]
struct DeviceQuery {
    device: String,
}

/// Anzeige-Zustand für ein Monitor-Gerät: löst die Feld-Zuweisung auf,
/// registriert den Poll und hängt einen offenen Fernbefehl an.
async fn monitor_device_state(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Query(q): Query<DeviceQuery>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    if q.device.is_empty() || q.device.len() > 64 {
        return (StatusCode::BAD_REQUEST, "Ungültige Geräte-ID").into_response();
    }
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        // Host nicht verbunden – das Gerät zeigt die Leerlauf-Seite.
        let state = empty_monitor_state(0, String::new());
        return ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response();
    };
    // Poll registrieren. Bei erreichter Obergrenze das am längsten nicht
    // gesehene Gerät verdrängen – so sperrt der Missbrauchs-Schutz keine
    // echten Geräte nach Geräte-Wechseln dauerhaft aus.
    if !namespace.monitor_seen.contains_key(&q.device)
        && namespace.monitor_seen.len() >= MAX_MONITOR_DEVICES
    {
        if let Some(oldest) = namespace
            .monitor_seen
            .iter()
            .min_by_key(|(_, &ts)| ts)
            .map(|(id, _)| id.clone())
        {
            namespace.monitor_seen.remove(&oldest);
        }
    }
    namespace.monitor_seen.insert(q.device.clone(), now_ms());
    let command = namespace.monitor_control.commands.get(&q.device).copied();
    let assigned = namespace
        .monitor_control
        .assignments
        .get(&q.device)
        .copied();
    let mut state = match assigned {
        Some(court_id) => build_monitor_state(namespace, court_id),
        None => unassigned_state(&q.device),
    };
    state.command = command;
    state.device_code = device_code(&q.device);
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

/// Nimmt die Geräte-Steuerdaten (Feld-Zuweisungen + Fernbefehle) vom
/// bts-light-Host entgegen. Nur erlaubt, solange der Host verbunden ist.
///
/// Wie alle Namespace-Routen bewusst ohne eigenes Auth-Token: der
/// 128-Bit-UUID-Namespace ist das Zugangsmerkmal. Worst Case ist ein
/// erzwungenes „Neu laden"/„Identifizieren" eines bekannten Turniers –
/// die Befehle sind ein geschlossenes Enum, kein Code.
async fn monitor_control_upload(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Json(control): Json<MonitorControl>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace");
    }
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        return (StatusCode::NOT_FOUND, "bts-light ist nicht verbunden.");
    };
    namespace.monitor_control = control;
    (StatusCode::OK, "ok")
}

/// Liefert dem bts-light-Host die Liste der gemeldeten Monitor-Geräte.
async fn monitor_devices_list(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let map = broker.namespaces.lock().await;
    let devices: Vec<MonitorDeviceInfo> = match map.get(&ns) {
        Some(n) => {
            // Cloud-Pfad transportiert die Zuweisungen weiterhin als
            // `HashMap<String, i64>` (CourtID-only). Info-Monitor-Zuweisungen
            // sind heute nur LAN-seitig – `MonitorTarget::Court`-Wrap ist
            // damit korrekt für alles, was über den Relay läuft.
            let assignments: std::collections::HashMap<String, relay_proto::MonitorTarget> = n
                .monitor_control
                .assignments
                .iter()
                .map(|(k, &v)| (k.clone(), relay_proto::MonitorTarget::court(v)))
                .collect();
            relay_proto::build_device_list(&assignments, &n.court_labels, &n.monitor_seen, now_ms())
        }
        None => Vec::new(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(devices)).into_response()
}

/// Query für den Ansage-Status der fernen Halle.
#[derive(serde::Deserialize)]
struct AnnounceStateQuery {
    #[serde(default)]
    hall: String,
    #[serde(default)]
    since: u64,
    /// Optionale Slave-ID – wenn gesetzt, registriert der Poll die Präsenz des
    /// Slaves (für die „ferne Halle online?"-Anzeige am Master).
    #[serde(default)]
    slave: String,
}

/// Liefert dem Cloud-Ansage-Slave die hallengefilterten Court-Matches (für die
/// Auto-Feld-Ansage) + neue Freitext-Ansagen (`id > since`). Leerer `hall` =
/// keine Hallen-Einschränkung. Registriert nebenbei die Slave-Präsenz.
async fn announce_state(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Query(q): Query<AnnounceStateQuery>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let mut map = broker.namespaces.lock().await;
    let state = match map.get_mut(&ns) {
        Some(n) => {
            // Präsenz des Slaves merken (rein informativ; Cap gegen Wildwuchs).
            if !q.slave.is_empty() {
                let id: String = q.slave.chars().take(64).collect();
                let hall: String = q.hall.chars().take(128).collect();
                let now = now_ms();
                // Veraltete Slaves (> 60 s ungesehen) entfernen → Slots werden
                // frei, der Cap blockiert keine echten Slaves nach Altlasten.
                n.slaves
                    .retain(|_, (_, last)| now.saturating_sub(*last) < 60_000);
                if n.slaves.len() < 64 || n.slaves.contains_key(&id) {
                    n.slaves.insert(id, (hall, now));
                }
            }
            let courts: Vec<relay_proto::AnnounceCourt> = n
                .court_matches
                .iter()
                .filter(|(cid, _)| {
                    let h = n.court_hall.get(cid).map(String::as_str).unwrap_or("");
                    q.hall.is_empty() || h.is_empty() || h == q.hall
                })
                .map(|(cid, m)| relay_proto::AnnounceCourt {
                    court_id: *cid,
                    label: n.court_labels.get(cid).cloned().unwrap_or_default(),
                    match_brief: Some(m.clone()),
                })
                .collect();
            let freetext: Vec<relay_proto::FreetextItem> = n
                .freetext
                .iter()
                .filter(|f| {
                    f.id > q.since && (f.hall.is_empty() || q.hall.is_empty() || f.hall == q.hall)
                })
                .cloned()
                .collect();
            // Aufgerufene Spiele der Halle (Slave-Spielübersicht + Nachruf,
            // Cluster C Stufe 2) — gleiche Hallenfilter-Regel wie bei courts.
            let prepared: Vec<relay_proto::PreparedMatch> = n
                .prepared
                .iter()
                .filter(|p| q.hall.is_empty() || p.hall.is_empty() || p.hall == q.hall)
                .cloned()
                .collect();
            relay_proto::AnnounceState {
                courts,
                freetext,
                prepared,
                // Geerbte Azure-Config (ADR 0003) — gleiche Vertrauensstufe
                // wie der übrige Namespace-Inhalt (Bearer = install_id).
                azure_tts: n.azure_tts.clone(),
            }
        }
        None => relay_proto::AnnounceState::default(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

/// Stellt einen kurzlebigen Telefon-Kopplungscode für den Namespace aus
/// (ADR 0004). Nur für Namespaces mit **verbundenem Host** — sonst könnte
/// jeder beliebige (noch unbenutzte) Namespaces mit Codes belegen. Genau
/// ein aktiver Code je Namespace: ein neuer ersetzt den alten.
async fn pairing_code_create(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let host_connected = broker
        .namespaces
        .lock()
        .await
        .get(&ns)
        .is_some_and(|n| n.host.is_some());
    if !host_connected {
        return (
            StatusCode::CONFLICT,
            "Kein verbundener Host für diesen Namespace",
        )
            .into_response();
    }
    let now = now_ms();
    let mut pairings = broker.pairings.lock().await;
    // Abgelaufene Codes und den bisherigen Code dieses Namespace räumen.
    pairings.retain(|_, e| e.expires_ms > now && e.namespace != ns);
    let code = loop {
        match gen_pairing_code() {
            Ok(c) if !pairings.contains_key(&c) => break c,
            Ok(_) => continue, // Kollision (praktisch nie) → neu würfeln
            Err(e) => {
                tracing::warn!("Pairing-Code-Erzeugung fehlgeschlagen: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Zufall nicht verfügbar")
                    .into_response();
            }
        }
    };
    pairings.insert(
        code.clone(),
        PairingEntry {
            namespace: ns,
            expires_ms: now + PAIRING_TTL_MS,
        },
    );
    Json(relay_proto::PairingCode {
        code,
        expires_in_s: PAIRING_TTL_MS / 1000,
    })
    .into_response()
}

/// Löst einen Telefon-Kopplungscode zum vollen Namespace auf (ADR 0004).
/// Fehlversuchs-Limit VOR dem Lookup: Ist das Fenster ausgeschöpft, wird
/// auch ein zufällig richtiger Code nicht mehr beantwortet (429) — sonst
/// wäre das Limit fürs Durchprobieren wirkungslos.
async fn pairing_resolve(
    State(broker): State<Broker>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    if !valid_pairing_code(&code) {
        return (StatusCode::NOT_FOUND, "Ungültiger Code").into_response();
    }
    let now = now_ms();
    {
        // JEDEN Versuch atomar in EINEM Lock zählen (auch erfolgreiche):
        // Prüfen und Erhöhen getrennt wäre ein TOCTOU-Fenster, in dem
        // parallele Requests das Limit überschießen (Review-Befund).
        // Legitime Kopplungen liegen um Größenordnungen unter dem Limit.
        let mut fails = broker.pair_fails.lock().await;
        if now.saturating_sub(fails.0) > PAIR_FAIL_WINDOW_MS {
            *fails = (now, 0);
        }
        fails.1 += 1;
        if fails.1 > PAIR_FAIL_LIMIT {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Zu viele Fehlversuche – kurz warten",
            )
                .into_response();
        }
    }
    {
        let mut pairings = broker.pairings.lock().await;
        pairings.retain(|_, e| e.expires_ms > now);
        if let Some(e) = pairings.get(&code) {
            return Json(relay_proto::PairingResolved {
                namespace: e.namespace.clone(),
            })
            .into_response();
        }
    }
    (StatusCode::NOT_FOUND, "Code unbekannt oder abgelaufen").into_response()
}

/// Slaves gelten als online, wenn ihr letzter Poll < 12 s her ist (4 verpasste
/// 3-s-Polls Toleranz).
const SLAVE_ONLINE_MS: u64 = 12_000;

/// Liefert dem Master die bekannten Cloud-Ansage-Slaves seines Namespaces samt
/// Online-Status – für die „ferne Halle online?"-Anzeige in der Kopfzeile.
async fn slaves_list(State(broker): State<Broker>, Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let now = now_ms();
    let map = broker.namespaces.lock().await;
    let slaves: Vec<relay_proto::SlaveInfo> = match map.get(&ns) {
        Some(n) => n
            .slaves
            .iter()
            .map(|(id, (hall, last))| relay_proto::SlaveInfo {
                id: id.clone(),
                hall: hall.clone(),
                online: now.saturating_sub(*last) < SLAVE_ONLINE_MS,
                last_seen_ms: *last,
            })
            .collect(),
        None => Vec::new(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(slaves)).into_response()
}

/// Leerer Monitor-Zustand (kein Match, keine Werbung) – Leerlauf-Anzeige.
fn empty_monitor_state(court_id: i64, court_label: String) -> MonitorState {
    MonitorState {
        court_id,
        court_label,
        tournament_name: String::new(),
        match_info: None,
        court_state: None,
        config: MonitorConfig::default(),
        ads: Vec::new(),
        command: None,
        device_code: String::new(),
        unassigned: false,
        redirect_to: None,
        server_now_ms: now_ms(),
        // Aufruf-Timer am Monitor: im Cloud-Pfad noch nicht durchgereicht
        // (Host→Relay-Push fehlt) → Default (aus). LAN-Pfad zeigt ihn bereits.
        on_court_since_ms: None,
        call_timer: relay_proto::CallTimerView::default(),
    }
}

/// Zustand für ein noch keinem Feld zugewiesenes Gerät (Kopplungs-Seite).
fn unassigned_state(device_id: &str) -> MonitorState {
    MonitorState {
        unassigned: true,
        device_code: device_code(device_id),
        ..empty_monitor_state(0, String::new())
    }
}

/// Baut den Monitor-Anzeige-Zustand aus dem gespeicherten Namespace-Stand
/// (für ein Feld per CourtID).
fn build_monitor_state(namespace: &Namespace, court_id: i64) -> MonitorState {
    let monitor = namespace.monitor.as_ref();
    let match_info = namespace
        .court_matches
        .get(&court_id)
        .map(|mb| MonitorMatch {
            match_id: mb.match_id,
            discipline: mb.discipline.clone(),
            event_label: mb.event_label.clone(),
            match_number: mb.match_number,
            team1: mb.team_a.iter().map(monitor_player).collect(),
            team2: mb.team_b.iter().map(monitor_player).collect(),
            sets: namespace
                .court_scores
                .get(&court_id)
                .cloned()
                .unwrap_or_default(),
        });
    MonitorState {
        court_id,
        court_label: namespace
            .court_labels
            .get(&court_id)
            .cloned()
            .unwrap_or_default(),
        tournament_name: monitor
            .map(|m| m.tournament_name.clone())
            .unwrap_or_default(),
        match_info,
        court_state: namespace.court_state.get(&court_id).cloned(),
        config: monitor.map(|m| m.config.clone()).unwrap_or_default(),
        ads: monitor
            .map(|m| (0..m.ads.len()).map(|i| i.to_string()).collect())
            .unwrap_or_default(),
        command: None,
        device_code: String::new(),
        unassigned: false,
        redirect_to: None,
        server_now_ms: now_ms(),
        // 1.-Aufruf-Zeitpunkt (relay-seitig gestempelt) + Aufruf-Timer-Schwellen
        // aus dem Host-Upload → der Cloud-Monitor zeigt dieselbe Aufruf-Uhr.
        on_court_since_ms: namespace.court_on_court_since.get(&court_id).copied(),
        call_timer: monitor.map(|m| m.call_timer.clone()).unwrap_or_default(),
    }
}

fn monitor_player(p: &PlayerBrief) -> MonitorPlayer {
    MonitorPlayer {
        name: p.name.clone(),
        // `PlayerBrief` führt nur den kombinierten Namen – Vor-/Nachname
        // bleiben leer, der Court-Monitor zerlegt dann `name` selbst.
        given: String::new(),
        family: String::new(),
        nationality: p.nationality.clone(),
    }
}

/// Liefert eine gebündelte SVG-Länderflagge (`/{ns}/flags/GER.svg`).
async fn flag_route(Path((ns, file)): Path<(String, String)>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    if file.is_empty() || file.contains(['/', '\\']) || file.contains("..") {
        return (StatusCode::NOT_FOUND, "Nicht gefunden").into_response();
    }
    match FLAGS.get_file(&file) {
        Some(f) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            f.contents(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Flagge nicht gefunden").into_response(),
    }
}

/// Liefert ein hochgeladenes Werbebild eines Namespace (per Index).
async fn ad_image(
    State(broker): State<Broker>,
    Path((ns, idx)): Path<(String, String)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let Ok(i) = idx.parse::<usize>() else {
        return (StatusCode::NOT_FOUND, "Nicht gefunden").into_response();
    };
    // Bytes unter dem Lock herauskopieren, dann den Lock fallen lassen –
    // ein mehrere MB großes memcpy darf nicht den Namespace-Mutex halten.
    let ad = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.monitor.as_ref())
            .and_then(|m| m.ads.get(i))
            .map(|ad| (ad.content_type.clone(), ad.bytes.clone()))
    };
    match ad {
        Some((content_type, bytes)) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Werbebild nicht gefunden").into_response(),
    }
}

/// Nimmt den Court-Monitor-Datensatz (Konfiguration + Werbebilder) vom
/// bts-light-Host entgegen. Nur erlaubt, solange der Host verbunden ist –
/// das verhindert das Anlegen von Namespaces ohne Host.
///
/// Bewusst ohne eigenes Auth-Token: Wer den 128-Bit-UUID-Namespace kennt,
/// darf hochladen – dasselbe Vertrauensmodell wie für die übrigen
/// Namespace-Routen. Worst Case ist das Überschreiben der Werbebilder
/// eines bekannten Turniers; kein Code, keine Ergebnis-Schreibrechte.
async fn monitor_upload(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Json(upload): Json<MonitorUpload>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace");
    }
    let mut ads = Vec::new();
    let mut total = 0usize;
    for ad in upload.ads.into_iter().take(MAX_ADS) {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(ad.data.as_bytes()) else {
            continue;
        };
        total += bytes.len();
        if total > MAX_ADS_TOTAL {
            break;
        }
        ads.push(AdImage {
            content_type: sanitize_content_type(&ad.content_type),
            bytes,
        });
    }
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        return (StatusCode::NOT_FOUND, "bts-light ist nicht verbunden.");
    };
    namespace.monitor = Some(MonitorBundle {
        config: upload.config,
        tournament_name: upload.tournament_name,
        ads,
        call_timer: upload.call_timer,
    });
    tracing::info!("Namespace '{ns}': Court-Monitor-Datensatz aktualisiert");
    (StatusCode::OK, "ok")
}

/// Lässt nur die erwarteten Bild-MIME-Typen durch (der Header kommt vom
/// Host und landet ungeprüft im Content-Type der Auslieferung).
fn sanitize_content_type(ct: &str) -> String {
    match ct {
        "image/png" | "image/webp" | "image/gif" | "image/jpeg" => ct.to_string(),
        _ => "image/jpeg".to_string(),
    }
}

/// Nimmt das Endergebnis vom Tablet entgegen, leitet es an den Host weiter
/// und wartet auf dessen `ResultAck` (BTP-Schreiben passiert lokal bei
/// bts-light).
async fn result(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Json(body): Json<ResultBody>,
) -> Json<ResultResponse> {
    if !valid_namespace(&ns) {
        return Json(ResultResponse::err("Unbekannter Namespace."));
    }
    let (ack_tx, ack_rx) = oneshot::channel();
    let req_id;
    {
        let mut map = broker.namespaces.lock().await;
        let Some(namespace) = map.get_mut(&ns) else {
            return Json(ResultResponse::err(
                "bts-light ist nicht mit dem Relay verbunden.",
            ));
        };
        let Some(host) = namespace.host.clone() else {
            return Json(ResultResponse::err(
                "bts-light ist nicht mit dem Relay verbunden.",
            ));
        };
        // Schutz gegen geflutete Ergebnis-Übermittlungen: jede hält bis zu
        // RESULT_TIMEOUT lang einen pending-Eintrag offen.
        if namespace.pending.len() >= MAX_PENDING_PER_NS {
            return Json(ResultResponse::err(
                "Zu viele offene Übermittlungen – bitte kurz warten.",
            ));
        }
        req_id = namespace.next_req;
        namespace.next_req += 1;
        namespace.pending.insert(req_id, ack_tx);
        let frame = RelayFrame::Result {
            req_id,
            court_id: body.court_id,
            court_label: body.court_label.clone(),
            match_id: body.match_id,
            sets: body.sets.clone(),
            retired: body.retired,
            walkover: body.walkover,
            winner: body.winner,
            cascade_walkover: body.cascade_walkover,
        };
        if host.send(text(&frame)).is_err() {
            namespace.pending.remove(&req_id);
            return Json(ResultResponse::err("bts-light ist nicht erreichbar."));
        }
    }
    match tokio::time::timeout(RESULT_TIMEOUT, ack_rx).await {
        Ok(Ok(resp)) => Json(resp),
        _ => {
            let mut map = broker.namespaces.lock().await;
            if let Some(namespace) = map.get_mut(&ns) {
                namespace.pending.remove(&req_id);
            }
            Json(ResultResponse::err(
                "Zeitüberschreitung – bts-light hat nicht geantwortet.",
            ))
        }
    }
}

// ─────────────────────────────── Tablet-WS ────────────────────────────────

async fn tablet_ws(
    ws: WebSocketUpgrade,
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.on_upgrade(move |socket| tablet_conn(socket, broker, ns))
        .into_response()
}

/// Eine Tablet-Verbindung: meldet sich für ein Feld (per CourtID) an,
/// leitet Score-Updates an den Host weiter, empfängt Match-Zuweisungen.
async fn tablet_conn(mut socket: WebSocket, broker: Broker, ns: String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    // Feld-Identität dieser Verbindung: die CourtID, sobald `identify` kam.
    let mut court: Option<i64> = None;
    // Schiedst dieses Tablet das Feld aktiv? Passive Tablets warten auf
    // „Übernehmen"; ihre Score-/Alert-Frames werden nicht weitergeleitet.
    let mut active = false;
    // Persistente Geräte-Kennung (aus identify/take_over) — leer bei
    // alten Tablet-Seiten. Für die Reconnect-Erkennung je Feld.
    let mut my_device = String::new();
    let mut ping = tokio::time::interval(HEARTBEAT);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Text(t) => {
                        match serde_json::from_str::<TabletMsg>(t.as_str()) {
                            Ok(TabletMsg::Identify { court_id, device_id, .. }) => {
                                my_device = device_id;
                                match attach_tablet(&broker, &ns, court_id, &my_device, &tx).await {
                                    AttachResult::Active => {
                                        tracing::info!("Tablet verbunden: Namespace '{ns}', Feld {court_id}");
                                        active = true;
                                        court = Some(court_id);
                                    }
                                    AttachResult::Occupied => {
                                        tracing::info!("Feld {court_id} belegt – Tablet wartet auf Übernahme");
                                        let _ = tx.send(text(&ServerMsg::CourtOccupied));
                                        court = Some(court_id);
                                    }
                                    AttachResult::Rejected => {
                                        let _ = socket.send(Message::Close(None)).await;
                                        break;
                                    }
                                }
                            }
                            Ok(TabletMsg::TakeOver { device_id }) => {
                                if let (Some(c), false) = (court, active) {
                                    if !device_id.is_empty() {
                                        my_device = device_id;
                                    }
                                    take_over_court(&broker, &ns, c, &my_device, &tx).await;
                                    active = true;
                                    tracing::info!("Tablet übernimmt Feld {c} (Namespace '{ns}')");
                                }
                            }
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history, match_id }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_score(&broker, &ns, c, score_a, score_b, sets_history, match_id, &tx).await;
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_battery(&broker, &ns, c, percent, charging).await;
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_alert(&broker, &ns, c, injury, official, &tx).await;
                                }
                            }
                            Ok(TabletMsg::StateSync { state }) => {
                                if let (Some(c), true) = (court, active) {
                                    store_court_state(&broker, &ns, c, state, &tx).await;
                                }
                            }
                            Ok(TabletMsg::Ping) => {
                                // Lebenszeichen des Tablets über die Cloud →
                                // sofort Pong zurück, ohne bts-light zu behelligen.
                                let _ = tx.send(text(&ServerMsg::Pong));
                            }
                            Err(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(m) => { if socket.send(m).await.is_err() { break } }
                    None => break,
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }

    // Nur das aktive Tablet räumt seinen Court-Eintrag ab.
    if let (Some(c), true) = (court, active) {
        detach_tablet(&broker, &ns, c, &tx).await;
        tracing::info!("Tablet getrennt: Namespace '{ns}', Feld {c}");
    }
}

/// Ergebnis eines Tablet-Verbindungsversuchs an einem Feld.
enum AttachResult {
    /// Das Tablet schiedst dieses Feld nun aktiv.
    Active,
    /// Das Feld ist belegt – das Tablet bleibt passiv (Übernahme möglich).
    Occupied,
    /// Abgewiesen, weil ein Limit erreicht ist.
    Rejected,
}

/// Liefert den bekannten Feldnamen (Anzeige) eines Felds im Namespace –
/// leer, solange der Host noch kein Frame für dieses Feld geschickt hat.
fn label_of(namespace: &Namespace, court_id: i64) -> String {
    namespace
        .court_labels
        .get(&court_id)
        .cloned()
        .unwrap_or_default()
}

/// Versucht, ein Tablet als aktiv schiedsendes Gerät an einem Feld (per
/// CourtID) zu registrieren. Ist das Feld schon belegt, bleibt das Tablet
/// passiv.
async fn attach_tablet(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    device_id: &str,
    tx: &Tx,
) -> AttachResult {
    let mut map = broker.namespaces.lock().await;
    if !map.contains_key(ns) && map.len() >= MAX_NAMESPACES {
        tracing::warn!("Namespace-Limit erreicht – Tablet für '{ns}' abgewiesen");
        return AttachResult::Rejected;
    }
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if namespace.tablets.contains_key(&court_id) {
        // Reconnect-Erkennung: Meldet sich DASSELBE Gerät erneut (tote
        // Vorgänger-Session nach Netz-Abriss), löst es seine alte Session
        // nahtlos ab — kein „Feld belegt"-Overlay fürs eigene Gerät.
        // Leere Kennungen (alte Tablet-Seiten) zählen nie als „dasselbe".
        let same_device = !device_id.is_empty()
            && namespace.tablet_devices.get(&court_id).map(String::as_str) == Some(device_id);
        if !same_device {
            return AttachResult::Occupied;
        }
        if let Some(old) = namespace.tablets.remove(&court_id) {
            let _ = old.send(text(&ServerMsg::SessionSuperseded));
        }
        tracing::info!("Feld {court_id} (Namespace '{ns}'): Reconnect desselben Geräts");
    }
    if namespace.tablets.len() >= MAX_TABLETS_PER_NS {
        tracing::warn!("Namespace '{ns}' am Tablet-Limit – Feld {court_id} abgewiesen");
        return AttachResult::Rejected;
    }
    namespace.tablets.insert(court_id, tx.clone());
    namespace
        .tablet_devices
        .insert(court_id, device_id.to_string());
    // Laufenden Spielstand auch beim NORMALEN Verbinden wiederherstellen
    // (Crash/Ersatz-Tablet) – nicht nur bei Übernahme. Das Tablet behält ihn
    // nur, wenn die matchId zum gleich gepushten Match passt, sonst überschreibt
    // der Host das Feld (kein Wiederaufleben eines alten Stands).
    //
    // Diagnose (14.06.-Vorfall: Ersatz-Tablet sprang auf 0:0): explizit
    // protokollieren, ob beim (Neu-)Verbinden ein gespeicherter Stand
    // wiederhergestellt wurde oder das Feld ohne Stand startet.
    if let Some(state) = namespace.court_state.get(&court_id) {
        let len = state.len();
        let _ = tx.send(text(&ServerMsg::StateRestore {
            state: state.clone(),
        }));
        tracing::info!("Feld {court_id} (Namespace '{ns}'): StateRestore gesendet ({len} Bytes)");
    } else {
        tracing::info!(
            "Feld {court_id} (Namespace '{ns}'): kein gespeicherter Stand – Tablet startet bei 0:0"
        );
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_id,
            court_label,
        }));
    }
    AttachResult::Active
}

/// Übernimmt ein belegtes Feld für ein bisher passives Tablet – das
/// zuvor aktive Tablet wird mit `SessionSuperseded` gesperrt.
async fn take_over_court(broker: &Broker, ns: &str, court_id: i64, device_id: &str, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if let Some(old) = namespace.tablets.insert(court_id, tx.clone()) {
        let _ = old.send(text(&ServerMsg::SessionSuperseded));
    }
    namespace
        .tablet_devices
        .insert(court_id, device_id.to_string());
    // Laufenden Spielstand an das übernehmende Tablet übergeben.
    if let Some(state) = namespace.court_state.get(&court_id) {
        let len = state.len();
        let _ = tx.send(text(&ServerMsg::StateRestore {
            state: state.clone(),
        }));
        tracing::info!(
            "Feld {court_id} (Namespace '{ns}'): Übernahme – StateRestore gesendet ({len} Bytes)"
        );
    } else {
        tracing::info!("Feld {court_id} (Namespace '{ns}'): Übernahme ohne gespeicherten Stand");
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_id,
            court_label,
        }));
    }
}

/// Ist `tx` noch das am Feld eingetragene (aktive) Tablet? Nach einem
/// Reconnect-Reclaim lebt die abgelöste Session evtl. noch kurz weiter —
/// ihre nachlaufenden Frames dürfen Cache und Host nicht mehr mit dem
/// ALTEN Stand füttern (sonst kehrt genau der überbügelte Stand zurück,
/// den die Reconnect-Logik verhindert).
fn is_holder(namespace: &Namespace, court_id: i64, tx: &Tx) -> bool {
    namespace
        .tablets
        .get(&court_id)
        .map(|t| t.same_channel(tx))
        .unwrap_or(false)
}

/// Speichert den gespiegelten Spielzustand des aktiven Tablets am Feld.
async fn store_court_state(broker: &Broker, ns: &str, court_id: i64, state: String, tx: &Tx) {
    if state.len() > MAX_STATE_LEN {
        return;
    }
    let mut map = broker.namespaces.lock().await;
    if let Some(namespace) = map.get_mut(ns) {
        if !is_holder(namespace, court_id, tx) {
            return;
        }
        // Stale-Filter (A4): Ein State des ALTEN Matches darf den beim
        // Match-Wechsel geleerten Cache nicht wieder befüllen — sonst
        // bekäme ein übernehmendes Gerät das falsche Spiel.
        if let Some(state_match) = relay_proto::state_sync_match_id(&state) {
            if !match_id_matches_court(namespace, court_id, state_match) {
                tracing::info!(
                    "State von Feld {court_id} verworfen: Tablet-State trägt Match \
                     {state_match}, Feld hat ein anderes (Namespace '{ns}')"
                );
                return;
            }
        }
        namespace.court_state.insert(court_id, state);
    }
}

/// Entfernt das Tablet wieder – nur, wenn der eingetragene Sender noch
/// unserer ist (ein Reconnect auf dasselbe Feld darf nichts wegräumen).
async fn detach_tablet(broker: &Broker, ns: &str, court_id: i64, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    let still_ours = namespace
        .tablets
        .get(&court_id)
        .map(|t| t.same_channel(tx))
        .unwrap_or(false);
    if still_ours {
        namespace.tablets.remove(&court_id);
        namespace.tablet_devices.remove(&court_id);
        let court_label = label_of(namespace, court_id);
        if let Some(host) = &namespace.host {
            let _ = host.send(text(&RelayFrame::TabletDisconnected {
                court_id,
                court_label,
            }));
        }
    }
    if namespace.is_empty() {
        map.remove(ns);
    }
}

/// Passt die vom Tablet gemeldete Match-ID zum aktuellen Court-Match?
/// `match_id == 0` (alte Tablet-Seite ohne das Feld) → kein Filter,
/// Verhalten wie vor dem Feature. Nennt das Tablet ein Match, wird
/// verworfen, wenn der Relay fürs Feld ein ANDERES kennt — **oder gar
/// keins**: Nach `MatchCleared` (Feld frei) ist ein Frame mit Match-ID
/// per Definition ein Nachzügler des alten Spiels und darf den gerade
/// geleerten Cache nicht wieder befüllen (A4-Review-Befund). Gefahrlos,
/// weil `MatchAssigned` den Cache füllt, BEVOR das Tablet die Zuweisung
/// sieht — ein legitimes neues Match ist hier immer schon bekannt.
fn match_id_matches_court(namespace: &Namespace, court_id: i64, match_id: i64) -> bool {
    if match_id == 0 {
        return true;
    }
    match namespace.court_matches.get(&court_id) {
        Some(current) => current.match_id == match_id,
        None => false,
    }
}

/// Leitet einen Live-Score von einem Tablet an den Host weiter und merkt
/// ihn zugleich für die Court-Monitor-Anzeige.
#[allow(clippy::too_many_arguments)]
async fn forward_score(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    score_a: i64,
    score_b: i64,
    sets_history: Vec<SetAb>,
    match_id: i64,
    tx: &Tx,
) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    if !is_holder(namespace, court_id, tx) {
        return;
    }
    // Stale-Filter (Turnier-Befund HM-03): Ein nach Doze/Reconnect noch
    // im ALTEN Spiel hängendes Tablet darf den beim Match-Wechsel frisch
    // geleerten Score-Cache nicht wieder mit dem alten Stand befüllen.
    if !match_id_matches_court(namespace, court_id, match_id) {
        tracing::info!(
            "Score von Feld {court_id} verworfen: Tablet zählt Match {match_id}, \
             Feld hat ein anderes (Namespace '{ns}')"
        );
        return;
    }
    // Vollständige Satzliste (abgeschlossene Sätze + laufender Satz) für
    // die Court-Monitor-Anzeige merken.
    let mut sets = sets_history.clone();
    sets.push(SetAb {
        a: score_a,
        b: score_b,
    });
    namespace.court_scores.insert(court_id, sets);
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::ScoreUpdate {
            court_id,
            court_label,
            score_a,
            score_b,
            sets_history,
            match_id,
        }));
    }
}

/// Leitet den Akkustand eines Tablets an den Host weiter.
async fn forward_battery(broker: &Broker, ns: &str, court_id: i64, percent: i64, charging: bool) {
    let map = broker.namespaces.lock().await;
    if let Some(namespace) = map.get(ns) {
        let court_label = label_of(namespace, court_id);
        if let Some(host) = namespace.host.as_ref() {
            let _ = host.send(text(&RelayFrame::Battery {
                court_id,
                court_label,
                percent,
                charging,
            }));
        }
    }
}

/// Leitet den Meldungs-Zustand eines Felds an den Host weiter.
async fn forward_alert(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    injury: bool,
    official: bool,
    tx: &Tx,
) {
    let map = broker.namespaces.lock().await;
    let Some(namespace) = map.get(ns) else {
        return;
    };
    if !is_holder(namespace, court_id, tx) {
        return;
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = namespace.host.as_ref() {
        let _ = host.send(text(&RelayFrame::Alert {
            court_id,
            court_label,
            injury,
            official,
        }));
    }
}

// ─────────────────────────────── Host-WS ──────────────────────────────────

async fn host_ws(
    ws: WebSocketUpgrade,
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.on_upgrade(move |socket| host_conn(socket, broker, ns))
        .into_response()
}

/// Ergebnis eines Host-Registrierungsversuchs ([`try_claim_host`]).
enum HostClaim {
    /// Slot übernommen; `true`, wenn dabei eine stumme alte Verbindung
    /// verdrängt wurde.
    Accepted { superseded: bool },
    /// Ein lebendiger Host hält den Slot — Verbindung abweisen.
    Refused,
}

/// Versucht, den Host-Slot eines Namespace zu übernehmen. Genau ein Host
/// ist erlaubt; ein LEBENDIGER Inhaber wird nie verdrängt (R4 — kein
/// fremder Host übernimmt die Kontrolle). Ist der Inhaber aber seit
/// [`HOST_STALE`] stumm (kein Frame, kein Pong), gilt er als tote
/// TCP-Leiche und der neue Host ersetzt ihn (Zombie-Host-Ablösung —
/// Turnier-Befund 19.07.: 333× „Zweiter Host abgewiesen" in 17 Minuten,
/// weil die tote alte Verbindung den Slot hielt).
fn try_claim_host(namespace: &mut Namespace, tx: &Tx, now: u64) -> HostClaim {
    let stale = namespace.host.is_some()
        && now.saturating_sub(namespace.host_last_seen) >= HOST_STALE.as_millis() as u64;
    match (&namespace.host, stale) {
        (Some(_), false) => HostClaim::Refused,
        (old, _) => {
            let superseded = old.is_some();
            namespace.host = Some(tx.clone());
            namespace.host_last_seen = now;
            HostClaim::Accepted { superseded }
        }
    }
}

/// Die Host-Verbindung (bts-light) eines Namespace. Genau eine ist erlaubt;
/// eine zweite wird abgewiesen — außer der bisherige Host ist nachweislich
/// stumm ([`try_claim_host`]), dann ersetzt ihn die neue Verbindung.
async fn host_conn(mut socket: WebSocket, broker: Broker, ns: String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    {
        let mut map = broker.namespaces.lock().await;
        if !map.contains_key(&ns) && map.len() >= MAX_NAMESPACES {
            tracing::warn!("Namespace-Limit erreicht – Host für '{ns}' abgewiesen");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        let namespace = map.entry(ns.clone()).or_insert_with(Namespace::new);
        match try_claim_host(namespace, &tx, now_ms()) {
            HostClaim::Refused => {
                tracing::warn!("Zweiter Host für Namespace '{ns}' abgewiesen");
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            HostClaim::Accepted { superseded } => {
                if superseded {
                    tracing::warn!(
                        "Stummen alten Host für Namespace '{ns}' ersetzt (Zombie-Ablösung)"
                    );
                }
            }
        }
        // Schon verbundene Tablets nachmelden, damit der Host ihre Matches
        // sofort pusht.
        let connected: Vec<i64> = namespace.tablets.keys().copied().collect();
        for court_id in connected {
            let court_label = label_of(namespace, court_id);
            let _ = tx.send(text(&RelayFrame::TabletConnected {
                court_id,
                court_label,
            }));
        }
    }
    tracing::info!("Host verbunden für Namespace '{ns}'");

    // Enger Ping-Takt + Stale-Abbruch: Eine tote TCP-Verbindung fällt beim
    // `send` u. U. minutenlang nicht auf (Kernel puffert) — deshalb zählt
    // hier die EMPFANGS-Seite: bleibt jedes Lebenszeichen (Frame/Pong)
    // länger als HOST_STALE aus, beendet sich die Verbindung selbst und
    // gibt den Slot frei.
    let mut ping = tokio::time::interval(HOST_PING);
    let mut last_incoming = tokio::time::Instant::now();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                last_incoming = tokio::time::Instant::now();
                match msg {
                    Message::Text(t) => {
                        if let Ok(frame) = serde_json::from_str::<HostFrame>(t.as_str()) {
                            if !handle_host_frame(&broker, &ns, frame, &tx).await {
                                // Wir sind nicht mehr der eingetragene Host
                                // (wiedererwachte Alt-Verbindung nach einer
                                // Ablösung) → sauber beenden; bts-light
                                // verbindet sich neu und sieht die Lage.
                                tracing::warn!(
                                    "Abgelöste Host-Verbindung für '{ns}' meldet sich zurück – getrennt"
                                );
                                break;
                            }
                        }
                    }
                    Message::Pong(_) => {
                        // Pong-Stempel für die Zombie-Erkennung festhalten —
                        // aber nur, solange wir der eingetragene Host sind
                        // (eine abgelöste Verbindung darf den Stempel des
                        // neuen Hosts nicht verfälschen).
                        let mut map = broker.namespaces.lock().await;
                        if let Some(namespace) = map.get_mut(&ns) {
                            if namespace.host.as_ref().is_some_and(|h| h.same_channel(&tx)) {
                                namespace.host_last_seen = now_ms();
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(m) => { if socket.send(m).await.is_err() { break } }
                    None => break,
                }
            }
            _ = ping.tick() => {
                if last_incoming.elapsed() >= HOST_STALE {
                    tracing::warn!(
                        "Host für Namespace '{ns}' seit {}s stumm – Verbindung als tot verworfen",
                        last_incoming.elapsed().as_secs()
                    );
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }

    // Aufräumen: Host-Slot freigeben (nur wenn noch unserer), offene
    // Ergebnis-Übermittlungen mit Fehler beantworten.
    {
        let mut map = broker.namespaces.lock().await;
        if let Some(namespace) = map.get_mut(&ns) {
            release_host_slot(namespace, &tx);
            for (_, pending) in namespace.pending.drain() {
                let _ = pending.send(ResultResponse::err("Verbindung zu bts-light verloren."));
            }
            if namespace.is_empty() {
                map.remove(&ns);
            }
        }
    }
    tracing::info!("Host getrennt für Namespace '{ns}'");
}

/// Gibt den Host-Slot frei — aber nur, wenn `tx` noch der eingetragene
/// Host ist. Eine per Zombie-Ablösung verdrängte Alt-Verbindung, die
/// später stirbt, darf den Slot des NEUEN Hosts nicht abräumen. Liefert
/// `true`, wenn der Slot tatsächlich freigegeben wurde.
fn release_host_slot(namespace: &mut Namespace, tx: &Tx) -> bool {
    if namespace
        .host
        .as_ref()
        .map(|h| h.same_channel(tx))
        .unwrap_or(false)
    {
        namespace.host = None;
        return true;
    }
    false
}

/// Verarbeitet ein Frame vom Host: an das passende Tablet weiterleiten bzw.
/// eine wartende Ergebnis-Übermittlung abschließen.
///
/// `sender` ist das Sende-Ende der aufrufenden Host-Verbindung: Frames
/// werden nur verarbeitet, wenn sie vom AKTUELL eingetragenen Host
/// stammen — eine per Zombie-Ablösung verdrängte Alt-Verbindung, die
/// wieder erwacht, darf den Zustand nicht mehr verändern. Liefert
/// `false`, wenn der Sender nicht (mehr) der eingetragene Host ist.
async fn handle_host_frame(broker: &Broker, ns: &str, frame: HostFrame, sender: &Tx) -> bool {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return false;
    };
    if !namespace
        .host
        .as_ref()
        .is_some_and(|h| h.same_channel(sender))
    {
        return false;
    }
    // Jedes Host-Frame ist ein Lebenszeichen für die Zombie-Erkennung.
    namespace.host_last_seen = now_ms();
    match frame {
        HostFrame::MatchAssigned {
            court_id,
            court_label,
            hall,
            match_brief,
            on_court_since_ms,
        } => {
            // Feldname (Anzeige) merken – der Monitor liest ihn.
            if !court_label.is_empty() {
                namespace.court_labels.insert(court_id, court_label);
            }
            // Halle des Felds merken – für die hallengefilterte Cloud-Ansage.
            namespace.court_hall.insert(court_id, hall);
            // Satzstand/Spielzustand nur bei einem ECHTEN Match-Wechsel
            // zurücksetzen. Ein erneutes `MatchAssigned` fürs selbe Match
            // (z. B. nach einem kurzen Tablet-Reconnect) darf den Monitor
            // nicht auf 0:0 zurückwerfen.
            let same_match = namespace
                .court_matches
                .get(&court_id)
                .map(|m| m.match_id == match_brief.match_id)
                .unwrap_or(false);
            if !same_match {
                namespace.court_scores.remove(&court_id);
                namespace.court_state.remove(&court_id);
            }
            // 1.-Aufruf-Zeitpunkt: den autoritativen Host-Stempel übernehmen
            // (gleicher Wert auch bei Reconnect, frisch je Turnier → kein
            // veralteter Stand). Fehlt er (älterer Host), Eintrag entfernen.
            match on_court_since_ms {
                Some(ts) => {
                    namespace.court_on_court_since.insert(court_id, ts);
                }
                None => {
                    namespace.court_on_court_since.remove(&court_id);
                }
            }
            namespace
                .court_matches
                .insert(court_id, match_brief.clone());
            if let Some(t) = namespace.tablets.get(&court_id) {
                let _ = t.send(text(&ServerMsg::MatchAssigned { match_brief }));
            }
        }
        HostFrame::MatchCleared {
            court_id,
            court_label,
            hall,
        } => {
            if !court_label.is_empty() {
                namespace.court_labels.insert(court_id, court_label);
            }
            namespace.court_hall.insert(court_id, hall);
            namespace.court_matches.remove(&court_id);
            namespace.court_scores.remove(&court_id);
            namespace.court_state.remove(&court_id);
            namespace.court_on_court_since.remove(&court_id);
            if let Some(t) = namespace.tablets.get(&court_id) {
                let _ = t.send(text(&ServerMsg::MatchCleared));
            }
        }
        HostFrame::Freetext { id, hall, text } => {
            // Längen hart begrenzen (Schutz vor RAM-Aufblähung durch
            // pathologische Frames; char-genau, kein Byte-Slice-Panic).
            let text: String = text.chars().take(1000).collect();
            let hall: String = hall.chars().take(128).collect();
            // Neue Freitext-Ansage zwischenspeichern (dedup nach id, Cap 50) –
            // der Cloud-Ansage-Slave holt sie über /info/announce/state.
            if !namespace.freetext.iter().any(|f| f.id == id) {
                namespace
                    .freetext
                    .push(relay_proto::FreetextItem { id, hall, text });
                let len = namespace.freetext.len();
                if len > 50 {
                    namespace.freetext.drain(0..len - 50);
                }
            }
        }
        HostFrame::ResultAck { req_id, ok, error } => {
            if let Some(pending) = namespace.pending.remove(&req_id) {
                let _ = pending.send(ResultResponse { ok, error });
            }
        }
        HostFrame::Courts { courts, azure_tts } => {
            // Vollständige Feld-Liste für das Cloud-Feldwechsel-Menü merken.
            // Leere Liste NICHT übernehmen: der Host schickt sie nur, um die
            // Azure-Vererbung zu transportieren, solange BTP noch kein
            // Turnier geladen hat — sie darf eine gültige Liste nicht wischen.
            if !courts.is_empty() {
                namespace.courts = courts;
            }
            // Azure-Vererbung: jeder Push ist autoritativ, auch `None`
            // (Azure am Master deaktiviert → geerbte Config verfällt).
            namespace.azure_tts = azure_tts;
        }
        HostFrame::Prepared { mut prepared } => {
            // Aufgerufene Spiele der fernen Hallen für die Slave-Spielübersicht
            // + den Nachruf merken (Cluster C Stufe 2). Jeder Push ersetzt die
            // Liste vollständig — ein leerer Push (kein Aufruf offen) leert sie
            // bewusst. Cap gegen pathologische Frames.
            prepared.truncate(200);
            namespace.prepared = prepared;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::{MatchBrief, PlayerBrief};

    #[test]
    fn pairing_code_is_eight_digits_and_random() {
        // Format: genau 8 Ziffern, führende Nullen erlaubt.
        let a = gen_pairing_code().unwrap();
        assert!(
            valid_pairing_code(&a),
            "Code nicht 8-stellig numerisch: {a}"
        );
        // Zwei Züge kollidieren praktisch nie – schützt vor einem
        // versehentlich konstanten Generator (z. B. vergessener Zufall).
        let b = gen_pairing_code().unwrap();
        let c = gen_pairing_code().unwrap();
        assert!(a != b || b != c, "Generator liefert konstant {a}");
    }

    #[test]
    fn valid_pairing_code_rejects_non_digits_and_wrong_length() {
        assert!(valid_pairing_code("00000000"));
        assert!(valid_pairing_code("12345678"));
        assert!(!valid_pairing_code("1234567"));
        assert!(!valid_pairing_code("123456789"));
        assert!(!valid_pairing_code("12a45678"));
        assert!(!valid_pairing_code("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
        assert!(!valid_pairing_code(""));
    }

    #[test]
    fn valid_namespace_accepts_uuid_rejects_garbage() {
        assert!(valid_namespace("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
        assert!(!valid_namespace(""));
        assert!(!valid_namespace("not-a-uuid"));
        // 32 Hex ohne Bindestriche – falsche Form.
        assert!(!valid_namespace("a1b2c3d4e5f67890abcdef1234567890abcd"));
        assert!(!valid_namespace("../../../etc/passwd"));
    }

    #[test]
    fn monitor_base_keeps_the_mount_path() {
        // Der Relay läuft hinter nginx unter /bts-relay/ – der Präfix muss
        // im Basis-Pfad erhalten bleiben, sonst zeigen die State-/Asset-
        // URLs des Monitors ins Leere.
        assert_eq!(
            monitor_base("https://badhub.de/bts-relay", "ns1"),
            "/bts-relay/ns1/"
        );
        assert_eq!(
            monitor_base("https://badhub.de/bts-relay/", "ns1"),
            "/bts-relay/ns1/"
        );
        // Relay direkt auf der Domain-Wurzel.
        assert_eq!(monitor_base("https://relay.example.com", "ns1"), "/ns1/");
    }

    fn brief(id: i64) -> MatchBrief {
        MatchBrief {
            match_id: id,
            team_a: vec![PlayerBrief {
                id: 1,
                name: "Anna".into(),
                nationality: Some("GER".into()),
            }],
            team_b: vec![PlayerBrief {
                id: 11,
                name: "Ben".into(),
                nationality: None,
            }],
            event_label: "HE G1".into(),
            best_of_sets: 3,
            target_score: 21,
            cap_score: 30,
            interval_at: Some(11),
            discipline: "mens_singles".into(),
            class_label: String::new(),
            match_number: Some(14),
            scorekeeper: Vec::new(),
        }
    }

    /// Registriert `tx` als Host des Namespace (wie eine frisch
    /// angenommene Host-Verbindung).
    async fn register_host(broker: &Broker, ns: &str, tx: &Tx) {
        let mut map = broker.namespaces.lock().await;
        let namespace = map.entry(ns.into()).or_insert_with(Namespace::new);
        namespace.host = Some(tx.clone());
        namespace.host_last_seen = now_ms();
    }

    /// Legt einen Namespace mit registriertem Host und einem Tablet (an
    /// Feld `court_id`) an; liefert Tablet-Empfangsende + Host-Sender.
    async fn broker_with_tablet(court_id: i64) -> (Broker, mpsc::UnboundedReceiver<Message>, Tx) {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (tx, rx) = mpsc::unbounded_channel();
        let (host_tx, _host_rx) = mpsc::unbounded_channel();
        let mut map = broker.namespaces.lock().await;
        let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
        ns.tablets.insert(court_id, tx);
        ns.host = Some(host_tx.clone());
        ns.host_last_seen = now_ms();
        drop(map);
        (broker, rx, host_tx)
    }

    #[tokio::test]
    async fn host_match_assigned_reaches_the_courts_tablet() {
        let (broker, mut rx, host) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
                match_brief: brief(7),
                on_court_since_ms: None,
            },
            &host,
        )
        .await;
        let msg = rx.try_recv().expect("Tablet bekommt das Frame");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: ServerMsg = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            parsed,
            ServerMsg::MatchAssigned {
                match_brief: brief(7)
            }
        );
    }

    #[tokio::test]
    async fn host_frame_for_unknown_court_is_dropped() {
        let (broker, mut rx, host) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchCleared {
                court_id: 999,
                court_label: "Feld 99".into(),
                hall: String::new(),
            },
            &host,
        )
        .await;
        assert!(rx.try_recv().is_err(), "fremdes Feld bekommt nichts");
    }

    /// Mehr-Hallen-Regression: zwei Felder heißen beide „1", haben aber
    /// verschiedene CourtIDs. Ein `MatchAssigned` für das eine Feld darf
    /// nur dessen Tablet erreichen, nicht das des gleichnamigen Felds.
    #[tokio::test]
    async fn host_frame_routes_by_court_id_not_name() {
        let broker = Broker::new("x".into());
        let (tx_a, mut rx_a) = mpsc::unbounded_channel();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel();
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tx_a); // Halle 1 · Feld „1"
            ns.tablets.insert(401, tx_b); // Halle 2 · Feld „1"
        }
        register_host(&broker, "ns1", &host).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 401,
                court_label: "1".into(),
                hall: String::new(),
                match_brief: brief(7),
                on_court_since_ms: None,
            },
            &host,
        )
        .await;
        // Nur das Tablet von Feld 401 bekommt das Match.
        assert!(rx_b.try_recv().is_ok(), "Feld 401 bekommt das Match");
        assert!(rx_a.try_recv().is_err(), "Feld 101 bleibt unberührt");
    }

    #[tokio::test]
    async fn reassign_same_match_keeps_the_score() {
        let broker = Broker::new("x".into());
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.court_matches.insert(101, brief(7));
            ns.court_scores.insert(101, vec![SetAb { a: 21, b: 15 }]);
        }
        register_host(&broker, "ns1", &host).await;
        // Erneutes MatchAssigned fürs SELBE Match (Tablet-Reconnect) →
        // der gemerkte Satzstand bleibt erhalten.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
                match_brief: brief(7),
                on_court_since_ms: Some(1000),
            },
            &host,
        )
        .await;
        assert_eq!(
            broker.namespaces.lock().await["ns1"].court_scores.get(&101),
            Some(&vec![SetAb { a: 21, b: 15 }])
        );
        // Aufruf-Timer: der Host-Stempel wird übernommen (auch bei Reconnect).
        assert_eq!(
            broker.namespaces.lock().await["ns1"]
                .court_on_court_since
                .get(&101),
            Some(&1000)
        );
        // Echter Match-Wechsel → Satzstand zurückgesetzt, neuer Aufruf-Stempel.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
                match_brief: brief(9),
                on_court_since_ms: Some(2000),
            },
            &host,
        )
        .await;
        let ns = broker.namespaces.lock().await;
        assert!(!ns["ns1"].court_scores.contains_key(&101));
        assert_eq!(ns["ns1"].court_on_court_since.get(&101), Some(&2000));
    }

    #[tokio::test]
    async fn result_ack_resolves_the_pending_request() {
        let broker = Broker::new("x".into());
        let (ack_tx, ack_rx) = oneshot::channel();
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.pending.insert(5, ack_tx);
        }
        register_host(&broker, "ns1", &host).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::ResultAck {
                req_id: 5,
                ok: true,
                error: None,
            },
            &host,
        )
        .await;
        assert_eq!(ack_rx.await.unwrap(), ResultResponse::ok());
    }

    #[tokio::test]
    async fn score_from_tablet_is_forwarded_to_the_host() {
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        // Der Sender muss als aktives Tablet des Felds eingetragen sein —
        // nur der aktuelle Halter darf Scores liefern (is_holder).
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, tablet_tx.clone());
        }
        forward_score(&broker, "ns1", 101, 11, 9, vec![], 0, &tablet_tx).await;
        let msg = host_rx.try_recv().expect("Host bekommt den Score");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: RelayFrame = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            parsed,
            RelayFrame::ScoreUpdate {
                court_id: 101,
                court_label: String::new(),
                score_a: 11,
                score_b: 9,
                sets_history: vec![],
                match_id: 0,
            }
        );
    }

    #[tokio::test]
    async fn score_for_foreign_match_is_dropped() {
        // Stale-Filter (A4, Turnier-Befund HM-03): Das Feld hat Match 9,
        // ein hängengebliebenes Tablet meldet noch Match 7 → Score wird
        // weder gecacht noch an den Host weitergereicht. Mit passender
        // (oder ohne) matchId fließt er normal.
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, tablet_tx.clone());
            ns.court_matches.insert(101, brief(9));
        }
        forward_score(&broker, "ns1", 101, 14, 16, vec![], 7, &tablet_tx).await;
        assert!(host_rx.try_recv().is_err(), "fremder Match-Score verworfen");
        assert!(
            !broker.namespaces.lock().await["ns1"]
                .court_scores
                .contains_key(&101),
            "Cache bleibt leer"
        );
        // Passende matchId → normal verarbeitet.
        forward_score(&broker, "ns1", 101, 1, 0, vec![], 9, &tablet_tx).await;
        assert!(host_rx.try_recv().is_ok(), "passender Score fließt");
        assert!(broker.namespaces.lock().await["ns1"]
            .court_scores
            .contains_key(&101));
    }

    #[tokio::test]
    async fn state_sync_for_foreign_match_is_dropped() {
        // Stale-Filter (A4): Ein state_sync des ALTEN Matches darf den
        // beim Match-Wechsel geleerten court_state nicht wieder befüllen.
        let broker = Broker::new("x".into());
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tablet_tx.clone());
            ns.court_matches.insert(101, brief(9));
        }
        let stale = r#"{"match":{"matchId":7},"finished":false}"#.to_string();
        store_court_state(&broker, "ns1", 101, stale, &tablet_tx).await;
        assert!(
            !broker.namespaces.lock().await["ns1"]
                .court_state
                .contains_key(&101),
            "alter Match-State verworfen"
        );
        let current = r#"{"match":{"matchId":9},"finished":false}"#.to_string();
        store_court_state(&broker, "ns1", 101, current, &tablet_tx).await;
        assert!(broker.namespaces.lock().await["ns1"]
            .court_state
            .contains_key(&101));
    }

    #[tokio::test]
    async fn score_after_match_cleared_is_dropped() {
        // A4-Review-Befund: Nach MatchCleared (Feld frei, kein Eintrag in
        // court_matches) ist ein Frame MIT Match-ID ein Nachzügler des
        // alten Spiels — er darf den geleerten Cache nicht neu befüllen.
        // Nur matchId 0 (alte Tablet-Seite) läuft weiter durch.
        let broker = Broker::new("x".into());
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tablet_tx.clone());
            // KEIN court_matches-Eintrag — wie nach MatchCleared.
        }
        forward_score(&broker, "ns1", 101, 21, 15, vec![], 7, &tablet_tx).await;
        let stale = r#"{"match":{"matchId":7}}"#.to_string();
        store_court_state(&broker, "ns1", 101, stale, &tablet_tx).await;
        let map = broker.namespaces.lock().await;
        assert!(!map["ns1"].court_scores.contains_key(&101));
        assert!(!map["ns1"].court_state.contains_key(&101));
    }

    #[test]
    fn second_host_for_a_namespace_is_refused_while_first_is_live() {
        // R4: Ein LEBENDIGER Host wird nie verdrängt — der zweite
        // Verbindungsversuch (z. B. versehentlich zweiter Master mit
        // derselben install_id) wird abgewiesen.
        let mut ns = Namespace::new();
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();
        assert!(matches!(
            try_claim_host(&mut ns, &tx1, 1_000_000),
            HostClaim::Accepted { superseded: false }
        ));
        // 5 s später (Host hat gerade gepongt): Abweisung.
        ns.host_last_seen = 1_000_000;
        assert!(matches!(
            try_claim_host(&mut ns, &tx2, 1_005_000),
            HostClaim::Refused
        ));
        assert!(ns.host.as_ref().unwrap().same_channel(&tx1));
    }

    #[test]
    fn silent_host_is_superseded_after_stale_timeout() {
        // Zombie-Ablösung (Turnier-Befund 19.07.: tote TCP-Verbindung
        // hielt den Slot 17 Minuten): Ist der Inhaber ≥ HOST_STALE stumm,
        // übernimmt die neue Verbindung.
        let mut ns = Namespace::new();
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();
        try_claim_host(&mut ns, &tx1, 1_000_000);
        let stale_ms = HOST_STALE.as_millis() as u64;
        // 1 ms UNTER der Schwelle: noch abgewiesen (Grenze ist `>=`).
        assert!(matches!(
            try_claim_host(&mut ns, &tx2, 1_000_000 + stale_ms - 1),
            HostClaim::Refused
        ));
        // Genau an der Schwelle: Übernahme.
        assert!(matches!(
            try_claim_host(&mut ns, &tx2, 1_000_000 + stale_ms),
            HostClaim::Accepted { superseded: true }
        ));
        assert!(
            ns.host.as_ref().unwrap().same_channel(&tx2),
            "neuer Host hält den Slot"
        );
    }

    #[test]
    fn superseded_connection_does_not_release_the_new_hosts_slot() {
        // Der wichtigste Korrektheits-Baustein der Ablösung: Stirbt die
        // verdrängte Alt-Verbindung SPÄTER, darf ihr Aufräumen den Slot
        // des neuen Hosts nicht leeren.
        let mut ns = Namespace::new();
        let (old_tx, _old_rx) = mpsc::unbounded_channel();
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        try_claim_host(&mut ns, &old_tx, 0);
        let stale_ms = HOST_STALE.as_millis() as u64;
        try_claim_host(&mut ns, &new_tx, stale_ms);
        // Alt-Verbindung stirbt und räumt auf → Slot bleibt beim neuen Host.
        assert!(!release_host_slot(&mut ns, &old_tx));
        assert!(ns.host.as_ref().unwrap().same_channel(&new_tx));
        // Der echte Inhaber gibt den Slot dagegen frei.
        assert!(release_host_slot(&mut ns, &new_tx));
        assert!(ns.host.is_none());
    }

    #[tokio::test]
    async fn frames_from_superseded_host_are_ignored() {
        // Eine verdrängte Alt-Verbindung, die wieder erwacht, darf den
        // Namespace-Zustand nicht mehr verändern (Sender-Guard).
        let (broker, _rx, host) = broker_with_tablet(101).await;
        let (old_host, _old_rx) = mpsc::unbounded_channel();
        let accepted = handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchCleared {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
            },
            &old_host,
        )
        .await;
        assert!(!accepted, "fremder/abgelöster Sender wird abgewiesen");
        // Der eingetragene Host bleibt unangetastet.
        assert!(broker.namespaces.lock().await["ns1"]
            .host
            .as_ref()
            .unwrap()
            .same_channel(&host));
    }

    #[test]
    fn host_frame_stamps_liveness() {
        // Konstanten-Beziehung der Stale-Erkennung: Ein gesunder Host
        // pongt alle HOST_PING — die Übernahme-Schwelle muss deutlich
        // darüber liegen, sonst würde ein lebendiger Host verdrängt.
        assert!(HOST_STALE >= HOST_PING * 3);
    }

    /// Reconnect-Erkennung: Meldet sich DASSELBE Gerät erneut an einem
    /// belegten Feld, löst es seine tote Vorgänger-Session nahtlos ab —
    /// kein „Feld belegt" fürs eigene Gerät (Turnier-Feedback 18.07.2026:
    /// Tablet verlor nach Netz-Aussetzer den Stand an sein „fremdes" Ich).
    #[tokio::test]
    async fn same_device_reconnect_replaces_old_session() {
        let (broker, mut old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.tablet_devices.insert(101, "dev-x".into());
        }
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "dev-x", &new_tx).await;
        assert!(
            matches!(res, AttachResult::Active),
            "eigenes Gerät kommt sofort rein"
        );
        // Die alte Session wird gesperrt, damit sie nicht weiterzählt.
        let msg = old_rx
            .try_recv()
            .expect("alte Session bekommt SessionSuperseded");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: ServerMsg = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(parsed, ServerMsg::SessionSuperseded);
    }

    /// Ein FREMDES Gerät sieht weiterhin „belegt" (Übernehmen-Dialog).
    #[tokio::test]
    async fn foreign_device_still_sees_occupied() {
        let (broker, mut old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            map.get_mut("ns1")
                .unwrap()
                .tablet_devices
                .insert(101, "dev-x".into());
        }
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "dev-anders", &new_tx).await;
        assert!(
            matches!(res, AttachResult::Occupied),
            "fremdes Gerät bleibt draußen"
        );
        assert!(old_rx.try_recv().is_err(), "alte Session bleibt aktiv");
    }

    /// Nachlaufende Frames einer ABGELÖSTEN Session (Reconnect-Reclaim/
    /// Übernahme) dürfen Cache und Host nicht mehr erreichen — sonst kehrt
    /// genau der alte Stand zurück, den die Reconnect-Logik verhindert.
    #[tokio::test]
    async fn superseded_session_frames_are_dropped() {
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        let (holder_tx, _holder_rx) = mpsc::unbounded_channel();
        let (old_tx, _old_rx) = mpsc::unbounded_channel::<Message>();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, holder_tx); // aktueller Halter ist ein ANDERER
        }
        forward_score(&broker, "ns1", 101, 3, 1, vec![], 0, &old_tx).await;
        store_court_state(&broker, "ns1", 101, "{\"alt\":true}".into(), &old_tx).await;
        assert!(
            host_rx.try_recv().is_err(),
            "Score der alten Session wird verworfen"
        );
        let map = broker.namespaces.lock().await;
        assert!(
            !map.get("ns1").unwrap().court_state.contains_key(&101),
            "alter Stand landet nicht im Cache"
        );
    }

    fn prepared(match_id: i64, hall: &str) -> relay_proto::PreparedMatch {
        relay_proto::PreparedMatch {
            match_id,
            hall: hall.into(),
            discipline: "mens_singles".into(),
            class_label: "A".into(),
            round_name: "G1".into(),
            team_a: vec![PlayerBrief {
                id: 1,
                name: "Anna Weber".into(),
                nationality: Some("GER".into()),
            }],
            team_b: vec![PlayerBrief {
                id: 2,
                name: "Bea Schulz".into(),
                nationality: None,
            }],
            match_number: Some(101),
            called_at_ms: 1_700_000_000_000,
        }
    }

    /// `HostFrame::Prepared` ersetzt die Liste vollständig; ein leerer Push
    /// leert sie (kein Aufruf mehr offen). Grundlage der Slave-Spielübersicht.
    #[tokio::test]
    async fn prepared_frame_replaces_and_clears_list() {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host, _hrx) = mpsc::unbounded_channel();
        register_host(&broker, "ns1", &host).await;

        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Prepared {
                prepared: vec![prepared(42, "Halle 1"), prepared(43, "Halle 2")],
            },
            &host,
        )
        .await;
        {
            let map = broker.namespaces.lock().await;
            assert_eq!(map.get("ns1").unwrap().prepared.len(), 2);
        }

        // Zweiter Push mit nur einem Spiel ersetzt die Liste vollständig.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Prepared {
                prepared: vec![prepared(43, "Halle 2")],
            },
            &host,
        )
        .await;
        {
            let map = broker.namespaces.lock().await;
            let p = &map.get("ns1").unwrap().prepared;
            assert_eq!(p.len(), 1);
            assert_eq!(p[0].match_id, 43);
        }

        // Leerer Push leert die Liste (alle Aufrufe zurückgenommen/aufs Feld).
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Prepared { prepared: vec![] },
            &host,
        )
        .await;
        let map = broker.namespaces.lock().await;
        assert!(map.get("ns1").unwrap().prepared.is_empty());
    }

    /// Der Hallenfilter der Ansage-Antwort zeigt jeder Halle nur ihre eigenen
    /// aufgerufenen Spiele (leere Halle am Match = überall sichtbar).
    #[test]
    fn prepared_hall_filter_matches_court_rule() {
        let all = [prepared(42, "Halle 1"), prepared(43, "Halle 2"), {
            let mut p = prepared(44, "");
            p.hall = String::new();
            p
        }];
        let for_hall = |hall: &str| -> Vec<i64> {
            all.iter()
                .filter(|p| hall.is_empty() || p.hall.is_empty() || p.hall == hall)
                .map(|p| p.match_id)
                .collect()
        };
        assert_eq!(for_hall("Halle 1"), vec![42, 44]);
        assert_eq!(for_hall("Halle 2"), vec![43, 44]);
        assert_eq!(for_hall(""), vec![42, 43, 44]);
    }

    /// Alte Tablet-Seiten (ohne deviceId) zählen nie als „dasselbe Gerät" —
    /// leere Kennungen dürfen einander nicht matchen.
    #[tokio::test]
    async fn empty_device_id_never_matches() {
        let (broker, _old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            map.get_mut("ns1")
                .unwrap()
                .tablet_devices
                .insert(101, String::new());
        }
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "", &new_tx).await;
        assert!(
            matches!(res, AttachResult::Occupied),
            "leer = wie bisher belegt"
        );
    }
}
