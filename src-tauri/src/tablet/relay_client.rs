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

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;

use relay_proto::{
    AdUpload, AnnounceState, CourtBrief, HostFrame, MonitorControl, MonitorDeviceInfo,
    MonitorUpload, PlayerBrief, PreparedMatch, RelayFrame, ResultBody, SetAb,
};

use crate::tablet::monitor;
use crate::tablet::server::{handle_score, match_brief, process_result, ServerCtx};
use crate::tablet::state::{MonitorNudge, MonitorNudgeTx, TabletState};
use crate::tablet::ws_verbindung::{self, Backoff, Schluss, SitzungsEnde};

/// Öffentliche Relay-Basis – der Host-Pfad hängt die `install_id` an.
/// Produktiv- oder Testsystem, abgeleitet aus der Push-URL der
/// Konfiguration (siehe [`crate::badhub_host`]) – ein Testturnier soll
/// nicht über den Produktiv-Relay laufen.
fn relay_host() -> String {
    crate::badhub_host::relay_wss()
}

/// HTTPS-Basis des Relays – für den Court-Monitor-Werbe-Upload.
fn relay_http() -> String {
    crate::badhub_host::relay_https()
}

/// Abstand der Match-Push-Ticks (Court → Tablet-Zuweisung).
const TICK: Duration = Duration::from_secs(2);

/// Abstand der Court-Monitor-Upload-Prüfung (Werbung/Konfiguration).
const MONITOR_TICK: Duration = Duration::from_secs(30);

/// Abstand des Geräte-Steuerungs-Abgleichs (Feld-Zuweisungen, Fernbefehle,
/// Geräteliste) – kurz, damit Befehle zügig am Monitor ankommen.
const CONTROL_TICK: Duration = Duration::from_secs(3);

/// Read-Idle-Schwelle (Hebel D / ADR 0020, Option A): Bleibt jedes
/// Lebenszeichen des Relays (Frame **oder** Relay-Ping) länger aus, gilt die
/// Verbindung als half-open tot → die Sitzung endet als Abriss, `run`
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

/// Frist je Nachricht im Schreiber-Task. Hängt das Senden länger, gilt die
/// Verbindung als tot (der Relay hätte den Host nach 15 s Stille ohnehin
/// verworfen) — die Sitzung endet und `run` verbindet neu.
const SENDE_FRIST: Duration = Duration::from_secs(20);

/// Wartende Ergebnisse je Feld (zusätzlich zu dem einen in Arbeit). Ein
/// Feld hat ein laufendes Spiel; mehr als der 8-s-Retry desselben
/// Ergebnisses staut sich im Normalfall nie.
const ERGEBNIS_SCHLANGE: usize = 4;

/// Sammel-Schlüssel für Ergebnisse an ein Feld, das dieser Turnier-PC gar
/// nicht kennt. Die CourtID kommt vom Tablet über den Relay und ist damit
/// fremdbestimmt — ohne den Sammler könnte ein Fremder beliebig viele
/// Arbeiter-Tasks anlegen. `process_result` lehnt solche Ergebnisse ohnehin
/// ab (R5); sie laufen nur nacheinander statt nebeneinander.
const SAMMEL_FELD: i64 = i64::MIN;

/// Frist für den Werbe-Upload. Bis zu 12 MB Bilder über einen Handy-Hotspot
/// passen nicht in die 15 s des gemeinsamen HTTP-Clients — der Upload
/// scheiterte dann alle 30 s neu.
const UPLOAD_FRIST: Duration = Duration::from_secs(120);

/// Was mit einem Ergebnis geschieht — im Betrieb [`process_result`], im Test
/// eine Attrappe (die Sitzung soll ohne BTP prüfbar sein).
type ErgebnisFuture = Pin<Box<dyn Future<Output = relay_proto::ResultResponse> + Send>>;
type ErgebnisVerarbeiter = Arc<dyn Fn(Arc<ServerCtx>, ResultBody) -> ErgebnisFuture + Send + Sync>;

/// Der echte Weg: dieselbe Prüfung und derselbe BTP-Write wie im
/// Hallennetz (R5) — unverändert, nur nicht mehr in der Lese-Schleife.
fn ergebnis_ueber_process_result() -> ErgebnisVerarbeiter {
    Arc::new(|ctx, body| Box::pin(async move { process_result(&ctx, &body).await }))
}

/// Ein Ergebnis auf dem Weg zum Arbeiter seines Felds.
struct ErgebnisAuftrag {
    req_id: u64,
    body: ResultBody,
    /// Ausgangskanal der Sitzung, aus der der Auftrag kam. Die Quittung
    /// geht **nur** dorthin: Der Relay zählt `req_id` je Namespace ab 1 —
    /// nach seinem Neustart könnte eine alte Nummer über eine neue Sitzung
    /// ein fremdes Ergebnis mit „ok" quittieren. Ist die Sitzung beendet,
    /// verpufft die Quittung; das Tablet wiederholt und trifft die
    /// Idempotenz (ADR 0018).
    antwort: mpsc::UnboundedSender<WsMessage>,
}

/// Ergebnis-Arbeiter je Feld (ADR 0091): Ergebnisse eines Felds laufen
/// strikt nacheinander, Felder untereinander nebeneinander. Gehört `run`,
/// nicht der Sitzung — ein BTP-Write läuft auch dann zu Ende, wenn die
/// Verbindung mittendrin abreißt.
///
/// Warum kein blankes `tokio::spawn` je Ergebnis: Der 8-s-Retry des Tablets
/// liefe dann **neben** dem ersten Versuch. Beide sähen das Spiel noch auf
/// dem Feld und schrieben beide nach BTP.
struct ErgebnisArbeiter {
    verarbeiter: ErgebnisVerarbeiter,
    felder: Mutex<HashMap<i64, mpsc::Sender<ErgebnisAuftrag>>>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl ErgebnisArbeiter {
    fn new(verarbeiter: ErgebnisVerarbeiter) -> Self {
        Self {
            verarbeiter,
            felder: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
        }
    }

    /// Reiht ein Ergebnis hinter die schon wartenden seines Felds. Ist die
    /// Schlange voll, kommt sofort eine **wiederholbare** Absage — das
    /// Tablet behält sein Ergebnis und versucht es erneut.
    fn einreihen(&self, ctx: &Arc<ServerCtx>, auftrag: ErgebnisAuftrag) {
        let court = auftrag.body.court_id;
        let schluessel = if ctx.tablet.courts().iter().any(|c| c.id == court) {
            court
        } else {
            SAMMEL_FELD
        };
        let mut auftrag = auftrag;
        for versuch in 0..2 {
            let sender = {
                let mut felder = self.felder.lock().unwrap();
                felder
                    .entry(schluessel)
                    .or_insert_with(|| {
                        let (tx, rx) = mpsc::channel(ERGEBNIS_SCHLANGE);
                        let task =
                            tokio::spawn(feld_arbeiter(ctx.clone(), self.verarbeiter.clone(), rx));
                        let mut tasks = self.tasks.lock().unwrap();
                        tasks.retain(|t| !t.is_finished());
                        tasks.push(task);
                        tx
                    })
                    .clone()
            };
            match sender.try_send(auftrag) {
                Ok(()) => return,
                // Der Arbeiter dieses Felds lebt nicht mehr (Panik im
                // Ergebnis-Weg). Ohne Ersatz bekäme das Feld bis zum Neustart
                // der Übertragung nur noch Absagen.
                Err(mpsc::error::TrySendError::Closed(a)) if versuch == 0 => {
                    tracing::error!(
                        "Ergebnis-Arbeiter für Feld {court} war beendet — neu gestartet"
                    );
                    self.felder.lock().unwrap().remove(&schluessel);
                    auftrag = a;
                }
                Err(mpsc::error::TrySendError::Full(a) | mpsc::error::TrySendError::Closed(a)) => {
                    tracing::warn!(
                        "Ergebnis für Feld {court} abgewiesen: Es warten schon {ERGEBNIS_SCHLANGE} Ergebnisse"
                    );
                    quittieren(
                        &a.antwort,
                        a.req_id,
                        relay_proto::ResultResponse::err(
                            "Der Turnier-PC verarbeitet gerade ein Ergebnis für dieses Feld. Bitte gleich noch einmal senden.",
                        ),
                    );
                    return;
                }
            }
        }
    }
}

impl Drop for ErgebnisArbeiter {
    fn drop(&mut self) {
        // `run` endet nur durch Abbruch (`stop_sync`) — dann sollen auch die
        // Arbeiter nicht weiterleben.
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }
}

/// Der Arbeiter eines Felds: ein Ergebnis nach dem anderen.
async fn feld_arbeiter(
    ctx: Arc<ServerCtx>,
    verarbeiter: ErgebnisVerarbeiter,
    mut rx: mpsc::Receiver<ErgebnisAuftrag>,
) {
    while let Some(auftrag) = rx.recv().await {
        let antwort = verarbeiter(ctx.clone(), auftrag.body).await;
        quittieren(&auftrag.antwort, auftrag.req_id, antwort);
    }
}

/// Schickt die Quittung an die Sitzung des Auftrags (oder ins Leere, wenn
/// es sie nicht mehr gibt).
fn quittieren(
    antwort: &mpsc::UnboundedSender<WsMessage>,
    req_id: u64,
    resp: relay_proto::ResultResponse,
) {
    let _ = antwort.send(text(&HostFrame::ResultAck {
        req_id,
        ok: resp.ok,
        error: resp.error,
        permanent: resp.permanent,
        superseded: resp.superseded,
    }));
}

/// Pause nach dem n-ten Fehlversuch in Folge: 30 s, 60 s, 120 s, 240 s,
/// dann 5 min.
fn pause_nach_fehlversuch(fehlversuche: u32) -> Duration {
    let stufe = fehlversuche.saturating_sub(1).min(4);
    Duration::from_secs((30u64 << stufe).min(300))
}

/// „Läuft schon"-Wächter für eine Hintergrund-Aufgabe (Werbe-Upload,
/// Steuerungs-Abgleich): höchstens ein Lauf zugleich — auch über die
/// Sitzungen eines `run` hinweg (Reconnects), deshalb gehört er `run`. Ein
/// Neustart der Übertragung beginnt mit frischen Wächtern; die alten Tasks
/// bricht [`Dienste`] dabei ab. Mit `mit_pausen` wartet er nach einem
/// Fehlschlag wachsend länger, statt im Takt neu anzurennen.
struct LaufWacht {
    mit_pausen: bool,
    stand: Mutex<LaufStand>,
}

#[derive(Default)]
struct LaufStand {
    laeuft: bool,
    fehlversuche: u32,
    fruehestens: Option<tokio::time::Instant>,
}

impl LaufWacht {
    fn new(mit_pausen: bool) -> Arc<Self> {
        Arc::new(Self {
            mit_pausen,
            stand: Mutex::new(LaufStand::default()),
        })
    }

    /// Beansprucht den Lauf. `None`, solange einer läuft oder die Pause
    /// nach einem Fehlschlag noch nicht um ist.
    fn beginnen(self: &Arc<Self>, jetzt: tokio::time::Instant) -> Option<Lauf> {
        let mut stand = self.stand.lock().unwrap();
        if stand.laeuft || stand.fruehestens.is_some_and(|t| jetzt < t) {
            return None;
        }
        stand.laeuft = true;
        Some(Lauf {
            wacht: self.clone(),
            gemeldet: false,
        })
    }
}

impl LaufWacht {
    /// Vergisst die Pause nach Fehlschlägen — zu Beginn einer Sitzung: Ein
    /// frisch gestarteter Relay hat keine Bilder mehr, und die Monitore
    /// sollen nicht bis zu fünf Minuten ohne Werbung bleiben, nur weil der
    /// letzte Versuch der alten Verbindung scheiterte.
    fn pause_vergessen(&self) {
        let mut stand = self.stand.lock().unwrap();
        stand.fehlversuche = 0;
        stand.fruehestens = None;
    }
}

/// Ein laufender Durchgang. Gibt den Wächter beim Fallenlassen wieder frei
/// — auch wenn der Task abgebrochen wird, bleibt nichts hängen.
struct Lauf {
    wacht: Arc<LaufWacht>,
    gemeldet: bool,
}

impl Lauf {
    /// Meldet das Ende des Durchgangs.
    fn fertig(mut self, ok: bool, jetzt: tokio::time::Instant) {
        self.gemeldet = true;
        let mut stand = self.wacht.stand.lock().unwrap();
        stand.laeuft = false;
        if ok || !self.wacht.mit_pausen {
            stand.fehlversuche = 0;
            stand.fruehestens = None;
        } else {
            stand.fehlversuche = stand.fehlversuche.saturating_add(1);
            stand.fruehestens = Some(jetzt + pause_nach_fehlversuch(stand.fehlversuche));
        }
    }
}

impl Drop for Lauf {
    fn drop(&mut self) {
        if !self.gemeldet {
            self.wacht.stand.lock().unwrap().laeuft = false;
        }
    }
}

/// Was über alle Sitzungen hinweg lebt (gehört `run`).
struct Dienste {
    ergebnisse: ErgebnisArbeiter,
    upload: Arc<LaufWacht>,
    steuerung: Arc<LaufWacht>,
    /// Laufende Upload-/Abgleich-Tasks. Sie enden mit `run` (siehe `Drop`):
    /// Sonst liefe nach dem Stoppen oder dem Speichern der Einstellungen ein
    /// Upload bis zu zwei Minuten mit dem alten Stand weiter, und ein alter
    /// Abgleich könnte die Geräteliste der neuen Übertragung überschreiben.
    hintergrund: Mutex<Vec<tokio::task::AbortHandle>>,
}

impl Dienste {
    fn new(verarbeiter: ErgebnisVerarbeiter) -> Self {
        Self {
            ergebnisse: ErgebnisArbeiter::new(verarbeiter),
            upload: LaufWacht::new(true),
            steuerung: LaufWacht::new(false),
            hintergrund: Mutex::new(Vec::new()),
        }
    }

    /// Startet eine Hintergrund-Aufgabe, die spätestens mit `run` endet.
    fn im_hintergrund(&self, aufgabe: impl Future<Output = ()> + Send + 'static) {
        let mut laufende = self.hintergrund.lock().unwrap();
        laufende.retain(|t| !t.is_finished());
        laufende.push(tokio::spawn(aufgabe).abort_handle());
    }
}

impl Drop for Dienste {
    fn drop(&mut self) {
        for task in self.hintergrund.lock().unwrap().drain(..) {
            task.abort();
        }
    }
}

/// Bricht einen Task ab, sobald der Besitzer endet — auch wenn dessen
/// Future nur fallengelassen wird.
struct Abbrecher(tokio::task::AbortHandle);

impl Drop for Abbrecher {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Schreiber-Task: besitzt die Sende-Hälfte und leert den Ausgangskanal.
/// Getrennt von der Lese-Schleife, damit eine große oder zäh abfließende
/// Nachricht das Lesen (und damit das Lebenszeichen) nicht aufhält. Endet
/// mit dem Grund, sobald das Senden scheitert oder länger als
/// [`SENDE_FRIST`] hängt.
fn schreiber_starten<K>(
    mut sink: K,
    mut rx: mpsc::UnboundedReceiver<WsMessage>,
) -> tokio::task::JoinHandle<String>
where
    K: futures_util::Sink<WsMessage> + Unpin + Send + 'static,
    K::Error: std::fmt::Display,
{
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            match tokio::time::timeout(SENDE_FRIST, sink.send(m)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return e.to_string(),
                Err(_) => {
                    return format!(
                        "Senden an den Relay hing länger als {} s",
                        SENDE_FRIST.as_secs()
                    )
                }
            }
        }
        "Ausgangskanal geschlossen".to_string()
    })
}

/// Verbindet bts-light dauerhaft zum Cloud-Relay – mit Reconnect-Backoff
/// (siehe [`ws_verbindung::Backoff`]). Läuft, bis der Task abgebrochen wird
/// (`stop_sync`).
pub async fn run(ctx: Arc<ServerCtx>, install_id: String) {
    let url = format!("{}/{install_id}/host-ws", relay_host());
    let mut backoff = Backoff::default();
    // Fernbefehl-Wächter über ALLE Sitzungen hinweg (Spec
    // `tablet-version-abgleich`, Review-Fund 24.08.2026).
    //
    // Anders als beim Tablet-Socket, wo er je Verbindung neu gesetzt wird:
    // Dort heißt „verbunden" = „Seite frisch geladen", ein Befehl davor gilt
    // zu Recht nicht mehr. Hier heißt es nur „der HOST hat wieder Leitung" —
    // die Tablets dahinter sind dabei gerade **nicht** frisch. Je Sitzung
    // gesetzt ginge ein Befehl, der in einen Abriss fällt (Backoff bis 30 s),
    // still verloren, während die Turnierleitung „Die Tablets laden neu"
    // gemeldet bekommt.
    //
    // Über die Sitzungen hinweg gilt beides: Ein Netzwackler ohne Befehl löst
    // nichts aus (der Stand ist unverändert), ein Befehl während des Abrisses
    // wird beim Wiederverbinden nachgeholt.
    let mut reload_wacht = crate::tablet::state::ReloadWacht::beim_verbinden(&ctx.tablet);
    // Ergebnis-Arbeiter und „läuft schon"-Wächter leben über die Sitzungen
    // hinweg: Ein BTP-Write oder Upload endet nicht mit der Verbindung.
    let dienste = Dienste::new(ergebnis_ueber_process_result());
    // Endet die Übertragung (Stoppen, Speichern), verschwindet auch der
    // Hinweis „anderer PC" — er gehört zu dieser Verbindung.
    let _lage = LageWacht(ctx.tablet.clone());
    loop {
        let ende = serve(&ctx, &url, &install_id, &mut reload_wacht, &dienste).await;
        ctx.tablet
            .cloud_lage_aendern(|l| l.sitzung_ende(ende, monitor::now_ms()));
        let pause = ws_verbindung::mit_jitter(backoff.pause_nach(ende), ws_verbindung::zufall());
        tracing::info!("Relay-Reconnect in {:.1} s ({ende:?})", pause.as_secs_f32());
        tokio::time::sleep(pause).await;
    }
}

/// Setzt die Cloud-Lage zurück, wenn `run` endet.
struct LageWacht(Arc<TabletState>);

impl Drop for LageWacht {
    fn drop(&mut self) {
        self.0.cloud_lage_aendern(|l| *l = Default::default());
    }
}

/// Lohnt es, den Werbe-Upload gleich noch einmal zu versuchen? Der Relay
/// antwortet 409, solange er diesen Turnier-PC noch nicht eingetragen hat
/// (der Upload kann die eigene Anmeldung überholen), und 503, wenn er
/// gerade zwei andere Uploads verarbeitet. Beides ist in Sekunden vorbei.
fn upload_gleich_wieder(fehler: &str) -> bool {
    fehler.starts_with("HTTP 409") || fehler.starts_with("HTTP 503")
}

/// So oft und in diesem Abstand wird der Upload dann wiederholt, bevor die
/// gewöhnliche Pause (30 s und länger) greift.
const UPLOAD_KURZ_VERSUCHE: u32 = 3;
const UPLOAD_KURZ_PAUSE: Duration = Duration::from_secs(3);

/// Eine Relay-Sitzung: verbinden (mit Frist), Frames austauschen, bis die
/// Verbindung endet. Liefert, wie sie ausging — daraus rechnet `run` die
/// Pause bis zum nächsten Versuch.
async fn serve(
    ctx: &Arc<ServerCtx>,
    url: &str,
    install_id: &str,
    reload_wacht: &mut crate::tablet::state::ReloadWacht,
    dienste: &Dienste,
) -> SitzungsEnde {
    let stream = match ws_verbindung::verbinden(url, ws_verbindung::VERBINDUNGS_FRIST).await {
        Ok(stream) => stream,
        Err(e) => {
            tracing::warn!("Relay nicht erreichbar: {e}");
            return SitzungsEnde::AufbauGescheitert;
        }
    };
    tracing::info!("Mit Cloud-Relay verbunden");
    ctx.tablet
        .cloud_lage_aendern(|l| l.verbunden(monitor::now_ms()));
    let beginn = tokio::time::Instant::now();
    let schluss = match sitzung_fahren(ctx, stream, install_id, reload_wacht, dienste).await {
        Ok(schluss) => {
            tracing::info!("Relay-Verbindung beendet ({schluss:?})");
            schluss
        }
        Err(e) => {
            tracing::warn!("Relay-Verbindung beendet: {e}");
            Schluss::Abriss
        }
    };
    ws_verbindung::einordnen(beginn.elapsed(), schluss)
}

/// Der Kern einer Sitzung, ohne den Verbindungsaufbau — generisch über den
/// Strom, damit Tests ihn über `tokio::io::duplex` mit echtem
/// WebSocket-Rahmen fahren können.
///
/// Die Schleife liest, verteilt und tickt; sie wartet auf nichts Langes:
/// Gesendet wird im Schreiber-Task, Ergebnisse laufen über die Arbeiter je
/// Feld, Upload und Steuerungs-Abgleich in eigenen Tasks. Solange sie
/// früher inline warteten (BTP bis 15 s und mehr, Upload bis 15 s), kam aus
/// dieser Schleife nichts heraus — nicht einmal das Pong —, und der Relay
/// warf den Turnier-PC nach 15 s Stille ab (trifft alle Cloud-Felder).
async fn sitzung_fahren<S>(
    ctx: &Arc<ServerCtx>,
    stream: WebSocketStream<S>,
    install_id: &str,
    reload_wacht: &mut crate::tablet::state::ReloadWacht,
    dienste: &Dienste,
) -> Result<Schluss, String>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (sink, mut read) = stream.split();
    let (tx, rx) = mpsc::unbounded_channel::<WsMessage>();
    let mut schreiber = schreiber_starten(sink, rx);
    dienste.upload.pause_vergessen();
    let _schreiber_ende = Abbrecher(schreiber.abort_handle());
    // CourtID → zuletzt ans Tablet gemeldete Match-ID. Verhindert, dass der
    // 2-s-Ticker unverändert dasselbe Match immer wieder pusht.
    let mut last_match: HashMap<i64, Option<CourtPushKey>> = HashMap::new();
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
    // Fingerabdruck je Sitzung (nach einem Relay-Neustart hat der Relay die
    // Bilder nicht mehr); der Upload-Task setzt ihn bei Erfolg.
    let monitor_fp: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    // Fingerabdruck der zuletzt gesendeten „aufgerufene Spiele"-Liste
    // (Cluster C Stufe 2) — `None` erzwingt das erste Senden nach dem
    // Verbinden (auch für die Wiederbefüllung nach einem Reconnect).
    let mut prepared_fp: Option<String> = None;
    // Meetingpoint-Stand (Spec `meetingpoint-spielbereit`): im 2-s-Takt,
    // nur bei Änderung; `None` erzwingt das erste Senden nach dem Verbinden.
    let mut meetingpoint_fp: Option<String> = None;
    // Geräte-Steuerung: Feld-Zuweisungen/Befehle pushen, Geräteliste holen.
    let mut control_ticker = tokio::time::interval(CONTROL_TICK);
    control_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let control_fp: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
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
    // Felder, deren Cloud-Zustand in DIESER Verbindung schon einmal an den
    // Relay zurückging (die eine Ausnahme von Riegel 1, siehe
    // `score_mirror_frame`). Lebt je Verbindung — nach einem Relay-Neustart
    // beginnt es leer.
    let mut rueckgespiegelt: HashSet<i64> = HashSet::new();

    loop {
        tokio::select! {
            // Feste Reihenfolge, Lesen zuerst: Liegt ein Frame bereit, wird
            // er gelesen, BEVOR der Idle-Wächter urteilt. Bei zufälliger
            // Wahl beendete der Wächter die Sitzung nach jeder längeren
            // Pause der Schleife selbst — obwohl das Lebenszeichen des
            // Relays schon im Puffer lag.
            biased;
            incoming = read.next() => {
                // Strom ohne Close-Frame zu Ende: ein Abriss.
                let Some(msg) = incoming else { return Ok(Schluss::Abriss) };
                let msg = msg.map_err(|e| e.to_string())?;
                // Lebenszeichen des Relays: deckt Text-Frame + Ping (+ alle
                // übrigen Frames) ab — der Read-Idle-Wächter braucht keinen
                // eigenen Client-Ping (Option A).
                last_incoming = tokio::time::Instant::now();
                match msg {
                    WsMessage::Text(t) => {
                        if let Ok(frame) = serde_json::from_str::<RelayFrame>(t.as_str()) {
                            handle_frame(ctx, frame, &tx, &mut last_match, &mut score_fp, dienste).await;
                        }
                    }
                    WsMessage::Ping(p) => { let _ = tx.send(WsMessage::Pong(p)); }
                    // Der Close-Code sagt, WARUM der Relay schließt (belegter
                    // Platz) — der Backoff unterscheidet danach.
                    WsMessage::Close(rahmen) => {
                        return Ok(Schluss::Close(rahmen.map(|r| u16::from(r.code))));
                    }
                    _ => {}
                }
            }
            grund = &mut schreiber => {
                return Err(grund.unwrap_or_else(|e| format!("Schreiber-Task beendet: {e}")));
            }
            _ = ticker.tick() => {
                // Fernbefehl an die Cloud-Tablets weiterreichen (Spec
                // `tablet-version-abgleich`). Der Relay setzt ihn in ein
                // `Reload` an jede seiner Tablet-Verbindungen um — mit
                // **seiner** Seiten-Marke, denn die Cloud-Tablets haben die
                // Seite von ihm, nicht von hier.
                if reload_wacht.faellig(&ctx.tablet) {
                    let _ = tx.send(text(&HostFrame::ReloadTablets));
                }
                push_all_courts(ctx, &tx, &mut last_match, &mut score_fp);
                // Score-Spiegel-Sweep NACH den Zuweisungen (gleiche FIFO-Wire
                // → der Relay kennt die Matches, bevor der Spiegel eintrifft).
                // Fängt drei nudge-lose Fälle mit ≤2 s Verzug ein: Reconnect/
                // Relay-Neustart (leerer Cache dort), Court-Wechsel (FP eben
                // invalidiert) und BTP-getriebene Stände ohne Tablet
                // (Handeingabe in BTP nudgt nicht). Dank Fingerabdruck sendet
                // ein Tick ohne Änderung nichts.
                for court in ctx.tablet.courts() {
                    if let Some(frame) = score_mirror_frame(&ctx.tablet, court.id, &mut score_fp, &mut rueckgespiegelt) {
                        let _ = tx.send(text(&frame));
                    }
                }
                push_freetext(ctx, &tx, &mut last_freetext);
                // Zugänge zuerst: Der Zustand nützt nichts, solange der Relay
                // niemanden kennt, dem er ihn zeigen dürfte.
                push_tl_auth(ctx, &tx, &mut tl_auth_fp);
                push_tl_state(ctx, &tx, &mut tl_state_rev);
                push_meetingpoint(ctx, &tx, &mut meetingpoint_fp);
            }
            _ = monitor_ticker.tick() => {
                upload_anstossen(ctx, install_id, &monitor_fp, dienste);
                // Feld-Liste fürs Cloud-Feldwechsel-Menü mitschicken (selten
                // veränderlich; erster Tick feuert sofort nach dem Verbinden).
                push_courts(ctx, &tx);
                // Aufgerufene Spiele für die Slave-Spielübersicht + den Nachruf
                // am Slave (Cluster C Stufe 2) — nur bei Änderung senden.
                push_prepared(ctx, &tx, &mut prepared_fp);
            }
            _ = control_ticker.tick() => {
                steuerung_anstossen(ctx, install_id, &control_fp, dienste);
            }
            nudge = nudge_rx.recv() => {
                // Feld hat sich geändert (Punkt, Zustand, Match): Stand
                // spiegeln, falls er sich fürs Relay wirklich unterscheidet.
                let court_id = nudge
                    .and_then(|n| serde_json::from_str::<MonitorNudge>(&n).ok())
                    .map(|n| n.court);
                if let Some(court_id) = court_id {
                    if let Some(frame) = score_mirror_frame(&ctx.tablet, court_id, &mut score_fp, &mut rueckgespiegelt) {
                        let _ = tx.send(text(&frame));
                    }
                }
            }
            _ = idle_ticker.tick() => {
                // Half-open erkannt: seit RELAY_READ_IDLE kein Frame/Ping.
                // `Err` beendet die Sitzung → `run` öffnet einen frischen
                // Socket (nach einer stabilen Sitzung nach rund 1 s). Kein
                // Client-Ping.
                if is_stale(last_incoming, tokio::time::Instant::now(), RELAY_READ_IDLE) {
                    return Err("Relay-Read-Idle: 15 s ohne Antwort → reconnect".into());
                }
            }
        }
    }
}

/// Stößt den Upload des Court-Monitor-Datensatzes (Werbung +
/// Anzeige-Konfiguration) an, falls er sich seit dem letzten erfolgreichen
/// Upload dieser Sitzung geändert hat. Ein Fingerabdruck (Konfiguration +
/// Werbebild-Namen/Größen/Zeiten) erspart unnötige Uploads der Bilddaten.
///
/// Der Upload läuft in einem eigenen Task: Bis zu 12 MB über einen
/// schmalen Uplink dürfen weder Punkte noch Pings aufhalten. Der Wächter
/// lässt höchstens einen zugleich zu und bremst nach Fehlschlägen.
fn upload_anstossen(
    ctx: &Arc<ServerCtx>,
    install_id: &str,
    zuletzt: &Arc<Mutex<Option<String>>>,
    dienste: &Dienste,
) {
    let fp = monitor_fingerprint(ctx);
    if zuletzt.lock().unwrap().as_deref() == Some(fp.as_str()) {
        return;
    }
    let Some(lauf) = dienste.upload.beginnen(tokio::time::Instant::now()) else {
        return;
    };
    let ctx = ctx.clone();
    let install_id = install_id.to_string();
    let zuletzt = zuletzt.clone();
    dienste.im_hintergrund(async move {
        let mut kurz = 0;
        let ergebnis = loop {
            match upload_monitor(&ctx, &install_id).await {
                Err(e) if upload_gleich_wieder(&e) && kurz < UPLOAD_KURZ_VERSUCHE => {
                    kurz += 1;
                    tracing::info!("Court-Monitor-Upload: {e} – neuer Versuch in 3 s");
                    tokio::time::sleep(UPLOAD_KURZ_PAUSE).await;
                }
                ergebnis => break ergebnis,
            }
        };
        let ok = ergebnis.is_ok();
        match ergebnis {
            Ok(()) => {
                tracing::info!("Court-Monitor-Datensatz zum Relay hochgeladen");
                *zuletzt.lock().unwrap() = Some(fp);
            }
            Err(e) => tracing::warn!("Court-Monitor-Upload fehlgeschlagen: {e}"),
        }
        lauf.fertig(ok, tokio::time::Instant::now());
    });
}

/// Stößt den Abgleich der Monitor-Geräte-Steuerung an — in einem eigenen
/// Task, höchstens einer zugleich (zwei HTTP-Anfragen nacheinander, die bei
/// zähem Netz je bis zur Frist des Clients dauern).
fn steuerung_anstossen(
    ctx: &Arc<ServerCtx>,
    install_id: &str,
    zuletzt: &Arc<Mutex<String>>,
    dienste: &Dienste,
) {
    let Some(lauf) = dienste.steuerung.beginnen(tokio::time::Instant::now()) else {
        return;
    };
    let ctx = ctx.clone();
    let install_id = install_id.to_string();
    let zuletzt = zuletzt.clone();
    dienste.im_hintergrund(async move {
        sync_monitor_control(&ctx, &install_id, &zuletzt).await;
        lauf.fertig(true, tokio::time::Instant::now());
    });
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
    let stream = monitor::read_ad_stream(&ctx.monitor_dir.join(monitor::AD_STREAM_FILE));
    let stile = monitor::read_ad_style(&ctx.monitor_dir.join(monitor::AD_STYLE_FILE));
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
        s.push_str(&ad_abdruck_zeile(
            &name,
            len,
            mtime,
            bar.contains(&name),
            stream.contains(&name),
            stile.get(&name),
        ));
    }
    s
}

/// Die Abdruck-Zeile eines Werbebilds.
///
/// Drei Dinge stehen hier, die man leicht vergisst und deren Fehlen still
/// bleibt: die **Leisten-Markierung** (sonst zeigt die Cloud-Leiste die alte
/// Auswahl), die **Stream-Markierung** (sonst zeigt die Cloud-Stream-Werbung
/// die alte Auswahl, ADR 0092) und der **Anzeige-Stil** (sonst bleibt der Cloud-Monitor für immer
/// schwarz — eine reine Farbänderung fasst weder Datei noch Verzeichnis an,
/// `maybe_upload_monitor` würde also nie auslösen, ADR 0041).
fn ad_abdruck_zeile(
    name: &str,
    len: u64,
    mtime: u64,
    in_bar: bool,
    in_stream: bool,
    stil: Option<&monitor::AdStyle>,
) -> String {
    // „Kein Eintrag" und „ausdrücklich die Vorgabe" sind derselbe Zustand und
    // müssen denselben Abdruck ergeben — sonst löste ein Stil, der nichts
    // ändert, einen Upload aus. `write_ad_style` filtert Vorgaben ohnehin
    // heraus; die Normalisierung hier macht den Abdruck davon unabhängig.
    let stil = stil.filter(|s| !s.ist_vorgabe());
    let bg = stil.map(|s| s.bg.as_str()).unwrap_or("");
    let zeigt_feld = stil.map(|s| s.show_court).unwrap_or(false);
    format!("|{name}:{len}:{mtime}:{in_bar}:{in_stream}:{bg}:{zeigt_feld}")
}

/// Baut den Court-Monitor-Datensatz und POSTet ihn zum Relay.
async fn upload_monitor(ctx: &ServerCtx, install_id: &str) -> Result<(), String> {
    let cfg = ctx.monitor_config();
    let bar = monitor::read_ad_bar(&ctx.monitor_dir.join(monitor::AD_BAR_FILE));
    let stream = monitor::read_ad_stream(&ctx.monitor_dir.join(monitor::AD_STREAM_FILE));
    let stile = monitor::read_ad_style(&ctx.monitor_dir.join(monitor::AD_STYLE_FILE));
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
            in_stream: stream.contains(&name),
            // Die Kontrastschrift rechnet der Host, nicht der Relay und nicht
            // die Anzeige (ADR 0041) — eine Quelle für den Kontrast.
            style: stile
                .get(&name)
                .map(|s| relay_proto::AdStyleWire {
                    bg: s.bg.clone(),
                    fg: monitor::schriftfarbe(&s.bg).to_string(),
                    show_court: s.show_court,
                })
                .unwrap_or_default(),
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
    let url = format!("{}/{install_id}/monitor", relay_http());
    let resp = ctx
        .http
        .post(&url)
        .timeout(UPLOAD_FRIST)
        .json(&upload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

/// Entfernt Ziele, die nur der Turnier-PC ausliefert (Spielfolge, Spec
/// spielfolge-anzeige), vor dem Upload. Ein Relay, das die Variante nicht
/// kennt, verwürfe sonst den **ganzen** Upload, und alle Cloud-Zuweisungen
/// frören ein. Im Cloud zeigt ein solches Gerät die Kopplungsseite — genau
/// das, was der Relay mit der Variante ohnehin täte.
fn ohne_nur_lan_ziele(
    mut targets: std::collections::HashMap<String, relay_proto::MonitorTarget>,
) -> std::collections::HashMap<String, relay_proto::MonitorTarget> {
    targets.retain(|_, t| !matches!(t, relay_proto::MonitorTarget::InfoSpielfolge { .. }));
    targets
}

/// Gleicht die Monitor-Geräte-Steuerung mit dem Relay ab: pusht die
/// Feld-Zuweisungen + Fernbefehle (nur bei Änderung) und holt die
/// aktuelle Geräteliste für die „Court-Monitore"-Seite.
async fn sync_monitor_control(ctx: &ServerCtx, install_id: &str, last_fp: &Mutex<String>) {
    // Volle Anzeige-Ziele in `targets` (Court, Info-Übersicht/Vorbereitung/
    // Sieger, Werbung, Kombi) — so kann der Cloud-Monitor auch Nicht-Court-
    // Sichten umleiten. `assignments` (nur CourtID) bleibt zusätzlich befüllt,
    // damit ein noch nicht aktualisiertes Relay wenigstens die Court-Ziele kennt.
    let targets = ohne_nur_lan_ziele(monitor::read_assignments(&ctx.assignments_path));
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
    let geaendert = fp != *last_fp.lock().unwrap();
    if geaendert {
        let url = format!("{}/{install_id}/monitor/control", relay_http());
        match ctx.http.post(&url).json(&control).send().await {
            Ok(r) if r.status().is_success() => *last_fp.lock().unwrap() = fp,
            Ok(r) => tracing::warn!("Monitor-Steuerung: HTTP {}", r.status()),
            Err(e) => tracing::warn!("Monitor-Steuerung fehlgeschlagen: {e}"),
        }
    }
    // Geräteliste vom Relay holen und im geteilten Zustand ablegen.
    let url = format!("{}/{install_id}/monitor-devices", relay_http());
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
    last_match: &mut HashMap<i64, Option<CourtPushKey>>,
    score_fp: &mut HashMap<i64, HostFrame>,
    dienste: &Dienste,
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
            // Nicht hier abwarten: Der Write nach BTP dauert bei trägem BTP
            // oder laufender TP-Kopie 15 s und mehr. Der Arbeiter des Felds
            // schreibt und quittiert an DIESE Sitzung (ADR 0091).
            dienste.ergebnisse.einreihen(
                ctx,
                ErgebnisAuftrag {
                    req_id,
                    body,
                    antwort: tx.clone(),
                },
            );
        }
        // Meetingpoint-Check-in aus der Cloud: dieselbe Prüfung wie im
        // Hallennetz (ADR 0074, R5-Analogie). Rein lokal, kein BTP-Write.
        RelayFrame::MeetingpointCheckin { req_id, body } => {
            // In einem eigenen Task wie `TlCommand`: Der Check-in klont den
            // Stand und schreibt auf die Platte — diese Schleife bedient aber
            // alle Tablets der Halle und beantwortet die Pings (Security-
            // Review M2).
            let ctx = ctx.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let config = ctx.app_config();
                let result = crate::tablet::meetingpoint::checkin(
                    &ctx.tablet,
                    &config.meetingpoint,
                    &body,
                    monitor::now_ms(),
                );
                let _ = tx.send(text(&HostFrame::MeetingpointAck {
                    req_id,
                    ok: result.is_ok(),
                    error: result.err(),
                }));
            });
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
            // Kompletter Abgleich: auch noch kurz nach dem Spielende
            // (Regel D der Spec tablet-stand-robust).
            ctx.tablet
                .verlauf_sync_annehmen(court_id, match_id, timeline);
        }
        // On-Demand-Abruf der TL-Oberfläche (AK-5): Antwort direkt aus dem
        // Store — winzige Payload, kein BTP-Zugriff, deshalb anders als
        // TlCommand ohne eigenen Task.
        // Zettel-Ereignisse (ADR 0037): gleicher Filter wie beim
        // Punktverlauf — der Frame muss zum aktuellen Court-Match passen.
        // Die Halter-Prüfung übernimmt im Cloud-Modus der Relay (eine
        // aktive Tablet-Session je Court, R4), wie beim Rally-Weg.
        RelayFrame::MatchEvent {
            court_id,
            match_id,
            event,
        } => {
            if ctx
                .tablet
                .match_for_court(court_id)
                .is_some_and(|m| m.id == match_id)
            {
                ctx.tablet.sheet_store().apply_event(match_id, event);
            }
        }
        // Zustand eines Cloud-Tablets (Spec tablet-stand-robust, „über
        // Kreuz"): mit Stale-Filter und Riegel 3 in `uebernimm_cloud_stand`.
        // Die Halter-Prüfung hat der Relay gemacht (R4), wie bei den
        // übrigen Tablet-Frames.
        RelayFrame::StateSync {
            court_id,
            match_id,
            state,
        } => {
            ctx.tablet.uebernimm_cloud_stand(court_id, match_id, state);
        }
        RelayFrame::MatchEventSync {
            court_id,
            match_id,
            events,
        } => {
            ctx.tablet
                .ereignis_sync_annehmen(court_id, match_id, events);
        }
        // Zettel-Abruf eines TL-Geräts (ADR 0039): Antwort direkt aus den
        // Stores, kein BTP-Zugriff — aber bis zu 40 Blätter sind spürbare
        // Rechenarbeit, deshalb abseits der Frame-Schleife.
        RelayFrame::ScoresheetRequest {
            req_id,
            match_ids,
            vorab,
        } => {
            let ctx = ctx.clone();
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let logo =
                    crate::tablet::scoresheet::logo_data_uri(&ctx.app_config_arc().tournament_logo);
                let modus = if vorab {
                    crate::tablet::scoresheet::Modus::Vorab
                } else {
                    crate::tablet::scoresheet::Modus::Normal
                };
                let html = crate::tablet::scoresheet::html_fuer(
                    &ctx.tablet,
                    logo.as_deref(),
                    modus,
                    &match_ids,
                );
                let _ = tx.send(text(&HostFrame::ScoresheetData {
                    req_id,
                    found: html.is_some(),
                    html: html.unwrap_or_default(),
                }));
            });
        }
        RelayFrame::TimelineRequest { req_id, match_id } => {
            let json = ctx.tablet.timeline_store().timeline_json(match_id);
            let _ = tx.send(text(&HostFrame::TimelineData {
                req_id,
                found: json.is_some(),
                json: json.unwrap_or_default(),
            }));
        }
        // Ebenso on-demand: Sperrlisten und Einsätze eines Schiedsrichters.
        // Diese Personendaten verlassen den Host nur auf gezielte Anfrage
        // eines gekoppelten Geräts — nie im Broadcast-Zustand.
        RelayFrame::OfficialDetailRequest {
            req_id,
            official_id,
        } => {
            let json = crate::tablet::tl::official_detail_json(&ctx.tablet, official_id);
            let _ = tx.send(text(&HostFrame::OfficialDetail { req_id, json }));
        }
        RelayFrame::TlSearchRequest {
            req_id,
            query,
            candidates,
        } => {
            // Abseits der Frame-Schleife rechnen: Die Suche läuft über den
            // ganzen Bestand und soll Punktstände nicht aufhalten (Spec
            // `tl-spielersuche`). Fehler (Begriff ungültig, belegt) reisen als
            // `{"error":…}`, der Relay macht 400/429 daraus. Den Begriff nie
            // protokollieren.
            let ctx = ctx.clone();
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let cfg = ctx.app_config_arc();
                let json = crate::tablet::suche::search_json_begrenzt(
                    &ctx.tablet,
                    &cfg,
                    &query,
                    candidates,
                    crate::tablet::monitor::now_ms(),
                )
                .unwrap_or_else(|f| f.relay_json());
                let _ = tx.send(text(&HostFrame::TlSearchResult { req_id, json }));
            });
        }
    }
}

/// Push-Schlüssel je Feld: Ändert sich einer seiner Teile, geht die
/// Zuweisung erneut an den Relay. (Match-ID, finalisiert, Besetzung,
/// Setzplatz Team 1, Setzplatz Team 2) — die Setzung gehört dazu, damit eine
/// Neusetzung in BTP ein schon belegtes Feld in der Cloud erreicht
/// (Spec setzplaetze-anzeigen, ADR 0059).
type CourtPushKey = (i64, bool, String, Option<String>, Option<String>);

/// Schiebt das aktuelle Match eines Felds (per CourtID) ans Tablet – nur,
/// wenn es sich gegenüber dem zuletzt gemeldeten Stand geändert hat.
/// `court_label` ist die Anzeige-Bezeichnung des Felds (bei Mehr-Hallen-
/// Turnieren mit Hallen-Präfix, z. B. „Halle 2 · 6").
fn push_court(
    ctx: &ServerCtx,
    court_id: i64,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_match: &mut HashMap<i64, Option<CourtPushKey>>,
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
    // Schlüssel inkl. Besetzung (siehe `server::push_match`): Eine
    // Zuweisung mitten im Spiel ändert die matchId nicht, muss das Tablet
    // aber erreichen — im Cloud-Weg genauso wie im LAN.
    let key = effective.as_ref().map(|m| {
        (
            m.id,
            finalized,
            crate::tablet::server::officials_key(&ctx.tablet.match_officials(m)),
            m.team1_seed.clone(),
            m.team2_seed.clone(),
        )
    });
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
                    let cfg = ctx.app_config();
                    let mut brief = match_brief(
                        &m,
                        sk,
                        ska,
                        &cfg.display,
                        finalized,
                        ctx.tablet.match_officials(&m),
                    );
                    brief.feld_check =
                        cfg.meetingpoint.enabled && cfg.meetingpoint.field_check_enabled;
                    brief
                },
                // Autoritativer 1.-Aufruf-Zeitstempel vom Host (gleiche Quelle
                // wie die Spielübersicht) – auch bei Reconnect identisch.
                on_court_since_ms: ctx.tablet.on_court_since_ms(court_id, m.id),
                // Wie im LAN (`server::push_match`): die Sätze des Hosts,
                // leer beim Legacy-Schalter.
                host_sets: ctx.tablet.host_sets(
                    court_id,
                    m.id,
                    !crate::tablet::server::ownership_active(&ctx.app_config()),
                ),
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
    last_match: &mut HashMap<i64, Option<CourtPushKey>>,
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
        reqwest::Url::parse(&format!("{}/{namespace}/info/announce/state", relay_http())).ok()?;
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
    let url = format!("{}/{namespace}/slaves", relay_http());
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
    let url = format!("{}/{namespace}/pairing-code", relay_http());
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
    let url = format!("{}/pair/{code}", relay_http());
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
    let url = format!("{}/{namespace}/courts", relay_http());
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
    let mut courts: Vec<CourtBrief> = ctx
        .tablet
        .courts()
        .into_iter()
        .map(|c| CourtBrief {
            id: c.id,
            label: ctx.tablet.court_display_label(c.id),
            // Rohe Halle (BTP-Location) mitschicken – der Cloud-Ansage-Slave
            // filtert damit auf die Felder seiner Halle (Geräte-Anschluss).
            hall: ctx.tablet.court_hall(c.id),
            hall_color: None,
        })
        .collect();
    // Hallen-Farben (Spec hallen-farben): einmal über die kanonische
    // Turnier-Hallenliste auflösen (nicht über die Court-Locations —
    // Review 2026-08-16), an jede Zeile hängen; der Relay reicht sie an
    // Cloud-Monitore/-Übersicht weiter.
    let farben =
        crate::hall_colors::effective_hall_colors(&ctx.app_config(), &ctx.tablet.hall_names());
    for c in &mut courts {
        c.hall_color = crate::hall_colors::farbe_fuer(&farben, &c.hall);
    }
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
    cfg: &crate::config::AppConfig,
) -> Vec<PreparedMatch> {
    // Hallen-Farben (Spec hallen-farben) einmal je Liste auflösen — die
    // Hallenliste des Turniers kommt aus dem Snapshot.
    let hallen: Vec<String> = snapshot.locations.iter().map(|l| l.name.clone()).collect();
    let hallen_farben = crate::hall_colors::effective_hall_colors(cfg, &hallen);
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
            let hall_color = crate::hall_colors::farbe_fuer(&hallen_farben, &hall);
            Some(PreparedMatch {
                match_id: m.id,
                hall,
                hall_color,
                discipline: m.discipline.as_str().to_string(),
                class_label: m.class_label.clone(),
                round_name: m.round_name.clone(),
                team_a: players(&m.team1),
                team_b: players(&m.team2),
                match_number: m.match_num,
                called_at_ms: call.called_at_ms,
                seed_a: m.team1_seed.clone(),
                seed_b: m.team2_seed.clone(),
            })
        })
        .collect()
}

/// Pusht den Meetingpoint-Stand aller Hallen an den Relay — nur bei
/// Änderung. Der Relay schneidet ihn je Seite auf die Halle zu.
fn push_meetingpoint(
    ctx: &ServerCtx,
    tx: &mpsc::UnboundedSender<WsMessage>,
    last_fp: &mut Option<String>,
) {
    let config = ctx.app_config();
    let snapshot = ctx.tablet.snapshot_clone();
    let view = crate::tablet::meetingpoint::view_alle_hallen(
        &ctx.tablet,
        snapshot.as_ref(),
        &config.meetingpoint,
    );
    let fp = serde_json::to_string(&view).unwrap_or_default();
    if last_fp.as_deref() == Some(fp.as_str()) {
        return;
    }
    *last_fp = Some(fp);
    let _ = tx.send(text(&HostFrame::Meetingpoint { view }));
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
    let prepared = build_prepared_list(
        &snapshot,
        &ctx.tablet.preparation_calls(),
        &ctx.app_config(),
    );
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
    azure_share_aus(&az)
}

/// Rein & testbar: Share nur, wenn der GEWÄHLTE Dienst vollständig ist, und
/// darin nur dessen Zugangsdaten (Spec `ansage-elevenlabs`, Review 03.11.2026):
/// Die Share ist über den Namespace lesbar — ein eingetragener, aber gerade
/// nicht genutzter Schlüssel soll nicht mitreisen. Bei ElevenLabs/OpenAI/
/// Gemini bleiben auch die Azure-Felder leer; ein alter Slave sieht dann
/// keine Stimme und bleibt still bei seiner Ersatzstimme statt bei jeder
/// Ansage einen Azure-Fehler zu melden.
fn azure_share_aus(az: &crate::config::AzureTtsConfig) -> Option<relay_proto::AzureTtsShare> {
    use crate::config::{dienst_art, DienstArt};
    if !az.vollstaendig() {
        return None;
    }
    let mut s = relay_proto::AzureTtsShare {
        dienst: az.dienst.clone(),
        teile: az.teile,
        ..Default::default()
    };
    match dienst_art(&az.dienst) {
        DienstArt::Azure => {
            s.region = az.region.clone();
            s.key = az.key.clone();
            s.voice = az.voice.clone();
            s.discipline_voices = az.discipline_voices.clone();
        }
        DienstArt::ElevenLabs(_) => {
            s.elevenlabs_key = az.elevenlabs_key.clone();
            s.elevenlabs_voice = az.elevenlabs_voice.clone();
        }
        DienstArt::OpenAi(_) => {
            s.openai_key = az.openai_key.clone();
            s.openai_voice = az.openai_voice.clone();
        }
        DienstArt::Gemini(_) => {
            s.gemini_key = az.gemini_key.clone();
            s.gemini_voice = az.gemini_voice.clone();
        }
    }
    Some(s)
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
///
/// **Über Kreuz** (Spec tablet-stand-robust, ADR 0083): Ein Zustand, der
/// selbst vom Relay kam, reist nicht zurück (Riegel 1 in
/// `score_mirror_of`) — mit einer Ausnahme: **einmal je Feld und
/// Relay-Verbindung**. Nach einem Relay-Neustart ist dessen Kopie weg; der
/// Host gibt sie so zurück, und ein Ersatzgerät bekommt Undo und Verlauf
/// statt nur der Sätze. Hält dort ein Cloud-Tablet das Feld, verwirft der
/// Relay sie (Riegel 2). `rueckgespiegelt` merkt, für welche Felder das in
/// dieser Verbindung schon geschehen ist.
fn score_mirror_frame(
    tablet: &TabletState,
    court_id: i64,
    last: &mut HashMap<i64, HostFrame>,
    rueckgespiegelt: &mut HashSet<i64>,
) -> Option<HostFrame> {
    let mut mirror = tablet.score_mirror_of(court_id)?;
    if mirror.state.is_none()
        && tablet.court_state_quelle(court_id) == Some(crate::tablet::state::StandQuelle::Cloud)
        && rueckgespiegelt.insert(court_id)
    {
        mirror.state = tablet.court_state(court_id);
    }
    let frame = HostFrame::ScoreUpdate {
        court_id,
        match_id: mirror.match_id,
        sets: mirror
            .sets
            .into_iter()
            .map(|(a, b)| SetAb { a, b })
            .collect(),
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

    #[test]
    fn spielfolge_ziel_reist_nicht_zum_relay() {
        let mut t = std::collections::HashMap::new();
        t.insert("a".to_string(), relay_proto::MonitorTarget::court(1));
        t.insert(
            "b".to_string(),
            relay_proto::MonitorTarget::InfoSpielfolge {
                hall: None,
                anordnung: None,
                anzahl: None,
            },
        );
        let rest = super::ohne_nur_lan_ziele(t);
        assert_eq!(rest.len(), 1);
        assert!(rest.contains_key("a"));
    }

    #[test]
    fn share_traegt_nur_die_zugangsdaten_des_gewaehlten_dienstes() {
        let mut az = crate::config::AzureTtsConfig {
            enabled: true,
            region: "we".into(),
            key: "az".into(),
            elevenlabs_key: "el".into(),
            openai_key: "oa".into(),
            gemini_key: "gm".into(),
            ..Default::default()
        };
        let s = super::azure_share_aus(&az).unwrap();
        assert_eq!((s.key.as_str(), s.region.as_str()), ("az", "we"));
        assert!(s.elevenlabs_key.is_empty() && s.openai_key.is_empty() && s.gemini_key.is_empty());

        az.dienst = "elevenlabs_v3".into();
        let s = super::azure_share_aus(&az).unwrap();
        assert_eq!(s.elevenlabs_key, "el");
        // Kein Azure mehr: alter Slave sieht keine Stimme → kein Fehlerbanner.
        assert!(s.key.is_empty() && s.region.is_empty() && s.voice.is_empty());
        assert!(s.openai_key.is_empty() && s.gemini_key.is_empty());

        az.dienst = "gemini_38_lite".into();
        let s = super::azure_share_aus(&az).unwrap();
        assert_eq!(s.gemini_key, "gm");
        assert!(s.elevenlabs_key.is_empty() && s.key.is_empty());

        // Gewählt, aber ohne Schlüssel → gar keine Share.
        az.gemini_key.clear();
        assert!(super::azure_share_aus(&az).is_none());
    }

    use super::*;

    #[test]
    fn hinweis_anderer_pc_verschwindet_mit_der_uebertragung() {
        // Die Verdrahtung: Der Merker lebt im `TabletState` (über Start und
        // Stopp hinweg) — endet `run`, setzt die Wacht ihn zurück.
        let tablet = Arc::new(TabletState::default());
        assert_eq!(tablet.cloud_hinweis(), None);
        {
            let _wacht = LageWacht(tablet.clone());
            // Seit einer Minute abgewiesen.
            let vor_einer_minute = monitor::now_ms() - 60_000;
            tablet
                .cloud_lage_aendern(|l| l.sitzung_ende(SitzungsEnde::Abgewiesen, vor_einer_minute));
            assert_eq!(tablet.cloud_hinweis(), Some(ws_verbindung::ANDERER_PC_TEXT));
        }
        assert_eq!(tablet.cloud_hinweis(), None, "nach dem Stoppen weg");
    }

    #[test]
    fn upload_wird_nur_bei_409_und_503_gleich_wiederholt() {
        // So meldet `upload_monitor` einen abgelehnten Upload.
        let text = |code: u16| format!("HTTP {}", reqwest::StatusCode::from_u16(code).unwrap());
        assert!(
            upload_gleich_wieder(&text(409)),
            "Turnier-PC noch nicht eingetragen"
        );
        assert!(upload_gleich_wieder(&text(503)), "Relay beschäftigt");
        // Alles andere wartet die gewöhnliche Pause: Bilder-Budget voll,
        // Upload zu groß, Netzfehler.
        assert!(!upload_gleich_wieder(&text(507)));
        assert!(!upload_gleich_wieder(&text(413)));
        assert!(!upload_gleich_wieder(&text(500)));
        assert!(!upload_gleich_wieder("error sending request"));
    }

    use crate::btp::model::{
        BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult, MatchStatus, ScoringFormat,
    };
    use crate::config::AppConfig;
    use crate::tablet::state::TabletState;
    use std::collections::HashMap;

    #[test]
    fn der_abdruck_bemerkt_eine_reine_farbaenderung() {
        // Datei unverändert, nur die Farbe anders: Der Abdruck MUSS sich
        // ändern, sonst lädt der Host nie neu hoch und der Cloud-Monitor
        // bleibt schwarz (ADR 0041).
        let ohne = ad_abdruck_zeile("ad-1.png", 100, 42, false, false, None);
        let schwarz = monitor::AdStyle::default();
        let weiss = monitor::AdStyle {
            bg: "#ffffff".to_string(),
            show_court: false,
        };
        let mit_feld = monitor::AdStyle {
            bg: "#ffffff".to_string(),
            show_court: true,
        };
        let a = ad_abdruck_zeile("ad-1.png", 100, 42, false, false, Some(&schwarz));
        let b = ad_abdruck_zeile("ad-1.png", 100, 42, false, false, Some(&weiss));
        let c = ad_abdruck_zeile("ad-1.png", 100, 42, false, false, Some(&mit_feld));
        assert_ne!(a, b, "Farbwechsel muss den Abdruck ändern");
        assert_ne!(b, c, "Feld-Häkchen muss den Abdruck ändern");
        // Kein Eintrag und ausdrücklich die Vorgabe sind derselbe Zustand —
        // beides heißt „nichts eingestellt".
        assert_eq!(
            ohne,
            ad_abdruck_zeile("ad-1.png", 100, 42, false, false, Some(&schwarz)),
            "Vorgabe darf keinen Upload auslösen"
        );
    }

    #[test]
    fn der_abdruck_bemerkt_ein_stream_haekchen() {
        // Ein reines Umhaken fasst weder Datei noch Verzeichnis an. Steht das
        // Flag nicht im Abdruck, zeigt die Cloud für immer die alte Auswahl
        // (ADR 0092).
        let aus = ad_abdruck_zeile("ad-1.png", 100, 42, false, false, None);
        let an = ad_abdruck_zeile("ad-1.png", 100, 42, false, true, None);
        assert_ne!(aus, an);
        // Und unabhängig von der Leiste.
        let leiste = ad_abdruck_zeile("ad-1.png", 100, 42, true, false, None);
        assert_ne!(an, leiste);
    }

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
        let mut rueck = HashSet::new();

        // Erster Stand (noch kein Tablet): Match bekannt, Sätze leer.
        match score_mirror_frame(&tablet, 101, &mut last, &mut rueck)
            .expect("erster Stand wird gemeldet")
        {
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
        assert!(score_mirror_frame(&tablet, 101, &mut last, &mut rueck).is_none());

        // Das Tablet zählt → der neue Stand wird genau einmal gemeldet.
        tablet.record_score(101, 7, vec![(11, 9)]);
        match score_mirror_frame(&tablet, 101, &mut last, &mut rueck)
            .expect("Änderung wird gemeldet")
        {
            HostFrame::ScoreUpdate { sets, .. } => {
                assert_eq!(sets, vec![relay_proto::SetAb { a: 11, b: 9 }]);
            }
            f => panic!("unerwartetes Frame: {f:?}"),
        }
        assert!(score_mirror_frame(&tablet, 101, &mut last, &mut rueck).is_none());

        // Feld ohne Match → nichts zu senden.
        assert!(score_mirror_frame(&tablet, 999, &mut last, &mut rueck).is_none());
    }

    /// Spec tablet-stand-robust, „über Kreuz" (AK3): Ein Zustand, der aus
    /// der Cloud kam, geht genau EINMAL je Feld und Relay-Verbindung zurück
    /// — nach einem Relay-Neustart ist dessen Kopie weg. Danach gilt Riegel 1.
    #[test]
    fn erster_spiegel_nach_relay_neustart_traegt_stand() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snapshot(vec![match_on_court(7, 101)]));
        tablet.record_score(101, 7, vec![(3, 2)]);
        let stand = r#"{"match":{"matchId":7},"rev":9,"history":[1]}"#;
        assert!(tablet.uebernimm_cloud_stand(101, 7, stand.to_string()));
        let zustand = |f: Option<HostFrame>| match f {
            Some(HostFrame::ScoreUpdate { state, .. }) => state,
            anderes => panic!("ScoreUpdate erwartet: {anderes:?}"),
        };

        // Neue Verbindung: der erste Spiegel trägt den vollen Stand.
        let mut last = HashMap::new();
        let mut rueck = HashSet::new();
        assert_eq!(
            zustand(score_mirror_frame(&tablet, 101, &mut last, &mut rueck)).as_deref(),
            Some(stand)
        );
        // Der nächste Punkt: nur noch die Sätze — kein Echo.
        tablet.record_score(101, 7, vec![(4, 2)]);
        assert_eq!(
            zustand(score_mirror_frame(&tablet, 101, &mut last, &mut rueck)),
            None
        );
        tablet.record_score(101, 7, vec![(5, 2)]);
        assert_eq!(
            zustand(score_mirror_frame(&tablet, 101, &mut last, &mut rueck)),
            None
        );

        // Relay-Neustart = neue Verbindung, frische Merker: wieder einmal.
        let mut last = HashMap::new();
        let mut rueck = HashSet::new();
        assert_eq!(
            zustand(score_mirror_frame(&tablet, 101, &mut last, &mut rueck)).as_deref(),
            Some(stand)
        );
        tablet.record_score(101, 7, vec![(6, 2)]);
        assert_eq!(
            zustand(score_mirror_frame(&tablet, 101, &mut last, &mut rueck)),
            None
        );

        // Ein LAN-Zustand reist wie eh und je bei jeder Änderung mit.
        tablet.set_court_state(101, r#"{"rev":10}"#.to_string());
        assert_eq!(
            zustand(score_mirror_frame(&tablet, 101, &mut last, &mut rueck)).as_deref(),
            Some(r#"{"rev":10}"#)
        );
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
            highlight: 0,
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
            pause_ms: None,
            preparation_call_ts: None,
            preparation_hall: None,
            preparation_call_rank: None,
            official1_id: None,
            official2_id: None,
            scoring: ScoringFormat::default(),
            team1_seed: None,
            team2_seed: None,
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
            Arc::new(std::sync::Mutex::new(AppConfig::default())),
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

        let prepared = build_prepared_list(&snap, &calls, &crate::config::AppConfig::default());
        assert_eq!(prepared.len(), 1, "nur das eine ruf-bare Spiel");
        assert_eq!(prepared[0].match_id, 42);
        assert_eq!(prepared[0].hall, "Halle 2", "LocationID → Hallenname");
        assert_eq!(
            prepared[0].hall_color, None,
            "eine Halle im Snapshot → farblos (Ein-Hallen-Gate)"
        );
        assert_eq!(prepared[0].team_a.len(), 1);
        assert_eq!(prepared[0].team_b.len(), 1);
    }

    #[test]
    fn build_prepared_list_carries_the_hall_color_in_multi_hall() {
        // Spec hallen-farben: Der Cloud-Aushang bekommt die Farbe des
        // Aufrufs mit — alphabetische Auto-Palette („Halle 1" → Ton 0,
        // „Halle 2" → Ton 1).
        use crate::btp::model::BtpLocation;
        use crate::tablet::state::PreparationCall;

        let mut snap = snapshot(vec![scheduled_match(42)]);
        snap.locations = vec![
            BtpLocation {
                id: 4,
                name: "Halle 1".into(),
            },
            BtpLocation {
                id: 5,
                name: "Halle 2".into(),
            },
        ];
        let calls = vec![PreparationCall {
            match_id: 42,
            location_id: Some(5),
            called_at_ms: 1_700_000_000_000,
        }];
        let prepared = build_prepared_list(&snap, &calls, &crate::config::AppConfig::default());
        assert_eq!(
            prepared[0].hall_color.as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[1])
        );
    }

    /// Cloud-Feld-Diffing: erster Push meldet die Zuweisung, ein unveränderter
    /// Stand wird dedupliziert (kein doppelter Push → kein Tablet-Reset), und
    /// verlässt das Match das Feld, kommt genau eine Aufhebung.
    #[test]
    fn push_court_sends_once_dedups_then_clears() {
        let ctx = ctx_with(vec![match_on_court(42, 101)]);
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        let mut last: HashMap<i64, Option<CourtPushKey>> = HashMap::new();
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

    /// Eine Neusetzung in BTP ändert die Match-ID nicht, muss ein schon
    /// belegtes Feld in der Cloud aber erreichen (Spec setzplaetze-anzeigen,
    /// B5): Der Push-Schlüssel enthält die Setzung, der Brief trägt sie.
    #[test]
    fn push_court_sendet_bei_neusetzung_erneut() {
        let mut m = match_on_court(42, 101);
        m.team1_seed = Some("1".into());
        let ctx = ctx_with(vec![m.clone()]);
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        let mut last: HashMap<i64, Option<CourtPushKey>> = HashMap::new();
        let mut score_fp: HashMap<i64, HostFrame> = HashMap::new();

        push_court(&ctx, 101, &tx, &mut last, &mut score_fp);
        let f1 = text_of(&rx.try_recv().expect("Zuweisung erwartet"));
        assert!(
            f1.contains("\"seedA\":\"1\""),
            "Brief trägt die Setzung: {f1}"
        );
        assert!(
            !f1.contains("seedB"),
            "ungesetzte Seite fehlt auf der Wire: {f1}"
        );

        // Gleicher Stand → kein Push.
        push_court(&ctx, 101, &tx, &mut last, &mut score_fp);
        assert!(rx.try_recv().is_err(), "kein Push ohne Änderung");

        // Neusetzung → erneuter Push mit dem neuen Wert.
        m.team1_seed = Some("3/4".into());
        ctx.tablet.set_snapshot(snapshot(vec![m]));
        push_court(&ctx, 101, &tx, &mut last, &mut score_fp);
        let f2 = text_of(&rx.try_recv().expect("erneuter Push erwartet"));
        assert!(f2.contains("\"type\":\"match_assigned\""), "war: {f2}");
        assert!(f2.contains("\"seedA\":\"3/4\""), "neue Setzung: {f2}");
    }

    /// Die Vorbereitungsliste für die Cloud trägt die Setzung mit; eine
    /// ungesetzte Seite fehlt auf der Wire.
    #[test]
    fn vorbereitungsliste_traegt_die_setzung() {
        use crate::tablet::state::PreparationCall;
        let mut m = scheduled_match(42);
        m.team2_seed = Some("5/8".into());
        let snap = snapshot(vec![m]);
        let calls = vec![PreparationCall {
            match_id: 42,
            location_id: None,
            called_at_ms: 1_700_000_000_000,
        }];
        let prepared = build_prepared_list(&snap, &calls, &crate::config::AppConfig::default());
        assert_eq!(prepared[0].seed_a, None);
        assert_eq!(prepared[0].seed_b.as_deref(), Some("5/8"));
        let json = serde_json::to_string(&prepared[0]).unwrap();
        assert!(json.contains("\"seedB\":\"5/8\""), "{json}");
        assert!(!json.contains("seedA"), "{json}");
    }

    // ── Host-Schleife entkoppelt (Spec relay-haerten, PR 1, ADR 0091) ──────

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Kontext mit zwei bekannten Feldern (CourtID 1 und 2).
    fn ctx_zwei_felder() -> Arc<ServerCtx> {
        let ctx = ctx_with(vec![]);
        let mut snap = snapshot(vec![]);
        snap.court_infos = vec![
            crate::btp::model::BtpCourt {
                id: 1,
                name: "1".into(),
                location_id: None,
                sort_order: 1,
            },
            crate::btp::model::BtpCourt {
                id: 2,
                name: "2".into(),
                location_id: None,
                sort_order: 2,
            },
        ];
        ctx.tablet.set_snapshot(snap);
        Arc::new(ctx)
    }

    fn ergebnis(court_id: i64, match_id: i64) -> ResultBody {
        serde_json::from_value(serde_json::json!({
            "matchId": match_id, "courtId": court_id, "sets": []
        }))
        .unwrap()
    }

    /// Was die Attrappe des Ergebnis-Verarbeiters gesehen hat.
    #[derive(Default)]
    struct Mess {
        laufend: AtomicUsize,
        hoechstens: AtomicUsize,
        folge: Mutex<Vec<i64>>,
        fertig: Mutex<Vec<i64>>,
    }

    /// Verarbeiter, der `dauer` lang „nach BTP schreibt".
    fn attrappe(mess: Arc<Mess>, dauer: Duration) -> ErgebnisVerarbeiter {
        Arc::new(move |_ctx, body| {
            let mess = mess.clone();
            Box::pin(async move {
                let n = mess.laufend.fetch_add(1, Ordering::SeqCst) + 1;
                mess.hoechstens.fetch_max(n, Ordering::SeqCst);
                mess.folge.lock().unwrap().push(body.match_id);
                tokio::time::sleep(dauer).await;
                mess.laufend.fetch_sub(1, Ordering::SeqCst);
                mess.fertig.lock().unwrap().push(body.match_id);
                relay_proto::ResultResponse::ok()
            })
        })
    }

    fn auftrag(
        req_id: u64,
        court: i64,
        match_id: i64,
        antwort: &mpsc::UnboundedSender<WsMessage>,
    ) -> ErgebnisAuftrag {
        ErgebnisAuftrag {
            req_id,
            body: ergebnis(court, match_id),
            antwort: antwort.clone(),
        }
    }

    /// Liest die nächste Quittung: (req_id, ok, permanent).
    async fn naechste_quittung(rx: &mut mpsc::UnboundedReceiver<WsMessage>) -> (u64, bool, bool) {
        let msg = rx.recv().await.expect("Quittung");
        match serde_json::from_str::<HostFrame>(&text_of(&msg)).unwrap() {
            HostFrame::ResultAck {
                req_id,
                ok,
                permanent,
                ..
            } => (req_id, ok, permanent),
            anderes => panic!("keine Quittung: {anderes:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ergebnisse_eines_felds_laufen_nacheinander() {
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let arbeiter = ErgebnisArbeiter::new(attrappe(mess.clone(), Duration::from_secs(10)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        arbeiter.einreihen(&ctx, auftrag(1, 1, 100, &tx));
        arbeiter.einreihen(&ctx, auftrag(2, 1, 101, &tx));
        assert_eq!(naechste_quittung(&mut rx).await, (1, true, false));
        assert_eq!(naechste_quittung(&mut rx).await, (2, true, false));
        assert_eq!(
            mess.hoechstens.load(Ordering::SeqCst),
            1,
            "nie zwei zugleich"
        );
        assert_eq!(*mess.folge.lock().unwrap(), vec![100, 101]);
    }

    #[tokio::test(start_paused = true)]
    async fn felder_laufen_nebeneinander() {
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let arbeiter = ErgebnisArbeiter::new(attrappe(mess.clone(), Duration::from_secs(10)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let start = tokio::time::Instant::now();
        arbeiter.einreihen(&ctx, auftrag(1, 1, 100, &tx));
        arbeiter.einreihen(&ctx, auftrag(2, 2, 200, &tx));
        naechste_quittung(&mut rx).await;
        naechste_quittung(&mut rx).await;
        assert_eq!(mess.hoechstens.load(Ordering::SeqCst), 2);
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "zwei Felder brauchen nicht die doppelte Zeit"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn unbekannte_felder_teilen_sich_einen_arbeiter() {
        // Die CourtID kommt vom Tablet — ein Fremder darf damit nicht
        // beliebig viele Arbeiter-Tasks anlegen.
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let arbeiter = ErgebnisArbeiter::new(attrappe(mess.clone(), Duration::from_secs(1)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        for (i, court) in [9001, 9002, 9003].into_iter().enumerate() {
            arbeiter.einreihen(&ctx, auftrag(i as u64, court, 1, &tx));
        }
        for _ in 0..3 {
            naechste_quittung(&mut rx).await;
        }
        assert_eq!(mess.hoechstens.load(Ordering::SeqCst), 1);
        assert_eq!(arbeiter.tasks.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_hinter_dem_ersten_versuch_wird_quittiert() {
        // Der 8-s-Retry des Tablets trifft ein, während der erste Versuch
        // noch schreibt: Er läuft DANACH (und trifft dann in
        // `process_result` die Idempotenz), nicht daneben.
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let arbeiter = ErgebnisArbeiter::new(attrappe(mess.clone(), Duration::from_secs(20)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        arbeiter.einreihen(&ctx, auftrag(7, 1, 100, &tx));
        tokio::time::sleep(Duration::from_secs(8)).await;
        arbeiter.einreihen(&ctx, auftrag(8, 1, 100, &tx));
        assert_eq!(naechste_quittung(&mut rx).await.0, 7);
        assert_eq!(
            *mess.fertig.lock().unwrap(),
            vec![100],
            "der Retry beginnt erst nach dem Ende des ersten Versuchs"
        );
        assert_eq!(naechste_quittung(&mut rx).await.0, 8);
        assert_eq!(mess.hoechstens.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn quittung_geht_nie_in_eine_neue_sitzung() {
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let arbeiter = ErgebnisArbeiter::new(attrappe(mess.clone(), Duration::from_secs(10)));
        let (tx_alt, rx_alt) = mpsc::unbounded_channel();
        arbeiter.einreihen(&ctx, auftrag(1, 1, 100, &tx_alt));
        // Verbindung reißt mitten im Write ab; eine neue Sitzung entsteht und
        // schickt demselben Arbeiter ein weiteres Ergebnis — mit derselben
        // Nummer, wie nach einem Relay-Neustart.
        drop(rx_alt);
        drop(tx_alt);
        let (tx_neu, mut rx_neu) = mpsc::unbounded_channel::<WsMessage>();
        arbeiter.einreihen(&ctx, auftrag(1, 1, 101, &tx_neu));
        assert_eq!(naechste_quittung(&mut rx_neu).await.0, 1);
        assert_eq!(
            *mess.fertig.lock().unwrap(),
            vec![100, 101],
            "der abgerissene Write lief zu Ende, dann der neue"
        );
        assert!(
            rx_neu.try_recv().is_err(),
            "die neue Sitzung bekommt genau EINE Quittung — die alte verpufft"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn toter_arbeiter_wird_ersetzt() {
        // Eine Panik im Ergebnis-Weg darf das Feld nicht bis zum Neustart
        // der Übertragung lahmlegen.
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let innen = attrappe(mess.clone(), Duration::from_secs(1));
        let verarbeiter: ErgebnisVerarbeiter = Arc::new(move |ctx, body| {
            if body.match_id == 666 {
                Box::pin(async { panic!("Testpanik im Ergebnis-Weg") })
            } else {
                innen(ctx, body)
            }
        });
        let arbeiter = ErgebnisArbeiter::new(verarbeiter);
        let (tx, mut rx) = mpsc::unbounded_channel();
        arbeiter.einreihen(&ctx, auftrag(1, 1, 666, &tx));
        tokio::time::sleep(Duration::from_millis(10)).await;
        arbeiter.einreihen(&ctx, auftrag(2, 1, 100, &tx));
        assert_eq!(naechste_quittung(&mut rx).await, (2, true, false));
    }

    #[tokio::test(start_paused = true)]
    async fn hintergrund_aufgaben_enden_mit_den_diensten() {
        let dienste = Dienste::new(attrappe(Arc::new(Mess::default()), Duration::ZERO));
        let lebt = Arc::new(());
        let halter = lebt.clone();
        dienste.im_hintergrund(async move {
            let _halter = halter;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(Arc::strong_count(&lebt), 2);
        drop(dienste);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(Arc::strong_count(&lebt), 1, "der Task wurde abgebrochen");
    }

    #[tokio::test(start_paused = true)]
    async fn neue_sitzung_vergisst_die_upload_pause() {
        let wacht = LaufWacht::new(true);
        let jetzt = tokio::time::Instant::now();
        wacht.beginnen(jetzt).expect("Lauf").fertig(false, jetzt);
        assert!(wacht.beginnen(jetzt).is_none());
        wacht.pause_vergessen();
        assert!(wacht.beginnen(jetzt).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn haengender_schreiber_beendet_sich_nach_der_frist() {
        // Die Gegenseite liest nicht: Das Senden einer großen Nachricht
        // bleibt im (kleinen) Puffer stecken.
        let (a, b) = tokio::io::duplex(256);
        let (client, server) = tokio::join!(
            tokio_tungstenite::client_async("ws://test/host-ws", a),
            tokio_tungstenite::accept_async(b)
        );
        let (client, _server_haelt_offen) = (client.unwrap().0, server.unwrap());
        let (sink, _read) = client.split();
        let (tx, rx) = mpsc::unbounded_channel();
        let start = tokio::time::Instant::now();
        let schreiber = schreiber_starten(sink, rx);
        tx.send(WsMessage::Text("x".repeat(1 << 20).into()))
            .unwrap();
        let grund = schreiber.await.unwrap();
        assert!(grund.contains("hing länger"), "{grund}");
        assert_eq!(start.elapsed(), SENDE_FRIST);
    }

    #[tokio::test(start_paused = true)]
    async fn volle_feld_schlange_sagt_wiederholbar_ab() {
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        // Der Write hängt praktisch ewig.
        let arbeiter = ErgebnisArbeiter::new(attrappe(mess.clone(), Duration::from_secs(3600)));
        let (tx, mut rx) = mpsc::unbounded_channel();
        arbeiter.einreihen(&ctx, auftrag(1, 1, 100, &tx));
        // Den Arbeiter den ersten Auftrag aus der Schlange nehmen lassen.
        tokio::time::sleep(Duration::from_millis(10)).await;
        for req in 2..=5 {
            arbeiter.einreihen(&ctx, auftrag(req, 1, 100, &tx));
        }
        assert!(rx.try_recv().is_err(), "vier dürfen warten");
        arbeiter.einreihen(&ctx, auftrag(6, 1, 100, &tx));
        let (req_id, ok, permanent) = naechste_quittung(&mut rx).await;
        assert_eq!((req_id, ok), (6, false));
        assert!(!permanent, "das Tablet soll es wieder versuchen");
        // Ein anderes Feld ist davon unberührt.
        arbeiter.einreihen(&ctx, auftrag(7, 2, 200, &tx));
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(mess.laufend.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn zweiter_upload_startet_nicht_solange_einer_laeuft() {
        let wacht = LaufWacht::new(true);
        let jetzt = tokio::time::Instant::now();
        let lauf = wacht.beginnen(jetzt).expect("erster Lauf");
        assert!(wacht.beginnen(jetzt).is_none());
        lauf.fertig(true, jetzt);
        assert!(wacht.beginnen(jetzt).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn abgebrochener_lauf_gibt_den_waechter_frei() {
        let wacht = LaufWacht::new(true);
        let jetzt = tokio::time::Instant::now();
        drop(wacht.beginnen(jetzt));
        assert!(wacht.beginnen(jetzt).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn fehlgeschlagener_upload_wartet_laenger() {
        let wacht = LaufWacht::new(true);
        let mut jetzt = tokio::time::Instant::now();
        for sekunden in [30u64, 60, 120, 240, 300, 300] {
            wacht.beginnen(jetzt).expect("Lauf").fertig(false, jetzt);
            let knapp_davor = jetzt + Duration::from_secs(sekunden - 1);
            assert!(wacht.beginnen(knapp_davor).is_none(), "{sekunden} s Pause");
            jetzt += Duration::from_secs(sekunden);
        }
        // Ein Erfolg setzt die Pause zurück.
        wacht.beginnen(jetzt).expect("Lauf").fertig(true, jetzt);
        wacht.beginnen(jetzt).expect("Lauf").fertig(false, jetzt);
        assert!(wacht.beginnen(jetzt + Duration::from_secs(30)).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn steuerungs_abgleich_pausiert_nach_fehlern_nicht() {
        let wacht = LaufWacht::new(false);
        let jetzt = tokio::time::Instant::now();
        wacht.beginnen(jetzt).expect("Lauf").fertig(false, jetzt);
        assert!(wacht.beginnen(jetzt).is_some());
    }

    type TestStrom = WebSocketStream<tokio::io::DuplexStream>;

    /// Ein WebSocket-Paar mit echtem Rahmen, ohne Netz.
    async fn ws_paar() -> (TestStrom, TestStrom) {
        let (a, b) = tokio::io::duplex(1 << 20);
        let (client, server) = tokio::join!(
            tokio_tungstenite::client_async("ws://test/host-ws", a),
            tokio_tungstenite::accept_async(b)
        );
        (client.unwrap().0, server.unwrap())
    }

    /// Dienste für Sitzungs-Tests: Upload und Steuerungs-Abgleich sind
    /// „belegt", damit kein Test ins Netz geht.
    fn dienste_ohne_netz(verarbeiter: ErgebnisVerarbeiter) -> (Arc<Dienste>, (Lauf, Lauf)) {
        let dienste = Arc::new(Dienste::new(verarbeiter));
        let jetzt = tokio::time::Instant::now();
        let belegt = (
            dienste.upload.beginnen(jetzt).unwrap(),
            dienste.steuerung.beginnen(jetzt).unwrap(),
        );
        (dienste, belegt)
    }

    fn sitzung_starten(
        ctx: Arc<ServerCtx>,
        client: TestStrom,
        dienste: Arc<Dienste>,
    ) -> tokio::task::JoinHandle<Result<Schluss, String>> {
        tokio::spawn(async move {
            let mut wacht = crate::tablet::state::ReloadWacht::beim_verbinden(&ctx.tablet);
            sitzung_fahren(&ctx, client, "ns", &mut wacht, &dienste).await
        })
    }

    #[tokio::test(start_paused = true)]
    async fn ping_wird_beantwortet_waehrend_ein_ergebnis_haengt() {
        let ctx = ctx_zwei_felder();
        let mess = Arc::new(Mess::default());
        let (dienste, _belegt) = dienste_ohne_netz(attrappe(mess.clone(), Duration::from_secs(20)));
        let (client, mut server) = ws_paar().await;
        let sitzung = sitzung_starten(ctx, client, dienste);

        let frame = RelayFrame::Result {
            req_id: 5,
            court_id: 1,
            court_label: "1".into(),
            match_id: 100,
            sets: vec![],
            retired: false,
            walkover: false,
            winner: None,
            cascade_walkover: false,
        };
        server
            .send(WsMessage::Text(
                serde_json::to_string(&frame).unwrap().into(),
            ))
            .await
            .unwrap();

        // Der Relay pingt alle 5 s; der Write hängt 20 s.
        let mut ping = tokio::time::interval(Duration::from_secs(5));
        let mut pongs_vor_der_quittung = 0;
        loop {
            tokio::select! {
                msg = server.next() => match msg.expect("Sitzung lebt").unwrap() {
                    WsMessage::Pong(_) => pongs_vor_der_quittung += 1,
                    WsMessage::Text(t) => {
                        if let Ok(HostFrame::ResultAck { req_id, ok, .. }) =
                            serde_json::from_str::<HostFrame>(t.as_str())
                        {
                            assert_eq!((req_id, ok), (5, true));
                            break;
                        }
                    }
                    _ => {}
                },
                _ = ping.tick() => {
                    server.send(WsMessage::Ping(vec![1].into())).await.unwrap();
                }
            }
        }
        assert!(
            pongs_vor_der_quittung >= 3,
            "während des Writes kamen nur {pongs_vor_der_quittung} Pongs"
        );
        assert!(!sitzung.is_finished(), "die Sitzung hat den Write überlebt");
    }

    #[tokio::test(start_paused = true)]
    async fn sitzung_ueberlebt_pausen_mit_lebenszeichen_im_puffer() {
        // Nach einer langen Pause der Schleife sind „Frame liegt bereit" und
        // „Idle-Ticker fällig" gleichzeitig wahr. Der Wächter darf dann
        // nicht vor dem Lesen urteilen — sonst trennt sich der Turnier-PC
        // selbst, obwohl der Relay ihn noch hält.
        //
        // Ehrlich: Dieser Test hält nur fest, dass eine Sitzung Pausen von
        // 16 s übersteht, wenn das Lebenszeichen im Puffer liegt. Den
        // eigentlichen Wettlauf stellt er NICHT nach — mit angehaltener Uhr
        // feuern die Timer erst, wenn die Laufzeit parkt, also nach dem
        // Lesen. Rot wird er ohne `biased` deshalb nicht; die feste
        // Reihenfolge im `select!` ist per Überlegung abgesichert, und die
        // langen Inline-Wartezeiten, die den Wettlauf auslösten, gibt es
        // nicht mehr (siehe die Tests darüber).
        let ctx = ctx_zwei_felder();
        let (dienste, _belegt) =
            dienste_ohne_netz(attrappe(Arc::new(Mess::default()), Duration::ZERO));
        let (client, mut server) = ws_paar().await;
        let sitzung = sitzung_starten(ctx, client, dienste);
        tokio::time::sleep(Duration::from_millis(10)).await;

        for runde in 0..40 {
            // Lebenszeichen liegt im Puffer, die Schleife kommt aber erst
            // 16 s später wieder dran.
            server.send(WsMessage::Ping(vec![1].into())).await.unwrap();
            tokio::time::advance(Duration::from_secs(16)).await;
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert!(
                !sitzung.is_finished(),
                "Runde {runde}: der Idle-Wächter hat die Sitzung beendet"
            );
        }
    }

    #[test]
    fn groesster_zettel_frame_bleibt_unter_dem_transportdeckel() {
        // Messung für den Transport-Deckel des Relays (Spec relay-haerten,
        // PR 4): Der Relay lässt höchstens so viel Zettel-HTML durch
        // (`MAX_SCORESHEET_HTML_LEN` in `relay/src/main.rs`). JSON verdoppelt
        // im ungünstigsten realistischen Fall jedes Zeichen (Anführungs-
        // zeichen, Zeilenumbrüche) — der Deckel muss das samt Umschlag fassen.
        const HTML_DECKEL: usize = relay_proto::MAX_SHEETS_PER_DOC * 128 * 1024 + 64 * 1024;
        const TRANSPORT_DECKEL: usize = 2 * HTML_DECKEL + 64 * 1024;
        let frame = HostFrame::ScoresheetData {
            req_id: u64::MAX,
            found: true,
            html: "\"".repeat(HTML_DECKEL),
        };
        let laenge = serde_json::to_string(&frame).unwrap().len();
        assert!(laenge > 2 * HTML_DECKEL, "Escaping verdoppelt: {laenge}");
        assert!(
            laenge <= TRANSPORT_DECKEL,
            "{laenge} Bytes passen nicht in {TRANSPORT_DECKEL}"
        );
        assert_eq!(TRANSPORT_DECKEL, 10_432 * 1024);
    }
}
