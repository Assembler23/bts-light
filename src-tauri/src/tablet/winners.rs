//! Sieger-/Podium-Ermittlung je Disziplin (Bewerb) aus dem BTP-Snapshot.
//!
//! Disziplin-Sieger = Sieger des K.o.-**Finales** des Bewerbs. Die Gruppenphase
//! ist nur Qualifikation und liefert keinen Disziplin-Champion — Gruppen-Draws
//! haben kein Finale und fallen daher automatisch raus. Platz 3:
//! - „Spiel um Platz 3" gespielt → ein 3. Platz (dessen Sieger);
//! - sonst → **beide Halbfinal-Verlierer** teilen sich Platz 3.
//!
//! BTP speichert keine fertige Platzierung; wir leiten sie aus den (gelesenen)
//! Match-Ergebnissen ab (Winner + RoundName) — wie es TP für seine
//! Siegerlisten intern auch tut.

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpSnapshot, Discipline, MatchResult, MatchStatus};

/// Ein platzierter Spieler/ein Team-Mitglied (mit Verein, falls bekannt).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WinnerPlayer {
    /// Kombinierter Anzeigename („Vorname Nachname") — Fallback fürs Frontend.
    pub name: String,
    /// Vorname(n) und Nachname GETRENNT (BTP `Firstname`/`Lastname`), damit der
    /// Monitor zweizeilig (Vorname/Nachname) rendern + Mittelnamen kürzen kann.
    /// Wichtig für mehrteilige Nachnamen (z. B. „Nguyen Duc").
    pub first: String,
    pub last: String,
    pub club: Option<String>,
}

/// Eine Platzierung (Rang) mit dem zugehörigen Team (1 Spieler im Einzel,
/// 2 im Doppel). Rang 3 kann zweimal vorkommen (zwei dritte Plätze).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placement {
    pub rank: u8,
    pub players: Vec<WinnerPlayer>,
    /// Platz kampflos entschieden (Walkover).
    pub walkover: bool,
}

/// Podium einer ausgespielten Disziplin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisciplineResult {
    /// Draw-ID (eindeutig je Bewerb) — stabiler Anker für die Rotation im
    /// Monitor; `draw_name` ist nicht zwingend eindeutig über Events hinweg.
    pub draw_id: i64,
    /// Bewerb-Name (= Name des K.o.-Draws, z. B. „JE U17 D").
    pub draw_name: String,
    pub discipline: Discipline,
    pub podium: Vec<Placement>,
    /// Zeitpunkt (Unix-ms), zu dem das Finale beendet wurde — für die Sortierung.
    pub finished_at: Option<u64>,
}

/// Normalisiert einen Rundennamen auf Kleinbuchstaben ohne Trenner/Leerzeichen
/// (z. B. „Spiel um Platz 3" → „spielumplatz3", „VF" → „vf").
fn norm(round_name: &str) -> String {
    round_name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_final(round_name: &str) -> bool {
    // Bewusst KEIN bare „f": ein Gruppen-/Slot-Bezeichner „F" würde sonst ein
    // Finale vortäuschen. BTP schreibt das Finale als „Finale"/„Final".
    let k = norm(round_name);
    k == "finale" || k == "final" || k.contains("endspiel")
}

fn is_third_place(round_name: &str) -> bool {
    let k = norm(round_name);
    k.contains("umplatz3")
        || k.contains("platz3")
        || k.contains("3platz")
        || k.contains("bronze")
        || k.contains("thirdplace")
        || k.contains("3rdplace")
        || k.starts_with("3rd")
}

fn is_semifinal(round_name: &str) -> bool {
    let k = norm(round_name);
    k.contains("halbfinale")
        || k.contains("semifinale")
        || k.contains("semifinal")
        || k == "hf"
        || k == "sf"
}

fn decided(m: &BtpMatch) -> bool {
    m.status == MatchStatus::Finished && m.winner.is_some()
}

/// Gewinner-Team (`winner==1` → team1) eines entschiedenen Matches.
fn winner_team(m: &BtpMatch) -> &[crate::btp::model::BtpPlayer] {
    if m.winner == Some(1) {
        &m.team1
    } else {
        &m.team2
    }
}

fn loser_team(m: &BtpMatch) -> &[crate::btp::model::BtpPlayer] {
    if m.winner == Some(1) {
        &m.team2
    } else {
        &m.team1
    }
}

fn placement(rank: u8, team: &[crate::btp::model::BtpPlayer], walkover: bool) -> Placement {
    Placement {
        rank,
        players: team
            .iter()
            .map(|p| WinnerPlayer {
                name: p.name.clone(),
                first: p.first.clone(),
                last: p.last.clone(),
                club: p.club.clone(),
            })
            .collect(),
        walkover,
    }
}

/// Podien aller Disziplinen, deren Finale entschieden ist (neueste zuerst).
pub fn discipline_results(snapshot: &BtpSnapshot) -> Vec<DisciplineResult> {
    use std::collections::HashMap;
    // Matches je Draw (= Bewerb-Einheit) sammeln.
    let mut by_draw: HashMap<i64, Vec<&BtpMatch>> = HashMap::new();
    for m in &snapshot.matches {
        by_draw.entry(m.draw_id).or_default().push(m);
    }

    let mut out = Vec::new();
    for ms in by_draw.values() {
        // Finale dieses Draws — fehlt es (z. B. Gruppen-Draw) → keine Disziplin.
        let Some(fm) = ms
            .iter()
            .copied()
            .find(|m| is_final(&m.round_name) && decided(m))
        else {
            continue;
        };

        let mut podium = vec![placement(
            1,
            winner_team(fm),
            fm.result == MatchResult::Walkover,
        )];
        // Platz 2 = Finalverlierer (sofern bekannt).
        if !loser_team(fm).is_empty() {
            podium.push(placement(2, loser_team(fm), false));
        }

        // Platz 3: Bronze-Match → ein 3.; sonst beide HF-Verlierer.
        if let Some(bm) = ms
            .iter()
            .copied()
            .find(|m| is_third_place(&m.round_name) && decided(m))
        {
            if !winner_team(bm).is_empty() {
                podium.push(placement(
                    3,
                    winner_team(bm),
                    bm.result == MatchResult::Walkover,
                ));
            }
        } else {
            for sm in ms
                .iter()
                .copied()
                .filter(|m| is_semifinal(&m.round_name) && decided(m))
            {
                if !loser_team(sm).is_empty() {
                    podium.push(placement(3, loser_team(sm), false));
                }
            }
        }

        out.push(DisciplineResult {
            draw_id: fm.draw_id,
            draw_name: fm.draw_name.clone(),
            discipline: fm.discipline,
            podium,
            finished_at: fm.finished_at,
        });
    }

    // Neueste Disziplinen zuerst; bei gleichem/fehlendem `finished_at` (BTP
    // liefert es derzeit gar nicht → immer None) STABIL nach draw_id, sonst
    // leakte die zufällige HashMap-Reihenfolge und die Liste „wackelte" bei
    // jedem Poll.
    out.sort_by(|a, b| {
        b.finished_at
            .cmp(&a.finished_at)
            .then(a.draw_id.cmp(&b.draw_id))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpPlayer, ScoringFormat};

    fn p(name: &str, club: Option<&str>) -> BtpPlayer {
        BtpPlayer {
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: None,
            club: club.map(String::from),
        }
    }

    fn m(
        draw_id: i64,
        round: &str,
        winner: Option<u8>,
        t1: Vec<BtpPlayer>,
        t2: Vec<BtpPlayer>,
    ) -> BtpMatch {
        BtpMatch {
            id: 0,
            draw_id,
            planning_id: 0,
            draw_name: "JE U17 D".into(),
            discipline: Discipline::MensSingles,
            round_name: round.into(),
            match_num: None,
            planned_time: None,
            team1: t1,
            team2: t2,
            entry1_id: 0,
            entry2_id: 0,
            court: None,
            court_id: None,
            sets: vec![],
            winner,
            result: MatchResult::Normal,
            status: if winner.is_some() {
                MatchStatus::Finished
            } else {
                MatchStatus::Scheduled
            },
            finished_at: winner.map(|_| 1),
            preparation_call_ts: None,
            preparation_hall: None,
            scoring: ScoringFormat::default(),
        }
    }

    fn snap(matches: Vec<BtpMatch>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches,
            courts: vec![],
            locations: vec![],
            court_infos: vec![],
        }
    }

    fn ranks(r: &DisciplineResult) -> Vec<u8> {
        r.podium.iter().map(|x| x.rank).collect()
    }

    #[test]
    fn ko_singles_final_yields_first_and_second_with_club() {
        let res = discipline_results(&snap(vec![m(
            1,
            "Finale",
            Some(1),
            vec![p("Anne", Some("VfL Lichtenrade"))],
            vec![p("Bernd", Some("Berliner SC"))],
        )]));
        assert_eq!(res.len(), 1);
        assert_eq!(ranks(&res[0]), vec![1, 2]);
        assert_eq!(res[0].podium[0].players[0].name, "Anne");
        assert_eq!(
            res[0].podium[0].players[0].club.as_deref(),
            Some("VfL Lichtenrade")
        );
        assert_eq!(res[0].podium[1].players[0].name, "Bernd");
    }

    #[test]
    fn doubles_final_lists_both_players() {
        let res = discipline_results(&snap(vec![m(
            1,
            "Finale",
            Some(2),
            vec![p("A1", None), p("A2", None)],
            vec![p("B1", Some("SC X")), p("B2", Some("SC Y"))],
        )]));
        // Sieger = Team 2 (B1/B2).
        assert_eq!(res[0].podium[0].rank, 1);
        assert_eq!(res[0].podium[0].players.len(), 2);
        assert_eq!(res[0].podium[0].players[1].club.as_deref(), Some("SC Y"));
    }

    #[test]
    fn bronze_match_gives_single_third() {
        let res = discipline_results(&snap(vec![
            m(
                1,
                "Finale",
                Some(1),
                vec![p("W", None)],
                vec![p("F2", None)],
            ),
            m(
                1,
                "Spiel um Platz 3",
                Some(1),
                vec![p("Bronze", None)],
                vec![p("Vierter", None)],
            ),
        ]));
        assert_eq!(ranks(&res[0]), vec![1, 2, 3]);
        assert_eq!(res[0].podium[2].players[0].name, "Bronze");
    }

    #[test]
    fn no_bronze_yields_two_thirds_from_semifinal_losers() {
        let res = discipline_results(&snap(vec![
            m(
                1,
                "Finale",
                Some(1),
                vec![p("W", None)],
                vec![p("F2", None)],
            ),
            m(
                1,
                "HF",
                Some(1),
                vec![p("SF1w", None)],
                vec![p("SF1l", None)],
            ),
            m(
                1,
                "Halbfinale",
                Some(2),
                vec![p("SF2l", None)],
                vec![p("SF2w", None)],
            ),
        ]));
        // Platz 1, 2, und ZWEI Platz 3 (beide HF-Verlierer).
        assert_eq!(ranks(&res[0]), vec![1, 2, 3, 3]);
        let thirds: Vec<&str> = res[0].podium[2..]
            .iter()
            .map(|x| x.players[0].name.as_str())
            .collect();
        assert!(thirds.contains(&"SF1l") && thirds.contains(&"SF2l"));
    }

    #[test]
    fn unfinished_final_is_not_a_result() {
        let res = discipline_results(&snap(vec![m(
            1,
            "Finale",
            None,
            vec![p("A", None)],
            vec![p("B", None)],
        )]));
        assert!(res.is_empty());
    }

    #[test]
    fn group_only_draw_is_ignored() {
        // Reine Gruppen-Matches (Runde "G1") ohne Finale → keine Disziplin.
        let res = discipline_results(&snap(vec![
            m(2, "G1", Some(1), vec![p("A", None)], vec![p("B", None)]),
            m(2, "G2", Some(2), vec![p("C", None)], vec![p("D", None)]),
        ]));
        assert!(res.is_empty());
    }

    #[test]
    fn walkover_final_flags_walkover() {
        let mut fm = m(1, "Finale", Some(1), vec![p("W", None)], vec![p("L", None)]);
        fm.result = MatchResult::Walkover;
        let res = discipline_results(&snap(vec![fm]));
        assert!(res[0].podium[0].walkover);
    }

    #[test]
    fn discipline_order_is_deterministic_by_draw_id() {
        // Mehrere Finals OHNE finished_at (BTP liefert es nie) → Reihenfolge muss
        // STABIL nach draw_id sein (nicht die zufällige HashMap-Reihenfolge), sonst
        // wackelt die Steuerliste bei jedem Poll.
        let build = || {
            [5_i64, 1, 9, 3, 7]
                .iter()
                .map(|&id| {
                    m(
                        id,
                        "Finale",
                        Some(1),
                        vec![p("W", None)],
                        vec![p("L", None)],
                    )
                })
                .collect::<Vec<_>>()
        };
        let ids: Vec<i64> = discipline_results(&snap(build()))
            .iter()
            .map(|d| d.draw_id)
            .collect();
        assert_eq!(ids, vec![1, 3, 5, 7, 9]);
        for _ in 0..5 {
            let again: Vec<i64> = discipline_results(&snap(build()))
                .iter()
                .map(|d| d.draw_id)
                .collect();
            assert_eq!(again, ids, "Reihenfolge muss über Aufrufe stabil sein");
        }
    }
}
