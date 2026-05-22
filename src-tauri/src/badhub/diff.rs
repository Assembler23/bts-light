//! Snapshot-Diff: entscheidet, ob ein voller `tset` oder ein kleines
//! `tupdate_match` an Badhub gesendet werden muss – oder nichts.
//!
//! Ein `tupdate_match` (~80 B) ist nur zulässig, wenn die Court-Belegung
//! unverändert ist und sich der Punktestand genau eines laufenden Matches
//! geändert hat. Jede strukturelle Änderung löst einen vollen `tset` aus.

use std::collections::BTreeMap;

use crate::badhub::payload::{build_tset, build_tupdate, TsetMessage, TupdateMessage};
use crate::btp::model::{BtpMatch, BtpSnapshot, MatchStatus};

/// Ergebnis eines Snapshot-Vergleichs.
#[derive(Debug, PartialEq)]
pub enum Update {
    /// Vollständiger Turnier-Stand.
    Full(TsetMessage),
    /// Nur ein geänderter Punktestand.
    Single(TupdateMessage),
    /// Keine relevante Änderung – nichts senden.
    None,
}

/// Vergleicht den vorigen mit dem aktuellen Snapshot.
pub fn diff(prev: Option<&BtpSnapshot>, current: &BtpSnapshot, rid: u64) -> Update {
    let Some(prev) = prev else {
        // Erster Snapshot – immer ein voller tset.
        return Update::Full(build_tset(current, rid));
    };

    // Strukturelle Änderung an der Court-Belegung → voller tset.
    if court_assignment(prev) != court_assignment(current) {
        return Update::Full(build_tset(current, rid));
    }

    // Gleiche Matches auf gleichen Courts: nur Punktestände vergleichen.
    let prev_sets: BTreeMap<i64, &Vec<(i64, i64)>> =
        on_court(prev).map(|m| (m.id, &m.sets)).collect();
    let changed: Vec<&BtpMatch> = on_court(current)
        .filter(|m| prev_sets.get(&m.id) != Some(&&m.sets))
        .collect();

    match changed.as_slice() {
        [] => Update::None,
        [m] => Update::Single(build_tupdate(m, rid)),
        // Mehrere gleichzeitige Änderungen → der Einfachheit halber voll.
        _ => Update::Full(build_tset(current, rid)),
    }
}

/// Iteriert über die aktuell einem Court zugewiesenen Matches.
fn on_court(snapshot: &BtpSnapshot) -> impl Iterator<Item = &BtpMatch> {
    snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::OnCourt)
}

/// Court-Belegung als sortierte Abbildung Match-ID → Court-Name.
fn court_assignment(snapshot: &BtpSnapshot) -> BTreeMap<i64, Option<String>> {
    on_court(snapshot)
        .map(|m| (m.id, m.court.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpPlayer, Discipline, MatchResult};

    fn match_on_court(id: i64, court: &str, sets: Vec<(i64, i64)>) -> BtpMatch {
        BtpMatch {
            id,
            draw_id: 1,
            planning_id: 1000 + id,
            draw_name: "HE".to_string(),
            discipline: Discipline::MensSingles,
            round_name: "G1".to_string(),
            match_num: Some(id),
            team1: vec![BtpPlayer {
                name: "A".to_string(),
                first: String::new(),
                last: "A".to_string(),
                member_id: None,
                nationality: None,
            }],
            team2: vec![BtpPlayer {
                name: "B".to_string(),
                first: String::new(),
                last: "B".to_string(),
                member_id: None,
                nationality: None,
            }],
            entry1_id: 0,
            entry2_id: 0,
            court: Some(court.to_string()),
            court_id: None,
            sets,
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::OnCourt,
            finished_at: None,
        }
    }

    fn snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            matches,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
        }
    }

    #[test]
    fn first_snapshot_is_always_full() {
        let current = snapshot(vec![match_on_court(1, "1", vec![(5, 3)])]);
        assert!(matches!(diff(None, &current, 1), Update::Full(_)));
    }

    #[test]
    fn unchanged_snapshot_sends_nothing() {
        let a = snapshot(vec![match_on_court(1, "1", vec![(5, 3)])]);
        let b = snapshot(vec![match_on_court(1, "1", vec![(5, 3)])]);
        assert_eq!(diff(Some(&a), &b, 1), Update::None);
    }

    #[test]
    fn single_score_change_yields_tupdate() {
        let a = snapshot(vec![match_on_court(1, "1", vec![(5, 3)])]);
        let b = snapshot(vec![match_on_court(1, "1", vec![(6, 3)])]);
        match diff(Some(&a), &b, 9) {
            Update::Single(msg) => {
                assert_eq!(msg.match_update.id, "btp_1");
                assert_eq!(msg.match_update.s, vec![[6, 3]]);
                assert_eq!(msg.rid, 9);
            }
            other => panic!("Single erwartet, war {other:?}"),
        }
    }

    #[test]
    fn court_reassignment_yields_full() {
        // Match 2 kommt neu auf einen Court → strukturelle Änderung.
        let a = snapshot(vec![match_on_court(1, "1", vec![(5, 3)])]);
        let b = snapshot(vec![
            match_on_court(1, "1", vec![(5, 3)]),
            match_on_court(2, "2", vec![(0, 0)]),
        ]);
        assert!(matches!(diff(Some(&a), &b, 1), Update::Full(_)));
    }

    #[test]
    fn two_score_changes_yield_full() {
        let a = snapshot(vec![
            match_on_court(1, "1", vec![(5, 3)]),
            match_on_court(2, "2", vec![(2, 2)]),
        ]);
        let b = snapshot(vec![
            match_on_court(1, "1", vec![(6, 3)]),
            match_on_court(2, "2", vec![(2, 3)]),
        ]);
        assert!(matches!(diff(Some(&a), &b, 1), Update::Full(_)));
    }
}
