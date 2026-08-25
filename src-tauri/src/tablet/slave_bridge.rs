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

/// HTTPS-Basis des Cloud-Relays (identisch zum Relay-Client) –
/// Produktiv- oder Testsystem, siehe [`crate::badhub_host`].
fn relay_http() -> String {
    crate::badhub_host::relay_https()
}

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

    /// Ist der Träger-Betrieb **eingeschaltet** — unabhängig davon, ob die
    /// Verbindung gerade steht?
    ///
    /// Der Unterschied entscheidet, was bei einem Aussetzer passiert:
    /// Eingeschaltet, aber nicht bereit → **warten** (die Herkunft bleibt).
    /// Gar nicht eingeschaltet → weiterleiten wie eh und je.
    fn traeger_betrieb(&self) -> bool {
        self.traeger.is_some()
    }

    /// Basis-URL des eigenen Namespace am Relay.
    fn relay_basis(&self) -> String {
        format!("{}/{}", relay_http(), self.master_namespace)
    }
}

/// Weiterleitungs-Ziel für den Cloud-Monitor des Masters. Der rohe
/// Query-String (inkl. `device=…`) wird 1:1 angehängt, damit der Master-Relay
/// das Gerät seinem Feld zuordnen kann. Reine Funktion → testbar.
fn monitor_redirect_url(master_ns: &str, raw_query: Option<&str>) -> String {
    let base = format!("{}/{master_ns}/monitor", relay_http());
    match raw_query {
        Some(q) if !q.is_empty() => format!("{base}?{q}"),
        _ => base,
    }
}

/// Weiterleitungs-Ziel für die Cloud-Tablet-Seite eines Felds. `id` ist eine
/// CourtID (i64) – rein numerisch, daher keine URL-Injektion möglich.
fn court_redirect_url(master_ns: &str, court_id: i64) -> String {
    format!("{}/{master_ns}/court/{court_id}", relay_http())
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

/// Seite für „der Träger steht gerade nicht".
///
/// **Warum warten statt weiterleiten.** Eine Weiterleitung in die Cloud
/// wechselt die Herkunft, und daran hängt mehr, als es aussieht:
///
/// * Ein **Tablet** bewahrt sein noch nicht bestätigtes Ergebnis unter der
///   Adresse auf, unter der es erfasst wurde. Nach einem Wechsel ist es für
///   das Gerät unsichtbar.
/// * Ein **Court-Monitor** erzeugt seine Geräte-Kennung ebenfalls dort. Nach
///   einem Wechsel hat er eine neue — und damit seine Feld-Zuweisung
///   verloren. Jemand müsste ihn in der Turnierleitung neu zuweisen.
///
/// Ein Träger-Aussetzer dauert typischerweise Sekunden (Backoff 1 s, dann
/// ansteigend). Dafür die Herkunft zu wechseln, wäre ein schlechter Tausch:
/// Die Seite lädt sich selbst neu und verschwindet von allein, sobald es
/// weitergeht. Bewusst ohne Knopf — es gibt nichts zu entscheiden, und ein
/// „Weiter"-Knopf lüde nur dazu ein, die Herkunft doch zu wechseln.
fn warteseite_html(was: &str) -> String {
    format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<meta http-equiv=\"refresh\" content=\"3\">\
<title>Verbindung wird aufgebaut</title><style>\
html,body{{margin:0;height:100%;background:#0b1120;color:#f8fafc;\
font-family:system-ui,sans-serif;display:flex;align-items:center;\
justify-content:center;text-align:center}}\
.box{{padding:2rem;max-width:32rem}}\
h1{{font-size:1.6rem;margin:0 0 .8rem}}\
p{{color:#cbd5e1;line-height:1.5;margin:.4rem 0}}\
.klein{{color:#94a3b8;font-size:.85rem;margin-top:1.2rem}}\
.punkt{{display:inline-block;width:.6rem;height:.6rem;border-radius:50%;\
background:#38bdf8;margin-right:.4rem;animation:blink 1.2s infinite}}\
@keyframes blink{{50%{{opacity:.25}}}}\
</style></head><body><div class=\"box\">\
<h1><span class=\"punkt\"></span>Verbindung wird aufgebaut</h1>\
<p>Die Halle verbindet sich gerade mit der Turnierleitung. {was}</p>\
<p>Diese Seite meldet sich von selbst, sobald es weitergeht — \
bitte nichts anfassen.</p>\
<p class=\"klein\">Dauert es länger als ein paar Minuten: Läuft der \
Turnier-PC in der Haupthalle, und hat dieser Rechner Internet?</p>\
</div></body></html>"
    )
}

/// `/health` — im Träger-Betrieb der **echte** Zustand vom Relay, sonst die
/// knappe Bestätigung für den Pi-Subnetz-Scan.
///
/// Warum nicht immer der Stummel: Zwei Dinge hängen an dieser Antwort.
/// `tablet.html` holt darüber `serverNowMs` und richtet seine Uhr danach —
/// ohne das laufen die Pausen-Enden, die in Server-Zeit zur Turnierleitung
/// reisen, gegen die ungeprüfte Tablet-Uhr (genau die Regression, die der
/// Kommentar an `tablet.html:1783` als früheren Review-Fund festhält). Und
/// `overview.html` bezieht von hier seine ganze Feld-Übersicht — der Stummel
/// ließe sie leer.
///
/// Der Subnetz-Scan der Pis ist zufrieden, solange überhaupt `200` kommt;
/// die durchgereichte Antwort erfüllt das ebenso.
async fn health(State(cfg): State<Arc<BridgeConfig>>, RawQuery(query): RawQuery) -> Response {
    if cfg.traegt().is_some() {
        return durchreichen(&cfg, "/health", query.as_deref(), None).await;
    }
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "ok": true, "bridge": "slave" })),
    )
        .into_response()
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
    if cfg.traeger_betrieb() {
        // Für Anzeigen wiegt der Herkunftswechsel sogar schwerer als beim
        // Tablet: Der Court-Monitor erzeugt seine Geräte-Kennung im Speicher
        // der Seite. Nach einem Wechsel hätte er eine neue — und damit seine
        // Feld-Zuweisung verloren.
        return (
            [(header::CACHE_CONTROL, "no-store")],
            Html(warteseite_html("Die Anzeige kommt gleich zurück.")),
        )
            .into_response();
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
        if cfg.traeger_betrieb() {
            return (
                [(header::CACHE_CONTROL, "no-store")],
                Html(warteseite_html("Die Anzeige kommt gleich zurück.")),
            )
                .into_response();
        }
        return Redirect::to(&format!("{}/court/{id}/display", cfg.relay_basis())).into_response();
    }
    // Label wie am LAN-Server mitgeben — sonst steht bis zum ersten
    // Zustands-Abruf nichts im Kopf der Anzeige.
    let label = super::relay_client::fetch_courts(&cfg.master_namespace)
        .await
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.label)
        .unwrap_or_default();
    let body = assets::MONITOR_HTML
        .replace("__MODE__", "fixed")
        .replace("__BASE__", "/")
        .replace("__COURT_LABEL__", &html_escape(&label));
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// `/court/{id}` — Tablet-Seite lokal im Träger-Betrieb, sonst 303.
async fn court(State(cfg): State<Arc<BridgeConfig>>, Path(id): Path<i64>) -> Response {
    if cfg.traegt().is_none() {
        // Träger-Betrieb an, aber gerade keine Verbindung: warten statt
        // umleiten. Ein Herkunftswechsel kostete das noch nicht bestätigte
        // Ergebnis dieses Tablets (siehe `warteseite_html`).
        if cfg.traeger_betrieb() {
            return (
                [(header::CACHE_CONTROL, "no-store")],
                Html(warteseite_html("Das Spielfeld ist gleich wieder da.")),
            )
                .into_response();
        }
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

/// Beantwortet einen Versions-Ping des Tablets mit der Marke **dieser**
/// Binärdatei — sonst `None`, der Frame reist dann normal weiter.
///
/// Siehe die Begründung an der Aufrufstelle: Der Slave hat die Seite
/// ausgeliefert, also muss auch der Pong von ihm kommen.
fn pong_auf_ping(roh: &str) -> Option<String> {
    match serde_json::from_str::<relay_proto::TabletMsg>(roh) {
        Ok(relay_proto::TabletMsg::Ping) => serde_json::to_string(&relay_proto::ServerMsg::Pong {
            marke: assets::seiten_marke().to_string(),
        })
        .ok(),
        _ => None,
    }
}

/// Ist das ein `Pong` des Relays? Der wird verworfen — siehe Aufrufstelle.
fn ist_pong(roh: &str) -> bool {
    matches!(
        serde_json::from_str::<relay_proto::ServerMsg>(roh),
        Ok(relay_proto::ServerMsg::Pong { .. })
    )
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
                    Message::Text(t) => {
                        // Den Versions-Ping beantwortet der Slave SELBST —
                        // er hat die Seite ausgeliefert, also ist seine Marke
                        // die richtige. Antwortete der Relay, käme SEINE, und
                        // weil Relay (bei jedem main-Merge) und App (bei
                        // Tags) aus verschiedenen Ständen stammen, wichen die
                        // beiden fast immer ab: Das Tablet lädt neu, bekommt
                        // dieselbe Seite mit derselben Marke, lädt wieder neu.
                        if let Some(pong) = pong_auf_ping(&t) {
                            if socket.send(Message::Text(pong.into())).await.is_err() { break }
                        }
                        // ABER: trotzdem durchreichen. Der Ping ist das
                        // EINZIGE, was ein Tablet ohne Punkteingabe
                        // regelmäßig sendet; am Relay stempelt er das
                        // Lebenszeichen des Substroms. Verschluckt man ihn,
                        // räumt die dortige Rückfallebene den Substrom nach
                        // 15 s ab — jedes ruhige Tablet verlöre im Minutentakt
                        // seinen Court-Slot (Review-Fund 25.08.2026).
                        traeger.frame(stream, t.to_string());
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            antwort = vom_relay.recv() => {
                match antwort {
                    Some(AnGeraet::Frame(p)) => {
                        // Der Relay beantwortet den durchgereichten Ping mit
                        // SEINER Marke. Das Gerät hat unsere längst — seine
                        // wäre die falsche und löste den Reload aus, den wir
                        // gerade verhindern. Also schlucken.
                        if ist_pong(&p) { continue }
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

/// Darf dieser Pfad an den Relay weitergereicht werden?
///
/// **Sicherheitsgrenze, kein Schönheitsfilter.** Die Ziel-URL entsteht durch
/// Anhängen an `…/bts-relay/<eigener-namespace>`, und `Url::parse` rechnet
/// `..`-Schritte anschließend heraus. Ein Gerät im Hallennetz könnte damit aus
/// dem eigenen Namespace ausbrechen und über die Brücke **fremde Turniere**
/// lesen — die Brücke wäre ein offener GET-Proxy auf beliebige
/// badhub.de-Pfade. Der Weg muss deshalb hier enden, bevor die URL gebaut
/// wird.
///
/// Geprüft wird auf rohe **und** prozentkodierte Aufstiege; axum normalisiert
/// den Pfad nicht.
fn pfad_ist_harmlos(pfad: &str) -> bool {
    if !pfad.starts_with('/') {
        return false;
    }
    let klein = pfad.to_ascii_lowercase();
    // `%2e` ist der kodierte Punkt, `%2f` der kodierte Schrägstrich — beide
    // dienen hier nur dazu, einen Aufstieg zu verschleiern.
    if klein.contains("%2e") || klein.contains("%2f") || klein.contains('\\') {
        return false;
    }
    !pfad.split('/').any(|teil| teil == ".." || teil == ".")
}

/// Reicht einen lesenden HTTP-Abruf an den Relay durch.
///
/// **Bewusst NICHT durch den Träger.** Über die Tablet-Strecke laufen
/// höchstens 64 KB; Werbebilder (12 MB) und Turnierlogo (2 MB) sind
/// HTTP-Abrufe. Zöge man sie in den Träger, teilten sie sich die Leitung mit
/// den Punkt-Frames (ADR 0048). Für das Gerät bleibt die Adresse trotzdem
/// lokal — es merkt nichts davon.
async fn durchreichen(
    cfg: &BridgeConfig,
    pfad: &str,
    query: Option<&str>,
    if_none_match: Option<&str>,
) -> Response {
    if !pfad_ist_harmlos(pfad) {
        tracing::warn!("Durchreichen abgelehnt (Pfad verlässt den Namespace): {pfad}");
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut url = format!("{}{pfad}", cfg.relay_basis());
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(q);
    }
    let mut anfrage = cfg.http.get(&url);
    // ETag durchreichen — in **beide** Richtungen. Anzeigen und Tablets
    // bauen ihr ganzes Abruf-Budget darauf: Sie schicken `If-None-Match` und
    // erwarten ein `304` ohne Rumpf (Spec monitor-livestand-push, S1).
    // Verschluckt die Brücke den Kopf, holt jede Anzeige der fernen Halle bei
    // jedem Takt den vollen Rumpf — die Ersparnis wäre ausgerechnet für die
    // Halle abgeschaltet, für die dieser Umbau gebaut ist.
    if let Some(marke) = if_none_match {
        anfrage = anfrage.header(reqwest::header::IF_NONE_MATCH, marke);
    }
    match anfrage.send().await {
        Ok(antwort) => {
            let status =
                StatusCode::from_u16(antwort.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let typ = antwort
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let etag = antwort
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let cache = antwort
                .headers()
                .get(reqwest::header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            // Bei `304` gibt es keinen Rumpf — und es darf auch keiner
            // erfunden werden, sonst hielte die Anzeige ihn für neue Daten.
            if status == StatusCode::NOT_MODIFIED {
                let mut antwort = Response::new(axum::body::Body::empty());
                *antwort.status_mut() = status;
                if let Some(e) = etag.and_then(|e| e.parse().ok()) {
                    antwort.headers_mut().insert(header::ETAG, e);
                }
                return antwort;
            }
            match antwort.bytes().await {
                Ok(rumpf) => {
                    let mut fertig = (status, [(header::CONTENT_TYPE, typ)], rumpf).into_response();
                    if let Some(e) = etag.and_then(|e| e.parse().ok()) {
                        fertig.headers_mut().insert(header::ETAG, e);
                    }
                    if let Some(c) = cache.and_then(|c| c.parse().ok()) {
                        fertig.headers_mut().insert(header::CACHE_CONTROL, c);
                    }
                    fertig
                }
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
    kopf: axum::http::HeaderMap,
) -> Response {
    if cfg.traegt().is_none() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let marke = kopf
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    // Sonderfall Vereinslogo: `tablet.html` fragt es bei leerem Präfix unter
    // `/info/club-logo` an — eine Route, die nur der LAN-Server hat, der
    // Relay aber **nicht**. Durchgereicht ergäbe das eine 404 und das Logo
    // fehlte. Der Slave holt es deshalb selbst bei badhub; für das Gerät
    // bleibt die Adresse lokal, und das Ziel „kein Gerät sieht badhub" hält.
    if uri.path() == "/info/club-logo" {
        return club_logo(&cfg, query.as_deref()).await;
    }
    durchreichen(&cfg, uri.path(), query.as_deref(), marke.as_deref()).await
}

/// Holt ein Vereinslogo bei badhub — siehe [`lesend`].
async fn club_logo(cfg: &BridgeConfig, query: Option<&str>) -> Response {
    let mut url = crate::badhub_host::api_url("v1/club-logo");
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

/// `/tablet-log` und `/pi-log` — Diagnose-Zeilen der lokal ausgelieferten
/// Seiten.
///
/// Sie landen im Log **dieses** Rechners und reisen von dort über den
/// gewohnten Weg (`log_upload.rs`) weiter, wenn der Betreiber das eingestellt
/// hat. Bewusst nicht an den Relay durchgereicht: Dort gibt es keine
/// Gegenstelle dafür, und die Zeilen gehören ohnehin zu **diesem** Gerätepark.
///
/// **Keine Frame-Nutzlast ins Log** — durch die Brücke reisen Spielernamen
/// und Lizenznummern; das Diagnose-Log wird hochgeladen.
async fn log_annehmen(
    Query(q): Query<std::collections::HashMap<String, String>>,
    rumpf: String,
) -> impl IntoResponse {
    let wer = q
        .get("court")
        .or_else(|| q.get("device"))
        .map(String::as_str)
        .unwrap_or("unbekannt");
    // Auf ein vernünftiges Maß kürzen: Die Seiten schicken bis zu ~800
    // Zeilen am Stück.
    let kurz: String = rumpf.chars().take(16 * 1024).collect();
    tracing::info!("Diagnose-Log der fernen Halle ({wer}):\n{kurz}");
    StatusCode::NO_CONTENT
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
        // Diagnose-Uploads der lokal ausgelieferten Seiten. Ohne diese
        // Routen liefen sie ins 404 — ausgerechnet in der fernen Halle, wo
        // man bei einer Störung am wenigsten danebenstehen kann.
        .route("/tablet-log", post(log_annehmen))
        .route("/pi-log", post(log_annehmen))
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

    /// Die Warteseite darf **niemals** in die Cloud verlinken — sie
    /// existiert ja gerade, um den Herkunftswechsel zu vermeiden. Ein Link
    /// dorthin (oder ein „Weiter"-Knopf) machte sie sinnlos.
    #[test]
    fn warteseite_verlinkt_nicht_in_die_cloud() {
        let html = warteseite_html("Das Spielfeld ist gleich wieder da.");
        assert!(
            !html.contains("badhub.de"),
            "die Warteseite darf keine Cloud-Adresse enthalten: {html}"
        );
        assert!(!html.contains("<a "), "kein Link, es gibt nichts zu wählen");
    }

    /// Sie muss sich selbst wieder wegräumen — sonst stünde das Tablet auch
    /// dann noch auf der Warteseite, wenn längst alles läuft, und jemand
    /// müsste durch die Halle laufen und jedes Gerät anfassen.
    #[test]
    fn warteseite_laedt_sich_selbst_neu() {
        let html = warteseite_html("test");
        assert!(
            html.contains("http-equiv=\"refresh\""),
            "ohne Selbst-Neuladen bliebe die Seite stehen: {html}"
        );
    }

    /// Der Versions-Ping muss **lokal** beantwortet werden.
    ///
    /// Reicht man ihn durch, antwortet der Relay mit SEINER Marke — und weil
    /// Relay und App aus verschiedenen Ständen stammen, weichen sie fast
    /// immer ab. Das Tablet lädt dann neu, bekommt dieselbe Seite mit
    /// derselben Marke und lädt wieder neu: ein Reload-Ringelspiel mitten im
    /// Turnier (Review-Fund 25.08.2026).
    #[test]
    fn versions_ping_wird_lokal_mit_eigener_marke_beantwortet() {
        let pong = pong_auf_ping(r#"{"type":"ping"}"#).expect("Ping muss lokal beantwortet werden");
        assert!(
            pong.contains(assets::seiten_marke()),
            "der Pong muss die Marke DIESER Binärdatei tragen: {pong}"
        );
    }

    /// Der Pong des Relays muss erkannt werden, damit er verworfen werden
    /// kann. Ließe man ihn durch, trüge er die Marke des **Relays** ans
    /// Gerät — und löste genau den Reload aus, den die lokale Antwort
    /// verhindern soll.
    #[test]
    fn pong_des_relays_wird_erkannt() {
        let relay_pong = serde_json::to_string(&relay_proto::ServerMsg::Pong {
            marke: "fremde-marke".into(),
        })
        .expect("serialisieren");
        assert!(ist_pong(&relay_pong));
        // Alles andere muss durchkommen — sonst verschluckt die Brücke
        // Match-Zuweisungen.
        assert!(!ist_pong(r#"{"type":"court_occupied"}"#));
        assert!(!ist_pong("kein json"));
    }

    /// Alles andere reist unverändert weiter — der Slave deutet keine
    /// Fachframes, er trägt sie nur.
    #[test]
    fn andere_frames_werden_nicht_abgefangen() {
        assert!(pong_auf_ping(r#"{"type":"score_update","scoreA":1,"scoreB":0}"#).is_none());
        assert!(pong_auf_ping(r#"{"type":"identify","courtId":1}"#).is_none());
        assert!(pong_auf_ping("kein json").is_none());
    }

    /// Sicherheitsgrenze des Durchreichers: Ein Pfad, der aus dem eigenen
    /// Namespace aufsteigt, darf nie zu einer URL werden — sonst wäre die
    /// Brücke ein offener Zugang zu **fremden Turnieren**.
    #[test]
    fn pfad_wachter_weist_aufstiege_ab() {
        assert!(pfad_ist_harmlos("/info/overview"));
        assert!(pfad_ist_harmlos("/ads/werbung-1.png"));
        assert!(pfad_ist_harmlos("/court/42/state"));

        assert!(!pfad_ist_harmlos(
            "/info/../../fremder-namespace/court/1/state"
        ));
        assert!(!pfad_ist_harmlos("/info/%2e%2e/%2e%2e/fremd"));
        assert!(
            !pfad_ist_harmlos("/info/%2E%2E/fremd"),
            "Groß-/Kleinschreibung"
        );
        assert!(!pfad_ist_harmlos("/info/%2f%2f/fremd"));
        assert!(!pfad_ist_harmlos("/info/./heimlich"));
        assert!(!pfad_ist_harmlos("info/ohne-schraegstrich"));
        assert!(!pfad_ist_harmlos("/info/..\\windows"));
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
