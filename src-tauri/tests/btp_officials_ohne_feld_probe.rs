//! **Messwerkzeug, kein Regressionstest.** Beantwortet die Messfrage der Spec
//! `sr-ar-beim-aufruf` (ADR 0080): Verträgt BTP Schiedsrichter-Writes an
//! Spielen **ohne Feld**, ohne eine Feldzuordnung anzutasten?
//!
//! Dass BTP `Official1ID/2ID` an einem Spiel ohne Feld übernimmt, ist seit
//! 13.08.2026 gemessen (`btp_officials_probe.rs`, V1). Offen ist das Risiko
//! aus #450 (CourtID-0-Klasse): Die Zuteilung beim Aufruf schreibt viel
//! häufiger an wartende Spiele, und zwar über `sync.rs::officials_entries`
//! mit `CourtID` aus dem Snapshot — für ein wartendes Spiel also `0`. Trifft
//! dieser Write kurz **nach** einer frischen Feldzuweisung ein (Snapshot noch
//! alt), könnte er das gerade vergebene Feld wieder löschen.
//!
//! Gemessen wird an einem wartenden Spiel eines **Testturniers**:
//!
//! 1. Officials-Write mit `CourtID 0` (genau die Form von `officials_entries`)
//!    an einem Spiel ohne Feld: übernommen? Feldzuordnung unverändert?
//! 2. Wettlauf: Feldzuweisung, unmittelbar danach Officials-Write mit dem
//!    veralteten `CourtID 0`: Bleibt das Feld?
//! 3. Wettlauf wie 2, aber **ohne** `CourtID`-Element: Bleibt das Feld?
//! 4. Löschen mit `0` an einem Spiel ohne Feld.
//!
//! Am Ende wird der Ausgangsstand wiederhergestellt (Feld frei, keine
//! Officials).
//!
//! **Während der Messung bts-light-Übertragung bzw. Auto-Vergabe AUS** — die
//! Freigabe am Ende gibt das gewählte Feld frei, egal wer es inzwischen
//! belegt hat. Bricht die Messung mittendrin ab (Netzfehler, Panic), springt
//! der Restore nicht an: dann Spiel und Feld laut Ausgabe in BTP von Hand
//! bereinigen. Standardmäßig übersprungen — braucht ein laufendes BTP mit
//! mindestens zwei Schiedsrichtern, einem wartenden Spiel ohne Feld und
//! einem freien Feld:
//!
//! ```text
//! cargo test -p bts-light --test btp_officials_ohne_feld_probe -- --ignored --nocapture
//! ```

use bts_light_lib::btp::model::{BtpSnapshot, MatchStatus};
use bts_light_lib::btp::{client, proto, wire, xml};
use std::time::Duration;

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

async fn snapshot(pw: Option<&str>) -> (Vec<xml::Node>, BtpSnapshot) {
    let raw = client::send_request(&host(), port(), &proto::tournament_info_request(pw))
        .await
        .expect("BTP erreichbar (Lesen)");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    let snap = bts_light_lib::btp::model::parse_snapshot(&nodes).expect("Snapshot");
    (nodes, snap)
}

async fn login(pw: Option<&str>) -> String {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("BTP erreichbar (LOGIN)");
    proto::parse_login_response(&proto::decode_response(&raw).expect("LOGIN-Antwort dekodierbar"))
        .expect("LOGIN akzeptiert (Passwort korrekt?)")
}

/// Stand am Match: (CourtID, Official1ID, Official2ID), roh gelesen.
async fn stand(pw: Option<&str>, match_id: i64) -> (Option<i64>, Option<i64>, Option<i64>) {
    let (nodes, _) = snapshot(pw).await;
    let m = suche(&nodes, "Matches")
        .expect("Matches vorhanden")
        .children()
        .iter()
        .find(|m| kind_int(m, "ID") == Some(match_id))
        .cloned()
        .expect("Ziel-Match vorhanden");
    let null_weg = |v: Option<i64>| v.filter(|x| *x != 0);
    (
        null_weg(kind_int(&m, "CourtID")),
        null_weg(kind_int(&m, "Official1ID")),
        null_weg(kind_int(&m, "Official2ID")),
    )
}

/// Pollt bis zu 10 s, bis `erwartet` erfüllt ist (BTP übernimmt asynchron,
/// ≤ 1 s, siehe docs/btp_protocol.md). Liefert den letzten Stand.
async fn warte(
    pw: Option<&str>,
    match_id: i64,
    erwartet: impl Fn(&(Option<i64>, Option<i64>, Option<i64>)) -> bool,
) -> (bool, (Option<i64>, Option<i64>, Option<i64>)) {
    let mut ist = stand(pw, match_id).await;
    for _ in 0..10 {
        if erwartet(&ist) {
            return (true, ist);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        ist = stand(pw, match_id).await;
    }
    (erwartet(&ist), ist)
}

async fn sende(bytes: Vec<u8>) {
    let raw = client::send_request(&host(), port(), &bytes)
        .await
        .expect("BTP erreichbar (Schreiben)");
    let antwort = proto::parse_update_response(&proto::decode_response(&raw).expect("dekodierbar"));
    println!("    Antwort: {antwort:?}");
}

/// Officials-Write in der Form von `officials_entries`: leerer
/// `Courts`-Block, `CourtID` wie übergeben.
fn officials_write(
    m: &bts_light_lib::btp::model::BtpMatch,
    court_id: i64,
    sr: i64,
    ar: i64,
    session: &str,
    pw: Option<&str>,
) -> Vec<u8> {
    proto::court_assign_request(
        &[],
        &[proto::MatchCourt {
            match_id: m.id,
            draw_id: m.draw_id,
            planning_id: m.planning_id,
            court_id,
            officials: Some((sr, ar)),
        }],
        session,
        pw,
    )
}

/// Feldzuweisung (bzw. Freigabe mit `None`) in der Form von `auto_assign`.
fn feld_write(
    m: &bts_light_lib::btp::model::BtpMatch,
    court_id: i64,
    vergeben: bool,
    session: &str,
    pw: Option<&str>,
) -> Vec<u8> {
    proto::court_assign_request(
        &[proto::CourtAssignment {
            court_id,
            match_id: vergeben.then_some(m.id),
        }],
        &[proto::MatchCourt {
            match_id: m.id,
            draw_id: m.draw_id,
            planning_id: m.planning_id,
            court_id: if vergeben { court_id } else { 0 },
            officials: None,
        }],
        session,
        pw,
    )
}

/// Officials-Write **ohne** `CourtID`-Element (Mess-Hilfe, nicht in proto.rs).
fn officials_write_ohne_court(
    m: &bts_light_lib::btp::model::BtpMatch,
    sr: i64,
    ar: i64,
    session: &str,
    pw: Option<&str>,
) -> Vec<u8> {
    let mut action = vec![
        xml::Node::string("ID", "SENDUPDATE"),
        xml::Node::string("Unicode", session),
    ];
    if let Some(p) = pw {
        action.push(xml::Node::string("Password", p));
    }
    let nodes = vec![
        xml::Node::group(
            "Header",
            vec![xml::Node::group(
                "Version",
                vec![xml::Node::integer("Hi", 1), xml::Node::integer("Lo", 1)],
            )],
        ),
        xml::Node::group("Action", action),
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
                            xml::Node::integer("ID", m.id),
                            xml::Node::integer("DrawID", m.draw_id),
                            xml::Node::integer("PlanningID", m.planning_id),
                            xml::Node::integer("Official1ID", sr),
                            xml::Node::integer("Official2ID", ar),
                        ],
                    )],
                )],
            )],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn do_official_writes_without_court_keep_court_assignments() {
    let pw = password();
    let pw = pw.as_deref();
    let (_, vorher) = snapshot(pw).await;
    println!("\n=== Turnier: {} ===", vorher.tournament_name);

    let ids: Vec<i64> = vorher.officials.iter().map(|o| o.id).collect();
    if ids.len() < 2 {
        println!("ABBRUCH: brauche mindestens zwei Schiedsrichter in BTP.");
        return;
    }
    let (sr, ar) = (ids[0], ids[1]);
    let Some(m) = vorher
        .matches
        .iter()
        .find(|m| {
            m.status == MatchStatus::Scheduled
                && !m.team1.is_empty()
                && !m.team2.is_empty()
                && m.court_id.is_none()
                && m.official1_id.unwrap_or(0) == 0
                && m.official2_id.unwrap_or(0) == 0
        })
        .cloned()
    else {
        println!("ABBRUCH: kein wartendes Spiel ohne Feld und ohne Officials.");
        return;
    };
    let belegt: Vec<i64> = vorher.matches.iter().filter_map(|m| m.court_id).collect();
    let Some(feld) = vorher
        .court_infos
        .iter()
        .map(|c| c.id)
        .find(|c| !belegt.contains(c))
    else {
        println!("ABBRUCH: kein freies Feld.");
        return;
    };
    println!(
        "Spiel {} (Nr. {:?}), freies Feld {feld}, SR {sr}, AR {ar}",
        m.id, m.match_num
    );
    let mut befunde: Vec<(&str, String)> = Vec::new();

    // ── 1: Officials-Write mit CourtID 0 an einem Spiel ohne Feld ──────
    println!("\n── 1: Officials mit CourtID 0, Spiel ohne Feld ──");
    sende(officials_write(&m, 0, sr, ar, &login(pw).await, pw)).await;
    let (ok, ist) = warte(pw, m.id, |s| s.1 == Some(sr) && s.2 == Some(ar)).await;
    befunde.push((
        "1 CourtID 0 ohne Feld",
        format!(
            "{} — Stand {ist:?}{}",
            if ok {
                "ÜBERNOMMEN"
            } else {
                "NICHT übernommen"
            },
            if ist.0.is_some() {
                " ⚠ Feld aufgetaucht!"
            } else {
                ""
            }
        ),
    ));

    // ── 2: Wettlauf Feldzuweisung → Officials mit veraltetem CourtID 0 ─
    println!("\n── 2: Feld vergeben, sofort Officials mit CourtID 0 ──");
    let s = login(pw).await;
    sende(feld_write(&m, feld, true, &s, pw)).await;
    sende(officials_write(&m, 0, ar, sr, &s, pw)).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (_, ist) = warte(pw, m.id, |s| s.1 == Some(ar) && s.2 == Some(sr)).await;
    befunde.push((
        "2 Wettlauf mit CourtID 0",
        if ist.0 == Some(feld) {
            format!("Feld BLEIBT ✔ — Stand {ist:?}")
        } else {
            format!("Feld VERLOREN ✘ — Stand {ist:?}")
        },
    ));

    // Zurück auf „kein Feld" für Messung 3.
    sende(feld_write(&m, feld, false, &login(pw).await, pw)).await;
    warte(pw, m.id, |s| s.0.is_none()).await;

    // ── 3: Wettlauf ohne CourtID-Element ───────────────────────────────
    println!("\n── 3: Feld vergeben, sofort Officials ohne CourtID ──");
    let s = login(pw).await;
    sende(feld_write(&m, feld, true, &s, pw)).await;
    sende(officials_write_ohne_court(&m, sr, ar, &s, pw)).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (_, ist) = warte(pw, m.id, |s| s.1 == Some(sr) && s.2 == Some(ar)).await;
    befunde.push((
        "3 Wettlauf ohne CourtID",
        if ist.0 == Some(feld) {
            format!("Feld BLEIBT ✔ — Stand {ist:?}")
        } else {
            format!("Feld VERLOREN ✘ — Stand {ist:?}")
        },
    ));

    // Feld wieder frei.
    sende(feld_write(&m, feld, false, &login(pw).await, pw)).await;
    warte(pw, m.id, |s| s.0.is_none()).await;

    // ── 4: Löschen mit 0 an einem Spiel ohne Feld ──────────────────────
    println!("\n── 4: Officials löschen (0), Spiel ohne Feld ──");
    sende(officials_write(&m, 0, 0, 0, &login(pw).await, pw)).await;
    let (ok, ist) = warte(pw, m.id, |s| s.1.is_none() && s.2.is_none()).await;
    befunde.push((
        "4 Löschen mit 0",
        format!(
            "{} — Stand {ist:?}",
            if ok { "GELÖSCHT" } else { "NICHT gelöscht" }
        ),
    ));

    // ── Wiederherstellen und prüfen ────────────────────────────────────
    let s = login(pw).await;
    sende(feld_write(&m, feld, false, &s, pw)).await;
    sende(officials_write(&m, 0, 0, 0, &s, pw)).await;
    let (sauber, ist) = warte(pw, m.id, |s| *s == (None, None, None)).await;

    println!("\n=== Ergebnis ===");
    for (frage, befund) in &befunde {
        println!("  {frage:<26} {befund}");
    }
    println!("  Ausgangsstand wiederhergestellt: {sauber} ({ist:?})");
    assert!(
        sauber,
        "RESTORE UNVOLLSTÄNDIG: Spiel {} steht bei {ist:?} — in BTP von Hand bereinigen",
        m.id
    );
}
