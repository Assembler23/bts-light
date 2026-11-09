//! **Messwerkzeug, kein Regressionstest.** Löscht ein Schiedsrichter-Write
//! die Zeilenfarbe `Match.Highlight`?
//!
//! Anlass (09.11.2026): Gerufene Spiele verloren ihr Gelb (P1) genau in dem
//! Moment, in dem die Rotation ihnen SR/AR zuschrieb. Der Write
//! (`proto::court_assign_request` mit `Matches`-Block) trägt kein
//! `Highlight` — für `CourtID` ist schon gemessen, dass BTP ein fehlendes
//! Feld auf 0 setzt (`btp_officials_ohne_feld_probe.rs`).
//!
//! Gemessen an einem wartenden Spiel ohne Feld und ohne Farbe:
//!
//! 1. Farbe 3 (Orange) setzen, dann Officials-Write in der Form von
//!    `officials_entries` (unveränderte Besetzung) **ohne** `Highlight`.
//!    Bleibt die Farbe?
//! 2. Farbe 3 erneut setzen, Officials-Write **mit** `Highlight=3`.
//!    Bleibt die Farbe, bleibt die Besetzung?
//!
//! Gegenrichtung (`does_a_highlight_write_clear_officials_or_court`):
//!
//! 3. Wartendes Spiel mit Besetzung: Farb-Write — bleibt die Besetzung?
//! 4. Laufendes Spiel: Farb-Write mit der eigenen Farbe — bleibt das Feld?
//!
//! **Ergebnis 09.11.2026** (Testturnier, BTP lokal): Mit der damaligen
//! Schreibform (Knoten ohne das jeweils andere Feld) war in 1, 3 und 4 alles
//! **weg** — Farbe, Besetzung, sogar Feld und SR des laufenden Spiels (von
//! Hand zurückgesetzt). 2 hielt die Farbe. BTP setzt `CourtID`,
//! `Official1ID/2ID` und `Highlight` aus jedem Match-Write, ein fehlendes
//! Element heißt 0. Seitdem tragen beide Writes alle vier Felder;
//! `farbe()` unten baut die Schreibform von `proto::highlight_request` nach,
//! 3 und 4 prüfen also den Fix.
//!
//! Am Ende: Farbe 0, Besetzung wie vorher. Standardmäßig übersprungen:
//!
//! ```text
//! cargo test -p bts-light --test btp_highlight_sr_write_probe -- --ignored --nocapture
//! ```

use bts_light_lib::btp::model::{BtpMatch, BtpSnapshot, MatchStatus};
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

async fn snapshot(pw: Option<&str>) -> BtpSnapshot {
    let raw = client::send_request(&host(), port(), &proto::tournament_info_request(pw))
        .await
        .expect("BTP erreichbar (Lesen)");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    bts_light_lib::btp::model::parse_snapshot(&nodes).expect("Snapshot")
}

async fn login(pw: Option<&str>) -> String {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("BTP erreichbar (LOGIN)");
    proto::parse_login_response(&proto::decode_response(&raw).expect("LOGIN-Antwort dekodierbar"))
        .expect("LOGIN akzeptiert (Passwort korrekt?)")
}

/// (Highlight, Official1ID, Official2ID, CourtID) nach bis zu 3 s Wartezeit.
async fn stand(pw: Option<&str>, id: i64) -> (u8, Option<i64>, Option<i64>, Option<i64>) {
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let s = snapshot(pw).await;
    let m = s.matches.iter().find(|m| m.id == id).expect("Match da");
    (m.highlight, m.official1_id, m.official2_id, m.court_id)
}

async fn sende(bytes: Vec<u8>) {
    let raw = client::send_request(&host(), port(), &bytes)
        .await
        .expect("BTP erreichbar (Schreiben)");
    let antwort = proto::parse_update_response(&proto::decode_response(&raw).expect("dekodierbar"));
    println!("    Antwort: {antwort:?}");
}

fn farbe(m: &BtpMatch, wert: u8, session: &str, pw: Option<&str>) -> Vec<u8> {
    proto::highlight_request(
        &[proto::HighlightEntry {
            match_id: m.id,
            draw_id: m.draw_id,
            planning_id: m.planning_id,
            highlight: wert,
            court_id: m.court_id.unwrap_or(0),
            officials: (m.official1_id.unwrap_or(0), m.official2_id.unwrap_or(0)),
        }],
        session,
        pw,
    )
}

/// Officials-Write wie `court_assign_request` (CourtID 0, unveränderte
/// Besetzung), wahlweise mit `Highlight`.
fn officials_write(
    m: &BtpMatch,
    highlight: Option<u8>,
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
    let mut kinder = vec![
        xml::Node::integer("ID", m.id),
        xml::Node::integer("CourtID", 0),
        xml::Node::integer("DrawID", m.draw_id),
        xml::Node::integer("PlanningID", m.planning_id),
        xml::Node::integer("Official1ID", m.official1_id.unwrap_or(0)),
        xml::Node::integer("Official2ID", m.official2_id.unwrap_or(0)),
    ];
    if let Some(h) = highlight {
        kinder.push(xml::Node::integer("Highlight", i64::from(h)));
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
                    vec![xml::Node::group("Match", kinder)],
                )],
            )],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_an_officials_write_clear_the_highlight() {
    let pw = password();
    let pw = pw.as_deref();
    let s = snapshot(pw).await;
    println!("\n=== Turnier: {} ===", s.tournament_name);
    // Ein wartendes Spiel ohne Feld und ohne Farbe — vom Ende der Liste,
    // damit es die laufende Rotation/Aufruf-Automatik nicht gerade anfasst.
    let m = s
        .matches
        .iter()
        .rev()
        .find(|m| {
            m.status == MatchStatus::Scheduled
                && m.court_id.is_none()
                && m.highlight == 0
                && !m.team1.is_empty()
                && !m.team2.is_empty()
        })
        .cloned()
        .expect("wartendes Spiel ohne Feld");
    println!(
        "Ziel: Match {} (Officials {:?}/{:?})",
        m.id, m.official1_id, m.official2_id
    );

    let session = login(pw).await;

    println!("\n1) Farbe 3, dann Officials-Write OHNE Highlight");
    sende(farbe(&m, 3, &session, pw)).await;
    println!("    nach Farbe:   {:?}", stand(pw, m.id).await);
    sende(officials_write(&m, None, &session, pw)).await;
    let k1 = stand(pw, m.id).await;
    println!(
        "    nach Officials: {k1:?}  → Farbe {}",
        if k1.0 == 3 { "BLEIBT" } else { "WEG" }
    );

    println!("\n2) Farbe 3, dann Officials-Write MIT Highlight=3");
    sende(farbe(&m, 3, &session, pw)).await;
    println!("    nach Farbe:   {:?}", stand(pw, m.id).await);
    sende(officials_write(&m, Some(3), &session, pw)).await;
    let k2 = stand(pw, m.id).await;
    println!(
        "    nach Officials: {k2:?}  → Farbe {}",
        if k2.0 == 3 { "BLEIBT" } else { "WEG" }
    );

    println!("\nRestore: Farbe 0");
    sende(farbe(&m, 0, &session, pw)).await;
    println!("    Endstand: {:?}", stand(pw, m.id).await);
}

/// Gegenrichtung: Löscht der Farb-Write (`highlight_request`, nur Identität
/// + `Highlight`) die Besetzung eines wartenden Spiels oder das Feld eines
/// laufenden? Der Farb-Write an einem laufenden Spiel schreibt dessen
/// **eigene** Farbe zurück, ändert also nichts außer dem Gemessenen.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_a_highlight_write_clear_officials_or_court() {
    let pw = password();
    let pw = pw.as_deref();
    let s = snapshot(pw).await;
    let session = login(pw).await;

    let ids: Vec<i64> = s.officials.iter().map(|o| o.id).take(2).collect();
    assert!(ids.len() == 2, "mindestens zwei Schiedsrichter im Turnier");
    let m = s
        .matches
        .iter()
        .rev()
        .find(|m| {
            m.status == MatchStatus::Scheduled
                && m.court_id.is_none()
                && m.highlight == 0
                && m.official1_id.is_none()
                && m.official2_id.is_none()
                && !m.team1.is_empty()
                && !m.team2.is_empty()
        })
        .cloned()
        .expect("wartendes Spiel ohne Feld, Farbe, Besetzung");
    println!(
        "\n3) Match {}: Besetzung {:?} setzen, dann Farb-Write",
        m.id, ids
    );
    let mut besetzt = m.clone();
    besetzt.official1_id = Some(ids[0]);
    besetzt.official2_id = Some(ids[1]);
    sende(officials_write(&besetzt, Some(0), &session, pw)).await;
    println!("    nach Besetzung: {:?}", stand(pw, m.id).await);
    sende(farbe(&besetzt, 3, &session, pw)).await;
    let k3 = stand(pw, m.id).await;
    println!(
        "    nach Farbe:     {k3:?}  → Besetzung {}",
        if k3.1 == Some(ids[0]) && k3.2 == Some(ids[1]) {
            "BLEIBT"
        } else {
            "WEG"
        }
    );
    println!("    Restore: Besetzung leer, Farbe 0");
    sende(officials_write(&m, Some(0), &session, pw)).await;
    println!("    Endstand: {:?}", stand(pw, m.id).await);

    if let Some(laufend) = s
        .matches
        .iter()
        .find(|m| m.status == MatchStatus::OnCourt && m.court_id.is_some())
        .cloned()
    {
        println!(
            "\n4) laufendes Match {} auf Court {:?}: Farb-Write mit eigener Farbe {}",
            laufend.id, laufend.court_id, laufend.highlight
        );
        sende(farbe(&laufend, laufend.highlight, &session, pw)).await;
        let k4 = stand(pw, laufend.id).await;
        println!(
            "    nach Farbe: {k4:?}  → Feld {}",
            if k4.3 == laufend.court_id {
                "BLEIBT"
            } else {
                "WEG — von Hand in BTP zurücksetzen!"
            }
        );
    } else {
        println!("\n4) kein laufendes Spiel — übersprungen");
    }
}

/// Wie schnell zeigt BTP einen eigenen Write beim Nachlesen? Liest nach dem
/// Write alle 100 ms und meldet, ab wann der Wert sichtbar ist. Entscheidet,
/// ob ein „frisch gelesener" Stand einen eben geschriebenen Wert schon trägt.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn how_fast_does_btp_show_own_writes() {
    let pw = password();
    let pw = pw.as_deref();
    let s = snapshot(pw).await;
    let session = login(pw).await;
    let ids: Vec<i64> = s.officials.iter().map(|o| o.id).take(2).collect();
    let m = s
        .matches
        .iter()
        .rev()
        .find(|m| {
            m.status == MatchStatus::Scheduled
                && m.court_id.is_none()
                && m.highlight == 0
                && m.official1_id.is_none()
                && m.official2_id.is_none()
                && !m.team1.is_empty()
                && !m.team2.is_empty()
        })
        .cloned()
        .expect("wartendes Spiel ohne Feld, Farbe, Besetzung");
    async fn bis_sichtbar(
        pw: Option<&str>,
        id: i64,
        ok: impl Fn(&BtpMatch) -> bool,
    ) -> Option<u128> {
        let t0 = std::time::Instant::now();
        for _ in 0..40 {
            let s = snapshot(pw).await;
            if s.matches.iter().any(|m| m.id == id && ok(m)) {
                return Some(t0.elapsed().as_millis());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        None
    }
    for runde in 1..=3 {
        sende(farbe(&m, 3, &session, pw)).await;
        let f = bis_sichtbar(pw, m.id, |x| x.highlight == 3).await;
        let mut besetzt = m.clone();
        besetzt.official1_id = Some(ids[0]);
        besetzt.official2_id = Some(ids[1]);
        sende(officials_write(&besetzt, Some(3), &session, pw)).await;
        let o = bis_sichtbar(pw, m.id, |x| x.official1_id == Some(ids[0])).await;
        println!("Runde {runde}: Farbe sichtbar nach {f:?} ms, Besetzung nach {o:?} ms");
        sende(officials_write(&m, Some(0), &session, pw)).await;
        bis_sichtbar(pw, m.id, |x| x.official1_id.is_none() && x.highlight == 0).await;
    }
    println!("Endstand: {:?}", stand(pw, m.id).await);
}
