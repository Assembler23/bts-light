//! Monitor-Brücke des Cloud-Ansage-Slaves (ferne Halle, ohne Extra-Rechner).
//!
//! In der fernen Halle läuft kein LAN-Tablet-Server (der Slave sagt nur an,
//! siehe [`super::relay_client`]). Tilos Court-Monitor-Pis suchen aber per
//! Subnetz-Scan einen lokalen Server auf `:8088/health` und laden dann
//! dessen `/monitor`. Dieser Mini-Server erfüllt genau das: Er antwortet auf
//! `/health` und **leitet `/monitor` auf den Cloud-Monitor des Masters um**
//! (`https://badhub.de/bts-relay/<master_ns>/monitor…`). Der Master-Relay löst
//! wie gewohnt Gerät→Feld auf. So finden die Pis den Slave lokal und zeigen
//! den Court-Monitor — ohne separate Brücken-Hardware.
//!
//! Bewusst NUR Monitore: Tablets hängen weiter direkt am Master-Relay
//! (Weg A / Direkt-Cloud), weil ihre Ergebnisse ins Master-BTP müssen.

use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::header;
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::{Json, Router};

use super::server::TABLET_PORT;

/// HTTPS-Basis des Cloud-Relays (identisch zum Relay-Client).
const RELAY_HTTP: &str = "https://badhub.de/bts-relay";

/// Baut die Weiterleitungs-Ziel-URL auf den Cloud-Monitor des Masters. Der
/// rohe Query-String (inkl. `device=…`) wird 1:1 angehängt, damit der
/// Master-Relay das Gerät seinem Feld zuordnen kann. Reine Funktion → testbar.
fn monitor_redirect_url(master_ns: &str, raw_query: Option<&str>) -> String {
    let base = format!("{RELAY_HTTP}/{master_ns}/monitor");
    match raw_query {
        Some(q) if !q.is_empty() => format!("{base}?{q}"),
        _ => base,
    }
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
    State(master_ns): State<Arc<String>>,
    RawQuery(query): RawQuery,
) -> impl IntoResponse {
    Redirect::to(&monitor_redirect_url(&master_ns, query.as_deref()))
}

/// Startet die Monitor-Brücke auf `0.0.0.0:8088`. `master_namespace` muss
/// vorab validiert sein (siehe `relay_client::valid_relay_namespace`). Im
/// Slave-Modus läuft KEIN LAN-Tablet-Server, daher ist der Port frei.
pub async fn run(master_namespace: String) -> std::io::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/monitor", get(monitor))
        .with_state(Arc::new(master_namespace));
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", TABLET_PORT)).await?;
    tracing::info!(
        "Slave-Monitor-Brücke lauscht auf 0.0.0.0:{TABLET_PORT} → Cloud-Monitor des Masters"
    );
    axum::serve(listener, app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_url_appends_device_query() {
        assert_eq!(
            monitor_redirect_url("abc-123", Some("device=pi-0000000070a061db")),
            "https://badhub.de/bts-relay/abc-123/monitor?device=pi-0000000070a061db"
        );
    }

    #[test]
    fn redirect_url_without_query() {
        assert_eq!(
            monitor_redirect_url("abc-123", None),
            "https://badhub.de/bts-relay/abc-123/monitor"
        );
        // Leerer Query zählt wie keiner.
        assert_eq!(
            monitor_redirect_url("abc-123", Some("")),
            "https://badhub.de/bts-relay/abc-123/monitor"
        );
    }
}
