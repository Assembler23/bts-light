//! Träger-Client des Slaves: bündelt die lokalen Geräte der fernen Halle
//! über **eine** WebSocket zum Master-Relay
//! ([ADR 0048](../../../docs/adr/0048-substrom-adressierung-traeger.md)).
//!
//! Jedes lokale Gerät ist ein **Substrom** mit eigener Kennung. Der Slave
//! terminiert dessen WebSocket selbst (Seiten, Marke, Pong) und schiebt nur
//! die Fachframes durch den Träger.
//!
//! **Der Slave ist für die Liveness seiner Geräte zuständig.** Hinter dem
//! Träger misst der Relay nur den Träger; stirbt ein lokales Gerät, muss der
//! Slave das melden. Er hält die echte Verbindung dorthin und sieht den
//! Abriss schneller als der Relay es je könnte. (Der Relay hat seit dem
//! Review eine eigene Rückfallebene — verlassen darf man sich darauf nicht.)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use relay_proto::{CarrierMsg, CarrierServerMsg, StreamKind, CARRIER_PROTO_VERSION};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::tablet::ws_verbindung::{self, Backoff, Schluss, SitzungsEnde};

/// HTTPS/WSS-Basis des Cloud-Relays (identisch zum Relay-Client) –
/// Produktiv- oder Testsystem, siehe [`crate::badhub_host`]. Die ferne
/// Halle muss denselben Relay wählen wie der Master; sonst wird der Träger
/// nie bereit und jedes Gerät fällt auf einen Relay zurück, auf dem kein
/// Host sitzt.
fn relay_ws() -> String {
    crate::badhub_host::relay_wss()
}

/// Nach dieser Stille ohne jedes Lebenszeichen gilt der Träger als tot.
/// Gegenstück zu `HOST_STALE` im Relay; der Relay pingt, wir werten aus.
const TRAEGER_READ_IDLE: Duration = Duration::from_secs(15);

/// So lange darf ein lokales Gerät schweigen, bevor der Slave seinen
/// Substrom schließt und damit den Court-Slot freigibt.
///
/// Deckungsgleich mit `TABLET_STALE` im Relay — dieselbe Zusage wie am
/// direkten Weg, nur eben hier gemessen.
const GERAET_STALE: Duration = Duration::from_secs(15);

/// Frist je Nachricht an den Relay. Hängt das Senden länger, gilt der
/// Träger als tot — ohne sie stünde die Schleife (und damit das Lesen)
/// unbegrenzt.
const SENDE_FRIST: Duration = Duration::from_secs(20);

/// Was der Träger einem lokalen Substrom zustellt.
pub enum AnGeraet {
    /// Fachframe vom Relay, 1:1 an das Gerät.
    Frame(String),
    /// Der Relay hat den Substrom geschlossen — Verbindung beenden.
    Schluss,
}

/// Griff auf den Träger: hier melden sich die lokalen Verbindungen an.
pub struct Traeger {
    /// Ausgang zum Träger-Task.
    hinaus: mpsc::UnboundedSender<CarrierMsg>,
    /// Nächste freie Substrom-Kennung.
    naechste: AtomicU32,
    /// Substrom-Kennung → Zustellkanal an das lokale Gerät.
    stroeme: Mutex<HashMap<u32, mpsc::UnboundedSender<AnGeraet>>>,
    /// Hat der Relay die Protokollfassung bestätigt?
    ///
    /// Solange nicht, nimmt der Slave keine Geräte an — sie liefen sonst ins
    /// Leere. Der Aufrufer weicht dann auf die Weiterleitung aus.
    bereit: AtomicBool,
    /// Wurde eine zu große Nutzlast schon gemeldet? (Eine Zeile genügt —
    /// ein Gerät mit zu großem Zustand schickt ihn bei jedem Punkt.)
    deckel_gemeldet: AtomicBool,
}

impl Traeger {
    fn neu(hinaus: mpsc::UnboundedSender<CarrierMsg>) -> Self {
        Self {
            hinaus,
            naechste: AtomicU32::new(1),
            stroeme: Mutex::new(HashMap::new()),
            bereit: AtomicBool::new(false),
            deckel_gemeldet: AtomicBool::new(false),
        }
    }

    /// Ist der Träger einsatzbereit? Nur dann darf ein Gerät lokal
    /// angenommen werden.
    pub fn bereit(&self) -> bool {
        self.bereit.load(Ordering::Relaxed)
    }

    /// Meldet ein lokales Gerät an und liefert Kennung + Zustellkanal.
    ///
    /// `None`, wenn der Träger nicht bereit ist — der Aufrufer schickt das
    /// Gerät dann auf den Direkt-Cloud-Weg.
    pub fn oeffne(
        &self,
        kind: StreamKind,
        court: Option<i64>,
    ) -> Option<(u32, mpsc::UnboundedReceiver<AnGeraet>)> {
        if !self.bereit() {
            return None;
        }
        let stream = self.naechste.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        self.stroeme
            .lock()
            .expect("Substrom-Mutex nicht vergiftet")
            .insert(stream, tx);
        if self
            .hinaus
            .send(CarrierMsg::StreamOpen {
                stream,
                kind,
                court,
            })
            .is_err()
        {
            self.vergiss(stream);
            return None;
        }
        Some((stream, rx))
    }

    /// Reicht einen Fachframe des Geräts an den Relay durch.
    ///
    /// Verworfen wird, was nie ankäme: Frames ohne bereiten Träger (sie
    /// stauten sich sonst im Ausgang und gingen nach dem Wiederverbinden an
    /// Substrom-Kennungen, die der Relay nicht mehr kennt) und Nutzlast über
    /// dem Deckel des Relays (er verwürfe sie ohnehin, still).
    pub fn frame(&self, stream: u32, payload: String) {
        if !self.bereit() {
            return;
        }
        if payload.len() > relay_proto::MAX_CARRIER_PAYLOAD_LEN {
            if !self.deckel_gemeldet.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "Träger: Nachricht eines Geräts mit {} Bytes verworfen (Deckel {} Bytes) — weitere werden nicht mehr gemeldet",
                    payload.len(),
                    relay_proto::MAX_CARRIER_PAYLOAD_LEN
                );
            }
            return;
        }
        let _ = self.hinaus.send(CarrierMsg::Frame { stream, payload });
    }

    /// Meldet, dass ein lokales Gerät weg ist. **Muss** gerufen werden, sonst
    /// hält ein totes Tablet seinen Court-Slot.
    pub fn schliesse(&self, stream: u32) {
        self.vergiss(stream);
        let _ = self.hinaus.send(CarrierMsg::StreamClose { stream });
    }

    fn vergiss(&self, stream: u32) {
        self.stroeme
            .lock()
            .expect("Substrom-Mutex nicht vergiftet")
            .remove(&stream);
    }

    /// Stellt eine Relay-Antwort dem passenden Gerät zu.
    fn zustellen(&self, stream: u32, was: AnGeraet) {
        let ziel = self
            .stroeme
            .lock()
            .expect("Substrom-Mutex nicht vergiftet")
            .get(&stream)
            .cloned();
        if let Some(tx) = ziel {
            let _ = tx.send(was);
        }
    }

    /// Bricht alle Substrome ab — der Träger ist weg.
    ///
    /// Die lokalen Verbindungen enden dadurch; ihre Geräte laden neu und
    /// landen wieder hier, sobald der Träger steht.
    fn alle_abbrechen(&self) {
        let alle: Vec<_> = self
            .stroeme
            .lock()
            .expect("Substrom-Mutex nicht vergiftet")
            .drain()
            .collect();
        for (_, tx) in alle {
            let _ = tx.send(AnGeraet::Schluss);
        }
    }
}

/// Baut den Träger und liefert den Griff dafür **samt** der Schleife, die ihn
/// am Leben hält.
///
/// Der Aufrufer spawnt die Schleife selbst — so bleibt dieses Modul frei von
/// der Frage, welche Laufzeit gerade zuständig ist, und der Aufrufer behält
/// das Handle zum Abbrechen.
pub fn starten(namespace: String) -> (Arc<Traeger>, impl std::future::Future<Output = ()>) {
    let (hinaus_tx, hinaus_rx) = mpsc::unbounded_channel::<CarrierMsg>();
    let traeger = Arc::new(Traeger::neu(hinaus_tx));
    let fuer_schleife = traeger.clone();
    let schleife = async move { lauf(fuer_schleife, namespace, hinaus_rx).await };
    (traeger, schleife)
}

/// Verbindungsschleife mit Backoff — derselbe Automat wie `relay_client::run`
/// ([`ws_verbindung::Backoff`]).
async fn lauf(
    traeger: Arc<Traeger>,
    namespace: String,
    mut hinaus_rx: mpsc::UnboundedReceiver<CarrierMsg>,
) {
    let url = format!("{}/{namespace}/carrier-ws", relay_ws());
    let mut backoff = Backoff::default();
    loop {
        let mut bereit_seit = None;
        let ergebnis = sitzung(&traeger, &url, &mut hinaus_rx, &mut bereit_seit).await;
        // Die Geräte hängen an dieser Verbindung; ohne Abbruch warteten sie
        // auf Antworten, die nie kommen.
        traeger.bereit.store(false, Ordering::Relaxed);
        traeger.alle_abbrechen();

        let ende = match ergebnis {
            Ok(()) => {
                tracing::info!("Träger beendet – neuer Versuch");
                traeger_ende(bereit_seit, Schluss::Close(None))
            }
            // Ein Relay, der unsere Fassung nicht kennt, wird sie auch beim
            // hundertsten Versuch nicht kennen. Aufgeben ist hier das
            // Richtige: Die Brücke fällt Route für Route auf die
            // Weiterleitung zurück, und die Halle arbeitet über die Cloud
            // weiter. Endloses Wiederholen hielte sie stattdessen in einem
            // Zustand, in dem `/ws` dauerhaft 503 liefert.
            Err(TraegerEnde::FassungAbgelehnt) => {
                tracing::warn!("Träger aufgegeben – Geräte laufen direkt über die Cloud");
                return;
            }
            Err(TraegerEnde::Aufbau(e)) => {
                tracing::warn!("Träger-Verbindung: {e}");
                SitzungsEnde::AufbauGescheitert
            }
            Err(TraegerEnde::Abbruch(e)) => {
                tracing::warn!("Träger-Verbindung: {e}");
                traeger_ende(bereit_seit, Schluss::Abriss)
            }
        };
        let pause = ws_verbindung::mit_jitter(backoff.pause_nach(ende), ws_verbindung::zufall());
        tokio::time::sleep(pause).await;
    }
}

/// Ordnet das Ende einer Träger-Sitzung ein. „Stabil" zählt erst ab dem
/// `Ready` des Relays: Eine Verbindung, die nie bereit wurde, hat der Halle
/// nichts genützt — egal wie lange sie offen war.
fn traeger_ende(bereit_seit: Option<tokio::time::Instant>, schluss: Schluss) -> SitzungsEnde {
    match (bereit_seit, schluss) {
        (Some(seit), schluss) => ws_verbindung::einordnen(seit.elapsed(), schluss),
        (None, Schluss::Close(_)) => SitzungsEnde::CloseOhneGrund,
        // Nie bereit und abgerissen: der Relay antwortet nicht richtig —
        // nicht im Fünf-Sekunden-Takt nachfragen.
        (None, Schluss::Abriss) => SitzungsEnde::Abriss,
    }
}

/// Warum eine Träger-Sitzung endete.
enum TraegerEnde {
    /// Der Relay kennt unsere Protokollfassung nicht — nicht wiederholen.
    FassungAbgelehnt,
    /// Die Verbindung kam gar nicht zustande (Netz, Frist).
    Aufbau(String),
    /// Alles andere: Netz weg, Relay neu gestartet, Lesefehler.
    Abbruch(String),
}

async fn sitzung(
    traeger: &Arc<Traeger>,
    url: &str,
    hinaus_rx: &mut mpsc::UnboundedReceiver<CarrierMsg>,
    bereit_seit: &mut Option<tokio::time::Instant>,
) -> Result<(), TraegerEnde> {
    let stream = ws_verbindung::verbinden(url, ws_verbindung::VERBINDUNGS_FRIST)
        .await
        .map_err(|e| TraegerEnde::Aufbau(format!("Verbindung zu {url}: {e}")))?;
    let (mut sink, mut read) = stream.split();

    // Reste der alten Sitzung verwerfen: Ihre Substrom-Kennungen kennt der
    // Relay nach dem Wiederverbinden nicht mehr (die Geräte wurden beim
    // Abriss ohnehin getrennt und melden sich neu an).
    while hinaus_rx.try_recv().is_ok() {}

    // Fassung aushandeln, bevor irgendein Gerät angenommen wird.
    let hallo = serde_json::to_string(&CarrierMsg::Hello {
        proto: CARRIER_PROTO_VERSION,
    })
    .map_err(|e| TraegerEnde::Abbruch(e.to_string()))?;
    senden_mit_frist(&mut sink, WsMessage::Text(hallo.into()))
        .await
        .map_err(|e| TraegerEnde::Abbruch(format!("Begruessung: {e}")))?;

    let mut letztes = tokio::time::Instant::now();
    let mut leerlauf = tokio::time::interval(TRAEGER_READ_IDLE / 3);
    leerlauf.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            eingang = read.next() => {
                // Strom ohne Close-Frame zu Ende: ein Abriss, wie beim Host.
                let Some(nachricht) = eingang else {
                    return Err(TraegerEnde::Abbruch("Verbindung ohne Close beendet".into()));
                };
                let nachricht = nachricht.map_err(|e| TraegerEnde::Abbruch(format!("Lesen: {e}")))?;
                letztes = tokio::time::Instant::now();
                match nachricht {
                    WsMessage::Text(t) => {
                        let Ok(frame) = serde_json::from_str::<CarrierServerMsg>(&t) else { continue };
                        match frame {
                            CarrierServerMsg::Ready { proto } => {
                                tracing::info!("Träger bereit (Fassung {proto})");
                                traeger.bereit.store(true, Ordering::Relaxed);
                                // Ab hier zählt die Sitzung: Hält sie
                                // 30 s, springt die Wartezeit zurück —
                                // sonst bliebe sie nach ein paar Aussetzern
                                // dauerhaft bei 30 s, und jeder spätere
                                // Abriss ließe die Halle so lange dunkel.
                                bereit_seit.get_or_insert_with(tokio::time::Instant::now);
                            }
                            CarrierServerMsg::Unsupported { proto } => {
                                // Der Relay ist älter als wir. Nicht endlos
                                // weiterversuchen — der Aufrufer schickt die
                                // Geräte auf den Direkt-Cloud-Weg, und die
                                // Halle arbeitet weiter.
                                tracing::warn!(
                                    "Relay kennt Träger-Fassung {proto} nicht – Geräte laufen direkt über die Cloud"
                                );
                                return Err(TraegerEnde::FassungAbgelehnt);
                            }
                            CarrierServerMsg::Frame { stream, payload } => {
                                traeger.zustellen(stream, AnGeraet::Frame(payload));
                            }
                            CarrierServerMsg::StreamClose { stream } => {
                                traeger.vergiss(stream);
                                traeger.zustellen(stream, AnGeraet::Schluss);
                            }
                        }
                    }
                    // Der Relay pingt; tokio-tungstenite pongt selbst.
                    WsMessage::Close(_) => return Ok(()),
                    _ => {}
                }
            }
            ausgang = hinaus_rx.recv() => {
                let Some(msg) = ausgang else { return Ok(()) };
                let roh = serde_json::to_string(&msg).map_err(|e| TraegerEnde::Abbruch(e.to_string()))?;
                senden_mit_frist(&mut sink, WsMessage::Text(roh.into()))
                    .await
                    .map_err(|e| TraegerEnde::Abbruch(format!("Senden: {e}")))?;
            }
            _ = leerlauf.tick() => {
                if letztes.elapsed() >= TRAEGER_READ_IDLE {
                    return Err(TraegerEnde::Abbruch(format!(
                        "seit {}s nichts vom Relay gehoert",
                        letztes.elapsed().as_secs()
                    )));
                }
            }
        }
    }
}

/// Sendet eine Nachricht an den Relay, höchstens [`SENDE_FRIST`] lang.
async fn senden_mit_frist<K>(sink: &mut K, nachricht: WsMessage) -> Result<(), String>
where
    K: futures_util::Sink<WsMessage> + Unpin,
    K::Error: std::fmt::Display,
{
    match tokio::time::timeout(SENDE_FRIST, sink.send(nachricht)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!("hing länger als {} s", SENDE_FRIST.as_secs())),
    }
}

/// Wie lange ein lokales Gerät schweigen darf. Öffentlich, damit die
/// lokalen Verbindungen dieselbe Grenze verwenden.
pub const fn geraet_stale() -> Duration {
    GERAET_STALE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traeger_bereit() -> (Arc<Traeger>, mpsc::UnboundedReceiver<CarrierMsg>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let t = Arc::new(Traeger::neu(tx));
        t.bereit.store(true, Ordering::Relaxed);
        (t, rx)
    }

    /// Ohne bestätigte Fassung darf kein Gerät angenommen werden — es liefe
    /// sonst ins Leere, statt auf den Direkt-Cloud-Weg auszuweichen.
    #[test]
    fn nicht_bereiter_traeger_nimmt_keine_geraete() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let t = Traeger::neu(tx);
        assert!(!t.bereit());
        assert!(t.oeffne(StreamKind::Tablet, None).is_none());
    }

    #[test]
    fn oeffnen_vergibt_aufsteigende_kennungen_und_meldet_sie() {
        let (t, mut rx) = traeger_bereit();
        let (a, _ra) = t.oeffne(StreamKind::Tablet, None).expect("Substrom A");
        let (b, _rb) = t.oeffne(StreamKind::Monitor, Some(7)).expect("Substrom B");
        assert_ne!(a, b, "jedes Gerät braucht eine eigene Kennung");

        match rx.try_recv().expect("StreamOpen A") {
            CarrierMsg::StreamOpen {
                stream,
                kind,
                court,
            } => {
                assert_eq!(stream, a);
                assert_eq!(kind, StreamKind::Tablet);
                assert_eq!(court, None);
            }
            anderes => panic!("unerwartet: {anderes:?}"),
        }
        match rx.try_recv().expect("StreamOpen B") {
            CarrierMsg::StreamOpen {
                stream,
                kind,
                court,
            } => {
                assert_eq!(stream, b);
                assert_eq!(kind, StreamKind::Monitor);
                assert_eq!(court, Some(7));
            }
            anderes => panic!("unerwartet: {anderes:?}"),
        }
    }

    /// Antworten müssen beim **richtigen** Gerät landen. Das ist die lokale
    /// Entsprechung der Relay-Invariante: Substrome dürfen einander nicht
    /// in die Quere kommen.
    #[test]
    fn antworten_gehen_an_den_richtigen_substrom() {
        let (t, _rx) = traeger_bereit();
        let (a, mut ra) = t.oeffne(StreamKind::Tablet, None).expect("A");
        let (b, mut rb) = t.oeffne(StreamKind::Tablet, None).expect("B");

        t.zustellen(a, AnGeraet::Frame("fuer-a".into()));
        t.zustellen(b, AnGeraet::Frame("fuer-b".into()));

        match ra.try_recv().expect("A hat Post") {
            AnGeraet::Frame(p) => assert_eq!(p, "fuer-a"),
            _ => panic!("falsche Zustellung an A"),
        }
        match rb.try_recv().expect("B hat Post") {
            AnGeraet::Frame(p) => assert_eq!(p, "fuer-b"),
            _ => panic!("falsche Zustellung an B"),
        }
        assert!(ra.try_recv().is_err(), "A darf nichts von B bekommen");
    }

    /// Schließen meldet den Substrom ab **und** sagt es dem Relay — sonst
    /// hielte ein totes Tablet seinen Court-Slot.
    #[test]
    fn schliessen_meldet_sich_beim_relay_ab() {
        let (t, mut rx) = traeger_bereit();
        let (a, mut ra) = t.oeffne(StreamKind::Tablet, None).expect("A");
        let _ = rx.try_recv(); // StreamOpen abräumen

        t.schliesse(a);
        match rx.try_recv().expect("StreamClose erwartet") {
            CarrierMsg::StreamClose { stream } => assert_eq!(stream, a),
            anderes => panic!("unerwartet: {anderes:?}"),
        }
        // Nach dem Abmelden erreicht das Gerät nichts mehr.
        t.zustellen(a, AnGeraet::Frame("zu spaet".into()));
        assert!(ra.try_recv().is_err());
    }

    /// Bricht der Träger weg, müssen **alle** lokalen Verbindungen enden.
    /// Blieben sie offen, warteten die Geräte auf Antworten, die nie kommen.
    #[test]
    fn traegerabriss_beendet_alle_substrome() {
        let (t, _rx) = traeger_bereit();
        let (_a, mut ra) = t.oeffne(StreamKind::Tablet, None).expect("A");
        let (_b, mut rb) = t.oeffne(StreamKind::Monitor, Some(3)).expect("B");

        t.alle_abbrechen();

        assert!(matches!(ra.try_recv(), Ok(AnGeraet::Schluss)));
        assert!(matches!(rb.try_recv(), Ok(AnGeraet::Schluss)));
    }

    /// Die Geduld mit einem stummen Gerät ist dieselbe wie am direkten Weg.
    #[test]
    fn geraete_geduld_entspricht_dem_direkten_weg() {
        assert_eq!(
            geraet_stale(),
            Duration::from_secs(15),
            "TABLET_STALE im Relay — dieselbe Zusage, nur hier gemessen"
        );
    }

    // ── Spec relay-haerten, PR 2 ─────────────────────────────────────────

    #[test]
    fn traeger_verwirft_frames_ohne_bereitschaft() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let t = Traeger::neu(tx);
        t.frame(1, "{}".into());
        assert!(
            rx.try_recv().is_err(),
            "ohne Bereitschaft staut sich nichts"
        );
        t.bereit.store(true, Ordering::Relaxed);
        t.frame(1, "{}".into());
        assert!(matches!(
            rx.try_recv(),
            Ok(CarrierMsg::Frame { stream: 1, .. })
        ));
    }

    #[test]
    fn traeger_sendet_keine_nutzlast_ueber_dem_deckel() {
        let (t, mut rx) = traeger_bereit();
        t.frame(1, "x".repeat(relay_proto::MAX_CARRIER_PAYLOAD_LEN + 1));
        assert!(rx.try_recv().is_err());
        assert!(t.deckel_gemeldet.load(Ordering::Relaxed));
        // Genau am Deckel geht sie durch.
        t.frame(1, "x".repeat(relay_proto::MAX_CARRIER_PAYLOAD_LEN));
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn traeger_gilt_erst_ab_ready_als_stabil() {
        // Nie bereit geworden: nicht stabil, egal wie lange offen.
        assert_eq!(traeger_ende(None, Schluss::Abriss), SitzungsEnde::Abriss);
        let bereit = tokio::time::Instant::now();
        tokio::time::advance(Duration::from_secs(29)).await;
        assert_eq!(
            traeger_ende(Some(bereit), Schluss::Close(None)),
            SitzungsEnde::CloseOhneGrund
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(
            traeger_ende(Some(bereit), Schluss::Abriss),
            SitzungsEnde::Stabil
        );
    }
}
