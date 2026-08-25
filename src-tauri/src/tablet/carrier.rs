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

/// HTTPS/WSS-Basis des Cloud-Relays (identisch zum Relay-Client).
const RELAY_WS: &str = "wss://badhub.de/bts-relay";

/// Nach dieser Stille ohne jedes Lebenszeichen gilt der Träger als tot.
/// Gegenstück zu `HOST_STALE` im Relay; der Relay pingt, wir werten aus.
const TRAEGER_READ_IDLE: Duration = Duration::from_secs(15);

/// So lange darf ein lokales Gerät schweigen, bevor der Slave seinen
/// Substrom schließt und damit den Court-Slot freigibt.
///
/// Deckungsgleich mit `TABLET_STALE` im Relay — dieselbe Zusage wie am
/// direkten Weg, nur eben hier gemessen.
const GERAET_STALE: Duration = Duration::from_secs(15);

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
}

impl Traeger {
    fn neu(hinaus: mpsc::UnboundedSender<CarrierMsg>) -> Self {
        Self {
            hinaus,
            naechste: AtomicU32::new(1),
            stroeme: Mutex::new(HashMap::new()),
            bereit: AtomicBool::new(false),
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
    pub fn frame(&self, stream: u32, payload: String) {
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

/// Verbindungsschleife mit Backoff — dasselbe Muster wie `relay_client::run`.
async fn lauf(
    traeger: Arc<Traeger>,
    namespace: String,
    mut hinaus_rx: mpsc::UnboundedReceiver<CarrierMsg>,
) {
    let url = format!("{RELAY_WS}/{namespace}/carrier-ws");
    let mut backoff = 1u64;
    loop {
        match sitzung(&traeger, &url, &mut hinaus_rx).await {
            Ok(()) => tracing::info!("Träger beendet – neuer Versuch"),
            Err(e) => tracing::warn!("Träger-Verbindung: {e}"),
        }
        // Die Geräte hängen an dieser Verbindung; ohne Abbruch warteten sie
        // auf Antworten, die nie kommen.
        traeger.bereit.store(false, Ordering::Relaxed);
        traeger.alle_abbrechen();
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn sitzung(
    traeger: &Arc<Traeger>,
    url: &str,
    hinaus_rx: &mut mpsc::UnboundedReceiver<CarrierMsg>,
) -> Result<(), String> {
    let (stream, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| format!("Verbindung zu {url}: {e}"))?;
    let (mut sink, mut read) = stream.split();

    // Fassung aushandeln, bevor irgendein Gerät angenommen wird.
    let hallo = serde_json::to_string(&CarrierMsg::Hello {
        proto: CARRIER_PROTO_VERSION,
    })
    .map_err(|e| e.to_string())?;
    sink.send(WsMessage::Text(hallo.into()))
        .await
        .map_err(|e| format!("Begrüßung: {e}"))?;

    let mut letztes = tokio::time::Instant::now();
    let mut leerlauf = tokio::time::interval(TRAEGER_READ_IDLE / 3);
    leerlauf.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            eingang = read.next() => {
                let Some(nachricht) = eingang else { return Ok(()) };
                let nachricht = nachricht.map_err(|e| format!("Lesen: {e}"))?;
                letztes = tokio::time::Instant::now();
                match nachricht {
                    WsMessage::Text(t) => {
                        let Ok(frame) = serde_json::from_str::<CarrierServerMsg>(&t) else { continue };
                        match frame {
                            CarrierServerMsg::Ready { proto } => {
                                tracing::info!("Träger bereit (Fassung {proto})");
                                traeger.bereit.store(true, Ordering::Relaxed);
                            }
                            CarrierServerMsg::Unsupported { proto } => {
                                // Der Relay ist älter als wir. Nicht endlos
                                // weiterversuchen — der Aufrufer schickt die
                                // Geräte auf den Direkt-Cloud-Weg, und die
                                // Halle arbeitet weiter.
                                tracing::warn!(
                                    "Relay kennt Träger-Fassung {proto} nicht – Geräte laufen direkt über die Cloud"
                                );
                                return Err("Fassung abgelehnt".into());
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
                let roh = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
                sink.send(WsMessage::Text(roh.into()))
                    .await
                    .map_err(|e| format!("Senden: {e}"))?;
            }
            _ = leerlauf.tick() => {
                if letztes.elapsed() >= TRAEGER_READ_IDLE {
                    return Err(format!(
                        "seit {}s nichts vom Relay gehört",
                        letztes.elapsed().as_secs()
                    ));
                }
            }
        }
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
}
