//! Geteilte JSON-Wire-Typen für den digitalen Tablet-Spielzettel.
//!
//! Zwei Verbindungs-Ebenen nutzen diese Typen:
//!
//! 1. **Tablet ↔ Server** ([`TabletMsg`], [`ServerMsg`], [`ResultBody`],
//!    [`ResultResponse`]). „Server" ist im LAN-Modus der eingebettete
//!    Server in bts-light, im Cloud-Modus der Relay. Die Wire-Form ist in
//!    beiden Fällen identisch – das Tablet (`tablet.html`) merkt keinen
//!    Unterschied.
//! 2. **bts-light-Host ↔ Relay** ([`HostFrame`], [`RelayFrame`]). Der
//!    Relay multiplext mehrere Tablets über eine einzige Host-Verbindung,
//!    deshalb trägt hier jedes Frame ein `courtLabel`.
//!
//! Beim Verändern der Renames aufpassen: `tablet.html` und der
//! verifizierte LAN-Pfad hängen exakt an dieser Wire-Form.

use serde::{Deserialize, Serialize};

// ─────────────────────────── Gemeinsame Bausteine ─────────────────────────

/// Ein Satz-Ergebnis als Punkte (Team A, Team B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAb {
    pub a: i64,
    pub b: i64,
}

/// Ein Spieler einer Paarung, wie ihn das Tablet anzeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerBrief {
    pub id: i64,
    pub name: String,
}

/// Match-Kurzinfo fürs Tablet (Schema wie bei badhub-tournament).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchBrief {
    #[serde(rename = "matchId")]
    pub match_id: i64,
    #[serde(rename = "teamA")]
    pub team_a: Vec<PlayerBrief>,
    #[serde(rename = "teamB")]
    pub team_b: Vec<PlayerBrief>,
    #[serde(rename = "eventLabel")]
    pub event_label: String,
    #[serde(rename = "bestOfSets")]
    pub best_of_sets: i64,
    #[serde(rename = "targetScore")]
    pub target_score: i64,
}

// ─────────────────────────── Tablet ↔ Server ──────────────────────────────

/// Nachrichten vom Tablet an den Server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TabletMsg {
    /// Erstes Frame: das Tablet bindet sich an seinen Court.
    #[serde(rename = "identify")]
    Identify {
        #[serde(rename = "courtLabel")]
        court_label: String,
    },
    /// Laufender Punktestand des aktuellen Satzes plus die schon
    /// abgeschlossenen Sätze.
    #[serde(rename = "score_update")]
    ScoreUpdate {
        #[serde(rename = "scoreA")]
        score_a: i64,
        #[serde(rename = "scoreB")]
        score_b: i64,
        #[serde(rename = "setsHistory", default)]
        sets_history: Vec<SetAb>,
    },
    /// Akkustand des Tablets (nur Android/Chrome – iPads liefern ihn nicht).
    #[serde(rename = "battery")]
    Battery { percent: i64, charging: bool },
}

/// Nachrichten vom Server an das Tablet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    /// BTP hat dem Court ein Match zugewiesen.
    #[serde(rename = "match_assigned")]
    MatchAssigned {
        #[serde(rename = "match")]
        match_brief: MatchBrief,
    },
    /// Der Court hat aktuell kein Match.
    #[serde(rename = "match_cleared")]
    MatchCleared,
}

/// Endergebnis-Body, den das Tablet per `POST …/result` schickt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultBody {
    #[serde(rename = "matchId")]
    pub match_id: i64,
    #[serde(rename = "courtLabel")]
    pub court_label: String,
    pub sets: Vec<SetAb>,
}

/// Antwort auf eine Ergebnis-Übermittlung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

impl ResultResponse {
    /// Erfolgsantwort.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    /// Fehlerantwort mit Meldung.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
    }
}

// ─────────────────────────── Host ↔ Relay ─────────────────────────────────

/// Frames von bts-light (dem „Host" eines Namespace) an den Relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostFrame {
    /// Court hat ein Match bekommen – an das zugehörige Tablet weiterleiten.
    MatchAssigned {
        #[serde(rename = "courtLabel")]
        court_label: String,
        #[serde(rename = "match")]
        match_brief: MatchBrief,
    },
    /// Court-Match aufgehoben.
    MatchCleared {
        #[serde(rename = "courtLabel")]
        court_label: String,
    },
    /// Antwort auf eine zuvor weitergeleitete Ergebnis-Übermittlung.
    ResultAck {
        #[serde(rename = "reqId")]
        req_id: u64,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
    },
}

/// Frames vom Relay an den bts-light-Host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayFrame {
    /// Ein Tablet hat sich für diesen Court verbunden.
    TabletConnected {
        #[serde(rename = "courtLabel")]
        court_label: String,
    },
    /// Das Tablet dieses Courts ist getrennt.
    TabletDisconnected {
        #[serde(rename = "courtLabel")]
        court_label: String,
    },
    /// Live-Punktestand von einem Tablet.
    ScoreUpdate {
        #[serde(rename = "courtLabel")]
        court_label: String,
        #[serde(rename = "scoreA")]
        score_a: i64,
        #[serde(rename = "scoreB")]
        score_b: i64,
        #[serde(rename = "setsHistory", default)]
        sets_history: Vec<SetAb>,
    },
    /// Endergebnis von einem Tablet – `req_id` korreliert die `ResultAck`.
    Result {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(rename = "courtLabel")]
        court_label: String,
        #[serde(rename = "matchId")]
        match_id: i64,
        sets: Vec<SetAb>,
    },
    /// Akkustand eines Tablets.
    Battery {
        #[serde(rename = "courtLabel")]
        court_label: String,
        percent: i64,
        charging: bool,
    },
}

// ─────────────────────────── Encoding-Helfer ──────────────────────────────

/// Minimaler Prozent-Encoder für einen URL-Pfad-Abschnitt (Court-Namen).
pub fn path_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Escapt HTML-Sonderzeichen inklusive `'`, weil der Court-Name in
/// `tablet.html` sowohl in HTML-Text als auch in einem JS-String-Literal
/// landet – ohne `'`-Escape könnte ein Apostroph das Literal aufbrechen.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serde-Roundtrip: deserialisieren, was wir serialisiert haben.
    fn roundtrip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    #[test]
    fn tablet_msg_identify_wire_form() {
        let json = r#"{"type":"identify","role":"tablet","courtLabel":"Feld 1"}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            TabletMsg::Identify {
                court_label: "Feld 1".to_string()
            }
        );
    }

    #[test]
    fn tablet_msg_score_update_ignores_extra_fields() {
        // tablet.html schickt zusätzlich currentSet/setsA/servingTeam – die
        // dürfen den Parser nicht stören.
        let json = r#"{"type":"score_update","courtLabel":"x","scoreA":21,"scoreB":19,
            "currentSet":2,"setsA":1,"setsB":0,"setsHistory":[{"a":21,"b":15}],"servingTeam":"a"}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            TabletMsg::ScoreUpdate {
                score_a: 21,
                score_b: 19,
                sets_history: vec![SetAb { a: 21, b: 15 }],
            }
        );
    }

    #[test]
    fn server_msg_match_assigned_uses_match_key() {
        let msg = ServerMsg::MatchAssigned {
            match_brief: MatchBrief {
                match_id: 7,
                team_a: vec![PlayerBrief {
                    id: 1,
                    name: "Anna".into(),
                }],
                team_b: vec![PlayerBrief {
                    id: 11,
                    name: "Ben".into(),
                }],
                event_label: "HE G1".into(),
                best_of_sets: 3,
                target_score: 21,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"match_assigned""#));
        assert!(json.contains(r#""match":{"#));
        roundtrip(&msg);
    }

    #[test]
    fn host_and_relay_frames_roundtrip() {
        roundtrip(&HostFrame::MatchCleared {
            court_label: "Feld 2".into(),
        });
        roundtrip(&HostFrame::ResultAck {
            req_id: 42,
            ok: false,
            error: Some("BTP abgelehnt".into()),
        });
        roundtrip(&RelayFrame::TabletConnected {
            court_label: "Feld 3".into(),
        });
        roundtrip(&RelayFrame::Result {
            req_id: 9,
            court_label: "Feld 1".into(),
            match_id: 18,
            sets: vec![SetAb { a: 21, b: 0 }, SetAb { a: 0, b: 21 }],
        });
    }

    #[test]
    fn result_response_omits_error_on_success() {
        let json = serde_json::to_string(&ResultResponse::ok()).unwrap();
        assert_eq!(json, r#"{"ok":true}"#);
        roundtrip(&ResultResponse::err("Zeitüberschreitung"));
    }

    #[test]
    fn path_encode_escapes_spaces_and_keeps_safe_chars() {
        assert_eq!(path_encode("Feld 1"), "Feld%201");
        assert_eq!(path_encode("Court-3"), "Court-3");
    }

    #[test]
    fn html_escape_neutralizes_markup_and_quotes() {
        assert_eq!(html_escape("a<b>&\"'c"), "a&lt;b&gt;&amp;&quot;&#39;c");
    }
}
