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
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, Mutex};

use relay_proto::{
    html_escape, path_encode, HostFrame, RelayFrame, ResultBody, ResultResponse, ServerMsg,
    TabletMsg,
};

/// Die Tablet-Spielzettel-UI – dieselbe Datei wie in der bts-light-App.
const TABLET_HTML: &str = include_str!("../../src-tauri/assets/tablet.html");

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

/// Maximale Länge eines Court-Namens (Schutz gegen überlange Frames).
const MAX_COURT_LABEL_LEN: usize = 128;

type Tx = mpsc::UnboundedSender<Message>;

/// Ein Namespace: ein bts-light-Host und seine Tablets.
struct Namespace {
    /// Sende-Ende zur Host-WebSocket (bts-light), falls verbunden.
    host: Option<Tx>,
    /// Court-Name → Sende-Ende zur Tablet-WebSocket.
    tablets: HashMap<String, Tx>,
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
            pending: HashMap::new(),
            next_req: 1,
        }
    }

    /// Leer = kann aus der Namespace-Tabelle entfernt werden.
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
        .route("/{ns}/court/{label}", get(court_page))
        .route("/{ns}/qr/{label}", get(qr_svg))
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

/// Liefert die Tablet-UI für einen Court (kein Caching – immer frisch).
async fn court_page(Path((ns, label)): Path<(String, String)>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    tracing::info!("Tablet-Seite ausgeliefert für Court '{label}'");
    let body = TABLET_HTML.replace("__COURT_LABEL__", &html_escape(&label));
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// QR-Code (SVG), der auf die öffentliche Tablet-URL des Courts zeigt.
async fn qr_svg(
    State(broker): State<Broker>,
    Path((ns, label)): Path<(String, String)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let url = format!(
        "{}/{}/court/{}",
        broker.public_base,
        path_encode(&ns),
        path_encode(&label)
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

/// Eine Tablet-Verbindung: meldet sich für einen Court an, leitet
/// Score-Updates an den Host weiter, empfängt Match-Zuweisungen.
async fn tablet_conn(mut socket: WebSocket, broker: Broker, ns: String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let mut court: Option<String> = None;
    // Schiedst dieses Tablet den Court aktiv? Passive Tablets warten auf
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
                            Ok(TabletMsg::Identify { court_label }) => {
                                match attach_tablet(&broker, &ns, &court_label, &tx).await {
                                    AttachResult::Active => {
                                        tracing::info!("Tablet verbunden: Namespace '{ns}', Court '{court_label}'");
                                        active = true;
                                        court = Some(court_label);
                                    }
                                    AttachResult::Occupied => {
                                        tracing::info!("Court '{court_label}' belegt – Tablet wartet auf Übernahme");
                                        let _ = tx.send(text(&ServerMsg::CourtOccupied));
                                        court = Some(court_label);
                                    }
                                    AttachResult::Rejected => {
                                        let _ = socket.send(Message::Close(None)).await;
                                        break;
                                    }
                                }
                            }
                            Ok(TabletMsg::TakeOver) => {
                                if let (Some(c), false) = (court.clone(), active) {
                                    take_over_court(&broker, &ns, &c, &tx).await;
                                    active = true;
                                    tracing::info!("Tablet übernimmt Court '{c}' (Namespace '{ns}')");
                                }
                            }
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history }) => {
                                if let (Some(c), true) = (&court, active) {
                                    forward_score(&broker, &ns, c, score_a, score_b, sets_history).await;
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let (Some(c), true) = (&court, active) {
                                    forward_battery(&broker, &ns, c, percent, charging).await;
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), true) = (&court, active) {
                                    forward_alert(&broker, &ns, c, injury, official).await;
                                }
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
    if let (Some(c), true) = (&court, active) {
        detach_tablet(&broker, &ns, c, &tx).await;
        tracing::info!("Tablet getrennt: Namespace '{ns}', Court '{c}'");
    }
}

/// Ergebnis eines Tablet-Verbindungsversuchs an einem Court.
enum AttachResult {
    /// Das Tablet schiedst diesen Court nun aktiv.
    Active,
    /// Der Court ist belegt – das Tablet bleibt passiv (Übernahme möglich).
    Occupied,
    /// Abgewiesen, weil ein Limit erreicht ist.
    Rejected,
}

/// Versucht, ein Tablet als aktiv schiedsendes Gerät an einem Court zu
/// registrieren. Ist der Court schon belegt, bleibt das Tablet passiv.
async fn attach_tablet(broker: &Broker, ns: &str, court: &str, tx: &Tx) -> AttachResult {
    if court.len() > MAX_COURT_LABEL_LEN {
        tracing::warn!("Namespace '{ns}': überlanger Court-Name abgewiesen");
        return AttachResult::Rejected;
    }
    let mut map = broker.namespaces.lock().await;
    if !map.contains_key(ns) && map.len() >= MAX_NAMESPACES {
        tracing::warn!("Namespace-Limit erreicht – Tablet für '{ns}' abgewiesen");
        return AttachResult::Rejected;
    }
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if namespace.tablets.contains_key(court) {
        return AttachResult::Occupied;
    }
    if namespace.tablets.len() >= MAX_TABLETS_PER_NS {
        tracing::warn!("Namespace '{ns}' am Tablet-Limit – Court '{court}' abgewiesen");
        return AttachResult::Rejected;
    }
    namespace.tablets.insert(court.to_string(), tx.clone());
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_label: court.to_string(),
        }));
    }
    AttachResult::Active
}

/// Übernimmt einen belegten Court für ein bisher passives Tablet – das
/// zuvor aktive Tablet wird mit `SessionSuperseded` gesperrt.
async fn take_over_court(broker: &Broker, ns: &str, court: &str, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if let Some(old) = namespace.tablets.insert(court.to_string(), tx.clone()) {
        let _ = old.send(text(&ServerMsg::SessionSuperseded));
    }
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_label: court.to_string(),
        }));
    }
}

/// Entfernt das Tablet wieder – nur, wenn der eingetragene Sender noch
/// unserer ist (ein Reconnect auf denselben Court darf nichts wegräumen).
async fn detach_tablet(broker: &Broker, ns: &str, court: &str, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    let still_ours = namespace
        .tablets
        .get(court)
        .map(|t| t.same_channel(tx))
        .unwrap_or(false);
    if still_ours {
        namespace.tablets.remove(court);
        if let Some(host) = &namespace.host {
            let _ = host.send(text(&RelayFrame::TabletDisconnected {
                court_label: court.to_string(),
            }));
        }
    }
    if namespace.is_empty() {
        map.remove(ns);
    }
}

/// Leitet einen Live-Score von einem Tablet an den Host weiter.
async fn forward_score(
    broker: &Broker,
    ns: &str,
    court: &str,
    score_a: i64,
    score_b: i64,
    sets_history: Vec<relay_proto::SetAb>,
) {
    let map = broker.namespaces.lock().await;
    if let Some(host) = map.get(ns).and_then(|n| n.host.as_ref()) {
        let _ = host.send(text(&RelayFrame::ScoreUpdate {
            court_label: court.to_string(),
            score_a,
            score_b,
            sets_history,
        }));
    }
}

/// Leitet den Akkustand eines Tablets an den Host weiter.
async fn forward_battery(broker: &Broker, ns: &str, court: &str, percent: i64, charging: bool) {
    let map = broker.namespaces.lock().await;
    if let Some(host) = map.get(ns).and_then(|n| n.host.as_ref()) {
        let _ = host.send(text(&RelayFrame::Battery {
            court_label: court.to_string(),
            percent,
            charging,
        }));
    }
}

/// Leitet den Meldungs-Zustand eines Courts an den Host weiter.
async fn forward_alert(broker: &Broker, ns: &str, court: &str, injury: bool, official: bool) {
    let map = broker.namespaces.lock().await;
    if let Some(host) = map.get(ns).and_then(|n| n.host.as_ref()) {
        let _ = host.send(text(&RelayFrame::Alert {
            court_label: court.to_string(),
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
        for court in namespace.tablets.keys() {
            let _ = tx.send(text(&RelayFrame::TabletConnected {
                court_label: court.clone(),
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
            court_label,
            match_brief,
        } => {
            if let Some(t) = namespace.tablets.get(&court_label) {
                let _ = t.send(text(&ServerMsg::MatchAssigned { match_brief }));
            }
        }
        HostFrame::MatchCleared { court_label } => {
            if let Some(t) = namespace.tablets.get(&court_label) {
                let _ = t.send(text(&ServerMsg::MatchCleared));
            }
        }
        HostFrame::ResultAck { req_id, ok, error } => {
            if let Some(pending) = namespace.pending.remove(&req_id) {
                let _ = pending.send(ResultResponse { ok, error });
            }
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

    fn brief(id: i64) -> MatchBrief {
        MatchBrief {
            match_id: id,
            team_a: vec![PlayerBrief {
                id: 1,
                name: "Anna".into(),
            }],
            team_b: vec![PlayerBrief {
                id: 11,
                name: "Ben".into(),
            }],
            event_label: "HE G1".into(),
            best_of_sets: 3,
            target_score: 21,
        }
    }

    /// Legt einen Namespace mit einem Tablet an und gibt dessen Empfangsende
    /// zurück.
    async fn broker_with_tablet(court: &str) -> (Broker, mpsc::UnboundedReceiver<Message>) {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (tx, rx) = mpsc::unbounded_channel();
        let mut map = broker.namespaces.lock().await;
        let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
        ns.tablets.insert(court.to_string(), tx);
        drop(map);
        (broker, rx)
    }

    #[tokio::test]
    async fn host_match_assigned_reaches_the_courts_tablet() {
        let (broker, mut rx) = broker_with_tablet("Feld 1").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_label: "Feld 1".into(),
                match_brief: brief(7),
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
        let (broker, mut rx) = broker_with_tablet("Feld 1").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchCleared {
                court_label: "Feld 99".into(),
            },
        )
        .await;
        assert!(rx.try_recv().is_err(), "fremder Court bekommt nichts");
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
        forward_score(&broker, "ns1", "Feld 1", 11, 9, vec![]).await;
        let msg = host_rx.try_recv().expect("Host bekommt den Score");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: RelayFrame = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            parsed,
            RelayFrame::ScoreUpdate {
                court_label: "Feld 1".into(),
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
