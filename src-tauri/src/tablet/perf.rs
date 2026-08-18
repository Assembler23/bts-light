//! Perf-Zähler der Anzeige-Strecke (Spec `monitor-livestand-push`, Etappe S0).
//!
//! Die Spec verbietet, den Netz-Teil ohne Vorher-Zahlen zu beginnen: Erst
//! messen, dann bauen. Gezählt wird deshalb genau das, was die Analyse als
//! Lastposten benannt hat — die Zustands-Abrufe (getrennt nach
//! nudge-getrieben und Fallback-Poll), die Rechnung je Abruf, der
//! Plattenschreibvorgang je Punkt und die verschickten Nudges.
//!
//! **Nur Zahlen.** Diese Werte wandern über den Log-Upload aus echten
//! Turnieren zurück und über `/debug/perf` aus dem LAN heraus; ein
//! Personenbezug hätte hier nichts zu suchen. Der Wächter-Test
//! `debug_perf_enthaelt_keine_personendaten` macht das durchsetzbar.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// Woher ein Zustands-Abruf kam. Die Trennung ist der Kern der Messung: Sie
/// beantwortet, wie viel Last der Push erzeugt und wie viel der Fallback —
/// ohne sie wäre nach S6 nicht zu sagen, welcher Hebel gewirkt hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quelle {
    /// Der Abruf folgte einem Nudge (`&src=push`).
    Push,
    /// Fallback-Takt — oder eine alte Seite, die noch kein `src` sendet.
    Poll,
}

impl Quelle {
    /// Liest die Quelle aus dem `src`-Query. **Alles außer genau `push`
    /// zählt als `poll`**, insbesondere das Fehlen: Eine Seite aus einem
    /// älteren Stand sendet den Parameter nicht, und ihre Abrufe sind
    /// tatsächlich Poll-Abrufe. Lieber die Push-Zahl zu klein als die
    /// Entlastung schöngerechnet.
    pub fn aus_query(src: Option<&str>) -> Self {
        match src {
            Some("push") => Quelle::Push,
            _ => Quelle::Poll,
        }
    }
}

/// Zahl der Histogramm-Fächer für `overview_build_ns`. Fach `i` sammelt
/// Dauern von 2^i bis 2^(i+1) Nanosekunden; 32 Fächer reichen bis gut 4 s
/// und damit weit über jeden realen Bau.
const FAECHER: usize = 32;

/// Die Zähler selbst. Alles `AtomicU64` mit `Relaxed`: Diese Werte sitzen in
/// den heißesten Pfaden der App (jeder Punkt, jeder Abruf jeder Anzeige) und
/// dürfen dort nichts kosten. Sie ordnen keinen anderen Speicherzugriff —
/// eine um eins verzählte Messgröße wäre folgenlos, eine Lock-Kontention im
/// Score-Pfad nicht.
#[derive(Debug, Default)]
pub struct PerfCounters {
    health_push: AtomicU64,
    health_push_bytes: AtomicU64,
    health_poll: AtomicU64,
    health_poll_bytes: AtomicU64,
    court_state_push: AtomicU64,
    court_state_push_bytes: AtomicU64,
    court_state_poll: AtomicU64,
    court_state_poll_bytes: AtomicU64,
    overview_builds: AtomicU64,
    overview_build_ns: AtomicU64,
    overview_build_ns_max: AtomicU64,
    /// Verteilung der Bau-Dauern, damit die Messtabelle ein p95 tragen kann.
    /// Ein reiner Mittelwert verstecke genau die Ausreißer, die auf dem Pi
    /// als Ruckeln ankommen.
    overview_faecher: [AtomicU64; FAECHER],
    persist_calls: AtomicU64,
    persist_ns: AtomicU64,
    persist_bytes: AtomicU64,
    nudges_sent: AtomicU64,
}

impl PerfCounters {
    /// Ein `/health`-Abruf mit seiner Antwortgröße.
    pub fn note_health(&self, quelle: Quelle, bytes: u64) {
        let (n, b) = match quelle {
            Quelle::Push => (&self.health_push, &self.health_push_bytes),
            Quelle::Poll => (&self.health_poll, &self.health_poll_bytes),
        };
        n.fetch_add(1, Ordering::Relaxed);
        b.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Ein `/court/{id}/state`-Abruf mit seiner Antwortgröße.
    pub fn note_court_state(&self, quelle: Quelle, bytes: u64) {
        let (n, b) = match quelle {
            Quelle::Push => (&self.court_state_push, &self.court_state_push_bytes),
            Quelle::Poll => (&self.court_state_poll, &self.court_state_poll_bytes),
        };
        n.fetch_add(1, Ordering::Relaxed);
        b.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Ein **Direktbau** des Übersichts-Zustands. Ab S1 zählt hier nur noch,
    /// was den Antwortcache verfehlt — die Zahl ist dann die Gegenprobe zur
    /// Trefferquote.
    pub fn note_overview_build(&self, ns: u64) {
        self.overview_builds.fetch_add(1, Ordering::Relaxed);
        self.overview_build_ns.fetch_add(ns, Ordering::Relaxed);
        self.overview_build_ns_max.fetch_max(ns, Ordering::Relaxed);
        self.overview_faecher[fach(ns)].fetch_add(1, Ordering::Relaxed);
    }

    /// Ein abgeschlossener Schreibvorgang der `live-scores.json`.
    pub fn note_persist(&self, ns: u64, bytes: u64) {
        self.persist_calls.fetch_add(1, Ordering::Relaxed);
        self.persist_ns.fetch_add(ns, Ordering::Relaxed);
        self.persist_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Ein verschickter Monitor-Nudge.
    pub fn note_nudge(&self) {
        self.nudges_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Aktueller Stand aller Zähler.
    pub fn snapshot(&self) -> PerfSnapshot {
        let lies = |a: &AtomicU64| a.load(Ordering::Relaxed);
        PerfSnapshot {
            health_push: lies(&self.health_push),
            health_push_bytes: lies(&self.health_push_bytes),
            health_poll: lies(&self.health_poll),
            health_poll_bytes: lies(&self.health_poll_bytes),
            court_state_push: lies(&self.court_state_push),
            court_state_push_bytes: lies(&self.court_state_push_bytes),
            court_state_poll: lies(&self.court_state_poll),
            court_state_poll_bytes: lies(&self.court_state_poll_bytes),
            overview_builds: lies(&self.overview_builds),
            overview_build_ns: lies(&self.overview_build_ns),
            overview_build_ns_max: lies(&self.overview_build_ns_max),
            overview_build_ns_p95: self.perzentil_ns(95),
            persist_calls: lies(&self.persist_calls),
            persist_ns: lies(&self.persist_ns),
            persist_bytes: lies(&self.persist_bytes),
            nudges_sent: lies(&self.nudges_sent),
        }
    }

    /// Obere Grenze des Fachs, in dem das `p`-te Perzentil der Bau-Dauern
    /// liegt. Ohne Messwerte 0.
    fn perzentil_ns(&self, p: u64) -> u64 {
        let werte: Vec<u64> = self
            .overview_faecher
            .iter()
            .map(|f| f.load(Ordering::Relaxed))
            .collect();
        let gesamt: u64 = werte.iter().sum();
        if gesamt == 0 {
            return 0;
        }
        // Aufgerundeter Rang: Bei 100 Werten und p=95 ist der 95. der
        // gesuchte — abgerundet träfe man den 94. und meldete zu wenig.
        let ziel = (gesamt * p).div_ceil(100).max(1);
        let mut gesehen = 0u64;
        for (i, n) in werte.iter().enumerate() {
            gesehen += n;
            if gesehen >= ziel {
                return fach_obergrenze(i);
            }
        }
        fach_obergrenze(FAECHER - 1)
    }
}

/// Fach-Index einer Dauer: `floor(log2(ns))`, gedeckelt. `0 ns` landet in
/// Fach 0 — eine Dauer unterhalb der Uhrenauflösung ist für die Verteilung
/// dasselbe wie „unmessbar kurz".
fn fach(ns: u64) -> usize {
    if ns == 0 {
        return 0;
    }
    ((63 - ns.leading_zeros()) as usize).min(FAECHER - 1)
}

/// Obere Grenze eines Fachs in Nanosekunden.
fn fach_obergrenze(i: usize) -> u64 {
    1u64 << (i + 1).min(63)
}

/// Ein Lesestand der Zähler — die Form, in der sie geloggt und über
/// `/debug/perf` ausgeliefert werden. Ausschließlich Zahlen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PerfSnapshot {
    pub health_push: u64,
    pub health_push_bytes: u64,
    pub health_poll: u64,
    pub health_poll_bytes: u64,
    pub court_state_push: u64,
    pub court_state_push_bytes: u64,
    pub court_state_poll: u64,
    pub court_state_poll_bytes: u64,
    pub overview_builds: u64,
    pub overview_build_ns: u64,
    pub overview_build_ns_max: u64,
    /// Obere Grenze des Fachs, in dem das 95. Perzentil liegt (0 ohne
    /// Messwerte). Bewusst die Grenze statt eines interpolierten Werts:
    /// Das Fach ist die Auflösung, die wirklich gemessen wurde.
    pub overview_build_ns_p95: u64,
    pub persist_calls: u64,
    pub persist_ns: u64,
    pub persist_bytes: u64,
    pub nudges_sent: u64,
}

impl PerfSnapshot {
    /// Was seit `vorher` hinzugekommen ist. Die Zähler werden abgezogen,
    /// **`max` und `p95` bleiben absolut** — sie beschreiben die Verteilung
    /// seit Programmstart, und eine „Differenz zweier Perzentile" wäre
    /// keine sinnvolle Größe.
    pub fn seit(&self, vorher: &PerfSnapshot) -> PerfSnapshot {
        // `saturating_sub`: Die Zähler laufen nur vorwärts, aber ein
        // Vergleich über einen Neustart hinweg (Takt hält den alten Stand,
        // die Zähler starten bei 0) darf keine Riesenzahl erfinden.
        let d = |neu: u64, alt: u64| neu.saturating_sub(alt);
        PerfSnapshot {
            health_push: d(self.health_push, vorher.health_push),
            health_push_bytes: d(self.health_push_bytes, vorher.health_push_bytes),
            health_poll: d(self.health_poll, vorher.health_poll),
            health_poll_bytes: d(self.health_poll_bytes, vorher.health_poll_bytes),
            court_state_push: d(self.court_state_push, vorher.court_state_push),
            court_state_push_bytes: d(self.court_state_push_bytes, vorher.court_state_push_bytes),
            court_state_poll: d(self.court_state_poll, vorher.court_state_poll),
            court_state_poll_bytes: d(self.court_state_poll_bytes, vorher.court_state_poll_bytes),
            overview_builds: d(self.overview_builds, vorher.overview_builds),
            overview_build_ns: d(self.overview_build_ns, vorher.overview_build_ns),
            persist_calls: d(self.persist_calls, vorher.persist_calls),
            persist_ns: d(self.persist_ns, vorher.persist_ns),
            persist_bytes: d(self.persist_bytes, vorher.persist_bytes),
            nudges_sent: d(self.nudges_sent, vorher.nudges_sent),
            // Verteilungswerte beschreiben den ganzen Lauf, nicht das Fenster.
            overview_build_ns_max: self.overview_build_ns_max,
            overview_build_ns_p95: self.overview_build_ns_p95,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zaehler_trennt_push_und_poll() {
        // Der Kern der Messung: nudge-getriebene und Fallback-Abrufe landen
        // in getrennten Zählern, samt ihrer Bytes.
        let p = PerfCounters::default();
        p.note_health(Quelle::Push, 1_000);
        p.note_health(Quelle::Push, 1_500);
        p.note_health(Quelle::Poll, 2_000);
        p.note_court_state(Quelle::Push, 300);
        p.note_court_state(Quelle::Poll, 400);
        let s = p.snapshot();
        assert_eq!(s.health_push, 2);
        assert_eq!(s.health_push_bytes, 2_500);
        assert_eq!(s.health_poll, 1);
        assert_eq!(s.health_poll_bytes, 2_000);
        assert_eq!(s.court_state_push, 1);
        assert_eq!(s.court_state_push_bytes, 300);
        assert_eq!(s.court_state_poll, 1);
        assert_eq!(s.court_state_poll_bytes, 400);
    }

    #[test]
    fn zaehler_ohne_src_zaehlt_als_poll() {
        // Akzeptanzkriterium: Ein Abruf ohne `src` (alte Seite) wird als
        // `poll` gezählt, nie als `push`. Ebenso alles Unbekannte.
        assert_eq!(Quelle::aus_query(None), Quelle::Poll);
        assert_eq!(Quelle::aus_query(Some("")), Quelle::Poll);
        assert_eq!(Quelle::aus_query(Some("irgendwas")), Quelle::Poll);
        assert_eq!(Quelle::aus_query(Some("poll")), Quelle::Poll);
        assert_eq!(Quelle::aus_query(Some("push")), Quelle::Push);
    }

    #[test]
    fn overview_build_ns_steigt_je_direktbau() {
        let p = PerfCounters::default();
        assert_eq!(p.snapshot().overview_builds, 0);
        p.note_overview_build(1_000);
        p.note_overview_build(3_000);
        let s = p.snapshot();
        assert_eq!(s.overview_builds, 2);
        assert_eq!(s.overview_build_ns, 4_000);
        assert_eq!(s.overview_build_ns_max, 3_000);
    }

    #[test]
    fn p95_kommt_aus_dem_histogramm() {
        // Die Messtabelle der Spec verlangt ein p95. 100 Bauten: 95 schnelle
        // (~1 µs) und 5 langsame (~8 ms) — das Perzentil muss im Fach der
        // schnellen liegen, nicht beim Mittelwert der beiden Wolken.
        let p = PerfCounters::default();
        for _ in 0..95 {
            p.note_overview_build(1_024);
        }
        for _ in 0..5 {
            p.note_overview_build(8_000_000);
        }
        let s = p.snapshot();
        assert!(
            s.overview_build_ns_p95 >= 1_024 && s.overview_build_ns_p95 <= 4_096,
            "p95 sollte im Fach der 95 schnellen Bauten liegen, war {}",
            s.overview_build_ns_p95
        );
        assert_eq!(s.overview_build_ns_max, 8_000_000);
    }

    #[test]
    fn persist_und_nudges_werden_gezaehlt() {
        let p = PerfCounters::default();
        p.note_persist(500_000, 2_048);
        p.note_persist(400_000, 2_050);
        p.note_nudge();
        p.note_nudge();
        p.note_nudge();
        let s = p.snapshot();
        assert_eq!(s.persist_calls, 2);
        assert_eq!(s.persist_ns, 900_000);
        assert_eq!(s.persist_bytes, 4_098);
        assert_eq!(s.nudges_sent, 3);
    }

    #[test]
    fn seit_zieht_die_vorherigen_zaehler_ab() {
        // Die 10-s-Logzeile meldet, was im Fenster passiert ist — sonst
        // stünde in jeder Zeile die Summe seit Programmstart und die Rate
        // wäre nicht ablesbar.
        let p = PerfCounters::default();
        p.note_health(Quelle::Poll, 100);
        p.note_overview_build(2_000);
        let erst = p.snapshot();
        p.note_health(Quelle::Poll, 900);
        p.note_health(Quelle::Push, 50);
        p.note_overview_build(6_000);
        let delta = p.snapshot().seit(&erst);
        assert_eq!(delta.health_poll, 1);
        assert_eq!(delta.health_poll_bytes, 900);
        assert_eq!(delta.health_push, 1);
        assert_eq!(delta.overview_builds, 1);
        assert_eq!(delta.overview_build_ns, 6_000);
        // Verteilungswerte bleiben absolut — sie beschreiben den ganzen Lauf.
        assert_eq!(delta.overview_build_ns_max, 6_000);
    }

    #[test]
    fn debug_perf_enthaelt_keine_personendaten() {
        // Wächter (Muster `the_state_never_carries_personal_data_beyond_its_purpose`
        // in `tl.rs`): Der Perf-Bericht verlässt das Gerät — über den
        // Log-Upload und über `/debug/perf`. Er darf ausschließlich Zahlen
        // tragen. Der Test prüft die STRUKTUR, nicht einzelne Feldnamen:
        // Sobald jemand ein Feld nachrüstet, das einen Namen, eine Match-ID
        // als Text oder eine Liste transportiert, schlägt er fehl — auch
        // wenn das Feld hier niemandem einfiel.
        let p = PerfCounters::default();
        p.note_health(Quelle::Push, 1_234);
        p.note_court_state(Quelle::Poll, 99);
        p.note_overview_build(4_711);
        p.note_persist(1_000, 64);
        p.note_nudge();
        let json = serde_json::to_value(p.snapshot()).expect("Snapshot ist serialisierbar");
        let obj = json.as_object().expect("Snapshot ist ein Objekt");
        assert!(!obj.is_empty(), "leerer Bericht wäre kein Beleg");
        for (feld, wert) in obj {
            assert!(
                wert.is_u64() || wert.is_i64(),
                "Feld `{feld}` ist keine Zahl, sondern {wert} — der Perf-Bericht \
                 trägt ausschließlich Zahlen (Spec monitor-livestand-push, S0)"
            );
        }
    }
}
