//! **Messwerkzeug, kein Regressionstest.** Sucht im rohen
//! `SENDTOURNAMENTINFO` eines laufenden BTP nach Ortsangaben an noch nicht
//! aufgerufenen Spielen.
//!
//! Hintergrund: Schritt 15 hatte an zwei Mitschnitten gemessen, dass ein
//! angesetztes Spiel keinen Spielort trägt — die Spalten „Feld"/„Spielort"
//! des BTP-Exports waren dort in allen 540 Zeilen leer. Offen blieb, was ein
//! Turnier liefert, das sie **pflegt**. Genau so eines steht jetzt bereit.
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test btp_location_probe -- --ignored --nocapture
//! ```
//!
//! Ausgabe enthält **keine Spielernamen**, nur Feldnamen und Strukturdaten.

use bts_light_lib::btp::{client, proto, xml};

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

/// Sammelt alle Kind-Feldnamen eines Knotens samt Wert-Art.
fn felder(node: &xml::Node) -> Vec<(String, String)> {
    node.children()
        .iter()
        .map(|c| {
            let wert = match c.value() {
                Some(xml::Value::Integer(i)) => format!("int {i}"),
                Some(xml::Value::String(s)) => format!("str \"{s}\""),
                Some(xml::Value::DateTime(_)) => "datetime".to_string(),
                Some(other) => format!("{other:?}"),
                None => format!("gruppe ({} Kinder)", c.children().len()),
            };
            (c.id().to_string(), wert)
        })
        .collect()
}

/// Zeigt die ersten wartenden Spiele mit **allen** Sortierschlüsseln —
/// damit sich eine Reihenfolge, die „nicht hinhaut", an Zahlen statt an
/// Vermutungen klären lässt.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn why_is_the_queue_in_this_order() {
    use bts_light_lib::btp::model::MatchStatus;
    use bts_light_lib::tablet::assign;

    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(password().as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    let snap = bts_light_lib::btp::model::parse_snapshot(&nodes).expect("Snapshot");

    let mut wartend: Vec<_> = snap
        .matches
        .iter()
        .filter(|m| {
            m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
        })
        .collect();
    wartend.sort_by_key(|m| assign::sort_key(m, false));

    println!(
        "\n=== Die ersten 15 von {} wartenden Spielen ===",
        wartend.len()
    );
    println!("#    MatchID PlannedTime    Display Nr     Ort      Runde/Auslosung");
    for (i, m) in wartend.iter().take(15).enumerate() {
        println!(
            "{:<4} {:<7} {:<14} {:<7} {:<6} {:<8} {} / {}",
            i + 1,
            m.id,
            m.planned_time.map(|t| t.to_string()).unwrap_or("-".into()),
            m.display_order.map(|d| d.to_string()).unwrap_or("-".into()),
            m.match_num.map(|n| n.to_string()).unwrap_or("-".into()),
            m.location_id.map(|l| l.to_string()).unwrap_or("-".into()),
            m.round_name,
            m.draw_name,
        );
    }

    // Woher kommt die Reihenfolge wirklich? Die Referenzliste aus BTP
    // sortiert nach Auslosung, dann Spielnummer. Also nachsehen, welche
    // Ordnungszahlen an Draw und Stage hängen.
    fn finde<'a>(nodes: &'a [xml::Node], gesucht: &str) -> Option<&'a xml::Node> {
        for n in nodes {
            if n.id() == gesucht {
                return Some(n);
            }
            if let Some(t) = finde(n.children(), gesucht) {
                return Some(t);
            }
        }
        None
    }
    fn kind_int(n: &xml::Node, id: &str) -> Option<i64> {
        n.children()
            .iter()
            .find(|c| c.id() == id)
            .and_then(|c| match c.value() {
                Some(xml::Value::Integer(i)) => Some(*i),
                _ => None,
            })
    }
    fn kind_str(n: &xml::Node, id: &str) -> String {
        n.children()
            .iter()
            .find(|c| c.id() == id)
            .and_then(|c| match c.value() {
                Some(xml::Value::String(s)) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
    if let Some(draws) = finde(&nodes, "Draws") {
        println!("\n=== Draws: ID, DisplayOrder, Position, StageID, Name ===");
        let mut liste: Vec<_> = draws
            .children()
            .iter()
            .map(|d| {
                (
                    kind_int(d, "ID").unwrap_or(0),
                    kind_int(d, "DisplayOrder"),
                    kind_int(d, "Position"),
                    kind_int(d, "StageID"),
                    kind_str(d, "Name"),
                )
            })
            .collect();
        liste.sort_by_key(|(_, d, _, _, _)| d.unwrap_or(i64::MAX));
        for (id, disp, pos, stage, name) in liste.iter().take(12) {
            let d = disp.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
            let p = pos.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
            let s = stage.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
            println!("  ID={id:<5} Display={d:<5} Position={p:<5} Stage={s:<5} {name}");
        }
    }

    // Und die Statusverteilung: Ein Spiel, das schon einmal auf einem Feld
    // stand, könnte einen anderen Status tragen und dadurch herausfallen.
    let mut nach_status = std::collections::BTreeMap::new();
    for m in &snap.matches {
        *nach_status.entry(format!("{:?}", m.status)).or_insert(0) += 1;
    }
    println!("\n=== Status aller Matches ===");
    for (s, n) in nach_status {
        println!("  {s:<12} {n}");
    }
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn what_does_this_tournament_send_about_locations() {
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(password().as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");

    println!("\n=== Wurzelknoten ===");
    for n in &nodes {
        println!("  {:<22} ({} Kinder)", n.id(), n.children().len());
    }

    // Den Knoten mit den Matches suchen, ohne die Verschachtelung zu raten.
    fn suche<'a>(nodes: &'a [xml::Node], gesucht: &str) -> Option<&'a xml::Node> {
        for n in nodes {
            if n.id() == gesucht {
                return Some(n);
            }
            if let Some(t) = suche(n.children(), gesucht) {
                return Some(t);
            }
        }
        None
    }

    let tournament = suche(&nodes, "Tournament").unwrap_or(&nodes[0]);
    println!("\n=== Knoten unter {} ===", tournament.id());
    for (name, art) in felder(tournament) {
        println!("  {name:<22} {art}");
    }

    // Alle Feldnamen, die an einem Match vorkommen — über ALLE Matches
    // gesammelt, damit kein selten gefülltes Feld durchrutscht.
    let Some(matches) = suche(&nodes, "Matches") else {
        println!("(keine Matches im Mitschnitt)");
        return;
    };
    let mut namen: std::collections::BTreeMap<String, (usize, String)> = Default::default();
    let mut gesamt = 0usize;
    for m in matches.children() {
        gesamt += 1;
        for (name, art) in felder(m) {
            let e = namen.entry(name).or_insert((0, art.clone()));
            e.0 += 1;
            e.1 = art;
        }
    }
    println!("\n=== Felder an {gesamt} Matches (Anzahl : Feld : Beispielwert) ===");
    for (name, (anzahl, beispiel)) in &namen {
        println!("  {anzahl:>5} × {name:<22} {beispiel}");
    }

    // 3. Gezielt: Alles, was nach Ort klingt.
    println!("\n=== Ortsverdächtige Felder an Matches ===");
    let verdacht = ["location", "court", "venue", "hall", "place", "site"];
    let treffer: Vec<&String> = namen
        .keys()
        .filter(|n| {
            let l = n.to_lowercase();
            verdacht.iter().any(|v| l.contains(v))
        })
        .collect();
    if treffer.is_empty() {
        println!("  (keine)");
    } else {
        for t in treffer {
            let (anzahl, beispiel) = &namen[t];
            println!("  {t:<22} an {anzahl} Matches, zuletzt {beispiel}");
        }
    }

    // 4. Und dasselbe für Draw/Event/Stage — vielleicht hängt der Ort dort.
    for gruppe in ["Draws", "Events", "Stages", "Locations", "Courts"] {
        let Some(knoten) = suche(&nodes, gruppe) else {
            continue;
        };
        let mut n: std::collections::BTreeSet<String> = Default::default();
        for kind in knoten.children() {
            for (name, _) in felder(kind) {
                n.insert(name);
            }
        }
        println!(
            "\n=== Felder unter {gruppe} ({} Einträge) ===\n  {}",
            knoten.children().len(),
            n.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
}
