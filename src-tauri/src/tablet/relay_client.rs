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

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use relay_proto::{
    AdUpload, HostFrame, MonitorControl, MonitorDeviceInfo, MonitorUpload, RelayFrame, ResultBody,
};

use crate::tablet::monitor;
use crate::tablet::server::{handle_score, match_brief, process_result, ServerCtx};

/// Öffentliche Relay-Basis – der Host-Pfad hängt die `install_id` an.
const RELAY_HOST: &str = "wss://badhub.de/bts-relay";

/// HTTPS-Basis des Relays – für den Court-Monitor-Werbe-Upload.
const RELAY_HTTP: &str = "https://badhub.de/bts-relay";

/// Abstand der Match-Push-Ticks (Court → Tablet-Zuweisung).
const TICK: Duration = Duration::from_secs(2);

/// Abstand der Court-Monitor-Upload-Prüfung (Werbung/Konfiguration).
const MONITOR_TICK: Duration = Duration::from_secs(30);

/// Abstand des Geräte-Steuerungs-Abgleichs (Feld-Zuweisungen, Fernbefehle,
/// Geräteliste) – kurz, damit Befehle zügig am Monitor ankommen.
const CONTROL_TICK: Duration = Duration::from_secs(3);

/// Obergrenze der Werbebilder bzw. ihrer Gesamtgröße beim Upload zum Relay.
const MAX_UPLOAD_ADS: usize = 24;
const MAX_UPLOAD_TOTAL: usize = 12 * 1024 * 1024;

/// Verbindet bts-light dauerhaft zum Cloud-Relay – mit Reconnect-Backoff
/// (1 s → 30 s). Läuft, bis der Task abgebrochen wird (`stop_sync`).
pub async fn run(ctx: Arc<ServerCtx>, install_id: String) {
    let url = format!("{RELAY_HOST}/{install_id}/host-ws");
    let mut backoff = 1u64;
    loop {
        if let Err(e) = serve(&ctx, &url, &install_id, &mut backoff).await {
            tracing::warn!("Relay-Verbindung beendet: {e}");
        }
        tracing::info!("Relay-Reconnect in {backoff}s");
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

/// Eine Relay-Sitzung: verbinden, Frames austauschen, bis die Verbindung
/// endet. `backoff` wird bei erfolgreichem Verbindungsaufbau zurückgesetzt.
async fn serve(
    ctx: &Arc<ServerCtx>,
    url: &str,
    install_id: &str,
    backoff: &mut u64,
) -> Result<(), String> {
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
    // Court-Monitor-Upload: erster Tick feuert sofort → Werbung/Konfig
    // direkt nach dem Verbinden hochladen, danach nur bei Änderung.
    let mut monitor_ticker = tokio::time::interval(MONITOR_TICK);
    monitor_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut monitor_fp: Option<String> = None;
    // Geräte-Steuerung: Feld-Zuweisungen/Befehle pushen, Geräteliste holen.
    let mut control_ticker = tokio::time::interval(CONTROL_TICK);
    control_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut control_fp = String::new();

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
            _ = monitor_ticker.tick() => {
                maybe_upload_monitor(ctx, install_id, &mut monitor_fp).await;
            }
            _ = control_ticker.tick() => {
                sync_monitor_control(ctx, install_id, &mut control_fp).await;
            }
        }
    }
    Ok(())
}

/// Lädt den Court-Monitor-Datensatz (Werbung + Anzeige-Konfiguration) zum
/// Relay hoch, falls er sich seit dem letzten erfolgreichen Upload geändert
/// hat. Ein Fingerabdruck (Konfiguration + Werbebild-Namen/Größen/Zeiten)
/// erspart unnötige Uploads der Bilddaten.
async fn maybe_upload_monitor(ctx: &ServerCtx, install_id: &str, last_fp: &mut Option<String>) {
    let fp = monitor_fingerprint(ctx);
    if last_fp.as_deref() == Some(fp.as_str()) {
        return;
    }
    match upload_monitor(ctx, install_id).await {
        Ok(()) => {
            tracing::info!("Court-Monitor-Datensatz zum Relay hochgeladen");
            *last_fp = Some(fp);
        }
        Err(e) => tracing::warn!("Court-Monitor-Upload fehlgeschlagen: {e}"),
    }
}

/// Fingerabdruck der Court-Monitor-Daten – ändert sich, sobald die
/// Konfiguration oder ein Werbebild (Name, Größe, Änderungszeit) wechselt.
fn monitor_fingerprint(ctx: &ServerCtx) -> String {
    let mut s = format!("{:?}", ctx.monitor_config());
    for name in monitor::list_ads(&ctx.monitor_dir) {
        let (len, mtime) = std::fs::metadata(ctx.monitor_dir.join(&name))
            .map(|m| {
                let mt = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (m.len(), mt)
            })
            .unwrap_or((0, 0));
        s.push_str(&format!("|{name}:{len}:{mtime}"));
    }
    s
}

/// Baut den Court-Monitor-Datensatz und POSTet ihn zum Relay.
async fn upload_monitor(ctx: &ServerCtx, install_id: &str) -> Result<(), String> {
    let cfg = ctx.monitor_config();
    let mut ads = Vec::new();
    let mut total = 0usize;
    for name in monitor::list_ads(&ctx.monitor_dir)
        .into_iter()
        .take(MAX_UPLOAD_ADS)
    {
        let Ok(bytes) = std::fs::read(ctx.monitor_dir.join(&name)) else {
            continue;
        };
        total += bytes.len();
        if total > MAX_UPLOAD_TOTAL {
            break;
        }
        ads.push(AdUpload {
            content_type: monitor::image_mime(&name).to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
    }
    let upload = MonitorUpload {
        config: monitor::to_monitor_config(&cfg),
        tournament_name: ctx.tablet.tournament_name(),
        ads,
    };
    let url = format!("{RELAY_HTTP}/{install_id}/monitor");
    let resp = ctx
        .http
        .post(&url)
        .json(&upload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

/// Gleicht die Monitor-Geräte-Steuerung mit dem Relay ab: pusht die
/// Feld-Zuweisungen + Fernbefehle (nur bei Änderung) und holt die
/// aktuelle Geräteliste für die „Court-Monitore"-Seite.
async fn sync_monitor_control(ctx: &ServerCtx, install_id: &str, last_fp: &mut String) {
    let control = MonitorControl {
        assignments: monitor::read_assignments(&ctx.assignments_path),
        commands: ctx.tablet.monitor_commands(),
    };
    let fp = serde_json::to_string(&control).unwrap_or_default();
    if fp != *last_fp {
        let url = format!("{RELAY_HTTP}/{install_id}/monitor/control");
        match ctx.http.post(&url).json(&control).send().await {
            Ok(r) if r.status().is_success() => *last_fp = fp,
            Ok(r) => tracing::warn!("Monitor-Steuerung: HTTP {}", r.status()),
            Err(e) => tracing::warn!("Monitor-Steuerung fehlgeschlagen: {e}"),
        }
    }
    // Geräteliste vom Relay holen und im geteilten Zustand ablegen.
    let url = format!("{RELAY_HTTP}/{install_id}/monitor-devices");
    if let Ok(resp) = ctx.http.get(&url).send().await {
        if let Ok(devices) = resp.json::<Vec<MonitorDeviceInfo>>().await {
            ctx.tablet.set_relay_monitor_devices(devices);
        }
    }
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
            // `last_match` bewusst NICHT entfernen – sonst pusht der nächste
            // Ticker ein unnötiges `MatchAssigned`. Ein Reconnect setzt es
            // ohnehin zurück und schiebt das Match dann frisch nach.
        }
        RelayFrame::ScoreUpdate {
            court_label,
            score_a,
            score_b,
            sets_history,
        } => {
            handle_score(&court_label, score_a, score_b, &sets_history, ctx).await;
        }
        RelayFrame::Battery {
            court_label,
            percent,
            charging,
        } => {
            ctx.tablet.record_battery(&court_label, percent, charging);
        }
        RelayFrame::Alert {
            court_label,
            injury,
            official,
        } => {
            ctx.tablet.record_alert(&court_label, injury, official);
        }
        RelayFrame::Result {
            req_id,
            court_label,
            match_id,
            sets,
            retired,
            winner,
        } => {
            let body = ResultBody {
                match_id,
                court_label,
                sets,
                retired,
                winner,
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
