//! Übersetzt einen `BtpSnapshot` in das `tset`-Payload-Format von badhub.de.
//!
//! Das Schema ist wire-kompatibel zum bestehenden Empfänger
//! `live_update.php` (Badhub-Repo, `docs/features/liveticker_bts.md`).
//!
//! Der `tset` umfasst Turniername, belegte Courts mit den laufenden
//! Matches, die zuletzt beendeten Matches und die anstehenden Matches.

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpSnapshot, MatchStatus};

/// Zeitfenster für „zuletzt beendet" – Matches älter als 4 h fallen raus.
const RECENT_FINISHED_WINDOW_MS: u64 = 4 * 60 * 60 * 1000;
/// Höchstzahl der „zuletzt beendet"-Einträge.
const RECENT_FINISHED_LIMIT: usize = 10;
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
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetCourt {
    /// Court-Bezeichnung wie in BTP (z. B. "1" oder "Feld 9").
    pub num: String,
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
    }
}

/// Konvertierung für ein beendetes Match (mit Ende-Zeit und Sieger).
fn to_finished_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        end_ts: m.finished_at,
        team1_won: m.winner.map(|w| w == 1),
        ..to_tset_match(m)
    }
}

/// Konvertierung für ein anstehendes Match (mit Spielnummer).
fn to_upcoming_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        match_num: m.match_num,
        ..to_tset_match(m)
    }
}

/// Beendete Matches der letzten 4 Stunden, neueste zuerst, max. 10.
fn recent_finished(snapshot: &BtpSnapshot, now_ms: u64) -> Vec<TsetMatch> {
    let mut finished: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Finished && m.winner.is_some())
        .filter(|m| match m.finished_at {
            Some(ts) => ts + RECENT_FINISHED_WINDOW_MS >= now_ms,
            None => false,
        })
        .collect();
    finished.sort_by_key(|m| std::cmp::Reverse(m.finished_at));
    finished.truncate(RECENT_FINISHED_LIMIT);
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
    // Nach Spielnummer; Matches ohne Nummer hinten anstellen.
    scheduled.sort_by_key(|m| m.match_num.unwrap_or(i64::MAX));
    scheduled.truncate(UPCOMING_LIMIT);
    scheduled.iter().map(|m| to_upcoming_match(m)).collect()
}

/// Baut die `tset`-Nachricht aus einem Snapshot.
pub fn build_tset(snapshot: &BtpSnapshot, rid: u64, now_ms: u64) -> TsetMessage {
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
            recent_finished_matches: recent_finished(snapshot, now_ms),
            upcoming_matches: upcoming(snapshot),
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
    use crate::btp::model::BtpPlayer;

    /// Fester Bezugszeitpunkt für die Tests.
    const NOW: u64 = 1_700_000_000_000;

    fn player(name: &str, member: Option<&str>, nat: Option<&str>) -> BtpPlayer {
        BtpPlayer {
            name: name.to_string(),
            member_id: member.map(String::from),
            nationality: nat.map(String::from),
        }
    }

    fn sample_match(id: i64, status: MatchStatus, court: Option<&str>) -> BtpMatch {
        BtpMatch {
            id,
            draw_name: "HE".to_string(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            team1: vec![player("Anna Müller", Some("08-001234"), Some("GER"))],
            team2: vec![player("Ben Schmidt", None, None)],
            court: court.map(String::from),
            sets: vec![(21, 19), (21, 15)],
            winner: None,
            status,
            finished_at: None,
        }
    }

    #[test]
    fn tset_matches_and_courts_cover_on_court_matches() {
        let snapshot = BtpSnapshot {
            tournament_name: "Test-Turnier".to_string(),
            matches: vec![
                sample_match(1, MatchStatus::OnCourt, Some("Feld 9")),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let tset = build_tset(&snapshot, 7, NOW);
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
            matches: vec![sample_match(14, MatchStatus::OnCourt, Some("1"))],
        };
        let m = &build_tset(&snapshot, 1, NOW).event.matches[0];
        assert_eq!(m.id, "btp_14");
        assert_eq!(m.n, "HE G1");
        assert_eq!(m.s, vec![[21, 19], [21, 15]]);
        assert_eq!(m.p0, ["Anna Müller"]);
        assert_eq!(m.p0_member_ids, [Some("08-001234".to_string())]);
        assert_eq!(m.p1, ["Ben Schmidt"]);
        assert_eq!(m.p1_member_ids, [None]);
    }

    #[test]
    fn recent_finished_keeps_recent_and_drops_old() {
        let mut fresh = sample_match(1, MatchStatus::Finished, None);
        fresh.winner = Some(1);
        fresh.finished_at = Some(NOW - 60_000); // vor 1 Minute
        let mut stale = sample_match(2, MatchStatus::Finished, None);
        stale.winner = Some(2);
        stale.finished_at = Some(NOW - 5 * 60 * 60 * 1000); // vor 5 Stunden

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            matches: vec![fresh, stale],
        };
        let event = build_tset(&snapshot, 1, NOW).event;
        assert_eq!(event.recent_finished_matches.len(), 1);
        let m = &event.recent_finished_matches[0];
        assert_eq!(m.id, "btp_1");
        assert_eq!(m.end_ts, Some(NOW - 60_000));
        assert_eq!(m.team1_won, Some(true));
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
            matches: vec![a, b],
        };
        let finished = build_tset(&snapshot, 1, NOW).event.recent_finished_matches;
        assert_eq!(finished[0].id, "btp_2");
        assert_eq!(finished[1].id, "btp_1");
    }

    #[test]
    fn upcoming_contains_scheduled_matches_with_num() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            matches: vec![
                sample_match(5, MatchStatus::Scheduled, None),
                sample_match(6, MatchStatus::OnCourt, Some("1")),
            ],
        };
        let upcoming = build_tset(&snapshot, 1, NOW).event.upcoming_matches;
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].id, "btp_5");
        assert_eq!(upcoming[0].match_num, Some(5));
    }

    #[test]
    fn serializes_to_expected_json_keys() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("1"))],
        };
        let json = serde_json::to_string(&build_tset(&snapshot, 42, NOW)).unwrap();
        assert!(json.contains(r#""type":"tset""#));
        assert!(json.contains(r#""recent_finished_matches":[]"#));
        assert!(json.contains(r#""upcoming_matches":[]"#));
        // Laufende Matches tragen keine Zusatzfelder.
        assert!(!json.contains("end_ts"));
        assert!(!json.contains("match_num"));
    }
}
