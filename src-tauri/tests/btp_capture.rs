//! Verifikation der BTP-Codecs gegen echte Wire-Mitschnitte.
//!
//! Die Fixtures stammen aus einem realen BTP (Test-Turnier "Test BTS Light"),
//! aufgezeichnet mit `tools/capture-btp.ps1`.

use bts_light_lib::btp::model::{self, Discipline, MatchStatus};
use bts_light_lib::btp::proto;
use bts_light_lib::btp::xml;

const LOGIN: &[u8] = include_bytes!("fixtures/btp-login.bin");
const TOURNAMENT: &[u8] = include_bytes!("fixtures/btp-tournament.bin");
/// Echter Zwei-Hallen-Mitschnitt: 11 Felder, Hallen „Halle 1" / „Halle 2",
/// Feldnamen wiederholen sich über die Hallen.
const TOURNAMENT_2HALLS: &[u8] = include_bytes!("fixtures/btp-tournament-2halls.bin");

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

    // Disziplin aus dem BTP-Event abgeleitet: das Test-Turnier ist ein
    // Herreneinzel (Event GameTypeID=1, GenderID=1).
    assert_eq!(on_court.discipline, Discipline::MensSingles);

    // Gesamtverteilung der Zustände.
    let count = |s: MatchStatus| snapshot.matches.iter().filter(|m| m.status == s).count();
    assert_eq!(count(MatchStatus::Finished), 2);
    assert_eq!(count(MatchStatus::OnCourt), 2);
    assert_eq!(count(MatchStatus::Scheduled), 6);
}

#[test]
fn single_hall_capture_has_one_location() {
    // Ein-Hallen-Turniere tragen genau eine Location ("Main Location") –
    // damit greift die Hallen-Trennung (erst ab zwei Locations) hier nicht.
    let nodes = proto::decode_response(TOURNAMENT).expect("dekodierbar");
    let snapshot = model::parse_snapshot(&nodes).expect("Snapshot");
    assert_eq!(snapshot.locations.len(), 1);
    assert_eq!(snapshot.locations[0].name, "Main Location");
    assert_eq!(snapshot.court_infos.len(), 4);
}

#[test]
fn two_hall_capture_parses_locations_and_courts() {
    // Mehr-Hallen-Turnier: BTP liefert die Standorte und je Feld eine
    // LocationID. Die Feldnamen wiederholen sich über die Hallen – nur
    // die CourtID ist eindeutig.
    let nodes = proto::decode_response(TOURNAMENT_2HALLS).expect("dekodierbar");
    let snapshot = model::parse_snapshot(&nodes).expect("Snapshot");

    // Zwei Hallen.
    assert_eq!(snapshot.locations.len(), 2);
    let location = |id: i64| {
        snapshot
            .locations
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.name.as_str())
    };
    assert_eq!(location(1), Some("Halle 1"));
    assert_eq!(location(4), Some("Halle 2"));

    // 11 Felder: 4 in Halle 1, 7 in Halle 2.
    assert_eq!(snapshot.court_infos.len(), 11);
    let in_hall = |loc: i64| {
        snapshot
            .court_infos
            .iter()
            .filter(|c| c.location_id == Some(loc))
            .count()
    };
    assert_eq!(in_hall(1), 4);
    assert_eq!(in_hall(4), 7);

    // „1" gibt es in beiden Hallen – Feldnamen sind NICHT eindeutig, die
    // CourtID schon. Das ist der Kern der Mehr-Hallen-Unterstützung.
    let ones: Vec<_> = snapshot
        .court_infos
        .iter()
        .filter(|c| c.name == "1")
        .collect();
    assert_eq!(ones.len(), 2);
    assert_ne!(ones[0].id, ones[1].id);

    // Felder liegen hallenweise gruppiert vor (erst Halle 1, dann Halle 2).
    let loc_order: Vec<_> = snapshot.court_infos.iter().map(|c| c.location_id).collect();
    assert_eq!(
        loc_order,
        vec![
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(4),
            Some(4),
            Some(4),
            Some(4),
            Some(4),
            Some(4),
            Some(4),
        ]
    );

    // Jedes einem Feld zugewiesene Match referenziert eine bekannte CourtID.
    let on_court = snapshot
        .matches
        .iter()
        .filter(|m| m.court_id.is_some())
        .count();
    assert!(
        on_court >= 1,
        "mind. ein Match sollte einem Feld zugewiesen sein"
    );
    for m in snapshot.matches.iter().filter(|m| m.court_id.is_some()) {
        let cid = m.court_id.unwrap();
        assert!(
            snapshot.court_infos.iter().any(|c| c.id == cid),
            "court_id {cid} muss zu einem bekannten Feld gehören"
        );
    }
}

/// Die Meldeliste steht **vor** der Auslosung bereit: Ein `Entry` trägt seine
/// `EventID` direkt und braucht dafür weder Draw noch Match. Das ist die
/// Grundlage des Hallen-Check-Ins (docs/features/spieler-check-in.md).
///
/// Der Mitschnitt hat 5 Spieler, die in **beiden** Klassen gemeldet sind —
/// zugleich der Beleg, dass ein Spieler in mehreren Klassen vorkommt.
#[test]
fn real_capture_yields_the_roster_per_event() {
    let nodes = proto::decode_response(TOURNAMENT_2HALLS).expect("Tournament-Antwort dekodierbar");
    let snapshot = model::parse_snapshot(&nodes).expect("Snapshot parsebar");

    // Zwei Klassen: „HE" (Herreneinzel) und „Test" (Mixed-Einzel).
    let mut events: Vec<(i64, &str)> = snapshot
        .events
        .iter()
        .map(|e| (e.id, e.name.as_str()))
        .collect();
    events.sort();
    assert_eq!(events, vec![(1, "HE"), (2, "Test")]);
    assert_eq!(
        snapshot
            .events
            .iter()
            .find(|e| e.id == 1)
            .unwrap()
            .discipline,
        Discipline::MensSingles
    );

    // 10 Meldungen, gleichmäßig auf beide Klassen verteilt.
    assert_eq!(snapshot.entries.len(), 10);
    for event_id in [1, 2] {
        assert_eq!(
            snapshot
                .entries
                .iter()
                .filter(|e| e.event_id == event_id)
                .count(),
            5,
            "Event {event_id} sollte 5 Meldungen haben"
        );
    }

    // Jede Meldung zeigt auf eine bekannte Klasse und hat aufgelöste Spieler.
    for entry in &snapshot.entries {
        assert!(
            snapshot.events.iter().any(|e| e.id == entry.event_id),
            "Meldung {} zeigt auf unbekanntes Event {}",
            entry.id,
            entry.event_id
        );
        assert!(
            !entry.players.is_empty(),
            "Meldung {} hat keine Spieler",
            entry.id
        );
        for p in &entry.players {
            assert!(
                !p.name.trim().is_empty(),
                "Spielername darf nicht leer sein"
            );
        }
    }

    // Einzel-Turnier: genau ein Spieler je Meldung.
    assert!(snapshot.entries.iter().all(|e| e.players.len() == 1));

    // Derselbe Spieler ist in beiden Klassen gemeldet.
    let in_first: Vec<i64> = snapshot
        .entries
        .iter()
        .filter(|e| e.event_id == 1)
        .flat_map(|e| e.players.iter().map(|p| p.id))
        .collect();
    let in_second: Vec<i64> = snapshot
        .entries
        .iter()
        .filter(|e| e.event_id == 2)
        .flat_map(|e| e.players.iter().map(|p| p.id))
        .collect();
    assert!(
        in_first.iter().any(|id| in_second.contains(id)),
        "mind. ein Spieler sollte in beiden Klassen gemeldet sein"
    );
}

/// Die Baumkante: Ein KO-Spiel verweist über `From1`/`From2` auf die
/// Planungspositionen, aus denen seine Teilnehmer kommen. Ohne sie lässt
/// sich nicht beantworten, ob eine Ergebnis-Korrektur ein Folgespiel
/// beträfe — und ohne diese Antwort darf die Turnierleitung nicht
/// überschreiben (siehe docs/btp_protocol.md, „Was macht BTP beim
/// Überschreiben einer Wertung?").
#[test]
fn real_capture_carries_the_bracket_edges() {
    let nodes = proto::decode_response(TOURNAMENT).expect("dekodierbar");
    let snapshot = model::parse_snapshot(&nodes).expect("Snapshot");

    // Mindestens ein Spiel im Mitschnitt hat einen Vorgänger — sonst wäre
    // der Test wertlos.
    let mit_kante = snapshot
        .matches
        .iter()
        .filter(|m| m.from1.is_some() || m.from2.is_some())
        .count();
    assert!(mit_kante > 0, "kein einziges Spiel mit Baumkante");

    // Die Kante zeigt auf eine **Planungsposition**, nicht zwingend auf ein
    // Spiel: In der ersten Runde ist es ein Setzplatz, und Setzplätze
    // verwirft der Parser (sie sind keine anzeigbaren Paarungen).
    //
    // **Was dieser Mitschnitt NICHT hergibt:** Er stammt aus einer reinen
    // Round-Robin-Gruppe — alle Kanten zeigen auf Setzplätze (1000…5000),
    // kein Spiel folgt auf ein anderes. Die Nachfolger-Suche der
    // Ergebnis-Korrektur lässt sich damit **nicht** gegen echte Daten
    // belegen. Ein Mitschnitt mit mehrstufigem KO-Draw fehlt und gehört zum
    // BTP-Experiment (docs/btp_protocol.md). Bis dahin hält dieser Test den
    // Ist-Zustand fest, statt eine Deckung vorzutäuschen.
    let mit_nachfolger = snapshot
        .matches
        .iter()
        .filter(|m| {
            snapshot.matches.iter().any(|o| {
                o.draw_id == m.draw_id
                    && (o.from1 == Some(m.planning_id) || o.from2 == Some(m.planning_id))
            })
        })
        .count();
    assert_eq!(
        mit_nachfolger, 0,
        "Der Mitschnitt hat jetzt einen KO-Baum — dann gehört die \
         Nachfolger-Suche hier gegen echte Daten geprüft."
    );
}

/// **Ein Turnier muss den Spielort nicht pflegen — dann kommt keiner an.**
///
/// Diese beiden Mitschnitte sind genau solche Turniere: kein einziges Match
/// trägt eine `LocationID`, die Halle hängt allein am Feld. Deshalb war hier
/// zunächst notiert, es *gebe* keinen Spielort an der Ansetzung.
///
/// **Das war zu weit geschlossen.** Am 09.08.2026 an einem Turnier gemessen,
/// das die Spalte pflegt: 48 Matches mit `Match.LocationID`, die meisten
/// ohne jede Feldzuweisung. bts-light liest das Feld seither
/// (`assign::hall_for_match`, Quelle `HallSource::Btp`).
///
/// Der Test hält deshalb nur noch fest, was diese Fixtures zeigen: Es gibt
/// Turniere ohne gepflegten Spielort, und für die muss die abgeleitete
/// Kaskade (Regel → Hand → Aufruf) bestehen bleiben.
#[test]
fn a_tournament_may_carry_no_planned_venue_at_all() {
    for (name, raw) in [
        ("Ein-Hallen-Mitschnitt", TOURNAMENT),
        ("Zwei-Hallen-Mitschnitt", TOURNAMENT_2HALLS),
    ] {
        let nodes = proto::decode_response(raw).expect("dekodierbar");
        let snapshot = model::parse_snapshot(&nodes).expect("Snapshot");
        // Ein Spiel ohne Feld hat auch keine Halle — es gibt schlicht kein
        // Feld, über das sie sich ableiten ließe.
        let ohne_feld = snapshot
            .matches
            .iter()
            .filter(|m| m.court_id.is_none())
            .count();
        assert!(
            ohne_feld > 0,
            "{name}: kein einziges Spiel ohne Feld — Fixture prüfen"
        );
        // In genau diesen Turnieren ist die Spalte ungepflegt.
        assert_eq!(
            snapshot
                .matches
                .iter()
                .filter(|m| m.location_id.is_some())
                .count(),
            0,
            "{name}: dieses Fixture soll ein Turnier OHNE gepflegten Spielort zeigen"
        );
        // Und die Felder tragen ihre Halle, nicht die Spiele.
        if !snapshot.locations.is_empty() {
            assert!(
                snapshot.court_infos.iter().any(|c| c.location_id.is_some()),
                "{name}: Hallen vorhanden, aber kein Feld hat eine"
            );
        }
    }
}
