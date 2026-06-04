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
    /// CourtID → Sende-Ende zur Tablet-WebSocket.
    tablets: HashMap<i64, Tx>,
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
    /// Vollständige Feld-Liste (vom Host via `HostFrame::Courts` gepusht) für
    /// das Cloud-Feldwechsel-Menü des Tablets (`/{ns}/courts`).
    courts: Vec<CourtBrief>,
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
            tablets: HashMap::new(),
            court_state: HashMap::new(),
            court_matches: HashMap::new(),
            court_scores: HashMap::new(),
            court_on_court_since: HashMap::new(),
            court_labels: HashMap::new(),
            courts: Vec::new(),
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
    /// Öffentliche Basis-URL für QR-Codes, z. B. `https://badhub.de/bts-relay`.
    public_base: String,
}

impl Broker {
    fn new(public_base: String) -> Self {
        Self {
            namespaces: Arc::new(Mutex::new(HashMap::new())),
            public_base,
        }
    }
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_ansi(false).init();

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
        .map(|c| serde_json::json!({ "id": c.id, "label": c.label }))
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
            winner: body.winner,
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
    let mut ping = tokio::time::interval(HEARTBEAT);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Text(t) => {
                        match serde_json::from_str::<TabletMsg>(t.as_str()) {
                            Ok(TabletMsg::Identify { court_id, .. }) => {
                                match attach_tablet(&broker, &ns, court_id, &tx).await {
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
                            Ok(TabletMsg::TakeOver) => {
                                if let (Some(c), false) = (court, active) {
                                    take_over_court(&broker, &ns, c, &tx).await;
                                    active = true;
                                    tracing::info!("Tablet übernimmt Feld {c} (Namespace '{ns}')");
                                }
                            }
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_score(&broker, &ns, c, score_a, score_b, sets_history).await;
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_battery(&broker, &ns, c, percent, charging).await;
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_alert(&broker, &ns, c, injury, official).await;
                                }
                            }
                            Ok(TabletMsg::StateSync { state }) => {
                                if let (Some(c), true) = (court, active) {
                                    store_court_state(&broker, &ns, c, state).await;
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
async fn attach_tablet(broker: &Broker, ns: &str, court_id: i64, tx: &Tx) -> AttachResult {
    let mut map = broker.namespaces.lock().await;
    if !map.contains_key(ns) && map.len() >= MAX_NAMESPACES {
        tracing::warn!("Namespace-Limit erreicht – Tablet für '{ns}' abgewiesen");
        return AttachResult::Rejected;
    }
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if namespace.tablets.contains_key(&court_id) {
        return AttachResult::Occupied;
    }
    if namespace.tablets.len() >= MAX_TABLETS_PER_NS {
        tracing::warn!("Namespace '{ns}' am Tablet-Limit – Feld {court_id} abgewiesen");
        return AttachResult::Rejected;
    }
    namespace.tablets.insert(court_id, tx.clone());
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
async fn take_over_court(broker: &Broker, ns: &str, court_id: i64, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if let Some(old) = namespace.tablets.insert(court_id, tx.clone()) {
        let _ = old.send(text(&ServerMsg::SessionSuperseded));
    }
    // Laufenden Spielstand an das übernehmende Tablet übergeben.
    if let Some(state) = namespace.court_state.get(&court_id) {
        let _ = tx.send(text(&ServerMsg::StateRestore {
            state: state.clone(),
        }));
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_id,
            court_label,
        }));
    }
}

/// Speichert den gespiegelten Spielzustand des aktiven Tablets am Feld.
async fn store_court_state(broker: &Broker, ns: &str, court_id: i64, state: String) {
    if state.len() > MAX_STATE_LEN {
        return;
    }
    let mut map = broker.namespaces.lock().await;
    if let Some(namespace) = map.get_mut(ns) {
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

/// Leitet einen Live-Score von einem Tablet an den Host weiter und merkt
/// ihn zugleich für die Court-Monitor-Anzeige.
async fn forward_score(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    score_a: i64,
    score_b: i64,
    sets_history: Vec<SetAb>,
) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
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
async fn forward_alert(broker: &Broker, ns: &str, court_id: i64, injury: bool, official: bool) {
    let map = broker.namespaces.lock().await;
    let Some(namespace) = map.get(ns) else {
        return;
    };
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

/// Die Host-Verbindung (bts-light) eines Namespace. Genau eine ist erlaubt;
/// eine zweite wird abgewiesen, damit kein fremder Host die Kontrolle
/// übernimmt.
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
        if namespace.host.is_some() {
            tracing::warn!("Zweiter Host für Namespace '{ns}' abgewiesen");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        namespace.host = Some(tx.clone());
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

    let mut ping = tokio::time::interval(HEARTBEAT);
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Text(t) => {
                        if let Ok(frame) = serde_json::from_str::<HostFrame>(t.as_str()) {
                            handle_host_frame(&broker, &ns, frame).await;
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

    // Aufräumen: Host-Slot freigeben (nur wenn noch unserer), offene
    // Ergebnis-Übermittlungen mit Fehler beantworten.
    {
        let mut map = broker.namespaces.lock().await;
        if let Some(namespace) = map.get_mut(&ns) {
            if namespace
                .host
                .as_ref()
                .map(|h| h.same_channel(&tx))
                .unwrap_or(false)
            {
                namespace.host = None;
            }
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

/// Verarbeitet ein Frame vom Host: an das passende Tablet weiterleiten bzw.
/// eine wartende Ergebnis-Übermittlung abschließen.
async fn handle_host_frame(broker: &Broker, ns: &str, frame: HostFrame) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    match frame {
        HostFrame::MatchAssigned {
            court_id,
            court_label,
            match_brief,
            on_court_since_ms,
        } => {
            // Feldname (Anzeige) merken – der Monitor liest ihn.
            if !court_label.is_empty() {
                namespace.court_labels.insert(court_id, court_label);
            }
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
        } => {
            if !court_label.is_empty() {
                namespace.court_labels.insert(court_id, court_label);
            }
            namespace.court_matches.remove(&court_id);
            namespace.court_scores.remove(&court_id);
            namespace.court_state.remove(&court_id);
            namespace.court_on_court_since.remove(&court_id);
            if let Some(t) = namespace.tablets.get(&court_id) {
                let _ = t.send(text(&ServerMsg::MatchCleared));
            }
        }
        HostFrame::ResultAck { req_id, ok, error } => {
            if let Some(pending) = namespace.pending.remove(&req_id) {
                let _ = pending.send(ResultResponse { ok, error });
            }
        }
        HostFrame::Courts { courts } => {
            // Vollständige Feld-Liste für das Cloud-Feldwechsel-Menü merken.
            namespace.courts = courts;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::{MatchBrief, PlayerBrief};

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
            match_number: Some(14),
            scorekeeper: Vec::new(),
        }
    }

    /// Legt einen Namespace mit einem Tablet an (an Feld `court_id`) und
    /// gibt dessen Empfangsende zurück.
    async fn broker_with_tablet(court_id: i64) -> (Broker, mpsc::UnboundedReceiver<Message>) {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (tx, rx) = mpsc::unbounded_channel();
        let mut map = broker.namespaces.lock().await;
        let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
        ns.tablets.insert(court_id, tx);
        drop(map);
        (broker, rx)
    }

    #[tokio::test]
    async fn host_match_assigned_reaches_the_courts_tablet() {
        let (broker, mut rx) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                match_brief: brief(7),
                on_court_since_ms: None,
            },
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
        let (broker, mut rx) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchCleared {
                court_id: 999,
                court_label: "Feld 99".into(),
            },
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
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tx_a); // Halle 1 · Feld „1"
            ns.tablets.insert(401, tx_b); // Halle 2 · Feld „1"
        }
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 401,
                court_label: "1".into(),
                match_brief: brief(7),
                on_court_since_ms: None,
            },
        )
        .await;
        // Nur das Tablet von Feld 401 bekommt das Match.
        assert!(rx_b.try_recv().is_ok(), "Feld 401 bekommt das Match");
        assert!(rx_a.try_recv().is_err(), "Feld 101 bleibt unberührt");
    }

    #[tokio::test]
    async fn reassign_same_match_keeps_the_score() {
        let broker = Broker::new("x".into());
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.court_matches.insert(101, brief(7));
            ns.court_scores.insert(101, vec![SetAb { a: 21, b: 15 }]);
        }
        // Erneutes MatchAssigned fürs SELBE Match (Tablet-Reconnect) →
        // der gemerkte Satzstand bleibt erhalten.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                match_brief: brief(7),
                on_court_since_ms: Some(1000),
            },
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
                match_brief: brief(9),
                on_court_since_ms: Some(2000),
            },
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
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.pending.insert(5, ack_tx);
        }
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::ResultAck {
                req_id: 5,
                ok: true,
                error: None,
            },
        )
        .await;
        assert_eq!(ack_rx.await.unwrap(), ResultResponse::ok());
    }

    #[tokio::test]
    async fn score_from_tablet_is_forwarded_to_the_host() {
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
        }
        forward_score(&broker, "ns1", 101, 11, 9, vec![]).await;
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
            }
        );
    }

    #[tokio::test]
    async fn second_host_for_a_namespace_is_refused() {
        let broker = Broker::new("x".into());
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut map = broker.namespaces.lock().await;
        let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
        ns.host = Some(tx);
        // Genau diese Bedingung prüft host_conn vor dem Registrieren.
        assert!(ns.host.is_some(), "zweiter Host würde abgewiesen");
    }
}
