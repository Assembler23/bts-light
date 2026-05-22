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
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};

use relay_proto::{
    device_code, html_escape, path_encode, MatchBrief, PlayerBrief, ResultBody, ResultResponse,
    ServerMsg, SetAb, TabletMsg,
};

use crate::badhub::diff::Update;
use crate::badhub::payload::build_tupdate;
use crate::badhub::push;
use crate::btp::model::BtpMatch;
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
    /// Pfad zur `monitor-assignments.json` (Gerät → Feld). Wird frisch
    /// gelesen, damit Zuweisungen aus dem Tool sofort greifen.
    pub assignments_path: PathBuf,
}

impl ServerCtx {
    pub fn new(
        tablet: Arc<TabletState>,
        config: AppConfig,
        http: reqwest::Client,
        monitor_dir: PathBuf,
        config_path: PathBuf,
        assignments_path: PathBuf,
    ) -> Self {
        Self {
            tablet,
            config,
            http,
            rid: AtomicU64::new(1),
            monitor_dir,
            config_path,
            assignments_path,
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

    /// Lädt die Geräte→Feld-Zuweisungen frisch von der Platte.
    pub fn monitor_assignments(&self) -> HashMap<String, String> {
        monitor::read_assignments(&self.assignments_path)
    }
}

/// Startet den Server auf `0.0.0.0:8088` und bedient ihn, bis der Task
/// abgebrochen wird.
pub async fn run(ctx: Arc<ServerCtx>) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/court/{label}", get(court_page))
        .route("/court/{label}/display", get(monitor_page))
        .route("/court/{label}/state", get(monitor_state))
        .route("/monitor", get(monitor_device_page))
        .route("/monitor/state", get(monitor_device_state))
        .route("/qr/{label}", get(qr_svg))
        .route("/flags/{file}", get(flag_route))
        .route("/ads/{file}", get(ad_image))
        .route("/health", get(health))
        .route("/result", post(result))
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

/// Landing-Page: zeigt die Tablet-Adressen je Court.
async fn index(State(ctx): State<Arc<ServerCtx>>) -> Html<String> {
    let host = lan_host();
    let courts = ctx.tablet.court_names();
    let mut rows = String::new();
    for c in &courts {
        rows.push_str(&format!(
            "<li><b>{}</b> &mdash; <a href=\"/court/{enc}\">/court/{}</a> \
             &middot; <a href=\"/qr/{enc}\">QR</a></li>",
            html_escape(c),
            html_escape(c),
            enc = path_encode(c),
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

/// Liefert die Tablet-UI für einen Court (kein Caching – immer frisch).
async fn court_page(Path(label): Path<String>) -> impl IntoResponse {
    tracing::info!("Tablet-Seite ausgeliefert für Court '{label}'");
    let body = TABLET_HTML.replace("__COURT_LABEL__", &html_escape(&label));
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// QR-Code (SVG), der auf die Tablet-URL des Courts zeigt.
async fn qr_svg(Path(label): Path<String>) -> impl IntoResponse {
    let url = format!("http://{}/court/{}", lan_host(), path_encode(&label));
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

/// Status-Schnappschuss für die bts-light-Oberfläche.
async fn health(State(ctx): State<Arc<ServerCtx>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "courts": ctx.tablet.overview(),
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

/// Liefert die Court-Monitor-Anzeige fest für ein Feld (`/court/X/display`).
async fn monitor_page(Path(label): Path<String>) -> impl IntoResponse {
    tracing::info!("Court-Monitor-Seite (fest) ausgeliefert für Court '{label}'");
    let body = render_monitor_html("fixed", "/", &label);
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Liefert die Court-Monitor-Anzeige im Geräte-Modus (`/monitor`) – das
/// Gerät bekommt sein Feld erst über die Zuweisung im Tool.
async fn monitor_device_page() -> impl IntoResponse {
    let body = render_monitor_html("device", "/", "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Anzeige-Zustand eines fest verdrahteten Feldes, im Sekundentakt gepollt.
async fn monitor_state(
    State(ctx): State<Arc<ServerCtx>>,
    Path(label): Path<String>,
) -> impl IntoResponse {
    let court = ctx.tablet.monitor_court(&label);
    let config = ctx.monitor_config();
    let ads = monitor::list_ads(&ctx.monitor_dir);
    let state = monitor::build_monitor_state(label, court, &config, ads);
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
    let mut state = match ctx.monitor_assignments().get(&device) {
        Some(court) => {
            let court_data = ctx.tablet.monitor_court(court);
            let config = ctx.monitor_config();
            let ads = monitor::list_ads(&ctx.monitor_dir);
            monitor::build_monitor_state(court.clone(), court_data, &config, ads)
        }
        None => monitor::unassigned_monitor_state(&device),
    };
    state.command = command;
    state.device_code = device_code(&device);
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
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
    let Some(m) = ctx.tablet.match_for_court(&body.court_label) else {
        return ResultResponse::err("Kein Match auf diesem Court.");
    };
    if m.id != body.match_id {
        return ResultResponse::err("Das Match auf dem Court hat inzwischen gewechselt.");
    }

    let sets: Vec<(i64, i64)> = body.sets.iter().map(|s| (s.a, s.b)).collect();
    if sets.len() > 9 {
        return ResultResponse::err("Ungültige Satzanzahl.");
    }
    if sets
        .iter()
        .any(|&(a, b)| !(0..=99).contains(&a) || !(0..=99).contains(&b))
    {
        return ResultResponse::err("Ungültiger Satzstand.");
    }
    // Sieger + ScoreStatus: bei Aufgabe (retired) ist der Sieger explizit
    // angegeben, sonst wird er aus den Sätzen abgeleitet.
    let (team1_won, score_status) = if body.retired {
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
        "Ergebnis vom Tablet: Court '{}', Match {}, Sätze {:?} – schreibe nach BTP",
        body.court_label,
        m.id,
        update.sets
    );
    match write_result_to_btp(&ctx.config, &update).await {
        Ok(()) => {
            ctx.tablet.clear_court(&body.court_label);
            tracing::info!("BTP-Schreiben OK: Match {}", m.id);
            // Nach einer Aufgabe: prüfen, ob die aufgebende Mannschaft in
            // derselben Disziplin noch Spiele hat, und der Turnierleitung
            // einen Walkover-Vorschlag hinterlegen.
            if body.retired {
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

// ─────────────────────────────── WebSocket ────────────────────────────────

/// Baut die Match-Kurzinfo fürs Tablet. BTP liefert das Spielsystem nicht
/// zuverlässig – Standard ist Best-of-3 bis 21 (Badminton-Normalfall).
pub(crate) fn match_brief(m: &BtpMatch) -> MatchBrief {
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
        best_of_sets: 3,
        target_score: 21,
        discipline: m.discipline.as_str().to_string(),
        match_number: m.match_num,
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
    let mut court: Option<String> = None;
    let mut last_match: Option<i64> = None;
    // Token der Court-Übernahme: `Some`, wenn dieses Tablet aktiv schiedst.
    let mut my_token: Option<u64> = None;
    let mut superseded = false;
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<TabletMsg>(text.as_str()) {
                            Ok(TabletMsg::Identify { court_label }) => {
                                court = Some(court_label.clone());
                                last_match = None;
                                if ctx.tablet.court_occupied(&court_label) {
                                    tracing::info!("Court '{court_label}' belegt – Tablet wartet auf Übernahme");
                                    send_msg(&mut socket, &ServerMsg::CourtOccupied).await;
                                } else {
                                    my_token = Some(ctx.tablet.claim_court(&court_label));
                                    ctx.tablet.attach_tablet(&court_label);
                                    tracing::info!("Tablet verbunden für Court '{court_label}'");
                                    push_match(&court_label, &ctx, &mut socket, &mut last_match).await;
                                }
                            }
                            Ok(TabletMsg::TakeOver) => {
                                if let (Some(c), None, false) = (court.clone(), my_token, superseded) {
                                    my_token = Some(ctx.tablet.claim_court(&c));
                                    ctx.tablet.attach_tablet(&c);
                                    last_match = None;
                                    tracing::info!("Tablet übernimmt Court '{c}'");
                                    if let Some(state) = ctx.tablet.court_state(&c) {
                                        send_msg(&mut socket, &ServerMsg::StateRestore { state }).await;
                                    }
                                    push_match(&c, &ctx, &mut socket, &mut last_match).await;
                                }
                            }
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history }) => {
                                if let (Some(c), Some(_)) = (&court, my_token) {
                                    handle_score(c, score_a, score_b, &sets_history, &ctx).await;
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let Some(c) = &court {
                                    ctx.tablet.record_battery(c, percent, charging);
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), Some(_)) = (&court, my_token) {
                                    ctx.tablet.record_alert(c, injury, official);
                                }
                            }
                            Ok(TabletMsg::StateSync { state }) => {
                                if let (Some(c), Some(_)) = (&court, my_token) {
                                    ctx.tablet.set_court_state(c, state);
                                }
                            }
                            Err(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                if let (Some(c), Some(token)) = (court.clone(), my_token) {
                    if ctx.tablet.is_court_active(&c, token) {
                        push_match(&c, &ctx, &mut socket, &mut last_match).await;
                    } else {
                        my_token = None;
                        superseded = true;
                        tracing::info!("Tablet für Court '{c}' wurde abgelöst");
                        send_msg(&mut socket, &ServerMsg::SessionSuperseded).await;
                    }
                }
            }
        }
    }

    // Aufräumen: nur das noch aktive Tablet gibt den Court frei.
    if let (Some(c), Some(token)) = (&court, my_token) {
        if ctx.tablet.is_court_active(c, token) {
            ctx.tablet.detach_tablet(c);
            ctx.tablet.release_court(c, token);
            tracing::info!("Tablet getrennt für Court '{c}'");
        }
    }
}

/// Sendet `match_assigned`/`match_cleared`, sobald sich das Match des
/// Courts gegenüber dem zuletzt gemeldeten Stand geändert hat.
async fn push_match(court: &str, ctx: &ServerCtx, socket: &mut WebSocket, last: &mut Option<i64>) {
    let current = ctx.tablet.match_for_court(court);
    let current_id = current.as_ref().map(|m| m.id);
    if current_id == *last {
        return;
    }
    *last = current_id;
    let msg = match &current {
        Some(m) => {
            tracing::info!("Court '{court}': Match {} ans Tablet zugewiesen", m.id);
            ServerMsg::MatchAssigned {
                match_brief: match_brief(m),
            }
        }
        None => {
            tracing::info!("Court '{court}': Match-Zuweisung aufgehoben");
            ServerMsg::MatchCleared
        }
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(Utf8Bytes::from(json))).await;
    }
}

/// Verarbeitet einen Live-Punktestand vom Tablet: merken + an den
/// Liveticker pushen. Von LAN-Server und Cloud-Relay-Client genutzt.
pub(crate) async fn handle_score(
    court: &str,
    score_a: i64,
    score_b: i64,
    history: &[SetAb],
    ctx: &ServerCtx,
) {
    let Some(m) = ctx.tablet.match_for_court(court) else {
        return;
    };
    if history.len() > 9 {
        return; // unplausibel viele Sätze – Nachricht verwerfen
    }
    // Vollständige Satzliste: abgeschlossene Sätze + laufender Satz.
    let mut sets: Vec<(i64, i64)> = history.iter().map(|s| (s.a, s.b)).collect();
    sets.push((score_a, score_b));
    ctx.tablet.record_score(court, m.id, sets.clone());

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
