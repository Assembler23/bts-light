//! Startzeit-Prognose (Spec `docs/features/spielzeiten-prognose.md`,
//! Etappe B): Median-Statistik der gemessenen Bruttozeiten und eine
//! deterministische Vollmodell-Simulation der Warteliste.
//!
//! Alles hier ist **rein** — keine Locks, keine Uhr, kein IO. Die Zutaten
//! (Felder, Warteliste, Messwerte, Blocker) stellt `tl::build_state_limited`
//! aus seinem Snapshot zusammen; gerechnet wird in **ganzen Minuten**, damit
//! der TL-State-Fingerprint nicht jede Sekunde kippt (Rev-Churn-Wächter).
//!
//! R2 bleibt gewahrt: Die Simulation erfindet keine Court→Match-Zuordnung —
//! sie spielt die vorhandene Vergabelogik (Reihenfolge der Warteliste,
//! Hallenregeln, Spieler-Mindestpause) nur zur **Anzeige** durch.

use std::collections::HashMap;

use crate::tablet::match_times::MatchTimeEntry;

/// Fester Übergangspuffer je Feldwechsel (E8): Feld räumen, Aufruf, Weg in
/// die Halle. Wird mit `auto_assign.wait_minutes` als Untergrenze
/// verrechnet (`effective_buffer_min`).
pub const TRANSITION_BUFFER_MINS: u64 = 2;

/// Ab so vielen Messwerten nutzt eine Stufe der Fallback-Kette ihre
/// eigenen Werte (E6).
pub const MIN_SAMPLES: usize = 3;

/// Ein Messwert: regulär beendetes Spiel mit allen drei Stempeln (E11).
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub class_label: String,
    pub discipline: String,
    /// Halle der ersten Feldzuweisung; leer bei Ein-Hallen-Turnieren und
    /// bei Messwerten aus der Zeit vor ADR 0036.
    pub hall: String,
    pub brutto_min: u64,
    pub netto_min: u64,
}

/// Median-Statistik über die Messwerte eines Turniers.
///
/// Alle Mediane werden **einmal beim Bau** vorberechnet (Review
/// 2026-08-16, bestätigt): `group_duration` läuft je Feld und je
/// Wartelisten-Spiel bei jedem TL-State-Bau — ohne Vorberechnung wären
/// das lineare Scans samt Sortierung über alle Messwerte, zigfach pro
/// 2-Sekunden-Poll. Der Bau selbst passiert nur je Messwert-Generation
/// (`TabletState::cached_time_stats`).
#[derive(Debug, Clone, Default)]
pub struct TimeStats {
    /// (Klasse, Disziplin) → (Anzahl, Brutto-Median, Netto-Median).
    group_medians: HashMap<(String, String), (usize, u64, u64)>,
    /// Klasse → (Anzahl, Brutto-Median, Netto-Median).
    class_medians: HashMap<String, (usize, u64, u64)>,
    /// Turnierweite Mediane (Brutto, Netto), sobald [`MIN_SAMPLES`]
    /// erreicht sind.
    tournament_medians: Option<(u64, u64)>,
    /// Vorberechnete Auswertungszeilen (je Klasse × Disziplin).
    rows: Vec<StatsRow>,
    /// Dieselben Messwerte, nur anders geschnitten (Spec
    /// `tl-sicht-feinschliff`, Punkt 1). Ebenfalls **beim Bau**
    /// vorberechnet, also einmal je Messwert-Generation und nicht je
    /// Poll — dieselbe Begründung wie oben.
    ///
    /// Sie speisen **ausschließlich die Anzeige**. Die Fallback-Kette der
    /// Prognose (`group_medians` → `class_medians` → `tournament_medians`)
    /// bleibt davon unberührt; das ist Nicht-Ziel N-3 der Spec und durch
    /// `die_hallen_achse_aendert_die_prognose_kette_nicht` festgehalten.
    rows_class: Vec<StatsRow>,
    rows_discipline: Vec<StatsRow>,
    rows_hall: Vec<StatsRow>,
}

/// Eine Zeile der Auswertung, Mediane in Minuten.
///
/// Dieselbe Form für alle vier Achsen: Je Achse tragen nur die Felder einen
/// Wert, nach denen dort gruppiert wird — die übrigen bleiben leer. Ein
/// fertig zusammengesetztes Label wäre kompakter, gäbe dem Host aber
/// Anzeige-Fachlichkeit, die er sonst konsequent der Seite lässt.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsRow {
    pub class_label: String,
    pub discipline: String,
    /// Nur auf der Hallen-Achse gefüllt; leer heißt dort „ohne Halle".
    pub hall: String,
    pub count: usize,
    pub brutto_min: u64,
    pub netto_min: u64,
    pub diff_min: u64,
}

/// Ergebnis der Simulation für ein wartendes Spiel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prediction {
    /// Voraussichtlicher Aufruf, Minuten seit Unix-Epoche (× 60 000 = ms).
    pub start_min: u64,
    /// Steht hinter der Dauer nur der Config-Default (keine Messwerte)?
    pub uncertain: bool,
}

/// Ein Feld in der Simulation. Gesperrte Felder gibt der Aufrufer gar
/// nicht erst herein.
#[derive(Debug, Clone)]
pub struct PredictCourt {
    pub hall: String,
    /// Frühestens frei (Minuten seit Epoche): freie Felder `now`, belegte
    /// `now + max(0, Gruppenwert − verstrichene Bruttozeit)`.
    pub free_at_min: u64,
}

/// Ein wartendes Spiel in Wartelisten-Reihenfolge.
#[derive(Debug, Clone)]
pub struct PredictMatch {
    pub match_id: i64,
    /// Zugeordnete Halle (leer = jede Halle erlaubt).
    pub hall: String,
    /// Erwartete Bruttodauer (Gruppenwert bzw. Default), Minuten.
    pub duration_min: u64,
    pub uncertain: bool,
    /// Spieler-Schlüssel (`assign::player_key`) beider Teams.
    pub players: Vec<String>,
}

/// Eingaben der Simulation — vom Aufrufer aus EINEM Snapshot gebaut.
#[derive(Debug, Clone, Default)]
pub struct PredictInput {
    pub now_min: u64,
    /// Effektiver Übergangspuffer (Minuten) je Feldvergabe.
    pub buffer_min: u64,
    /// Mindestpause eines Spielers zwischen zwei Spielen (Minuten).
    pub rest_min: u64,
    pub courts: Vec<PredictCourt>,
    /// Spieler-Schlüssel → frühestens wieder einsatzbereit (Minuten seit
    /// Epoche). Aus laufenden Spielen und bestehenden Blockern.
    pub player_ready_min: HashMap<String, u64>,
    pub queue: Vec<PredictMatch>,
}

/// Effektiver Übergangspuffer (E8-Klärung 3): Die Auto-Vergabe belegt ein
/// Feld frühestens nach `wait_minutes` — ein davon unabhängiger fester
/// Puffer wäre bei größerem `wait_minutes` systematisch zu optimistisch.
pub fn effective_buffer_min(auto_assign_enabled: bool, wait_minutes: f64) -> u64 {
    if auto_assign_enabled && wait_minutes.is_finite() && wait_minutes > 0.0 {
        TRANSITION_BUFFER_MINS.max(wait_minutes.floor() as u64)
    } else {
        TRANSITION_BUFFER_MINS
    }
}

/// Median einer (unsortierten) Minutenliste: bei gerader Anzahl das
/// Mittel der beiden mittleren Werte. `None` bei leerer Liste.
fn median_min(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2
    })
}

/// Messwerte aus dem Zeiten-Store ziehen (E11: nur regulär beendete
/// Spiele mit allen drei Stempeln) und zur Statistik bündeln.
pub fn time_stats(entries: &HashMap<i64, MatchTimeEntry>) -> TimeStats {
    let mut measurements: Vec<Measurement> = entries
        .values()
        .filter(|e| e.regular)
        .filter_map(|e| {
            let assigned = e.first_assigned_ms?;
            let first_point = e.first_point_ms?;
            let finished = e.finished_ms?;
            // Plausibilitätsregel wie überall (BTP-Duration, Beendet-Liste):
            // über Nacht geparkte Spiele vergiften den Median nicht. Netto
            // auf Brutto geklemmt — ein Score, der den Host vor dem ersten
            // Sync-Poll erreicht, stempelt sonst „netto > brutto".
            let brutto_min =
                crate::tablet::match_times::plausible_duration_mins(assigned, finished)? as u64;
            let netto_min = (finished.saturating_sub(first_point) / 60_000).min(brutto_min);
            Some(Measurement {
                class_label: e.class_label.trim().to_string(),
                discipline: e.discipline.trim().to_string(),
                // Getrimmt wie Klasse und Disziplin: Der Schlüssel ist der
                // freie BTP-Hallenname, und ein Leerzeichen am Rand spaltete
                // sonst eine Halle in zwei Zeilen (ADR 0036).
                hall: e.hall.trim().to_string(),
                brutto_min,
                netto_min,
            })
        })
        .collect();
    // Deterministische Reihenfolge, unabhängig von der HashMap-Iteration.
    measurements.sort_by(|a, b| {
        (&a.class_label, &a.discipline, a.brutto_min, a.netto_min).cmp(&(
            &b.class_label,
            &b.discipline,
            b.brutto_min,
            b.netto_min,
        ))
    });

    // Mediane einmal vorberechnen (siehe Struct-Kommentar). Brutto und
    // Netto laufen als Paar mit, damit die Live-Restzeitschätzung
    // (Etappe D) dieselbe Fallback-Stufe für beide Werte nutzt.
    let mut group_werte: HashMap<(String, String), Vec<(u64, u64)>> = HashMap::new();
    let mut class_werte: HashMap<String, Vec<(u64, u64)>> = HashMap::new();
    let mut alle: Vec<(u64, u64)> = Vec::new();
    for m in &measurements {
        group_werte
            .entry((m.class_label.clone(), m.discipline.clone()))
            .or_default()
            .push((m.brutto_min, m.netto_min));
        class_werte
            .entry(m.class_label.clone())
            .or_default()
            .push((m.brutto_min, m.netto_min));
        alle.push((m.brutto_min, m.netto_min));
    }
    fn brutto_netto_medians(v: &[(u64, u64)]) -> Option<(u64, u64)> {
        let brutto: Vec<u64> = v.iter().map(|x| x.0).collect();
        let netto: Vec<u64> = v.iter().map(|x| x.1).collect();
        Some((median_min(&brutto)?, median_min(&netto)?))
    }
    let group_medians: HashMap<(String, String), (usize, u64, u64)> = group_werte
        .into_iter()
        .filter_map(|(k, v)| brutto_netto_medians(&v).map(|(b, n)| (k, (v.len(), b, n))))
        .collect();
    let class_medians = class_werte
        .into_iter()
        .filter_map(|(k, v)| brutto_netto_medians(&v).map(|(b, n)| (k, (v.len(), b, n))))
        .collect();
    let tournament_medians = if alle.len() >= MIN_SAMPLES {
        brutto_netto_medians(&alle)
    } else {
        None
    };

    let mut groups: Vec<(String, String)> = measurements
        .iter()
        .map(|m| (m.class_label.clone(), m.discipline.clone()))
        .collect();
    groups.dedup();
    let rows = groups
        .into_iter()
        .map(|(class, disc)| {
            // Brutto/Netto aus den EINMAL vorberechneten Gruppen-Medianen
            // (Review 2026-08-17): Panel und Prognose müssen dieselben
            // Zahlen zeigen — eine zweite Rechnung könnte auseinanderlaufen.
            // Nur der Differenz-Median bleibt lokal: Median der
            // Einzel-Differenzen ≠ Differenz der Mediane (bewusst so).
            let (count, brutto_min, netto_min) = group_medians
                .get(&(class.clone(), disc.clone()))
                .copied()
                .unwrap_or((0, 0, 0));
            let diff: Vec<u64> = measurements
                .iter()
                .filter(|m| m.class_label == class && m.discipline == disc)
                .map(|m| m.brutto_min.saturating_sub(m.netto_min))
                .collect();
            StatsRow {
                class_label: class,
                discipline: disc,
                hall: String::new(),
                count,
                brutto_min,
                netto_min,
                diff_min: median_min(&diff).unwrap_or(0),
            }
        })
        .collect();

    // Die drei zusätzlichen Achsen aus DENSELBEN Messwerten (Punkt 1 der
    // Spec). Bewusst hier und nicht in der Fallback-Kette: Sie sind reine
    // Anzeige (N-3).
    let rows_class = zeilen_nach(&measurements, |m| {
        (m.class_label.clone(), String::new(), String::new())
    });
    let rows_discipline = zeilen_nach(&measurements, |m| {
        (String::new(), m.discipline.clone(), String::new())
    });
    let rows_hall = zeilen_nach(&measurements, |m| {
        (String::new(), String::new(), m.hall.clone())
    });

    TimeStats {
        group_medians,
        class_medians,
        tournament_medians,
        rows,
        rows_class,
        rows_discipline,
        rows_hall,
    }
}

/// Auswertungszeilen für eine Achse: gruppiert die Messwerte nach dem
/// Schlüssel `(Klasse, Disziplin, Halle)`, den `key` liefert, und rechnet je
/// Gruppe dieselben vier Zahlen wie die Klasse×Disziplin-Achse.
///
/// Der Differenz-Median bleibt bewusst der Median der **Einzel**-Differenzen
/// — er ist NICHT die Differenz der beiden Mediane (dasselbe wie oben bei
/// `rows`, und aus demselben Grund).
///
/// Sortiert wird über den Schlüssel selbst, damit die Reihenfolge
/// deterministisch ist; leere Schlüssel („ohne Halle") landen dabei oben.
fn zeilen_nach(
    measurements: &[Measurement],
    key: impl Fn(&Measurement) -> (String, String, String),
) -> Vec<StatsRow> {
    let mut gruppen: HashMap<(String, String, String), Vec<(u64, u64)>> = HashMap::new();
    for m in measurements {
        gruppen
            .entry(key(m))
            .or_default()
            .push((m.brutto_min, m.netto_min));
    }
    let mut schluessel: Vec<(String, String, String)> = gruppen.keys().cloned().collect();
    schluessel.sort();
    schluessel
        .into_iter()
        .map(|k| {
            let werte = &gruppen[&k];
            let brutto: Vec<u64> = werte.iter().map(|x| x.0).collect();
            let netto: Vec<u64> = werte.iter().map(|x| x.1).collect();
            let diff: Vec<u64> = werte.iter().map(|(b, n)| b.saturating_sub(*n)).collect();
            StatsRow {
                class_label: k.0,
                discipline: k.1,
                hall: k.2,
                count: werte.len(),
                brutto_min: median_min(&brutto).unwrap_or(0),
                netto_min: median_min(&netto).unwrap_or(0),
                diff_min: median_min(&diff).unwrap_or(0),
            }
        })
        .collect()
}

impl TimeStats {
    /// Auswertungszeilen je Klasse × Disziplin (Median Brutto/Netto/
    /// Differenz, Anzahl), sortiert nach Klasse, dann Disziplin —
    /// vorberechnet beim Bau.
    ///
    /// Als Slice statt Klon: Der TL-Zustand wird alle ein bis zwei Sekunden
    /// gebaut, und mit vier Achsen wären das vier Klone je Bau (Review
    /// 18.08.2026). Der Aufrufer mappt direkt in die Wire-Zeilen.
    pub fn rows(&self) -> &[StatsRow] {
        &self.rows
    }

    /// Auswertung nach **Klasse** (über alle Disziplinen und Hallen).
    pub fn rows_class(&self) -> &[StatsRow] {
        &self.rows_class
    }

    /// Auswertung nach **Disziplin** (über alle Klassen und Hallen).
    pub fn rows_discipline(&self) -> &[StatsRow] {
        &self.rows_discipline
    }

    /// Auswertung nach **Halle**. Messwerte ohne Halle stehen in einer
    /// eigenen Zeile mit leerem Namen — sie dürfen keine echte Halle
    /// verfälschen (A1.8).
    pub fn rows_hall(&self) -> &[StatsRow] {
        &self.rows_hall
    }

    /// Turnierweiter Brutto-Median (ab [`MIN_SAMPLES`] Messwerten).
    pub fn tournament_brutto_min(&self) -> Option<u64> {
        self.tournament_medians.map(|(b, _)| b)
    }

    /// Erwartete Bruttodauer eines Spiels mit Fallback-Kette (E5–E7):
    /// Klasse×Disziplin (≥3) → Klasse (≥3) → Turnier (≥3) → Default.
    /// Leeres `class_label` springt direkt auf die Turnierstufe.
    /// `true` = nur der Default steht dahinter (Anzeige „~hh:mm").
    pub fn group_duration(
        &self,
        class_label: &str,
        discipline: &str,
        default_mins: f64,
    ) -> (u64, bool) {
        let g = self.group_times(class_label, discipline, default_mins);
        (g.brutto_min, g.uncertain)
    }

    /// Wie [`Self::group_duration`], aber mit Netto- und Differenz-Median
    /// **derselben** Fallback-Stufe (Etappe D) — die Live-Restzeit darf
    /// Brutto und Netto nicht aus verschiedenen Stufen mischen. Reine
    /// Map-Zugriffe auf die vorberechneten Mediane.
    pub fn group_times(
        &self,
        class_label: &str,
        discipline: &str,
        default_mins: f64,
    ) -> GroupTimes {
        let class = class_label.trim();
        let disc = discipline.trim();
        let stufe = |n: usize, brutto: u64, netto: u64| -> Option<GroupTimes> {
            (n >= MIN_SAMPLES).then_some(GroupTimes::new(brutto, netto, false))
        };
        if !class.is_empty() {
            if let Some(&(n, b, nt)) = self
                .group_medians
                .get(&(class.to_string(), disc.to_string()))
            {
                if let Some(g) = stufe(n, b, nt) {
                    return g;
                }
            }
            if let Some(&(n, b, nt)) = self.class_medians.get(class) {
                if let Some(g) = stufe(n, b, nt) {
                    return g;
                }
            }
        }
        if let Some((b, nt)) = self.tournament_medians {
            return GroupTimes::new(b, nt, false);
        }
        let default = if default_mins.is_finite() && default_mins > 0.0 {
            default_mins.round() as u64
        } else {
            25
        };
        GroupTimes::new(default, default, true)
    }
}

/// Gruppen-Zeitwerte für die Live-Restzeitschätzung (Etappe D): dieselbe
/// Fallback-Kette wie [`TimeStats::group_duration`], aber mit dem
/// Netto-Median dazu. Ohne belastbare Messwerte gilt der Default als reine
/// Bruttodauer (Netto = Brutto) — das 0:0-Modell hält das Feld dann
/// schlicht die volle Dauer. Die Anlaufzeit ist stets Brutto − Netto und
/// wird am Verwendungsort gerechnet, nicht als drittes Feld mitgeschleppt
/// (Review 2026-08-17: ein gespeichertes Derivat kann nur desynchronisieren).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupTimes {
    pub brutto_min: u64,
    pub netto_min: u64,
    pub uncertain: bool,
}

impl GroupTimes {
    /// Der EINE Konstruktor aller Fallback-Stufen: klemmt Netto auf Brutto
    /// (ein Score, der den Host vor dem ersten Sync-Poll erreicht, stempelte
    /// sonst „netto > brutto").
    fn new(brutto_min: u64, netto_min: u64, uncertain: bool) -> Self {
        Self {
            brutto_min,
            netto_min: netto_min.min(brutto_min),
            uncertain,
        }
    }
}

/// Eingaben der Live-Restzeitschätzung eines belegten Felds (Etappe D).
/// Der Aufrufer (tl.rs) ruft sie nur, wenn das Feld wirklich live zählt
/// (Tablet/Zähltafel verbunden oder Erster-Punkt-Stempel vorhanden).
#[derive(Debug, Clone)]
pub struct LiveRemainInput {
    pub now_ms: u64,
    /// Sätze in Spielreihenfolge, der letzte ist der laufende.
    pub sets: Vec<(i64, i64)>,
    /// Zählformat des Matches; 0 = unbekannt → Badminton-Normalfall 3/21.
    pub best_of: i64,
    pub target: i64,
    pub cap: i64,
    pub first_assigned_ms: Option<u64>,
    pub first_point_ms: Option<u64>,
    /// Gruppenwerte aus [`TimeStats::group_times`]; die Anlaufzeit ist
    /// Brutto − Netto und wird hier gerechnet statt mitgereicht.
    pub netto_median_min: u64,
    pub brutto_median_min: u64,
}

/// Prior-Anteil eines dritten Satzes an der erwarteten Gesamtpunktzahl
/// eines Matches — nur noch fürs **Tempo-Prior** (Sekunden je Punkt aus
/// dem Netto-Median). Der tatsächliche Entscheidungssatz-Anteil kommt
/// live aus [`p_race`]/[`expected_sets`].
const PRIOR_EXTRA_SET_SHARE: f64 = 0.35;
/// Glättung der Punktstärke Richtung 50 % (virtuelle Punkte): dämpft die
/// ersten Ballwechsel, lässt aber einen 15:5-Erstsatz als Favoriten-Signal
/// deutlich durchschlagen (Nutzer-Beispiele 17.08.2026).
const STRENGTH_SMOOTHING_POINTS: f64 = 20.0;
/// Logistische Näherung der Standardnormal-Verteilungsfunktion
/// (Amemiya-Konstante) — Rust-std hat kein `erf`.
const LOGISTIC_PHI: f64 = 1.702;
/// Bayes-Glättung des Eigentempos: so viele „virtuelle" Punkte zum
/// Prior-Tempo, damit die ersten Ballwechsel das Modell nicht verzerren.
const PACE_SMOOTHING_POINTS: f64 = 10.0;
/// Erwartete Gesamtpunkte eines Satzes relativ zum Zielpunkt (21 → ~35).
const PTS_PER_SET_FACTOR: f64 = 1.65;
/// Pauschale Satzpause je erwartetem weiteren Satz (Minuten).
const SET_BREAK_MINS: f64 = 2.0;

/// Satz fertig? Zielpunkt erreicht und 2 Vorsprung — oder am Deckel.
/// Bewusst lokale Kopie der Server-Regel (`server::set_is_complete`):
/// dieses Modul bleibt rein und ohne Server-Abhängigkeit.
fn set_complete(a: i64, b: i64, target: i64, cap: i64) -> bool {
    let (leader, trailer) = (a.max(b), a.min(b));
    // Ein Satz ohne Führenden ist nie fertig — 30:30 kann nur ein kaputter
    // oder manipulierter Score-Frame liefern (Review 2026-08-17) und darf
    // niemandem gutgeschrieben werden.
    leader > trailer && leader >= target && (leader - trailer >= 2 || (cap > 0 && leader >= cap))
}

/// Logistische Näherung der Standardnormal-Verteilungsfunktion Φ(z).
fn phi(z: f64) -> f64 {
    1.0 / (1.0 + (-LOGISTIC_PHI * z).exp())
}

/// Gewinnwahrscheinlichkeit eines Punkte-Wettrennens: A braucht noch `na`
/// Punkte, B noch `nb`, A gewinnt jeden Ballwechsel mit `p`.
/// Normal-Näherung über die erwartete Punktdifferenz am Horizont
/// `na + nb` — genau das gewünschte Verhalten: 10:6 nach einem
/// 15:5-Erstsatz ist praktisch durch, 7:11 kippt klar, 13:13 bei
/// ausgeglichener Stärke ist ein Münzwurf.
fn p_race(na: f64, nb: f64, p: f64) -> f64 {
    let den = ((na + nb) * p * (1.0 - p)).sqrt();
    if den <= f64::EPSILON {
        return if p >= 0.5 { 1.0 } else { 0.0 };
    }
    phi((nb * p - na * (1.0 - p)) / den)
}

/// Erwartete Zahl **weiterer** Sätze aus Satzstand `a:b` (Sieger braucht
/// `need`), wenn A jeden Satz mit Wahrscheinlichkeit `p` gewinnt.
/// Rekursionstiefe ≤ 2×need−1 — für Best-of-3 drei Zustände.
fn expected_sets(a: i64, b: i64, need: i64, p: f64) -> f64 {
    if a >= need || b >= need {
        0.0
    } else {
        1.0 + p * expected_sets(a + 1, b, need, p) + (1.0 - p) * expected_sets(a, b + 1, need, p)
    }
}

/// Geschätzte Restzeit (Minuten, ≥ 1) eines laufenden Spiels aus dem
/// Live-Stand (Etappe D). Idee: erwartete Restpunkte aus Satzstand und
/// Zählsystem × gemessenes Eigentempo dieses Spiels. Ohne ersten Punkt
/// hält das Feld die volle Nettodauer plus Rest-Anlauf — ein zäh
/// startendes 0:0-Spiel wird nicht mehr „gleich frei" gerechnet.
pub fn live_remaining_min(input: &LiveRemainInput) -> u64 {
    let target = if input.target > 0 { input.target } else { 21 };
    // `NumSets` kommt roh aus BTP (nur > 0 geprüft) und [`expected_sets`]
    // ist eine Doppel-Rekursion mit ~C(2n, n) Aufrufen — ohne Klemme fröre
    // ein kaputter Wert den ~2-s-TL-State-Bau ein (Review 2026-08-17).
    // Mehr als Best-of-5 spielt kein Badminton-Format.
    let best_of = if input.best_of > 0 {
        input.best_of.min(5)
    } else {
        3
    };
    let need = best_of / 2 + 1;
    let clamp_hi = 2 * input.brutto_median_min.max(1);
    let points: i64 = input.sets.iter().map(|&(a, b)| a.max(0) + b.max(0)).sum();

    // Noch kein Punkt: volle Nettodauer plus Rest-Anlauf (Brutto − Netto,
    // minus bereits verstrichene Zeit seit der Feldzuweisung).
    let first_point = match input.first_point_ms {
        Some(fp) if points > 0 => fp,
        _ => {
            let elapsed_min = input
                .first_assigned_ms
                .map(|a| input.now_ms.saturating_sub(a) / 60_000)
                .unwrap_or(0);
            let anlauf = input
                .brutto_median_min
                .saturating_sub(input.netto_median_min);
            let rest_anlauf = anlauf.saturating_sub(elapsed_min);
            return (input.netto_median_min + rest_anlauf).clamp(1, clamp_hi);
        }
    };
    // Kein stiller Fallback (Review 2026-08-17): points > 0 garantiert
    // mindestens einen Satz — bräche ein Umbau diese Invariante, soll das
    // in den Tests knallen statt eine plausibel falsche Zahl zu liefern.
    let &(last_a, last_b) = input
        .sets
        .last()
        .expect("points > 0 garantiert mindestens einen Satz");

    // Abgeschlossene Sätze zählen; der letzte Eintrag ist der laufende.
    let mut sets_a = 0i64;
    let mut sets_b = 0i64;
    let mut completed_pts: Vec<i64> = Vec::new();
    for &(a, b) in &input.sets {
        if set_complete(a, b, target, input.cap) {
            if a > b {
                sets_a += 1;
            } else {
                sets_b += 1;
            }
            completed_pts.push(a + b);
        }
    }
    let last_complete = set_complete(last_a, last_b, target, input.cap);
    if last_complete && sets_a.max(sets_b) >= need {
        return 1; // entschieden — nur noch Ergebnis eintragen
    }

    // Eigentempo (Sekunden je Punkt): Nettozeit / gespielte Punkte, mit
    // Prior geglättet und auf [0,5; 2] × Prior geklemmt — friert der
    // Stand ein (Tablet tot), läuft die Schätzung sonst davon.
    let target_f = target as f64;
    let pts_per_set_prior = target_f * PTS_PER_SET_FACTOR;
    let expected_match_pts = (need as f64 + PRIOR_EXTRA_SET_SHARE) * pts_per_set_prior;
    let prior_pace = input.netto_median_min.max(1) as f64 * 60.0 / expected_match_pts;
    // Nettozeit minutengranular (Rev-Churn-Wächter): Der TL-State entsteht
    // alle ~2 s — sekundengenau schöbe jede Uhr-Sekunde das Tempo minimal
    // und kippte die Aufrundung mitten in der Minute. Gleiche Minute und
    // gleicher Stand ⇒ exakt gleiches Ergebnis.
    let netto_elapsed_sec =
        (input.now_ms / 60_000).saturating_sub(first_point / 60_000) as f64 * 60.0;
    let pace = ((netto_elapsed_sec + prior_pace * PACE_SMOOTHING_POINTS)
        / (points as f64 + PACE_SMOOTHING_POINTS))
        .clamp(prior_pace * 0.5, prior_pace * 2.0);

    // Punkte je Satz: fertige Sätze DIESES Matches, sonst Format-Prior.
    let pts_per_set = if completed_pts.is_empty() {
        pts_per_set_prior
    } else {
        completed_pts.iter().sum::<i64>() as f64 / completed_pts.len() as f64
    };

    // Restpunkte des laufenden Satzes: proportional zum Weg des Führenden
    // zum Zielpunkt; solange der Satz läuft, mindestens 2 (Verlängerung).
    let rest_current = if last_complete {
        0.0
    } else {
        let leader = last_a.max(last_b) as f64;
        (pts_per_set * (1.0 - leader / target_f)).max(2.0)
    };

    // Künftige Sätze als **Erwartungswert** (Nutzer-Wunsch 17.08.2026):
    // Wie wahrscheinlich der laufende Satz an wen geht, sagt das
    // Punkte-Wettrennen aus Satzstand und Punktstärke — ein 15:5-Erstsatz
    // macht den Favoriten auch im zweiten Satz zum klaren Favoriten, ein
    // 7:11-Rückstand kippt den Satz trotzdem. Danach zählt jeder mögliche
    // weitere Satz mit seiner Wahrscheinlichkeit statt mit fester Quote.
    let pts_a_total: i64 = input.sets.iter().map(|&(a, _)| a.max(0)).sum();
    let p_pt = (pts_a_total as f64 + STRENGTH_SMOOTHING_POINTS / 2.0)
        / (points as f64 + STRENGTH_SMOOTHING_POINTS);
    let p_fresh = p_race(target_f, target_f, p_pt);
    let expected_future = if last_complete {
        expected_sets(sets_a, sets_b, need, p_fresh)
    } else {
        let na = (target - last_a).max(1) as f64;
        let nb = (target - last_b).max(1) as f64;
        let p_cur = p_race(na, nb, p_pt);
        p_cur * expected_sets(sets_a + 1, sets_b, need, p_fresh)
            + (1.0 - p_cur) * expected_sets(sets_a, sets_b + 1, need, p_fresh)
    };

    let total_min = (rest_current + expected_future * pts_per_set) * pace / 60.0
        + SET_BREAK_MINS * expected_future;
    (total_min.ceil() as u64).clamp(1, clamp_hi)
}

/// Vollmodell-Simulation (E8): spielt die Warteliste in Reihenfolge auf
/// die Felder durch. Je Spiel das früheste erlaubte Feld;
/// `start = max(feld_frei + puffer, spieler_bereit)`. Ausgenommene Spiele
/// stehen gar nicht erst in der `queue` (sie belegen kein Feld und
/// bekommen keine Prognose). Spiele ohne erlaubtes Feld bekommen keinen
/// Eintrag.
pub fn predict_starts(input: &PredictInput) -> HashMap<i64, Prediction> {
    // Ereignis-Schleife statt starrem Reihenfolge-Durchlauf (Review
    // 2026-08-16, bestätigter Befund): Die echte Auto-Vergabe belegt ein
    // frei werdendes Feld mit dem ERSTEN BEREITEN Spiel der Liste — ein
    // blockiertes überspringt sie (`sync.rs::auto_assign`). Ein
    // Reihenfolge-Durchlauf ließe das blockierte Spiel das Feld
    // „auf Verdacht" reservieren und verschöbe alles dahinter.
    // `None` = Feld für die restlichen Spiele unbrauchbar (Hallenregel).
    let mut free_at: Vec<Option<u64>> = input
        .courts
        .iter()
        .map(|c| Some(c.free_at_min.max(input.now_min)))
        .collect();
    let mut ready = input.player_ready_min.clone();
    let mut pending: Vec<&PredictMatch> = input.queue.iter().collect();
    let mut out = HashMap::new();

    while !pending.is_empty() {
        // Nächstes Ereignis: das Feld, das am frühesten frei wird.
        let Some((idx, t_free)) = free_at
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.map(|v| (i, v)))
            .min_by_key(|&(i, v)| (v, i))
        else {
            break; // kein nutzbares Feld mehr → Rest ohne Prognose
        };
        let court_hall = input.courts[idx].hall.trim();
        let t0 = t_free + input.buffer_min;

        // Erster BEREITER Kandidat in Listen-Reihenfolge gewinnt das Feld;
        // ist keiner bereit, wartet das Feld auf den, der am frühesten
        // bereit wird (Reihenfolge als Gleichstands-Regel).
        let mut best: Option<(usize, u64)> = None;
        let mut passt_keiner = true;
        for (pos, m) in pending.iter().enumerate() {
            let hall = m.hall.trim();
            if !(hall.is_empty() || court_hall == hall) {
                continue;
            }
            passt_keiner = false;
            let players_ready = m
                .players
                .iter()
                .filter_map(|p| ready.get(p).copied())
                .max()
                .unwrap_or(input.now_min);
            let start = t0.max(players_ready);
            if players_ready <= t0 {
                best = Some((pos, start));
                break;
            }
            if best.is_none_or(|(_, s)| start < s) {
                best = Some((pos, start));
            }
        }
        if passt_keiner {
            // Für dieses Feld gibt es kein erlaubtes Spiel mehr — aus der
            // Rotation nehmen, sonst drehte die Schleife ewig darauf.
            free_at[idx] = None;
            continue;
        }
        let (pos, start) = best.expect("passt_keiner deckt den None-Fall ab");
        let m = pending.remove(pos);
        out.insert(
            m.match_id,
            Prediction {
                start_min: start,
                uncertain: m.uncertain,
            },
        );
        let ende = start + m.duration_min;
        free_at[idx] = Some(ende);
        for p in &m.players {
            ready.insert(p.clone(), ende + input.rest_min);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        class: &str,
        disc: &str,
        assigned: u64,
        first_point: u64,
        finished: u64,
        regular: bool,
    ) -> MatchTimeEntry {
        MatchTimeEntry {
            first_assigned_ms: Some(assigned),
            first_point_ms: Some(first_point),
            finished_ms: Some(finished),
            finished_seen_ms: Some(finished),
            decided_seen_ms: None,
            pause_ms: None,
            class_label: class.to_string(),
            discipline: disc.to_string(),
            hall: String::new(),
            regular,
            off_court_polls: 0,
            finished_conflict_polls: 0,
        }
    }

    fn minuten(m: u64) -> u64 {
        m * 60_000
    }

    fn stats_aus(eintraege: Vec<MatchTimeEntry>) -> TimeStats {
        let map: HashMap<i64, MatchTimeEntry> = eintraege
            .into_iter()
            .enumerate()
            .map(|(i, e)| (i as i64, e))
            .collect();
        time_stats(&map)
    }

    /// Wie `entry`, aber mit Halle — für die Hallen-Achse (ADR 0036).
    fn entry_in(
        halle: &str,
        class: &str,
        disc: &str,
        assigned: u64,
        first_point: u64,
        finished: u64,
    ) -> MatchTimeEntry {
        MatchTimeEntry {
            hall: halle.to_string(),
            ..entry(class, disc, assigned, first_point, finished, true)
        }
    }

    // ── Die vier Achsen (Spec tl-sicht-feinschliff, Punkt 1) ────────────

    #[test]
    fn die_klassen_achse_fasst_alle_disziplinen_zusammen() {
        // A1.1: „Wie lange dauern die A-Spiele?" — über alle Disziplinen.
        let stats = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(20), true),
            entry("A", "DD", 0, minuten(5), minuten(30), true),
            entry("B", "HE", 0, minuten(5), minuten(40), true),
        ]);

        let zeilen = stats.rows_class();
        assert_eq!(zeilen.len(), 2, "A und B");
        assert_eq!(zeilen[0].class_label, "A");
        assert_eq!(zeilen[0].count, 2, "beide A-Spiele, egal welche Disziplin");
        assert_eq!(
            zeilen[0].discipline, "",
            "die Disziplin spielt hier keine Rolle"
        );
        assert_eq!(zeilen[1].class_label, "B");
        assert_eq!(zeilen[1].count, 1);
    }

    #[test]
    fn die_disziplin_achse_fasst_alle_klassen_zusammen() {
        // A1.1: „Sind Doppel langsamer als Einzel?" — über alle Klassen.
        let stats = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(20), true),
            entry("B", "HE", 0, minuten(5), minuten(30), true),
            entry("A", "DD", 0, minuten(5), minuten(40), true),
        ]);

        let zeilen = stats.rows_discipline();
        assert_eq!(zeilen.len(), 2, "HE und DD");
        assert_eq!(zeilen[0].discipline, "DD");
        assert_eq!(zeilen[0].count, 1);
        assert_eq!(
            zeilen[0].class_label, "",
            "die Klasse spielt hier keine Rolle"
        );
        assert_eq!(zeilen[1].discipline, "HE");
        assert_eq!(zeilen[1].count, 2);
    }

    #[test]
    fn die_hallen_achse_zaehlt_je_halle() {
        // Die Frage, wegen der die Achse überhaupt gebaut wird: Läuft eine
        // Halle systematisch langsamer als die andere?
        let stats = stats_aus(vec![
            entry_in("Halle A", "A", "HE", 0, minuten(5), minuten(20)),
            entry_in("Halle A", "B", "DD", 0, minuten(5), minuten(30)),
            entry_in("Halle B", "A", "HE", 0, minuten(5), minuten(50)),
        ]);

        let zeilen = stats.rows_hall();
        assert_eq!(zeilen.len(), 2);
        assert_eq!(zeilen[0].hall, "Halle A");
        assert_eq!(zeilen[0].count, 2);
        assert_eq!(zeilen[0].brutto_min, 25, "Median aus 20 und 30");
        assert_eq!(zeilen[1].hall, "Halle B");
        assert_eq!(zeilen[1].brutto_min, 50);
    }

    #[test]
    fn messwerte_ohne_halle_stehen_in_einer_eigenen_zeile() {
        // A1.8: Wer mitten im Turnier aktualisiert, hat Messwerte ohne
        // Halle. Sie dürfen keine echte Halle verfälschen — also eigene
        // Zeile, statt sie einer beliebigen zuzuschlagen.
        let stats = stats_aus(vec![
            entry_in("Halle A", "A", "HE", 0, minuten(5), minuten(20)),
            entry("A", "HE", 0, minuten(5), minuten(60), true),
        ]);

        let zeilen = stats.rows_hall();
        assert_eq!(zeilen.len(), 2);
        let ohne: Vec<&StatsRow> = zeilen.iter().filter(|z| z.hall.is_empty()).collect();
        assert_eq!(ohne.len(), 1, "genau eine Zeile ohne Halle");
        assert_eq!(ohne[0].count, 1);
        assert_eq!(ohne[0].brutto_min, 60);
        let a: Vec<&StatsRow> = zeilen.iter().filter(|z| z.hall == "Halle A").collect();
        assert_eq!(a[0].brutto_min, 20, "die echte Halle bleibt unverfälscht");
    }

    #[test]
    fn die_vier_achsen_zaehlen_dieselben_messwerte() {
        // A1.5: Jede Achse zerlegt dieselbe Menge — nur anders geschnitten.
        // Stimmt eine Summe nicht, fehlt irgendwo ein Messwert oder wird
        // einer doppelt gezählt.
        let stats = stats_aus(vec![
            entry_in("Halle A", "A", "HE", 0, minuten(5), minuten(20)),
            entry_in("Halle A", "A", "DD", 0, minuten(5), minuten(30)),
            entry_in("Halle B", "B", "HE", 0, minuten(5), minuten(40)),
            entry_in("Halle B", "B", "HE", 0, minuten(5), minuten(50)),
            entry("C", "MX", 0, minuten(5), minuten(60), true),
        ]);

        let summe = |zeilen: &[StatsRow]| zeilen.iter().map(|z| z.count).sum::<usize>();
        assert_eq!(summe(stats.rows()), 5, "Klasse x Disziplin");
        assert_eq!(summe(stats.rows_class()), 5, "nach Klasse");
        assert_eq!(summe(stats.rows_discipline()), 5, "nach Disziplin");
        assert_eq!(summe(stats.rows_hall()), 5, "nach Halle");
    }

    #[test]
    fn die_hallen_achse_aendert_die_prognose_kette_nicht() {
        // A1.10 / Nicht-Ziel N-3 — der wichtigste Test dieses Features:
        // Die Statistik ist nicht nur Anzeige, dieselben Mediane speisen
        // Wartelisten-Prognose und Live-Restzeit. Die neue Achse darf daran
        // NICHTS ändern. Zwei Fixtures, identisch bis auf die Halle.
        let ohne_hallen = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(20), true),
            entry("A", "HE", 0, minuten(5), minuten(30), true),
            entry("A", "HE", 0, minuten(1), minuten(40), true),
            entry("B", "DD", 0, minuten(5), minuten(50), true),
        ]);
        let mit_hallen = stats_aus(vec![
            entry_in("Halle A", "A", "HE", 0, minuten(5), minuten(20)),
            entry_in("Halle B", "A", "HE", 0, minuten(5), minuten(30)),
            entry_in("Halle A", "A", "HE", 0, minuten(1), minuten(40)),
            entry_in("Halle B", "B", "DD", 0, minuten(5), minuten(50)),
        ]);

        for (class, disc) in [("A", "HE"), ("B", "DD"), ("C", "MX")] {
            assert_eq!(
                ohne_hallen.group_times(class, disc, 25.0),
                mit_hallen.group_times(class, disc, 25.0),
                "die Fallback-Kette darf sich fuer {class}/{disc} nicht bewegen"
            );
            assert_eq!(
                ohne_hallen.group_duration(class, disc, 25.0),
                mit_hallen.group_duration(class, disc, 25.0),
            );
        }
        assert_eq!(
            ohne_hallen.tournament_brutto_min(),
            mit_hallen.tournament_brutto_min(),
        );
    }

    #[test]
    fn jede_achse_zeigt_auch_eine_einzelne_messung() {
        // A1.4: Anders als die Prognose-Kette (die erst ab MIN_SAMPLES
        // eigene Werte nutzt) zeigt die ANZEIGE jede Gruppe ab dem ersten
        // Messwert — sonst bliebe das Panel am Turniermorgen leer.
        let stats = stats_aus(vec![entry_in(
            "Halle A",
            "A",
            "HE",
            0,
            minuten(5),
            minuten(20),
        )]);
        assert_eq!(stats.rows().len(), 1);
        assert_eq!(stats.rows_class().len(), 1);
        assert_eq!(stats.rows_discipline().len(), 1);
        assert_eq!(stats.rows_hall().len(), 1);
    }

    // ── Statistik ───────────────────────────────────────────────────────

    #[test]
    fn nur_regulaere_spiele_mit_allen_stempeln_zaehlen() {
        let mut unvollstaendig = entry("A", "HE", 0, minuten(5), minuten(30), true);
        unvollstaendig.first_assigned_ms = None;
        let stats = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(30), true),
            entry("A", "HE", 0, minuten(5), minuten(30), false), // manuell
            unvollstaendig,
        ]);
        let rows = stats.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 1);
    }

    #[test]
    fn unplausible_messwerte_vergiften_den_median_nicht() {
        // Review-Befund 2026-08-16: Ein über Nacht auf dem Feld geparktes
        // Spiel (Mehrtages-Turnier) misst z. B. 960 min Brutto — solche
        // Werte fliegen aus der Statistik, statt den Median zu verschieben.
        let stats = stats_aus(vec![
            entry("A", "HE", 0, 0, minuten(20), true),
            entry("A", "HE", 0, 0, minuten(30), true),
            entry("A", "HE", 0, 0, minuten(960), true),
        ]);
        assert_eq!(stats.rows()[0].count, 2);
        assert_eq!(stats.rows()[0].brutto_min, 25);
    }

    #[test]
    fn die_zeile_traegt_mediane_von_brutto_netto_und_differenz() {
        // Drei Spiele A×HE: Brutto 20/30/40 → Median 30; Netto 15/25/39
        // → Median 25; Differenz je Spiel 5/5/1 → Median 5.
        let stats = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(20), true),
            entry("A", "HE", 0, minuten(5), minuten(30), true),
            entry("A", "HE", 0, minuten(1), minuten(40), true),
        ]);
        let rows = stats.rows();
        assert_eq!(rows[0].count, 3);
        assert_eq!(rows[0].brutto_min, 30);
        assert_eq!(rows[0].netto_min, 25);
        assert_eq!(rows[0].diff_min, 5);
    }

    #[test]
    fn der_median_mittelt_bei_gerader_anzahl() {
        // Brutto 20 und 30 → Median 25.
        let stats = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(20), true),
            entry("A", "HE", 0, minuten(5), minuten(30), true),
        ]);
        assert_eq!(stats.rows()[0].brutto_min, 25);
    }

    #[test]
    fn die_fallback_kette_greift_stufenweise() {
        // 3× A×HE (Median 30), 3× A×DD (Median 50) → Klasse A hat 6 Werte,
        // Turnier 6 Werte.
        let stats = stats_aus(vec![
            entry("A", "HE", 0, 0, minuten(20), true),
            entry("A", "HE", 0, 0, minuten(30), true),
            entry("A", "HE", 0, 0, minuten(40), true),
            entry("A", "DD", 0, 0, minuten(50), true),
            entry("A", "DD", 0, 0, minuten(50), true),
            entry("A", "DD", 0, 0, minuten(50), true),
        ]);
        // Gruppe vorhanden: A×HE → 30, sicher.
        assert_eq!(stats.group_duration("A", "HE", 25.0), (30, false));
        // Gruppe fehlt (A×MX): Klasse A gesamt → Median über 6 Werte
        // (20,30,40,50,50,50) → (40+50)/2 = 45.
        assert_eq!(stats.group_duration("A", "MX", 25.0), (45, false));
        // Klasse fehlt (B): Turnier gesamt → ebenfalls 45.
        assert_eq!(stats.group_duration("B", "HE", 25.0), (45, false));
    }

    #[test]
    fn ohne_genug_messwerte_gilt_der_default_als_unsicher() {
        let stats = stats_aus(vec![
            entry("A", "HE", 0, 0, minuten(20), true),
            entry("A", "HE", 0, 0, minuten(30), true),
        ]);
        // Nur 2 Messwerte: alle Stufen unter MIN_SAMPLES → Default 25, unsicher.
        assert_eq!(stats.group_duration("A", "HE", 25.0), (25, true));
    }

    #[test]
    fn ein_leeres_klassen_label_springt_auf_die_turnierstufe() {
        let stats = stats_aus(vec![
            entry("", "HE", 0, 0, minuten(20), true),
            entry("", "HE", 0, 0, minuten(30), true),
            entry("", "HE", 0, 0, minuten(40), true),
        ]);
        // Leere Klasse bildet KEINE eigene Gruppe/Klassen-Stufe, aber die
        // Messwerte zählen zur Turnierstufe.
        assert_eq!(stats.group_duration("", "HE", 25.0), (30, false));
        assert_eq!(stats.tournament_brutto_min(), Some(30));
    }

    // ── Puffer ──────────────────────────────────────────────────────────

    #[test]
    fn der_puffer_ist_mindestens_zwei_minuten() {
        assert_eq!(effective_buffer_min(false, 5.0), 2);
        assert_eq!(effective_buffer_min(true, 0.0), 2);
        assert_eq!(effective_buffer_min(true, 5.0), 5);
        assert_eq!(effective_buffer_min(true, 2.5), 2);
    }

    // ── Simulation ──────────────────────────────────────────────────────

    fn feld(hall: &str, free_at: u64) -> PredictCourt {
        PredictCourt {
            hall: hall.to_string(),
            free_at_min: free_at,
        }
    }

    fn spiel(id: i64, hall: &str, dauer: u64, spieler: &[&str]) -> PredictMatch {
        PredictMatch {
            match_id: id,
            hall: hall.to_string(),
            duration_min: dauer,
            uncertain: false,
            players: spieler.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn grundfall_zwei_felder_drei_spiele() {
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("", 1_000), feld("", 1_000)],
            player_ready_min: HashMap::new(),
            queue: vec![
                spiel(1, "", 20, &["a", "b"]),
                spiel(2, "", 20, &["c", "d"]),
                spiel(3, "", 20, &["e", "f"]),
            ],
        };
        let p = predict_starts(&input);
        assert_eq!(p[&1].start_min, 1_002);
        assert_eq!(p[&2].start_min, 1_002);
        // Spiel 3 wartet, bis das erste Feld frei wird: 1002+20+2 = 1024.
        assert_eq!(p[&3].start_min, 1_024);
    }

    #[test]
    fn belegte_felder_werden_ab_ihrer_restzeit_frei() {
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("", 1_015)], // laufendes Spiel, noch ~15 min
            player_ready_min: HashMap::new(),
            queue: vec![spiel(1, "", 20, &["a"])],
        };
        let p = predict_starts(&input);
        assert_eq!(p[&1].start_min, 1_017);
    }

    #[test]
    fn die_spieler_mindestpause_verschiebt_den_start() {
        let mut ready = HashMap::new();
        ready.insert("a".to_string(), 1_030); // Spieler a pausiert bis 1030
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("", 1_000)],
            player_ready_min: ready,
            queue: vec![spiel(1, "", 20, &["a", "b"])],
        };
        let p = predict_starts(&input);
        assert_eq!(
            p[&1].start_min, 1_030,
            "max(Feld frei + Puffer, Pause vorbei)"
        );
    }

    #[test]
    fn die_hallenregel_bindet_ein_spiel_an_seine_halle() {
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("Halle A", 1_000), feld("Halle B", 1_050)],
            player_ready_min: HashMap::new(),
            queue: vec![spiel(1, "Halle B", 20, &["a"])],
        };
        let p = predict_starts(&input);
        assert_eq!(p[&1].start_min, 1_052, "wartet auf SEINE Halle");
    }

    #[test]
    fn ohne_erlaubtes_feld_gibt_es_keine_prognose() {
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("Halle A", 1_000)],
            player_ready_min: HashMap::new(),
            queue: vec![spiel(1, "Halle C", 20, &["a"])],
        };
        assert!(predict_starts(&input).is_empty());
    }

    #[test]
    fn derselbe_spieler_kettet_seine_spiele_mit_ruhezeit() {
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 20,
            courts: vec![feld("", 1_000), feld("", 1_000)],
            player_ready_min: HashMap::new(),
            queue: vec![spiel(1, "", 30, &["a", "b"]), spiel(2, "", 30, &["a", "c"])],
        };
        let p = predict_starts(&input);
        assert_eq!(p[&1].start_min, 1_002);
        // Spieler a: fertig 1032, + 20 Ruhe = bereit 1052 — obwohl Feld 2
        // schon ab 1002 frei wäre.
        assert_eq!(p[&2].start_min, 1_052);
    }

    #[test]
    fn ein_blockiertes_spiel_reserviert_kein_feld() {
        // Review 2026-08-16 (bestätigt): Die ECHTE Auto-Vergabe überspringt
        // ein pausierendes Spiel und gibt das freie Feld dem nächsten
        // bereiten (sync.rs::auto_assign). Die Simulation muss das genauso
        // spielen — sonst hält Spiel A das Feld 40 Minuten „auf Verdacht"
        // und jede Prognose dahinter ist um Dutzende Minuten zu spät.
        let mut ready = HashMap::new();
        ready.insert("a".to_string(), 1_040); // Spieler a pausiert bis 1040
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("", 1_000)],
            player_ready_min: ready,
            queue: vec![spiel(1, "", 25, &["a"]), spiel(2, "", 25, &["b"])],
        };
        let p = predict_starts(&input);
        assert_eq!(p[&2].start_min, 1_002, "das bereite Spiel zieht vor");
        assert_eq!(
            p[&1].start_min, 1_040,
            "das blockierte startet nach seiner Pause (Feld ab 1029 frei)"
        );
    }

    // ── Live-Restzeit (Etappe D) ────────────────────────────────────────

    /// Gruppen-Zeitwerte wie im Fallback-Test oben, aber mit Netto/Differenz.
    #[test]
    fn group_times_liefert_netto_und_differenz_derselben_stufe() {
        let stats = stats_aus(vec![
            entry("A", "HE", 0, minuten(5), minuten(20), true),
            entry("A", "HE", 0, minuten(5), minuten(30), true),
            entry("A", "HE", 0, minuten(1), minuten(40), true),
        ]);
        // Brutto-Median 30, Netto-Median 25 (Anlauf = Differenz = 5).
        assert_eq!(
            stats.group_times("A", "HE", 25.0),
            GroupTimes {
                brutto_min: 30,
                netto_min: 25,
                uncertain: false
            }
        );
        // Ohne Messwerte: Default zählt komplett als Netto.
        let leer = stats_aus(vec![]);
        assert_eq!(
            leer.group_times("A", "HE", 25.0),
            GroupTimes {
                brutto_min: 25,
                netto_min: 25,
                uncertain: true
            }
        );
    }

    /// Standard-Zutaten der Live-Tests: Bo3 bis 21 (Deckel 30), Gruppen-
    /// Mediane Netto 20 / Brutto 25 (Anlauf also 5), erster Punkt bei
    /// Minute 1000, `elapsed_net_min` Minuten Nettozeit gespielt.
    fn live(sets: &[(i64, i64)], elapsed_net_min: u64) -> LiveRemainInput {
        LiveRemainInput {
            now_ms: minuten(1_000 + elapsed_net_min),
            sets: sets.to_vec(),
            best_of: 3,
            target: 21,
            cap: 30,
            first_assigned_ms: Some(minuten(995)),
            first_point_ms: Some(minuten(1_000)),
            netto_median_min: 20,
            brutto_median_min: 25,
        }
    }

    #[test]
    fn endphase_im_dritten_satz_gibt_das_feld_gleich_frei() {
        // 14:6 im dritten Satz nach 30 min: nur noch der Satzrest zählt —
        // statt „Median − verstrichen" (liefe längst auf 0) kommen ~4 min
        // heraus.
        let input = live(&[(21, 15), (15, 21), (14, 6)], 30);
        assert_eq!(live_remaining_min(&input), 4);
    }

    #[test]
    fn ohne_ersten_punkt_haelt_das_feld_die_volle_dauer() {
        // 0:0 drei Minuten nach Zuweisung: volle Nettodauer + Rest-Anlauf.
        let mut input = live(&[(0, 0)], 0);
        input.first_point_ms = None;
        input.now_ms = minuten(998);
        assert_eq!(live_remaining_min(&input), 22, "20 Netto + 2 Rest-Anlauf");
        // Anlauf überzogen: es bleibt die volle Nettodauer.
        input.now_ms = minuten(1_010);
        assert_eq!(live_remaining_min(&input), 20);
    }

    #[test]
    fn ein_moeglicher_entscheidungssatz_zaehlt_nach_wahrscheinlichkeit() {
        // 21:10 gewonnen UND 11:8 vorn: Der Favorit macht sehr wahrscheinlich
        // in zwei Sätzen zu — der dritte Satz zählt nur noch mit ~6 % hinein,
        // das Feld ist praktisch nur noch den Satzrest belegt.
        let vorn = live(&[(21, 10), (11, 8)], 15);
        assert_eq!(live_remaining_min(&vorn), 5);
        // Führt stattdessen der Satzverlierer 11:8, kippt das Bild: gut die
        // Hälfte eines dritten Satzes kommt dazu (der Erstsatz-Sieger bleibt
        // nach Punkten leicht favorisiert, den zweiten doch noch zu drehen).
        let hinten = live(&[(21, 10), (8, 11)], 15);
        assert_eq!(live_remaining_min(&hinten), 11);
    }

    #[test]
    fn bei_gleichstand_zaehlt_etwa_der_halbe_entscheidungssatz() {
        // 21:18er-Erstsatz (fast ausgeglichen) und 13:13 im zweiten: Ausgang
        // offen — knapp der halbe dritte Satz wandert in die Schätzung.
        let input = live(&[(21, 18), (13, 13)], 20);
        assert_eq!(live_remaining_min(&input), 11);
    }

    #[test]
    fn ein_eingefrorener_stand_klemmt_das_tempo() {
        // Tablet tot bei 5:3, Uhr läuft 40 min weiter: ohne Klemme explodierte
        // das Tempo (>140 s/Punkt) — gedeckelt auf 2× Prior bleiben ~40 min.
        let input = live(&[(5, 3)], 40);
        assert_eq!(live_remaining_min(&input), 40);
    }

    #[test]
    fn verlaengerung_rechnet_mindestens_zwei_restpunkte() {
        // 24:23 über dem Zielpunkt: rechnerisch wäre der Satzrest negativ —
        // solange der Satz läuft, bleiben mindestens 2 Restpunkte stehen,
        // und die Verlängerung ist praktisch ein Münzwurf (halber
        // Entscheidungssatz obendrauf).
        let input = live(&[(24, 23)], 15);
        assert_eq!(live_remaining_min(&input), 20);
    }

    #[test]
    fn das_zaehlsystem_des_matches_bestimmt_die_satzlaenge() {
        // 15er-Format (Deckel 21): Sätze sind kürzer, die fertigen Sätze
        // dieses Matches liefern die Punkte-je-Satz-Schätzung (25) — und der
        // klare Favorit (15:10 + 10:5) drückt den Entscheidungssatz-Anteil
        // auf ein paar Prozent.
        let mut input = live(&[(15, 10), (10, 5)], 12);
        input.target = 15;
        input.cap = 21;
        input.netto_median_min = 15;
        input.brutto_median_min = 18;
        assert_eq!(live_remaining_min(&input), 3);
    }

    #[test]
    fn die_restzeit_ist_auf_das_doppelte_brutto_gedeckelt() {
        // Wie der eingefrorene Stand (roh ~40 min), aber mit kleinem
        // Brutto-Median: die Ergebnis-Klemme greift bei 2 × 15 = 30.
        let mut input = live(&[(5, 3)], 40);
        input.brutto_median_min = 15;
        assert_eq!(live_remaining_min(&input), 30);
    }

    #[test]
    fn innerhalb_einer_minute_bleibt_die_schaetzung_stabil() {
        // Rev-Churn-Wächter: Der TL-State wird alle ~2 s gebaut — ohne
        // Minuten-Quantisierung der Nettozeit schöbe jede Sekunde das Tempo
        // minimal und kippte die Aufrundung mitten in der Minute (hier:
        // 4,96 → 5,09 min bei +30 s). Gleiche Minute ⇒ gleiches Ergebnis.
        let a = live(&[(21, 10), (11, 8)], 15);
        let mut b = a.clone();
        b.now_ms += 30_000;
        assert_eq!(live_remaining_min(&a), live_remaining_min(&b));
    }

    #[test]
    fn ein_gleichstand_ist_nie_ein_fertiger_satz() {
        // 30:30 kann nur ein manipulierter/kaputter Score-Frame liefern
        // (regulär endet der Satz bei 30:29 am Deckel) — er darf trotzdem
        // niemandem als Satzgewinn gutgeschrieben werden, sondern zählt
        // als laufende Verlängerung.
        let input = live(&[(30, 30)], 10);
        assert_eq!(live_remaining_min(&input), 13);
    }

    #[test]
    fn ein_entschiedenes_spiel_ist_gleich_frei() {
        // Beide Sätze durch, Ergebnis noch nicht eingetragen: das Feld wird
        // in einer Minute frei — kein Zuschlag für unmögliche Sätze.
        let input = live(&[(21, 5), (21, 7)], 25);
        assert_eq!(live_remaining_min(&input), 1);
    }

    #[test]
    fn ein_kaputtes_best_of_wird_geklemmt() {
        // `NumSets` kommt roh aus BTP (nur > 0 geprüft) — `expected_sets`
        // ist eine Doppel-Rekursion mit ~C(2n, n) Aufrufen, ein NumSets von
        // 41 fröre den ~2-s-TL-State-Bau ein (Review 2026-08-17). Alles
        // über Best-of-5 wird deshalb wie Best-of-5 gerechnet.
        let mut kaputt = live(&[(11, 8)], 10);
        kaputt.best_of = 41;
        let mut bo5 = live(&[(11, 8)], 10);
        bo5.best_of = 5;
        assert_eq!(live_remaining_min(&kaputt), live_remaining_min(&bo5));
    }

    #[test]
    fn die_satzregel_bleibt_mit_der_server_regel_im_gleichschritt() {
        // Drift-Wächter (Review 2026-08-17): `set_complete` ist bewusst eine
        // lokale Kopie von `server::set_is_complete` — plus Gleichstands-
        // Wache (30:30 zählt hier nie). Über alle erreichbaren Stände der
        // gängigen Formate müssen beide dasselbe sagen; ändert jemand die
        // Server-Regel, schlägt dieser Test an.
        for &(target, cap) in &[(21, 30), (15, 21), (11, 11), (11, 15)] {
            for a in 0..=cap {
                for b in 0..=cap {
                    if a == b {
                        continue; // Gleichstand: bewusste lokale Abweichung
                    }
                    assert_eq!(
                        set_complete(a, b, target, cap),
                        crate::tablet::server::set_is_complete(a, b, target, cap),
                        "({a},{b}) bei {target}/{cap}"
                    );
                }
            }
        }
    }

    #[test]
    fn fehlendes_format_faellt_auf_bo3_bis_21() {
        let mut input = live(&[(21, 15), (15, 21), (14, 6)], 30);
        input.best_of = 0;
        input.target = 0;
        input.cap = 0;
        assert_eq!(live_remaining_min(&input), 4, "wie der Normalfall");
    }

    #[test]
    fn die_unsicherheit_des_gruppenwerts_wandert_in_die_prognose() {
        let mut m = spiel(1, "", 25, &["a"]);
        m.uncertain = true;
        let input = PredictInput {
            now_min: 1_000,
            buffer_min: 2,
            rest_min: 0,
            courts: vec![feld("", 1_000)],
            player_ready_min: HashMap::new(),
            queue: vec![m],
        };
        assert!(predict_starts(&input)[&1].uncertain);
    }
}
