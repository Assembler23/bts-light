//! Der badhub-Sender: ein eigener Task für alles, was an den
//! Liveticker-Endpunkt geht. Spec `badhub-push-entkoppeln`, PR 2a (ADR 0086).
//!
//! Bis v0.9.381 sendete der Sync-Zyklus selbst und wartete auf die Antwort —
//! ein hängendes badhub hielt damit Feldvergabe, Rotation, Druck und Ansage
//! des NÄCHSTEN Zyklus auf. Jetzt legt der Zyklus seinen Stand nur noch in
//! ein Postfach und läuft weiter; der Sender holt ihn ab.
//!
//! „Neuester gewinnt": Liegt noch ein ungesendeter Stand im Postfach, wenn
//! der nächste kommt, wird er ersetzt. Ein Stau alter Stände kann so nicht
//! entstehen, und nach einer langen Anfrage geht der aktuelle Stand hinaus,
//! nicht der von vor einer Minute.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Notify;

use crate::badhub::push::Transport;
use crate::badhub::strecke::{Anzeige, HaltGrund};
use crate::btp::model::BtpSnapshot;
use crate::config::AppConfig;
use crate::sync::PushTeil;
use crate::tablet::state::TabletState;

/// Das Postfach zwischen Sync-Zyklus und Sender.
#[derive(Default)]
struct Postfach {
    /// Der neueste noch nicht abgeholte Stand.
    stand: Mutex<Option<BtpSnapshot>>,
    wecker: Notify,
}

/// Griff auf das Postfach — der Zyklus liefert ab, der Sender holt ab.
#[derive(Clone, Default)]
pub struct Griff(Arc<Postfach>);

impl Griff {
    pub fn neu() -> Self {
        Self::default()
    }

    /// Legt den Stand ab und kehrt sofort zurück — kein Netz, kein Warten.
    /// Ein älterer, noch nicht abgeholter Stand wird ersetzt.
    pub fn stand_abliefern(&self, stand: BtpSnapshot) {
        *self.0.stand.lock().unwrap_or_else(|e| e.into_inner()) = Some(stand);
        // `notify_one` merkt sich das Signal, falls der Sender gerade sendet
        // — es geht nicht verloren.
        self.0.wecker.notify_one();
    }

    /// Wartet auf den nächsten Stand und nimmt ihn heraus.
    async fn abholen(&self) -> BtpSnapshot {
        loop {
            if let Some(stand) = self
                .0
                .stand
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                return stand;
            }
            self.0.wecker.notified().await;
        }
    }

    /// Liegt gerade ein nicht abgeholter Stand im Postfach? (Tests)
    #[cfg(test)]
    fn wartet(&self) -> Option<String> {
        self.0
            .stand
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.tournament_name.clone())
    }
}

/// Was der Sender über die Strecke weiß — für die Statuszeile. Liegt in
/// `AppState`; `get_status` rechnet daraus beim Abruf den Text („nächster
/// Versuch in N s" zählt so zwischen zwei Zyklen herunter).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PushLage {
    /// Noch keine Sendung ausgewertet (gerade gestartet, oder kein Sender).
    #[default]
    Unbekannt,
    Ok,
    Gestoert {
        naechster: Instant,
    },
    Gedrosselt {
        /// Uhrzeit des nächsten Versuchs, „HH:MM".
        uhrzeit: String,
    },
    Angehalten {
        wand: bool,
    },
    Abgelehnt {
        status: u16,
        naechster: Instant,
    },
    /// Der Sender-Task ist an einer Panik gestorben. Der Zyklus legt weiter
    /// Stände ab, aber niemand holt sie — ohne diese Lage zeigte die Zeile
    /// für immer „Liveticker aktuell".
    Ausgefallen,
}

impl PushLage {
    /// Übersetzt die Anzeige der Strecke. `jetzt_uhr` ist die Wanduhr für die
    /// Angabe „um HH:MM" (die Strecke selbst rechnet mit `Instant`).
    pub fn aus(
        anzeige: Anzeige,
        jetzt: Instant,
        jetzt_uhr: chrono::DateTime<chrono::Local>,
    ) -> Self {
        match anzeige {
            Anzeige::Frei => PushLage::Ok,
            Anzeige::Gestoert { rest } => PushLage::Gestoert {
                naechster: jetzt + rest,
            },
            Anzeige::Gedrosselt { rest } => PushLage::Gedrosselt {
                uhrzeit: (jetzt_uhr + chrono::Duration::from_std(rest).unwrap_or_default())
                    .format("%H:%M")
                    .to_string(),
            },
            Anzeige::Angehalten { grund } => PushLage::Angehalten {
                wand: grund == HaltGrund::Wand,
            },
            Anzeige::Abgelehnt { status, rest } => PushLage::Abgelehnt {
                status,
                naechster: jetzt + rest,
            },
        }
    }
}

/// Sekunden bis `ziel`, aufgerundet und mindestens 1 — „in 0 s" stünde sonst
/// da, bis der Sender den Versuch wirklich macht (er wacht mit dem nächsten
/// abgelieferten Stand auf, also bis zu einem Zyklus später).
pub fn sekunden_bis(ziel: Instant, jetzt: Instant) -> u64 {
    let rest = ziel.saturating_duration_since(jetzt);
    (rest.as_secs() + u64::from(rest.subsec_nanos() > 0)).max(1)
}

/// Meldet den Ausfall, wenn der Sender an einer Panik stirbt — nicht beim
/// gewollten Abbruch (Stoppen, Speichern): Dann wird die Future nur
/// fallengelassen, ohne Panik.
struct AusfallWache(LageSchlitz);

impl Drop for AusfallWache {
    fn drop(&mut self) {
        if std::thread::panicking() {
            *self.0.lock().unwrap_or_else(|e| e.into_inner()) = PushLage::Ausgefallen;
            tracing::error!(
                "badhub-Sender ausgefallen (Panik) — bis zum Neustart der Uebertragung geht nichts mehr an badhub"
            );
        }
    }
}

/// Der geteilte Schlitz für die Lage.
pub type LageSchlitz = Arc<Mutex<PushLage>>;

/// Was der Sender zum Arbeiten braucht.
pub struct SenderTeile {
    pub postfach: Griff,
    pub teil: PushTeil,
    /// Wertkopie der Konfiguration zum Zeitpunkt des Starts — wie der
    /// Sync-Zyklus. Ein Speichern der Einstellungen startet beide neu.
    pub config: AppConfig,
    pub transport: Arc<dyn Transport>,
    pub tablet: Arc<TabletState>,
    pub lage: LageSchlitz,
}

/// Die Sender-Schleife. Läuft, bis ihr Task abgebrochen wird (der
/// Sync-Loop hält sie unter einem Wächter — Stoppen oder Speichern beendet
/// sie, eine laufende Anfrage wird dabei verworfen).
///
/// Je abgeholtem Stand ein Durchgang des badhub-Teils: Liveticker, dann
/// Meldeliste, Spielplan, Punktverlauf — streng nacheinander. Was gesendet
/// wird, entscheidet weiterhin [`PushTeil::an_badhub_senden`]; neu ist nur,
/// WER wartet.
pub async fn lauf(teile: SenderTeile) {
    let SenderTeile {
        postfach,
        mut teil,
        config,
        transport,
        tablet,
        lage,
    } = teile;
    let _wache = AusfallWache(lage.clone());
    loop {
        let mut stand = postfach.abholen().await;
        // Die Tablet-Stände unmittelbar vor dem Senden noch einmal
        // überlegen: Der Stand kann im Postfach gelegen haben, während ein
        // Tablet weitergezählt hat — ein voller Stand soll nie einen
        // älteren Punkt tragen als den, der gerade bekannt ist.
        tablet.apply_tablet_scores(&mut stand);
        teil.an_badhub_senden(&config, transport.as_ref(), &tablet, stand)
            .await;
        *lage.lock().unwrap_or_else(|e| e.into_inner()) =
            PushLage::aus(teil.anzeige(), Instant::now(), chrono::Local::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::badhub::push::Transport;
    use crate::badhub::strecke::{Ausgang, Kanal, PushMerker, Strecke, Tor};
    use std::future::Future;
    use std::pin::Pin;
    use std::time::Duration;

    /// Obergrenze, wie lange ein Test auf den Sender wartet.
    const TEST_FRIST: Duration = Duration::from_secs(5);

    /// Was die Attrappe auf eine Sendung antwortet.
    #[derive(Clone)]
    enum Antwort {
        Ok,
        Status(u16),
        /// Keine Antwort (Netz weg).
        Transport,
        /// Hängt, bis der Test das Signal gibt — dann „ok".
        HaengtBis(Arc<Notify>),
        /// Der Sender-Task stirbt an dieser Sendung.
        Panik,
    }

    /// Transport ohne Netz: folgt einem Skript und merkt sich, was gesendet
    /// wurde.
    struct Attrappe {
        skript: Mutex<Vec<Antwort>>,
        gesendet: Mutex<Vec<(Kanal, String)>>,
    }

    impl Attrappe {
        fn neu(skript: Vec<Antwort>) -> Arc<Self> {
            Arc::new(Self {
                skript: Mutex::new(skript),
                gesendet: Mutex::new(Vec::new()),
            })
        }

        fn gesendet(&self) -> Vec<(Kanal, String)> {
            self.gesendet.lock().unwrap().clone()
        }

        fn tsets(&self) -> Vec<String> {
            self.gesendet()
                .into_iter()
                .filter(|(k, _)| *k == Kanal::Tset)
                .map(|(_, b)| b)
                .collect()
        }
    }

    impl Transport for Attrappe {
        fn sende<'a>(
            &'a self,
            _url: &'a str,
            _password: &'a str,
            kanal: Kanal,
            body: Vec<u8>,
        ) -> Pin<Box<dyn Future<Output = Ausgang> + Send + 'a>> {
            Box::pin(async move {
                self.gesendet
                    .lock()
                    .unwrap()
                    .push((kanal, String::from_utf8_lossy(&body).to_string()));
                // Die n-te Sendung bekommt die n-te Antwort; danach „ok".
                let antwort = {
                    let mut skript = self.skript.lock().unwrap();
                    if skript.is_empty() {
                        Antwort::Ok
                    } else {
                        skript.remove(0)
                    }
                };
                let gut = |code: u16| Ausgang {
                    status: Some(code),
                    echte_antwort: true,
                    ..Default::default()
                };
                match antwort {
                    Antwort::Ok => gut(200),
                    Antwort::Status(code) => gut(code),
                    Antwort::Transport => Ausgang {
                        fehler: Some("Verbindung abgelehnt".into()),
                        ..Default::default()
                    },
                    Antwort::HaengtBis(signal) => {
                        signal.notified().await;
                        gut(200)
                    }
                    Antwort::Panik => panic!("Testpanik im Sender"),
                }
            })
        }
    }

    /// Ein Stand mit dem Namen als Turniernamen — und dem ersten Spiel auf
    /// einem Feld dieses Namens: So unterscheiden sich zwei Stände in der
    /// Feldbelegung, und der Diff ergibt jedes Mal einen vollen `tset`.
    fn stand(name: &str) -> BtpSnapshot {
        let roh = include_bytes!("../../tests/fixtures/btp-tournament.bin");
        let knoten = crate::btp::proto::decode_response(roh).expect("dekodierbar");
        let mut stand = crate::btp::model::parse_snapshot(&knoten).expect("Snapshot");
        stand.tournament_name = name.to_string();
        let spiel = &mut stand.matches[0];
        spiel.status = crate::btp::model::MatchStatus::OnCourt;
        spiel.court = Some(name.to_string());
        spiel.court_id = Some(1);
        stand
    }

    fn config(mit_logo: bool) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.badhub.url = "http://unbenutzt.invalid/api".into();
        cfg.badhub.password = "pw".into();
        if mit_logo {
            cfg.tournament_logo.data = "QUJDREVG".into();
            cfg.tournament_logo.mime = "image/png".into();
        }
        cfg
    }

    struct Aufbau {
        postfach: Griff,
        lage: LageSchlitz,
        task: tokio::task::JoinHandle<()>,
    }

    fn starten(attrappe: Arc<Attrappe>, tor: Tor, mit_logo: bool) -> Aufbau {
        let postfach = Griff::neu();
        let lage = LageSchlitz::default();
        let task = tokio::spawn(lauf(SenderTeile {
            postfach: postfach.clone(),
            teil: PushTeil::mit_tor(tor),
            config: config(mit_logo),
            transport: attrappe,
            tablet: Arc::new(TabletState::default()),
            lage: lage.clone(),
        }));
        Aufbau {
            postfach,
            lage,
            task,
        }
    }

    /// Wartet, bis die Bedingung gilt (der Sender läuft in seinem eigenen
    /// Task).
    async fn bis(mut bedingung: impl FnMut() -> bool, was: &str) {
        let beginn = tokio::time::Instant::now();
        while !bedingung() {
            assert!(
                beginn.elapsed() < TEST_FRIST,
                "wartete vergeblich auf: {was}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ablieferung_kehrt_sofort_zurueck() {
        // Es gibt gar keinen Sender, der abholt — die Ablieferung ist
        // trotzdem sofort fertig (sie ist nicht einmal `async`), und im
        // Postfach liegt nur der neueste Stand.
        let postfach = Griff::neu();
        for name in ["A", "B", "C"] {
            postfach.stand_abliefern(stand(name));
        }
        assert_eq!(postfach.wartet().as_deref(), Some("C"));
    }

    #[tokio::test(start_paused = true)]
    async fn neuester_stand_gewinnt_waehrend_der_upload_haengt() {
        let signal = Arc::new(Notify::new());
        let attrappe = Attrappe::neu(vec![Antwort::HaengtBis(signal.clone())]);
        let a = starten(attrappe.clone(), Strecke::frisch().als_tor(), false);

        a.postfach.stand_abliefern(stand("A"));
        bis(|| attrappe.tsets().len() == 1, "erster Upload beginnt").await;
        // Während der Upload hängt, kommen zwei weitere Stände.
        a.postfach.stand_abliefern(stand("B"));
        a.postfach.stand_abliefern(stand("C"));
        tokio::time::sleep(Duration::from_secs(30)).await;
        assert_eq!(attrappe.tsets().len(), 1, "nichts überholt den Upload");

        signal.notify_one();
        bis(|| attrappe.tsets().len() == 2, "zweiter Upload").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let tsets = attrappe.tsets();
        assert_eq!(tsets.len(), 2, "B wurde nie gesendet");
        assert!(tsets[0].contains("\"A\""), "{}", tsets[0]);
        assert!(tsets[1].contains("\"C\""), "{}", tsets[1]);
        assert_eq!(*a.lage.lock().unwrap(), PushLage::Ok);
    }

    #[tokio::test(start_paused = true)]
    async fn nach_stoerung_folgt_ein_voller_tset_ohne_logo() {
        let attrappe = Attrappe::neu(vec![Antwort::Transport]);
        let tor = Strecke::frisch().als_tor();
        let a = starten(attrappe.clone(), tor.clone(), true);

        a.postfach.stand_abliefern(stand("T"));
        bis(
            || matches!(*a.lage.lock().unwrap(), PushLage::Gestoert { .. }),
            "Störung erkannt",
        )
        .await;
        assert!(attrappe.tsets()[0].contains("tournament_logo"));

        // Der nächste Zyklus liefert ab, die Frist läuft noch: nichts geht.
        a.postfach.stand_abliefern(stand("T"));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(attrappe.gesendet().len(), 1);

        // Nach der Frist: ein voller Stand, ohne Logo.
        tor.lock().unwrap().vorspulen(Duration::from_secs(5));
        a.postfach.stand_abliefern(stand("T"));
        bis(|| attrappe.tsets().len() == 2, "Probe-tset").await;
        assert!(!attrappe.tsets()[1].contains("tournament_logo"));
        bis(|| *a.lage.lock().unwrap() == PushLage::Ok, "wieder frei").await;
    }

    #[tokio::test(start_paused = true)]
    async fn halt_endet_erst_mit_neuem_sender() {
        let merker = Arc::new(Mutex::new(PushMerker::default()));
        let neues_tor = || Strecke::neu(merker.clone(), "u", "z", Instant::now()).als_tor();
        let attrappe = Attrappe::neu(vec![Antwort::Status(401)]);
        let tor = neues_tor();
        let a = starten(attrappe.clone(), tor.clone(), false);

        a.postfach.stand_abliefern(stand("T"));
        bis(
            || *a.lage.lock().unwrap() == PushLage::Angehalten { wand: false },
            "Halt",
        )
        .await;
        // Auch Stunden und viele Zyklen später: kein weiterer Versuch.
        for _ in 0..5 {
            tor.lock().unwrap().vorspulen(Duration::from_secs(3600));
            a.postfach.stand_abliefern(stand("T"));
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        assert_eq!(attrappe.gesendet().len(), 1);

        // Speichern der Einstellungen: alter Sender weg, neuer Sender mit
        // neuer Strecke — der Halt ist aufgehoben.
        a.task.abort();
        let b = starten(attrappe.clone(), neues_tor(), false);
        b.postfach.stand_abliefern(stand("T"));
        bis(|| attrappe.gesendet().len() >= 2, "neuer Versuch").await;
        bis(|| *b.lage.lock().unwrap() == PushLage::Ok, "ok").await;
    }

    #[tokio::test(start_paused = true)]
    async fn abbruch_beendet_den_sender() {
        let signal = Arc::new(Notify::new());
        let attrappe = Attrappe::neu(vec![Antwort::HaengtBis(signal.clone())]);
        let a = starten(attrappe.clone(), Strecke::frisch().als_tor(), false);
        a.postfach.stand_abliefern(stand("A"));
        bis(|| attrappe.gesendet().len() == 1, "Upload läuft").await;

        // Stoppen mitten in der Anfrage: Sie wird verworfen, und was danach
        // im Postfach landet, holt niemand mehr ab.
        a.task.abort();
        signal.notify_one();
        a.postfach.stand_abliefern(stand("B"));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(attrappe.gesendet().len(), 1);
        assert_eq!(a.postfach.wartet().as_deref(), Some("B"));
        assert_eq!(*a.lage.lock().unwrap(), PushLage::Unbekannt);
    }

    #[tokio::test(start_paused = true)]
    async fn panik_im_sender_wird_gemeldet() {
        let attrappe = Attrappe::neu(vec![Antwort::Panik]);
        let a = starten(attrappe.clone(), Strecke::frisch().als_tor(), false);
        a.postfach.stand_abliefern(stand("A"));
        bis(
            || *a.lage.lock().unwrap() == PushLage::Ausgefallen,
            "Ausfall gemeldet",
        )
        .await;
        assert!(a.task.await.unwrap_err().is_panic());
    }

    #[test]
    fn countdown_rundet_auf_und_zeigt_nie_null() {
        let jetzt = Instant::now();
        assert_eq!(sekunden_bis(jetzt + Duration::from_millis(4200), jetzt), 5);
        assert_eq!(sekunden_bis(jetzt + Duration::from_secs(20), jetzt), 20);
        assert_eq!(sekunden_bis(jetzt, jetzt), 1);
        assert_eq!(sekunden_bis(jetzt, jetzt + Duration::from_secs(9)), 1);
    }
}
