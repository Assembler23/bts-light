//! Dienst für Punktspiele ohne Oberfläche: `bts-light --kopflos-dienst <spool>`.
//!
//! badhub legt je Begegnung einen Ordner `<spool>/<kennung>/` mit einem
//! Auftrag an (badhub `OhnePc`, `docs/features/punktspiel-kopflos.md`). Dieser
//! Dienst läuft dauerhaft auf dem Server und hält zu jedem Ordner genau einen
//! Prozess `bts-light --kopflos <ordner>` am Leben:
//!
//! - **starten**, sobald ein Ordner einen Auftrag oder eine Konfiguration hat;
//! - **neu starten** nach einem Absturz (Ende-Code 70 oder ein Signal), mit
//!   wachsender Pause;
//! - **nicht** neu starten nach einem Startfehler (Ende-Code 1 oder 2): dann
//!   braucht es einen neuen Auftrag. Der Ordner bekommt die Marke `fehler`;
//! - **beenden und abräumen**, sobald badhub die Marke `ende` hineinlegt —
//!   samt Gerätetoken und Konfiguration;
//! - **aufräumen**, was liegen bleibt: Ordner mit `fehler` nach einem Tag,
//!   jeden Ordner nach 14 Tagen.
//!
//! Ein Prozess je Begegnung und nicht alle in einem: der Kern hält seinen
//! Zustand prozessweit (Punktspiel, Live- oder Testsystem), und ein Absturz
//! trifft so nur eine Begegnung.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// Takt, in dem der Spool angesehen wird.
const TAKT: Duration = Duration::from_secs(3);
/// Höchstens so viele Prozesse zugleich (badhub begrenzt auf 30).
pub const PROZESSE_MAX: usize = 40;
/// Ein Ordner mit der Marke `fehler` bleibt so lange liegen (zum Nachsehen).
const FEHLER_FRIST: Duration = Duration::from_secs(24 * 3600);
/// Nach dieser Zeit endet jede Begegnung — ein Punktspiel dauert einen Tag.
const ORDNER_FRIST: Duration = Duration::from_secs(14 * 24 * 3600);

/// Was der Dienst von einem Ordner sieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lage {
    /// badhub hat das Ende verlangt (`ende`).
    pub ende: bool,
    /// Der letzte Start ist gescheitert (`fehler`).
    pub fehler: bool,
    /// Es gibt etwas zu starten (`auftrag.json` oder `config/config.json`).
    pub startbar: bool,
    /// Alter des Ordners.
    pub alter: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aktion {
    /// Prozess beenden (falls einer läuft) und den Ordner löschen.
    Abraeumen,
    Starten,
    Nichts,
}

/// Die eine Entscheidung je Ordner und Takt — rein, ohne Dateizugriff.
/// `pause` = nach einem Absturz noch nicht wieder starten; `platz` = die
/// Obergrenze der Prozesse ist nicht erreicht.
pub fn entscheide(lage: Lage, laeuft: bool, pause: bool, platz: bool) -> Aktion {
    if lage.ende || lage.alter >= ORDNER_FRIST || (lage.fehler && lage.alter >= FEHLER_FRIST) {
        return Aktion::Abraeumen;
    }
    if laeuft || lage.fehler || !lage.startbar || pause || !platz {
        return Aktion::Nichts;
    }
    Aktion::Starten
}

/// Pause vor dem n-ten Neustart nach einem Absturz: 5 s, 10 s … höchstens 60 s.
pub fn neustart_pause(versuche: u32) -> Duration {
    Duration::from_secs(u64::from(versuche.clamp(1, 12)) * 5)
}

/// Braucht dieser Ende-Code einen neuen Auftrag (kein Neustart)? `1` Start
/// fehlgeschlagen, `2` falscher Aufruf — siehe `kopflos`.
pub fn ist_startfehler(code: Option<i32>) -> bool {
    matches!(code, Some(1) | Some(2))
}

fn lage_lesen(ordner: &Path, jetzt: SystemTime) -> Lage {
    let alter = std::fs::metadata(ordner)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .ok()
        .and_then(|t| jetzt.duration_since(t).ok())
        .unwrap_or_default();
    Lage {
        ende: ordner.join("ende").exists(),
        fehler: ordner.join("fehler").exists(),
        startbar: ordner.join("auftrag.json").is_file()
            || ordner.join("config").join("config.json").is_file(),
        alter,
    }
}

/// Ordnername = Kennung der Installation (UUID): nichts anderes im Spool wird
/// angefasst, schon gar nicht gelöscht.
fn ist_kennung(name: &str) -> bool {
    name.len() == 36 && crate::quelle::badhub::uuid_gueltig(name)
}

struct Kind {
    prozess: Child,
}

/// Merker je Ordner nach einem Absturz.
struct Neustart {
    versuche: u32,
    fruehestens: Instant,
}

/// Der Dienst: Spool, laufende Prozesse und wie ein Prozess gestartet wird.
pub struct Dienst {
    spool: PathBuf,
    kinder: HashMap<String, Kind>,
    neustarts: HashMap<String, Neustart>,
    befehl: Box<dyn Fn(&Path) -> Command + Send>,
}

impl Dienst {
    pub fn neu(spool: PathBuf, befehl: Box<dyn Fn(&Path) -> Command + Send>) -> Self {
        Self {
            spool,
            kinder: HashMap::new(),
            neustarts: HashMap::new(),
            befehl,
        }
    }

    pub fn laufende(&self) -> usize {
        self.kinder.len()
    }

    fn beenden(&mut self, name: &str) {
        if let Some(mut k) = self.kinder.remove(name) {
            let _ = k.prozess.kill();
            let _ = k.prozess.wait();
            tracing::info!(target: "kopflos-dienst", "{name}: beendet");
        }
        self.neustarts.remove(name);
    }

    /// Beendete Prozesse einsammeln: Startfehler markieren, Abstürze für einen
    /// späteren Neustart vormerken.
    fn einsammeln(&mut self, jetzt: Instant) {
        let fertig: Vec<(String, Option<i32>)> = self
            .kinder
            .iter_mut()
            .filter_map(|(name, k)| match k.prozess.try_wait() {
                Ok(Some(status)) => Some((name.clone(), status.code())),
                _ => None,
            })
            .collect();
        for (name, code) in fertig {
            self.kinder.remove(&name);
            if ist_startfehler(code) {
                tracing::warn!(target: "kopflos-dienst", "{name}: Start fehlgeschlagen (Code {code:?}) — wartet auf einen neuen Auftrag");
                let _ = std::fs::write(self.spool.join(&name).join("fehler"), format!("{code:?}"));
                self.neustarts.remove(&name);
            } else {
                let n = self.neustarts.entry(name.clone()).or_insert(Neustart {
                    versuche: 0,
                    fruehestens: jetzt,
                });
                n.versuche += 1;
                n.fruehestens = jetzt + neustart_pause(n.versuche);
                tracing::warn!(target: "kopflos-dienst", "{name}: unerwartet beendet (Code {code:?}), Neustart in {} s", neustart_pause(n.versuche).as_secs());
            }
        }
    }

    /// Ein Takt: beendete Prozesse einsammeln, jeden Ordner ansehen, handeln.
    pub fn takt(&mut self, jetzt: Instant, uhr: SystemTime) {
        self.einsammeln(jetzt);
        let mut gesehen: Vec<String> = Vec::new();
        let eintraege = match std::fs::read_dir(&self.spool) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(target: "kopflos-dienst", "Spool nicht lesbar: {e}");
                return;
            }
        };
        for eintrag in eintraege.flatten() {
            let name = eintrag.file_name().to_string_lossy().into_owned();
            let ordner = eintrag.path();
            if !ist_kennung(&name) || !ordner.is_dir() {
                continue;
            }
            gesehen.push(name.clone());
            let pause = self
                .neustarts
                .get(&name)
                .is_some_and(|n| jetzt < n.fruehestens);
            let aktion = entscheide(
                lage_lesen(&ordner, uhr),
                self.kinder.contains_key(&name),
                pause,
                self.kinder.len() < PROZESSE_MAX,
            );
            match aktion {
                Aktion::Abraeumen => {
                    self.beenden(&name);
                    if let Err(e) = std::fs::remove_dir_all(&ordner) {
                        tracing::warn!(target: "kopflos-dienst", "{name}: Ordner nicht gelöscht: {e}");
                    } else {
                        tracing::info!(target: "kopflos-dienst", "{name}: abgeräumt");
                    }
                }
                Aktion::Starten => {
                    let mut befehl = (self.befehl)(&ordner);
                    befehl.stdin(Stdio::null());
                    match befehl.spawn() {
                        Ok(prozess) => {
                            tracing::info!(target: "kopflos-dienst", "{name}: gestartet (Prozess {})", prozess.id());
                            self.kinder.insert(name, Kind { prozess });
                        }
                        Err(e) => {
                            tracing::warn!(target: "kopflos-dienst", "{name}: Start nicht möglich: {e}")
                        }
                    }
                }
                Aktion::Nichts => {}
            }
        }
        // Prozess ohne Ordner (von Hand gelöscht): beenden.
        let verwaist: Vec<String> = self
            .kinder
            .keys()
            .filter(|n| !gesehen.contains(n))
            .cloned()
            .collect();
        for name in verwaist {
            self.beenden(&name);
        }
    }

    /// Alle Prozesse beenden (Ende des Dienstes).
    pub fn alle_beenden(&mut self) {
        let namen: Vec<String> = self.kinder.keys().cloned().collect();
        for name in namen {
            self.beenden(&name);
        }
    }
}

impl Drop for Dienst {
    fn drop(&mut self) {
        self.alle_beenden();
    }
}

/// `--kopflos-dienst <spool>`: läuft, bis der Prozess beendet wird. `None` =
/// kein solcher Aufruf.
pub fn einstieg(args: &[String]) -> Option<i32> {
    let i = args.iter().position(|a| a == "--kopflos-dienst")?;
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    let Some(spool) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("Aufruf: bts-light --kopflos-dienst <spool>");
        return Some(2);
    };
    if !spool.is_dir() {
        eprintln!(
            "bts-light --kopflos-dienst: Spool {} fehlt",
            spool.display()
        );
        return Some(1);
    }
    let programm = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("bts-light --kopflos-dienst: eigene Programmdatei nicht gefunden: {e}");
            return Some(1);
        }
    };
    tracing::info!(target: "kopflos-dienst", "Dienst läuft, Spool {}", spool.display());
    let mut dienst = Dienst::neu(
        spool,
        Box::new(move |ordner| {
            let mut c = Command::new(&programm);
            c.arg("--kopflos").arg(ordner);
            c
        }),
    );
    loop {
        dienst.takt(Instant::now(), SystemTime::now());
        std::thread::sleep(TAKT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K1: &str = "0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f";

    fn lage(ende: bool, fehler: bool, startbar: bool, alter_s: u64) -> Lage {
        Lage {
            ende,
            fehler,
            startbar,
            alter: Duration::from_secs(alter_s),
        }
    }

    #[test]
    fn entscheidung_je_ordner() {
        // Auftrag da, nichts läuft: starten
        assert_eq!(
            entscheide(lage(false, false, true, 5), false, false, true),
            Aktion::Starten
        );
        // läuft schon, Pause nach Absturz, kein Platz, nichts zu starten: warten
        assert_eq!(
            entscheide(lage(false, false, true, 5), true, false, true),
            Aktion::Nichts
        );
        assert_eq!(
            entscheide(lage(false, false, true, 5), false, true, true),
            Aktion::Nichts
        );
        assert_eq!(
            entscheide(lage(false, false, true, 5), false, false, false),
            Aktion::Nichts
        );
        assert_eq!(
            entscheide(lage(false, false, false, 5), false, false, true),
            Aktion::Nichts
        );
        // Startfehler: kein neuer Versuch, nach einem Tag weg
        assert_eq!(
            entscheide(lage(false, true, true, 60), false, false, true),
            Aktion::Nichts
        );
        assert_eq!(
            entscheide(lage(false, true, true, 25 * 3600), false, false, true),
            Aktion::Abraeumen
        );
        // Ende von badhub: immer abräumen, auch wenn er läuft
        assert_eq!(
            entscheide(lage(true, false, true, 5), true, false, true),
            Aktion::Abraeumen
        );
        // nach 14 Tagen endet alles
        assert_eq!(
            entscheide(lage(false, false, true, 15 * 24 * 3600), true, false, true),
            Aktion::Abraeumen
        );
    }

    #[test]
    fn neustart_pause_waechst_und_ist_gedeckelt() {
        assert_eq!(neustart_pause(1), Duration::from_secs(5));
        assert_eq!(neustart_pause(3), Duration::from_secs(15));
        assert_eq!(neustart_pause(99), Duration::from_secs(60));
        assert!(ist_startfehler(Some(1)) && ist_startfehler(Some(2)));
        assert!(!ist_startfehler(Some(70)) && !ist_startfehler(None) && !ist_startfehler(Some(0)));
    }

    #[test]
    fn nur_ordner_mit_kennung_zaehlen() {
        assert!(ist_kennung(K1));
        assert!(!ist_kennung("lost+found") && !ist_kennung("..") && !ist_kennung("0f3a1c2e"));
    }

    #[cfg(unix)]
    fn dienst_mit(spool: &Path, skript: &'static str) -> Dienst {
        Dienst::neu(
            spool.to_path_buf(),
            Box::new(move |ordner| {
                let mut c = Command::new("sh");
                c.arg("-c").arg(skript).arg("sh").arg(ordner);
                c
            }),
        )
    }

    #[cfg(unix)]
    fn warte_bis(mut f: impl FnMut() -> bool) -> bool {
        for _ in 0..100 {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[cfg(unix)]
    #[test]
    fn startet_zum_auftrag_und_raeumt_bei_ende_ab() {
        let spool = tempfile::tempdir().unwrap();
        let ordner = spool.path().join(K1);
        std::fs::create_dir(&ordner).unwrap();
        std::fs::create_dir(spool.path().join("kein-auftrag")).unwrap();
        let mut d = dienst_mit(spool.path(), "sleep 30");
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 0, "ohne Auftrag startet nichts");
        std::fs::write(ordner.join("auftrag.json"), b"{}").unwrap();
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 1);
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 1, "kein zweiter Prozess für denselben Ordner");
        // badhub verlangt das Ende
        std::fs::write(ordner.join("ende"), b"x").unwrap();
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 0);
        assert!(!ordner.exists(), "Ordner samt Inhalt ist weg");
        assert!(
            spool.path().join("kein-auftrag").exists(),
            "Fremdes im Spool bleibt unberührt"
        );
    }

    #[cfg(unix)]
    #[test]
    fn startfehler_wird_markiert_und_nicht_wiederholt() {
        let spool = tempfile::tempdir().unwrap();
        let ordner = spool.path().join(K1);
        std::fs::create_dir(&ordner).unwrap();
        std::fs::write(ordner.join("auftrag.json"), b"{}").unwrap();
        let mut d = dienst_mit(spool.path(), "exit 1");
        d.takt(Instant::now(), SystemTime::now());
        assert!(warte_bis(|| {
            d.takt(Instant::now(), SystemTime::now());
            ordner.join("fehler").exists()
        }));
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 0, "kein neuer Versuch ohne neuen Auftrag");
    }

    #[cfg(unix)]
    #[test]
    fn absturz_wird_nach_einer_pause_neu_gestartet() {
        let spool = tempfile::tempdir().unwrap();
        let ordner = spool.path().join(K1);
        std::fs::create_dir(&ordner).unwrap();
        std::fs::write(ordner.join("auftrag.json"), b"{}").unwrap();
        // erster Lauf stürzt ab (Code 70), danach läuft er
        let mut d = dienst_mit(
            spool.path(),
            "if [ -e \"$1/lief\" ]; then sleep 30; else touch \"$1/lief\"; exit 70; fi",
        );
        let start = Instant::now();
        d.takt(start, SystemTime::now());
        assert!(warte_bis(|| ordner.join("lief").exists()));
        assert!(warte_bis(|| {
            d.takt(start, SystemTime::now());
            d.laufende() == 0
        }));
        assert!(
            !ordner.join("fehler").exists(),
            "ein Absturz ist kein Startfehler"
        );
        d.takt(start + Duration::from_secs(1), SystemTime::now());
        assert_eq!(d.laufende(), 0, "in der Pause noch nicht");
        d.takt(start + Duration::from_secs(600), SystemTime::now());
        assert_eq!(d.laufende(), 1, "nach der Pause wieder gestartet");
    }

    #[cfg(unix)]
    #[test]
    fn prozess_ohne_ordner_wird_beendet() {
        let spool = tempfile::tempdir().unwrap();
        let ordner = spool.path().join(K1);
        std::fs::create_dir(&ordner).unwrap();
        std::fs::write(ordner.join("auftrag.json"), b"{}").unwrap();
        let mut d = dienst_mit(spool.path(), "sleep 30");
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 1);
        std::fs::remove_dir_all(&ordner).unwrap();
        d.takt(Instant::now(), SystemTime::now());
        assert_eq!(d.laufende(), 0);
    }
}
