//! Zeitplan im TL-Web (Spec `tl-zeitplan`, ADR 0094): reine Rechenregeln,
//! ohne `TabletState` — damit sie ohne Fixture prüfbar sind.

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
}
