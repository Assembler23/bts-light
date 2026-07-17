//! Monitor- & Tablet-Brücke des Cloud-Ansage-Slaves (ferne Halle, ohne
//! Extra-Rechner).
//!
//! In der fernen Halle läuft kein LAN-Tablet-Server (der Slave sagt nur an,
//! siehe [`super::relay_client`]). Tilos Court-Monitor-Pis suchen aber per
//! Subnetz-Scan einen lokalen Server auf `:8088/health`, und die Crew möchte
//! die Tablets wie in der Master-Halle über `IP/felder` bedienen. Dieser
//! Mini-Server erfüllt beides und **leitet alles auf den Master-Relay um**:
//!
//! - `/health` → 200 (der Subnetz-Scan der Pis wird fündig).
//! - `/monitor[?device=…]` → 302 auf den **Cloud-Monitor** des Masters
//!   (`…/{ns}/monitor…`; der Relay löst Gerät→Feld auf).
//! - `/felder` → eine schlanke **Feld-Auswahlseite** (Felder DIESER Halle,
//!   aus der Relay-Feldliste), jedes Feld verlinkt auf die **Cloud-Tablet-
//!   Seite** des Masters.
//! - `/court/{id}` → 302 auf die Cloud-Tablet-Seite `…/{ns}/court/{id}`.
//!
//! So bedient die ferne Halle Tablets **und** TVs mit derselben lokalen
//! Slave-IP wie die Master-Halle — die Geräte hängen aber weiter direkt am
//! Master-Relay (Weg A / Direkt-Cloud), damit Ergebnisse ins Master-BTP
//! fließen. Der Slave schreibt selbst nichts.

use std::sync::Arc;

use axum::extract::{Path, RawQuery, State};
use axum::http::header;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use axum::{Json, Router};

use relay_proto::{html_escape, CourtBrief};

use super::server::TABLET_PORT;

/// HTTPS-Basis des Cloud-Relays (identisch zum Relay-Client).
const RELAY_HTTP: &str = "https://badhub.de/bts-relay";

/// Laufzeit-Konfiguration der Brücke: Master-Namespace (Ziel aller
/// Weiterleitungen) + eigene Halle (Filter der Feld-Auswahlseite).
struct BridgeConfig {
    master_namespace: String,
    /// Eigene Halle (BTP-Location); leer = keine Hallen-Einschränkung.
    hall: String,
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

/// Rendert die Feld-Auswahlseite: je Feld der eigenen Halle ein großer Knopf,
/// der auf die Cloud-Tablet-Seite des Masters zeigt. `hall` leer = alle Felder.
/// Reine Funktion (Felder werden vom Aufrufer geholt) → testbar.
fn felder_page_html(master_ns: &str, courts: &[CourtBrief], hall: &str) -> String {
    let mut buttons = String::new();
    for c in courts.iter().filter(|c| hall.is_empty() || c.hall == hall) {
        buttons.push_str(&format!(
            "<a class=\"feld\" href=\"{}\">{}</a>",
            html_escape(&court_redirect_url(master_ns, c.id)),
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

/// `/monitor[?device=…]` — 302 auf den Cloud-Monitor des Masters.
async fn monitor(
    State(cfg): State<Arc<BridgeConfig>>,
    RawQuery(query): RawQuery,
) -> impl IntoResponse {
    Redirect::to(&monitor_redirect_url(
        &cfg.master_namespace,
        query.as_deref(),
    ))
}

/// `/court/{id}` — 302 auf die Cloud-Tablet-Seite des Felds.
async fn court(State(cfg): State<Arc<BridgeConfig>>, Path(id): Path<i64>) -> impl IntoResponse {
    Redirect::to(&court_redirect_url(&cfg.master_namespace, id))
}

/// `/felder` — Feld-Auswahlseite (Felder dieser Halle → Cloud-Tablet-Seite).
async fn felder(State(cfg): State<Arc<BridgeConfig>>) -> impl IntoResponse {
    // Feldliste live aus dem Relay (vom Master gepusht). Leer bei Netz-/
    // Parse-Fehler → die Seite zeigt dann den Warte-Hinweis.
    let courts = super::relay_client::fetch_courts(&cfg.master_namespace).await;
    Html(felder_page_html(&cfg.master_namespace, &courts, &cfg.hall))
}

/// Startet die Slave-Brücke auf `0.0.0.0:8088`. `master_namespace` muss vorab
/// validiert sein (siehe `relay_client::valid_relay_namespace`). Im Slave-Modus
/// läuft KEIN LAN-Tablet-Server, daher ist der Port frei. `hall` filtert die
/// Feld-Auswahlseite (leer = alle Felder).
pub async fn run(master_namespace: String, hall: String) -> std::io::Result<()> {
    let cfg = Arc::new(BridgeConfig {
        master_namespace,
        hall,
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/monitor", get(monitor))
        .route("/felder", get(felder))
        .route("/court/{id}", get(court))
        .with_state(cfg);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", TABLET_PORT)).await?;
    tracing::info!(
        "Slave-Brücke lauscht auf 0.0.0.0:{TABLET_PORT} → Tablets & Monitore des Masters (Cloud)"
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
        let html = felder_page_html("ns-1", &courts, "WR");
        assert!(html.contains("https://badhub.de/bts-relay/ns-1/court/47"));
        assert!(html.contains("WR · 1"));
        // Fremde Halle nicht anbieten.
        assert!(!html.contains("court/37"));
        assert!(!html.contains("HM · 1"));
    }

    #[test]
    fn felder_page_without_hall_filter_shows_all() {
        let courts = vec![brief(47, "WR · 1", "WR"), brief(37, "HM · 1", "HM")];
        let html = felder_page_html("ns-1", &courts, "");
        assert!(html.contains("court/47"));
        assert!(html.contains("court/37"));
    }

    #[test]
    fn felder_page_empty_shows_hint() {
        let html = felder_page_html("ns-1", &[], "WR");
        assert!(html.contains("Noch keine Felder"));
    }
}
