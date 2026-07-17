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
    AdUpload, AnnounceState, CourtBrief, HostFrame, MonitorControl, MonitorDeviceInfo,
    MonitorUpload, RelayFrame, ResultBody,
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
    // CourtID → zuletzt ans Tablet gemeldete Match-ID. Verhindert, dass der
    // 2-s-Ticker unverändert dasselbe Match immer wieder pusht.
    let mut last_match: HashMap<i64, Option<i64>> = HashMap::new();
    // Zuletzt an den Relay gepushte Freitext-ID (B1a: Cloud-Ansage der fernen
    // Halle). Nur neue Items (id > last) werden geschickt.
    let mut last_freetext: u64 = 0;
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
                push_freetext(ctx, &tx, &mut last_freetext);
            }
            _ = monitor_ticker.tick() => {
                maybe_upload_monitor(ctx, install_id, &mut monitor_fp).await;
                // Feld-Liste fürs Cloud-Feldwechsel-Menü mitschicken (selten
                // veränderlich; erster Tick feuert sofort nach dem Verbinden).
                push_courts(ctx, &tx);
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
    // Monitor- UND Aufruf-Timer-Config: ändert der Operator die Schwellen,
    // muss der Upload neu feuern, damit der Relay sie kennt.
    let app = ctx.app_config();
    let mut s = format!("{:?}|{:?}", app.court_monitor, app.call_timer);
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
    let ct = ctx.app_config().call_timer;
    let upload = MonitorUpload {
        config: monitor::to_monitor_config(&cfg),
        tournament_name: ctx.tablet.tournament_name(),
        ads,
        call_timer: relay_proto::CallTimerView {
            enabled: ct.enabled,
            second_call_minutes: ct.second_call_minutes,
            third_call_minutes: ct.third_call_minutes,
        },
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
    // Cloud-Pfad transportiert weiterhin nur Feld-Zuweisungen (CourtID).
    // Info-Monitor-Zuweisungen (`InfoOverview`/`InfoPreparation`) sind heute
    // LAN-only — sie werden hier verworfen (Cloud-Pis bleiben dann
    // unzugewiesen). TODO: Cloud-Wire-Protokoll für Info-Targets ausbauen.
    let assignments: std::collections::HashMap<String, i64> =
        monitor::read_assignments(&ctx.assignments_path)
            .into_iter()
            .filter_map(|(k, t)| t.court_id().map(|c| (k, c)))
            .collect();
    let control = MonitorControl {
        assignments,
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
    last_match: &mut HashMap<i64, Option<i64>>,
) {
    match frame {
        RelayFrame::TabletConnected { court_id, .. } => {
            ctx.tablet.attach_tablet(court_id);
            tracing::info!("Tablet verbunden für Feld {court_id} (Cloud)");
            // Sofort das aktuelle Match nachschieben (statt 2 s zu warten).
            last_match.remove(&court_id);
            push_court(ctx, court_id, tx, last_match);
        }
        RelayFrame::TabletDisconnected { court_id, .. } => {
            ctx.tablet.detach_tablet(court_id);
            tracing::info!("Tablet getrennt für Feld {court_id} (Cloud)");
            // `last_match` bewusst NICHT entfernen – sonst pusht der nächste
            // Ticker ein unnötiges `MatchAssigned`. Ein Reconnect setzt es
            // ohnehin zurück und schiebt das Match dann frisch nach.
        }
        RelayFrame::ScoreUpdate {
            court_id,
            score_a,
            score_b,
            sets_history,
            ..
        } => {
            handle_score(court_id, score_a, score_b, &sets_history, ctx).await;
        }
        RelayFrame::Battery {
            court_id,
            percent,
            charging,
            ..
        } => {
            ctx.tablet.record_battery(court_id, percent, charging);
        }
        RelayFrame::Alert {
            court_id,
            injury,
            official,
            ..
        } => {
            ctx.tablet.record_alert(court_id, injury, official);
        }
        RelayFrame::Result {
            req_id,
            court_id,
            court_label,
            match_id,
            sets,
            retired,
            walkover,
            winner,
            cascade_walkover,
        } => {
            let body = ResultBody {
                match_id,
                court_id,
                court_label,
                sets,
                retired,
                walkover,
                winner,
                cascade_walkover,
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

/// Schiebt das aktuelle Match eines Felds (per CourtID) ans Tablet – nur,
/// wenn es sich gegenüber dem zuletzt gemeldeten Stand geändert hat.
/// `court_label` ist die Anzeige-Bezeichnung des Felds (bei Mehr-Hallen-
/// Turnieren mit Hallen-Präfix, z. B. „Halle 2 · 6").
fn push_court(
    ctx: &ServerCtx,
    court_id: i64,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_match: &mut HashMap<i64, Option<i64>>,
) {
    let court_label = ctx.tablet.court_display_label(court_id);
    let hall = ctx.tablet.court_hall(court_id);
    let current = ctx.tablet.match_for_court(court_id);
    let current_id = current.as_ref().map(|m| m.id);
    if last_match.get(&court_id) == Some(&current_id) {
        return;
    }
    last_match.insert(court_id, current_id);
    let frame = match current {
        Some(m) => {
            tracing::info!(
                "Feld {court_id}: Match {} ans Tablet zugewiesen (Cloud)",
                m.id
            );
            HostFrame::MatchAssigned {
                court_id,
                court_label,
                hall,
                match_brief: match_brief(&m, ctx.tablet.scorekeeper(court_id)),
                // Autoritativer 1.-Aufruf-Zeitstempel vom Host (gleiche Quelle
                // wie die Spielübersicht) – auch bei Reconnect identisch.
                on_court_since_ms: ctx.tablet.on_court_since_ms(court_id, m.id),
            }
        }
        None => {
            tracing::info!("Feld {court_id}: Match-Zuweisung aufgehoben (Cloud)");
            HostFrame::MatchCleared {
                court_id,
                court_label,
                hall,
            }
        }
    };
    let _ = tx.send(text(&frame));
}

/// 2-s-Ticker: prüft jedes Feld auf eine geänderte Match-Zuweisung.
fn push_all_courts(
    ctx: &ServerCtx,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_match: &mut HashMap<i64, Option<i64>>,
) {
    for court in ctx.tablet.courts() {
        push_court(ctx, court.id, tx, last_match);
    }
}

/// Cloud-Ansage-Slave (B1a): holt den Ansage-Status (hallengefilterte
/// Court-Matches + neue Freitexte) aus dem Cloud-Relay des Masters. `namespace`
/// = Kopplungs-Code des Masters, `hall` = eigene Halle (leer = alle),
/// `since` = letzte gesehene Freitext-ID. `None` bei Netz-/Parse-Fehler.
pub async fn fetch_announce_state(
    namespace: &str,
    hall: &str,
    since: u64,
    slave: &str,
) -> Option<AnnounceState> {
    // Kopplungs-Code (= install_id-UUID) hart validieren: nur Hex+Bindestrich,
    // plausible Länge. Schützt den URL-Pfad vor Fremdzeichen und erspart sinnlose
    // Requests bei Tippfehlern.
    if namespace.len() < 8
        || namespace.len() > 64
        || !namespace
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
    {
        return None;
    }
    let mut url =
        reqwest::Url::parse(&format!("{RELAY_HTTP}/{namespace}/info/announce/state")).ok()?;
    url.query_pairs_mut()
        .append_pair("hall", hall)
        .append_pair("since", &since.to_string());
    if !slave.is_empty() {
        url.query_pairs_mut().append_pair("slave", slave);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?;
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<AnnounceState>().await.ok()
}

/// Master: bekannte Cloud-Ansage-Slaves des eigenen Namespaces abfragen (für die
/// „ferne Halle online?"-Anzeige). Leere Liste bei Netz-/Parse-Fehler.
pub async fn fetch_slaves(namespace: &str) -> Vec<relay_proto::SlaveInfo> {
    if namespace.len() < 8
        || namespace.len() > 64
        || !namespace
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
    {
        return Vec::new();
    }
    let url = format!("{RELAY_HTTP}/{namespace}/slaves");
    let fetch = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .ok()?;
        let resp = client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<Vec<relay_proto::SlaveInfo>>().await.ok()
    };
    fetch.await.unwrap_or_default()
}

/// Prüft einen vom Nutzer eingegebenen Master-Kopplungs-Code (Relay-Namespace),
/// bevor er in eine URL fließt: nur Hex + Bindestrich, plausible Länge. `.`/`/`
/// sind damit ausgeschlossen → kein Pfad-Confusion (`../`) beim URL-Bau, keine
/// sinnlosen Requests bei Tippfehlern. Bewusst großzügiger als die strikte
/// 36-Zeichen-UUID-Prüfung des Relays (der Server weist Abweichungen ohnehin ab).
pub fn valid_relay_namespace(namespace: &str) -> bool {
    namespace.len() >= 8
        && namespace.len() <= 64
        && namespace
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// Slave: die vollständige Feld-Liste des Master-Namespace holen (`/{ns}/courts`,
/// vom Master via `HostFrame::Courts` gepusht – inkl. roher Halle je Feld). Der
/// Cloud-Ansage-Slave filtert daraus die Felder **seiner** Halle und zeigt deren
/// Tablet-QR-/Monitor-Links (Geräte-Anschluss ferne Halle). Leer bei Netz-/
/// Parse-Fehler oder solange der Master noch nichts gepusht hat.
pub async fn fetch_courts(namespace: &str) -> Vec<CourtBrief> {
    if !valid_relay_namespace(namespace) {
        return Vec::new();
    }
    let url = format!("{RELAY_HTTP}/{namespace}/courts");
    let fetch = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .ok()?;
        let resp = client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<Vec<CourtBrief>>().await.ok()
    };
    fetch.await.unwrap_or_default()
}

/// 2-s-Ticker: neue Freitext-Ansagen (`id > last_freetext`) an den Relay pushen,
/// damit der Cloud-Ansage-Slave der fernen Halle sie abholen kann (B1a).
fn push_freetext(ctx: &ServerCtx, tx: &mpsc::UnboundedSender<WsMessage>, last_freetext: &mut u64) {
    // hall="" → alle Hallen; der Relay/Slave filtert selbst nach Ziel-Halle.
    for item in ctx.tablet.freetext_since("", *last_freetext) {
        *last_freetext = (*last_freetext).max(item.id);
        let _ = tx.send(text(&HostFrame::Freetext {
            id: item.id,
            hall: item.hall,
            text: item.text,
        }));
    }
}

/// Sendet die vollständige Feld-Liste an den Relay – Grundlage des Feldwechsels
/// im PIN-Menü des Tablets im Cloud-Modus (LAN baut `/courts` direkt). Selten
/// veränderlich; periodisch (alle 30 s) genügt.
fn push_courts(ctx: &ServerCtx, tx: &mpsc::UnboundedSender<WsMessage>) {
    let courts: Vec<CourtBrief> = ctx
        .tablet
        .courts()
        .into_iter()
        .map(|c| CourtBrief {
            id: c.id,
            label: ctx.tablet.court_display_label(c.id),
            // Rohe Halle (BTP-Location) mitschicken – der Cloud-Ansage-Slave
            // filtert damit auf die Felder seiner Halle (Geräte-Anschluss).
            hall: ctx.tablet.court_hall(c.id),
        })
        .collect();
    if !courts.is_empty() {
        let _ = tx.send(text(&HostFrame::Courts { courts }));
    }
}

/// Serialisiert einen Wert zu einem WebSocket-Text-Frame.
fn text<T: serde::Serialize>(value: &T) -> WsMessage {
    WsMessage::Text(serde_json::to_string(value).unwrap_or_default().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{
        BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult, MatchStatus, ScoringFormat,
    };
    use crate::config::AppConfig;
    use crate::tablet::state::TabletState;
    use std::collections::HashMap;

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

    fn match_on_court(id: i64, court_id: i64) -> BtpMatch {
        BtpMatch {
            id,
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
            court_id: Some(court_id),
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

    fn snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches,
            courts: vec!["1".into()],
            locations: vec![],
            court_infos: vec![],
        }
    }

    fn ctx_with(matches: Vec<BtpMatch>) -> ServerCtx {
        let tablet = Arc::new(TabletState::default());
        tablet.set_snapshot(snapshot(matches));
        let tmp = std::env::temp_dir();
        ServerCtx::new(
            tablet,
            AppConfig::default(),
            reqwest::Client::new(),
            tmp.clone(),
            tmp.join("bts_rc_config.json"),
            tmp.join("bts_rc_assign.json"),
            tmp,
        )
    }

    fn text_of(msg: &WsMessage) -> String {
        match msg {
            WsMessage::Text(t) => t.to_string(),
            _ => String::new(),
        }
    }

    /// Cloud-Feld-Diffing: erster Push meldet die Zuweisung, ein unveränderter
    /// Stand wird dedupliziert (kein doppelter Push → kein Tablet-Reset), und
    /// verlässt das Match das Feld, kommt genau eine Aufhebung.
    #[test]
    fn push_court_sends_once_dedups_then_clears() {
        let ctx = ctx_with(vec![match_on_court(42, 101)]);
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        let mut last: HashMap<i64, Option<i64>> = HashMap::new();

        // 1) Zuweisung → genau ein MatchAssigned.
        push_court(&ctx, 101, &tx, &mut last);
        let f1 = rx.try_recv().expect("ein Frame erwartet");
        assert!(
            text_of(&f1).contains("\"type\":\"match_assigned\""),
            "erwartet match_assigned, war: {}",
            text_of(&f1)
        );

        // 2) Unveränderter Stand → kein erneuter Push (Dedup).
        push_court(&ctx, 101, &tx, &mut last);
        assert!(
            rx.try_recv().is_err(),
            "kein doppelter Push bei gleichem Stand"
        );

        // 3) Match verlässt das Feld → genau ein MatchCleared.
        ctx.tablet.set_snapshot(snapshot(vec![]));
        push_court(&ctx, 101, &tx, &mut last);
        let f3 = rx.try_recv().expect("Clear-Frame erwartet");
        assert!(
            text_of(&f3).contains("\"type\":\"match_cleared\""),
            "erwartet match_cleared, war: {}",
            text_of(&f3)
        );
    }
}
