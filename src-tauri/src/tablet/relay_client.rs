//! Relay-Client: bts-light im Cloud-Modus.
//!
//! Statt selbst einen Server zu betreiben (LAN-Modus, [`super::server`]),
//! verbindet sich bts-light hier **ausgehend** zum Cloud-Relay auf
//! badhub.de. Eine ausgehende Verbindung lässt jede Firmen-Firewall durch –
//! damit erreichen die Tablets bts-light auch auf gesperrten Turnier-PCs.
//!
//! Der Relay multiplext alle Tablets über diese eine Verbindung. bts-light
//! ist der „Host" seines Namespace (= `install_id`). Der BTP-Schreibweg
//! bleibt lokal: ein eingehendes Ergebnis wird mit derselben
//! [`process_result`]-Logik wie im LAN-Modus nach BTP geschrieben.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use relay_proto::{HostFrame, RelayFrame, ResultBody};

use crate::tablet::server::{handle_score, match_brief, process_result, ServerCtx};

/// Öffentliche Relay-Basis – der Host-Pfad hängt die `install_id` an.
const RELAY_HOST: &str = "wss://badhub.de/bts-relay";

/// Abstand der Match-Push-Ticks (Court → Tablet-Zuweisung).
const TICK: Duration = Duration::from_secs(2);

/// Verbindet bts-light dauerhaft zum Cloud-Relay – mit Reconnect-Backoff
/// (1 s → 30 s). Läuft, bis der Task abgebrochen wird (`stop_sync`).
pub async fn run(ctx: Arc<ServerCtx>, install_id: String) {
    let url = format!("{RELAY_HOST}/{install_id}/host-ws");
    let mut backoff = 1u64;
    loop {
        if let Err(e) = serve(&ctx, &url, &mut backoff).await {
            tracing::warn!("Relay-Verbindung beendet: {e}");
        }
        tracing::info!("Relay-Reconnect in {backoff}s");
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

/// Eine Relay-Sitzung: verbinden, Frames austauschen, bis die Verbindung
/// endet. `backoff` wird bei erfolgreichem Verbindungsaufbau zurückgesetzt.
async fn serve(ctx: &Arc<ServerCtx>, url: &str, backoff: &mut u64) -> Result<(), String> {
    let (stream, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| e.to_string())?;
    *backoff = 1;
    tracing::info!("Mit Cloud-Relay verbunden");

    let (mut sink, mut read) = stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
    // Court → zuletzt ans Tablet gemeldete Match-ID. Verhindert, dass der
    // 2-s-Ticker unverändert dasselbe Match immer wieder pusht.
    let mut last_match: HashMap<String, Option<i64>> = HashMap::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            incoming = read.next() => {
                let Some(msg) = incoming else { break };
                let msg = msg.map_err(|e| e.to_string())?;
                match msg {
                    WsMessage::Text(t) => {
                        if let Ok(frame) = serde_json::from_str::<RelayFrame>(t.as_str()) {
                            handle_frame(ctx, frame, &tx, &mut last_match).await;
                        }
                    }
                    WsMessage::Ping(p) => { let _ = tx.send(WsMessage::Pong(p)); }
                    WsMessage::Close(_) => break,
                    _ => {}
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(m) => sink.send(m).await.map_err(|e| e.to_string())?,
                    None => break,
                }
            }
            _ = ticker.tick() => {
                push_all_courts(ctx, &tx, &mut last_match);
            }
        }
    }
    Ok(())
}

/// Verarbeitet ein Frame vom Relay.
async fn handle_frame(
    ctx: &Arc<ServerCtx>,
    frame: RelayFrame,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_match: &mut HashMap<String, Option<i64>>,
) {
    match frame {
        RelayFrame::TabletConnected { court_label } => {
            ctx.tablet.attach_tablet(&court_label);
            tracing::info!("Tablet verbunden für Court '{court_label}' (Cloud)");
            // Sofort das aktuelle Match nachschieben (statt 2 s zu warten).
            last_match.remove(&court_label);
            push_court(ctx, &court_label, tx, last_match);
        }
        RelayFrame::TabletDisconnected { court_label } => {
            ctx.tablet.detach_tablet(&court_label);
            tracing::info!("Tablet getrennt für Court '{court_label}' (Cloud)");
            last_match.remove(&court_label);
        }
        RelayFrame::ScoreUpdate {
            court_label,
            score_a,
            score_b,
            sets_history,
        } => {
            handle_score(&court_label, score_a, score_b, &sets_history, ctx).await;
        }
        RelayFrame::Result {
            req_id,
            court_label,
            match_id,
            sets,
        } => {
            let body = ResultBody {
                match_id,
                court_label,
                sets,
            };
            let resp = process_result(ctx, &body).await;
            let _ = tx.send(text(&HostFrame::ResultAck {
                req_id,
                ok: resp.ok,
                error: resp.error,
            }));
        }
    }
}

/// Schiebt das aktuelle Match eines Courts ans Tablet – nur, wenn es sich
/// gegenüber dem zuletzt gemeldeten Stand geändert hat.
fn push_court(
    ctx: &ServerCtx,
    court: &str,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_match: &mut HashMap<String, Option<i64>>,
) {
    let current = ctx.tablet.match_for_court(court);
    let current_id = current.as_ref().map(|m| m.id);
    if last_match.get(court) == Some(&current_id) {
        return;
    }
    last_match.insert(court.to_string(), current_id);
    let frame = match current {
        Some(m) => {
            tracing::info!(
                "Court '{court}': Match {} ans Tablet zugewiesen (Cloud)",
                m.id
            );
            HostFrame::MatchAssigned {
                court_label: court.to_string(),
                match_brief: match_brief(&m),
            }
        }
        None => {
            tracing::info!("Court '{court}': Match-Zuweisung aufgehoben (Cloud)");
            HostFrame::MatchCleared {
                court_label: court.to_string(),
            }
        }
    };
    let _ = tx.send(text(&frame));
}

/// 2-s-Ticker: prüft jeden Court auf eine geänderte Match-Zuweisung.
fn push_all_courts(
    ctx: &ServerCtx,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_match: &mut HashMap<String, Option<i64>>,
) {
    for court in ctx.tablet.court_names() {
        push_court(ctx, &court, tx, last_match);
    }
}

/// Serialisiert einen Wert zu einem WebSocket-Text-Frame.
fn text<T: serde::Serialize>(value: &T) -> WsMessage {
    WsMessage::Text(serde_json::to_string(value).unwrap_or_default().into())
}
