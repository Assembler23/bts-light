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
}

/// Baut die gemeinsamen Top-Level-Knoten aller Requests.
fn base_request(action: &str, password: Option<&str>) -> Vec<Node> {
    let mut action_children = vec![Node::string("ID", action)];
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
    wire::encode_message(&xml::encode(&base_request("LOGIN", password)))
}

/// Fertige Wire-Bytes für einen `SENDTOURNAMENTINFO`-Request.
pub fn tournament_info_request(password: Option<&str>) -> Vec<u8> {
    wire::encode_message(&xml::encode(&base_request("SENDTOURNAMENTINFO", password)))
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
}
