//! Verifikation der BTP-Codecs gegen echte Wire-Mitschnitte.
//!
//! Die Fixtures stammen aus einem realen BTP (Test-Turnier "Test BTS Light"),
//! aufgezeichnet mit `tools/capture-btp.ps1`.

use bts_light_lib::btp::model::{self, MatchStatus};
use bts_light_lib::btp::proto;
use bts_light_lib::btp::xml;

const LOGIN: &[u8] = include_bytes!("fixtures/btp-login.bin");
const TOURNAMENT: &[u8] = include_bytes!("fixtures/btp-tournament.bin");

#[test]
fn real_login_capture_yields_session_key() {
    let nodes = proto::decode_response(LOGIN).expect("Login-Antwort dekodierbar");
    let key = proto::parse_login_response(&nodes).expect("Login erfolgreich");
    assert_eq!(key, "202605200925493078");
}

#[test]
fn real_tournament_capture_decodes() {
    let nodes = proto::decode_response(TOURNAMENT).expect("Tournament-Antwort dekodierbar");
    let result = xml::find(&nodes, "Result").expect("Result-Gruppe");
    let tournament = xml::find(result.children(), "Tournament").expect("Tournament-Gruppe");

    // Turniername steht im Setting mit ID 1001.
    let settings = xml::find(tournament.children(), "Settings").expect("Settings-Gruppe");
    let name = settings.children().iter().find_map(|setting| {
        let id = xml::find(setting.children(), "ID")?.value()?.as_int()?;
        if id != 1001 {
            return None;
        }
        xml::find(setting.children(), "Value")?.value()?.as_str()
    });
    assert_eq!(name, Some("Test BTS Light"));

    // Das Test-Turnier hat fünf Spieler.
    let players = xml::find(tournament.children(), "Players").expect("Players-Gruppe");
    assert_eq!(players.children().len(), 5);
}

#[test]
fn real_tournament_capture_parses_to_snapshot() {
    let nodes = proto::decode_response(TOURNAMENT).expect("dekodierbar");
    let snapshot = model::parse_snapshot(&nodes).expect("Snapshot");

    assert_eq!(snapshot.tournament_name, "Test BTS Light");
    // 25 Match-Einträge im XML, davon 10 echte Paarungen (IsMatch=true).
    assert_eq!(snapshot.matches.len(), 10);

    let by_id = |id: i64| {
        snapshot
            .matches
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("Match {id} fehlt"))
    };
    let names = |team: &[model::BtpPlayer]| -> Vec<String> {
        team.iter().map(|p| p.name.clone()).collect()
    };

    // Beendetes Match mit Ergebnis: Bernd unterliegt Ulla 2:21, 5:21.
    let finished = by_id(19);
    assert_eq!(finished.status, MatchStatus::Finished);
    assert_eq!(names(&finished.team1), ["Bernd"]);
    assert_eq!(names(&finished.team2), ["Ulla"]);
    assert_eq!(finished.sets, [(2, 21), (5, 21)]);
    assert_eq!(finished.winner, Some(2));

    // Laufendes Match: Anne gegen Hilde auf Court "1".
    let on_court = by_id(22);
    assert_eq!(on_court.status, MatchStatus::OnCourt);
    assert_eq!(on_court.court.as_deref(), Some("1"));
    assert_eq!(names(&on_court.team1), ["Anne"]);
    assert_eq!(names(&on_court.team2), ["Hilde"]);

    // Gesamtverteilung der Zustände.
    let count = |s: MatchStatus| snapshot.matches.iter().filter(|m| m.status == s).count();
    assert_eq!(count(MatchStatus::Finished), 2);
    assert_eq!(count(MatchStatus::OnCourt), 2);
    assert_eq!(count(MatchStatus::Scheduled), 6);
}
