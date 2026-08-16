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
    pub brutto_min: u64,
    pub netto_min: u64,
}

/// Median-Statistik über die Messwerte eines Turniers.
#[derive(Debug, Clone, Default)]
pub struct TimeStats {
    measurements: Vec<Measurement>,
}

/// Eine Zeile der Auswertung (je Klasse × Disziplin), Mediane in Minuten.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsRow {
    pub class_label: String,
    pub discipline: String,
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
            Some(Measurement {
                class_label: e.class_label.trim().to_string(),
                discipline: e.discipline.trim().to_string(),
                brutto_min: finished.saturating_sub(assigned) / 60_000,
                netto_min: finished.saturating_sub(first_point) / 60_000,
            })
        })
        // Plausibilitätsgrenze (Review 2026-08-16): über Nacht geparkte
        // Spiele messen Stunden — die vergiften den Median nicht.
        .filter(|m| m.brutto_min <= crate::tablet::match_times::MAX_PLAUSIBLE_BRUTTO_MIN as u64)
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
    TimeStats { measurements }
}

impl TimeStats {
    /// Auswertungszeilen je Klasse × Disziplin (Median Brutto/Netto/
    /// Differenz, Anzahl), sortiert nach Klasse, dann Disziplin.
    pub fn rows(&self) -> Vec<StatsRow> {
        let mut groups: Vec<(&str, &str)> = self
            .measurements
            .iter()
            .map(|m| (m.class_label.as_str(), m.discipline.as_str()))
            .collect();
        groups.dedup();
        groups
            .into_iter()
            .map(|(class, disc)| {
                let (mut brutto, mut netto, mut diff) = (Vec::new(), Vec::new(), Vec::new());
                for m in self
                    .measurements
                    .iter()
                    .filter(|m| m.class_label == class && m.discipline == disc)
                {
                    brutto.push(m.brutto_min);
                    netto.push(m.netto_min);
                    diff.push(m.brutto_min.saturating_sub(m.netto_min));
                }
                StatsRow {
                    class_label: class.to_string(),
                    discipline: disc.to_string(),
                    count: brutto.len(),
                    brutto_min: median_min(&brutto).unwrap_or(0),
                    netto_min: median_min(&netto).unwrap_or(0),
                    diff_min: median_min(&diff).unwrap_or(0),
                }
            })
            .collect()
    }

    /// Turnierweiter Brutto-Median (ab [`MIN_SAMPLES`] Messwerten).
    pub fn tournament_brutto_min(&self) -> Option<u64> {
        if self.measurements.len() < MIN_SAMPLES {
            return None;
        }
        let brutto: Vec<u64> = self.measurements.iter().map(|m| m.brutto_min).collect();
        median_min(&brutto)
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
        let class = class_label.trim();
        let disc = discipline.trim();
        let stufe = |filter: &dyn Fn(&&Measurement) -> bool| -> Option<u64> {
            let brutto: Vec<u64> = self
                .measurements
                .iter()
                .filter(filter)
                .map(|m| m.brutto_min)
                .collect();
            if brutto.len() < MIN_SAMPLES {
                return None;
            }
            median_min(&brutto)
        };
        if !class.is_empty() {
            if let Some(v) = stufe(&|m| m.class_label == class && m.discipline == disc) {
                return (v, false);
            }
            if let Some(v) = stufe(&|m| m.class_label == class) {
                return (v, false);
            }
        }
        if let Some(v) = self.tournament_brutto_min() {
            return (v, false);
        }
        let default = if default_mins.is_finite() && default_mins > 0.0 {
            default_mins.round() as u64
        } else {
            25
        };
        (default, true)
    }
}

/// Vollmodell-Simulation (E8): spielt die Warteliste in Reihenfolge auf
/// die Felder durch. Je Spiel das früheste erlaubte Feld;
/// `start = max(feld_frei + puffer, spieler_bereit)`. Ausgenommene Spiele
/// stehen gar nicht erst in der `queue` (sie belegen kein Feld und
/// bekommen keine Prognose). Spiele ohne erlaubtes Feld bekommen keinen
/// Eintrag.
pub fn predict_starts(input: &PredictInput) -> HashMap<i64, Prediction> {
    let mut free_at: Vec<u64> = input
        .courts
        .iter()
        .map(|c| c.free_at_min.max(input.now_min))
        .collect();
    let mut ready = input.player_ready_min.clone();
    let mut out = HashMap::new();

    for m in &input.queue {
        // Frühestes erlaubtes Feld (leere Spiel-Halle = jede Halle).
        let hall = m.hall.trim();
        let best = input
            .courts
            .iter()
            .enumerate()
            .filter(|(_, c)| hall.is_empty() || c.hall.trim() == hall)
            .min_by_key(|&(i, _)| free_at[i]);
        let Some((idx, _)) = best else {
            continue; // kein erlaubtes Feld → keine Prognose
        };
        let players_ready = m
            .players
            .iter()
            .filter_map(|p| ready.get(p).copied())
            .max()
            .unwrap_or(input.now_min);
        let start = (free_at[idx] + input.buffer_min).max(players_ready);
        out.insert(
            m.match_id,
            Prediction {
                start_min: start,
                uncertain: m.uncertain,
            },
        );
        let ende = start + m.duration_min;
        free_at[idx] = ende;
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
            class_label: class.to_string(),
            discipline: disc.to_string(),
            regular,
            off_court_polls: 0,
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
        assert_eq!(p[&1].start_min, 1_030, "max(Feld frei + Puffer, Pause vorbei)");
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
