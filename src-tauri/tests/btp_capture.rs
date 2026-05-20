//! Verifikation der BTP-Codecs gegen echte Wire-Mitschnitte.
//!
//! Die Fixtures stammen aus einem realen BTP (Test-Turnier "Test BTS Light"),
//! aufgezeichnet mit `tools/capture-btp.ps1`.

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
