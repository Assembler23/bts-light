//! Übersetzt einen `BtpSnapshot` in das `tset`-Payload-Format von badhub.de.
//!
//! Das Schema ist wire-kompatibel zum bestehenden Empfänger
//! `live_update.php` (Badhub-Repo, `docs/features/liveticker_bts.md`).
//!
//! Dieser Builder erzeugt den Kern-`tset`: Turniername, belegte Courts und
//! die zugehörigen laufenden Matches. Optionale Badhub-Felder
//! (`recent_finished_matches`, `upcoming_matches`, Logo) werden vom
//! Empfänger toleriert und später ergänzt.

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpSnapshot, MatchStatus};

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
    pub matches: Vec<TsetMatch>,
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
}

/// Stabile, turnierweit eindeutige Match-ID für den Badhub-Payload.
fn match_id(btp_match_id: i64) -> String {
    format!("btp_{btp_match_id}")
}

fn to_tset_match(m: &BtpMatch) -> TsetMatch {
    let name = format!("{} {}", m.draw_name, m.round_name);
    TsetMatch {
        id: match_id(m.id),
        n: name.trim().to_string(),
        s: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
        p0: m.team1.iter().map(|p| p.name.clone()).collect(),
        p0_member_ids: m.team1.iter().map(|p| p.member_id.clone()).collect(),
        p0_nationalities: m.team1.iter().map(|p| p.nationality.clone()).collect(),
        p1: m.team2.iter().map(|p| p.name.clone()).collect(),
        p1_member_ids: m.team2.iter().map(|p| p.member_id.clone()).collect(),
        p1_nationalities: m.team2.iter().map(|p| p.nationality.clone()).collect(),
    }
}

/// Baut die `tset`-Nachricht aus einem Snapshot.
///
/// `event.matches` und `event.courts` umfassen die aktuell auf einem Court
/// laufenden Matches – der Empfänger verknüpft beide über die Match-ID.
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
                match_id: match_id(m.id),
            })
        })
        .collect();
    let matches = on_court.iter().map(|m| to_tset_match(m)).collect();

    TsetMessage {
        kind: "tset",
        event: TsetEvent {
            tournament_name: snapshot.tournament_name.clone(),
            courts,
            matches,
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
            team1: vec![player("Anna Müller", Some("08-001234"), Some("GER"))],
            team2: vec![player("Ben Schmidt", None, None)],
            court: court.map(String::from),
            sets: vec![(21, 19), (21, 15)],
            winner: None,
            status,
        }
    }

    #[test]
    fn tset_contains_only_on_court_matches() {
        let snapshot = BtpSnapshot {
            tournament_name: "Test-Turnier".to_string(),
            matches: vec![
                sample_match(1, MatchStatus::OnCourt, Some("Feld 9")),
                sample_match(2, MatchStatus::Finished, None),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let tset = build_tset(&snapshot, 7);
        assert_eq!(tset.kind, "tset");
        assert_eq!(tset.rid, 7);
        assert_eq!(tset.event.tournament_name, "Test-Turnier");
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
        let m = &build_tset(&snapshot, 1).event.matches[0];
        assert_eq!(m.id, "btp_14");
        assert_eq!(m.n, "HE G1");
        assert_eq!(m.s, vec![[21, 19], [21, 15]]);
        assert_eq!(m.p0, ["Anna Müller"]);
        assert_eq!(m.p0_member_ids, [Some("08-001234".to_string())]);
        assert_eq!(m.p0_nationalities, [Some("GER".to_string())]);
        assert_eq!(m.p1, ["Ben Schmidt"]);
        assert_eq!(m.p1_member_ids, [None]);
    }

    #[test]
    fn serializes_to_expected_json_keys() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("1"))],
        };
        let json = serde_json::to_string(&build_tset(&snapshot, 42)).unwrap();
        assert!(json.contains(r#""type":"tset""#));
        assert!(json.contains(r#""_id":"btp_1""#));
        assert!(json.contains(r#""rid":42"#));
        // Fehlende MemberID erscheint als null.
        assert!(json.contains(r#""p1_member_ids":[null]"#));
    }
}
