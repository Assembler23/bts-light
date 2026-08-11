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

use bts_light_lib::btp::{client, proto, wire, xml};

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

/// Baut einen `SENDUPDATE`, der **ausschließlich** `Match.LocationID`
/// schreibt (`ID, DrawID, PlanningID, LocationID` — sonst nichts, insb. kein
/// `Status`). Bewusst NICHT in `proto.rs`: reine Mess-Hilfe für diese
/// Schreib-Probe, kein Produktionscode. Baustruktur (Header/Action/Client +
/// Update/Tournament/Matches/Match) folgt exakt dem privaten `base_request`
/// aus `proto.rs`, nachgebaut aus den öffentlichen `xml::Node`-Bausteinen,
/// da `base_request` selbst nicht `pub` ist.
fn location_only_request(
    match_id: i64,
    draw_id: i64,
    planning_id: i64,
    location_id: i64,
    session_key: &str,
    password: Option<&str>,
) -> Vec<u8> {
    let mut action_children = vec![xml::Node::string("ID", "SENDUPDATE")];
    action_children.push(xml::Node::string("Unicode", session_key));
    if let Some(pw) = password {
        action_children.push(xml::Node::string("Password", pw));
    }
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
            vec![xml::Node::group(
                "Tournament",
                vec![xml::Node::group(
                    "Matches",
                    vec![xml::Node::group(
                        "Match",
                        vec![
                            xml::Node::integer("ID", match_id),
                            xml::Node::integer("DrawID", draw_id),
                            xml::Node::integer("PlanningID", planning_id),
                            xml::Node::integer("LocationID", location_id),
                        ],
                    )],
                )],
            )],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

/// Roh-XML einer Wire-Antwort für die Ausgabe – dekodiert, aber sonst
/// unangetastet (keine Spielernamen werden extra herausgefiltert; die
/// SENDUPDATE-Antwort enthält ohnehin nur `Action`, keine Spielerdaten).
fn raw_preview(bytes: &[u8]) -> String {
    match wire::decode_message(bytes) {
        Ok(xml) => xml,
        Err(e) => format!(
            "(nicht als Wire-XML dekodierbar: {e}; {} Rohbytes)",
            bytes.len()
        ),
    }
}

/// Frisches LOGIN gegen BTP, liefert den Session-Schlüssel für den
/// nachfolgenden SENDUPDATE. Eigene Funktion, weil die Messung zweimal
/// einloggt (Schreiben + Restore) – eine wiederverwendete Session könnte
/// durch serverseitige Regeln zwischenzeitlich ungültig geworden sein.
async fn login(pw: Option<&str>) -> String {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("BTP erreichbar (LOGIN)");
    proto::parse_login_response(&proto::decode_response(&raw).expect("LOGIN-Antwort dekodierbar"))
        .expect("LOGIN akzeptiert (Passwort korrekt?)")
}

/// Verpackt beliebige Match-Kinder in einen vollständigen `SENDUPDATE`.
/// Gemeinsames Gerüst der Schreib-Varianten unten.
fn match_update_request(
    match_children: Vec<xml::Node>,
    session_key: &str,
    password: Option<&str>,
) -> Vec<u8> {
    let mut action_children = vec![xml::Node::string("ID", "SENDUPDATE")];
    action_children.push(xml::Node::string("Unicode", session_key));
    if let Some(pw) = password {
        action_children.push(xml::Node::string("Password", pw));
    }
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
            vec![xml::Node::group(
                "Tournament",
                vec![xml::Node::group(
                    "Matches",
                    vec![xml::Node::group("Match", match_children)],
                )],
            )],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

/// Sucht den ROHEN Match-Knoten (unparsed) einer Match-ID im Snapshot.
fn raw_match_node(nodes: &[xml::Node], match_id: i64) -> Option<xml::Node> {
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
    let matches = suche(nodes, "Matches")?;
    matches
        .children()
        .iter()
        .find(|m| {
            m.children()
                .iter()
                .find(|c| c.id() == "ID")
                .and_then(|c| c.value())
                .and_then(|v| v.as_int())
                == Some(match_id)
        })
        .cloned()
}

/// Kinder eines rohen Match-Knotens: `LocationID` ersetzt/ergänzt,
/// `Status` entfernt (Check-in-Bitfeld — schreiben markiert Spieler als
/// nicht eingecheckt, siehe `court_assign_request` in proto.rs).
fn mirrored_children(raw: &xml::Node, location_id: i64) -> Vec<xml::Node> {
    let mut children: Vec<xml::Node> = raw
        .children()
        .iter()
        .filter(|c| c.id() != "LocationID" && c.id() != "Status")
        .cloned()
        .collect();
    children.push(xml::Node::integer("LocationID", location_id));
    children
}

/// **Messung, kein Regressionstest — SCHREIBT probeweise nach BTP und
/// stellt danach den Originalzustand wieder her.**
///
/// Frage: Nimmt BTP einen `SENDUPDATE` an, der an einem angesetzten Match
/// **ausschließlich** `Match.LocationID` setzt (Kandidat für „Spielort vor
/// der Feldvergabe ändern")? Oder ignoriert/lehnt BTP das ab, weil
/// `LocationID` serverseitig als reines Auslosungs-/Anzeige-Datum gilt?
///
/// Ablauf: VORHER-Snapshot → Kandidat wählen (Scheduled, beide Teams,
/// kein Court) → minimaler SENDUPDATE nur mit LocationID → NACHHER-Snapshot
/// (LocationID sowie CourtID/Sets/Winner/Status auf Nebenwirkungen prüfen)
/// → Originalwert zurückschreiben → Restore verifizieren. Bricht das Match
/// NICHT im veränderten Zustand zurück, wenn der Restore fehlschlägt
/// (`assert!` am Ende) – das Turnier muss unverändert bleiben.
///
/// ```text
/// cargo test -p bts-light --test btp_location_probe -- --ignored --nocapture does_btp_accept_a_location_write_for_a_scheduled_match
/// ```
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_btp_accept_a_location_write_for_a_scheduled_match() {
    use bts_light_lib::btp::model::MatchStatus;

    let pw = password();
    let pw_ref = pw.as_deref();

    // --- VORHER lesen -------------------------------------------------
    let before = client::fetch_snapshot(&host(), port(), pw_ref)
        .await
        .expect("BTP erreichbar (VORHER-Snapshot)");

    println!("\n=== Turnier: {} ===", before.tournament_name);
    println!(
        "Locations ({}): {:?}",
        before.locations.len(),
        before.locations
    );

    if before.locations.is_empty() {
        println!("\nABBRUCH: Turnier pflegt KEINE Locations — Messung nicht möglich.");
        return;
    }

    // Kandidat: angesetzt, beide Teams gesetzt, KEIN Feld zugewiesen —
    // damit die Messung nicht mit einer laufenden Feldzuweisung
    // interferiert. Deterministische Reihenfolge nach MatchID.
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
    println!(
        "Kandidaten (Scheduled, beide Teams, ohne Court): {}",
        kandidaten.len()
    );

    // Erstes Kandidat/Location-Paar, bei dem sich die LocationID vom
    // aktuellen Wert unterscheidet (auch None ≠ jede echte ID zählt als
    // Unterschied).
    let Some((ziel_match, ziel_location)) = kandidaten.iter().find_map(|&m| {
        before
            .locations
            .iter()
            .find(|l| Some(l.id) != m.location_id)
            .map(|l| (m, l.id))
    }) else {
        println!(
            "\nABBRUCH: kein angesetztes Match ohne Feld gefunden, dem eine \
             abweichende LocationID zugewiesen werden könnte."
        );
        return;
    };

    let match_id = ziel_match.id;
    let draw_id = ziel_match.draw_id;
    let planning_id = ziel_match.planning_id;
    let original_location_id = ziel_match.location_id;
    let original_court_id = ziel_match.court_id;
    let original_sets = ziel_match.sets.clone();
    let original_winner = ziel_match.winner;
    let original_status = ziel_match.status.clone();

    println!("\n=== Ziel-Match ===");
    println!("  MatchID={match_id} DrawID={draw_id} PlanningID={planning_id}");
    println!(
        "  LocationID VORHER: {}",
        original_location_id
            .map(|l| l.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!("  LocationID NEU (Ziel): {ziel_location}");
    println!("  CourtID VORHER: {original_court_id:?}");
    println!("  Sets VORHER:    {original_sets:?}");
    println!("  Winner VORHER:  {original_winner:?}");
    println!("  Status VORHER:  {original_status:?}");

    // --- SCHREIBEN: NUR LocationID -------------------------------------
    let session = login(pw_ref).await;
    let write_raw = client::send_request(
        &host(),
        port(),
        &location_only_request(
            match_id,
            draw_id,
            planning_id,
            ziel_location,
            &session,
            pw_ref,
        ),
    )
    .await
    .expect("BTP erreichbar (SENDUPDATE LocationID)");

    println!(
        "\n=== Rohantwort SENDUPDATE (nur LocationID) ===\n{}",
        raw_preview(&write_raw)
    );

    let write_nodes = proto::decode_response(&write_raw).expect("SENDUPDATE-Antwort dekodierbar");
    let write_result = proto::parse_update_response(&write_nodes);
    println!("\nparse_update_response: {write_result:?}");

    // --- NACHHER lesen (vor Restore) -----------------------------------
    let after = client::fetch_snapshot(&host(), port(), pw_ref)
        .await
        .expect("BTP erreichbar (NACHHER-Snapshot)");
    let nach_match = after
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .expect("Ziel-Match nach dem Schreiben noch im Turnier vorhanden");

    println!("\n=== Ziel-Match NACHHER (vor Restore) ===");
    println!(
        "  LocationID NACHHER: {:?}  (VORHER: {:?}, Ziel war: {ziel_location})",
        nach_match.location_id, original_location_id
    );
    println!(
        "  CourtID NACHHER:    {:?}  (VORHER: {original_court_id:?})",
        nach_match.court_id
    );
    println!(
        "  Sets NACHHER:       {:?}  (VORHER: {original_sets:?})",
        nach_match.sets
    );
    println!(
        "  Winner NACHHER:     {:?}  (VORHER: {original_winner:?})",
        nach_match.winner
    );
    println!(
        "  Status NACHHER:     {:?}  (VORHER: {original_status:?})",
        nach_match.status
    );
    println!(
        "  (BTP liefert keinen sichtbaren Check-in-Bitfeld-Wert am Match — die \
         Probe schreibt bewusst KEIN `Status`, siehe `court_assign_request`-Kommentar \
         in proto.rs, damit dieses Bitfeld unangetastet bleibt.)"
    );

    let sonst_unveraendert = nach_match.court_id == original_court_id
        && nach_match.sets == original_sets
        && nach_match.winner == original_winner
        && nach_match.status == original_status;
    println!("\nAndere Felder unverändert (CourtID/Sets/Winner/Status): {sonst_unveraendert}");
    assert!(
        sonst_unveraendert,
        "Der LocationID-Write hat NEBENWIRKUNGEN an anderen Feldern erzeugt — \
         MatchID={match_id}. VORHER: Court={original_court_id:?} Sets={original_sets:?} \
         Winner={original_winner:?} Status={original_status:?}; NACHHER: \
         Court={:?} Sets={:?} Winner={:?} Status={:?}",
        nach_match.court_id, nach_match.sets, nach_match.winner, nach_match.status
    );

    // --- VERDIKT --------------------------------------------------------
    let verdikt = if let Err(e) = &write_result {
        format!("ABGELEHNT ({e:?})")
    } else if nach_match.location_id == Some(ziel_location) {
        "ANGENOMMEN".to_string()
    } else {
        "IGNORIERT (stiller No-Op)".to_string()
    };
    println!("\n########################################");
    println!("LOCATION-WRITE: {verdikt}");
    println!("########################################");

    // --- RESTORE: Originalwert zurückschreiben --------------------------
    // 0 = LocationID löschen (BTP-Konvention, siehe `location_id`-Parsing:
    // "0 gilt als nicht gesetzt").
    let restore_target = original_location_id.unwrap_or(0);
    println!("\n=== RESTORE: LocationID zurück auf {restore_target} (Original) ===");

    let session2 = login(pw_ref).await;
    let restore_raw = client::send_request(
        &host(),
        port(),
        &location_only_request(
            match_id,
            draw_id,
            planning_id,
            restore_target,
            &session2,
            pw_ref,
        ),
    )
    .await
    .expect("BTP erreichbar (RESTORE)");
    println!(
        "\n=== Rohantwort RESTORE ===\n{}",
        raw_preview(&restore_raw)
    );
    let restore_result = proto::parse_update_response(
        &proto::decode_response(&restore_raw).expect("Restore-Antwort dekodierbar"),
    );
    println!("Restore parse_update_response: {restore_result:?}");

    let after_restore = client::fetch_snapshot(&host(), port(), pw_ref)
        .await
        .expect("BTP erreichbar (Restore-Kontrolle)");
    let restored_match = after_restore
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .expect("Ziel-Match nach Restore noch im Turnier vorhanden");

    println!(
        "\nLocationID nach Restore: {:?}  (Original: {:?})",
        restored_match.location_id, original_location_id
    );
    let restore_ok = restored_match.location_id == original_location_id;
    println!("Restore erfolgreich (Original wiederhergestellt): {restore_ok}");

    assert!(
        restore_ok,
        "RESTORE FEHLGESCHLAGEN — Turnier NICHT im Originalzustand! MatchID={match_id}, \
         Original-LocationID={original_location_id:?}, jetzt {:?}. MANUELL IN BTP PRÜFEN!",
        restored_match.location_id
    );
}

/// **Messung, kein Regressionstest — SCHREIBT probeweise und stellt
/// zurück.** Varianten-Matrix zum LocationID-Write (Nutzer-Wunsch
/// 11.08.2026: „Spielort nach BTP zurück wäre mir sehr wichtig").
///
/// Die erste Probe oben hat nur EINE Schreibform gemessen (minimaler
/// Update: `ID, DrawID, PlanningID, LocationID` → Result=1, still
/// ignoriert). Diese Matrix testet die noch offenen Hypothesen:
///
/// - **V2 `mit-plannedtime`** — BTPs Ansetzungs-Dialog pflegt Zeit und
///   Spielort zusammen; vielleicht übernimmt BTP die LocationID nur im
///   Verbund mit der (unverändert gespiegelten) `PlannedTime`.
/// - **V3 `voller-spiegel`** — der komplette rohe Match-Knoten wird
///   gespiegelt (ohne `Status`, s. o.), nur die LocationID ersetzt;
///   vielleicht verwirft BTP unvollständige Knoten still.
/// - **V4 `als-string`** — BTP typisiert Felder unterschiedlich;
///   vielleicht erwartet der Parser die LocationID als String.
///
/// Je Variante: schreiben → zurücklesen → Verdikt → Original
/// zurückschreiben → Restore verifizieren → Nebenwirkungen prüfen.
///
/// ```text
/// cargo test -p bts-light --test btp_location_probe -- --ignored --nocapture which_location_write_variant_sticks
/// ```
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn which_location_write_variant_sticks() {
    use bts_light_lib::btp::model::MatchStatus;

    let pw = password();
    let pw_ref = pw.as_deref();

    let raw_before = client::send_request(&host(), port(), &proto::tournament_info_request(pw_ref))
        .await
        .expect("BTP erreichbar (VORHER)");
    let nodes_before = proto::decode_response(&raw_before).expect("dekodierbar");
    let before = bts_light_lib::btp::model::parse_snapshot(&nodes_before).expect("Snapshot");

    println!("\n=== Turnier: {} ===", before.tournament_name);
    if before.locations.is_empty() {
        println!("ABBRUCH: Turnier pflegt KEINE Locations — Messung nicht möglich.");
        return;
    }

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
    let Some((ziel_match, ziel_location)) = kandidaten.iter().find_map(|&m| {
        before
            .locations
            .iter()
            .find(|l| Some(l.id) != m.location_id)
            .map(|l| (m, l.id))
    }) else {
        println!("ABBRUCH: kein geeignetes Match gefunden.");
        return;
    };

    let match_id = ziel_match.id;
    let draw_id = ziel_match.draw_id;
    let planning_id = ziel_match.planning_id;
    let original_location_id = ziel_match.location_id;
    let original_court_id = ziel_match.court_id;
    let original_sets = ziel_match.sets.clone();
    let original_winner = ziel_match.winner;
    let original_status = ziel_match.status.clone();
    let raw_match = raw_match_node(&nodes_before, match_id).expect("roher Match-Knoten");
    let raw_planned = raw_match
        .children()
        .iter()
        .find(|c| c.id() == "PlannedTime")
        .cloned();

    println!("Ziel: MatchID={match_id}, LocationID {original_location_id:?} → {ziel_location}\n");

    // Die Basis-Kinder je Variante — Ziel- und Restore-Wert entstehen
    // unten aus demselben Bauplan.
    let bauplan = |location_id: i64, variante: &str| -> Vec<xml::Node> {
        match variante {
            "mit-plannedtime" => {
                let mut kinder = vec![
                    xml::Node::integer("ID", match_id),
                    xml::Node::integer("DrawID", draw_id),
                    xml::Node::integer("PlanningID", planning_id),
                    xml::Node::integer("LocationID", location_id),
                ];
                if let Some(pt) = &raw_planned {
                    kinder.push(pt.clone());
                }
                kinder
            }
            "voller-spiegel" => mirrored_children(&raw_match, location_id),
            "als-string" => vec![
                xml::Node::integer("ID", match_id),
                xml::Node::integer("DrawID", draw_id),
                xml::Node::integer("PlanningID", planning_id),
                xml::Node::string("LocationID", location_id.to_string()),
            ],
            _ => unreachable!(),
        }
    };

    let mut ergebnisse: Vec<(String, String)> = Vec::new();
    for variante in ["mit-plannedtime", "voller-spiegel", "als-string"] {
        println!("──── Variante {variante} ────");
        let session = login(pw_ref).await;
        let write_raw = client::send_request(
            &host(),
            port(),
            &match_update_request(bauplan(ziel_location, variante), &session, pw_ref),
        )
        .await
        .expect("BTP erreichbar (Variante schreiben)");
        let write_result = proto::parse_update_response(
            &proto::decode_response(&write_raw).expect("Antwort dekodierbar"),
        );
        println!("  Antwort: {write_result:?}");

        let after = client::fetch_snapshot(&host(), port(), pw_ref)
            .await
            .expect("BTP erreichbar (NACHHER)");
        let nach = after
            .matches
            .iter()
            .find(|m| m.id == match_id)
            .expect("Match noch vorhanden");

        let verdikt = if write_result.is_err() {
            format!("ABGELEHNT ({write_result:?})")
        } else if nach.location_id == Some(ziel_location) {
            "ANGENOMMEN ✔".to_string()
        } else {
            "IGNORIERT (stiller No-Op)".to_string()
        };
        println!("  LocationID NACHHER: {:?} → {verdikt}", nach.location_id);

        // Nebenwirkungen sofort prüfen — eine Variante mit Kollateralschaden
        // bricht die Messung ab, bevor sie weiteres anfasst.
        assert!(
            nach.court_id == original_court_id
                && nach.sets == original_sets
                && nach.winner == original_winner
                && nach.status == original_status,
            "Variante {variante} hat NEBENWIRKUNGEN erzeugt — MANUELL IN BTP PRÜFEN! \
             (Court {:?}, Sets {:?}, Winner {:?}, Status {:?})",
            nach.court_id,
            nach.sets,
            nach.winner,
            nach.status
        );

        // Restore mit demselben Bauplan (0 = löschen, BTP-Konvention).
        let session2 = login(pw_ref).await;
        let _ = client::send_request(
            &host(),
            port(),
            &match_update_request(
                bauplan(original_location_id.unwrap_or(0), variante),
                &session2,
                pw_ref,
            ),
        )
        .await
        .expect("BTP erreichbar (Restore)");
        let restored = client::fetch_snapshot(&host(), port(), pw_ref)
            .await
            .expect("BTP erreichbar (Restore-Kontrolle)");
        let restored_match = restored
            .matches
            .iter()
            .find(|m| m.id == match_id)
            .expect("Match noch vorhanden");
        assert!(
            restored_match.location_id == original_location_id,
            "RESTORE nach Variante {variante} FEHLGESCHLAGEN — MatchID={match_id}, \
             Original {original_location_id:?}, jetzt {:?}. MANUELL IN BTP PRÜFEN!",
            restored_match.location_id
        );
        println!("  Restore ok.\n");
        ergebnisse.push((variante.to_string(), verdikt));
    }

    println!("########################################");
    println!("ERGEBNIS DER VARIANTEN-MATRIX:");
    for (variante, verdikt) in &ergebnisse {
        println!("  {variante:<18} {verdikt}");
    }
    println!("########################################");
    println!(
        "Steht überall IGNORIERT/ABGELEHNT, ist der Wire-Weg ausgereizt — \
         dann bleiben: Spielort in BTP pflegen (bts-light liest ihn bereits), \
         eine neuere BTP-Version messen oder eine Anfrage an Visual Reality."
    );
}
