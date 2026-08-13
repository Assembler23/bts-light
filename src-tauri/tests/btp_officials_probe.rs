//! **Messwerkzeug, kein Regressionstest.** Beantwortet die drei offenen
//! Messfragen der Spec „Schiedsrichtermanagement"
//! (docs/features/schiedsrichter-management.md):
//!
//! 1. Welche Felder trägt ein `Officials > Official` — insbesondere: gibt
//!    es eine `ClubID` (Verein), analog `Player.ClubID`?
//! 2. Wie hängen `Official1ID`/`Official2ID` am `Match` (Semantik SR/AR)?
//! 3. Nimmt BTP Official-IDs in einem `SENDUPDATE` an — eigenständig
//!    und/oder eingebettet in die Feldzuweisung — oder verwirft es sie
//!    still (Präzedenzfall `LocationID`, Messung 10.08.2026)?
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test btp_officials_probe -- --ignored --nocapture
//! ```
//!
//! Namen werden in der Ausgabe maskiert (erster Buchstabe + Länge) —
//! Strukturdaten genügen für die Messung.

use bts_light_lib::btp::{client, proto, wire, xml};
use std::collections::BTreeMap;

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

fn kind_int(n: &xml::Node, id: &str) -> Option<i64> {
    n.children()
        .iter()
        .find(|c| c.id() == id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_int())
}

/// Namensfelder maskieren (erster Buchstabe + Länge), Rest im Klartext.
fn wert_maskiert(feld: &str, value: Option<&xml::Value>) -> String {
    let maskieren = matches!(feld, "Name" | "FirstName" | "Lastname" | "Asianname");
    match value {
        Some(xml::Value::Integer(i)) => format!("int {i}"),
        Some(xml::Value::String(s)) if maskieren => {
            let erster = s.chars().next().map(|c| c.to_string()).unwrap_or_default();
            format!("str \"{erster}…\" ({} Zeichen)", s.chars().count())
        }
        Some(xml::Value::String(s)) => format!("str \"{s}\""),
        Some(xml::Value::DateTime(_)) => "datetime".to_string(),
        Some(other) => format!("{other:?}"),
        None => "gruppe".to_string(),
    }
}

/// **Frage 1 + 2 (lesend):** Officials-Struktur und Official-Felder an
/// Matches. Kein Schreiben.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn what_does_this_tournament_send_about_officials() {
    let pw = password();
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(pw.as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");

    // 1. Container unter Tournament — existiert "Officials"? Was noch?
    let tournament = suche(&nodes, "Tournament").expect("Tournament-Knoten");
    println!("\n=== Container unter Tournament ===");
    for c in tournament.children() {
        println!("  {:<22} {} Kinder", c.id(), c.children().len());
    }

    // 2. Officials: alle Feldnamen über alle Einträge + jeder Eintrag im
    //    Detail (Namen maskiert).
    match suche(&nodes, "Officials") {
        None => println!("\n=== KEIN Officials-Container im Snapshot ==="),
        Some(g) => {
            println!("\n=== Officials: {} Einträge ===", g.children().len());
            let mut felder: BTreeMap<String, usize> = BTreeMap::new();
            for o in g.children() {
                for c in o.children() {
                    *felder.entry(c.id().to_string()).or_insert(0) += 1;
                }
            }
            println!("Felder über alle Einträge: {felder:?}");
            for o in g.children() {
                let kv: Vec<String> = o
                    .children()
                    .iter()
                    .map(|c| format!("{}={}", c.id(), wert_maskiert(c.id(), c.value())))
                    .collect();
                println!("  {}", kv.join("  "));
            }

            // Vereinsverdächtige Felder gezielt benennen.
            let verdacht = ["club", "verein", "team", "association", "member"];
            let treffer: Vec<&String> = felder
                .keys()
                .filter(|n| {
                    let l = n.to_lowercase();
                    verdacht.iter().any(|v| l.contains(v))
                })
                .collect();
            println!("Vereinsverdächtige Official-Felder: {treffer:?}");
        }
    }

    // 3. Clubs-Container (zum Auflösen einer etwaigen ClubID).
    if let Some(g) = suche(&nodes, "Clubs") {
        println!("\n=== Clubs: {} Einträge ===", g.children().len());
        for c in g.children().iter().take(5) {
            let kv: Vec<String> = c
                .children()
                .iter()
                .map(|x| format!("{}={}", x.id(), wert_maskiert(x.id(), x.value())))
                .collect();
            println!("  {}", kv.join("  "));
        }
    } else {
        println!("\n=== kein Clubs-Container ===");
    }

    // 4. Matches: Wie oft sind Official1ID/Official2ID vorhanden, welche
    //    Werte tragen sie? (Semantik-Frage 2: In BTP je ein Spiel mit nur
    //    SR bzw. SR+AR pflegen, dann hier ablesen, welches Feld was ist.)
    let Some(matches) = suche(&nodes, "Matches") else {
        println!("\n(keine Matches)");
        return;
    };
    let mut mit_o1 = 0usize;
    let mut mit_o2 = 0usize;
    println!("\n=== Matches mit Official-Feldern ===");
    for m in matches.children() {
        let o1 = kind_int(m, "Official1ID");
        let o2 = kind_int(m, "Official2ID");
        if o1.is_some() || o2.is_some() {
            mit_o1 += o1.is_some() as usize;
            mit_o2 += o2.is_some() as usize;
            println!(
                "  MatchID={:?} Court={:?} Official1ID={o1:?} Official2ID={o2:?}",
                kind_int(m, "ID"),
                kind_int(m, "CourtID"),
            );
        }
    }
    println!(
        "Summe: {} Matches, {mit_o1}× Official1ID, {mit_o2}× Official2ID",
        matches.children().len()
    );

    // 5. Alle Feldnamen an Matches (fällt ein weiteres Official-/
    //    Schiedsrichter-Feld auf?).
    let mut namen: BTreeMap<String, usize> = BTreeMap::new();
    for m in matches.children() {
        for c in m.children() {
            *namen.entry(c.id().to_string()).or_insert(0) += 1;
        }
    }
    println!("\n=== Feldnamen an Matches (Anzahl × Name) ===");
    for (name, anzahl) in &namen {
        println!("  {anzahl:>5} × {name}");
    }
}

/// Verpackt beliebige Match-Kinder (plus optionalen Courts-Block) in einen
/// vollständigen `SENDUPDATE`. Bewusst NICHT in proto.rs — reine Mess-Hilfe
/// (Baustruktur wie `base_request`, nachgebaut aus den öffentlichen
/// `xml::Node`-Bausteinen).
fn update_request_raw(
    match_children: Vec<xml::Node>,
    courts_block: Option<xml::Node>,
    session_key: &str,
    password: Option<&str>,
) -> Vec<u8> {
    let mut action_children = vec![xml::Node::string("ID", "SENDUPDATE")];
    action_children.push(xml::Node::string("Unicode", session_key));
    if let Some(pw) = password {
        action_children.push(xml::Node::string("Password", pw));
    }
    let mut tournament_children = Vec::new();
    if let Some(courts) = courts_block {
        tournament_children.push(courts);
    }
    tournament_children.push(xml::Node::group(
        "Matches",
        vec![xml::Node::group("Match", match_children)],
    ));
    let nodes = vec![
        xml::Node::group(
            "Header",
            vec![xml::Node::group(
                "Version",
                vec![xml::Node::integer("Hi", 1), xml::Node::integer("Lo", 1)],
            )],
        ),
        xml::Node::group("Action", action_children),
        xml::Node::group("Client", vec![xml::Node::string("IP", "bts-light")]),
        xml::Node::group(
            "Update",
            vec![xml::Node::group("Tournament", tournament_children)],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

async fn login(pw: Option<&str>) -> String {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("BTP erreichbar (LOGIN)");
    proto::parse_login_response(&proto::decode_response(&raw).expect("LOGIN-Antwort dekodierbar"))
        .expect("LOGIN akzeptiert (Passwort korrekt?)")
}

/// Roher Match-Knoten einer Match-ID aus einem frischen Snapshot.
async fn raw_match(pw: Option<&str>, match_id: i64) -> xml::Node {
    let raw = client::send_request(&host(), port(), &proto::tournament_info_request(pw))
        .await
        .expect("BTP erreichbar (Lesen)");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    let matches = suche(&nodes, "Matches").expect("Matches vorhanden");
    matches
        .children()
        .iter()
        .find(|m| kind_int(m, "ID") == Some(match_id))
        .cloned()
        .expect("Ziel-Match vorhanden")
}

/// **Frage 3 (SCHREIBT probeweise und stellt zurück):** Zwei Schreibformen —
/// V1 eigenständig (`ID, DrawID, PlanningID, Official1ID, Official2ID`),
/// V2 eingebettet in die Feldzuweisungs-Form (`+ CourtID` + Courts-Block,
/// die letilo-Form aus `call_match`). Je Form: schreiben → roh zurücklesen
/// → Verdikt → Restore (0 = löschen, BTP-Konvention) → verifizieren.
/// Bewusst KEIN `Status`-Feld (Check-in-Bitfeld, Regression v0.9.103).
///
/// ```text
/// cargo test -p bts-light --test btp_officials_probe -- --ignored --nocapture does_btp_accept_official_writes
/// ```
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_btp_accept_official_writes() {
    use bts_light_lib::btp::model::MatchStatus;

    let pw = password();
    let pw_ref = pw.as_deref();

    // --- VORHER: Snapshot + Officials-IDs ------------------------------
    let raw_before = client::send_request(&host(), port(), &proto::tournament_info_request(pw_ref))
        .await
        .expect("BTP erreichbar (VORHER)");
    let nodes_before = proto::decode_response(&raw_before).expect("dekodierbar");
    let before = bts_light_lib::btp::model::parse_snapshot(&nodes_before).expect("Snapshot");
    println!("\n=== Turnier: {} ===", before.tournament_name);

    let Some(officials) = suche(&nodes_before, "Officials") else {
        println!("ABBRUCH: kein Officials-Container — erst Schiedsrichter in BTP anlegen.");
        return;
    };
    let official_ids: Vec<i64> = officials
        .children()
        .iter()
        .filter_map(|o| kind_int(o, "ID"))
        .collect();
    println!("Official-IDs im Turnier: {official_ids:?}");
    if official_ids.len() < 2 {
        println!("ABBRUCH: brauche mindestens zwei Officials (SR + AR).");
        return;
    }
    let (sr_id, ar_id) = (official_ids[0], official_ids[1]);

    // Kandidat: angesetzt, beide Teams, kein Feld, keine Officials.
    let mut kandidaten: Vec<&bts_light_lib::btp::model::BtpMatch> = before
        .matches
        .iter()
        .filter(|m| {
            m.status == MatchStatus::Scheduled
                && !m.team1.is_empty()
                && !m.team2.is_empty()
                && m.court_id.is_none()
        })
        .collect();
    kandidaten.sort_by_key(|m| m.id);
    let Some(ziel) = kandidaten
        .iter()
        .find(|m| {
            let roh = raw_match_node_in(&nodes_before, m.id);
            roh.as_ref()
                .map(|r| {
                    kind_int(r, "Official1ID").is_none() && kind_int(r, "Official2ID").is_none()
                })
                .unwrap_or(false)
        })
        .copied()
    else {
        println!("ABBRUCH: kein angesetztes Match ohne Feld und ohne Officials gefunden.");
        return;
    };
    let (match_id, draw_id, planning_id) = (ziel.id, ziel.draw_id, ziel.planning_id);
    println!("Ziel-Match: ID={match_id} DrawID={draw_id} PlanningID={planning_id}");
    println!("Schreibe: Official1ID={sr_id}, Official2ID={ar_id}\n");

    // Freies Feld für V2 suchen (von keinem nicht-fertigen Match belegt).
    let belegt: std::collections::HashSet<i64> = before
        .matches
        .iter()
        .filter(|m| m.status != MatchStatus::Finished)
        .filter_map(|m| m.court_id)
        .collect();
    let freies_feld = before.court_infos.iter().find(|c| !belegt.contains(&c.id));

    // Bauplan je Variante; `o1`/`o2` = 0 fürs Restore (löschen).
    let match_kinder = |o1: i64, o2: i64, court_id: Option<i64>| -> Vec<xml::Node> {
        let mut kinder = vec![
            xml::Node::integer("ID", match_id),
            xml::Node::integer("DrawID", draw_id),
            xml::Node::integer("PlanningID", planning_id),
        ];
        if let Some(cid) = court_id {
            kinder.push(xml::Node::integer("CourtID", cid));
        }
        kinder.push(xml::Node::integer("Official1ID", o1));
        kinder.push(xml::Node::integer("Official2ID", o2));
        kinder
    };
    let schreibe = |kinder: Vec<xml::Node>, courts: Option<xml::Node>| {
        let pw2 = pw.clone();
        async move {
            let session = login(pw2.as_deref()).await;
            let raw = client::send_request(
                &host(),
                port(),
                &update_request_raw(kinder, courts, &session, pw2.as_deref()),
            )
            .await
            .expect("BTP erreichbar (Schreiben)");
            proto::parse_update_response(
                &proto::decode_response(&raw).expect("Antwort dekodierbar"),
            )
        }
    };
    // Erste Messung (13.08.2026) zeigte: BTP übernimmt die Writes
    // ASYNCHRON — ein sofortiges Zurücklesen sieht noch den alten Stand.
    // Deshalb: pollen, bis der Erwartungswert steht (oder Timeout).
    let warte_auf = |erwartet_o1: Option<i64>, erwartet_o2: Option<i64>| async move {
        for versuch in 0..15u32 {
            let roh = raw_match(pw_ref, match_id).await;
            let ist = (kind_int(&roh, "Official1ID"), kind_int(&roh, "Official2ID"));
            if ist == (erwartet_o1, erwartet_o2) {
                return (true, ist, versuch);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        let roh = raw_match(pw_ref, match_id).await;
        let ist = (kind_int(&roh, "Official1ID"), kind_int(&roh, "Official2ID"));
        (ist == (erwartet_o1, erwartet_o2), ist, 15)
    };

    let mut ergebnisse: Vec<(String, String)> = Vec::new();

    // ── V1: eigenständiger Officials-Write (ohne Feld) ─────────────────
    println!("──── V1: eigenständig (ID/DrawID/PlanningID + Official1ID/2) ────");
    println!(
        "  Antwort: {:?}",
        schreibe(match_kinder(sr_id, ar_id, None), None).await
    );
    let (ok, ist, versuche) = warte_auf(Some(sr_id), Some(ar_id)).await;
    let v1 = if ok {
        format!("ANGENOMMEN ✔ (sichtbar nach {versuche} s)")
    } else {
        format!("IGNORIERT (nach 15 s noch O1={:?} O2={:?})", ist.0, ist.1)
    };
    println!("  → {v1}");
    // Restore V1 (0 = löschen).
    println!(
        "  Restore: {:?}",
        schreibe(match_kinder(0, 0, None), None).await
    );
    let (restore_ok, ist, _) = warte_auf(None, None).await;
    println!(
        "  nach Restore: O1={:?} O2={:?} (gelöscht: {restore_ok})\n",
        ist.0, ist.1
    );
    assert!(
        restore_ok,
        "RESTORE V1 FEHLGESCHLAGEN — Match {match_id} trägt noch Officials {ist:?}. \
         MANUELL IN BTP PRÜFEN!"
    );
    ergebnisse.push(("V1 eigenständig".into(), v1));

    // ── V2: eingebettet in die Feldzuweisung (letilo call_match) ───────
    if let Some(feld) = freies_feld {
        println!("──── V2: eingebettet in Feldzuweisung (Feld „{}", feld.name);
        let courts_zu = xml::Node::group(
            "Courts",
            vec![xml::Node::group(
                "Court",
                vec![
                    xml::Node::integer("ID", feld.id),
                    xml::Node::integer("MatchID", match_id),
                ],
            )],
        );
        println!(
            "  Zuweisung+Officials: {:?}",
            schreibe(match_kinder(sr_id, ar_id, Some(feld.id)), Some(courts_zu)).await
        );
        let (ok, ist, versuche) = warte_auf(Some(sr_id), Some(ar_id)).await;
        let v2 = if ok {
            format!("ANGENOMMEN ✔ (sichtbar nach {versuche} s)")
        } else {
            format!("IGNORIERT (nach 15 s noch O1={:?} O2={:?})", ist.0, ist.1)
        };
        println!("  → {v2}");

        // Restore V2: Feld frei + Officials löschen.
        let courts_frei = xml::Node::group(
            "Courts",
            vec![xml::Node::group(
                "Court",
                vec![xml::Node::integer("ID", feld.id)],
            )],
        );
        println!(
            "  Restore (Feld frei + Officials 0): {:?}",
            schreibe(match_kinder(0, 0, Some(0)), Some(courts_frei)).await
        );
        let (restore_ok, ist, _) = warte_auf(None, None).await;
        let roh = raw_match(pw_ref, match_id).await;
        let court_nachher = kind_int(&roh, "CourtID");
        println!(
            "  nach Restore: O1={:?} O2={:?} CourtID={court_nachher:?} (gelöscht: {restore_ok})",
            ist.0, ist.1
        );
        assert!(
            restore_ok && (court_nachher.is_none() || court_nachher == Some(0)),
            "RESTORE V2 UNVOLLSTÄNDIG: Match {match_id} Officials {ist:?}, CourtID \
             {court_nachher:?} — MANUELL IN BTP PRÜFEN!"
        );
        ergebnisse.push(("V2 mit Feldzuweisung".into(), v2));
    } else {
        println!("──── V2 übersprungen: kein freies Feld ────");
    }

    // Nebenwirkungen: Sets/Winner/Status unverändert?
    let after = client::fetch_snapshot(&host(), port(), pw_ref)
        .await
        .expect("BTP erreichbar (Kontrolle)");
    let nach = after
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .expect("Match vorhanden");
    assert!(
        nach.sets == ziel.sets && nach.winner == ziel.winner && nach.status == ziel.status,
        "NEBENWIRKUNG an Sets/Winner/Status — MANUELL IN BTP PRÜFEN! \
         (Sets {:?}, Winner {:?}, Status {:?})",
        nach.sets,
        nach.winner,
        nach.status
    );

    println!("\n########################################");
    println!("OFFICIAL-WRITES:");
    for (variante, verdikt) in &ergebnisse {
        println!("  {variante:<22} {verdikt}");
    }
    println!("########################################");
}

/// Roher Match-Knoten in bereits dekodierten Nodes (sync-Variante).
fn raw_match_node_in(nodes: &[xml::Node], match_id: i64) -> Option<xml::Node> {
    let matches = suche(nodes, "Matches")?;
    matches
        .children()
        .iter()
        .find(|m| kind_int(m, "ID") == Some(match_id))
        .cloned()
}
