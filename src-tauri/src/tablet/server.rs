//! Eingebetteter HTTP+WebSocket-Server für die Schiedsrichter-Tablets
//! (LAN-Modus).
//!
//! bts-light ist damit der zentrale Hub: Tablets laden die Spielzettel-UI,
//! binden sich an einen Court, bekommen das von BTP zugewiesene Match,
//! zählen Punkte (Live-Score → Liveticker) und schreiben am Spielende das
//! Ergebnis via `SENDUPDATE` zurück nach BTP.
//!
//! Im Cloud-Modus läuft dieser Server nicht – stattdessen verbindet sich
//! [`crate::tablet::relay_client`] ausgehend zum Relay. Die Kernlogik
//! ([`ServerCtx`], [`process_result`], [`handle_score`], [`match_brief`])
//! ist `pub(crate)` und wird von beiden Modi geteilt.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};

use relay_proto::{
    device_code, html_escape, path_encode, MatchBrief, PlayerBrief, ResultBody, ResultResponse,
    ServerMsg, SetAb, TabletMsg,
};

use crate::badhub::diff::Update;
use crate::badhub::payload::build_tupdate;
use crate::badhub::push;
use crate::btp::model::{BtpMatch, MatchStatus};
use crate::btp::{client, proto};
use crate::config::{AppConfig, CourtMonitorConfig};
use crate::tablet::assets::{self, TABLET_HTML};
use crate::tablet::monitor;
use crate::tablet::state::TabletState;

/// Fester Port des Tablet-Servers im Hallen-LAN.
pub const TABLET_PORT: u16 = 8088;

/// Geteilter Kontext der Tablet-Logik – im LAN-Modus von den HTTP-/WS-
/// Handlern genutzt, im Cloud-Modus vom Relay-Client.
pub struct ServerCtx {
    pub tablet: Arc<TabletState>,
    config: AppConfig,
    pub(crate) http: reqwest::Client,
    /// Request-IDs für Liveticker-Pushes. Eigener Zähler – Badhub spiegelt
    /// `rid` nur zurück, dedupliziert nicht; eine Kollision mit dem
    /// Sync-Loop wäre folgenlos.
    rid: AtomicU64,
    /// Verzeichnis der hochgeladenen Court-Monitor-Werbebilder (`court-ads`).
    pub monitor_dir: PathBuf,
    /// Pfad zur `config.json` – der Court-Monitor lädt seine Konfiguration
    /// frisch von dort, damit Änderungen im Tool ohne Neustart greifen.
    config_path: PathBuf,
    /// Pfad zur Monitor-Zuweisungsdatei (Gerät → CourtID). Wird frisch
    /// gelesen, damit Zuweisungen aus dem Tool sofort greifen.
    pub assignments_path: PathBuf,
    /// App-Log-Verzeichnis (wie „Logs öffnen"). Hierhin schreibt der Server die
    /// von den Tablets hochgeladenen Diagnoselogs (Unterordner `tablet-logs`).
    pub log_dir: PathBuf,
}

impl ServerCtx {
    pub fn new(
        tablet: Arc<TabletState>,
        config: AppConfig,
        http: reqwest::Client,
        monitor_dir: PathBuf,
        config_path: PathBuf,
        assignments_path: PathBuf,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            tablet,
            config,
            http,
            rid: AtomicU64::new(1),
            monitor_dir,
            config_path,
            assignments_path,
            log_dir,
        }
    }

    fn next_rid(&self) -> u64 {
        self.rid.fetch_add(1, Ordering::Relaxed)
    }

    /// Lädt die aktuelle Court-Monitor-Konfiguration frisch von der Platte.
    /// Schlägt das Lesen fehl, gelten die Default-Werte.
    pub fn monitor_config(&self) -> CourtMonitorConfig {
        AppConfig::load_from(&self.config_path)
            .map(|c| c.court_monitor)
            .unwrap_or_default()
    }

    /// Gesamte App-Config frisch von der Platte (Default bei Fehler) – für
    /// Aufrufer, die mehrere Felder daraus brauchen, ohne doppelt zu lesen.
    pub fn app_config(&self) -> AppConfig {
        AppConfig::load_from(&self.config_path).unwrap_or_default()
    }

    /// Lädt die Geräte→Target-Zuweisungen frisch von der Platte. Ein
    /// Target ist entweder eine CourtID (klassischer Court-Monitor) oder
    /// ein Info-Display (`InfoOverview` / `InfoPreparation`).
    pub fn monitor_assignments(&self) -> HashMap<String, relay_proto::MonitorTarget> {
        monitor::read_assignments(&self.assignments_path)
    }
}

/// Startet den Server auf `0.0.0.0:8088` und bedient ihn, bis der Task
/// abgebrochen wird.
pub async fn run(ctx: Arc<ServerCtx>) -> std::io::Result<()> {
    let app = Router::new()
        // TV-Launcher: kurze Root-Adresse landet auf einem Auswahl-Menü
        // (Fernbedienung statt langer ?halle=-URLs). Kurz-Pfade leiten direkt.
        .route("/", get(tv_page))
        .route("/tv", get(tv_page))
        .route("/status", get(index))
        .route("/alle", get(|| async { Redirect::to("/info/overview") }))
        .route("/next", get(|| async { Redirect::to("/info/preparation") }))
        .route("/h/{n}", get(hall_short))
        .route("/court/{id}", get(court_page))
        .route("/courts", get(courts_list))
        .route("/felder", get(lobby_page))
        .route("/court/{id}/display", get(monitor_page))
        .route("/court/{id}/state", get(monitor_state))
        .route("/monitor", get(monitor_device_page))
        .route("/monitor/state", get(monitor_device_state))
        .route("/qr/{id}", get(qr_svg))
        .route("/flags/{file}", get(flag_route))
        .route("/ads/{file}", get(ad_image))
        .route("/health", get(health))
        .route("/info/overview", get(info_overview_page))
        .route("/info/preparation", get(info_preparation_page))
        .route("/info/preparation/state", get(info_preparation_state))
        .route("/info/winners", get(info_winners_page))
        .route("/info/winners/state", get(info_winners_state))
        .route("/info/club-logo", get(info_club_logo))
        .route("/info/announce/freetext", get(info_announce_freetext))
        .route("/info/ad", get(info_ad_page))
        .route("/info/ad/state", get(info_ad_state))
        .route("/combo", get(combo_page))
        .route("/combo/state", get(combo_state))
        .route("/result", post(result))
        .route("/tablet-log", post(tablet_log))
        .route("/pi-log", post(pi_log))
        .route("/ws", get(ws_upgrade))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", TABLET_PORT)).await?;
    tracing::info!("Tablet-Server lauscht auf http://{}", lan_host());
    axum::serve(listener, app).await
}

/// LAN-Adresse `<ip>:<port>` für Tablet-URLs und QR-Codes.
pub fn lan_host() -> String {
    match local_ip_address::local_ip() {
        Ok(ip) => format!("{ip}:{TABLET_PORT}"),
        Err(_) => format!("localhost:{TABLET_PORT}"),
    }
}

// ─────────────────────────────── HTTP-Routen ──────────────────────────────

/// TV-Launcher (`/` und `/tv`): Vollbild-Auswahlmenü, per Fernbedienung
/// bedienbar — so muss am Smart-TV nur einmal die kurze Adresse getippt werden
/// statt langer `?halle=`-URLs.
async fn tv_page(State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    // Konfigurierten badhub-Liveticker einsetzen, damit der Launcher auch die
    // öffentlichen Online-Anzeigen je Halle anbieten kann. Defensiv: nur eine
    // saubere http(s)-URL ohne Zeichen, die das JS-String-Literal aufbrechen
    // könnten (Anführungszeichen/Backslash/Spitzklammern/Whitespace) – sonst
    // leer (keine Online-Kacheln).
    let live = ctx.app_config().badhub.live_url;
    let safe = (live.starts_with("https://") || live.starts_with("http://"))
        && !live
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\' | '<' | '>' | '`'));
    let body = assets::TV_HTML.replace("__LIVE_URL__", if safe { &live } else { "" });
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Kurz-Pfad `/h/{n}` → leitet auf die Court-Übersicht der n-ten Halle
/// (1-basiert, Hallen alphabetisch sortiert). Unbekannte Nummer → alle Hallen.
/// Spart das Tippen langer `?halle=`-URLs an der TV-Fernbedienung.
async fn hall_short(State(ctx): State<Arc<ServerCtx>>, Path(n): Path<usize>) -> Redirect {
    let mut halls: Vec<String> = ctx
        .tablet
        .overview()
        .into_iter()
        .map(|c| c.location)
        .filter(|l| !l.is_empty())
        .collect();
    halls.sort();
    halls.dedup();
    match n.checked_sub(1).and_then(|i| halls.get(i)) {
        Some(h) => Redirect::to(&format!("/info/overview?halle={}", path_encode(h))),
        None => Redirect::to("/info/overview"),
    }
}

/// Landing-Page (Debug, `/status`): zeigt die Tablet-Adressen je Court. Die URL
/// trägt die stabile CourtID, der angezeigte Text den Feldnamen.
async fn index(State(ctx): State<Arc<ServerCtx>>) -> Html<String> {
    let host = lan_host();
    let courts = ctx.tablet.courts();
    let mut rows = String::new();
    for c in &courts {
        // Anzeigename inkl. Halle bei Mehr-Hallen-Turnieren ("Halle 2 · 6").
        let label = ctx.tablet.court_display_label(c.id);
        rows.push_str(&format!(
            "<li><b>{}</b> &mdash; <a href=\"/court/{id}\">/court/{id}</a> \
             &middot; <a href=\"/qr/{id}\">QR</a></li>",
            html_escape(&label),
            id = c.id,
        ));
    }
    if courts.is_empty() {
        rows.push_str(
            "<li><i>Noch keine Courts geladen – bts-light muss zuerst mit BTP \
             verbunden sein.</i></li>",
        );
    }
    Html(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>bts-light Tablet-Server</title>\
         <style>body{{font-family:system-ui;max-width:40rem;margin:2rem auto;padding:0 1rem}}\
         code{{background:#f1f5f9;padding:.1em .4em;border-radius:.25rem}}\
         li{{margin:.3rem 0}}</style>\
         <h1>&#127992; bts-light Tablet-Server</h1>\
         <p>LAN-Adresse <code>http://{host}</code></p>\
         <h2>Spielfelder</h2><ul>{rows}</ul>"
    ))
}

/// Liefert die Tablet-UI für ein Feld (per CourtID; kein Caching – immer
/// frisch). Der Platzhalter `__COURT_ID__` trägt die Identität,
/// `__COURT_LABEL__` den Feldnamen für die Anzeige.
async fn court_page(
    State(ctx): State<Arc<ServerCtx>>,
    Path(court_id): Path<i64>,
) -> impl IntoResponse {
    let label = court_label_for(&ctx, court_id);
    // PIN fürs Einstellungs-Menü (Feldwechsel) – Live-Config. NUR Ziffern
    // (Bedien-PIN; leer → Default „0000"). Ziffern sind in einem JS-String-
    // Literal unkritisch → kein Escape nötig (Code-Review-Hinweis: html_escape
    // wäre für einen JS-Kontext der falsche Escaper).
    let pin: String = ctx
        .app_config()
        .tablet_settings_pin
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(8)
        .collect();
    let pin = if pin.is_empty() {
        "0000".to_string()
    } else {
        pin
    };
    tracing::info!("Tablet-Seite ausgeliefert für Feld {court_id} ('{label}')");
    let body = TABLET_HTML
        .replace("__COURT_ID__", &court_id.to_string())
        .replace("__COURT_LABEL__", &html_escape(&label))
        .replace("__TABLET_PIN__", &pin);
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Feldliste (CourtID + Anzeige-Label) für den Feldwechsel im PIN-Menü des
/// Tablets – so kann das Tablet ohne QR-Scan auf ein anderes Feld umschalten,
/// und die Felder-Lobby (`/felder`) baut daraus ihre Kacheln.
/// Bewusst ohne Auth (wie die anderen Anzeige-Routen): Nutzung nur im Hallen-LAN.
/// Enthält die Spielernamen der laufenden Partie (`pairing`) – dieselbe Exposition
/// wie Zähltablett und Court-Monitor, die die Namen im LAN ohnehin anzeigen.
async fn courts_list(State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    // Spielernamen eines Teams kompakt verbinden ("Müller / Schmidt").
    let names = |players: &[crate::btp::model::BtpPlayer]| {
        players
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(" / ")
    };
    let items: Vec<serde_json::Value> = ctx
        .tablet
        .courts()
        .into_iter()
        .map(|c| {
            // Belegt = ein Tablet zählt das Feld bereits (Doppelbelegung-Schutz).
            // Paarung/Untertitel für die Felder-Lobby, damit man sieht, was auf
            // dem Feld läuft, bevor man es antippt.
            let m = ctx.tablet.match_for_court(c.id);
            let (pairing, sub) = match &m {
                Some(m) => {
                    let a = names(&m.team1);
                    let b = names(&m.team2);
                    let pairing = if a.is_empty() && b.is_empty() {
                        String::new()
                    } else {
                        format!("{a} — {b}")
                    };
                    let sub = format!("{} {}", m.draw_name, m.round_name)
                        .trim()
                        .to_string();
                    (pairing, sub)
                }
                None => (String::new(), String::new()),
            };
            serde_json::json!({
                "id": c.id,
                "label": ctx.tablet.court_display_label(c.id),
                "occupied": ctx.tablet.court_occupied(c.id),
                "pairing": pairing,
                "sub": sub,
            })
        })
        .collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::Value::Array(items)),
    )
}

/// Felder-Lobby (`/felder`): Start-Seite fürs Zähltablett. Listet alle Felder
/// (Live-Belegung via `/courts`-Poll), Tippen auf ein Feld führt auf
/// `/court/{id}` (zählen bzw. – bei Belegung – die bestehende Übernahme-Abfrage).
async fn lobby_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::LOBBY_HTML),
    )
}

/// Fester (verbandsweiter) Token zum Weiterleiten der Tablet-Logs an badhub –
/// derselbe wie Diagnose-/Pi-Log. Nicht geheim (Bedien-Token, nicht-PII-Daten).
const TABLET_LOG_TOKEN: &str = "d896d5c45f1dfe72d324be2da0dcc8031e447809f9a3c1ce";

#[derive(serde::Deserialize)]
struct TabletLogQuery {
    #[serde(default)]
    court: i64,
}

/// Nimmt das Diagnoselog eines Zähltablets entgegen (LAN, ohne Auth wie die
/// anderen Hallen-Routen): legt es lokal unter `<log_dir>/tablet-logs/court-<id>.log`
/// ab (über „Logs öffnen" greifbar) UND leitet es – sofern Internet da ist – an
/// die badhub-Cloud weiter (fire-and-forget, scheitert still ohne Uplink).
async fn tablet_log(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<TabletLogQuery>,
    body: String,
) -> impl IntoResponse {
    if body.len() > 2 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }
    let court_id = q.court;
    // 1) Lokal beim Turnier-PC ablegen (auch offline verfügbar).
    let dir = ctx.log_dir.join("tablet-logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("court-{court_id}.log")), &body);
    // 2) An die Cloud weiterleiten – Geräte-ID inkl. install_id, damit sich
    //    verschiedene PCs/Turniere nicht gegenseitig überschreiben.
    let install = ctx.app_config().install_id;
    let device_id = if install.is_empty() {
        format!("court-{court_id}")
    } else {
        format!("{install}-court-{court_id}")
    };
    let http = ctx.http.clone();
    tokio::spawn(async move {
        let _ = http
            .post("https://badhub.de/api/tablet_log.php")
            .bearer_auth(TABLET_LOG_TOKEN)
            .header("X-Device-Id", device_id)
            .header(header::CONTENT_TYPE, "text/plain")
            .timeout(std::time::Duration::from_secs(8))
            .body(body)
            .send()
            .await;
    });
    StatusCode::OK
}

#[derive(serde::Deserialize)]
struct PiLogQuery {
    /// Geräte-ID des Pi-Monitors (= `pi-<CPU-Serial>`), vom Pi-Startskript
    /// mitgeschickt. Bestimmt den Dateinamen + die Cloud-Geräte-ID.
    #[serde(default)]
    device: String,
}

/// Nimmt das Verbindungslog eines Pi-Court-Monitors entgegen (LAN, ohne Auth
/// wie die anderen Hallen-Routen). Einheitlich mit den Tablets: der Pi postet
/// im LAN an den PC (plain HTTP – kein TLS/keine Pi-Uhr nötig), der PC legt es
/// lokal unter `<log_dir>/pi-logs/<device>.log` ab UND leitet es – sofern
/// Internet da ist – an die badhub-Cloud weiter (fire-and-forget).
async fn pi_log(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<PiLogQuery>,
    body: String,
) -> impl IntoResponse {
    if body.len() > 2 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }
    // Geräte-ID auf dateinamen-/header-sichere Zeichen reduzieren.
    let id: String = q
        .device
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    let id = if id.is_empty() {
        "pi-unbekannt".to_string()
    } else {
        id
    };
    // 1) Lokal beim Turnier-PC ablegen (auch offline verfügbar, „Logs öffnen").
    let dir = ctx.log_dir.join("pi-logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{id}.log")), &body);
    // 2) An die Cloud weiterleiten (gleicher Token + Endpoint wie der frühere
    //    Direkt-Upload der Pis). Bewusst OHNE install_id-Präfix (anders als bei
    //    Tablets): die Pi-Serial ist global eindeutig → ein Cloud-Log je
    //    physischem Pi über alle Turniere (gut für Ferndiagnose desselben Geräts).
    let http = ctx.http.clone();
    tokio::spawn(async move {
        let _ = http
            .post("https://badhub.de/api/pi_log.php")
            .bearer_auth(TABLET_LOG_TOKEN)
            .header("X-Device-Id", id)
            .header(header::CONTENT_TYPE, "text/plain")
            .timeout(std::time::Duration::from_secs(8))
            .body(body)
            .send()
            .await;
    });
    StatusCode::OK
}

/// Löst die CourtID auf ihre Anzeige-Bezeichnung auf. Bei Mehr-Hallen-
/// Turnieren `"{Halle} · {Feld}"` (z. B. „Halle 2 · 6"), sonst nur der
/// Feldname. Leer, wenn die ID kein bekanntes Feld ist (z. B. nach einem
/// Turnierwechsel).
fn court_label_for(ctx: &ServerCtx, court_id: i64) -> String {
    ctx.tablet.court_display_label(court_id)
}

/// QR-Code (SVG), der auf die Tablet-URL des Felds (per CourtID) zeigt.
async fn qr_svg(Path(court_id): Path<i64>) -> impl IntoResponse {
    let url = format!("http://{}/court/{}", lan_host(), court_id);
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

/// Optionaler `device`-Query-Param. Wird von den Info-Pages
/// (`overview.html`, `preparation.html`, `ad.html`) mitgegeben, damit
/// der State-Poll als „Lebenszeichen" gezaehlt wird – sonst gilt der
/// Pi auf einer Info-Page als offline, weil `record_monitor_poll`
/// nur in `/monitor/state` aufgerufen wird (Code-Review v0.9.22).
#[derive(serde::Deserialize, Default)]
struct DeviceHeartbeat {
    #[serde(default)]
    device: Option<String>,
}

/// Markiert das Geraet als „gesehen", falls eine Device-ID im Query
/// kam. Geteilte Hilfsfunktion fuer alle Info-State-Endpoints und
/// `/health`.
fn note_heartbeat(ctx: &ServerCtx, q: &DeviceHeartbeat) {
    if let Some(d) = q.device.as_deref() {
        if !d.is_empty() && d.len() <= 64 {
            // Rueckgabewert (Fernbefehl) ignorieren — Info-Pages
            // verarbeiten Commands ueber den separaten /monitor/state-Poll.
            let _ = ctx.tablet.record_monitor_poll(d);
        }
    }
}

/// Status-Schnappschuss für die bts-light-Oberfläche. Optional
/// `?device=<id>` als Lebenszeichen-Markierung von der Info-Page.
async fn health(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> Json<serde_json::Value> {
    note_heartbeat(&ctx, &q);
    Json(serde_json::json!({
        "ok": true,
        "courts": ctx.tablet.overview(),
        // Server-Zeit, damit das Tablet seinen Uhr-Offset zum Server
        // bestimmen kann und Pausen-`endsAt` in Server-Zeit setzt — so
        // zeigen Tablet und TV denselben Countdown (sonst Drift durch
        // abweichende Geraeteuhren). v0.9.32.
        "serverNowMs": monitor::now_ms(),
    }))
}

// ─────────────────────────────── Court-Monitor ────────────────────────────

/// Rendert `monitor.html` mit den Platzhaltern. `base` ist der URL-Präfix
/// für Unter-Ressourcen (`/` im LAN), `mode` ist `fixed` oder `device`.
fn render_monitor_html(mode: &str, base: &str, court_label: &str) -> String {
    assets::MONITOR_HTML
        .replace("__MODE__", mode)
        .replace("__BASE__", base)
        .replace("__COURT_LABEL__", &html_escape(court_label))
}

/// Liefert die Court-Monitor-Anzeige fest für ein Feld
/// (`/court/{id}/display`, per CourtID).
async fn monitor_page(
    State(ctx): State<Arc<ServerCtx>>,
    Path(court_id): Path<i64>,
) -> impl IntoResponse {
    let label = court_label_for(&ctx, court_id);
    tracing::info!("Court-Monitor-Seite (fest) ausgeliefert für Feld {court_id} ('{label}')");
    let body = render_monitor_html("fixed", "/", &label);
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Liefert die Court-Monitor-Anzeige im Geräte-Modus (`/monitor`) – das
/// Gerät bekommt sein Feld erst über die Zuweisung im Tool.
async fn monitor_device_page() -> impl IntoResponse {
    let body = render_monitor_html("device", "/", "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Anzeige-Zustand eines fest verdrahteten Feldes (per CourtID), im
/// Sekundentakt gepollt.
async fn monitor_state(
    State(ctx): State<Arc<ServerCtx>>,
    Path(court_id): Path<i64>,
) -> impl IntoResponse {
    let label = court_label_for(&ctx, court_id);
    let court = ctx.tablet.monitor_court(court_id);
    let cfg = ctx.app_config();
    let ads = monitor::list_ads(&ctx.monitor_dir);
    let state = monitor::build_monitor_state(
        court_id,
        label,
        court,
        &cfg.court_monitor,
        &cfg.call_timer,
        ads,
    );
    ([(header::CACHE_CONTROL, "no-store")], Json(state))
}

/// Query-Parameter der Geräte-Modus-Abfrage: die Geräte-ID.
#[derive(serde::Deserialize)]
struct DeviceQuery {
    device: String,
}

/// Anzeige-Zustand für ein Monitor-Gerät: löst die Feld-Zuweisung auf,
/// registriert den Poll und hängt einen offenen Fernbefehl an.
async fn monitor_device_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceQuery>,
) -> impl IntoResponse {
    let device = q.device;
    if device.is_empty() || device.len() > 64 {
        return (StatusCode::BAD_REQUEST, "Ungültige Geräte-ID").into_response();
    }
    let command = ctx.tablet.record_monitor_poll(&device);
    let assignment = ctx.monitor_assignments().get(&device).cloned();
    let mut state = match assignment {
        Some(relay_proto::MonitorTarget::Court { court_id }) => {
            let label = court_label_for(&ctx, court_id);
            let court_data = ctx.tablet.monitor_court(court_id);
            let cfg = ctx.app_config();
            let ads = monitor::list_ads(&ctx.monitor_dir);
            monitor::build_monitor_state(
                court_id,
                label,
                court_data,
                &cfg.court_monitor,
                &cfg.call_timer,
                ads,
            )
        }
        // Nicht-Court-Targets (Info, Ad): der Pi soll auf die passende
        // Anzeige-HTML umleiten. Wir liefern einen minimalen MonitorState
        // mit `redirect_to`; die monitor.html springt darauf.
        Some(ref target) if target.redirect_path().is_some() => {
            let mut s = monitor::unassigned_monitor_state(&device);
            s.unassigned = false;
            let mut path = target.redirect_path();
            // Kombi nebeneinander (Hochformat je Feld): globaler Schalter aus
            // den Court-Monitor-Einstellungen hängt `&dir=v` an die Kombi-URL.
            if matches!(target, relay_proto::MonitorTarget::CourtCombo { .. })
                && ctx.app_config().court_monitor.combo_vertical
            {
                if let Some(p) = path.as_mut() {
                    p.push_str("&dir=v");
                }
            }
            s.redirect_to = path;
            s
        }
        // Sollte unerreichbar sein (redirect_path() ist Some für alle
        // Nicht-Court-Varianten), aber strukturiert exhaustiv:
        Some(_) | None => monitor::unassigned_monitor_state(&device),
    };
    state.command = command;
    state.device_code = device_code(&device);
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

// ─────────────────────────────── Info-Monitore ────────────────────────────
//
// Read-only Hallen-Displays, kein Bezug zu einem bestimmten Feld. Werden
// per Master-Image oder URL auf einem Pi geöffnet:
//   /info/overview      → Court-Übersicht (Hallen × Felder × aktuelles Spiel)
//   /info/preparation   → Spiele in Vorbereitung (Liste, gerufene zuerst)
// Beide unterstützen URL-Parameter:
//   ?halle=<Name>       → filtert auf eine Halle
//   ?rotate=90|180|270  → Pivot-Monitor um N° drehen (CSS-Transform).

/// Liefert die HTML der Court-Übersicht. Pollt selbst `/health`.
async fn info_overview_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::OVERVIEW_HTML),
    )
}

/// Liefert die HTML des Vorbereitungs-Monitors. Pollt
/// `/info/preparation/state`.
async fn info_preparation_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::PREPARATION_HTML),
    )
}

/// Sieger-/Podium-Monitor. Pollt `/info/winners/state` für die Disziplin-Podien.
async fn info_winners_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::WINNERS_HTML),
    )
}

/// JSON-Zustand für den Sieger-Monitor: Podien aller ausgespielten Disziplinen.
async fn info_winners_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    let results = ctx.tablet.discipline_results();
    // `selected` = vom Operator gewählte Disziplin (Draw-ID). Der Monitor zeigt
    // genau diese (keine Rotation); `null` → Begrüßungsbild.
    let selected = ctx.tablet.winners_selection();
    // Turniername für die Footer-Zeile (über der Disziplin) mitliefern.
    let tournament = ctx.tablet.tournament_name();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({
            "disciplines": results,
            "selected": selected,
            "tournament": tournament,
        })),
    )
}

#[derive(serde::Deserialize)]
struct ClubLogoQuery {
    /// BTP-Vereinsname (z. B. „BC Tempelhof (Berlin)").
    name: String,
}

/// Vereinslogo für den Sieger-Monitor: matcht den Vereinsnamen gegen die
/// Badhub-Vereinsliste und liefert das Bild lokal aus (auch für LAN-TVs ohne
/// eigenes Internet). Kein Treffer / kein Logo → 404 (der Monitor blendet das
/// `<img>` dann per `onerror` weg).
async fn info_club_logo(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<ClubLogoQuery>,
) -> impl IntoResponse {
    match crate::tablet::club_logos::resolve(&ctx.config.badhub, &ctx.http, &q.name).await {
        Some((content_type, bytes)) => (
            [
                (header::CONTENT_TYPE, content_type),
                // Logos sind stabil – TVs dürfen sie lange cachen.
                (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(serde::Deserialize)]
struct FreetextQuery {
    #[serde(default)]
    hall: String,
    #[serde(default)]
    since: u64,
}

/// Freitext-Ansagen für eine Halle (`id > since`). Ein Ansage-Slave pollt das
/// vom Master, um Freitexte seiner Halle (oder „alle") anzusagen.
async fn info_announce_freetext(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<FreetextQuery>,
) -> impl IntoResponse {
    let items = ctx.tablet.freetext_since(&q.hall, q.since);
    ([(header::CACHE_CONTROL, "no-store")], Json(items))
}

/// Liefert die HTML der Werbe-Anzeige. Pollt `/info/ad/state` für die
/// Bilder-Liste; mode/file/device kommen über den Query-String.
async fn info_ad_page() -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store")], Html(assets::AD_HTML))
}

/// JSON-Zustand für die Werbe-Anzeige: aktuelle Bilder-Liste +
/// Rotations-Intervall. Liest die Bilder aus dem Court-Ad-Verzeichnis
/// (gleicher Pool wie der Court-Monitor) und nutzt
/// `MonitorConfig.ad_interval_s` als Intervall.
async fn info_ad_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    let ads = monitor::list_ads(&ctx.monitor_dir);
    let config = ctx.monitor_config();
    let payload = serde_json::json!({
        "ads": ads,
        "intervalS": config.ad_interval_s.max(1),
    });
    ([(header::CACHE_CONTROL, "no-store")], Json(payload))
}

/// Liefert die HTML der Kombi-Anzeige (mehrere Felder als Bänder). Die
/// gewünschten CourtIDs + optionale `device`/`rotate` kommen über den
/// Query-String, die Live-Daten holt die Seite über `/combo/state`.
async fn combo_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::COMBO_HTML),
    )
}

/// Query der Kombi-Anzeige: `courts=1,2,3` (kommasepariert) plus
/// optionaler `device`-Heartbeat.
#[derive(serde::Deserialize, Default)]
struct ComboQuery {
    #[serde(default)]
    courts: String,
    #[serde(default)]
    device: Option<String>,
}

/// JSON-Zustand für die Kombi-Anzeige: filtert die Felder-Übersicht auf
/// die in `?courts=` genannten CourtIDs und behält deren Reihenfolge.
/// Greift auf denselben `overview()`-Datenstand zurück wie `/health`.
async fn combo_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<ComboQuery>,
) -> impl IntoResponse {
    // Heartbeat (analog Info-Pages, v0.9.22): Poll als Lebenszeichen.
    if let Some(d) = q.device.as_deref() {
        if !d.is_empty() && d.len() <= 64 {
            let _ = ctx.tablet.record_monitor_poll(d);
        }
    }
    // Gewünschte CourtIDs in der angegebenen Reihenfolge parsen.
    // Max. 3 Felder (UI-Cap auch serverseitig spiegeln) und Duplikate
    // entfernen — sonst rendert combo.html bei einer manuell gebauten
    // URL (?courts=1,1,1,…) unleserlich viele/doppelte Bänder
    // (Code-Review v0.9.28 MEDIUM/LOW).
    let mut wanted: Vec<i64> = Vec::new();
    for id in q
        .courts
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
    {
        if !wanted.contains(&id) {
            wanted.push(id);
        }
        if wanted.len() >= 3 {
            break;
        }
    }
    let overview = ctx.tablet.overview();
    // Je gewünschter ID das passende Feld heraussuchen, Reihenfolge
    // beibehalten (nicht die overview-Reihenfolge). Unbekannte IDs
    // werden übersprungen.
    let courts: Vec<&crate::tablet::state::CourtOverview> = wanted
        .iter()
        .filter_map(|id| overview.iter().find(|c| c.court_id == *id))
        .collect();
    // serverNowMs reicht combo.html die Server-Zeit für den Pausen-Countdown
    // durch (Pi hat evtl. keine synchrone Uhr; endsAt steht in Server-Zeit).
    let payload = serde_json::json!({ "courts": courts, "serverNowMs": monitor::now_ms() });
    ([(header::CACHE_CONTROL, "no-store")], Json(payload))
}

/// JSON-Zustand für den Vorbereitungs-Monitor: alle eingeplanten,
/// ruf-baren Spiele (beide Teams stehen fest), gerufene zuerst sortiert.
/// Aufgerufene Spiele tragen `call.hall` + `call.called_at_ms` —
/// derselbe Datenstand, der auch `commands::preparation_candidates`
/// liefert, nur als reines HTTP-JSON statt Tauri-Command.
async fn info_preparation_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    let snapshot = match ctx.tablet.snapshot_clone() {
        Some(s) => s,
        None => {
            return (
                [(header::CACHE_CONTROL, "no-store")],
                Json(serde_json::json!({ "candidates": [] })),
            )
                .into_response();
        }
    };
    let calls = ctx.tablet.preparation_calls();

    let mut candidates: Vec<serde_json::Value> = snapshot
        .matches
        .iter()
        .filter(|m| {
            m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
        })
        .map(|m| {
            let call = calls.iter().find(|c| c.match_id == m.id).map(|c| {
                let hall = c
                    .location_id
                    .and_then(|lid| {
                        snapshot
                            .locations
                            .iter()
                            .find(|l| l.id == lid)
                            .map(|l| l.name.clone())
                    })
                    .unwrap_or_default();
                serde_json::json!({
                    "hall": hall,
                    "called_at_ms": c.called_at_ms,
                })
            });
            serde_json::json!({
                "match_id": m.id,
                "label": format!("{} {}", m.draw_name, m.round_name).trim().to_string(),
                "team1": m.team1.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                "team2": m.team2.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                "match_num": m.match_num,
                "planned_time": m.planned_time,
                "call": call,
            })
        })
        .collect();

    // Gerufene zuerst, dann nach BTP-Ansetzung (PlannedTime), danach nach
    // Spielnummer (ohne Zeit/Nummer hinten) – konsistent zur Auto-Feldvergabe.
    candidates.sort_by_key(|c| {
        let has_call = c.get("call").map(|v| !v.is_null()).unwrap_or(false);
        let planned = c
            .get("planned_time")
            .and_then(|v| v.as_i64())
            .unwrap_or(i64::MAX);
        let num = c
            .get("match_num")
            .and_then(|v| v.as_i64())
            .unwrap_or(i64::MAX);
        (!has_call, planned, num)
    });

    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "candidates": candidates })),
    )
        .into_response()
}

/// Liefert eine gebündelte SVG-Länderflagge (`/flags/GER.svg`).
async fn flag_route(Path(file): Path<String>) -> impl IntoResponse {
    match assets::flag_svg(&file) {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Flagge nicht gefunden").into_response(),
    }
}

/// Liefert ein hochgeladenes Werbebild aus dem `court-ads`-Verzeichnis.
async fn ad_image(
    State(ctx): State<Arc<ServerCtx>>,
    Path(file): Path<String>,
) -> impl IntoResponse {
    if !monitor::is_safe_image_name(&file) {
        return (StatusCode::NOT_FOUND, "Nicht gefunden").into_response();
    }
    match tokio::fs::read(ctx.monitor_dir.join(&file)).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, monitor::image_mime(&file)),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Werbebild nicht gefunden").into_response(),
    }
}

// ─────────────────────────────── Ergebnis → BTP ───────────────────────────

/// Nimmt das Endergebnis vom Tablet entgegen und schreibt es nach BTP.
async fn result(
    State(ctx): State<Arc<ServerCtx>>,
    Json(body): Json<ResultBody>,
) -> Json<ResultResponse> {
    Json(process_result(&ctx, &body).await)
}

/// Validiert ein Endergebnis vom Tablet und schreibt es per `SENDUPDATE`
/// nach BTP. Von beiden Modi genutzt: vom LAN-`/result`-Handler und vom
/// Cloud-Relay-Client. Die Validierung ist zugleich die Sicherheits-
/// Mitigation des Cloud-Modus (Match-ID muss zum Court-Match passen,
/// Satzstand plausibel).
pub(crate) async fn process_result(ctx: &ServerCtx, body: &ResultBody) -> ResultResponse {
    let Some(m) = ctx.tablet.match_for_court(body.court_id) else {
        return ResultResponse::err("Kein Match auf diesem Court.");
    };
    if m.id != body.match_id {
        return ResultResponse::err("Das Match auf dem Court hat inzwischen gewechselt.");
    }

    // Aufgabe und Kampflos schließen sich aus – beide gesetzt ist ein
    // Client-Fehler (das Status-Mapping unten würde sonst stillschweigend
    // walkover bevorzugen).
    if body.walkover && body.retired {
        return ResultResponse::err("Aufgabe und Kampflos zugleich – ungültig.");
    }

    let mut sets: Vec<(i64, i64)> = body.sets.iter().map(|s| (s.a, s.b)).collect();
    if sets.len() > 9 {
        return ResultResponse::err("Ungültige Satzanzahl.");
    }
    if sets
        .iter()
        .any(|&(a, b)| !(0..=99).contains(&a) || !(0..=99).contains(&b))
    {
        return ResultResponse::err("Ungültiger Satzstand.");
    }
    // Sieger + BTP-ScoreStatus (0=normal, 1=Walkover, 2=Aufgabe/Retired):
    // Bei Kampflos (walkover) und Aufgabe (retired) ist der Sieger explizit
    // angegeben; sonst wird er aus den Sätzen abgeleitet. Kampflos wird ohne
    // gespielte Sätze gewertet → die Satzliste wird verworfen.
    let (team1_won, score_status) = if body.walkover {
        sets.clear();
        match body.winner {
            Some(1) => (true, 1),
            Some(2) => (false, 1),
            _ => return ResultResponse::err("Kampflos ohne gültigen Sieger."),
        }
    } else if body.retired {
        match body.winner {
            Some(1) => (true, 2),
            Some(2) => (false, 2),
            _ => return ResultResponse::err("Aufgabe ohne gültigen Sieger."),
        }
    } else {
        if sets.is_empty() {
            return ResultResponse::err("Ungültige Satzanzahl.");
        }
        let team1_sets = sets.iter().filter(|(a, b)| a > b).count();
        let team2_sets = sets.iter().filter(|(a, b)| b > a).count();
        if team1_sets == team2_sets {
            return ResultResponse::err("Unentschiedener Satzstand – kein Sieger ermittelbar.");
        }
        (team1_sets > team2_sets, 0)
    };
    let update = proto::MatchUpdate {
        btp_match_id: m.id,
        draw_id: m.draw_id,
        planning_id: m.planning_id,
        sets,
        team1_won,
        duration_mins: 0,
        score_status,
    };

    tracing::info!(
        "Ergebnis vom Tablet: Feld {} ('{}'), Match {}, Sätze {:?} – schreibe nach BTP",
        body.court_id,
        body.court_label,
        m.id,
        update.sets
    );
    match write_result_to_btp(&ctx.config, &update).await {
        Ok(()) => {
            ctx.tablet.clear_court(body.court_id);
            tracing::info!("BTP-Schreiben OK: Match {}", m.id);
            // Feld in BTP freigeben (Court ohne MatchID + Match.CourtID=0). Sonst
            // bleibt das beendete Spiel dort „auf dem Feld" und die Spieler werden
            // nicht wieder als verfügbar (rot) angezeigt — BTP räumt beendete
            // Spiele nicht zuverlässig selbst ab. Best-effort: das Ergebnis ist
            // bereits geschrieben, ein Fehler hier darf die Wertung nicht kippen.
            if let Err(e) = write_courts_to_btp(
                &ctx.config,
                &[proto::CourtAssignment {
                    court_id: body.court_id,
                    match_id: None,
                }],
                &[proto::MatchCourt {
                    match_id: m.id,
                    draw_id: m.draw_id,
                    planning_id: m.planning_id,
                    court_id: 0,
                }],
            )
            .await
            {
                tracing::warn!(
                    "Feldfreigabe nach Ergebnis fehlgeschlagen (Match {}): {e}",
                    m.id
                );
            }
            // Nach einer Aufgabe NUR dann einen Walkover-Vorschlag für die
            // restlichen Spiele der Disziplin hinterlegen, wenn das Tablet das
            // ausdrücklich gewählt hat (echte Verletzung → `cascade_walkover`).
            // Ohne das Flag zählt nur dieses eine Spiel als Aufgabe. Bei einem
            // echten Kampflos (score_status=1) ebenfalls nicht – das ist bereits
            // die finale Wertung dieses Spiels.
            if body.retired && body.cascade_walkover {
                register_walkover_proposal(ctx, &m, team1_won);
            }
            ResultResponse::ok()
        }
        Err(e) => {
            tracing::warn!("BTP-Schreiben fehlgeschlagen (Match {}): {e}", m.id);
            ResultResponse::err(e)
        }
    }
}

/// Hinterlegt nach einer Aufgabe einen Walkover-Vorschlag für die
/// restlichen Spiele der aufgebenden Mannschaft – aber nur, wenn es in
/// derselben Disziplin überhaupt noch wertbare Spiele gibt.
fn register_walkover_proposal(ctx: &ServerCtx, m: &BtpMatch, team1_won: bool) {
    // Die aufgebende Mannschaft ist der Verlierer der Begegnung.
    let (entry_id, retired_players) = if team1_won {
        (m.entry2_id, &m.team2)
    } else {
        (m.entry1_id, &m.team1)
    };
    if entry_id == 0 {
        return; // Mannschaft nicht eindeutig auflösbar
    }
    if ctx.tablet.walkover_candidates(entry_id).is_empty() {
        return; // keine weiteren Spiele – kein Vorschlag nötig
    }
    let retired_team = retired_players
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(" / ");
    tracing::info!(
        "Aufgabe Entry {entry_id} ({retired_team}, {}) – Walkover-Vorschlag hinterlegt",
        m.draw_name
    );
    ctx.tablet
        .add_walkover_proposal(crate::tablet::state::WalkoverProposal {
            id: entry_id.to_string(),
            entry_id,
            retired_team,
            draw_name: m.draw_name.clone(),
            created_at_ms: now_ms(),
        });
}

/// Aktuelle Unix-Zeit in Millisekunden.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// LOGIN → Session-Schlüssel → `SENDUPDATE`. Schreibt ein einzelnes
/// Match-Ergebnis nach BTP – auch für kampflose Wertungen (Walkover)
/// aus der Turnierleitung wiederverwendet.
pub(crate) async fn write_result_to_btp(
    config: &AppConfig,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw = client::send_request(host, port, &proto::update_request(update, &session, pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// LOGIN → Session-Schlüssel → `SENDUPDATE` mit Courts-Block. Schreibt
/// **Feld-Zuweisungen** nach BTP (Match auf Feld setzen / Feld freigeben) –
/// nach dem Vorbild des Original-BTS. Bidirektional: das, was hier geschrieben
/// wird, liest bts-light beim nächsten Poll als OnCourt zurück.
pub(crate) async fn write_courts_to_btp(
    config: &AppConfig,
    courts: &[proto::CourtAssignment],
    match_courts: &[proto::MatchCourt],
) -> Result<(), String> {
    if courts.is_empty() && match_courts.is_empty() {
        return Ok(());
    }
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw = client::send_request(
        host,
        port,
        &proto::court_assign_request(courts, match_courts, &session, pw),
    )
    .await
    .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

// ─────────────────────────────── WebSocket ────────────────────────────────

/// Baut die Match-Kurzinfo fürs Tablet. BTP liefert das Spielsystem nicht
/// zuverlässig – Standard ist Best-of-3 bis 21 (Badminton-Normalfall).
pub(crate) fn match_brief(m: &BtpMatch, scorekeeper: Vec<String>) -> MatchBrief {
    let team = |players: &[crate::btp::model::BtpPlayer], base: i64| {
        players
            .iter()
            .enumerate()
            .map(|(i, p)| PlayerBrief {
                id: base + i as i64,
                name: p.name.clone(),
                nationality: p.nationality.clone(),
            })
            .collect()
    };
    MatchBrief {
        match_id: m.id,
        team_a: team(&m.team1, 1),
        team_b: team(&m.team2, 11),
        event_label: format!("{} {}", m.draw_name, m.round_name)
            .trim()
            .to_string(),
        best_of_sets: m.scoring.best_of,
        target_score: m.scoring.target_score,
        cap_score: m.scoring.cap_score,
        interval_at: m.scoring.interval_at,
        discipline: m.discipline.as_str().to_string(),
        match_number: m.match_num,
        scorekeeper,
    }
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, ctx))
}

/// Sendet eine `ServerMsg` über den Tablet-Socket.
async fn send_msg(socket: &mut WebSocket, msg: &ServerMsg) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = socket.send(Message::Text(Utf8Bytes::from(json))).await;
    }
}

/// Eine Tablet-Verbindung: empfängt identify/score_update/alert, pusht alle
/// 2 s das aktuell von BTP zugewiesene Match. Pro Court schiedst genau ein
/// Tablet aktiv – ein zweites Gerät kann den Court übernehmen.
async fn handle_socket(mut socket: WebSocket, ctx: Arc<ServerCtx>) {
    // Feld-Identität dieser Verbindung: die CourtID, sobald sich das Tablet
    // per `identify` gebunden hat.
    let mut court: Option<i64> = None;
    // Zuletzt ans Tablet gemeldete Match-ID. Sentinel `Some(i64::MIN)` =
    // „in dieser Verbindung noch nichts gesendet", damit der ERSTE push_match
    // immer feuert – auch ein `MatchCleared`, wenn das Feld leer ist. Sonst
    // behielte ein nach Inaktivität neu verbundenes Tablet sein altes (längst
    // entferntes) Match, weil `None == None` (kein Match) den Dedup auslöste.
    let mut last_match: Option<i64> = Some(i64::MIN);
    // Token der Court-Übernahme: `Some`, wenn dieses Tablet aktiv schiedst.
    let mut my_token: Option<u64> = None;
    let mut superseded = false;
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Zeitpunkt der letzten empfangenen Nachricht (jede Art, inkl. App-Ping
    // und Protokoll-Pong). Bricht der Router weg, liefert der Browser oft
    // KEIN Close – die TCP-Verbindung bleibt serverseitig minutenlang als
    // „offen" hängen und hält das Feld belegt, sodass das zurückkehrende
    // Tablet beim Reconnect „belegt" zu hören bekommt. Erkennt der Server
    // nach STALE_AFTER kein Lebenszeichen mehr, schließt er die Verbindung
    // selbst → das Feld wird frei und kann sofort neu belegt werden.
    let mut last_seen = std::time::Instant::now();
    // 10 s, BEWUSST KÜRZER als der Tablet-Watchdog (15 s): Bricht der Router
    // weg, gibt der Server das Feld schon nach 10 s frei – also bevor das
    // Tablet (frühestens nach 15 s Stille) sich neu meldet. So ist das Feld
    // beim Reconnect bereits frei, das „Feld belegt"-Overlay erscheint gar
    // nicht erst und das Tablet belegt direkt selbst neu. Auf einer gesunden
    // Verbindung trifft der Browser den Protokoll-Ping alle ~2 s mit Pong →
    // last_seen bleibt frisch, 10 s lösen also keinen Fehlschluss aus. Ein
    // seltener Fehlschluss unter Last wäre harmlos: das Tablet verbindet sich
    // sofort neu (Stand ist persistiert und wird re-gepusht).
    const STALE_AFTER: Duration = Duration::from_secs(10);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                last_seen = std::time::Instant::now();
                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<TabletMsg>(text.as_str()) {
                            Ok(TabletMsg::Identify { court_id, .. }) => {
                                court = Some(court_id);
                                last_match = None;
                                if ctx.tablet.court_occupied(court_id) {
                                    tracing::info!("Feld {court_id} belegt – Tablet wartet auf Übernahme");
                                    send_msg(&mut socket, &ServerMsg::CourtOccupied).await;
                                } else {
                                    my_token = Some(ctx.tablet.claim_court(court_id));
                                    ctx.tablet.attach_tablet(court_id);
                                    tracing::info!("Tablet verbunden für Feld {court_id}");
                                    // Gespeicherten Spielstand auch beim normalen
                                    // Verbinden wiederherstellen (nicht nur bei
                                    // Übernahme): so startet ein neu verbundenes
                                    // ODER Ersatz-Tablet nach einem Crash nicht bei
                                    // 0:0. Das Tablet behält den Stand nur, wenn die
                                    // matchId zum gleich gepushten Match passt
                                    // (tablet.html), sonst überschreibt push_match.
                                    if let Some(state) = ctx.tablet.court_state(court_id) {
                                        send_msg(&mut socket, &ServerMsg::StateRestore { state })
                                            .await;
                                    }
                                    push_match(court_id, &ctx, &mut socket, &mut last_match).await;
                                }
                            }
                            Ok(TabletMsg::TakeOver) => {
                                if let (Some(c), None, false) = (court, my_token, superseded) {
                                    my_token = Some(ctx.tablet.claim_court(c));
                                    ctx.tablet.attach_tablet(c);
                                    last_match = None;
                                    tracing::info!("Tablet übernimmt Feld {c}");
                                    if let Some(state) = ctx.tablet.court_state(c) {
                                        send_msg(&mut socket, &ServerMsg::StateRestore { state }).await;
                                    }
                                    push_match(c, &ctx, &mut socket, &mut last_match).await;
                                }
                            }
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history }) => {
                                if let (Some(c), Some(_)) = (court, my_token) {
                                    handle_score(c, score_a, score_b, &sets_history, &ctx).await;
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let Some(c) = court {
                                    ctx.tablet.record_battery(c, percent, charging);
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), Some(_)) = (court, my_token) {
                                    ctx.tablet.record_alert(c, injury, official);
                                }
                            }
                            Ok(TabletMsg::StateSync { state }) => {
                                if let (Some(c), Some(_)) = (court, my_token) {
                                    ctx.tablet.set_court_state(c, state);
                                }
                            }
                            Ok(TabletMsg::Ping) => {
                                // Lebenszeichen → sofort Pong zurück, damit das
                                // Tablet eine tote Verbindung erkennen kann.
                                send_msg(&mut socket, &ServerMsg::Pong).await;
                            }
                            Err(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                // Tote Verbindung (Router weg, kein Close vom Browser) erkennen
                // und schließen, damit das Feld nicht dauerhaft belegt bleibt.
                if last_seen.elapsed() > STALE_AFTER {
                    tracing::info!(
                        "Tablet-Verbindung still seit >{}s – schließe (Feld {court:?})",
                        STALE_AFTER.as_secs()
                    );
                    break;
                }
                // Protokoll-Ping: hält die Leitung wach und lässt auch ältere
                // Tablets (ohne App-Ping) durch ihr Pong als „lebend" gelten;
                // schlägt das Senden fehl, ist die Verbindung tot → Schluss.
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
                if let (Some(c), Some(token)) = (court, my_token) {
                    if ctx.tablet.is_court_active(c, token) {
                        push_match(c, &ctx, &mut socket, &mut last_match).await;
                    } else {
                        my_token = None;
                        superseded = true;
                        tracing::info!("Tablet für Feld {c} wurde abgelöst");
                        send_msg(&mut socket, &ServerMsg::SessionSuperseded).await;
                    }
                }
            }
        }
    }

    // Aufräumen: nur das noch aktive Tablet gibt das Feld frei.
    if let (Some(c), Some(token)) = (court, my_token) {
        if ctx.tablet.is_court_active(c, token) {
            ctx.tablet.detach_tablet(c);
            ctx.tablet.release_court(c, token);
            tracing::info!("Tablet getrennt für Feld {c}");
        }
    }
}

/// Sendet `match_assigned`/`match_cleared`, sobald sich das Match des
/// Felds (per CourtID) gegenüber dem zuletzt gemeldeten Stand geändert hat.
async fn push_match(
    court_id: i64,
    ctx: &ServerCtx,
    socket: &mut WebSocket,
    last: &mut Option<i64>,
) {
    let current = ctx.tablet.match_for_court(court_id);
    let current_id = current.as_ref().map(|m| m.id);
    if current_id == *last {
        return;
    }
    *last = current_id;
    let msg = match &current {
        Some(m) => {
            tracing::info!("Feld {court_id}: Match {} ans Tablet zugewiesen", m.id);
            ServerMsg::MatchAssigned {
                match_brief: match_brief(m, ctx.tablet.scorekeeper(court_id)),
            }
        }
        None => {
            tracing::info!("Feld {court_id}: Match-Zuweisung aufgehoben");
            ServerMsg::MatchCleared
        }
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(Utf8Bytes::from(json))).await;
    }
}

/// Entscheiden die bereits abgeschlossenen Sätze das Match schon? (Ein Team hat
/// die Mehrheit der Best-of-N-Sätze gewonnen.) Damit unterscheiden wir einen
/// 0:0-„Geistersatz" NACH Spielende von einem echten neuen Satz zwischen zwei
/// Sätzen – ohne dafür ein separates `finished`-Signal zu brauchen (das im
/// Cloud-Pfad nicht vorliegt). Funktioniert in LAN- und Cloud-Modus identisch.
fn match_decided(best_of: i64, completed: &[(i64, i64)]) -> bool {
    let need = best_of / 2 + 1;
    let (mut a, mut b) = (0, 0);
    for &(sa, sb) in completed {
        if sa > sb {
            a += 1;
        } else if sb > sa {
            b += 1;
        }
    }
    a >= need || b >= need
}

/// Verarbeitet einen Live-Punktestand vom Tablet: merken + an den
/// Liveticker pushen. Von LAN-Server und Cloud-Relay-Client genutzt.
pub(crate) async fn handle_score(
    court_id: i64,
    score_a: i64,
    score_b: i64,
    history: &[SetAb],
    ctx: &ServerCtx,
) {
    let Some(m) = ctx.tablet.match_for_court(court_id) else {
        return;
    };
    if history.len() > 9 {
        return; // unplausibel viele Sätze – Nachricht verwerfen
    }
    // Vollständige Satzliste: abgeschlossene Sätze + laufender Satz.
    // Den laufenden 0:0-Satz NUR dann weglassen, wenn er ein „Geistersatz"
    // NACH Spielende ist – d. h. die abgeschlossenen Sätze entscheiden das
    // Match bereits (das Tablet setzt currentSet beim Match-Ende auf 0:0).
    // ZWISCHEN den Sätzen (Match noch offen) MUSS der 0:0-Satz erhalten
    // bleiben, sonst klebt der Court-Monitor nach der Satzpause am alten
    // Satzstand, bis der erste Punkt fällt. Erster Satz (history leer): bleibt.
    let mut sets: Vec<(i64, i64)> = history.iter().map(|s| (s.a, s.b)).collect();
    let ghost_after_finish =
        score_a == 0 && score_b == 0 && match_decided(m.scoring.best_of, &sets);
    if !ghost_after_finish {
        sets.push((score_a, score_b));
    }
    ctx.tablet.record_score(court_id, m.id, sets.clone());

    let mut live = m;
    live.sets = sets;
    let update = Update::Single(build_tupdate(&live, ctx.next_rid()));
    if let Err(e) = push::push_update(
        &ctx.http,
        &ctx.config.badhub.url,
        &ctx.config.badhub.password,
        &update,
    )
    .await
    {
        tracing::warn!("Live-Score-Push fehlgeschlagen: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpPlayer, BtpSnapshot, Discipline, MatchResult, ScoringFormat};
    use crate::btp::wire;
    use crate::btp::xml::{self, Node, Value};
    use crate::config::BtpConfig;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // ───────────────────────── Test-Helfer (BTP-Ergebnis-Pfad) ──────────────

    /// Antwort-Frame im BTP-Format: Action{ID=REPLY, Result=1, [extra]}.
    fn mock_reply(extra: Vec<Node>) -> Vec<u8> {
        let mut c = vec![Node::string("ID", "REPLY"), Node::integer("Result", 1)];
        c.extend(extra);
        wire::encode_message(&xml::encode(&[Node::group("Action", c)]))
    }

    /// Mock-BTP: LOGIN → Session, SENDUPDATE → aufzeichnen + bestätigen.
    /// Liefert Port und den Aufzeichnungs-Puffer der SENDUPDATE-Requests.
    async fn spawn_mock_btp() -> (u16, Arc<Mutex<Vec<Vec<Node>>>>) {
        let recorded: Arc<Mutex<Vec<Vec<Node>>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut header = [0u8; 4];
                if sock.read_exact(&mut header).await.is_err() {
                    continue;
                }
                let len = i32::from_be_bytes(header) as usize;
                let mut payload = vec![0u8; len];
                if sock.read_exact(&mut payload).await.is_err() {
                    continue;
                }
                let mut full = header.to_vec();
                full.extend_from_slice(&payload);
                let nodes = proto::decode_response(&full).unwrap();
                let action = xml::find(&nodes, "Action").unwrap();
                let id = xml::find(action.children(), "ID")
                    .and_then(Node::value)
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if id == "LOGIN" {
                    sock.write_all(&mock_reply(vec![Node::string("Unicode", "SESSION")]))
                        .await
                        .unwrap();
                } else {
                    rec.lock().unwrap().push(nodes.clone());
                    sock.write_all(&mock_reply(vec![])).await.unwrap();
                }
            }
        });
        (port, recorded)
    }

    fn player(n: &str) -> BtpPlayer {
        BtpPlayer {
            name: n.to_string(),
            first: String::new(),
            last: n.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        }
    }

    /// Match id=42 auf Court 101 (OnCourt), zwei Einzel-Spieler.
    fn match_on_court() -> BtpMatch {
        BtpMatch {
            id: 42,
            draw_id: 7,
            planning_id: 1001,
            draw_name: "HE".into(),
            discipline: Discipline::MensSingles,
            round_name: "G1".into(),
            match_num: Some(1),
            planned_time: None,
            team1: vec![player("A")],
            team2: vec![player("B")],
            entry1_id: 0,
            entry2_id: 0,
            court: Some("1".into()),
            court_id: Some(101),
            sets: vec![],
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::OnCourt,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            scoring: ScoringFormat::default(),
        }
    }

    /// ServerCtx mit Match 42 auf Court 101; BTP zeigt auf 127.0.0.1:`port`.
    /// Für Ablehnungs-Tests genügt ein toter Port (es kommt nie zum Schreiben).
    fn make_ctx(port: u16) -> ServerCtx {
        let tablet = Arc::new(TabletState::default());
        tablet.set_snapshot(BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches: vec![match_on_court()],
            courts: vec!["1".into()],
            locations: vec![],
            court_infos: vec![],
        });
        let config = AppConfig {
            btp: BtpConfig {
                host: "127.0.0.1".into(),
                port,
                password: None,
            },
            ..Default::default()
        };
        let tmp = std::env::temp_dir();
        ServerCtx::new(
            tablet,
            config,
            reqwest::Client::new(),
            tmp.clone(),
            tmp.join("bts_test_config.json"),
            tmp.join("bts_test_assign.json"),
            tmp,
        )
    }

    /// Standard-Ergebnis-Body (Match 42 / Court 101) mit gegebenen Sätzen.
    fn body_with(sets: &[(i64, i64)]) -> ResultBody {
        ResultBody {
            match_id: 42,
            court_id: 101,
            court_label: "1".into(),
            sets: sets.iter().map(|&(a, b)| SetAb { a, b }).collect(),
            retired: false,
            walkover: false,
            winner: None,
            cascade_walkover: false,
        }
    }

    /// Kinder des Match-Knotens aus einem aufgezeichneten SENDUPDATE-Request.
    fn match_fields(req: &[Node]) -> Vec<Node> {
        let upd = xml::find(req, "Update").unwrap();
        let tour = xml::find(upd.children(), "Tournament").unwrap();
        let matches = xml::find(tour.children(), "Matches").unwrap();
        xml::find(matches.children(), "Match")
            .unwrap()
            .children()
            .to_vec()
    }

    fn int(children: &[Node], id: &str) -> Option<i64> {
        xml::find(children, id)
            .and_then(Node::value)
            .and_then(Value::as_int)
    }

    // Die Logik hinter dem 0:0-Geistersatz-Fix: Zwischen den Sätzen ist das
    // Match NICHT entschieden → der laufende 0:0-Satz bleibt erhalten (Monitor
    // zeigt sofort 0:0). Erst wenn die Mehrheit der Sätze gewonnen ist, gilt
    // ein 0:0 als Geistersatz nach Spielende und wird weggelassen.
    #[test]
    fn match_decided_best_of_3() {
        assert!(!match_decided(3, &[])); // erster Satz – offen
        assert!(!match_decided(3, &[(21, 7)])); // 1:0 – Satzpause, neuer Satz
        assert!(!match_decided(3, &[(21, 7), (15, 21)])); // 1:1 – Entscheidungssatz
        assert!(match_decided(3, &[(21, 7), (21, 15)])); // 2:0 – entschieden
        assert!(match_decided(3, &[(21, 7), (15, 21), (21, 18)])); // 2:1 – entschieden
    }

    #[test]
    fn match_decided_best_of_1_and_5() {
        assert!(!match_decided(1, &[])); // einziger Satz läuft
        assert!(match_decided(1, &[(21, 15)])); // 1:0 in Bo1 → entschieden
        assert!(!match_decided(5, &[(21, 1), (21, 2)])); // 2:0 in Bo5 – noch offen
        assert!(match_decided(5, &[(21, 1), (21, 2), (21, 3)])); // 3:0 – entschieden
    }

    /// Regression (v0.9.113): Nach einem Ergebnis muss `process_result` das Feld
    /// in BTP freigeben (Court ohne MatchID + Match.CourtID=0), sonst bleiben die
    /// Spieler in BTP „auf dem Feld" (nicht wieder rot/verfügbar). GENAU ZWEI
    /// SENDUPDATEs: das Ergebnis UND die Feldfreigabe.
    #[tokio::test]
    async fn process_result_frees_court_in_btp() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);

        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(
            resp.ok,
            "Ergebnis sollte erfolgreich sein: {:?}",
            resp.error
        );

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs.len(), 2, "Ergebnis + Feldfreigabe = 2 SENDUPDATE");

        // Zweiter SENDUPDATE = Feldfreigabe: Court 101 OHNE MatchID, Match.CourtID=0.
        let upd = xml::find(&reqs[1], "Update").expect("Update");
        let tour = xml::find(upd.children(), "Tournament").expect("Tournament");
        let courts = xml::find(tour.children(), "Courts").expect("Courts-Block (Feldfreigabe)");
        let court = xml::find(courts.children(), "Court").expect("Court");
        assert_eq!(int(court.children(), "ID"), Some(101));
        assert!(
            xml::find(court.children(), "MatchID").is_none(),
            "frei = Court ohne MatchID"
        );
        let matches = xml::find(tour.children(), "Matches").expect("Matches");
        let mnode = xml::find(matches.children(), "Match").expect("Match");
        assert_eq!(int(mnode.children(), "CourtID"), Some(0));
    }

    /// Sieger wird aus den Sätzen abgeleitet (Team 2 gewinnt 0:2) und als
    /// `Winner=2`, `ScoreStatus=0`, mit beiden Sätzen nach BTP geschrieben.
    #[tokio::test]
    async fn result_winner_derived_from_sets() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);

        let resp = process_result(&ctx, &body_with(&[(10, 21), (15, 21)])).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(2), "Team 2 gewinnt");
        assert_eq!(int(&m, "ScoreStatus"), Some(0), "regulär ausgespielt");
        let sets = xml::find(&m, "Sets").expect("Sets");
        assert_eq!(sets.children().len(), 2, "beide Sätze übertragen");
    }

    /// Aufgabe (Retired): Sieger explizit, `ScoreStatus=2`.
    #[tokio::test]
    async fn result_retired_sets_score_status_2() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let mut body = body_with(&[(21, 10), (5, 11)]);
        body.retired = true;
        body.winner = Some(1);

        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(1));
        assert_eq!(int(&m, "ScoreStatus"), Some(2), "Aufgabe");
    }

    /// Kampflos (Walkover): `ScoreStatus=1`, Satzliste wird verworfen.
    #[tokio::test]
    async fn result_walkover_clears_sets() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let mut body = body_with(&[(21, 10), (21, 15)]); // Sätze werden ignoriert
        body.walkover = true;
        body.winner = Some(2);

        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(2));
        assert_eq!(int(&m, "ScoreStatus"), Some(1), "Kampflos");
        let sets = xml::find(&m, "Sets").expect("Sets");
        assert!(sets.children().is_empty(), "Kampflos verwirft Sätze");
    }

    // ── Ablehnungen: ungültige Ergebnisse werden NICHT nach BTP geschrieben ──
    // (process_result bricht vor jedem Netzwerkzugriff ab; toter Port genügt.)

    async fn rejected(body: ResultBody) -> super::ResultResponse {
        let ctx = make_ctx(1); // Port 1 wird nie kontaktiert
        process_result(&ctx, &body).await
    }

    #[tokio::test]
    async fn rejects_empty_sets_without_walkover_or_retired() {
        assert!(!rejected(body_with(&[])).await.ok);
    }

    #[tokio::test]
    async fn rejects_drawn_sets() {
        // 1:1 → kein Sieger ableitbar.
        assert!(!rejected(body_with(&[(21, 10), (10, 21)])).await.ok);
    }

    #[tokio::test]
    async fn rejects_too_many_sets() {
        let many: Vec<(i64, i64)> = (0..10).map(|_| (21, 0)).collect();
        assert!(!rejected(body_with(&many)).await.ok);
    }

    #[tokio::test]
    async fn rejects_invalid_set_score() {
        assert!(!rejected(body_with(&[(100, 0)])).await.ok);
    }

    #[tokio::test]
    async fn rejects_walkover_without_winner() {
        let mut b = body_with(&[]);
        b.walkover = true; // winner bleibt None
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_retired_without_winner() {
        let mut b = body_with(&[(21, 10)]);
        b.retired = true; // winner bleibt None
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_walkover_and_retired_together() {
        let mut b = body_with(&[]);
        b.walkover = true;
        b.retired = true;
        b.winner = Some(1);
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_when_court_match_changed() {
        let mut b = body_with(&[(21, 10), (21, 12)]);
        b.match_id = 999; // anderes Match als auf dem Court (42)
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_when_no_match_on_court() {
        let mut b = body_with(&[(21, 10), (21, 12)]);
        b.court_id = 999; // kein Match auf diesem Feld
        assert!(!rejected(b).await.ok);
    }
}
