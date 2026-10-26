//! Messwerkzeug: Stimmen die Match-IDs des BTP-Protokolls mit `PlayerMatch.id`
//! in der TP-Datei überein, und reist die Turnier-GUID (`LinkCode`) im
//! Snapshot mit? Grundlage der Spec `tp-datei-upload` (Annahme A3).
//!
//! Die Sonde zeigt nur die **Protokollseite**. Die Dateiseite (Jet-Datenbank)
//! wurde ad hoc mit `access-parser` gelesen, siehe `docs/btp_protocol.md`.
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test btp_tp_match_probe -- --ignored --nocapture
//! ```
//!
//! Read-only: nur `SENDTOURNAMENTINFO`. Ausgabe ohne Spielernamen (ein
//! `Match`-Knoten trägt nur IDs, Zahlen und Zeiten).

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

fn child<'a>(n: &'a xml::Node, id: &str) -> Option<&'a xml::Node> {
    n.children().iter().find(|c| c.id() == id)
}
fn int(n: &xml::Node, id: &str) -> Option<i64> {
    child(n, id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_int())
}

/// Rohwert eines Knotens; Gruppen (Knoten mit Kindern) werden als solche
/// benannt, damit `Sets` nicht wie ein leeres Feld aussieht.
fn roh(n: &xml::Node) -> String {
    match n.value() {
        Some(xml::Value::Integer(i)) => i.to_string(),
        Some(xml::Value::String(s)) => format!("\"{s}\""),
        Some(xml::Value::Float(f)) => f.to_string(),
        Some(xml::Value::Bool(b)) => b.to_string(),
        Some(xml::Value::DateTime(d)) => format!("{d:?}"),
        None if !n.children().is_empty() => format!("Gruppe({} Kinder)", n.children().len()),
        None => "·".into(),
    }
}

fn zelle(n: &xml::Node, id: &str) -> String {
    child(n, id).map(roh).unwrap_or_else(|| "·".into())
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

    let Some(matches) = find(&nodes, "Matches") else {
        println!(
            "\n  kein Matches-Container — Liga-Modus (PlayerMatches)? oder Turnier ohne Auslosung"
        );
        return;
    };
    let liste: Vec<&xml::Node> = matches
        .children()
        .iter()
        .filter(|m| m.id() == "Match")
        .collect();
    let ids: Vec<i64> = liste.iter().filter_map(|m| int(m, "ID")).collect();
    let echte = liste
        .iter()
        .filter(|m| child(m, "IsMatch").and_then(|c| c.value()) == Some(&xml::Value::Bool(true)))
        .count();
    println!(
        "\n=== Match-Zeilen (inkl. Slots/Spiegel): {} Stück, davon IsMatch=true: {}, ID-Bereich {}..{} ===",
        liste.len(),
        echte,
        ids.iter().min().map(|i| i.to_string()).unwrap_or_default(),
        ids.iter().max().map(|i| i.to_string()).unwrap_or_default(),
    );

    println!("\n=== Felder der Matches mit ID 5 und ID 1 (Vergleich mit PlayerMatch) ===");
    for ziel in [5, 1] {
        match liste.iter().find(|m| int(m, "ID") == Some(ziel)) {
            Some(m) => {
                let felder: Vec<String> = m
                    .children()
                    .iter()
                    .map(|c| format!("{}={}", c.id(), roh(c)))
                    .collect();
                println!("  ID {ziel}: {}", felder.join("  "));
            }
            None => println!("  ID {ziel}: nicht im Snapshot"),
        }
    }

    println!(
        "\n=== Erste 8 Match-Zeilen: ID · DrawID · Round · MatchNr · From1/From2 · CourtID ==="
    );
    for m in liste.iter().take(8) {
        println!(
            "  {:>5} {:>4} {:>3} {:>4} {:>5}/{:<5} {}",
            zelle(m, "ID"),
            zelle(m, "DrawID"),
            zelle(m, "Round"),
            zelle(m, "MatchNr"),
            zelle(m, "From1"),
            zelle(m, "From2"),
            zelle(m, "CourtID"),
        );
    }
}
