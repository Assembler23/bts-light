//! **Messwerkzeug, kein Regressionstest.** Zieht die Meldeliste des
//! Hallen-Check-Ins (`centry_list`, ADR 0009) aus einem **laufenden** BTP und
//! zeigt, was bts-light an badhub senden würde.
//!
//! Hintergrund: Die badhub-Gegenstelle (Schnitt B) steht noch nicht —
//! `live_update.php` kennt nur `tset`/`tupdate_match` und antwortet auf
//! `centry_list` mit HTTP 400. Ein Ende-zu-Ende-Test ist deshalb heute nicht
//! möglich. Prüfbar ist aber das, was den Check-In inhaltlich trägt: ob BTP
//! `Events` und `Entries` liefert, ob die Meldungen **vor der Auslosung** je
//! Klasse auflösen und ob Doppel-/Mixed-Meldungen ihre beiden Partner
//! mitbringen.
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test checkin_roster_probe -- --ignored --nocapture
//! ```
//!
//! Umgebungsvariablen: `BTP_HOST` (Standard 127.0.0.1), `BTP_PORT`
//! (Standard 9901), `BTP_PASSWORD`, `CHECKIN_GUID` (Turnier-GUID, nur für die
//! Formatprüfung), `SHOW_NAMES=1` (Namen mit ausgeben).
//!
//! **Ohne `SHOW_NAMES=1` enthält die Ausgabe keine Spielernamen** — nur
//! Strukturdaten und Zählungen. Ein Geburtsjahr enthält sie nie; der Test
//! prüft das ausdrücklich am fertigen JSON.

use std::collections::{HashMap, HashSet};

use bts_light_lib::badhub::payload::build_checkin_roster;
use bts_light_lib::btp::model::Discipline;
use bts_light_lib::btp::{client, model, proto, xml};

fn host() -> String {
    std::env::var("BTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn port() -> u16 {
    std::env::var("BTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9901)
}

fn password() -> Option<String> {
    std::env::var("BTP_PASSWORD").ok().filter(|p| !p.is_empty())
}

fn show_names() -> bool {
    std::env::var("SHOW_NAMES").is_ok_and(|v| v == "1")
}

/// Die Kinder des `Tournament`-Knotens — dieselbe Ebene, auf der
/// `parse_snapshot` arbeitet (`Result > Tournament > Events|Entries|…`).
fn tournament_children(nodes: &[xml::Node]) -> &[xml::Node] {
    xml::find(nodes, "Result")
        .and_then(|r| xml::find(r.children(), "Tournament"))
        .map_or(&[][..], |t| t.children())
}

/// Zählt die rohen Knoten einer Gruppe — die Bezugsgröße dafür, wie viele
/// Meldungen beim Auflösen verworfen wurden.
fn raw_count(t: &[xml::Node], group: &str) -> usize {
    xml::find(t, group).map_or(0, |g| g.children().len())
}

/// Ganzzahliges Kindfeld eines Knotens (BTP schreibt IDs als Integer).
fn child_int(node: &xml::Node, field: &str) -> Option<i64> {
    node.children()
        .iter()
        .find(|c| c.id() == field)
        .and_then(|c| match c.value() {
            Some(xml::Value::Integer(i)) => Some(*i),
            _ => None,
        })
}

/// EntryID → nennt BTP dort überhaupt einen zweiten Spieler?
///
/// Das trennt die beiden Fälle, die im Roster gleich aussehen: eine Meldung
/// **ohne** `Player2ID` ist in BTP als Einzelmeldung gepflegt (Partner noch
/// gesucht) — eine **mit** `Player2ID`, die im Roster trotzdem nur einen
/// Spieler trägt, ist ein Datenfehler in BTP (Spieler fehlt in `Players`).
fn raw_partner_flags(t: &[xml::Node]) -> HashMap<i64, bool> {
    let Some(entries) = xml::find(t, "Entries") else {
        return HashMap::new();
    };
    entries
        .children()
        .iter()
        .filter_map(|e| {
            let id = child_int(e, "ID")?;
            Some((id, child_int(e, "Player2ID").is_some_and(|p| p > 0)))
        })
        .collect()
}

/// Zwei Spieler erwartet BTP bei Doppel und Mixed; eine Meldung mit nur einem
/// aufgelösten Spieler ist dort ein Datenfehler (fehlender Partner).
fn is_doubles(d: Discipline) -> bool {
    matches!(
        d,
        Discipline::MensDoubles | Discipline::WomensDoubles | Discipline::Mixed
    )
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn what_would_the_checkin_receive() {
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(password().as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    let snapshot = model::parse_snapshot(&nodes).expect("Snapshot");

    let t = tournament_children(&nodes);
    let roh_events = raw_count(t, "Events");
    let roh_entries = raw_count(t, "Entries");
    let partner_im_btp = raw_partner_flags(t);

    // Platzhalter-GUID: badhub bekommt hier nichts, sie steht nur im Payload.
    let guid = std::env::var("CHECKIN_GUID")
        .unwrap_or_else(|_| "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string());
    let roster = build_checkin_roster(&snapshot, &guid, 1);

    println!("\n=== Meldeliste für den Hallen-Check-In ===");
    println!("Turnier:        {}", roster.tournament_name);
    println!("Turnier-GUID:   {}", roster.tournament_uuid);
    println!(
        "BTP liefert:    {roh_events} Klassen, {roh_entries} Meldungen (roh)"
    );
    println!(
        "aufgelöst:      {} Klassen, {} Meldungen",
        snapshot.events.len(),
        snapshot.entries.len()
    );
    println!(
        "gesendet würde: {} Klassen, {} Meldungen",
        roster.classes.len(),
        roster.entries.len()
    );

    if snapshot.events.is_empty() || snapshot.entries.is_empty() {
        println!(
            "\n!! BTP liefert keine Events/Entries — ohne sie steht keine \
             Meldeliste bereit."
        );
        return;
    }

    // --- je Klasse ---------------------------------------------------------
    println!("\n--- Klassen (in der Meldeliste) ---");
    println!(
        "{:>6}  {:<20} {:<16} {:>9} {:>7} {:>7} {:>7}",
        "ID", "Name", "Disziplin", "Meldungen", "Spieler", "Lizenz", "Verein"
    );
    for class in &roster.classes {
        let entries: Vec<_> = roster
            .entries
            .iter()
            .filter(|e| e.event_id == class.event_id)
            .collect();
        let players: Vec<_> = entries.iter().flat_map(|e| &e.players).collect();
        let mit_lizenz = players.iter().filter(|p| p.member_id.is_some()).count();
        let mit_verein = players.iter().filter(|p| p.club.is_some()).count();
        println!(
            "{:>6}  {:<20} {:<16} {:>9} {:>7} {:>7} {:>7}",
            class.event_id,
            class.name,
            class.discipline.as_str(),
            entries.len(),
            players.len(),
            mit_lizenz,
            mit_verein
        );

        // Doppel/Mixed mit nur einem Spieler im Roster — zwei verschiedene
        // Fälle, die hier auseinandergehalten werden: in BTP ohne Partner
        // gemeldet (normal, Partner noch gesucht) oder Partner nicht
        // auflösbar (Datenfehler in BTP; die Meldung bleibt trotzdem, damit
        // der anwesende Partner einchecken kann).
        if is_doubles(class.discipline) {
            let (kaputt, offen): (Vec<i64>, Vec<i64>) = entries
                .iter()
                .filter(|e| e.players.len() < 2)
                .map(|e| e.entry_id)
                .partition(|id| partner_im_btp.get(id).copied().unwrap_or(false));
            if !offen.is_empty() {
                println!(
                    "        ·  {} Meldung(en) in BTP ohne Partner (EntryIDs {:?})",
                    offen.len(),
                    offen
                );
            }
            if !kaputt.is_empty() {
                println!(
                    "        !! {} Meldung(en) mit unauflösbarem Partner (EntryIDs {:?})",
                    kaputt.len(),
                    kaputt
                );
            }
        }
    }

    // Klassen, die BTP kennt, die aber niemand gemeldet hat — sie werden
    // bewusst weggelassen (leere Liste, in die niemand einchecken kann).
    let ohne_meldung: Vec<&str> = snapshot
        .events
        .iter()
        .filter(|ev| !roster.classes.iter().any(|c| c.event_id == ev.id))
        .map(|ev| ev.name.as_str())
        .collect();
    if !ohne_meldung.is_empty() {
        println!(
            "\nKlassen ohne Meldung (nicht gesendet): {}",
            ohne_meldung.join(", ")
        );
    }

    // --- Spieler in mehreren Klassen ---------------------------------------
    // Der Check-In gilt je Klasse; diese Spieler checken mehrfach ein.
    let mut klassen_je_spieler: HashMap<i64, HashSet<i64>> = HashMap::new();
    for e in &roster.entries {
        for p in &e.players {
            klassen_je_spieler
                .entry(p.player_id)
                .or_default()
                .insert(e.event_id);
        }
    }
    let mehrfach = klassen_je_spieler.values().filter(|k| k.len() > 1).count();
    println!(
        "\nSpieler gesamt: {} — davon in mehreren Klassen: {mehrfach}",
        klassen_je_spieler.len()
    );

    // --- Wire-Form ---------------------------------------------------------
    let json = serde_json::to_string(&roster).expect("serialisierbar");
    println!("Payload-Größe:  {} Bytes", json.len());

    if show_names() {
        println!("\n--- Erste 5 Meldungen im Klartext ---");
        for e in roster.entries.iter().take(5) {
            let namen: Vec<String> = e
                .players
                .iter()
                .map(|p| {
                    format!(
                        "{} {}{}",
                        p.first,
                        p.last,
                        p.member_id
                            .as_ref()
                            .map(|m| format!(" [{m}]"))
                            .unwrap_or_default()
                    )
                })
                .collect();
            println!("  Entry {} (Event {}): {}", e.entry_id, e.event_id, namen.join(" / "));
        }
    } else {
        println!("(Namen ausgeblendet — mit SHOW_NAMES=1 mit ausgeben)");
    }

    // --- Zusicherungen -----------------------------------------------------
    // Datenschutz: kein Geburtsjahr, auch nicht mittelbar.
    for verboten in ["birth", "Birth", "yob", "geburt", "Geburt"] {
        assert!(
            !json.contains(verboten),
            "Payload enthält \"{verboten}\" — Geburtsdaten gehören nicht in die Meldeliste"
        );
    }
    // Eine Meldung ohne Namen wäre auf der Check-In-Seite nicht anklickbar.
    assert!(
        roster.entries.iter().all(|e| !e.players.is_empty()),
        "Meldung ohne aufgelösten Spieler im Payload"
    );
    // Jede Meldung muss ihre Klasse in der Liste finden.
    assert!(
        roster
            .entries
            .iter()
            .all(|e| roster.classes.iter().any(|c| c.event_id == e.event_id)),
        "Meldung verweist auf eine Klasse, die nicht mitgesendet wird"
    );
}
