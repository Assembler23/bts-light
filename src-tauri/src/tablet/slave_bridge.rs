//! Monitor-, Tablet- und Träger-Brücke des Cloud-Ansage-Slaves (ferne Halle,
//! ohne Extra-Rechner).
//!
//! In der fernen Halle läuft kein LAN-Tablet-Server (der Slave sagt nur an,
//! siehe [`super::relay_client`]). Diese Brücke bedient trotzdem Tablets und
//! Court-Monitore — in **zwei** Betriebsarten:
//!
//! **Weiterleitung (Vorgabe).** Alles zeigt per `303` auf den Master-Relay;
//! die Geräte hängen direkt an der Cloud ([ADR 0002](../../../docs/adr/0002-ferne-halle-direkt-cloud-geraete.md),
//! „Weg A"). Der Slave ist dabei nicht im Datenpfad.
//!
//! **Träger (`slave_mux.enabled`).** Die Brücke liefert die Seiten **selbst**
//! aus und terminiert die WebSockets lokal; ihr Fachverkehr läuft gebündelt
//! über **eine** Trägerverbindung zum Relay ([ADR 0048](../../../docs/adr/0048-substrom-adressierung-traeger.md)).
//! In der Halle taucht dann keine badhub-Adresse mehr auf.
//!
//! **Warum der Slave die Seiten selbst ausliefern muss, wenn er terminiert:**
//! Die `seiten_marke` ist ein Hash über den Seiteninhalt, den *die
//! ausliefernde Binärdatei* berechnet, und das Tablet vergleicht sie gegen
//! den `Pong`. Käme die Seite vom Slave und der `Pong` vom Relay, wichen die
//! Marken fast immer ab — jedes Tablet der Halle meldete dauerhaft
//! „veraltet", und ein Reload-Befehl liefe in eine Schleife. Der Slave ist
//! eine bts-light-Binärdatei und trägt dieselben Seiten wie der LAN-Server;
//! seine Marke ist für seine Seite die richtige.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, RawQuery, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};

use relay_proto::{html_escape, CourtBrief, StreamKind};

use super::assets;
use super::carrier::{AnGeraet, Traeger};
use super::server::TABLET_PORT;

/// HTTPS-Basis des Cloud-Relays (identisch zum Relay-Client).
const RELAY_HTTP: &str = "https://badhub.de/bts-relay";

/// Laufzeit-Konfiguration der Brücke: Master-Namespace (Ziel aller
/// Weiterleitungen), eigene Halle (Filter der Feld-Auswahlseite) und —
/// falls eingeschaltet — der Träger.
struct BridgeConfig {
    master_namespace: String,
    /// Eigene Halle (BTP-Location); leer = keine Hallen-Einschränkung.
    hall: String,
    /// Gesetzt = Träger-Betrieb. Ist er nicht bereit, weicht jede Route
    /// einzeln auf die Weiterleitung aus — die Halle arbeitet weiter.
    traeger: Option<Arc<Traeger>>,
    /// Für die durchgereichten HTTP-Abrufe (Bilder, Zustände, Ergebnisse).
    http: reqwest::Client,
}

impl BridgeConfig {
    /// Trägt der Träger gerade? Nur dann terminiert die Brücke lokal.
    fn traegt(&self) -> Option<&Arc<Traeger>> {
        self.traeger.as_ref().filter(|t| t.bereit())
    }

    /// Basis-URL des eigenen Namespace am Relay.
    fn relay_basis(&self) -> String {
        format!("{RELAY_HTTP}/{}", self.master_namespace)
    }
}

/// Weiterleitungs-Ziel für den Cloud-Monitor des Masters. Der rohe
/// Query-String (inkl. `device=…`) wird 1:1 angehängt, damit der Master-Relay
/// das Gerät seinem Feld zuordnen kann. Reine Funktion → testbar.
fn monitor_redirect_url(master_ns: &str, raw_query: Option<&str>) -> String {
    let base = format!("{RELAY_HTTP}/{master_ns}/monitor");
    match raw_query {
        Some(q) if !q.is_empty() => format!("{base}?{q}"),
        _ => base,
    }
}

/// Weiterleitungs-Ziel für die Cloud-Tablet-Seite eines Felds. `id` ist eine
/// CourtID (i64) – rein numerisch, daher keine URL-Injektion möglich.
fn court_redirect_url(master_ns: &str, court_id: i64) -> String {
    format!("{RELAY_HTTP}/{master_ns}/court/{court_id}")
}

/// Rendert die Feld-Auswahlseite: je Feld der eigenen Halle ein großer Knopf.
///
/// `lokal` = der Träger trägt, die Knöpfe zeigen auf **diesen** Server;
/// sonst direkt in die Cloud. `hall` leer = alle Felder. Reine Funktion
/// (Felder werden vom Aufrufer geholt) → testbar.
fn felder_page_html(master_ns: &str, courts: &[CourtBrief], hall: &str, lokal: bool) -> String {
    let mut buttons = String::new();
    for c in courts.iter().filter(|c| hall.is_empty() || c.hall == hall) {
        let ziel = if lokal {
            format!("/court/{}", c.id)
        } else {
            court_redirect_url(master_ns, c.id)
        };
        buttons.push_str(&format!(
            "<a class=\"feld\" href=\"{}\">{}</a>",
            html_escape(&ziel),
            html_escape(&c.label),
        ));
    }
    if buttons.is_empty() {
        buttons.push_str(
            "<p class=\"hint\">Noch keine Felder – warte auf den Master (Cloud aktiv?).</p>",
        );
    }
    format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>Feld wählen</title><style>\
html,body{{margin:0;height:100%;background:#0b1120;color:#f8fafc;\
font-family:system-ui,sans-serif}}\
h1{{font-size:1.4rem;padding:1rem;margin:0}}\
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(9rem,1fr));\
gap:.8rem;padding:0 1rem 1rem}}\
.feld{{display:flex;align-items:center;justify-content:center;min-height:5rem;\
background:#1e293b;color:#f8fafc;text-decoration:none;border-radius:.8rem;\
font-size:1.3rem;font-weight:700;border:2px solid #334155}}\
.feld:active{{background:#334155}}\
.hint{{padding:0 1rem;color:#94a3b8}}\
</style></head><body><h1>Feld wählen</h1><div class=\"grid\">{buttons}</div></body></html>"
    )
}

/// `/health` — bestätigt dem Pi-Subnetz-Scan einen erreichbaren Server.
async fn health() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "ok": true, "bridge": "slave" })),
    )
}

/// `/monitor[?device=…]` — lokale Anzeige im Träger-Betrieb, sonst 303.
async fn monitor(State(cfg): State<Arc<BridgeConfig>>, RawQuery(query): RawQuery) -> Response {
    if cfg.traegt().is_some() {
        // `__BASE__` wird serverseitig gesetzt (anders als bei `tablet.html`,
        // das seinen Präfix selbst ableitet) — mit UNSEREM Präfix, sonst
        // fragte die Anzeige Bilder und Zustände weiter bei badhub ab.
        let body = assets::MONITOR_HTML
            .replace("__MODE__", "device")
            .replace("__BASE__", "/")
            .replace("__COURT_LABEL__", "");
        return ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response();
    }
    Redirect::to(&monitor_redirect_url(
        &cfg.master_namespace,
        query.as_deref(),
    ))
    .into_response()
}

/// `/court/{id}/display` — feste Court-Anzeige (nur im Träger-Betrieb).
async fn court_display(State(cfg): State<Arc<BridgeConfig>>, Path(id): Path<i64>) -> Response {
    if cfg.traegt().is_none() {
        return Redirect::to(&format!("{}/court/{id}/display", cfg.relay_basis())).into_response();
    }
    let body = assets::MONITOR_HTML
        .replace("__MODE__", "fixed")
        .replace("__BASE__", "/")
        .replace("__COURT_LABEL__", "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// `/court/{id}` — Tablet-Seite lokal im Träger-Betrieb, sonst 303.
async fn court(State(cfg): State<Arc<BridgeConfig>>, Path(id): Path<i64>) -> Response {
    if cfg.traegt().is_none() {
        return Redirect::to(&court_redirect_url(&cfg.master_namespace, id)).into_response();
    }
    // Label kennt der Slave aus der Relay-Feldliste; fehlt es, bleibt die
    // Nummer stehen — die Seite ist dadurch nicht weniger bedienbar.
    let label = super::relay_client::fetch_courts(&cfg.master_namespace)
        .await
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.label)
        .unwrap_or_else(|| format!("Feld {id}"));
    let body = assets::TABLET_HTML
        .replace("__COURT_ID__", &id.to_string())
        .replace("__COURT_LABEL__", &html_escape(&label))
        // Der Cloud-Weg setzt die PIN leer; hier ebenso — die ferne Halle
        // bedient dieselben Geräte wie zuvor.
        .replace("__TABLET_PIN__", "")
        // UNSERE Marke: Wir liefern die Seite aus, also stempeln wir auch
        // den Pong (siehe Modul-Doku).
        .replace("__SEITEN_MARKE__", assets::seiten_marke());
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// `/felder` — Feld-Auswahlseite (Felder dieser Halle).
async fn felder(State(cfg): State<Arc<BridgeConfig>>) -> impl IntoResponse {
    // Feldliste live aus dem Relay (vom Master gepusht). Leer bei Netz-/
    // Parse-Fehler → die Seite zeigt dann den Warte-Hinweis.
    let courts = super::relay_client::fetch_courts(&cfg.master_namespace).await;
    Html(felder_page_html(
        &cfg.master_namespace,
        &courts,
        &cfg.hall,
        cfg.traegt().is_some(),
    ))
}

type Response = axum::response::Response;

/// Query der Monitor-Nudge-WS: optionale CourtID (wie am LAN-Server).
#[derive(serde::Deserialize)]
struct MonitorWsQuery {
    court: Option<i64>,
}

/// `/ws` — Tablet-WebSocket, lokal terminiert und als Substrom getragen.
async fn tablet_ws(ws: WebSocketUpgrade, State(cfg): State<Arc<BridgeConfig>>) -> Response {
    if cfg.traegt().is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ws.on_upgrade(move |socket| substrom_bedienen(socket, cfg, StreamKind::Tablet, None))
        .into_response()
}

/// `/monitor-ws` — Anzeige-Nudge, lokal terminiert und als Substrom getragen.
async fn monitor_ws(
    ws: WebSocketUpgrade,
    State(cfg): State<Arc<BridgeConfig>>,
    Query(q): Query<MonitorWsQuery>,
) -> Response {
    if cfg.traegt().is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    ws.on_upgrade(move |socket| substrom_bedienen(socket, cfg, StreamKind::Monitor, q.court))
        .into_response()
}

/// Bedient **eine** lokale WebSocket als Substrom des Trägers.
///
/// Der Slave ist hier für die Liveness zuständig: Bleibt das Gerät länger als
/// [`super::carrier::geraet_stale`] stumm, wird der Substrom geschlossen und
/// damit sein Court-Slot freigegeben. Ohne das hielte ein totes Tablet sein
/// Feld, solange der Träger lebt.
async fn substrom_bedienen(
    mut socket: WebSocket,
    cfg: Arc<BridgeConfig>,
    kind: StreamKind,
    court: Option<i64>,
) {
    let Some(traeger) = cfg.traegt().cloned() else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };
    let Some((stream, mut vom_relay)) = traeger.oeffne(kind, court) else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };

    let mut ping = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut letztes = tokio::time::Instant::now();

    loop {
        tokio::select! {
            eingang = socket.recv() => {
                let Some(Ok(msg)) = eingang else { break };
                letztes = tokio::time::Instant::now();
                match msg {
                    Message::Text(t) => traeger.frame(stream, t.to_string()),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            antwort = vom_relay.recv() => {
                match antwort {
                    Some(AnGeraet::Frame(p)) => {
                        if socket.send(Message::Text(p.into())).await.is_err() { break }
                    }
                    Some(AnGeraet::Schluss) | None => break,
                }
            }
            _ = ping.tick() => {
                if letztes.elapsed() >= super::carrier::geraet_stale() {
                    tracing::warn!("Gerät an Substrom {stream} stumm – Verbindung verworfen");
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }

    traeger.schliesse(stream);
}

/// Reicht einen lesenden HTTP-Abruf an den Relay durch.
///
/// **Bewusst NICHT durch den Träger.** Über die Tablet-Strecke laufen
/// höchstens 64 KB; Werbebilder (12 MB) und Turnierlogo (2 MB) sind
/// HTTP-Abrufe. Zöge man sie in den Träger, teilten sie sich die Leitung mit
/// den Punkt-Frames (ADR 0048). Für das Gerät bleibt die Adresse trotzdem
/// lokal — es merkt nichts davon.
async fn durchreichen(cfg: &BridgeConfig, pfad: &str, query: Option<&str>) -> Response {
    let mut url = format!("{}{pfad}", cfg.relay_basis());
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(q);
    }
    match cfg.http.get(&url).send().await {
        Ok(antwort) => {
            let status =
                StatusCode::from_u16(antwort.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let typ = antwort
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            match antwort.bytes().await {
                Ok(rumpf) => (status, [(header::CONTENT_TYPE, typ)], rumpf).into_response(),
                Err(e) => {
                    tracing::warn!("Durchreichen von {url}: {e}");
                    StatusCode::BAD_GATEWAY.into_response()
                }
            }
        }
        Err(e) => {
            tracing::warn!("Durchreichen von {url}: {e}");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Alle lesenden Zusatz-Abrufe der Seiten (`/info/*`, `/ads/*`, `/courts`,
/// `/flags/*`, `/monitor/state`, `/court/{id}/state`).
async fn lesend(
    State(cfg): State<Arc<BridgeConfig>>,
    uri: axum::http::Uri,
    RawQuery(query): RawQuery,
) -> Response {
    if cfg.traegt().is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    // Sonderfall Vereinslogo: `tablet.html` fragt es bei leerem Präfix unter
    // `/info/club-logo` an — eine Route, die nur der LAN-Server hat, der
    // Relay aber **nicht**. Durchgereicht ergäbe das eine 404 und das Logo
    // fehlte. Der Slave holt es deshalb selbst bei badhub; für das Gerät
    // bleibt die Adresse lokal, und das Ziel „kein Gerät sieht badhub" hält.
    if uri.path() == "/info/club-logo" {
        return club_logo(&cfg, query.as_deref()).await;
    }
    durchreichen(&cfg, uri.path(), query.as_deref()).await
}

/// Holt ein Vereinslogo bei badhub — siehe [`lesend`].
async fn club_logo(cfg: &BridgeConfig, query: Option<&str>) -> Response {
    let mut url = "https://badhub.de/api/v1/club-logo".to_string();
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(q);
    }
    match cfg.http.get(&url).send().await {
        Ok(antwort) => {
            let status =
                StatusCode::from_u16(antwort.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let typ = antwort
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/png")
                .to_string();
            match antwort.bytes().await {
                Ok(rumpf) => (status, [(header::CONTENT_TYPE, typ)], rumpf).into_response(),
                // Ein fehlendes Logo ist kein Störfall: Die Seite blendet das
                // Bild per `onerror` aus und zählt unbeirrt weiter.
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `/result` — Ergebnis des Tablets an den Relay.
///
/// Geht als eigener HTTP-Aufruf, nicht durch den Träger: Der Relay wartet auf
/// die Bestätigung des Masters (`RESULT_TIMEOUT` 8 s), und dieses Warten hätte
/// im Träger andere Ströme aufgehalten. Die Prüfung des Ergebnisses bleibt
/// unverändert beim Master (R5).
async fn result(State(cfg): State<Arc<BridgeConfig>>, rumpf: String) -> Response {
    if cfg.traegt().is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let url = format!("{}/result", cfg.relay_basis());
    match cfg
        .http
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(rumpf)
        .send()
        .await
    {
        Ok(antwort) => {
            let status =
                StatusCode::from_u16(antwort.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let text = antwort.text().await.unwrap_or_default();
            (status, text).into_response()
        }
        Err(e) => {
            tracing::warn!("Ergebnis an {url}: {e}");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Startet die Slave-Brücke auf `0.0.0.0:8088`.
///
/// `master_namespace` muss vorab validiert sein (siehe
/// `relay_client::valid_relay_namespace`). Im Slave-Modus läuft KEIN
/// LAN-Tablet-Server, der Port ist also frei. `hall` filtert die
/// Feld-Auswahlseite (leer = alle Felder). `traeger` gesetzt = Träger-Betrieb.
pub async fn run(
    master_namespace: String,
    hall: String,
    traeger: Option<Arc<Traeger>>,
) -> std::io::Result<()> {
    let cfg = Arc::new(BridgeConfig {
        master_namespace,
        hall,
        traeger,
        http: reqwest::Client::new(),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/monitor", get(monitor))
        .route("/felder", get(felder))
        .route("/court/{id}", get(court))
        .route("/court/{id}/display", get(court_display))
        .route("/ws", get(tablet_ws))
        .route("/monitor-ws", get(monitor_ws))
        .route("/result", post(result))
        // Lesende Zusatz-Abrufe der Seiten — als eigene HTTPS-Anfrage, nicht
        // durch den Träger (siehe `durchreichen`).
        .route("/courts", get(lesend))
        .route("/monitor/state", get(lesend))
        .route("/court/{id}/state", get(lesend))
        .route("/info/{*rest}", get(lesend))
        .route("/ads/{*rest}", get(lesend))
        .route("/flags/{*rest}", get(lesend))
        .with_state(cfg);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", TABLET_PORT)).await?;
    tracing::info!(
        "Slave-Brücke lauscht auf 0.0.0.0:{TABLET_PORT} → Tablets & Monitore des Masters"
    );
    axum::serve(listener, app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief(id: i64, label: &str, hall: &str) -> CourtBrief {
        CourtBrief {
            id,
            label: label.into(),
            hall: hall.into(),
            // Reist per Serde-Default durch die Brücke — hier irrelevant.
            hall_color: None,
        }
    }

    #[test]
    fn monitor_redirect_appends_device_query() {
        assert_eq!(
            monitor_redirect_url("abc-123", Some("device=pi-0000000070a061db")),
            "https://badhub.de/bts-relay/abc-123/monitor?device=pi-0000000070a061db"
        );
    }

    #[test]
    fn monitor_redirect_without_query() {
        assert_eq!(
            monitor_redirect_url("abc-123", None),
            "https://badhub.de/bts-relay/abc-123/monitor"
        );
        assert_eq!(
            monitor_redirect_url("abc-123", Some("")),
            "https://badhub.de/bts-relay/abc-123/monitor"
        );
    }

    #[test]
    fn court_redirect_targets_cloud_tablet_page() {
        assert_eq!(
            court_redirect_url("abc-123", 47),
            "https://badhub.de/bts-relay/abc-123/court/47"
        );
    }

    #[test]
    fn felder_page_lists_only_own_hall_and_links_to_cloud() {
        let courts = vec![
            brief(47, "WR · 1", "WR"),
            brief(48, "WR · 2", "WR"),
            brief(37, "HM · 1", "HM"),
        ];
        let html = felder_page_html("ns-1", &courts, "WR", false);
        assert!(html.contains("https://badhub.de/bts-relay/ns-1/court/47"));
        assert!(html.contains("WR · 1"));
        // Fremde Halle nicht anbieten.
        assert!(!html.contains("court/37"));
        assert!(!html.contains("HM · 1"));
    }

    #[test]
    fn felder_page_without_hall_filter_shows_all() {
        let courts = vec![brief(47, "WR · 1", "WR"), brief(37, "HM · 1", "HM")];
        let html = felder_page_html("ns-1", &courts, "", false);
        assert!(html.contains("court/47"));
        assert!(html.contains("court/37"));
    }

    #[test]
    fn felder_page_empty_shows_hint() {
        let html = felder_page_html("ns-1", &[], "WR", false);
        assert!(html.contains("Noch keine Felder"));
    }

    /// Im Träger-Betrieb zeigt die Feldauswahl auf **diesen** Server — sonst
    /// führe der erste Klick die Crew wieder auf eine badhub-Adresse, und das
    /// Ziel des ganzen Umbaus wäre verfehlt.
    #[test]
    fn felder_page_verlinkt_im_traeger_betrieb_lokal() {
        let courts = vec![brief(47, "WR · 1", "WR")];
        let html = felder_page_html("ns-1", &courts, "WR", true);
        assert!(html.contains("href=\"/court/47\""), "lokaler Link fehlt");
        assert!(
            !html.contains("badhub.de"),
            "im Träger-Betrieb darf keine Cloud-Adresse auftauchen: {html}"
        );
    }

    /// Der Slave stempelt die Seite mit **seiner** Marke. Käme sie vom Relay,
    /// meldete jedes Tablet der Halle dauerhaft „veraltet" und ein
    /// Reload-Befehl liefe in eine Schleife.
    #[test]
    fn tablet_seite_traegt_die_marke_dieser_binaerdatei() {
        let marke = assets::seiten_marke();
        assert!(!marke.is_empty(), "Marke darf nicht leer sein");
        let seite = assets::TABLET_HTML.replace("__SEITEN_MARKE__", marke);
        assert!(seite.contains(marke));
        assert!(
            !seite.contains("__SEITEN_MARKE__"),
            "Platzhalter muss ersetzt sein"
        );
    }
}
