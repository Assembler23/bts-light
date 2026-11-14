//! Zeitplan im TL-Web (Spec `tl-zeitplan`, ADR 0094): reine Rechenregeln,
//! ohne `TabletState` — damit sie ohne Fixture prüfbar sind.

/// Wo ein Spiel für die Zeitplan-Messung steht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpielLage {
    /// Angesetzt und noch nicht gerufen.
    Wartend,
    /// Angesetzt und in die Vorbereitung gerufen.
    Gerufen,
    AufFeld,
    Beendet,
    /// Kampflos gewertet — es gab keinen Start, das Spiel beginnt kein
    /// Zeitfenster.
    Kampflos,
}

impl SpielLage {
    fn begonnen(self) -> bool {
        matches!(self, Self::Gerufen | Self::AufFeld | Self::Beendet)
    }
}

/// Ein Spiel, so weit die Zeitplan-Messung es kennen muss.
#[derive(Debug, Clone, Copy)]
pub struct ZpSpiel {
    /// BTP-Planzeit `YYYYMMDDHHMM`.
    pub planned_time: Option<i64>,
    pub lage: SpielLage,
    /// Prognose (Unix-ms, „nur Startwert"), nur an wartenden Spielen
    /// ausgewertet.
    pub prognose: Option<(u64, bool)>,
}

/// Der Zeitplan-Stand im TL-Zustand. Der Host wählt das Bezugsfenster, die
/// Seite bildet die Differenz zur Planzeit (ADR 0094): Ein fertiger
/// Minutenwert bewegte die Revision im Minutentakt, und aus der gekürzten
/// Cloud-Warteliste könnte die Seite das Fenster nicht selbst finden.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TlSchedule {
    /// `no_plan` · `done_today` · `last_round` · `off` · `no_forecast` · `ok`.
    pub kind: String,
    /// Ist heute schon ein Spiel gerufen, auf dem Feld oder beendet? Solange
    /// nicht, zeigt die Seite „Beginn HH:MM".
    pub started: bool,
    /// Planzeit des Bezugsfensters, roh — für die Anzeige der Uhrzeit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_time: Option<i64>,
    /// Dieselbe als Unix-ms — für die Differenz.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_ms: Option<u64>,
    /// Früheste Prognose der wartenden Spiele des Bezugsfensters. Aus dem
    /// Fingerabdruck genullt (zeitabgeleitet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_start_ms: Option<u64>,
    /// Steht hinter dieser Prognose nur der Startwert? Ebenfalls genullt.
    pub predicted_uncertain: bool,
}

/// Ab wie vielen begonnenen Spielen ein Zeitfenster als begonnen gilt: Ein
/// einzeln vorgezogenes Spiel soll die Messung nicht ans übernächste Fenster
/// schieben. Ein Fenster mit nur einem Spiel beginnt mit diesem.
const BEGINN_AB_SPIELEN: usize = 2;

/// Wählt das Bezugsfenster und seinen Bezugswert.
///
/// `spiele` in Wartelisten-Reihenfolge (bei gleicher Prognose gewinnt das
/// erste); `heute` als `YYYYMMDD` — Parameter statt Uhr-Aufruf, damit die
/// Tests nicht an der Zeitzone des Rechners hängen. `zu_ms` rechnet die
/// Wandzeit um ([`crate::btp::model::planned_time_unix_ms`]).
pub fn lage(
    spiele: &[ZpSpiel],
    heute: u32,
    prognose_an: bool,
    zu_ms: impl Fn(i64) -> Option<u64>,
) -> TlSchedule {
    let kind = |k: &str| k.to_string();
    if spiele.iter().all(|s| s.planned_time.is_none()) {
        return TlSchedule {
            kind: kind("no_plan"),
            ..Default::default()
        };
    }
    let heutige: Vec<(i64, &ZpSpiel)> = spiele
        .iter()
        .filter_map(|s| Some((s.planned_time?, s)))
        .filter(|(pt, _)| pt / 10_000 == i64::from(heute))
        .collect();
    let started = heutige.iter().any(|(_, s)| s.lage.begonnen());
    let ohne_wert = |k: &str| TlSchedule {
        kind: kind(k),
        started,
        ..Default::default()
    };

    let mut fenster: Vec<i64> = heutige
        .iter()
        .filter(|(_, s)| s.lage == SpielLage::Wartend)
        .map(|(pt, _)| *pt)
        .collect();
    fenster.sort_unstable();
    fenster.dedup();
    if fenster.is_empty() {
        let laeuft = heutige
            .iter()
            .any(|(_, s)| matches!(s.lage, SpielLage::Gerufen | SpielLage::AufFeld));
        return ohne_wert(if laeuft { "last_round" } else { "done_today" });
    }

    let begonnen = |t: i64| {
        let im_fenster = heutige.iter().filter(|(pt, _)| *pt == t);
        let groesse = im_fenster
            .clone()
            .filter(|(_, s)| s.lage != SpielLage::Kampflos)
            .count();
        let gestartet = im_fenster.filter(|(_, s)| s.lage.begonnen()).count();
        gestartet >= BEGINN_AB_SPIELEN.min(groesse).max(1)
    };
    let Some(t) = fenster.into_iter().find(|t| !begonnen(*t)) else {
        return ohne_wert("last_round");
    };

    let planned_ms = zu_ms(t);
    let mit_fenster = |k: &str| TlSchedule {
        kind: kind(k),
        started,
        planned_time: Some(t),
        planned_ms,
        ..Default::default()
    };
    if !prognose_an {
        return mit_fenster("off");
    }
    // Früheste Prognose, nicht die des ersten Spiels: Ist das erste
    // blockiert (Spieler steht noch auf dem Feld), läuft das Fenster
    // trotzdem pünktlich an.
    let mut beste: Option<(u64, bool)> = None;
    for (pt, s) in &heutige {
        if *pt != t || s.lage != SpielLage::Wartend {
            continue;
        }
        if let Some((ms, unsicher)) = s.prognose {
            if beste.is_none_or(|(b, _)| ms < b) {
                beste = Some((ms, unsicher));
            }
        }
    }
    match (beste, planned_ms) {
        (Some((ms, unsicher)), Some(_)) => TlSchedule {
            predicted_start_ms: Some(ms),
            predicted_uncertain: unsicher,
            ..mit_fenster("ok")
        },
        // Keine Prognose (etwa nur offene Paarungen) oder mehrdeutige
        // Wandzeit: neutral bleiben statt ein Ersatzmaß zu erfinden.
        _ => mit_fenster("no_forecast"),
    }
}

/// Jenseits davon ist eine Abweichung kein „verspätet gestartet" mehr,
/// sondern ein Spiel, das an einem anderen Tag lief als geplant, oder ein
/// Stempel aus einer falsch gestellten Uhr. Dann lieber keine Zahl.
const MAX_ABWEICHUNG_MIN: i64 = 720;

/// Abstand des tatsächlichen Starts (erste Feldzuweisung) zur Planzeit in
/// ganzen Minuten; positiv = später als geplant. Zur Null hin abgeschnitten,
/// damit „4:30 zu früh" als −4 und nicht als −5 dasteht.
///
/// `None` bei Walkover (es gab keinen Start), ohne Stempel, ohne Planzeit
/// und bei unplausibel großem Betrag.
pub fn start_abweichung_min(
    planned_ms: Option<u64>,
    first_assigned_ms: Option<u64>,
    walkover: bool,
) -> Option<i64> {
    if walkover {
        return None;
    }
    let plan = i64::try_from(planned_ms?).ok()?;
    let start = i64::try_from(first_assigned_ms?).ok()?;
    let mins = (start - plan) / 60_000;
    (mins.abs() <= MAX_ABWEICHUNG_MIN).then_some(mins)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: u64 = 1_800_000_000_000;
    const MIN: u64 = 60_000;

    #[test]
    fn abweichung_spaeter_gestartet() {
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN + 18 * MIN + 59_000), false),
            Some(18)
        );
        assert_eq!(start_abweichung_min(Some(PLAN), Some(PLAN), false), Some(0));
    }

    #[test]
    fn abweichung_frueher_schneidet_zur_null() {
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN - 4 * MIN - 30_000), false),
            Some(-4)
        );
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN - 30_000), false),
            Some(0)
        );
    }

    #[test]
    fn abweichung_fehlt_bei_walkover() {
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN + MIN), true),
            None
        );
    }

    #[test]
    fn abweichung_fehlt_ohne_stempel() {
        assert_eq!(start_abweichung_min(Some(PLAN), None, false), None);
        assert_eq!(start_abweichung_min(None, Some(PLAN), false), None);
    }

    #[test]
    fn abweichung_fehlt_ueber_zwoelf_stunden() {
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN + 720 * MIN), false),
            Some(720)
        );
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN + 721 * MIN), false),
            None
        );
        assert_eq!(
            start_abweichung_min(Some(PLAN), Some(PLAN - 721 * MIN), false),
            None
        );
    }

    // ── lage ────────────────────────────────────────────────────────────

    const HEUTE: u32 = 20_261_114;
    const T10: i64 = 202_611_141_000;
    const T11: i64 = 202_611_141_100;
    const T14: i64 = 202_611_141_400;
    const MORGEN9: i64 = 202_611_150_900;

    /// Test-Umrechnung ohne Zeitzone: Minuten seit Mitternacht in ms.
    fn ms(pt: i64) -> Option<u64> {
        let hhmm = (pt % 10_000) as u64;
        Some((hhmm / 100 * 60 + hhmm % 100) * 60_000)
    }

    fn sp(pt: i64, lage: SpielLage) -> ZpSpiel {
        ZpSpiel {
            planned_time: Some(pt),
            lage,
            prognose: None,
        }
    }

    fn mit_prognose(pt: i64, min_nach_mitternacht: u64) -> ZpSpiel {
        ZpSpiel {
            planned_time: Some(pt),
            lage: SpielLage::Wartend,
            prognose: Some((min_nach_mitternacht * 60_000, false)),
        }
    }

    use SpielLage::*;

    #[test]
    fn keine_planzeiten() {
        let ohne = ZpSpiel {
            planned_time: None,
            lage: Wartend,
            prognose: None,
        };
        assert_eq!(lage(&[ohne, ohne], HEUTE, true, ms).kind, "no_plan");
        assert_eq!(lage(&[], HEUTE, true, ms).kind, "no_plan");
    }

    #[test]
    fn heute_keine_spiele_mehr() {
        let s = lage(&[sp(T10, Beendet), sp(T11, Beendet)], HEUTE, true, ms);
        assert_eq!(s.kind, "done_today");
        assert!(s.started);
        assert_eq!(s.planned_time, None);
    }

    #[test]
    fn spiele_anderer_tage_zaehlen_nicht() {
        let s = lage(
            &[sp(T10, Beendet), mit_prognose(MORGEN9, 600)],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "done_today");
        // Nur Spiele von morgen: auch dann steht heute nichts mehr aus.
        let nur_morgen = lage(&[mit_prognose(MORGEN9, 600)], HEUTE, true, ms);
        assert_eq!(nur_morgen.kind, "done_today");
        assert!(!nur_morgen.started);
    }

    #[test]
    fn letzte_runde_laeuft() {
        // Nichts wartet mehr, aber es wird noch gespielt.
        let s = lage(&[sp(T10, Beendet), sp(T11, AufFeld)], HEUTE, true, ms);
        assert_eq!(s.kind, "last_round");
        // Es wartet noch etwas, aber jedes Fenster ist begonnen.
        let s = lage(
            &[sp(T11, AufFeld), sp(T11, Gerufen), mit_prognose(T11, 700)],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "last_round");
        assert_eq!(s.planned_time, None);
    }

    #[test]
    fn ein_vorgezogenes_spiel_beginnt_kein_fenster() {
        let s = lage(
            &[
                sp(T10, AufFeld),
                sp(T10, AufFeld),
                sp(T14, AufFeld), // einzeln vorgezogen
                mit_prognose(T14, 14 * 60 + 3),
                mit_prognose(T14, 14 * 60 + 20),
            ],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "ok");
        assert_eq!(s.planned_time, Some(T14));
    }

    #[test]
    fn zwei_spiele_beginnen_das_fenster() {
        let s = lage(
            &[
                sp(T11, AufFeld),
                sp(T11, Gerufen),
                mit_prognose(T11, 11 * 60 + 30),
                mit_prognose(T14, 14 * 60 + 8),
            ],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.planned_time, Some(T14), "11:00 ist begonnen");
        assert_eq!(s.predicted_start_ms, Some((14 * 60 + 8) * 60_000));
    }

    #[test]
    fn fenster_mit_einem_spiel_beginnt_mit_diesem() {
        // Das 11:00-Fenster hat ein einziges Spiel und es wartet: unbegonnen.
        let s = lage(&[mit_prognose(T11, 670)], HEUTE, true, ms);
        assert_eq!(s.planned_time, Some(T11));
        // Ein Fenster aus einem regulären und einem kampflosen Spiel: Das
        // reguläre allein beginnt es — danach wartet dort nichts mehr.
        let s = lage(
            &[sp(T11, Kampflos), sp(T11, AufFeld), mit_prognose(T14, 845)],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.planned_time, Some(T14));
    }

    #[test]
    fn walkover_beginnt_kein_fenster() {
        let s = lage(
            &[
                sp(T11, Kampflos),
                sp(T11, Kampflos),
                mit_prognose(T11, 665),
                mit_prognose(T11, 668),
            ],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "ok");
        assert_eq!(s.planned_time, Some(T11));
        assert!(!s.started, "kampflos ist kein Spielbeginn");
    }

    #[test]
    fn fruehestes_unbegonnenes_fenster_gewinnt() {
        let s = lage(
            &[
                mit_prognose(T14, 14 * 60),
                sp(T10, Beendet),
                sp(T10, AufFeld),
                mit_prognose(T10, 10 * 60 + 40), // Rest des laufenden Fensters
                mit_prognose(T11, 11 * 60 + 18),
            ],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "ok");
        assert_eq!(s.planned_time, Some(T11));
        assert_eq!(s.planned_ms, Some(11 * 60 * 60_000));
        assert_eq!(s.predicted_start_ms, Some((11 * 60 + 18) * 60_000));
        assert!(s.started);
    }

    #[test]
    fn minimum_der_prognosen_nicht_das_erste_der_liste() {
        let mut unsicher = mit_prognose(T11, 11 * 60 + 2);
        unsicher.prognose = Some(((11 * 60 + 2) * 60_000, true));
        let s = lage(
            &[
                mit_prognose(T11, 11 * 60 + 25), // blockiert, steht oben
                unsicher,
                mit_prognose(T11, 11 * 60 + 2), // gleich früh, aber später in der Liste
            ],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.predicted_start_ms, Some((11 * 60 + 2) * 60_000));
        assert!(s.predicted_uncertain, "bei Gleichstand das erste der Liste");
    }

    #[test]
    fn nur_offene_paarungen_ergibt_no_forecast() {
        let s = lage(
            &[sp(T10, Beendet), sp(T11, Wartend), sp(T11, Wartend)],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "no_forecast");
        assert_eq!(s.planned_time, Some(T11));
        assert_eq!(s.predicted_start_ms, None);
        // Kein Sprung zum übernächsten Fenster, auch wenn das eine Prognose hat.
        let s = lage(&[sp(T11, Wartend), mit_prognose(T14, 850)], HEUTE, true, ms);
        assert_eq!(s.kind, "no_forecast");
        assert_eq!(s.planned_time, Some(T11));
    }

    #[test]
    fn mehrdeutige_wandzeit_ergibt_no_forecast() {
        let s = lage(&[mit_prognose(T11, 670)], HEUTE, true, |_| None);
        assert_eq!(s.kind, "no_forecast");
        assert_eq!(s.planned_ms, None);
    }

    #[test]
    fn prognose_aus() {
        let s = lage(&[mit_prognose(T11, 670)], HEUTE, false, ms);
        assert_eq!(s.kind, "off");
        assert_eq!(s.planned_time, Some(T11));
        assert_eq!(s.predicted_start_ms, None);
    }

    #[test]
    fn vor_dem_ersten_spiel_started_false() {
        let s = lage(
            &[mit_prognose(T10, 600), mit_prognose(T11, 660)],
            HEUTE,
            true,
            ms,
        );
        assert_eq!(s.kind, "ok");
        assert!(!s.started);
        assert_eq!(s.planned_time, Some(T10));
    }

    #[test]
    fn leere_felder_bleiben_vom_draht_weg() {
        let json = serde_json::to_string(&lage(&[sp(T10, Beendet)], HEUTE, true, ms)).unwrap();
        assert!(!json.contains("planned_ms"), "{json}");
        assert!(!json.contains("predicted_start_ms"), "{json}");
    }
}
