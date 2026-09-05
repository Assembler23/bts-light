//! **Messwerkzeug, kein Regressionstest.** Wie codiert BTP die Farben aus
//! „Hervorheben" im Feld `Match.Highlight` — und nimmt BTP eine solche
//! Farbe per `SENDUPDATE` an?
//!
//! Hintergrund: bts-light schreibt `Highlight:1`/`0` schon für
//! Vorbereitungs-Aufrufe (P1), liest das Feld aber nicht. Die
//! Turnierleitungs-Seite soll die BTP-Farben zeigen und setzen können.
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test btp_highlight_probe -- --ignored --nocapture
//! ```
//!
//! Ausgabe enthält **keine Spielernamen**, nur IDs, Konkurrenz und Runde.

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
    xml::find(n.children(), id)?.value()?.as_int()
}

fn kind_str(n: &xml::Node, id: &str) -> String {
    xml::find(n.children(), id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn snapshot() -> Vec<xml::Node> {
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(password().as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    proto::decode_response(&raw).expect("dekodierbar")
}

#[derive(Debug, Clone)]
struct Zeile {
    id: i64,
    draw_id: i64,
    planning_id: i64,
    draw: String,
    nr: i64,
    runde: String,
    highlight: i64,
    status: i64,
    court_id: Option<i64>,
}

fn zeilen(nodes: &[xml::Node]) -> Vec<Zeile> {
    let draws: std::collections::HashMap<i64, String> = suche(nodes, "Draws")
        .map(|d| {
            d.children()
                .iter()
                .filter_map(|n| Some((kind_int(n, "ID")?, kind_str(n, "Name"))))
                .collect()
        })
        .unwrap_or_default();
    let Some(matches) = suche(nodes, "Matches") else {
        return vec![];
    };
    matches
        .children()
        .iter()
        .filter_map(|m| {
            let draw_id = kind_int(m, "DrawID").unwrap_or(0);
            Some(Zeile {
                id: kind_int(m, "ID")?,
                draw_id,
                planning_id: kind_int(m, "PlanningID").unwrap_or(0),
                draw: draws.get(&draw_id).cloned().unwrap_or_default(),
                nr: kind_int(m, "MatchNr").unwrap_or(0),
                runde: kind_str(m, "RoundName"),
                highlight: kind_int(m, "Highlight").unwrap_or(-1),
                status: kind_int(m, "Status").unwrap_or(-1),
                court_id: kind_int(m, "CourtID"),
            })
        })
        .collect()
}

fn drucke(z: &Zeile) {
    println!(
        "  Match {:>6}  Draw {:<20} #{:<3} {:<10} Highlight={:>4} (0x{:06X})  Status={} Court={:?}",
        z.id, z.draw, z.nr, z.runde, z.highlight, z.highlight, z.status, z.court_id
    );
}

/// Liest, welche Zahl BTP für welche gesetzte Farbe liefert. Vorher im
/// BTP ein paar Spiele mit verschiedenen Farben markieren.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn which_highlight_values_does_btp_send() {
    let nodes = snapshot().await;
    let alle = zeilen(&nodes);
    let mut verteilung: std::collections::BTreeMap<i64, usize> = Default::default();
    for z in &alle {
        *verteilung.entry(z.highlight).or_default() += 1;
    }
    println!("\n=== Highlight-Verteilung über {} Matches ===", alle.len());
    for (wert, n) in &verteilung {
        println!("  {n:>5} × Highlight={wert} (0x{wert:06X})");
    }
    println!("\n=== Markierte Spiele ===");
    for z in alle.iter().filter(|z| z.highlight != 0) {
        drucke(z);
    }
}

/// Minimaler `SENDUPDATE` nur mit Identität + `Highlight=<wert>` — Bauart
/// wie `proto::highlight_request`, aber mit freiem Wert. Bewusst hier und
/// nicht in `proto.rs`: reine Mess-Hilfe.
fn highlight_value_request(z: &Zeile, wert: i64, session_key: &str, pw: Option<&str>) -> Vec<u8> {
    let mut action = vec![
        xml::Node::string("ID", "SENDUPDATE"),
        xml::Node::string("Unicode", session_key),
    ];
    if let Some(pw) = pw {
        action.push(xml::Node::string("Password", pw));
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
                            xml::Node::integer("ID", z.id),
                            xml::Node::integer("DrawID", z.draw_id),
                            xml::Node::integer("PlanningID", z.planning_id),
                            xml::Node::integer("Highlight", wert),
                        ],
                    )],
                )],
            )],
        ),
    ];
    wire::encode_message(&xml::encode(&nodes))
}

async fn schreibe(z: &Zeile, wert: i64, pw: Option<&str>) {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("LOGIN");
    let session = proto::parse_login_response(&proto::decode_response(&raw).unwrap())
        .expect("LOGIN akzeptiert");
    let req = highlight_value_request(z, wert, &session, pw);
    let raw = client::send_request(&host(), port(), &req)
        .await
        .expect("SENDUPDATE");
    let antwort = proto::decode_response(&raw).unwrap();
    match proto::parse_update_response(&antwort) {
        Ok(()) => println!("  SENDUPDATE Match {} ← Highlight={wert}: Result=1", z.id),
        Err(e) => println!("  SENDUPDATE Match {} ← Highlight={wert}: FEHLER {e}", z.id),
    }
}

async fn lies(id: i64) -> Option<Zeile> {
    zeilen(&snapshot().await).into_iter().find(|z| z.id == id)
}

/// Schreibt probeweise eine Farbe und stellt sie wieder zurück.
///
/// Ablauf: markiertes Spiel A (Wert V) als Farbquelle; unmarkiertes Spiel
/// B ohne Feld als Ziel. B←V → nachlesen → B←0 → nachlesen. Danach A←0 →
/// nachlesen → A←V (misst, ob auch das Löschen einer BTP-Farbe hält).
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_btp_accept_a_highlight_colour_write() {
    let pw = password();
    let vorher = zeilen(&snapshot().await);
    let a = vorher
        .iter()
        .find(|z| z.highlight != 0)
        .cloned()
        .expect("mindestens ein in BTP markiertes Spiel");
    let b = vorher
        .iter()
        .find(|z| z.highlight == 0 && z.court_id.is_none() && z.draw_id == a.draw_id)
        .or_else(|| {
            vorher
                .iter()
                .find(|z| z.highlight == 0 && z.court_id.is_none())
        })
        .cloned()
        .expect("ein unmarkiertes Spiel ohne Feld");
    println!("\nQuelle A:");
    drucke(&a);
    println!("Ziel B:");
    drucke(&b);

    schreibe(&b, a.highlight, pw.as_deref()).await;
    let b1 = lies(b.id).await.unwrap();
    println!(
        "  nachgelesen B: Highlight={} Status={}",
        b1.highlight, b1.status
    );
    schreibe(&b, 0, pw.as_deref()).await;
    let b2 = lies(b.id).await.unwrap();
    println!(
        "  nachgelesen B: Highlight={} Status={}",
        b2.highlight, b2.status
    );

    schreibe(&a, 0, pw.as_deref()).await;
    let a1 = lies(a.id).await.unwrap();
    println!(
        "  nachgelesen A: Highlight={} Status={}",
        a1.highlight, a1.status
    );
    schreibe(&a, a.highlight, pw.as_deref()).await;
    let a2 = lies(a.id).await.unwrap();
    println!(
        "  nachgelesen A: Highlight={} Status={}",
        a2.highlight, a2.status
    );

    println!("\n=== Befund ===");
    println!(
        "  Setzen B 0→{}: {}",
        a.highlight,
        if b1.highlight == a.highlight {
            "HÄLT"
        } else {
            "verworfen"
        }
    );
    println!(
        "  Löschen B →0:  {}",
        if b2.highlight == 0 {
            "HÄLT"
        } else {
            "verworfen"
        }
    );
    println!(
        "  Löschen A →0:  {}",
        if a1.highlight == 0 {
            "HÄLT"
        } else {
            "verworfen"
        }
    );
    println!(
        "  Nebenwirkung Status: B {}→{}→{}, A {}→{}→{}",
        b.status, b1.status, b2.status, a.status, a1.status, a2.status
    );
    assert_eq!(b2.highlight, 0, "B nicht zurückgestellt");
    assert_eq!(a2.highlight, a.highlight, "A nicht zurückgestellt");
}

/// Schreibt EINE Farbe (Umgebung `BTP_HL_VALUE`, Standard 3 = Orange) an ein
/// unmarkiertes Spiel ohne Feld (oder `BTP_HL_MATCH`) und lässt sie stehen —
/// zur Sichtprüfung im BTP-Fenster, ob eine über das Netz geschriebene
/// Farbe dort erscheint. Zurücksetzen: dieselbe Probe mit `BTP_HL_VALUE=0`
/// und `BTP_HL_MATCH=<ID>`.
#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT und lässt stehen"]
async fn set_one_highlight_for_visual_check() {
    let pw = password();
    let wert: i64 = std::env::var("BTP_HL_VALUE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let gewuenscht: Option<i64> = std::env::var("BTP_HL_MATCH")
        .ok()
        .and_then(|v| v.parse().ok());
    let alle = zeilen(&snapshot().await);
    let ziel = match gewuenscht {
        Some(id) => alle
            .iter()
            .find(|z| z.id == id)
            .cloned()
            .expect("Match-ID vorhanden"),
        None => alle
            .iter()
            .find(|z| z.highlight == 0 && z.court_id.is_none() && z.nr > 0 && !z.runde.is_empty())
            .cloned()
            .expect("unmarkiertes Spiel ohne Feld"),
    };
    println!("\nZiel:");
    drucke(&ziel);
    schreibe(&ziel, wert, pw.as_deref()).await;
    let danach = lies(ziel.id).await.unwrap();
    println!("  nachgelesen: Highlight={}", danach.highlight);
}
