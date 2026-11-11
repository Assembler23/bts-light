//! Gemeinsamer Verbindungsaufbau zum Cloud-Relay — für den Host
//! ([`super::relay_client`]) und den Träger der fernen Halle
//! ([`super::carrier`]). Spec `relay-haerten`, PR 2.
//!
//! Zwei Dinge fehlten beiden: eine **Frist** um den Aufbau (hinter einem
//! Captive Portal hing `connect_async` unbegrenzt) und ein Backoff, der
//! nicht schon beim Handshake zurückspringt. Wies der Relay die Verbindung
//! gleich wieder ab, entstand sonst ein Dauer-Reconnect im Sekundentakt.

use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Frist für DNS, TCP, TLS und WebSocket-Upgrade zusammen.
pub const VERBINDUNGS_FRIST: Duration = Duration::from_secs(10);

/// Erst ab dieser Dauer gilt eine Sitzung als stabil und die Pause springt
/// auf den Anfang zurück.
pub const STABIL_AB: Duration = Duration::from_secs(30);

/// Längste Pause zwischen zwei Versuchen, wenn das Netz oder der Relay
/// nicht erreichbar ist.
const PAUSE_MAX: u64 = 30;

/// Längste Pause, wenn der Relay erreichbar ist, die Verbindung aber gleich
/// wieder schließt (Platz belegt, Relay startet neu). Bewusst kurz: Nach
/// einem Netzwechsel ist die eigene tote Verbindung bis zu 15 s lang der
/// „andere" Host — wer dann dicht nachfragt, übernimmt nach rund 20 s statt
/// nach 45 s.
const PAUSE_MAX_ABWEISUNG: u64 = 5;

/// Ein Abriss so kurz nach dem Aufbau zählt wie eine Abweisung. Der Relay
/// schließt bei belegtem Platz sofort, ohne zu lesen; der Host hat dann
/// meist schon Frames geschickt. Liegen sie ungelesen im Puffer, beendet das
/// Betriebssystem des Relays die Verbindung mit einem Reset — und unter
/// Windows kommt der Close-Frame dann womöglich gar nicht mehr an
/// (Review-Befund, nicht nachgemessen). Ohne diese Regel wüchse die Pause
/// in genau dem Fall bis 30 s, für den die 5 s gedacht sind.
const SOFORT_ABRISS: Duration = Duration::from_secs(5);

/// Close-Code des Relays bei belegtem Host-Platz. Bis der Relay ihn sendet
/// (Spec `relay-haerten`, PR 7, dann in `relay-proto`), steht er hier — ein
/// älterer Relay schließt ohne Grund, was der Automat gleich behandelt.
pub const CLOSE_HOST_BELEGT: u16 = 4409;

/// Der Strom einer ausgehenden Relay-Verbindung.
pub type Strom = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Baut die WebSocket-Verbindung auf — höchstens `frist` lang.
pub async fn verbinden(url: &str, frist: Duration) -> Result<Strom, String> {
    match tokio::time::timeout(frist, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((strom, _))) => Ok(strom),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!(
            "Verbindungsaufbau dauerte länger als {} s",
            frist.as_secs()
        )),
    }
}

/// Wie eine aufgebaute Verbindung zu Ende ging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schluss {
    /// Die Gegenseite hat ordentlich geschlossen, mit oder ohne Close-Code.
    Close(Option<u16>),
    /// Lesefehler, Stille, Sendefehler, Strom ohne Close beendet.
    Abriss,
}

/// Ausgang eines Verbindungsversuchs, aus Sicht des Backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitzungsEnde {
    /// Die Sitzung stand mindestens [`STABIL_AB`].
    Stabil,
    /// Der Aufbau scheiterte (Netz, Frist, 502).
    AufbauGescheitert,
    /// Der Relay hat mit „Host belegt" geschlossen.
    Abgewiesen,
    /// Ordentlicher Close kurz nach dem Aufbau, ohne (bekannten) Grund —
    /// so schließt auch ein älterer Relay bei belegtem Platz.
    CloseOhneGrund,
    /// Abriss kurz nach dem Aufbau.
    Abriss,
}

/// Ordnet das Ende einer **aufgebauten** Sitzung ein.
pub fn einordnen(dauer: Duration, schluss: Schluss) -> SitzungsEnde {
    if dauer >= STABIL_AB {
        return SitzungsEnde::Stabil;
    }
    match schluss {
        Schluss::Close(Some(CLOSE_HOST_BELEGT)) => SitzungsEnde::Abgewiesen,
        Schluss::Close(_) => SitzungsEnde::CloseOhneGrund,
        Schluss::Abriss if dauer < SOFORT_ABRISS => SitzungsEnde::CloseOhneGrund,
        Schluss::Abriss => SitzungsEnde::Abriss,
    }
}

/// Backoff zwischen den Verbindungsversuchen, als reiner Automat.
#[derive(Debug)]
pub struct Backoff {
    /// Pause vor dem nächsten Versuch, in Sekunden (ohne Jitter).
    naechste: u64,
}

impl Default for Backoff {
    fn default() -> Self {
        Self { naechste: 1 }
    }
}

impl Backoff {
    /// Die Pause nach diesem Ausgang (ohne Jitter) — und merkt sich die
    /// verdoppelte für den nächsten Fehlschlag.
    pub fn pause_nach(&mut self, ende: SitzungsEnde) -> Duration {
        let deckel = match ende {
            SitzungsEnde::Stabil => {
                self.naechste = 1;
                PAUSE_MAX
            }
            SitzungsEnde::AufbauGescheitert | SitzungsEnde::Abriss => PAUSE_MAX,
            SitzungsEnde::Abgewiesen | SitzungsEnde::CloseOhneGrund => PAUSE_MAX_ABWEISUNG,
        };
        let pause = self.naechste.min(deckel);
        self.naechste = (pause * 2).min(deckel);
        Duration::from_secs(pause)
    }
}

/// Streut eine Pause um ± 20 %. `zufall` liegt in `0.0..=1.0`. Ohne Streuung
/// kämen nach einem Relay-Neustart alle Turnier-PCs im selben Augenblick.
pub fn mit_jitter(pause: Duration, zufall: f64) -> Duration {
    pause.mul_f64(0.8 + 0.4 * zufall.clamp(0.0, 1.0))
}

/// Eine Zufallszahl in `0.0..1.0` — ohne eigene Abhängigkeit: Der
/// Hash-Zustand der Standardbibliothek ist je Aufruf zufällig gesät, das
/// genügt zum Streuen einer Pause.
pub fn zufall() -> f64 {
    let wert = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    (wert >> 11) as f64 / (1u64 << 53) as f64
}

// ── Dashboard-Meldung „anderer PC" (Spec relay-haerten, PR 9) ──────────

/// Erst nach so langer Abweisung erscheint die Meldung. Nach einem
/// Netzwechsel ist die EIGENE tote Verbindung bis zu 15 s lang der „andere
/// PC" (so lange hält der Relay einen stummen Turnier-PC) — das soll
/// niemanden erschrecken.
pub const ABWEISUNG_MELDEN_NACH_MS: u64 = 30_000;

/// Der Text der Meldung.
pub const ANDERER_PC_TEXT: &str = "Ein anderer PC ist mit diesem Turnier verbunden";

/// Lage der Cloud-Verbindung, soweit das Dashboard sie braucht. Reiner
/// Wert; die Uhr kommt von außen.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CloudLage {
    /// Seit wann der Relay diesen PC mit „Host belegt" abweist (Unix-ms).
    abgewiesen_seit_ms: Option<u64>,
    /// Seit wann die laufende Verbindung steht (Unix-ms).
    verbunden_seit_ms: Option<u64>,
}

impl CloudLage {
    /// Eine Verbindung ist aufgebaut. Das löscht die Abweisung noch NICHT:
    /// Auch eine abgewiesene Verbindung ist erst einmal aufgebaut, bevor
    /// der Relay sie mit Grund schließt.
    pub fn verbunden(&mut self, jetzt_ms: u64) {
        self.verbunden_seit_ms = Some(jetzt_ms);
    }

    /// Eine Sitzung ist zu Ende.
    pub fn sitzung_ende(&mut self, ende: SitzungsEnde, jetzt_ms: u64) {
        self.verbunden_seit_ms = None;
        match ende {
            SitzungsEnde::Abgewiesen => {
                self.abgewiesen_seit_ms.get_or_insert(jetzt_ms);
            }
            // Ein Close ohne Grund oder ein gescheiterter Aufbau sagt nichts
            // über den Platz: Der Close-Frame einer Abweisung kann verloren
            // gehen, nginx kann einmal 502 liefern. Eine laufende Abweisung
            // bleibt deshalb stehen — sonst begänne ihre Frist bei jedem
            // solchen Aussetzer von vorn. (Ein älterer Relay nennt nie einen
            // Grund; mit ihm beginnt nie eine Abweisung.)
            SitzungsEnde::CloseOhneGrund | SitzungsEnde::AufbauGescheitert => {}
            // Die Verbindung stand: Dieser PC hatte den Platz.
            SitzungsEnde::Stabil | SitzungsEnde::Abriss => self.abgewiesen_seit_ms = None,
        }
    }

    /// Die Meldung fürs Dashboard, wenn die Abweisung lange genug anhält.
    pub fn meldung(&self, jetzt_ms: u64) -> Option<&'static str> {
        let seit = self.abgewiesen_seit_ms?;
        // Steht die laufende Verbindung länger, als eine Abweisung dauert,
        // hat dieser PC den Platz bekommen.
        let steht = self
            .verbunden_seit_ms
            .is_some_and(|v| jetzt_ms.saturating_sub(v) >= SOFORT_ABRISS.as_millis() as u64);
        (!steht && jetzt_ms.saturating_sub(seit) >= ABWEISUNG_MELDEN_NACH_MS)
            .then_some(ANDERER_PC_TEXT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verbindung_gegen_stummen_port_endet_nach_frist() {
        // Der Port nimmt die TCP-Verbindung an, antwortet aber nie auf das
        // Upgrade — wie ein Captive Portal, das Pakete schluckt.
        let lauscher = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = lauscher.local_addr().unwrap().port();
        let start = std::time::Instant::now();
        let ergebnis = verbinden(
            &format!("ws://127.0.0.1:{port}/x"),
            Duration::from_millis(200),
        )
        .await;
        let Err(fehler) = ergebnis else {
            panic!("darf nicht verbinden");
        };
        assert!(fehler.contains("länger"), "{fehler}");
        assert!(start.elapsed() < Duration::from_secs(5));
        drop(lauscher);
    }

    fn sekunden(b: &mut Backoff, ende: SitzungsEnde) -> u64 {
        b.pause_nach(ende).as_secs()
    }

    #[test]
    fn aufbaufehler_verdoppeln_bis_dreissig_sekunden() {
        let mut b = Backoff::default();
        let folge: Vec<u64> = (0..7)
            .map(|_| sekunden(&mut b, SitzungsEnde::AufbauGescheitert))
            .collect();
        assert_eq!(folge, vec![1, 2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn backoff_setzt_erst_nach_stabiler_sitzung_zurueck() {
        let mut b = Backoff::default();
        for _ in 0..4 {
            b.pause_nach(SitzungsEnde::AufbauGescheitert);
        }
        // Verbunden, aber nach wenigen Sekunden wieder weg: Die Pause wächst
        // weiter (früher sprang sie schon beim Handshake auf 1 s zurück).
        assert_eq!(sekunden(&mut b, SitzungsEnde::Abriss), 16);
        assert_eq!(sekunden(&mut b, SitzungsEnde::Abriss), 30);
        // Erst eine stabile Sitzung setzt zurück.
        assert_eq!(sekunden(&mut b, SitzungsEnde::Stabil), 1);
        assert_eq!(sekunden(&mut b, SitzungsEnde::Abriss), 2);
    }

    #[test]
    fn abweisung_pausiert_hoechstens_fuenf_sekunden() {
        let mut b = Backoff::default();
        let folge: Vec<u64> = (0..5)
            .map(|_| sekunden(&mut b, SitzungsEnde::Abgewiesen))
            .collect();
        assert_eq!(folge, vec![1, 2, 4, 5, 5]);
        // Auch aus einer langen Netz-Pause heraus.
        let mut b = Backoff::default();
        for _ in 0..7 {
            b.pause_nach(SitzungsEnde::AufbauGescheitert);
        }
        assert_eq!(sekunden(&mut b, SitzungsEnde::Abgewiesen), 5);
    }

    #[test]
    fn close_ohne_grund_zaehlt_wie_abweisung() {
        // Ein älterer Relay schließt bei belegtem Platz ohne Grund.
        let kurz = Duration::from_secs(1);
        assert_eq!(
            einordnen(kurz, Schluss::Close(None)),
            SitzungsEnde::CloseOhneGrund
        );
        assert_eq!(
            einordnen(kurz, Schluss::Close(Some(1000))),
            SitzungsEnde::CloseOhneGrund
        );
        assert_eq!(
            einordnen(kurz, Schluss::Close(Some(CLOSE_HOST_BELEGT))),
            SitzungsEnde::Abgewiesen
        );
        // Ein Abriss gleich nach dem Aufbau ist dieselbe Lage (der Close-Frame
        // kann unterwegs verloren gehen); erst ein späterer ist ein Netzproblem.
        assert_eq!(
            einordnen(kurz, Schluss::Abriss),
            SitzungsEnde::CloseOhneGrund
        );
        assert_eq!(
            einordnen(SOFORT_ABRISS, Schluss::Abriss),
            SitzungsEnde::Abriss
        );
        let mut a = Backoff::default();
        let mut b = Backoff::default();
        for _ in 0..5 {
            assert_eq!(
                a.pause_nach(SitzungsEnde::CloseOhneGrund),
                b.pause_nach(SitzungsEnde::Abgewiesen)
            );
        }
    }

    #[test]
    fn stabile_sitzung_zaehlt_unabhaengig_vom_schluss() {
        for schluss in [
            Schluss::Abriss,
            Schluss::Close(None),
            Schluss::Close(Some(CLOSE_HOST_BELEGT)),
        ] {
            assert_eq!(einordnen(STABIL_AB, schluss), SitzungsEnde::Stabil);
        }
    }

    #[test]
    fn jitter_bleibt_im_band() {
        let pause = Duration::from_secs(10);
        assert_eq!(mit_jitter(pause, 0.0), Duration::from_secs(8));
        assert_eq!(mit_jitter(pause, 1.0), Duration::from_secs(12));
        assert_eq!(mit_jitter(pause, 0.5), Duration::from_secs(10));
        // Werte außerhalb werden eingefangen.
        assert_eq!(mit_jitter(pause, 7.0), Duration::from_secs(12));
        for _ in 0..200 {
            let z = zufall();
            assert!((0.0..1.0).contains(&z), "{z}");
            let p = mit_jitter(pause, z);
            assert!(p >= Duration::from_secs(8) && p <= Duration::from_secs(12));
        }
    }

    #[test]
    fn abweisung_wird_erst_nach_dreissig_sekunden_gemeldet() {
        let mut lage = CloudLage::default();
        assert_eq!(lage.meldung(0), None);
        // Der Relay weist ab — immer wieder (jeder Versuch baut erst auf).
        let mut t = 1_000;
        lage.verbunden(t);
        lage.sitzung_ende(SitzungsEnde::Abgewiesen, t + 50);
        assert_eq!(lage.meldung(t + 50), None, "nicht sofort");
        for _ in 0..5 {
            t += 5_000;
            lage.verbunden(t);
            lage.sitzung_ende(SitzungsEnde::Abgewiesen, t + 50);
        }
        // 25 s nach der ersten Abweisung: noch still.
        assert_eq!(lage.meldung(t + 50), None);
        // Ab 30 s: Meldung — gezählt ab der ERSTEN Abweisung.
        assert_eq!(lage.meldung(1_050 + ABWEISUNG_MELDEN_NACH_MS - 1), None);
        assert_eq!(
            lage.meldung(1_050 + ABWEISUNG_MELDEN_NACH_MS),
            Some(ANDERER_PC_TEXT)
        );
    }

    #[test]
    fn meldung_verschwindet_nach_verbindung() {
        let mut lage = CloudLage::default();
        lage.sitzung_ende(SitzungsEnde::Abgewiesen, 0);
        let spaet = ABWEISUNG_MELDEN_NACH_MS + 10_000;
        assert_eq!(lage.meldung(spaet), Some(ANDERER_PC_TEXT));
        // Ein neuer Versuch ist aufgebaut: Das allein beweist nichts — auch
        // eine Abweisung beginnt so.
        lage.verbunden(spaet);
        assert_eq!(lage.meldung(spaet + 1_000), Some(ANDERER_PC_TEXT));
        // Steht die Verbindung länger, als eine Abweisung dauert, ist die
        // Meldung weg — ohne dass die Sitzung enden müsste.
        assert_eq!(lage.meldung(spaet + 5_000), None);
        // Und nach ihrem Ende bleibt sie weg.
        lage.sitzung_ende(SitzungsEnde::Stabil, spaet + 600_000);
        assert_eq!(lage.meldung(spaet + 600_000), None);
    }

    #[test]
    fn alter_relay_ohne_grund_meldet_nie() {
        let mut lage = CloudLage::default();
        for t in 0..100 {
            lage.verbunden(t * 5_000);
            lage.sitzung_ende(SitzungsEnde::CloseOhneGrund, t * 5_000 + 10);
        }
        assert_eq!(lage.meldung(10_000_000), None);
    }

    #[test]
    fn verlorener_close_grund_setzt_die_frist_nicht_zurueck() {
        // Dauer-Abweisung, bei der jeder zweite Close-Frame verloren geht
        // (dann sieht die App nur einen Close ohne Grund) und nginx einmal
        // 502 liefert.
        let mut lage = CloudLage::default();
        lage.sitzung_ende(SitzungsEnde::Abgewiesen, 0);
        lage.sitzung_ende(SitzungsEnde::CloseOhneGrund, 5_000);
        lage.sitzung_ende(SitzungsEnde::AufbauGescheitert, 10_000);
        lage.sitzung_ende(SitzungsEnde::Abgewiesen, 15_000);
        lage.sitzung_ende(SitzungsEnde::CloseOhneGrund, 20_000);
        assert_eq!(
            lage.meldung(ABWEISUNG_MELDEN_NACH_MS),
            Some(ANDERER_PC_TEXT),
            "gezählt ab der ersten Abweisung"
        );
        // Stand die Verbindung dagegen (Abriss nach mehr als 5 s), hatte
        // dieser PC den Platz: Eine neue Abweisung beginnt eine neue Frist.
        lage.sitzung_ende(SitzungsEnde::Abriss, 40_000);
        assert_eq!(lage.meldung(50_000), None);
        lage.sitzung_ende(SitzungsEnde::Abgewiesen, 50_000);
        assert_eq!(lage.meldung(50_000 + ABWEISUNG_MELDEN_NACH_MS - 1), None);
        assert_eq!(
            lage.meldung(50_000 + ABWEISUNG_MELDEN_NACH_MS),
            Some(ANDERER_PC_TEXT)
        );
    }
}
