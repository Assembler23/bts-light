//! Messwerkzeug: Stimmen die Match-IDs des BTP-Protokolls mit `PlayerMatch.id`
//! in der TP-Datei überein, und reist die Turnier-GUID (`LinkCode`) im
//! Snapshot mit? Grundlage der Spec `tp-datei-upload` (Annahme A3).
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test btp_tp_match_probe -- --ignored --nocapture
//! ```
//!
//! Read-only: nur `SENDTOURNAMENTINFO`. Ausgabe ohne Spielernamen.

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

fn find<'a>(nodes: &'a [xml::Node], id: &str) -> Option<&'a xml::Node> {
    for n in nodes {
        if n.id() == id {
            return Some(n);
        }
        if let Some(t) = find(n.children(), id) {
            return Some(t);
        }
    }
    None
}

fn roh(v: Option<&xml::Value>) -> String {
    match v {
        Some(xml::Value::Integer(i)) => i.to_string(),
        Some(xml::Value::String(s)) => format!("\"{s}\""),
        Some(xml::Value::Float(f)) => f.to_string(),
        Some(xml::Value::Bool(b)) => b.to_string(),
        Some(xml::Value::DateTime(d)) => format!("{d:?}"),
        None => "·".into(),
    }
}

fn int(n: &xml::Node, id: &str) -> Option<i64> {
    n.children()
        .iter()
        .find(|c| c.id() == id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_int())
}

/// Sammelt alle String-Werte, die wie eine GUID aussehen (Pfad → Wert).
fn guids(nodes: &[xml::Node], pfad: &mut Vec<String>, out: &mut Vec<(String, String)>) {
    for n in nodes {
        if let Some(xml::Value::String(s)) = n.value() {
            let t = s.trim().trim_matches(|c| c == '{' || c == '}');
            if t.len() == 36 && t.matches('-').count() == 4 {
                out.push((format!("{}/{}", pfad.join("/"), n.id()), t.to_string()));
            }
        }
        if !n.children().is_empty() {
            pfad.push(n.id().to_string());
            guids(n.children(), pfad, out);
            pfad.pop();
        }
    }
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn match_ids_und_guid_gegen_tp_datei() {
    let pw = password();
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(pw.as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");

    let mut g = Vec::new();
    guids(&nodes, &mut Vec::new(), &mut g);
    println!("\n=== GUID-artige Strings im Snapshot ===");
    if g.is_empty() {
        println!("  (keine)");
    }
    for (p, v) in &g {
        println!("  {p:<50} {v}");
    }

    let matches = find(&nodes, "Matches").expect("Matches-Container");
    let liste: Vec<&xml::Node> = matches
        .children()
        .iter()
        .filter(|m| m.id() == "Match")
        .collect();
    let ids: Vec<i64> = liste.iter().filter_map(|m| int(m, "ID")).collect();
    println!(
        "\n=== Matches: {} Stück, ID-Bereich {:?}..{:?} ===",
        liste.len(),
        ids.iter().min(),
        ids.iter().max()
    );

    println!("\n=== Felder der Matches mit ID 5 und ID 1 (Vergleich mit PlayerMatch) ===");
    for ziel in [5, 1] {
        match liste.iter().find(|m| int(m, "ID") == Some(ziel)) {
            Some(m) => {
                let felder: Vec<String> = m
                    .children()
                    .iter()
                    .map(|c| format!("{}={}", c.id(), roh(c.value())))
                    .collect();
                println!("  ID {ziel}: {}", felder.join("  "));
            }
            None => println!("  ID {ziel}: nicht im Snapshot"),
        }
    }

    println!("\n=== Erste 8 Matches: ID · DrawID · RoundNr · MatchNr · Van1/Van2 · Court ===");
    for m in liste.iter().take(8) {
        println!(
            "  {:>5} {:>4} {:>3} {:>4} {:>5}/{:<5} {:?}",
            roh(m
                .children()
                .iter()
                .find(|c| c.id() == "ID")
                .and_then(|c| c.value())),
            roh(m
                .children()
                .iter()
                .find(|c| c.id() == "DrawID")
                .and_then(|c| c.value())),
            roh(m
                .children()
                .iter()
                .find(|c| c.id() == "RoundNr")
                .and_then(|c| c.value())),
            roh(m
                .children()
                .iter()
                .find(|c| c.id() == "MatchNr")
                .and_then(|c| c.value())),
            roh(m
                .children()
                .iter()
                .find(|c| c.id() == "Van1")
                .and_then(|c| c.value())),
            roh(m
                .children()
                .iter()
                .find(|c| c.id() == "Van2")
                .and_then(|c| c.value())),
            int(m, "CourtID"),
        );
    }
}
