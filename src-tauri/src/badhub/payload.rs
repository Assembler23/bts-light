//! Übersetzt einen `BtpSnapshot` in das `tset`-Payload-Format von badhub.de.
//!
//! Das Schema ist wire-kompatibel zum bestehenden Empfänger
//! `live_update.php` (Badhub-Repo, `docs/features/liveticker_bts.md`).
//!
//! Der `tset` umfasst Turniername, belegte Courts mit den laufenden
//! Matches, die zuletzt beendeten Matches und die anstehenden Matches.

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpSnapshot, MatchResult, MatchStatus};

/// Höchstzahl der beendeten Matches im `tset`. Großzügig bemessen, damit an
/// einem Turniertag praktisch alle Spiele erscheinen; deckelt nur extrem
/// große Turniere, damit das Payload nicht unbegrenzt wächst.
const FINISHED_LIMIT: usize = 500;
/// Höchstzahl der „in Vorbereitung"-Einträge.
const UPCOMING_LIMIT: usize = 15;

/// Eine `tset`-Nachricht für `live_update.php`.
#[derive(Debug, Serialize, PartialEq)]
pub struct TsetMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub event: TsetEvent,
    pub rid: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetEvent {
    pub tournament_name: String,
    pub courts: Vec<TsetCourt>,
    /// Aktuell auf einem Court laufende Matches.
    pub matches: Vec<TsetMatch>,
    /// Zuletzt beendete Matches (neueste zuerst).
    pub recent_finished_matches: Vec<TsetMatch>,
    /// Anstehende Matches (in Vorbereitung).
    pub upcoming_matches: Vec<TsetMatch>,
    /// Turnierlogo (Base64, ohne `data:`-Präfix) für badhubs `#live-logo`.
    /// Wird in `sync` aus der Config injiziert; bei leerem Logo NICHT gesendet
    /// (badhub blendet das Element dann aus). Gleiche Feldnamen wie Original-BTS.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tournament_logo: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tournament_logo_mime: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tournament_logo_background_color: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetCourt {
    /// Court-Bezeichnung wie in BTP (z. B. "1" oder "Feld 9").
    pub num: String,
    /// Halle/Standort des Felds (BTP-`Location`-Name). Leer bei
    /// Ein-Hallen-Turnieren – der Liveticker-Monitor gruppiert erst, wenn
    /// die Halle gesetzt ist.
    pub hall: String,
    /// Verweist auf `TsetMatch._id`.
    pub match_id: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetMatch {
    #[serde(rename = "_id")]
    pub id: String,
    /// Anzeigename, z. B. "HE G1".
    pub n: String,
    /// Satz-Ergebnisse als `[Team1, Team2]`-Punktepaare.
    pub s: Vec<[i64; 2]>,
    pub p0: Vec<String>,
    pub p0_member_ids: Vec<Option<String>>,
    pub p0_nationalities: Vec<Option<String>>,
    pub p1: Vec<String>,
    pub p1_member_ids: Vec<Option<String>>,
    pub p1_nationalities: Vec<Option<String>>,
    /// Ende-Zeitstempel (nur bei beendeten Matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ts: Option<u64>,
    /// Hat Team 1 gewonnen? (nur bei beendeten Matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team1_won: Option<bool>,
    /// Spielnummer (nur bei anstehenden Matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_num: Option<i64>,
    /// Nicht-regulärer Ausgang: "walkover" | "retired" | "disqualified".
    /// Fehlt bei regulär ausgespielten Matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<&'static str>,
    /// Zeitpunkt (Unix-Millisekunden), zu dem die Turnierleitung das Match
    /// „in Vorbereitung" gerufen hat (nur bei anstehenden Matches). Der
    /// `display=next`-Monitor zeigt damit „vor X Min aufgerufen". Der
    /// Wire-Feldname `preparation_call_ts` wird von badhub.de wörtlich
    /// gelesen – nicht umbenennen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_call_ts: Option<u64>,
    /// Halle, für die der Aufruf gilt (nur bei hallenweise gerufenen
    /// Matches). Fehlt bei hallenunabhängigen Aufrufen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hall: Option<String>,
}

/// Stabile, turnierweit eindeutige Match-ID für den Badhub-Payload.
fn match_id(btp_match_id: i64) -> String {
    format!("btp_{btp_match_id}")
}

/// Basis-Konvertierung – ohne die zustandsabhängigen Zusatzfelder.
fn to_tset_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        id: match_id(m.id),
        n: format!("{} {}", m.draw_name, m.round_name)
            .trim()
            .to_string(),
        s: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
        p0: m.team1.iter().map(|p| p.name.clone()).collect(),
        p0_member_ids: m.team1.iter().map(|p| p.member_id.clone()).collect(),
        p0_nationalities: m.team1.iter().map(|p| p.nationality.clone()).collect(),
        p1: m.team2.iter().map(|p| p.name.clone()).collect(),
        p1_member_ids: m.team2.iter().map(|p| p.member_id.clone()).collect(),
        p1_nationalities: m.team2.iter().map(|p| p.nationality.clone()).collect(),
        end_ts: None,
        team1_won: None,
        match_num: None,
        outcome: None,
        preparation_call_ts: None,
        hall: None,
    }
}

/// Payload-Wert für die Ergebnisart; `None` bei regulärem Ausgang.
fn outcome_str(result: MatchResult) -> Option<&'static str> {
    match result {
        MatchResult::Normal => None,
        MatchResult::Walkover => Some("walkover"),
        MatchResult::Retired => Some("retired"),
        MatchResult::Disqualified => Some("disqualified"),
    }
}

/// Konvertierung für ein beendetes Match (mit Ende-Zeit, Sieger, Ausgang).
fn to_finished_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        end_ts: m.finished_at,
        team1_won: m.winner.map(|w| w == 1),
        outcome: outcome_str(m.result),
        ..to_tset_match(m)
    }
}

/// Konvertierung für ein anstehendes Match (mit Spielnummer und – falls
/// von der Turnierleitung gerufen – Vorbereitungs-Zeitstempel und Halle).
fn to_upcoming_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        match_num: m.match_num,
        preparation_call_ts: m.preparation_call_ts,
        hall: m.preparation_hall.clone(),
        ..to_tset_match(m)
    }
}

/// Alle beendeten Matches des laufenden Turniertags, neueste zuerst.
///
/// `finished_at` wird ausschließlich während des laufenden bts-light-Betriebs
/// gesetzt – die Liste umfasst damit alle an diesem Tag gespielten Matches.
/// `FINISHED_LIMIT` greift nur als Schutz bei extrem großen Turnieren.
fn recent_finished(snapshot: &BtpSnapshot) -> Vec<TsetMatch> {
    let mut finished: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Finished && m.winner.is_some())
        .filter(|m| m.finished_at.is_some())
        .collect();
    finished.sort_by_key(|m| std::cmp::Reverse(m.finished_at));
    finished.truncate(FINISHED_LIMIT);
    finished.iter().map(|m| to_finished_match(m)).collect()
}

/// Anstehende Matches (geplant, noch nicht auf Court, mit Spielern), max. 15.
fn upcoming(snapshot: &BtpSnapshot) -> Vec<TsetMatch> {
    let mut scheduled: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Scheduled)
        .filter(|m| !m.team1.is_empty() || !m.team2.is_empty())
        .collect();
    // Gerufene Matches („in Vorbereitung") zuerst, damit ein Aufruf nie aus
    // dem UPCOMING_LIMIT fällt; danach nach Spielnummer (ohne Nummer hinten).
    // Ist nichts gerufen, ist `is_some()` überall false und die Sortierung
    // degeneriert exakt zur bisherigen Spielnummern-Reihenfolge.
    scheduled.sort_by_key(|m| {
        (
            m.preparation_call_ts.is_none(),
            m.match_num.unwrap_or(i64::MAX),
        )
    });
    scheduled.truncate(UPCOMING_LIMIT);
    scheduled.iter().map(|m| to_upcoming_match(m)).collect()
}

/// Baut die `tset`-Nachricht aus einem Snapshot.
pub fn build_tset(snapshot: &BtpSnapshot, rid: u64) -> TsetMessage {
    let on_court: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::OnCourt)
        .collect();

    let courts = on_court
        .iter()
        .filter_map(|m| {
            m.court.as_ref().map(|c| TsetCourt {
                num: c.clone(),
                // Halle des Felds für den Liveticker-Hallen-Monitor; bei
                // Ein-Hallen-Turnieren leer.
                hall: m
                    .court_id
                    .map(|id| snapshot.court_location_name(id))
                    .unwrap_or_default(),
                match_id: match_id(m.id),
            })
        })
        .collect();

    TsetMessage {
        kind: "tset",
        event: TsetEvent {
            tournament_name: snapshot.tournament_name.clone(),
            courts,
            matches: on_court.iter().map(|m| to_tset_match(m)).collect(),
            recent_finished_matches: recent_finished(snapshot),
            upcoming_matches: upcoming(snapshot),
            // Logo wird erst im Sync-Loop aus der Config gefüllt (build_tset
            // kennt die Config nicht) – hier leer lassen.
            tournament_logo: String::new(),
            tournament_logo_mime: String::new(),
            tournament_logo_background_color: String::new(),
        },
        rid,
    }
}

/// Eine kleine `tupdate_match`-Nachricht – nur Match-ID und Satzstand.
/// Wird gesendet, wenn sich ausschließlich der Punktestand geändert hat.
#[derive(Debug, Serialize, PartialEq)]
pub struct TupdateMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(rename = "match")]
    pub match_update: TupdateMatch,
    pub rid: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TupdateMatch {
    #[serde(rename = "_id")]
    pub id: String,
    pub s: Vec<[i64; 2]>,
}

/// Baut eine `tupdate_match`-Nachricht für ein Match mit geändertem Score.
pub fn build_tupdate(m: &BtpMatch, rid: u64) -> TupdateMessage {
    TupdateMessage {
        kind: "tupdate_match",
        match_update: TupdateMatch {
            id: match_id(m.id),
            s: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
        },
        rid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpCourt, BtpLocation, BtpPlayer, Discipline};

    /// Fester Bezugszeitpunkt für die Tests.
    const NOW: u64 = 1_700_000_000_000;

    fn player(name: &str, member: Option<&str>, nat: Option<&str>) -> BtpPlayer {
        BtpPlayer {
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: member.map(String::from),
            nationality: nat.map(String::from),
            club: None,
        }
    }

    fn sample_match(id: i64, status: MatchStatus, court: Option<&str>) -> BtpMatch {
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
            team1: vec![player("Anna Müller", Some("08-001234"), Some("GER"))],
            team2: vec![player("Ben Schmidt", None, None)],
            entry1_id: 0,
            entry2_id: 0,
            court: court.map(String::from),
            court_id: None,
            sets: vec![(21, 19), (21, 15)],
            winner: None,
            result: MatchResult::Normal,
            status,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    #[test]
    fn tset_matches_and_courts_cover_on_court_matches() {
        let snapshot = BtpSnapshot {
            tournament_name: "Test-Turnier".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![
                sample_match(1, MatchStatus::OnCourt, Some("Feld 9")),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let tset = build_tset(&snapshot, 7);
        assert_eq!(tset.kind, "tset");
        assert_eq!(tset.rid, 7);
        assert_eq!(tset.event.matches.len(), 1);
        assert_eq!(tset.event.courts.len(), 1);
        assert_eq!(tset.event.courts[0].num, "Feld 9");
        assert_eq!(tset.event.courts[0].match_id, "btp_1");
    }

    #[test]
    fn tset_match_maps_players_and_score() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![sample_match(14, MatchStatus::OnCourt, Some("1"))],
        };
        let m = &build_tset(&snapshot, 1).event.matches[0];
        assert_eq!(m.id, "btp_14");
        assert_eq!(m.n, "HE G1");
        assert_eq!(m.s, vec![[21, 19], [21, 15]]);
        assert_eq!(m.p0, ["Anna Müller"]);
        assert_eq!(m.p0_member_ids, [Some("08-001234".to_string())]);
        assert_eq!(m.p1, ["Ben Schmidt"]);
        assert_eq!(m.p1_member_ids, [None]);
    }

    #[test]
    fn recent_finished_keeps_all_matches_of_the_day() {
        // Kein Zeitfenster mehr: auch früh am Tag beendete Matches bleiben in
        // der Liste, solange bts-light läuft. Nur Matches ohne Zeitstempel
        // (noch nicht von der Sync-Engine erfasst) fallen raus.
        let mut early = sample_match(1, MatchStatus::Finished, None);
        early.winner = Some(1);
        early.finished_at = Some(NOW - 8 * 60 * 60 * 1000); // vor 8 Stunden
        let mut late = sample_match(2, MatchStatus::Finished, None);
        late.winner = Some(2);
        late.finished_at = Some(NOW - 60_000); // vor 1 Minute
        let mut unstamped = sample_match(3, MatchStatus::Finished, None);
        unstamped.winner = Some(1);
        unstamped.finished_at = None;

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![early, late, unstamped],
        };
        let finished = build_tset(&snapshot, 1).event.recent_finished_matches;
        // early + late bleiben, unstamped fällt raus; neueste zuerst.
        assert_eq!(finished.len(), 2);
        assert_eq!(finished[0].id, "btp_2");
        assert_eq!(finished[1].id, "btp_1");
        assert_eq!(finished[0].team1_won, Some(false));
        assert_eq!(finished[1].end_ts, Some(NOW - 8 * 60 * 60 * 1000));
    }

    #[test]
    fn recent_finished_sorted_newest_first() {
        let mut a = sample_match(1, MatchStatus::Finished, None);
        a.winner = Some(1);
        a.finished_at = Some(NOW - 600_000);
        let mut b = sample_match(2, MatchStatus::Finished, None);
        b.winner = Some(1);
        b.finished_at = Some(NOW - 60_000); // neuer

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![a, b],
        };
        let finished = build_tset(&snapshot, 1).event.recent_finished_matches;
        assert_eq!(finished[0].id, "btp_2");
        assert_eq!(finished[1].id, "btp_1");
    }

    #[test]
    fn upcoming_contains_scheduled_matches_with_num() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![
                sample_match(5, MatchStatus::Scheduled, None),
                sample_match(6, MatchStatus::OnCourt, Some("1")),
            ],
        };
        let upcoming = build_tset(&snapshot, 1).event.upcoming_matches;
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].id, "btp_5");
        assert_eq!(upcoming[0].match_num, Some(5));
    }

    #[test]
    fn upcoming_puts_called_matches_first_and_carries_hall() {
        // Match 9 hat eine kleinere Spielnummer, aber Match 5 ist gerufen –
        // der Aufruf muss trotz höherer Nummer vorne stehen.
        let mut called = sample_match(5, MatchStatus::Scheduled, None);
        called.match_num = Some(50);
        called.preparation_call_ts = Some(NOW - 120_000);
        called.preparation_hall = Some("Halle 2".to_string());
        let mut uncalled = sample_match(9, MatchStatus::Scheduled, None);
        uncalled.match_num = Some(9);

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![uncalled, called],
        };
        let upcoming = build_tset(&snapshot, 1).event.upcoming_matches;
        assert_eq!(upcoming.len(), 2);
        // Gerufenes Match zuerst, trotz höherer Spielnummer.
        assert_eq!(upcoming[0].id, "btp_5");
        assert_eq!(upcoming[0].preparation_call_ts, Some(NOW - 120_000));
        assert_eq!(upcoming[0].hall.as_deref(), Some("Halle 2"));
        // Nicht gerufenes Match dahinter, ohne Vorbereitungs-Felder.
        assert_eq!(upcoming[1].id, "btp_9");
        assert_eq!(upcoming[1].preparation_call_ts, None);
        assert_eq!(upcoming[1].hall, None);
    }

    #[test]
    fn upcoming_order_unchanged_when_nothing_is_called() {
        // Ohne Aufrufe degeneriert die Sortierung exakt zur Spielnummern-
        // Reihenfolge – das alte Verhalten bleibt unverändert.
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![
                sample_match(7, MatchStatus::Scheduled, None),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let upcoming = build_tset(&snapshot, 1).event.upcoming_matches;
        // sample_match setzt match_num = id → nach Nummer sortiert: 3, 7.
        assert_eq!(upcoming[0].id, "btp_3");
        assert_eq!(upcoming[1].id, "btp_7");
    }

    #[test]
    fn serializes_to_expected_json_keys() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("1"))],
        };
        let json = serde_json::to_string(&build_tset(&snapshot, 42)).unwrap();
        assert!(json.contains(r#""type":"tset""#));
        assert!(json.contains(r#""recent_finished_matches":[]"#));
        assert!(json.contains(r#""upcoming_matches":[]"#));
        // Laufende Matches tragen keine Zusatzfelder.
        assert!(!json.contains("end_ts"));
        assert!(!json.contains("match_num"));
    }

    #[test]
    fn finished_walkover_carries_outcome() {
        let mut walkover = sample_match(1, MatchStatus::Finished, None);
        walkover.winner = Some(1);
        walkover.result = MatchResult::Walkover;
        walkover.finished_at = Some(NOW - 60_000);
        let mut regular = sample_match(2, MatchStatus::Finished, None);
        regular.winner = Some(2);
        regular.finished_at = Some(NOW - 60_000);

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![walkover, regular],
        };
        let finished = build_tset(&snapshot, 1).event.recent_finished_matches;
        let by_id = |id: &str| finished.iter().find(|m| m.id == id).unwrap();
        assert_eq!(by_id("btp_1").outcome, Some("walkover"));
        assert_eq!(by_id("btp_2").outcome, None);
    }

    #[test]
    fn tset_court_carries_the_hall_for_multi_hall_tournaments() {
        // Mehr-Hallen-Turnier: der TsetCourt trägt die Halle des Felds,
        // aufgelöst über court_id → court_infos → locations.
        let mut m = sample_match(1, MatchStatus::OnCourt, Some("1"));
        m.court_id = Some(101);
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: vec![
                BtpLocation {
                    id: 1,
                    name: "Halle 1".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle 2".to_string(),
                },
            ],
            court_infos: vec![BtpCourt {
                id: 101,
                name: "1".to_string(),
                location_id: Some(2),
                sort_order: 1,
            }],
            matches: vec![m],
        };
        let tset = build_tset(&snapshot, 1);
        assert_eq!(tset.event.courts.len(), 1);
        assert_eq!(tset.event.courts[0].num, "1");
        assert_eq!(tset.event.courts[0].hall, "Halle 2");
    }

    #[test]
    fn tset_court_hall_is_empty_for_single_hall_tournaments() {
        // Ein-Hallen-Turnier (keine Locations): die Halle bleibt leer, der
        // Liveticker-Monitor zeigt dann wie bisher ein flaches Raster.
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("1"))],
        };
        let tset = build_tset(&snapshot, 1);
        assert_eq!(tset.event.courts[0].hall, "");
    }
}
