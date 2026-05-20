//! Parser: VISUALXML-Knotenbaum einer `SENDTOURNAMENTINFO`-Antwort →
//! `BtpSnapshot`.
//!
//! BTP speichert ein Round-Robin als Teilnehmer-Slots (Match-Einträge mit
//! `EntryID`) und Paarungen (Match-Einträge mit `From1`/`From2`, die auf die
//! `PlanningID` der Slots verweisen). Echte, anzeigbare Paarungen tragen
//! `IsMatch = true`. Siehe `docs/btp_protocol.md`.

use std::collections::HashMap;

use crate::btp::xml::{self, Node};

/// Spielzustand eines Matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchStatus {
    /// Eingeplant, aber weder auf einem Court noch beendet.
    Scheduled,
    /// Aktuell einem Court zugewiesen.
    OnCourt,
    /// Mit Sieger abgeschlossen.
    Finished,
}

/// Eine anzeigbare Paarung.
#[derive(Debug, Clone, PartialEq)]
pub struct BtpMatch {
    pub id: i64,
    /// Name der Auslosung, z. B. "HE".
    pub draw_name: String,
    /// Runden-/Spielbezeichnung, z. B. "G1".
    pub round_name: String,
    /// Spielernamen Team 1 (ein Name bei Einzel, zwei bei Doppel).
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Court-Name, falls dem Match ein Court zugewiesen ist.
    pub court: Option<String>,
    /// Satz-Ergebnisse als (Team1, Team2)-Punkte.
    pub sets: Vec<(i64, i64)>,
    /// Sieger: 1 oder 2, falls entschieden.
    pub winner: Option<u8>,
    pub status: MatchStatus,
}

/// Aufbereiteter Turnier-Stand aus einer `SENDTOURNAMENTINFO`-Antwort.
#[derive(Debug, Clone, PartialEq)]
pub struct BtpSnapshot {
    pub tournament_name: String,
    pub matches: Vec<BtpMatch>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Antwort enthält keine <Tournament>-Daten")]
    NoTournament,
}

/// Parst den Knotenbaum einer `SENDTOURNAMENTINFO`-Antwort.
pub fn parse_snapshot(nodes: &[Node]) -> Result<BtpSnapshot, ModelError> {
    let tournament = xml::find(nodes, "Result")
        .and_then(|r| xml::find(r.children(), "Tournament"))
        .ok_or(ModelError::NoTournament)?;
    let t = tournament.children();

    let players = player_map(t);
    let entries = entry_map(t);
    let slots = slot_map(t);
    let courts = court_map(t);
    let draws = draw_map(t);

    Ok(BtpSnapshot {
        tournament_name: setting_str(t, 1001).unwrap_or_default(),
        matches: parse_matches(t, &players, &entries, &slots, &courts, &draws),
    })
}

// --- Lookup-Tabellen ------------------------------------------------------

/// PlayerID → Anzeigename.
fn player_map(t: &[Node]) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    let Some(players) = xml::find(t, "Players") else {
        return map;
    };
    for p in players.children() {
        let Some(id) = child_int(p, "ID") else {
            continue;
        };
        let last = child_str(p, "Lastname").unwrap_or_default();
        let first = child_str(p, "Firstname").unwrap_or_default();
        let name = if first.is_empty() {
            last.to_string()
        } else {
            format!("{first} {last}")
        };
        map.insert(id, name);
    }
    map
}

/// EntryID → enthaltene PlayerIDs (eine bei Einzel, zwei bei Doppel).
fn entry_map(t: &[Node]) -> HashMap<i64, Vec<i64>> {
    let mut map = HashMap::new();
    let Some(entries) = xml::find(t, "Entries") else {
        return map;
    };
    for e in entries.children() {
        let Some(id) = child_int(e, "ID") else {
            continue;
        };
        let player_ids = ["Player1ID", "Player2ID"]
            .iter()
            .filter_map(|field| child_int(e, field))
            .collect();
        map.insert(id, player_ids);
    }
    map
}

/// PlanningID eines Teilnehmer-Slots → EntryID.
fn slot_map(t: &[Node]) -> HashMap<i64, i64> {
    let mut map = HashMap::new();
    let Some(matches) = xml::find(t, "Matches") else {
        return map;
    };
    for m in matches.children() {
        if let (Some(planning), Some(entry)) = (child_int(m, "PlanningID"), child_int(m, "EntryID"))
        {
            map.insert(planning, entry);
        }
    }
    map
}

/// CourtID → Court-Name.
fn court_map(t: &[Node]) -> HashMap<i64, String> {
    id_name_map(t, "Courts")
}

/// DrawID → Draw-Name.
fn draw_map(t: &[Node]) -> HashMap<i64, String> {
    id_name_map(t, "Draws")
}

/// Generische ID→Name-Tabelle für Container mit gleichförmigen Einträgen.
fn id_name_map(t: &[Node], container: &str) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    let Some(group) = xml::find(t, container) else {
        return map;
    };
    for node in group.children() {
        if let (Some(id), Some(name)) = (child_int(node, "ID"), child_str(node, "Name")) {
            map.insert(id, name.to_string());
        }
    }
    map
}

// --- Match-Parsing --------------------------------------------------------

fn parse_matches(
    t: &[Node],
    players: &HashMap<i64, String>,
    entries: &HashMap<i64, Vec<i64>>,
    slots: &HashMap<i64, i64>,
    courts: &HashMap<i64, String>,
    draws: &HashMap<i64, String>,
) -> Vec<BtpMatch> {
    let mut out = Vec::new();
    let Some(matches) = xml::find(t, "Matches") else {
        return out;
    };
    for m in matches.children() {
        // Nur echte, anzeigbare Paarungen – Slots und Spiegel-Einträge
        // tragen kein IsMatch=true.
        if child_bool(m, "IsMatch") != Some(true) {
            continue;
        }
        let resolve = |planning: Option<i64>| resolve_team(planning, slots, entries, players);
        let court = child_int(m, "CourtID").and_then(|id| courts.get(&id).cloned());
        let winner = child_int(m, "Winner").and_then(|w| u8::try_from(w).ok());
        let status = if winner.is_some() {
            MatchStatus::Finished
        } else if court.is_some() {
            MatchStatus::OnCourt
        } else {
            MatchStatus::Scheduled
        };
        out.push(BtpMatch {
            id: child_int(m, "ID").unwrap_or_default(),
            draw_name: child_int(m, "DrawID")
                .and_then(|id| draws.get(&id).cloned())
                .unwrap_or_default(),
            round_name: child_str(m, "RoundName").unwrap_or_default().to_string(),
            team1: resolve(child_int(m, "From1")),
            team2: resolve(child_int(m, "From2")),
            court,
            sets: parse_sets(m),
            winner,
            status,
        });
    }
    out
}

/// Löst die `From`-PlanningID über Slot → Entry → Player zu Spielernamen auf.
/// Leerer Vec, wenn die Kette nicht aufgeht (z. B. noch offener KO-Platz).
fn resolve_team(
    planning_id: Option<i64>,
    slots: &HashMap<i64, i64>,
    entries: &HashMap<i64, Vec<i64>>,
    players: &HashMap<i64, String>,
) -> Vec<String> {
    let Some(planning) = planning_id else {
        return Vec::new();
    };
    let Some(entry_id) = slots.get(&planning) else {
        return Vec::new();
    };
    let Some(player_ids) = entries.get(entry_id) else {
        return Vec::new();
    };
    player_ids
        .iter()
        .filter_map(|id| players.get(id).cloned())
        .collect()
}

/// Liest die Satz-Ergebnisse aus dem `Sets`-Container eines Matches.
fn parse_sets(m: &Node) -> Vec<(i64, i64)> {
    let Some(sets) = xml::find(m.children(), "Sets") else {
        return Vec::new();
    };
    sets.children()
        .iter()
        .filter_map(|set| Some((child_int(set, "T1")?, child_int(set, "T2")?)))
        .collect()
}

// --- Kleine Knoten-Zugriffshelfer -----------------------------------------

fn child_int(node: &Node, id: &str) -> Option<i64> {
    xml::find(node.children(), id)?.value()?.as_int()
}

fn child_str<'a>(node: &'a Node, id: &str) -> Option<&'a str> {
    xml::find(node.children(), id)?.value()?.as_str()
}

fn child_bool(node: &Node, id: &str) -> Option<bool> {
    xml::find(node.children(), id)?.value()?.as_bool()
}

/// Wert eines `Setting` mit gegebener numerischer ID.
fn setting_str(t: &[Node], setting_id: i64) -> Option<String> {
    let settings = xml::find(t, "Settings")?;
    settings.children().iter().find_map(|s| {
        if child_int(s, "ID") == Some(setting_id) {
            child_str(s, "Value").map(String::from)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::xml::{Node, Value};

    #[test]
    fn missing_tournament_is_an_error() {
        assert!(matches!(parse_snapshot(&[]), Err(ModelError::NoTournament)));
    }

    #[test]
    fn empty_tournament_yields_name_without_matches() {
        // Result > Tournament > Settings > Setting(1001 = Name).
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![Node::group(
                    "Settings",
                    vec![Node::group(
                        "Setting",
                        vec![
                            Node::integer("ID", 1001),
                            Node::string("Value", "Leeres Turnier"),
                        ],
                    )],
                )],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.tournament_name, "Leeres Turnier");
        assert!(snapshot.matches.is_empty());
    }

    #[test]
    fn match_status_derives_from_winner_and_court() {
        // Bool-Helfer, da xml::Node keinen Bool-Konstruktor hat.
        let is_match = || Node::Item {
            id: "IsMatch".to_string(),
            value: Value::Bool(true),
        };
        let finished = Node::group(
            "Match",
            vec![
                Node::integer("ID", 1),
                is_match(),
                Node::integer("Winner", 1),
            ],
        );
        let on_court = Node::group(
            "Match",
            vec![
                Node::integer("ID", 2),
                is_match(),
                Node::integer("CourtID", 9),
            ],
        );
        let scheduled = Node::group("Match", vec![Node::integer("ID", 3), is_match()]);
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "Courts",
                        vec![Node::group(
                            "Court",
                            vec![Node::integer("ID", 9), Node::string("Name", "Court 9")],
                        )],
                    ),
                    Node::group("Matches", vec![finished, on_court, scheduled]),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        let status: Vec<_> = snapshot.matches.iter().map(|m| &m.status).collect();
        assert_eq!(
            status,
            vec![
                &MatchStatus::Finished,
                &MatchStatus::OnCourt,
                &MatchStatus::Scheduled
            ]
        );
    }

    #[test]
    fn resolves_doubles_pairing_over_slot_and_entry() {
        // Doppel-Entry mit zwei Spielern; das Match verweist über From1 auf
        // den Teilnehmer-Slot (PlanningID 100 → Entry 10). From2 fehlt – das
        // Gegner-Team bleibt leer (wie bei einem Freilos).
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "Players",
                        vec![
                            Node::group(
                                "Player",
                                vec![
                                    Node::integer("ID", 1),
                                    Node::string("Lastname", "Müller"),
                                    Node::string("Firstname", "Anna"),
                                ],
                            ),
                            Node::group(
                                "Player",
                                vec![
                                    Node::integer("ID", 2),
                                    Node::string("Lastname", "Schmidt"),
                                    Node::string("Firstname", "Ben"),
                                ],
                            ),
                        ],
                    ),
                    Node::group(
                        "Entries",
                        vec![Node::group(
                            "Entry",
                            vec![
                                Node::integer("ID", 10),
                                Node::integer("Player1ID", 1),
                                Node::integer("Player2ID", 2),
                            ],
                        )],
                    ),
                    Node::group(
                        "Matches",
                        vec![
                            // Teilnehmer-Slot.
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("PlanningID", 100),
                                    Node::integer("EntryID", 10),
                                ],
                            ),
                            // Echtes Match, verweist per From1 auf den Slot.
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("ID", 5),
                                    Node::Item {
                                        id: "IsMatch".to_string(),
                                        value: Value::Bool(true),
                                    },
                                    Node::integer("From1", 100),
                                ],
                            ),
                        ],
                    ),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches.len(), 1);
        assert_eq!(snapshot.matches[0].team1, ["Anna Müller", "Ben Schmidt"]);
        assert!(snapshot.matches[0].team2.is_empty());
    }
}
