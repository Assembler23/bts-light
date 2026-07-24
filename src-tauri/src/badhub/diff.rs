//! Snapshot-Diff: entscheidet, ob ein voller `tset` oder ein kleines
//! `tupdate_match` an Badhub gesendet werden muss – oder nichts.
//!
//! Ein `tupdate_match` (~80 B) ist nur zulässig, wenn die Court-Belegung
//! unverändert ist und sich der Punktestand genau eines laufenden Matches
//! geändert hat. Jede strukturelle Änderung löst einen vollen `tset` aus.

use std::collections::BTreeMap;

use crate::badhub::payload::{
    build_tset, build_tupdate, CheckinRosterMessage, TsetMessage, TupdateMessage,
};
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

    // Geänderte „in Vorbereitung"-Aufrufe → voller tset. Ohne diesen Check
    // würde ein neuer/zurückgenommener Aufruf erst beim nächsten Heartbeat
    // (bis zu 60 s) gesendet.
    if preparation_calls(prev) != preparation_calls(current) {
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

/// Entscheidet, ob die Meldeliste (Hallen-Check-In) gesendet werden muss.
///
/// Gibt die Nachricht nur zurück, wenn sie sich inhaltlich von der zuletzt
/// gesendeten unterscheidet. Ohne diesen Filter gingen mehrere hundert Namen
/// im 5-Sekunden-Poll-Takt über die Leitung, obwohl sich eine Meldeliste an
/// einem Turniertag kaum ändert.
///
/// Anders als beim `tset` gibt es hier **keinen Heartbeat**: die Meldeliste
/// ist Stammdaten, badhub hält sie dauerhaft. Erst eine echte Änderung
/// (Nachmeldung, Abmeldung, korrigierter Name, neue Klasse) löst einen Push
/// aus.
pub fn roster_update(
    prev: Option<&CheckinRosterMessage>,
    current: CheckinRosterMessage,
) -> Option<CheckinRosterMessage> {
    match prev {
        Some(p) if p.same_content_as(&current) => None,
        _ => Some(current),
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

/// Fingerabdruck der „in Vorbereitung"-Aufrufe: Match-ID → (Aufruf-Zeit,
/// Halle), nur für tatsächlich gerufene Matches. Ändert er sich, muss ein
/// voller tset gesendet werden, damit der Aufruf sofort beim Monitor
/// ankommt. Die Halle gehört dazu, damit auch ein reiner Hallen-Wechsel
/// (gleiche Aufruf-Zeit) erkannt wird.
fn preparation_calls(snapshot: &BtpSnapshot) -> BTreeMap<i64, (u64, Option<&str>)> {
    snapshot
        .matches
        .iter()
        .filter_map(|m| {
            m.preparation_call_ts
                .map(|ts| (m.id, (ts, m.preparation_hall.as_deref())))
        })
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
            class_label: String::new(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: vec![BtpPlayer {
                id: 0,
                name: "A".to_string(),
                first: String::new(),
                last: "A".to_string(),
                member_id: None,
                nationality: None,
                club: None,
            }],
            team2: vec![BtpPlayer {
                id: 0,
                name: "B".to_string(),
                first: String::new(),
                last: "B".to_string(),
                member_id: None,
                nationality: None,
                club: None,
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
            preparation_call_ts: None,
            preparation_hall: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    fn snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            matches,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
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
    fn changed_preparation_call_yields_full() {
        // Ein eingeplantes Match, das in einem Zyklus gerufen wird, muss
        // sofort einen vollen tset auslösen – nicht erst beim Heartbeat.
        let mut uncalled = match_on_court(1, "1", vec![(0, 0)]);
        uncalled.status = MatchStatus::Scheduled;
        uncalled.court = None;
        let mut called = uncalled.clone();
        called.preparation_call_ts = Some(1_700_000_000_000);

        let before = snapshot(vec![uncalled.clone()]);
        let after = snapshot(vec![called]);
        // Aufruf neu gesetzt → voller tset.
        assert!(matches!(diff(Some(&before), &after, 1), Update::Full(_)));
        // Kein Aufruf in beiden Snapshots → nichts senden.
        assert!(matches!(
            diff(Some(&snapshot(vec![uncalled.clone()])), &before, 1),
            Update::None
        ));
        // Aufruf zurückgenommen (vorher gerufen, jetzt nicht mehr) → voller tset.
        let mut still_called = uncalled.clone();
        still_called.preparation_call_ts = Some(1_700_000_000_000);
        let a = snapshot(vec![still_called]);
        assert!(matches!(diff(Some(&a), &before, 1), Update::Full(_)));
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

    // --- Meldeliste (Hallen-Check-In) --------------------------------------

    fn roster_snapshot() -> BtpSnapshot {
        let player = |id: i64, first: &str, last: &str| BtpPlayer {
            id,
            name: format!("{first} {last}"),
            first: first.to_string(),
            last: last.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        };
        BtpSnapshot {
            tournament_name: "CP Open".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: Vec::new(),
            events: vec![crate::btp::model::BtpEvent {
                id: 1,
                name: "HE A".to_string(),
                discipline: Discipline::MensSingles,
            }],
            entries: vec![crate::btp::model::BtpEntry {
                id: 10,
                event_id: 1,
                players: vec![player(1, "Anna", "Beispiel")],
            }],
        }
    }

    fn roster_of(snapshot: &BtpSnapshot, rid: u64) -> CheckinRosterMessage {
        crate::badhub::payload::build_checkin_roster(snapshot, "GUID-1", rid)
    }

    #[test]
    fn roster_is_sent_when_nothing_was_sent_before() {
        let snapshot = roster_snapshot();
        assert!(roster_update(None, roster_of(&snapshot, 1)).is_some());
    }

    #[test]
    fn unchanged_roster_is_not_sent_again() {
        // Der Poll laeuft alle 5 s — ohne diesen Filter gingen mehrere hundert
        // Namen im Poll-Takt ueber die Leitung.
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);
        assert_eq!(roster_update(Some(&sent), roster_of(&snapshot, 2)), None);
    }

    #[test]
    fn a_new_entry_triggers_a_push() {
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);

        let mut with_extra = roster_snapshot();
        with_extra.entries.push(crate::btp::model::BtpEntry {
            id: 11,
            event_id: 1,
            players: vec![BtpPlayer {
                id: 2,
                name: "Bea Muster".to_string(),
                first: "Bea".to_string(),
                last: "Muster".to_string(),
                member_id: None,
                nationality: None,
                club: None,
            }],
        });
        assert!(roster_update(Some(&sent), roster_of(&with_extra, 2)).is_some());
    }

    #[test]
    fn a_withdrawn_entry_triggers_a_push() {
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);

        let mut without = roster_snapshot();
        without.entries.clear();
        assert!(roster_update(Some(&sent), roster_of(&without, 2)).is_some());
    }

    #[test]
    fn a_corrected_player_name_triggers_a_push() {
        // Tippfehler-Korrektur in BTP muss auf der Check-In-Seite ankommen,
        // sonst findet der Spieler seinen Namen nicht.
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);

        let mut renamed = roster_snapshot();
        renamed.entries[0].players[0].last = "Beispiel-Meier".to_string();
        assert!(roster_update(Some(&sent), roster_of(&renamed, 2)).is_some());
    }

    #[test]
    fn a_renamed_class_triggers_a_push() {
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);

        let mut renamed = roster_snapshot();
        renamed.events[0].name = "HE Anfaenger".to_string();
        assert!(roster_update(Some(&sent), roster_of(&renamed, 2)).is_some());
    }

    #[test]
    fn a_late_licence_number_triggers_a_push() {
        // Wird die Lizenznummer nachgepflegt, braucht badhub sie fuers
        // Anonymisierungs-Gate.
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);

        let mut with_licence = roster_snapshot();
        with_licence.entries[0].players[0].member_id = Some("08-012002".to_string());
        assert!(roster_update(Some(&sent), roster_of(&with_licence, 2)).is_some());
    }

    #[test]
    fn switching_the_tournament_triggers_a_push() {
        let snapshot = roster_snapshot();
        let sent = roster_of(&snapshot, 1);
        let other = crate::badhub::payload::build_checkin_roster(&snapshot, "GUID-2", 2);
        assert!(roster_update(Some(&sent), other).is_some());
    }
}
