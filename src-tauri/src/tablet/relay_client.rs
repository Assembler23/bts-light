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
    MonitorUpload, PlayerBrief, PreparedMatch, RelayFrame, ResultBody, SetAb,
};

use crate::tablet::monitor;
use crate::tablet::server::{handle_score, match_brief, process_result, ServerCtx};
use crate::tablet::state::{MonitorNudge, MonitorNudgeTx, TabletState};

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

/// Read-Idle-Schwelle (Hebel D / ADR 0020, Option A): Bleibt jedes
/// Lebenszeichen des Relays (Frame **oder** Relay-Ping) länger aus, gilt die
/// Verbindung als half-open tot → `serve` gibt `Err` zurück, `run`
/// reconnectet (frischer Socket, Backoff-Reset). **Kein eigener Client-Ping.**
///
/// **Kopplungs-Vertrag:** Diese Schwelle setzt voraus, dass der Relay
/// mindestens alle ~5 s pingt (`HOST_PING` der `relay`-Crate). Relay und App
/// liegen im selben Repo und werden koordiniert deployt — eine künftige
/// `HOST_PING`-Änderung ist eine bewusst abzustimmende Änderung.
const RELAY_READ_IDLE: Duration = Duration::from_secs(15);

/// Reine Stale-Entscheidung: liegt der letzte Empfang mindestens `threshold`
/// zurück? Ausgelagert, damit die Grenz-Semantik (`>=`) ohne Laufzeit/Clock
/// prüfbar ist. `tokio::time::Instant`, damit `tokio::time::pause()` in Tests
/// griffe (konsistent mit dem Read-Idle-Ticker).
fn is_stale(last: tokio::time::Instant, now: tokio::time::Instant, threshold: Duration) -> bool {
    now.duration_since(last) >= threshold
}

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
    let mut last_match: HashMap<i64, Option<(i64, bool)>> = HashMap::new();
    // Zuletzt an den Relay gepushte Freitext-ID (B1a: Cloud-Ansage der fernen
    // Halle). Nur neue Items (id > last) werden geschickt.
    let mut last_freetext: u64 = 0;
    // Turnierleitungs-Zugänge und -Zustand. Beide beginnen bei „noch nichts
    // gesendet", damit der Relay nach einem Reconnect sofort wieder weiß, wer
    // zugelassen ist und was anzuzeigen wäre — er vergisst beides, sobald die
    // Host-Verbindung abreißt.
    let mut tl_auth_fp: Option<String> = None;
    let mut tl_state_rev: u64 = 0;
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Court-Monitor-Upload: erster Tick feuert sofort → Werbung/Konfig
    // direkt nach dem Verbinden hochladen, danach nur bei Änderung.
    let mut monitor_ticker = tokio::time::interval(MONITOR_TICK);
    monitor_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut monitor_fp: Option<String> = None;
    // Fingerabdruck der zuletzt gesendeten „aufgerufene Spiele"-Liste
    // (Cluster C Stufe 2) — `None` erzwingt das erste Senden nach dem
    // Verbinden (auch für die Wiederbefüllung nach einem Reconnect).
    let mut prepared_fp: Option<String> = None;
    // Geräte-Steuerung: Feld-Zuweisungen/Befehle pushen, Geräteliste holen.
    let mut control_ticker = tokio::time::interval(CONTROL_TICK);
    control_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut control_fp = String::new();
    // Read-Idle-Wächter (Hebel D): erkennt eine half-open Host-Verbindung an
    // ausbleibendem Empfang. Der Ticker feuert dichter als die Schwelle
    // (~5 s bei RELAY_READ_IDLE = 15 s), damit die Stille spätestens ~15 s
    // nach dem letzten Frame/Ping auffällt.
    let mut last_incoming = tokio::time::Instant::now();
    let mut idle_ticker = tokio::time::interval(RELAY_READ_IDLE / 3);
    idle_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Score-Spiegel (Turnier-Befund 13.08.2026): Der Host abonniert seinen
    // EIGENEN Monitor-Nudge-Kanal („alle Felder", wie die LAN-Übersicht) und
    // spiegelt bei jedem Signal den Stand des Felds zum Relay — sonst bleiben
    // Cloud-Monitor/-Übersicht auf 0:0, sobald die Tablets im LAN zählen.
    // Der Guard trägt das Abo bei JEDEM Sitzungsende wieder aus (auch beim
    // Read-Idle-`Err`), damit Reconnects den Fan-out-Deckel nicht auffüllen.
    let (nudge_tx, mut nudge_rx) = mpsc::unbounded_channel::<String>();
    if !ctx.tablet.subscribe_monitor(None, &nudge_tx) {
        tracing::warn!(
            "Score-Spiegel: Nudge-Deckel erreicht — Cloud-Anzeigen folgen nur dem Match-Wechsel"
        );
    }
    let _nudge_guard = NudgeGuard {
        tablet: &ctx.tablet,
        tx: nudge_tx,
    };
    // Kein Spiegel VOR dem ersten Zuweisungs-Push: Der Relay verwirft einen
    // Spiegel für ein ihm unbekanntes Match (Stale-Schutz) — nach seinem
    // Neustart wäre der Stand dann bis zum nächsten Punkt unsichtbar
    // (Review-Befund). Der erste `ticker`-Tick feuert sofort und schickt
    // MatchAssigned + Spiegel in der richtigen Reihenfolge.
    let mut score_fp: HashMap<i64, HostFrame> = HashMap::new();

    loop {
        tokio::select! {
            incoming = read.next() => {
                let Some(msg) = incoming else { break };
                let msg = msg.map_err(|e| e.to_string())?;
                // Lebenszeichen des Relays: deckt Text-Frame + Ping (+ alle
                // übrigen Frames) ab — der Read-Idle-Wächter braucht keinen
                // eigenen Client-Ping (Option A).
                last_incoming = tokio::time::Instant::now();
                match msg {
                    WsMessage::Text(t) => {
                        if let Ok(frame) = serde_json::from_str::<RelayFrame>(t.as_str()) {
                            handle_frame(ctx, frame, &tx, &mut last_match, &mut score_fp).await;
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
                push_all_courts(ctx, &tx, &mut last_match, &mut score_fp);
                // Score-Spiegel-Sweep NACH den Zuweisungen (gleiche FIFO-Wire
                // → der Relay kennt die Matches, bevor der Spiegel eintrifft).
                // Fängt drei nudge-lose Fälle mit ≤2 s Verzug ein: Reconnect/
                // Relay-Neustart (leerer Cache dort), Court-Wechsel (FP eben
                // invalidiert) und BTP-getriebene Stände ohne Tablet
                // (Handeingabe in BTP nudgt nicht). Dank Fingerabdruck sendet
                // ein Tick ohne Änderung nichts.
                for court in ctx.tablet.courts() {
                    if let Some(frame) = score_mirror_frame(&ctx.tablet, court.id, &mut score_fp) {
                        let _ = tx.send(text(&frame));
                    }
                }
                push_freetext(ctx, &tx, &mut last_freetext);
                // Zugänge zuerst: Der Zustand nützt nichts, solange der Relay
                // niemanden kennt, dem er ihn zeigen dürfte.
                push_tl_auth(ctx, &tx, &mut tl_auth_fp);
                push_tl_state(ctx, &tx, &mut tl_state_rev);
            }
            _ = monitor_ticker.tick() => {
                maybe_upload_monitor(ctx, install_id, &mut monitor_fp).await;
                // Feld-Liste fürs Cloud-Feldwechsel-Menü mitschicken (selten
                // veränderlich; erster Tick feuert sofort nach dem Verbinden).
                push_courts(ctx, &tx);
                // Aufgerufene Spiele für die Slave-Spielübersicht + den Nachruf
                // am Slave (Cluster C Stufe 2) — nur bei Änderung senden.
                push_prepared(ctx, &tx, &mut prepared_fp);
            }
            _ = control_ticker.tick() => {
                sync_monitor_control(ctx, install_id, &mut control_fp).await;
            }
            nudge = nudge_rx.recv() => {
                // Feld hat sich geändert (Punkt, Zustand, Match): Stand
                // spiegeln, falls er sich fürs Relay wirklich unterscheidet.
                let court_id = nudge
                    .and_then(|n| serde_json::from_str::<MonitorNudge>(&n).ok())
                    .map(|n| n.court);
                if let Some(court_id) = court_id {
                    if let Some(frame) = score_mirror_frame(&ctx.tablet, court_id, &mut score_fp) {
                        let _ = tx.send(text(&frame));
                    }
                }
            }
            _ = idle_ticker.tick() => {
                // Half-open erkannt: seit RELAY_READ_IDLE kein Frame/Ping.
                // `Err` beendet `serve` → `run` öffnet einen frischen Socket
                // (Backoff resettet bei Erfolg auf ~1 s). Kein Client-Ping.
                if is_stale(last_incoming, tokio::time::Instant::now(), RELAY_READ_IDLE) {
                    return Err("Relay-Read-Idle: 15 s ohne Antwort → reconnect".into());
                }
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
    // Turnierlogo: Länge des Base64 + MIME reicht als Wechsel-Indikator (ein
    // neues Logo ändert beides), ohne die ganzen Daten in den Abdruck zu ziehen.
    s.push_str(&format!(
        "|logo:{}:{}",
        app.tournament_logo.data.len(),
        app.tournament_logo.mime
    ));
    let bar = monitor::read_ad_bar(&ctx.monitor_dir.join(monitor::AD_BAR_FILE));
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
        // Bar-Markierung in den Abdruck: ein Umschalten „in Leiste" muss den
        // Upload neu auslösen, sonst zeigt der Cloud-Monitor die alte Auswahl.
        let in_bar = bar.contains(&name);
        s.push_str(&format!("|{name}:{len}:{mtime}:{in_bar}"));
    }
    s
}

/// Baut den Court-Monitor-Datensatz und POSTet ihn zum Relay.
async fn upload_monitor(ctx: &ServerCtx, install_id: &str) -> Result<(), String> {
    let cfg = ctx.monitor_config();
    let bar = monitor::read_ad_bar(&ctx.monitor_dir.join(monitor::AD_BAR_FILE));
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
            in_bar: bar.contains(&name),
        });
    }
    let app = ctx.app_config();
    let ct = &app.call_timer;
    // Turnierlogo für die Sponsor-Leiste mitschicken (nur wenn gesetzt). Die
    // Config hält es bereits Base64 – kein erneutes Kodieren nötig.
    let logo = if app.tournament_logo.data.is_empty() {
        None
    } else {
        Some(relay_proto::LogoUpload {
            content_type: if app.tournament_logo.mime.is_empty() {
                "image/png".to_string()
            } else {
                app.tournament_logo.mime.clone()
            },
            data: app.tournament_logo.data.clone(),
        })
    };
    let upload = MonitorUpload {
        config: monitor::to_monitor_config(&cfg),
        tournament_name: ctx.tablet.tournament_name(),
        ads,
        call_timer: relay_proto::CallTimerView {
            enabled: ct.enabled,
            second_call_minutes: ct.second_call_minutes,
            third_call_minutes: ct.third_call_minutes,
        },
        logo,
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
    // Volle Anzeige-Ziele in `targets` (Court, Info-Übersicht/Vorbereitung/
    // Sieger, Werbung, Kombi) — so kann der Cloud-Monitor auch Nicht-Court-
    // Sichten umleiten. `assignments` (nur CourtID) bleibt zusätzlich befüllt,
    // damit ein noch nicht aktualisiertes Relay wenigstens die Court-Ziele kennt.
    let targets = monitor::read_assignments(&ctx.assignments_path);
    let assignments: std::collections::HashMap<String, i64> = targets
        .iter()
        .filter_map(|(k, t)| t.court_id().map(|c| (k.clone(), c)))
        .collect();
    let control = MonitorControl {
        assignments,
        targets,
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
    last_match: &mut HashMap<i64, Option<(i64, bool)>>,
    score_fp: &mut HashMap<i64, HostFrame>,
) {
    match frame {
        RelayFrame::TabletConnected { court_id, .. } => {
            ctx.tablet.attach_tablet(court_id);
            tracing::info!("Tablet verbunden für Feld {court_id} (Cloud)");
            // Sofort das aktuelle Match nachschieben (statt 2 s zu warten).
            last_match.remove(&court_id);
            push_court(ctx, court_id, tx, last_match, score_fp);
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
            match_id,
            ..
        } => {
            // handle_score filtert Nachzügler alter Matches selbst
            // (Stale-Filter A4) — wirkt so auch hinter einem ALTEN Relay,
            // das die matchId noch nicht prüft (0 = kein Filter).
            handle_score(court_id, score_a, score_b, &sets_history, match_id, ctx).await;
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
        // TL-Web über die Cloud: **derselbe** Ausführungsweg wie im
        // Hallennetz (`tl::execute`) — so wird jede Mutation genau einmal
        // geprüft, ganz wie bei den Ergebnissen der Tablets (R5).
        RelayFrame::TlCommand {
            req_id,
            device_id,
            op_id,
            view_rev,
            action,
        } => {
            // **In einem eigenen Task**, nicht hier abgewartet: Diese
            // Schleife bedient *eine* Verbindung für alle Tablets und
            // Monitore der Halle. Ein Kommando schreibt nach BTP (Anmeldung
            // + Aktualisierung); hängt BTP, käme aus dieser Schleife nichts
            // mehr heraus — nicht einmal das Pong. Der Relay hielte den Host
            // für tot und träfe die ganze Halle. Der Zeitablauf am Relay (20 s)
            // deckt den Fall ab, dass die Antwort ausbleibt.
            let ctx = ctx.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                // Der Relay hat den Zugang geprüft und schickt nur die
                // Kennung. Hier wird sie gegen die eigene Geräteliste
                // gehalten: Sonst hinge die Nachvollziehbarkeit allein am
                // Relay.
                let config = ctx.app_config();
                let response = match crate::tablet::tl::device_by_id(&config, &device_id) {
                    Some(device) => {
                        crate::tablet::tl::execute(
                            &ctx,
                            &device,
                            &op_id,
                            monitor::now_ms(),
                            view_rev,
                            action,
                        )
                        .await
                    }
                    None => relay_proto::TlResponse::err(
                        relay_proto::TlErrorCode::NotAllowed,
                        "Dieses Gerät ist auf dem Turnier-PC nicht (mehr) freigegeben.",
                    ),
                };
                let _ = tx.send(text(&HostFrame::TlAck { req_id, response }));
            });
        }
        // Punktverlauf (ADR 0014): gleicher Filter wie im LAN — der Frame
        // muss zum aktuellen Court-Match passen (HM-03, AK-3/AK-11). Die
        // Halter-Prüfung übernimmt im Cloud-Modus der Relay (eine aktive
        // Tablet-Session je Court, R4; Frames verdrängter Sessions
        // verwirft er) — wie beim ScoreUpdate-Weg.
        RelayFrame::Rally {
            court_id,
            match_id,
            set,
            n,
            winner,
            score_a,
            score_b,
        } => {
            if ctx
                .tablet
                .match_for_court(court_id)
                .is_some_and(|m| m.id == match_id)
            {
                ctx.tablet
                    .timeline_store()
                    .apply_rally(match_id, set, n, &winner, score_a, score_b);
            }
        }
        RelayFrame::RallySync {
            court_id,
            match_id,
            timeline,
        } => {
            if ctx
                .tablet
                .match_for_court(court_id)
                .is_some_and(|m| m.id == match_id)
            {
                ctx.tablet.timeline_store().apply_sync(match_id, timeline);
            }
        }
        // On-Demand-Abruf der TL-Oberfläche (AK-5): Antwort direkt aus dem
        // Store — winzige Payload, kein BTP-Zugriff, deshalb anders als
        // TlCommand ohne eigenen Task.
        RelayFrame::TimelineRequest { req_id, match_id } => {
            let json = ctx.tablet.timeline_store().timeline_json(match_id);
            let _ = tx.send(text(&HostFrame::TimelineData {
                req_id,
                found: json.is_some(),
                json: json.unwrap_or_default(),
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
    last_match: &mut HashMap<i64, Option<(i64, bool)>>,
    score_fp: &mut HashMap<i64, HostFrame>,
) {
    let court_label = ctx.tablet.court_display_label(court_id);
    let hall = ctx.tablet.court_hall(court_id);
    // A2 / ADR 0017, Regel b: Wie im LAN-Pfad (`push_match`) ein in BTP
    // finalisiertes Match dem Tablet mit `finalized:true` nachreichen, solange
    // es noch dieselbe matchId trägt — `match_for_court` liefert es nicht mehr
    // (Status Finished ≠ OnCourt). `finalized` reist vom Host (= BTP-Wahrheit)
    // im MatchBrief zum Relay und weiter zum Tablet.
    let current = ctx.tablet.match_for_court(court_id);
    let (effective, finalized) = match current {
        Some(m) => (Some(m), false),
        None => match ctx.tablet.recently_finalized(court_id) {
            Some(mid) => (ctx.tablet.snapshot_match(mid), true),
            None => (None, false),
        },
    };
    let finalized = finalized && effective.is_some();
    let key = effective.as_ref().map(|m| (m.id, finalized));
    if last_match.get(&court_id) == Some(&key) {
        return;
    }
    last_match.insert(court_id, key);
    // Zuweisungs-Wechsel: Der Relay setzt seinen Anzeige-Cache neu auf und
    // hat einen davor eingetroffenen Spiegel womöglich verworfen (Match dort
    // noch unbekannt — z. B. nach Relay-Neustart oder Court-Wechsel). Den
    // Spiegel-Fingerabdruck verwerfen, damit der Sweep dieses Ticks den
    // Stand direkt NACH der Zuweisung erneut schickt (dieselbe FIFO-Wire).
    score_fp.remove(&court_id);
    let frame = match effective {
        Some(m) => {
            tracing::info!(
                "Feld {court_id}: Match {} ans Tablet zugewiesen (Cloud, finalized={finalized})",
                m.id
            );
            HostFrame::MatchAssigned {
                court_id,
                court_label,
                hall,
                match_brief: {
                    let (sk, ska) = ctx.tablet.scorekeeper_display(court_id);
                    match_brief(&m, sk, ska, &ctx.app_config().display, finalized)
                },
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
    last_match: &mut HashMap<i64, Option<(i64, bool)>>,
    score_fp: &mut HashMap<i64, HostFrame>,
) {
    for court in ctx.tablet.courts() {
        push_court(ctx, court.id, tx, last_match, score_fp);
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

/// Master: kurzlebigen 8-stelligen Telefon-Kopplungscode beim Relay
/// anfordern (ADR 0004). Der Relay stellt ihn nur aus, wenn der Host dieses
/// Namespace gerade verbunden ist (Cloud-Modus läuft).
pub async fn request_pairing_code(namespace: &str) -> Result<relay_proto::PairingCode, String> {
    if !valid_relay_namespace(namespace) {
        return Err("Ungültiger Kopplungs-Code (install_id)".to_string());
    }
    let url = format!("{RELAY_HTTP}/{namespace}/pairing-code");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .send()
        .await
        .map_err(|_| "Relay nicht erreichbar – Internet prüfen".to_string())?;
    match resp.status() {
        s if s.is_success() => resp
            .json::<relay_proto::PairingCode>()
            .await
            .map_err(|e| e.to_string()),
        reqwest::StatusCode::CONFLICT => Err(
            "Der Master ist nicht mit der Cloud verbunden – erst „Start“ drücken (Verbindungsart Cloud bzw. LAN + Cloud).".to_string(),
        ),
        // Alter Relay kennt die Route nicht (404) → verständlich melden.
        reqwest::StatusCode::NOT_FOUND => Err(
            "Der Cloud-Server kennt den Telefon-Code noch nicht (Update ausstehend). Solange den langen Kopplungs-Code verwenden.".to_string(),
        ),
        s => Err(format!("Relay-Fehler {s}")),
    }
}

/// Slave: 8-stelligen Telefon-Kopplungscode beim Relay gegen den vollen
/// Master-Kopplungs-Code (Namespace/`install_id`) einlösen (ADR 0004).
pub async fn resolve_pairing_code(code: &str) -> Result<String, String> {
    if code.len() != 8 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Der Telefon-Code hat genau 8 Ziffern".to_string());
    }
    let url = format!("{RELAY_HTTP}/pair/{code}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|_| "Relay nicht erreichbar – Internet prüfen".to_string())?;
    match resp.status() {
        s if s.is_success() => resp
            .json::<relay_proto::PairingResolved>()
            .await
            .map(|p| p.namespace)
            .map_err(|e| e.to_string()),
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            Err("Zu viele Fehlversuche am Server – eine Minute warten".to_string())
        }
        _ => Err(
            "Code unbekannt oder abgelaufen – am Master einen neuen Telefon-Code erzeugen"
                .to_string(),
        ),
    }
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
/// veränderlich; periodisch (alle 30 s) genügt. Huckepack dabei: die Azure-TTS-
/// Vererbung an Cloud-Ansage-Slaves (ADR 0003).
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
    // Auch bei (noch) leerer Feldliste senden, wenn es eine Azure-Config zu
    // vererben gibt — sonst hinge die Vererbung daran, dass BTP schon ein
    // Turnier geladen hat (Review-Befund). Der Relay übernimmt eine leere
    // Feldliste nicht (schützt das Tablet-Feldwechsel-Menü bei Aussetzern).
    let azure_tts = azure_share(ctx);
    // A2 / ADR 0017: den Legacy-rev-Schalter mit durchreichen, damit der
    // Relay `ownership_active` setzen kann (Laufzeit-Rollback im Cloud-Modus).
    // Frisch aus der Config, wie überall — der Schalter greift ohne Neustart.
    let reconnect_legacy_rev = ctx.app_config().reconnect_legacy_rev;
    if !courts.is_empty() || azure_tts.is_some() {
        let _ = tx.send(text(&HostFrame::Courts {
            courts,
            azure_tts,
            reconnect_legacy_rev,
        }));
    }
}

/// Baut die Liste der aufgerufenen Spiele (in Vorbereitung gerufen) für die
/// Slave-Spielübersicht + den Nachruf am Slave (Cluster C Stufe 2). Nur
/// gerufene, noch eingeplante Paarungen mit zwei feststehenden Mannschaften;
/// jede wird mit ihrem Hallennamen versehen (Grundlage der Hallenfilterung am
/// Relay). Rein funktional → unit-testbar; gleiche Ruf-Barkeitsregel wie
/// `preparation_candidates`.
fn build_prepared_list(
    snapshot: &crate::btp::model::BtpSnapshot,
    calls: &[crate::tablet::state::PreparationCall],
) -> Vec<PreparedMatch> {
    let players = |ps: &[crate::btp::model::BtpPlayer]| -> Vec<PlayerBrief> {
        ps.iter()
            .map(|p| PlayerBrief {
                id: p.id,
                name: p.name.clone(),
                nationality: p.nationality.clone(),
                club: p.club.clone(),
            })
            .collect()
    };
    calls
        .iter()
        .filter_map(|call| {
            let m = snapshot.matches.iter().find(|m| m.id == call.match_id)?;
            // Nur noch ruf-bare, echte Paarungen (wie preparation_candidates):
            // Ergebnis/Feld-Ruf räumt den Aufruf beim nächsten Push weg.
            if m.status != crate::btp::model::MatchStatus::Scheduled
                || m.team1.is_empty()
                || m.team2.is_empty()
            {
                return None;
            }
            let hall = call
                .location_id
                .and_then(|lid| snapshot.locations.iter().find(|l| l.id == lid))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            Some(PreparedMatch {
                match_id: m.id,
                hall,
                discipline: m.discipline.as_str().to_string(),
                class_label: m.class_label.clone(),
                round_name: m.round_name.clone(),
                team_a: players(&m.team1),
                team_b: players(&m.team2),
                match_number: m.match_num,
                called_at_ms: call.called_at_ms,
            })
        })
        .collect()
}

/// Pusht die aufgerufenen Spiele an den Relay – nur bei Änderung gegenüber dem
/// letzten Push (Fingerabdruck). Ein leerer Push leert die Relay-Liste bewusst
/// (kein Aufruf mehr offen). `None`-Fingerabdruck erzwingt das erste Senden.
fn push_prepared(
    ctx: &ServerCtx,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_fp: &mut Option<String>,
) {
    let Some(snapshot) = ctx.tablet.snapshot_clone() else {
        return;
    };
    let prepared = build_prepared_list(&snapshot, &ctx.tablet.preparation_calls());
    let fp = serde_json::to_string(&prepared).unwrap_or_default();
    if last_fp.as_deref() == Some(fp.as_str()) {
        return;
    }
    *last_fp = Some(fp);
    let _ = tx.send(text(&HostFrame::Prepared { prepared }));
}

/// Spiegelt die zugelassenen Turnierleitungs-Geräte an den Relay.
///
/// Nur bei Änderung — aber **immer** einmal nach dem Verbinden (`last_fp`
/// ist dann `None`): Der Relay vergisst die Zugänge, sobald der Turnier-PC
/// abreißt, und ohne diesen ersten Push bliebe die Oberfläche nach jedem
/// Reconnect ausgesperrt.
///
/// Auch die **leere** Liste wird geschickt: Sie ist der Widerruf des letzten
/// Geräts und die Wirkung des Ausschalters.
fn push_tl_auth(
    ctx: &ServerCtx,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_fp: &mut Option<String>,
) {
    // **Nur mit gelesener Konfiguration.** Beim Speichern schreibt die App
    // die Datei neu; trifft dieser Takt genau dieses Fenster, ergäbe die
    // Standard-Konfiguration eine leere Liste — und die ist beim Relay der
    // Widerruf **aller** Geräte. Ein Speichervorgang in den Einstellungen
    // würde reihenweise Turnierleitungs-Geräte aussperren.
    let Ok(config) = ctx.app_config_result() else {
        return;
    };
    let devices = crate::tablet::tl::auth_devices(&config);
    // Mehr Geräte, als der Relay annimmt? Dann verwirft er das **ganze**
    // Frame — und zwar stillschweigend. Hier zu kappen wäre falsch (ein
    // halbierter Widerruf ist schlimmer als keiner), also lieber laut sein:
    // Ohne diese Meldung suchte man den Fehler auf der falschen Seite.
    if devices.len() > relay_proto::MAX_TL_DEVICES_MIRRORED {
        tracing::warn!(
            "Turnierleitungs-Geräte: {} eingetragen, der Relay nimmt höchstens {} — \
             die Cloud-Oberfläche bleibt gesperrt, bis alte Kopplungen entfernt sind",
            devices.len(),
            relay_proto::MAX_TL_DEVICES_MIRRORED
        );
        return;
    }
    let fp = crate::tablet::tl::auth_fingerprint(&devices);
    if last_fp.as_deref() == Some(fp.as_str()) {
        return;
    }
    *last_fp = Some(fp);
    let _ = tx.send(text(&HostFrame::TlAuth { devices }));
}

/// Schiebt den Anzeige-Zustand der Turnierleitung an den Relay.
///
/// Nur bei echter Änderung: Die Revision steigt genau dann, und der Relay
/// beantwortet unveränderte Stände mit „nichts Neues". Ohne diese Schranke
/// liefe alle zwei Sekunden ein voller Turnierstand durchs Netz — auf
/// Mobilfunkgeräten der Turnierleitung.
///
/// Ist die Oberfläche abgeschaltet, wird nichts gepusht: Ohne Zugänge kann
/// der Relay den Stand ohnehin niemandem zeigen.
fn push_tl_state(ctx: &ServerCtx, tx: &mpsc::UnboundedSender<WsMessage>, last_rev: &mut u64) {
    let config = ctx.app_config();
    if !config.tl_web.enabled {
        return;
    }
    // Gekürzt auf das, was der Relay ablegt — er verwirft Größeres, ohne es
    // zu melden.
    let (json, rev) = crate::tablet::tl::state_for_relay(&ctx.tablet, &config, monitor::now_ms());
    if rev == *last_rev {
        return;
    }
    *last_rev = rev;
    let _ = tx.send(text(&HostFrame::TlState { rev, json }));
}

/// Azure-TTS-Konfiguration für die Vererbung an Cloud-Slaves (ADR 0003).
/// Frisch von Platte gelesen, damit Einstellungs-Änderungen ohne Neustart
/// greifen. `None`, wenn Azure aus oder unvollständig ist — der Relay
/// verwirft dann eine früher geerbte Config.
fn azure_share(ctx: &ServerCtx) -> Option<relay_proto::AzureTtsShare> {
    let az = ctx.app_config().azure_tts;
    (az.enabled && !az.key.is_empty() && !az.region.is_empty()).then_some(
        relay_proto::AzureTtsShare {
            region: az.region,
            key: az.key,
            voice: az.voice,
            discipline_voices: az.discipline_voices,
        },
    )
}

/// Trägt das Monitor-Nudge-Abo des Score-Spiegels beim Sitzungsende wieder
/// aus — als Drop-Guard, damit auch der `Err`-Ausstieg des Read-Idle-Wächters
/// aufräumt und Reconnects den Fan-out-Deckel (`MAX_MONITOR_SUBS`) nicht
/// allmählich auffüllen.
struct NudgeGuard<'a> {
    tablet: &'a TabletState,
    tx: MonitorNudgeTx,
}

impl Drop for NudgeGuard<'_> {
    fn drop(&mut self) {
        self.tablet.unsubscribe_monitor(None, &self.tx);
    }
}

/// Baut den Score-Spiegel-Frame (Host→Relay) für ein Feld — oder `None`,
/// wenn dort kein Match liegt oder sich seit dem letzten Senden nichts
/// geändert hat (Turnier-Befund 13.08.2026: Im LAN(+Cloud)-Betrieb zählen
/// die Tablets am Relay vorbei; ohne diesen Spiegel bleiben Cloud-Monitor
/// und Cloud-Übersicht auf 0:0 stehen).
///
/// `last` hält je Feld das zuletzt gebaute Frame als Fingerabdruck: Der
/// Nudge-Kanal feuert auch für Akku-/Verbindungs-Ereignisse, und ein
/// unverändert wiederholtes Frame würde die Cloud-Monitore grundlos wecken.
/// `push_court` verwirft den Eintrag eines Felds bei jedem Zuweisungs-Push —
/// der Sweep spiegelt dann erneut, NACHDEM der Relay das Match kennt.
fn score_mirror_frame(
    tablet: &TabletState,
    court_id: i64,
    last: &mut HashMap<i64, HostFrame>,
) -> Option<HostFrame> {
    let mirror = tablet.score_mirror_of(court_id)?;
    let frame = HostFrame::ScoreUpdate {
        court_id,
        match_id: mirror.match_id,
        sets: mirror.sets.into_iter().map(|(a, b)| SetAb { a, b }).collect(),
        state: mirror.state,
    };
    if last.get(&court_id) == Some(&frame) {
        return None;
    }
    last.insert(court_id, frame.clone());
    Some(frame)
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

    #[test]
    fn is_stale_grenzfaelle() {
        use tokio::time::{Duration, Instant};
        let t0 = Instant::now();
        // Knapp unter der Schwelle → noch lebendig (Grenze ist `>=`).
        let almost = t0 + RELAY_READ_IDLE - Duration::from_millis(1);
        assert!(!is_stale(t0, almost, RELAY_READ_IDLE));
        // Exakt an der Schwelle → Read-Idle, reconnect.
        assert!(is_stale(t0, t0 + RELAY_READ_IDLE, RELAY_READ_IDLE));
        // Deutlich darüber → Read-Idle.
        assert!(is_stale(
            t0,
            t0 + RELAY_READ_IDLE + Duration::from_secs(5),
            RELAY_READ_IDLE
        ));
    }

    #[test]
    fn gesunde_verbindung_nicht_stale() {
        use tokio::time::{Duration, Instant};
        // Frischer Empfang (4 s alt bei RELAY_READ_IDLE = 15 s) → kein Drop.
        let now = Instant::now();
        let last = now - Duration::from_secs(4);
        assert!(!is_stale(last, now, RELAY_READ_IDLE));
    }

    /// Score-Spiegel Host→Relay (Turnier-Befund 13.08.2026): Der Frame trägt
    /// den effektiven Feld-Stand (Tablet-Score + Tablet-Zustand), wird aber
    /// nur bei ÄNDERUNG gebaut — der Nudge-Kanal feuert auch für Akku-/
    /// Verbindungs-Ereignisse, und unverändertes Wiederholen würde die
    /// Cloud-Monitore grundlos wecken.
    #[test]
    fn score_mirror_frame_reports_only_changes() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snapshot(vec![match_on_court(7, 101)]));
        let mut last = HashMap::new();

        // Erster Stand (noch kein Tablet): Match bekannt, Sätze leer.
        match score_mirror_frame(&tablet, 101, &mut last).expect("erster Stand wird gemeldet") {
            HostFrame::ScoreUpdate {
                court_id,
                match_id,
                sets,
                state,
            } => {
                assert_eq!((court_id, match_id), (101, 7));
                assert!(sets.is_empty(), "ohne Tablet: BTP-Stand (leer)");
                assert!(state.is_none(), "kein Tablet-Zustand vorhanden");
            }
            f => panic!("unerwartetes Frame: {f:?}"),
        }
        // Unverändert → nichts zu senden.
        assert!(score_mirror_frame(&tablet, 101, &mut last).is_none());

        // Das Tablet zählt → der neue Stand wird genau einmal gemeldet.
        tablet.record_score(101, 7, vec![(11, 9)]);
        match score_mirror_frame(&tablet, 101, &mut last).expect("Änderung wird gemeldet") {
            HostFrame::ScoreUpdate { sets, .. } => {
                assert_eq!(sets, vec![relay_proto::SetAb { a: 11, b: 9 }]);
            }
            f => panic!("unerwartetes Frame: {f:?}"),
        }
        assert!(score_mirror_frame(&tablet, 101, &mut last).is_none());

        // Feld ohne Match → nichts zu senden.
        assert!(score_mirror_frame(&tablet, 999, &mut last).is_none());
    }

    fn player(n: &str) -> BtpPlayer {
        BtpPlayer {
            id: 0,
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
            display_order: None,
            from1: None,
            from2: None,
            id,
            draw_id: 7,
            planning_id: 1001,
            draw_name: "HE".into(),
            discipline: Discipline::MensSingles,
            class_label: String::new(),
            round_name: "G1".into(),
            match_num: Some(1),
            planned_time: None,
            team1: vec![player("A")],
            team2: vec![player("B")],
            entry1_id: 0,
            entry2_id: 0,
            court: Some("1".into()),
            court_id: Some(court_id),
            location_id: None,
            sets: vec![],
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::OnCourt,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            official1_id: None,
            official2_id: None,
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
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
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

    fn scheduled_match(id: i64) -> BtpMatch {
        let mut m = match_on_court(id, 0);
        m.status = MatchStatus::Scheduled;
        m.court = None;
        m.court_id = None;
        m
    }

    /// `build_prepared_list` liefert nur gerufene, noch eingeplante Paarungen –
    /// mit aufgelöstem Hallennamen. Aufrufe zu Nicht-mehr-ruf-baren Matches
    /// (aufs Feld / beendet / fehlend) fallen raus (Grundlage der Slave-
    /// Spielübersicht, Cluster C Stufe 2).
    #[test]
    fn build_prepared_list_only_callable_with_hall() {
        use crate::btp::model::BtpLocation;
        use crate::tablet::state::PreparationCall;

        let mut snap = snapshot(vec![
            scheduled_match(42),
            match_on_court(43, 101), // schon aufs Feld → nicht mehr ruf-bar
        ]);
        snap.locations = vec![BtpLocation {
            id: 5,
            name: "Halle 2".into(),
        }];

        let calls = vec![
            PreparationCall {
                match_id: 42,
                location_id: Some(5),
                called_at_ms: 1_700_000_000_000,
            },
            PreparationCall {
                match_id: 43, // steht auf dem Feld → ausgefiltert
                location_id: Some(5),
                called_at_ms: 1_700_000_000_000,
            },
            PreparationCall {
                match_id: 999, // kein Match im Snapshot → ausgefiltert
                location_id: None,
                called_at_ms: 1_700_000_000_000,
            },
        ];

        let prepared = build_prepared_list(&snap, &calls);
        assert_eq!(prepared.len(), 1, "nur das eine ruf-bare Spiel");
        assert_eq!(prepared[0].match_id, 42);
        assert_eq!(prepared[0].hall, "Halle 2", "LocationID → Hallenname");
        assert_eq!(prepared[0].team_a.len(), 1);
        assert_eq!(prepared[0].team_b.len(), 1);
    }

    /// Cloud-Feld-Diffing: erster Push meldet die Zuweisung, ein unveränderter
    /// Stand wird dedupliziert (kein doppelter Push → kein Tablet-Reset), und
    /// verlässt das Match das Feld, kommt genau eine Aufhebung.
    #[test]
    fn push_court_sends_once_dedups_then_clears() {
        let ctx = ctx_with(vec![match_on_court(42, 101)]);
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        let mut last: HashMap<i64, Option<(i64, bool)>> = HashMap::new();
        let mut score_fp: HashMap<i64, HostFrame> = HashMap::new();

        // Der Spiegel-Fingerabdruck des Felds ist gesetzt (als hätte der
        // Sweep schon gespiegelt) — ein Zuweisungs-Push muss ihn verwerfen.
        score_fp.insert(
            101,
            HostFrame::ScoreUpdate {
                court_id: 101,
                match_id: 42,
                sets: vec![],
                state: None,
            },
        );

        // 1) Zuweisung → genau ein MatchAssigned; Spiegel-FP invalidiert,
        // damit der Sweep des Ticks den Stand NACH der Zuweisung neu spiegelt
        // (Review-Befund: Relay-Neustart/Court-Wechsel verwarf den Spiegel,
        // der Fingerabdruck verhinderte dann jede Wiederholung).
        push_court(&ctx, 101, &tx, &mut last, &mut score_fp);
        let f1 = rx.try_recv().expect("ein Frame erwartet");
        assert!(
            text_of(&f1).contains("\"type\":\"match_assigned\""),
            "erwartet match_assigned, war: {}",
            text_of(&f1)
        );
        assert!(
            !score_fp.contains_key(&101),
            "Zuweisungs-Push verwirft den Spiegel-Fingerabdruck"
        );

        // 2) Unveränderter Stand → kein erneuter Push (Dedup), FP unberührt.
        score_fp.insert(
            101,
            HostFrame::ScoreUpdate {
                court_id: 101,
                match_id: 42,
                sets: vec![],
                state: None,
            },
        );
        push_court(&ctx, 101, &tx, &mut last, &mut score_fp);
        assert!(
            rx.try_recv().is_err(),
            "kein doppelter Push bei gleichem Stand"
        );
        assert!(
            score_fp.contains_key(&101),
            "Dedup-Fall lässt den Spiegel-Fingerabdruck stehen"
        );

        // 3) Match verlässt das Feld → genau ein MatchCleared.
        ctx.tablet.set_snapshot(snapshot(vec![]));
        push_court(&ctx, 101, &tx, &mut last, &mut score_fp);
        let f3 = rx.try_recv().expect("Clear-Frame erwartet");
        assert!(
            text_of(&f3).contains("\"type\":\"match_cleared\""),
            "erwartet match_cleared, war: {}",
            text_of(&f3)
        );
        assert!(
            !score_fp.contains_key(&101),
            "auch die Aufhebung verwirft den Spiegel-Fingerabdruck"
        );
    }
}
