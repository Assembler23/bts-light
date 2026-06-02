//! BTP-Protokoll: Request-Builder und Auswertung der Antworten.
//!
//! Setzt auf den Codecs `wire` und `xml` auf. Siehe `docs/btp_protocol.md`.

use crate::btp::wire;
use crate::btp::xml::{self, Node, Value};

/// Client-Identifier, der in jedem Request mitgeschickt wird.
const CLIENT_ID: &str = "bts-light";

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("Wire-Fehler: {0}")]
    Wire(#[from] wire::WireError),
    #[error("XML-Fehler: {0}")]
    Xml(#[from] xml::XmlError),
    #[error("Antwort enthält kein <Action>-Element")]
    NoAction,
    #[error("unerwartete Login-Antwort (ID '{0}', erwartet 'REPLY')")]
    UnexpectedReply(String),
    #[error("Login abgelehnt – Passwort vermutlich falsch")]
    LoginRejected,
    #[error("Login-Antwort enthält keinen Session-Schlüssel")]
    NoSessionKey,
    #[error("BTP hat das Schreiben abgelehnt (Result != 1) – Netzwerk-Edits in BTP aktiv?")]
    UpdateRejected,
}

/// Baut die gemeinsamen Top-Level-Knoten aller Requests.
///
/// `session_key` ist nur bei schreibenden Requests (`SENDUPDATE`) gesetzt –
/// er stammt aus der LOGIN-Antwort und landet als `Unicode` in `Action`.
fn base_request(action: &str, password: Option<&str>, session_key: Option<&str>) -> Vec<Node> {
    let mut action_children = vec![Node::string("ID", action)];
    if let Some(key) = session_key {
        action_children.push(Node::string("Unicode", key));
    }
    if let Some(pw) = password {
        action_children.push(Node::string("Password", pw));
    }
    vec![
        Node::group(
            "Header",
            vec![Node::group(
                "Version",
                vec![Node::integer("Hi", 1), Node::integer("Lo", 1)],
            )],
        ),
        Node::group("Action", action_children),
        Node::group("Client", vec![Node::string("IP", CLIENT_ID)]),
    ]
}

/// Fertige Wire-Bytes für einen `LOGIN`-Request.
pub fn login_request(password: Option<&str>) -> Vec<u8> {
    wire::encode_message(&xml::encode(&base_request("LOGIN", password, None)))
}

/// Fertige Wire-Bytes für einen `SENDTOURNAMENTINFO`-Request.
pub fn tournament_info_request(password: Option<&str>) -> Vec<u8> {
    wire::encode_message(&xml::encode(&base_request(
        "SENDTOURNAMENTINFO",
        password,
        None,
    )))
}

/// Ein nach BTP zurückzuschreibendes Match-Ergebnis (Einzel-Draw, keine
/// Liga). Das Match wird über `btp_match_id` + `draw_id` + `planning_id`
/// eindeutig adressiert.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchUpdate {
    /// BTP-interne Match-ID (`Match.ID`).
    pub btp_match_id: i64,
    /// Draw, in dem das Match liegt (`Match.DrawID`).
    pub draw_id: i64,
    /// Planungsposition des Matches im Draw (`Match.PlanningID`).
    pub planning_id: i64,
    /// Satz-Ergebnisse als (Team1, Team2)-Punkte, in Spielreihenfolge.
    pub sets: Vec<(i64, i64)>,
    /// `true`, wenn Team 1 gewonnen hat (BTP `Winner` = 1, sonst 2).
    pub team1_won: bool,
    /// Spieldauer in Minuten; 0, falls unbekannt.
    pub duration_mins: i64,
    /// BTP `ScoreStatus`: 0 = regulär ausgespielt, 2 = Aufgabe (Retired).
    pub score_status: i64,
}

/// Fertige Wire-Bytes für einen `SENDUPDATE`-Request – schreibt ein
/// Ergebnis zurück nach BTP. `session_key` stammt aus der LOGIN-Antwort,
/// `password` nur, wenn das BTP-Turnier passwortgeschützt ist.
pub fn update_request(update: &MatchUpdate, session_key: &str, password: Option<&str>) -> Vec<u8> {
    let sets: Vec<Node> = update
        .sets
        .iter()
        .map(|&(t1, t2)| {
            Node::group(
                "Set",
                vec![Node::integer("T1", t1), Node::integer("T2", t2)],
            )
        })
        .collect();

    let match_node = Node::group(
        "Match",
        vec![
            Node::integer("ID", update.btp_match_id),
            Node::group("Sets", sets),
            Node::integer("Winner", if update.team1_won { 1 } else { 2 }),
            // ScoreStatus: 0 = regulär ausgespielt, 2 = Aufgabe (Retired).
            Node::integer("ScoreStatus", update.score_status),
            Node::integer("Duration", update.duration_mins),
            Node::integer("Status", 0),
            Node::integer("DrawID", update.draw_id),
            Node::integer("PlanningID", update.planning_id),
        ],
    );

    let mut nodes = base_request("SENDUPDATE", password, Some(session_key));
    nodes.push(Node::group(
        "Update",
        vec![Node::group(
            "Tournament",
            vec![Node::group("Matches", vec![match_node])],
        )],
    ));
    wire::encode_message(&xml::encode(&nodes))
}

/// Eine Feld-Zuweisung für BTP. `match_id = Some(id)` weist das Match dem
/// Feld zu, `None` gibt das Feld frei (BTS-Vorbild: ein `Court` ohne `MatchID`
/// im Courts-Block bedeutet „frei").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CourtAssignment {
    /// BTP-interne Court-ID (`Court.ID`).
    pub court_id: i64,
    /// Zugewiesenes Match (`Court.MatchID`); `None` = Feld freigeben.
    pub match_id: Option<i64>,
}

/// Setzt/löscht die Feldzuordnung AM MATCH selbst (`Match.CourtID`). `court_id`
/// = 0 löscht die Zuordnung (Halle + Feld verschwinden aus den BTP-Match-
/// Eigenschaften). Bewusst OHNE `Winner`/`Sets`/`ScoreStatus` – das ist ein
/// reines Feld-Update, kein Ergebnis (Vorbild BTS: Result-Felder nur wenn ein
/// Ergebnis vorliegt).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchCourt {
    pub match_id: i64,
    pub draw_id: i64,
    pub planning_id: i64,
    /// Neue Court-ID am Match; `0` = Zuordnung löschen.
    pub court_id: i64,
}

/// Fertige Wire-Bytes für einen `SENDUPDATE`, der **Feld-Zuweisungen** nach BTP
/// schreibt: einen `Courts`-Block (`Court{ID,[MatchID]}`, MatchID weglassen =
/// frei) UND optional einen `Matches`-Block, der `Match.CourtID` setzt/löscht.
/// Beides in einem Request (BTP akzeptiert `Courts` + `Matches` parallel). So
/// wird beim Freigeben nicht nur die Court-Verknüpfung gelöst, sondern auch
/// Halle+Feld am Match entfernt (`court_id = 0`). Nach Vorbild Original-BTS.
pub fn court_assign_request(
    courts: &[CourtAssignment],
    match_courts: &[MatchCourt],
    session_key: &str,
    password: Option<&str>,
) -> Vec<u8> {
    let mut tournament_children = Vec::new();

    if !courts.is_empty() {
        let court_nodes: Vec<Node> = courts
            .iter()
            .map(|a| {
                let mut children = vec![Node::integer("ID", a.court_id)];
                // MatchID nur setzen, wenn zugewiesen wird; weglassen = frei.
                if let Some(mid) = a.match_id {
                    children.push(Node::integer("MatchID", mid));
                }
                Node::group("Court", children)
            })
            .collect();
        tournament_children.push(Node::group("Courts", court_nodes));
    }

    if !match_courts.is_empty() {
        let match_nodes: Vec<Node> = match_courts
            .iter()
            .map(|mc| {
                Node::group(
                    "Match",
                    vec![
                        Node::integer("ID", mc.match_id),
                        Node::integer("Status", 0),
                        // 0 = Feldzuordnung am Match löschen.
                        Node::integer("CourtID", mc.court_id),
                        Node::integer("DrawID", mc.draw_id),
                        Node::integer("PlanningID", mc.planning_id),
                    ],
                )
            })
            .collect();
        tournament_children.push(Node::group("Matches", match_nodes));
    }

    let mut nodes = base_request("SENDUPDATE", password, Some(session_key));
    nodes.push(Node::group(
        "Update",
        vec![Node::group("Tournament", tournament_children)],
    ));
    wire::encode_message(&xml::encode(&nodes))
}

/// Dekodiert eine Wire-Antwort zu VISUALXML-Knoten.
pub fn decode_response(wire_bytes: &[u8]) -> Result<Vec<Node>, ProtoError> {
    let xml = wire::decode_message(wire_bytes)?;
    Ok(xml::decode(&xml)?)
}

/// Wertet eine `LOGIN`-Antwort aus und liefert den Session-Schlüssel.
pub fn parse_login_response(nodes: &[Node]) -> Result<String, ProtoError> {
    let action = xml::find(nodes, "Action").ok_or(ProtoError::NoAction)?;
    let children = action.children();

    let reply = xml::find(children, "ID")
        .and_then(Node::value)
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reply != "REPLY" {
        return Err(ProtoError::UnexpectedReply(reply.to_string()));
    }

    if xml::find(children, "Result")
        .and_then(Node::value)
        .and_then(Value::as_int)
        != Some(1)
    {
        return Err(ProtoError::LoginRejected);
    }

    let key = xml::find(children, "Unicode")
        .and_then(Node::value)
        .and_then(Value::as_str)
        .ok_or(ProtoError::NoSessionKey)?;
    Ok(key.to_string())
}

/// Wertet eine `SENDUPDATE`-Antwort aus. `Ok(())` nur bei `Result == 1`;
/// alles andere bedeutet, dass BTP das Schreiben nicht übernommen hat.
pub fn parse_update_response(nodes: &[Node]) -> Result<(), ProtoError> {
    let action = xml::find(nodes, "Action").ok_or(ProtoError::NoAction)?;
    let children = action.children();

    let reply = xml::find(children, "ID")
        .and_then(Node::value)
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reply != "REPLY" {
        return Err(ProtoError::UnexpectedReply(reply.to_string()));
    }

    if xml::find(children, "Result")
        .and_then(Node::value)
        .and_then(Value::as_int)
        != Some(1)
    {
        return Err(ProtoError::UpdateRejected);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Action-GROUP aus einer dekodierten Request-Wire-Nachricht ziehen.
    fn action_of(wire_bytes: &[u8]) -> Vec<Node> {
        let nodes = decode_response(wire_bytes).unwrap();
        xml::find(&nodes, "Action").unwrap().children().to_vec()
    }

    fn child_str<'a>(nodes: &'a [Node], id: &str) -> Option<&'a str> {
        xml::find(nodes, id)?.value()?.as_str()
    }

    #[test]
    fn login_request_carries_action_and_client() {
        let action = action_of(&login_request(None));
        assert_eq!(child_str(&action, "ID"), Some("LOGIN"));
        // Ohne Passwort darf kein Password-ITEM enthalten sein.
        assert!(xml::find(&action, "Password").is_none());
    }

    #[test]
    fn login_request_includes_password_when_set() {
        let action = action_of(&login_request(Some("geheim")));
        assert_eq!(child_str(&action, "Password"), Some("geheim"));
    }

    #[test]
    fn tournament_info_request_has_correct_action() {
        let action = action_of(&tournament_info_request(None));
        assert_eq!(child_str(&action, "ID"), Some("SENDTOURNAMENTINFO"));
    }

    #[test]
    fn request_carries_version_header() {
        let nodes = decode_response(&login_request(None)).unwrap();
        let version = xml::find(&nodes, "Header")
            .and_then(|h| xml::find(h.children(), "Version"))
            .unwrap();
        assert_eq!(
            xml::find(version.children(), "Hi").and_then(Node::value),
            Some(&Value::Integer(1))
        );
    }

    /// Baut eine LOGIN-Antwort als Knotenbaum.
    fn login_reply(id: &str, result: i64, unicode: Option<&str>) -> Vec<Node> {
        let mut children = vec![Node::string("ID", id), Node::integer("Result", result)];
        if let Some(u) = unicode {
            children.push(Node::string("Unicode", u));
        }
        vec![Node::group("Action", children)]
    }

    #[test]
    fn parse_login_success_returns_session_key() {
        let reply = login_reply("REPLY", 1, Some("SESSION-42"));
        assert_eq!(parse_login_response(&reply).unwrap(), "SESSION-42");
    }

    #[test]
    fn parse_login_wrong_password_is_rejected() {
        let reply = login_reply("REPLY", 0, None);
        assert!(matches!(
            parse_login_response(&reply),
            Err(ProtoError::LoginRejected)
        ));
    }

    #[test]
    fn parse_login_unexpected_reply_id() {
        let reply = login_reply("ERROR", 1, Some("x"));
        assert!(matches!(
            parse_login_response(&reply),
            Err(ProtoError::UnexpectedReply(id)) if id == "ERROR"
        ));
    }

    #[test]
    fn parse_login_missing_action() {
        assert!(matches!(
            parse_login_response(&[]),
            Err(ProtoError::NoAction)
        ));
    }

    #[test]
    fn parse_login_missing_session_key() {
        let reply = login_reply("REPLY", 1, None);
        assert!(matches!(
            parse_login_response(&reply),
            Err(ProtoError::NoSessionKey)
        ));
    }

    // --- SENDUPDATE -------------------------------------------------------

    fn child_int(nodes: &[Node], id: &str) -> Option<i64> {
        xml::find(nodes, id)?.value()?.as_int()
    }

    fn sample_update() -> MatchUpdate {
        MatchUpdate {
            btp_match_id: 42,
            draw_id: 7,
            planning_id: 1003,
            sets: vec![(21, 19), (21, 15)],
            team1_won: true,
            duration_mins: 28,
            score_status: 0,
        }
    }

    /// Kinder des `Match`-Knotens aus einer SENDUPDATE-Wire-Nachricht.
    fn match_node(wire_bytes: &[u8]) -> Vec<Node> {
        let nodes = decode_response(wire_bytes).unwrap();
        let update = xml::find(&nodes, "Update").unwrap();
        let tournament = xml::find(update.children(), "Tournament").unwrap();
        let matches = xml::find(tournament.children(), "Matches").unwrap();
        xml::find(matches.children(), "Match")
            .unwrap()
            .children()
            .to_vec()
    }

    #[test]
    fn update_request_uses_sendupdate_action_with_session_key() {
        let action = action_of(&update_request(&sample_update(), "SESSION-9", None));
        assert_eq!(child_str(&action, "ID"), Some("SENDUPDATE"));
        assert_eq!(child_str(&action, "Unicode"), Some("SESSION-9"));
        assert!(xml::find(&action, "Password").is_none());
    }

    #[test]
    fn update_request_includes_password_when_set() {
        let action = action_of(&update_request(&sample_update(), "S", Some("geheim")));
        assert_eq!(child_str(&action, "Password"), Some("geheim"));
    }

    #[test]
    fn update_request_encodes_match_identity_and_result() {
        let m = match_node(&update_request(&sample_update(), "S", None));
        assert_eq!(child_int(&m, "ID"), Some(42));
        assert_eq!(child_int(&m, "DrawID"), Some(7));
        assert_eq!(child_int(&m, "PlanningID"), Some(1003));
        assert_eq!(child_int(&m, "Winner"), Some(1));
        assert_eq!(child_int(&m, "Duration"), Some(28));
        assert_eq!(child_int(&m, "ScoreStatus"), Some(0));
        assert_eq!(child_int(&m, "Status"), Some(0));
    }

    #[test]
    fn update_request_winner_is_two_when_team1_lost() {
        let mut u = sample_update();
        u.team1_won = false;
        let m = match_node(&update_request(&u, "S", None));
        assert_eq!(child_int(&m, "Winner"), Some(2));
    }

    #[test]
    fn update_request_encodes_every_set_in_order() {
        let m = match_node(&update_request(&sample_update(), "S", None));
        let sets = xml::find(&m, "Sets").unwrap();
        let set_nodes = sets.children();
        assert_eq!(set_nodes.len(), 2);
        assert_eq!(child_int(set_nodes[0].children(), "T1"), Some(21));
        assert_eq!(child_int(set_nodes[0].children(), "T2"), Some(19));
        assert_eq!(child_int(set_nodes[1].children(), "T1"), Some(21));
        assert_eq!(child_int(set_nodes[1].children(), "T2"), Some(15));
    }

    /// Baut eine SENDUPDATE-Antwort als Knotenbaum.
    fn update_reply(id: &str, result: i64) -> Vec<Node> {
        vec![Node::group(
            "Action",
            vec![Node::string("ID", id), Node::integer("Result", result)],
        )]
    }

    #[test]
    fn parse_update_success_is_ok() {
        assert!(parse_update_response(&update_reply("REPLY", 1)).is_ok());
    }

    #[test]
    fn parse_update_rejected_when_result_not_one() {
        assert!(matches!(
            parse_update_response(&update_reply("REPLY", 0)),
            Err(ProtoError::UpdateRejected)
        ));
    }

    #[test]
    fn parse_update_unexpected_reply_id() {
        assert!(matches!(
            parse_update_response(&update_reply("ERROR", 1)),
            Err(ProtoError::UnexpectedReply(id)) if id == "ERROR"
        ));
    }

    // --- SENDUPDATE Courts (Feldvergabe) ----------------------------------

    /// Kinder des `Courts`-Knotens aus einer SENDUPDATE-Wire-Nachricht.
    fn courts_block(wire_bytes: &[u8]) -> Vec<Node> {
        let nodes = decode_response(wire_bytes).unwrap();
        let update = xml::find(&nodes, "Update").unwrap();
        let tournament = xml::find(update.children(), "Tournament").unwrap();
        xml::find(tournament.children(), "Courts")
            .unwrap()
            .children()
            .to_vec()
    }

    #[test]
    fn courts_update_uses_sendupdate_action_with_session_key() {
        let req = court_assign_request(
            &[CourtAssignment {
                court_id: 5,
                match_id: Some(42),
            }],
            &[],
            "SESSION-7",
            None,
        );
        let action = action_of(&req);
        assert_eq!(child_str(&action, "ID"), Some("SENDUPDATE"));
        assert_eq!(child_str(&action, "Unicode"), Some("SESSION-7"));
    }

    #[test]
    fn courts_update_assign_includes_matchid() {
        let req = court_assign_request(
            &[CourtAssignment {
                court_id: 5,
                match_id: Some(42),
            }],
            &[],
            "S",
            None,
        );
        let court = xml::find(&courts_block(&req), "Court")
            .unwrap()
            .children()
            .to_vec();
        assert_eq!(child_int(&court, "ID"), Some(5));
        assert_eq!(child_int(&court, "MatchID"), Some(42));
    }

    #[test]
    fn courts_update_free_omits_matchid() {
        // Freigeben: Court ohne MatchID (BTS-Vorbild).
        let req = court_assign_request(
            &[CourtAssignment {
                court_id: 7,
                match_id: None,
            }],
            &[],
            "S",
            None,
        );
        let court = xml::find(&courts_block(&req), "Court")
            .unwrap()
            .children()
            .to_vec();
        assert_eq!(child_int(&court, "ID"), Some(7));
        assert!(xml::find(&court, "MatchID").is_none());
    }

    #[test]
    fn court_assign_clears_match_courtid_without_result() {
        // Freigeben mit Match-Block: Court ohne MatchID + Match.CourtID=0,
        // OHNE Winner/Sets (kein Ergebnis schreiben!).
        let req = court_assign_request(
            &[CourtAssignment {
                court_id: 7,
                match_id: None,
            }],
            &[MatchCourt {
                match_id: 42,
                draw_id: 3,
                planning_id: 1002,
                court_id: 0,
            }],
            "S",
            None,
        );
        let m = match_node(&req);
        assert_eq!(child_int(&m, "ID"), Some(42));
        assert_eq!(child_int(&m, "CourtID"), Some(0));
        assert_eq!(child_int(&m, "DrawID"), Some(3));
        assert_eq!(child_int(&m, "PlanningID"), Some(1002));
        // Kein Ergebnis: weder Winner noch Sets dürfen mitgehen.
        assert!(xml::find(&m, "Winner").is_none());
        assert!(xml::find(&m, "Sets").is_none());
    }
}
