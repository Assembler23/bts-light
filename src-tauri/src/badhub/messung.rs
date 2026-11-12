//! Feldmessung der badhub-Strecke (Spec `badhub-push-entkoppeln`,
//! Messschritt B): **eine** Sammelzeile je Minute im Diagnose-Log.
//!
//! Das Log läuft auf Info-Stufe; eine Zeile je Sendung wären bei zwei
//! Punkten je Sekunde einige tausend Zeilen je Stunde. Deshalb wird hier
//! gesammelt: je Kanal Anzahl, Median und Maximum der Dauer, größter Körper
//! und Fehlschläge — dazu, wie oft der Spielplan gegenüber dem vorigen
//! unverändert hinausging. Aus diesen Zahlen fallen die Entscheidungen vor
//! PR 2b (trägt die serielle Strecke?), PR 3 und PR 5.
//!
//! In der Zeile stehen nur Kanalnamen und Zahlen — nie ein Körper, nie ein
//! Name, nie die Adresse.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::strecke::{Ausgang, Kanal};

/// Länge eines Sammelfensters.
const FENSTER: Duration = Duration::from_secs(60);

/// So viele Dauern hält ein Kanal je Fenster höchstens für den Median. Mehr
/// Sendungen zählen weiter (Anzahl, Maximum), nur der Median stützt sich dann
/// auf die ersten Werte — der Speicher bleibt gedeckelt.
const MAX_DAUERN: usize = 4096;

/// Reihenfolge der Kanäle in der Zeile.
const KANAELE: [Kanal; 5] = [
    Kanal::Tupdate,
    Kanal::Tset,
    Kanal::Sched,
    Kanal::Meldeliste,
    Kanal::Verlauf,
];

#[derive(Default)]
struct KanalSumme {
    anzahl: u32,
    dauern_ms: Vec<u32>,
    max_ms: u32,
    max_bytes: usize,
    fehler: u32,
}

impl KanalSumme {
    fn median_ms(&self) -> u32 {
        let mut sortiert = self.dauern_ms.clone();
        sortiert.sort_unstable();
        sortiert.get(sortiert.len() / 2).copied().unwrap_or(0)
    }
}

/// Ein Sammelfenster. Rein — die Uhr kommt von außen, damit die Tests ohne
/// Warten auskommen.
#[derive(Default)]
pub struct Fenster {
    beginn: Option<Instant>,
    kanaele: [KanalSumme; 5],
    /// Spielplan ging hinaus und war inhaltlich derselbe wie der vorige.
    sched_gleich: u32,
    /// Spielplan ging hinaus und hatte sich geändert (oder war der erste).
    sched_geaendert: u32,
}

fn platz(kanal: Kanal) -> usize {
    KANAELE
        .iter()
        .position(|k| *k == kanal)
        .expect("jeder Kanal steht in KANAELE")
}

impl Fenster {
    /// Schließt das Fenster, wenn es abgelaufen ist, und gibt seine Zeile
    /// zurück. Ein Fenster ohne jede Sendung ergibt keine Zeile.
    fn abschliessen_wenn_faellig(&mut self, jetzt: Instant) -> Option<String> {
        let beginn = *self.beginn.get_or_insert(jetzt);
        let lief = jetzt.saturating_duration_since(beginn);
        if lief < FENSTER {
            return None;
        }
        let zeile = self.zeile(lief);
        *self = Fenster {
            beginn: Some(jetzt),
            ..Default::default()
        };
        zeile
    }

    /// Vermerkt eine Sendung. Liefert die Zeile des **vorigen** Fensters,
    /// wenn es mit dieser Sendung abgelaufen ist.
    pub fn sendung(
        &mut self,
        jetzt: Instant,
        kanal: Kanal,
        erfolg: bool,
        bytes: usize,
        dauer: Duration,
    ) -> Option<String> {
        let zeile = self.abschliessen_wenn_faellig(jetzt);
        let ms = u32::try_from(dauer.as_millis()).unwrap_or(u32::MAX);
        let k = &mut self.kanaele[platz(kanal)];
        k.anzahl += 1;
        if k.dauern_ms.len() < MAX_DAUERN {
            k.dauern_ms.push(ms);
        }
        k.max_ms = k.max_ms.max(ms);
        k.max_bytes = k.max_bytes.max(bytes);
        if !erfolg {
            k.fehler += 1;
        }
        zeile
    }

    /// Vermerkt, ob ein hinausgehender Spielplan sich gegenüber dem vorigen
    /// geändert hat.
    pub fn spielplan(&mut self, geaendert: bool) {
        if geaendert {
            self.sched_geaendert += 1;
        } else {
            self.sched_gleich += 1;
        }
    }

    fn zeile(&self, lief: Duration) -> Option<String> {
        let mut teile = Vec::new();
        for (kanal, k) in KANAELE.iter().zip(&self.kanaele) {
            if k.anzahl == 0 {
                continue;
            }
            teile.push(format!(
                "{} n={} median={}ms max={}ms groesste={}B fehler={}",
                kanal.name(),
                k.anzahl,
                k.median_ms(),
                k.max_ms,
                k.max_bytes,
                k.fehler
            ));
        }
        if teile.is_empty() {
            return None;
        }
        if self.sched_gleich + self.sched_geaendert > 0 {
            teile.push(format!(
                "spielplan geaendert={} gleich={}",
                self.sched_geaendert, self.sched_gleich
            ));
        }
        Some(format!(
            "badhub-Messung ({} s): {}",
            lief.as_secs(),
            teile.join("; ")
        ))
    }
}

/// Das eine Fenster des Prozesses: Gesendet wird aus dem Sender-Task **und**
/// aus den Punkt-Arbeitern der Tablets — beide sollen in derselben Zeile
/// stehen.
static FENSTER_JETZT: Mutex<Option<Fenster>> = Mutex::new(None);

fn mit_fenster<T>(f: impl FnOnce(&mut Fenster) -> T) -> T {
    // Ein vergifteter Mutex darf das Senden nicht stören: Es ist nur Statistik.
    let mut riegel = FENSTER_JETZT.lock().unwrap_or_else(|e| e.into_inner());
    f(riegel.get_or_insert_with(Fenster::default))
}

/// Vermerkt eine Sendung an badhub und schreibt, wenn die Minute um ist, die
/// Sammelzeile ins Log.
pub fn sendung_vermerken(kanal: Kanal, ausgang: &Ausgang) {
    let erfolg = matches!(ausgang.status, Some(200..=299));
    let zeile =
        mit_fenster(|f| f.sendung(Instant::now(), kanal, erfolg, ausgang.bytes, ausgang.dauer));
    if let Some(zeile) = zeile {
        tracing::info!("{zeile}");
    }
}

/// Vermerkt, ob der hinausgehende Spielplan sich geändert hat.
pub fn spielplan_vermerken(geaendert: bool) {
    mit_fenster(|f| f.spielplan(geaendert));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn innerhalb_der_minute_gibt_es_keine_zeile() {
        let t0 = Instant::now();
        let mut f = Fenster::default();
        assert!(f.sendung(t0, Kanal::Tupdate, true, 64, ms(80)).is_none());
        assert!(f
            .sendung(
                t0 + Duration::from_secs(59),
                Kanal::Tupdate,
                true,
                64,
                ms(90)
            )
            .is_none());
    }

    #[test]
    fn nach_der_minute_kommt_die_zeile_des_alten_fensters() {
        let t0 = Instant::now();
        let mut f = Fenster::default();
        f.sendung(t0, Kanal::Tupdate, true, 64, ms(80));
        f.sendung(t0, Kanal::Tupdate, true, 77, ms(300));
        f.sendung(t0, Kanal::Tupdate, false, 70, ms(120));
        f.sendung(t0, Kanal::Sched, true, 458_256, ms(900));
        f.spielplan(false);
        let zeile = f
            .sendung(
                t0 + Duration::from_secs(61),
                Kanal::Tset,
                true,
                15_756,
                ms(200),
            )
            .expect("Fenster ist abgelaufen");
        assert_eq!(
            zeile,
            "badhub-Messung (61 s): \
             tupdate_match n=3 median=120ms max=300ms groesste=77B fehler=1; \
             sched n=1 median=900ms max=900ms groesste=458256B fehler=0; \
             spielplan geaendert=0 gleich=1"
        );
    }

    #[test]
    fn die_ausloesende_sendung_zaehlt_ins_neue_fenster() {
        let t0 = Instant::now();
        let mut f = Fenster::default();
        f.sendung(t0, Kanal::Tupdate, true, 64, ms(80));
        let t1 = t0 + Duration::from_secs(60);
        assert!(f.sendung(t1, Kanal::Tset, true, 100, ms(200)).is_some());
        let zeile = f
            .sendung(
                t1 + Duration::from_secs(60),
                Kanal::Tupdate,
                true,
                64,
                ms(1),
            )
            .expect("zweites Fenster abgelaufen");
        assert!(zeile.contains("tset n=1"), "{zeile}");
        assert!(!zeile.contains("tupdate_match"), "{zeile}");
    }

    #[test]
    fn der_spielplan_zaehler_beginnt_je_fenster_neu() {
        let t0 = Instant::now();
        let mut f = Fenster::default();
        f.sendung(t0, Kanal::Sched, true, 10, ms(5));
        f.spielplan(true);
        let t1 = t0 + Duration::from_secs(60);
        let erste = f.sendung(t1, Kanal::Sched, true, 10, ms(5)).unwrap();
        assert!(erste.ends_with("spielplan geaendert=1 gleich=0"), "{erste}");
        let zweite = f
            .sendung(t1 + Duration::from_secs(60), Kanal::Sched, true, 10, ms(5))
            .unwrap();
        assert!(!zweite.contains("spielplan"), "{zweite}");
    }

    #[test]
    fn der_median_bleibt_gedeckelt_die_anzahl_nicht() {
        let t0 = Instant::now();
        let mut f = Fenster::default();
        for _ in 0..MAX_DAUERN {
            f.sendung(t0, Kanal::Tupdate, true, 64, ms(10));
        }
        f.sendung(t0, Kanal::Tupdate, true, 64, ms(5000));
        let zeile = f
            .sendung(t0 + Duration::from_secs(60), Kanal::Tset, true, 1, ms(1))
            .unwrap();
        assert!(
            zeile.contains(&format!("n={} median=10ms max=5000ms", MAX_DAUERN + 1)),
            "{zeile}"
        );
        assert!(f.kanaele[platz(Kanal::Tupdate)].dauern_ms.len() <= MAX_DAUERN);
    }

    /// Die Zeile geht mit dem Diagnose-Log hinaus: nur Kanalnamen und Zahlen.
    #[test]
    fn die_zeile_traegt_nur_kanalnamen_und_zahlen() {
        let t0 = Instant::now();
        let mut f = Fenster::default();
        for k in KANAELE {
            f.sendung(t0, k, true, 1234, ms(42));
        }
        f.spielplan(true);
        let zeile = f
            .sendung(t0 + Duration::from_secs(60), Kanal::Tset, true, 1, ms(1))
            .unwrap();
        let erlaubt = [
            "badhub-Messung",
            "tupdate_match",
            "tset",
            "sched",
            "centry_list",
            "tverlauf",
            "spielplan",
            "geaendert",
            "gleich",
            "groesste",
            "median",
            "max",
            "fehler",
            "n",
            "ms",
            "s",
            "B",
        ];
        for wort in zeile
            .split(|c: char| !c.is_alphabetic() && c != '_' && c != '-')
            .filter(|w| !w.is_empty())
        {
            assert!(
                erlaubt.contains(&wort),
                "unerwartetes Wort {wort:?} in {zeile}"
            );
        }
    }
}
