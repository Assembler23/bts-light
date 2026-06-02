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
    device_code, html_escape, MatchBrief, PlayerBrief, ResultBody, ResultResponse, ServerMsg,
    SetAb, TabletMsg,
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
        .route("/", get(index))
        .route("/court/{id}", get(court_page))
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
        .route("/info/ad", get(info_ad_page))
        .route("/info/ad/state", get(info_ad_state))
        .route("/combo", get(combo_page))
        .route("/combo/state", get(combo_state))
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

/// Landing-Page: zeigt die Tablet-Adressen je Court. Die URL trägt die
/// stabile CourtID, der angezeigte Text den Feldnamen.
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
    tracing::info!("Tablet-Seite ausgeliefert für Feld {court_id} ('{label}')");
    let body = TABLET_HTML
        .replace("__COURT_ID__", &court_id.to_string())
        .replace("__COURT_LABEL__", &html_escape(&label));
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
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
    let config = ctx.monitor_config();
    let ads = monitor::list_ads(&ctx.monitor_dir);
    let state = monitor::build_monitor_state(court_id, label, court, &config, ads);
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
            let config = ctx.monitor_config();
            let ads = monitor::list_ads(&ctx.monitor_dir);
            monitor::build_monitor_state(court_id, label, court_data, &config, ads)
        }
        // Nicht-Court-Targets (Info, Ad): der Pi soll auf die passende
        // Anzeige-HTML umleiten. Wir liefern einen minimalen MonitorState
        // mit `redirect_to`; die monitor.html springt darauf.
        Some(ref target) if target.redirect_path().is_some() => {
            let mut s = monitor::unassigned_monitor_state(&device);
            s.unassigned = false;
            s.redirect_to = target.redirect_path();
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
    let payload = serde_json::json!({ "courts": courts });
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
                "call": call,
            })
        })
        .collect();

    // Gerufene Spiele zuerst, dann nach Spielnummer (ohne Nummer hinten).
    candidates.sort_by_key(|c| {
        let has_call = c.get("call").map(|v| !v.is_null()).unwrap_or(false);
        let num = c
            .get("match_num")
            .and_then(|v| v.as_i64())
            .unwrap_or(i64::MAX);
        (!has_call, num)
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
        best_of_sets: 3,
        target_score: 21,
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
    let mut last_match: Option<i64> = None;
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
    // Einen laufenden Satz von 0:0 NICHT anhängen, wenn schon Sätze
    // gespielt sind — sonst erscheint nach Spielende (currentSet wurde
    // auf 0:0 zurückgesetzt) ein leerer „Geistersatz" in der Anzeige.
    // Beim allerersten Satz (history leer) bleibt 0:0 erhalten.
    let mut sets: Vec<(i64, i64)> = history.iter().map(|s| (s.a, s.b)).collect();
    if !(score_a == 0 && score_b == 0 && !sets.is_empty()) {
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
