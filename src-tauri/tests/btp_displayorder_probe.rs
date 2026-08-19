//! **Messwerkzeug, kein Regressionstest.** Beantwortet die offene
//! Messfrage aus dem Brief „Spielliste per Drag&Drop manuell sortierbar"
//! (`docs/features/_intake/spielliste-manuelle-reihenfolge/1-brief.md`):
//! Nimmt BTP einen `SENDUPDATE`, der **ausschließlich** `Match.DisplayOrder`
//! setzt, an — oder verwirft es das still, wie schon bei `LocationID`
//! gemessen (`btp_location_probe.rs`, 10.08.2026)?
//!
//! Aufbau bewusst 1:1 am Muster von
//! `btp_location_probe::does_btp_accept_a_location_write_for_a_scheduled_match`
//! orientiert — derselbe Diagnose-Stil hat dort den LocationID-Befund
//! zweifelsfrei geklärt.
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP.
//! **Läuft NICHT automatisch gegen das Live-Turnier** — vor dem Start
//! bewusst prüfen, gegen welches BTP `BTP_HOST`/`BTP_PORT` zeigen (Standard:
//! `127.0.0.1:9901`, also das gerade offene BTP-Fenster). Schreibt
//! probeweise und stellt danach den Originalwert wieder her; bricht mit
//! `assert!` ab, falls der Restore nicht nachweisbar gelingt.
//!
//! ```text
//! cargo test -p bts-light --test btp_displayorder_probe -- --ignored --nocapture
//! ```
//!
//! Ausgabe enthält keine Spielernamen, nur Strukturdaten (IDs, Zeiten,
//! Sortierschlüssel).

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

/// Baut einen `SENDUPDATE`, der **ausschließlich** `Match.DisplayOrder`
/// schreibt (`ID, DrawID, PlanningID, DisplayOrder` — sonst nichts, insb.
/// kein `Status`). Bewusst NICHT in `proto.rs`: reine Mess-Hilfe, kein
/// Produktionscode. Baustruktur nachgebaut aus `xml::Node`, analog
/// `btp_location_probe::location_only_request`.
fn displayorder_only_request(
    match_id: i64,
    draw_id: i64,
    planning_id: i64,
    display_order: i64,
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
                            xml::Node::integer("DisplayOrder", display_order),
                        ],
                    )],
                )],
            )],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

fn raw_preview(bytes: &[u8]) -> String {
    match wire::decode_message(bytes) {
        Ok(xml) => xml,
        Err(e) => format!(
            "(nicht als Wire-XML dekodierbar: {e}; {} Rohbytes)",
            bytes.len()
        ),
    }
}

async fn login(pw: Option<&str>) -> String {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("BTP erreichbar (LOGIN)");
    proto::parse_login_response(&proto::decode_response(&raw).expect("LOGIN-Antwort dekodierbar"))
        .expect("LOGIN akzeptiert (Passwort korrekt?)")
}

/// **Messung, kein Regressionstest — SCHREIBT probeweise nach BTP und
/// stellt danach den Originalzustand wieder her.**
///
/// Frage: Nimmt BTP einen `SENDUPDATE` an, der an einem angesetzten Match
/// **ausschließlich** `Match.DisplayOrder` setzt? Oder ignoriert/lehnt BTP
/// das ab wie schon bei `LocationID` gemessen?
///
/// Ablauf: VORHER-Snapshot → Kandidat wählen (Scheduled, beide Teams, kein
/// Court) → neuen DisplayOrder-Wert wählen (deutlich abweichend vom
/// Original, damit ein stiller No-Op nicht zufällig wie ein Treffer
/// aussieht) → minimaler SENDUPDATE nur mit DisplayOrder → NACHHER-Snapshot
/// (DisplayOrder sowie CourtID/Sets/Winner/Status/PlannedTime auf
/// Nebenwirkungen prüfen) → Originalwert zurückschreiben → Restore
/// verifizieren. Bricht das Match NICHT im veränderten Zustand zurück, wenn
/// der Restore fehlschlägt (`assert!` am Ende) — das Turnier muss
/// unverändert bleiben.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_btp_accept_a_displayorder_write_for_a_scheduled_match() {
    use bts_light_lib::btp::model::MatchStatus;
    use bts_light_lib::tablet::assign;

    let pw = password();
    let pw_ref = pw.as_deref();

    // --- VORHER lesen -------------------------------------------------
    let before = client::fetch_snapshot(&host(), port(), pw_ref)
        .await
        .expect("BTP erreichbar (VORHER-Snapshot)");

    println!("\n=== Turnier: {} ===", before.tournament_name);

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

    let Some(ziel_match) = kandidaten.first().copied() else {
        println!("\nABBRUCH: kein angesetztes Match ohne Feld gefunden.");
        return;
    };

    let match_id = ziel_match.id;
    let draw_id = ziel_match.draw_id;
    let planning_id = ziel_match.planning_id;
    let original_display_order = ziel_match.display_order;
    let original_planned_time = ziel_match.planned_time;
    let original_court_id = ziel_match.court_id;
    let original_sets = ziel_match.sets.clone();
    let original_winner = ziel_match.winner;
    let original_status = ziel_match.status.clone();

    // Deutlich abweichender Zielwert — weit weg vom Original, damit ein
    // stiller No-Op nicht zufällig als Treffer durchgeht.
    let ziel_display_order = original_display_order.unwrap_or(0) + 100_000;

    println!("\n=== Ziel-Match ===");
    println!("  MatchID={match_id} DrawID={draw_id} PlanningID={planning_id}");
    println!(
        "  DisplayOrder VORHER: {}",
        original_display_order
            .map(|d| d.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!("  DisplayOrder NEU (Ziel): {ziel_display_order}");
    println!("  PlannedTime VORHER: {original_planned_time:?}");
    println!("  CourtID VORHER: {original_court_id:?}");
    println!("  Sets VORHER:    {original_sets:?}");
    println!("  Winner VORHER:  {original_winner:?}");
    println!("  Status VORHER:  {original_status:?}");

    // --- SCHREIBEN: NUR DisplayOrder -----------------------------------
    let session = login(pw_ref).await;
    let write_raw = client::send_request(
        &host(),
        port(),
        &displayorder_only_request(
            match_id,
            draw_id,
            planning_id,
            ziel_display_order,
            &session,
            pw_ref,
        ),
    )
    .await
    .expect("BTP erreichbar (SENDUPDATE DisplayOrder)");

    println!(
        "\n=== Rohantwort SENDUPDATE (nur DisplayOrder) ===\n{}",
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
        "  DisplayOrder NACHHER: {:?}  (VORHER: {:?}, Ziel war: {ziel_display_order})",
        nach_match.display_order, original_display_order
    );
    println!(
        "  PlannedTime NACHHER: {:?}  (VORHER: {original_planned_time:?})",
        nach_match.planned_time
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

    let sonst_unveraendert = nach_match.planned_time == original_planned_time
        && nach_match.court_id == original_court_id
        && nach_match.sets == original_sets
        && nach_match.winner == original_winner
        && nach_match.status == original_status;
    println!(
        "\nAndere Felder unverändert (PlannedTime/CourtID/Sets/Winner/Status): {sonst_unveraendert}"
    );
    assert!(
        sonst_unveraendert,
        "Der DisplayOrder-Write hat NEBENWIRKUNGEN an anderen Feldern erzeugt — \
         MatchID={match_id}. VORHER: PlannedTime={original_planned_time:?} \
         Court={original_court_id:?} Sets={original_sets:?} Winner={original_winner:?} \
         Status={original_status:?}; NACHHER: PlannedTime={:?} Court={:?} Sets={:?} \
         Winner={:?} Status={:?}",
        nach_match.planned_time,
        nach_match.court_id,
        nach_match.sets,
        nach_match.winner,
        nach_match.status
    );

    // --- VERDIKT --------------------------------------------------------
    let verdikt = if let Err(e) = &write_result {
        format!("ABGELEHNT ({e:?})")
    } else if nach_match.display_order == Some(ziel_display_order) {
        "ANGENOMMEN".to_string()
    } else {
        "IGNORIERT (stiller No-Op)".to_string()
    };
    println!("\n########################################");
    println!("DISPLAYORDER-WRITE: {verdikt}");
    println!("########################################");

    // Falls angenommen: gleich zeigen, wie sich das auf unseren eigenen
    // Sortierschlüssel auswirkt (PlannedTime → DisplayOrder → MatchNr → ID)
    // — beantwortet zugleich, ob der neue Wert das Match innerhalb seines
    // PlannedTime-Slots tatsächlich nach hinten schiebt.
    if nach_match.display_order == Some(ziel_display_order) {
        let mut wartend: Vec<_> = after
            .matches
            .iter()
            .filter(|m| {
                m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
            })
            .collect();
        wartend.sort_by_key(|m| assign::sort_key(m, false));
        let neue_position = wartend.iter().position(|m| m.id == match_id);
        println!(
            "Position von MatchID={match_id} in der eigenen Sortierung NACH dem Write: {:?} von {}",
            neue_position,
            wartend.len()
        );
    }

    // --- RESTORE: Originalwert zurückschreiben --------------------------
    let restore_target = original_display_order.unwrap_or(0);
    println!("\n=== RESTORE: DisplayOrder zurück auf {restore_target} (Original) ===");

    let session2 = login(pw_ref).await;
    let restore_raw = client::send_request(
        &host(),
        port(),
        &displayorder_only_request(
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
        "\nDisplayOrder nach Restore: {:?}  (Original: {:?})",
        restored_match.display_order, original_display_order
    );
    let restore_ok = restored_match.display_order == original_display_order;
    println!("Restore erfolgreich (Original wiederhergestellt): {restore_ok}");

    assert!(
        restore_ok,
        "RESTORE FEHLGESCHLAGEN — Turnier NICHT im Originalzustand! MatchID={match_id}, \
         Original-DisplayOrder={original_display_order:?}, jetzt {:?}. MANUELL IN BTP PRÜFEN!",
        restored_match.display_order
    );
}
