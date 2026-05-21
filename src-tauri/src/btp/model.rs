//! Parser: VISUALXML-Knotenbaum einer `SENDTOURNAMENTINFO`-Antwort →
//! `BtpSnapshot`.
//!
//! BTP speichert ein Round-Robin als Teilnehmer-Slots (Match-Einträge mit
//! `EntryID`) und Paarungen (Match-Einträge mit `From1`/`From2`, die auf die
//! `PlanningID` der Slots verweisen). Echte, anzeigbare Paarungen tragen
//! `IsMatch = true`. Siehe `docs/btp_protocol.md`.

use std::collections::HashMap;

use serde::Serialize;

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

/// Wie ein Match entschieden wurde (BTP-Feld `ScoreStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    /// Regulär ausgespielt.
    Normal,
    /// Kampfloser Sieg (Walkover) – kein Spiel stattgefunden.
    Walkover,
    /// Aufgabe während des Spiels.
    Retired,
    /// Disqualifikation.
    Disqualified,
}

impl MatchResult {
    /// Leitet das Ergebnis aus dem BTP-Feld `ScoreStatus` ab.
    fn from_score_status(score_status: i64) -> MatchResult {
        match score_status {
            1 => MatchResult::Walkover,
            2 => MatchResult::Retired,
            3 => MatchResult::Disqualified,
            _ => MatchResult::Normal,
        }
    }
}

/// Disziplin eines Matches, aus dem BTP-Event abgeleitet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Discipline {
    /// Herreneinzel.
    MensSingles,
    /// Dameneinzel.
    WomensSingles,
    /// Herrendoppel.
    MensDoubles,
    /// Damendoppel.
    WomensDoubles,
    /// Gemischtes Doppel.
    Mixed,
    /// Nicht bestimmbar (Event fehlt oder unbekannte IDs).
    #[default]
    Unknown,
}

impl Discipline {
    /// Leitet die Disziplin aus den BTP-Event-Feldern ab.
    /// `GameTypeID`: 1 = Einzel, 2 = Doppel. `GenderID`: 1 = Herren,
    /// 2 = Damen, 3 = Mixed.
    fn from_event(game_type_id: i64, gender_id: i64) -> Discipline {
        match (game_type_id, gender_id) {
            (_, 3) => Discipline::Mixed,
            (1, 1) => Discipline::MensSingles,
            (1, 2) => Discipline::WomensSingles,
            (2, 1) => Discipline::MensDoubles,
            (2, 2) => Discipline::WomensDoubles,
            _ => Discipline::Unknown,
        }
    }
}

/// Ein Spieler einer Paarung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtpPlayer {
    /// Anzeigename ("Vorname Nachname" bzw. nur Nachname).
    pub name: String,
    /// Lizenznummer (BTP `MemberID`, z. B. "08-010493"), falls vorhanden.
    pub member_id: Option<String>,
    /// Nationalität als ISO-Code (BTP `Country`, z. B. "GER"), falls vorhanden.
    pub nationality: Option<String>,
}

/// Eine anzeigbare Paarung.
#[derive(Debug, Clone, PartialEq)]
pub struct BtpMatch {
    pub id: i64,
    /// Draw-ID des Matches – zusammen mit `planning_id` adressiert es das
    /// Match beim Zurückschreiben nach BTP (`SENDUPDATE`).
    pub draw_id: i64,
    /// Planungsposition des Matches im Draw (`Match.PlanningID`).
    pub planning_id: i64,
    /// Name der Auslosung, z. B. "HE".
    pub draw_name: String,
    /// Disziplin des Matches (aus dem BTP-Event abgeleitet).
    pub discipline: Discipline,
    /// Runden-/Spielbezeichnung, z. B. "G1".
    pub round_name: String,
    /// Spielnummer (BTP `MatchNr`), falls vergeben.
    pub match_num: Option<i64>,
    /// Team 1 (ein Spieler bei Einzel, zwei bei Doppel).
    pub team1: Vec<BtpPlayer>,
    pub team2: Vec<BtpPlayer>,
    /// EntryID von Team 1 – identifiziert die Mannschaft draw-weit
    /// eindeutig (0, falls der Platz noch offen ist). Damit lassen sich
    /// nach einer Aufgabe die übrigen Spiele derselben Mannschaft finden.
    pub entry1_id: i64,
    /// EntryID von Team 2 (0, falls der Platz noch offen ist).
    pub entry2_id: i64,
    /// Court-Name, falls dem Match ein Court zugewiesen ist.
    pub court: Option<String>,
    /// Satz-Ergebnisse als (Team1, Team2)-Punkte.
    pub sets: Vec<(i64, i64)>,
    /// Sieger: 1 oder 2, falls entschieden.
    pub winner: Option<u8>,
    /// Art der Entscheidung (normal, Walkover, Aufgabe, Disqualifikation).
    pub result: MatchResult,
    pub status: MatchStatus,
    /// Zeitpunkt (Unix-Millisekunden), zu dem das Match erstmals als
    /// beendet erkannt wurde. BTP liefert keinen End-Zeitstempel – dieses
    /// Feld wird von der Sync-Engine gesetzt, nicht vom Parser.
    pub finished_at: Option<u64>,
}

/// Aufbereiteter Turnier-Stand aus einer `SENDTOURNAMENTINFO`-Antwort.
#[derive(Debug, Clone, PartialEq)]
pub struct BtpSnapshot {
    pub tournament_name: String,
    pub matches: Vec<BtpMatch>,
    /// Alle Court-Namen des Turniers (BTP-Reihenfolge), auch leere Courts –
    /// damit der Tablet-Server jedem Court eine Adresse zuordnen kann.
    pub courts: Vec<String>,
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
    let disciplines = draw_discipline_map(t);

    // Court-Namen nach CourtID sortiert – das ergibt die BTP-Anlegereihenfolge.
    let mut court_pairs: Vec<(&i64, &String)> = courts.iter().collect();
    court_pairs.sort_by_key(|(id, _)| **id);
    let court_names: Vec<String> = court_pairs.into_iter().map(|(_, n)| n.clone()).collect();

    Ok(BtpSnapshot {
        tournament_name: setting_str(t, 1001).unwrap_or_default(),
        matches: parse_matches(t, &players, &entries, &slots, &courts, &draws, &disciplines),
        courts: court_names,
    })
}

// --- Lookup-Tabellen ------------------------------------------------------

/// PlayerID → Spielerdaten.
fn player_map(t: &[Node]) -> HashMap<i64, BtpPlayer> {
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
        map.insert(
            id,
            BtpPlayer {
                name,
                member_id: child_str(p, "MemberID").map(String::from),
                nationality: child_str(p, "Country").map(String::from),
            },
        );
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

/// (DrawID, PlanningID) eines Teilnehmer-Slots → EntryID.
///
/// PlanningIDs sind nur INNERHALB eines Draws eindeutig – BTP vergibt z. B.
/// in jedem Draw die Slots 1000, 2000, 3000 … Ohne die DrawID im Schlüssel
/// überschreiben sich gleichnamige Slots verschiedener Draws gegenseitig und
/// Paarungen lösen zu fremden Spielern auf ("Hilde gegen Hilde").
fn slot_map(t: &[Node]) -> HashMap<(i64, i64), i64> {
    let mut map = HashMap::new();
    let Some(matches) = xml::find(t, "Matches") else {
        return map;
    };
    for m in matches.children() {
        if let (Some(draw), Some(planning), Some(entry)) = (
            child_int(m, "DrawID"),
            child_int(m, "PlanningID"),
            child_int(m, "EntryID"),
        ) {
            map.insert((draw, planning), entry);
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

/// DrawID → Disziplin. BTP führt die Disziplin am Event (`GameTypeID` +
/// `GenderID`); jeder Draw verweist per `EventID` auf sein Event.
fn draw_discipline_map(t: &[Node]) -> HashMap<i64, Discipline> {
    // EventID → Disziplin.
    let mut events: HashMap<i64, Discipline> = HashMap::new();
    if let Some(group) = xml::find(t, "Events") {
        for e in group.children() {
            if let Some(id) = child_int(e, "ID") {
                let game = child_int(e, "GameTypeID").unwrap_or(0);
                let gender = child_int(e, "GenderID").unwrap_or(0);
                events.insert(id, Discipline::from_event(game, gender));
            }
        }
    }
    // DrawID → Disziplin (über die EventID des Draws).
    let mut map = HashMap::new();
    if let Some(group) = xml::find(t, "Draws") {
        for d in group.children() {
            if let (Some(draw_id), Some(event_id)) =
                (child_int(d, "ID"), child_int(d, "EventID"))
            {
                map.insert(
                    draw_id,
                    events
                        .get(&event_id)
                        .copied()
                        .unwrap_or(Discipline::Unknown),
                );
            }
        }
    }
    map
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

#[allow(clippy::too_many_arguments)]
fn parse_matches(
    t: &[Node],
    players: &HashMap<i64, BtpPlayer>,
    entries: &HashMap<i64, Vec<i64>>,
    slots: &HashMap<(i64, i64), i64>,
    courts: &HashMap<i64, String>,
    draws: &HashMap<i64, String>,
    disciplines: &HashMap<i64, Discipline>,
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
        // From1/From2 verweisen auf Slots im SELBEN Draw wie das Match.
        let draw_id = child_int(m, "DrawID");
        let resolve =
            |planning: Option<i64>| resolve_team(draw_id, planning, slots, entries, players);
        let court = child_int(m, "CourtID").and_then(|id| courts.get(&id).cloned());
        let winner = child_int(m, "Winner").and_then(|w| u8::try_from(w).ok());
        let status = if winner.is_some() {
            MatchStatus::Finished
        } else if court.is_some() {
            MatchStatus::OnCourt
        } else {
            MatchStatus::Scheduled
        };
        let (entry1_id, team1) = resolve(child_int(m, "From1"));
        let (entry2_id, team2) = resolve(child_int(m, "From2"));
        out.push(BtpMatch {
            id: child_int(m, "ID").unwrap_or_default(),
            draw_id: draw_id.unwrap_or_default(),
            planning_id: child_int(m, "PlanningID").unwrap_or_default(),
            draw_name: draw_id
                .and_then(|id| draws.get(&id).cloned())
                .unwrap_or_default(),
            discipline: draw_id
                .and_then(|id| disciplines.get(&id).copied())
                .unwrap_or(Discipline::Unknown),
            round_name: child_str(m, "RoundName").unwrap_or_default().to_string(),
            match_num: child_int(m, "MatchNr").filter(|&n| n > 0),
            team1,
            team2,
            entry1_id,
            entry2_id,
            court,
            sets: parse_sets(m),
            winner,
            result: MatchResult::from_score_status(child_int(m, "ScoreStatus").unwrap_or(0)),
            status,
            // BTP liefert keinen End-Zeitstempel; die Sync-Engine setzt das.
            finished_at: None,
        });
    }
    out
}

/// Löst die `From`-PlanningID über Slot → Entry → Player auf und liefert
/// `(EntryID, Spieler)`. Der Slot wird im Draw des Matches gesucht
/// (PlanningIDs sind nur dort eindeutig). `(0, [])`, wenn die Kette nicht
/// aufgeht (z. B. noch offener KO-Platz).
fn resolve_team(
    draw_id: Option<i64>,
    planning_id: Option<i64>,
    slots: &HashMap<(i64, i64), i64>,
    entries: &HashMap<i64, Vec<i64>>,
    players: &HashMap<i64, BtpPlayer>,
) -> (i64, Vec<BtpPlayer>) {
    let (Some(draw), Some(planning)) = (draw_id, planning_id) else {
        return (0, Vec::new());
    };
    let Some(entry_id) = slots.get(&(draw, planning)) else {
        return (0, Vec::new());
    };
    let Some(player_ids) = entries.get(entry_id) else {
        return (*entry_id, Vec::new());
    };
    let team = player_ids
        .iter()
        .filter_map(|id| players.get(id).cloned())
        .collect();
    (*entry_id, team)
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
                                    Node::string("MemberID", "08-001234"),
                                    Node::string("Country", "GER"),
                                ],
                            ),
                            // Spieler 2 ohne MemberID/Country.
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
                                    Node::integer("DrawID", 1),
                                    Node::integer("PlanningID", 100),
                                    Node::integer("EntryID", 10),
                                ],
                            ),
                            // Echtes Match, verweist per From1 auf den Slot.
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("ID", 5),
                                    Node::integer("DrawID", 1),
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
        let team1 = &snapshot.matches[0].team1;
        let names: Vec<&str> = team1.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Anna Müller", "Ben Schmidt"]);
        assert_eq!(team1[0].member_id.as_deref(), Some("08-001234"));
        assert_eq!(team1[0].nationality.as_deref(), Some("GER"));
        assert_eq!(team1[1].member_id, None);
        assert!(snapshot.matches[0].team2.is_empty());
        // EntryID von Team 1 ist aufgelöst, Team 2 bleibt offen (0).
        assert_eq!(snapshot.matches[0].entry1_id, 10);
        assert_eq!(snapshot.matches[0].entry2_id, 0);
    }

    /// Regression: zwei Draws verwenden beide den Slot PlanningID 100. Ohne
    /// DrawID im Slot-Schlüssel lösen beide Matches zum selben Spieler auf
    /// ("Hilde gegen Hilde"). Jedes Match muss den Slot seines eigenen Draws
    /// treffen.
    #[test]
    fn slots_with_same_planning_id_in_different_draws_do_not_collide() {
        let is_match = || Node::Item {
            id: "IsMatch".to_string(),
            value: Value::Bool(true),
        };
        let player = |id, name| {
            Node::group(
                "Player",
                vec![Node::integer("ID", id), Node::string("Lastname", name)],
            )
        };
        let entry = |id, player_id| {
            Node::group(
                "Entry",
                vec![
                    Node::integer("ID", id),
                    Node::integer("Player1ID", player_id),
                ],
            )
        };
        let slot = |draw, planning, entry_id| {
            Node::group(
                "Match",
                vec![
                    Node::integer("DrawID", draw),
                    Node::integer("PlanningID", planning),
                    Node::integer("EntryID", entry_id),
                ],
            )
        };
        let pairing = |id, draw, from| {
            Node::group(
                "Match",
                vec![
                    Node::integer("ID", id),
                    Node::integer("DrawID", draw),
                    is_match(),
                    Node::integer("From1", from),
                ],
            )
        };
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group("Players", vec![player(1, "Anna"), player(2, "Hilde")]),
                    Node::group("Entries", vec![entry(10, 1), entry(20, 2)]),
                    Node::group(
                        "Matches",
                        vec![
                            // Beide Draws nutzen Slot-PlanningID 100.
                            slot(1, 100, 10),
                            slot(2, 100, 20),
                            pairing(5, 1, 100),
                            pairing(6, 2, 100),
                        ],
                    ),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        let team1_name = |i: usize| snapshot.matches[i].team1[0].name.as_str();
        assert_eq!(team1_name(0), "Anna");
        assert_eq!(team1_name(1), "Hilde");
    }

    #[test]
    fn discipline_resolves_over_draw_and_event() {
        // Match → Draw 1 → Event 7 (GameTypeID 2, GenderID 3) → Mixed.
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "Events",
                        vec![Node::group(
                            "Event",
                            vec![
                                Node::integer("ID", 7),
                                Node::integer("GameTypeID", 2),
                                Node::integer("GenderID", 3),
                            ],
                        )],
                    ),
                    Node::group(
                        "Draws",
                        vec![Node::group(
                            "Draw",
                            vec![
                                Node::integer("ID", 1),
                                Node::integer("EventID", 7),
                                Node::string("Name", "GD"),
                            ],
                        )],
                    ),
                    Node::group(
                        "Matches",
                        vec![Node::group(
                            "Match",
                            vec![
                                Node::integer("ID", 5),
                                Node::integer("DrawID", 1),
                                Node::Item {
                                    id: "IsMatch".to_string(),
                                    value: Value::Bool(true),
                                },
                            ],
                        )],
                    ),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches[0].discipline, Discipline::Mixed);
    }

    #[test]
    fn discipline_unknown_when_event_missing() {
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![Node::group(
                    "Matches",
                    vec![Node::group(
                        "Match",
                        vec![
                            Node::integer("ID", 5),
                            Node::integer("DrawID", 1),
                            Node::Item {
                                id: "IsMatch".to_string(),
                                value: Value::Bool(true),
                            },
                        ],
                    )],
                )],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches[0].discipline, Discipline::Unknown);
    }
}
