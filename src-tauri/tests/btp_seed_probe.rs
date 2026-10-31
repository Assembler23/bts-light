//! **Messwerkzeug, kein Regressionstest.** Liefert BTP die Setzplätze — und
//! wenn ja, wo und in welcher Form?
//!
//! Hintergrund: Die Struktur-Probe vom 12.08.2026 (`checkin_roster_probe.rs`)
//! hat am `StageEntry` ein optionales Feld `Seed1` gesehen, aber nie
//! ausgewertet. Bevor die Setznummer irgendwo angezeigt wird, klärt diese
//! Probe:
//!
//! 1. Welche Felder mit „Seed" im Namen gibt es **irgendwo** im Baum
//!    (`Seed1`, `Seed2`, an `Entry`, `Draw`, …) — samt Werttyp.
//! 2. Sind sie bei gesetzten Meldungen tatsächlich gefüllt, und bei
//!    ungesetzten leer oder `0`?
//! 3. Wie codiert BTP geteilte Setzungen (3/4, 5/8): gleiche Nummer,
//!    fortlaufende Nummer oder ein zweites Feld?
//! 4. Hängt die Setzung je Stage (Hauptfeld vs. Quali) oder je Meldung?
//!
//! Für eine aussagekräftige Messung braucht das Turnier mindestens eine
//! KO-Konkurrenz mit Setzliste inklusive geteilter Plätze (z. B. 1, 2, 3/4).
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test btp_seed_probe -- --ignored --nocapture
//! ```
//!
//! Read-only: nur `SENDTOURNAMENTINFO`. Ausgabe enthält **keine
//! Spielernamen**, nur IDs, Konkurrenz und Stage.

use bts_light_lib::btp::{client, proto, xml};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

fn child<'a>(n: &'a xml::Node, id: &str) -> Option<&'a xml::Node> {
    n.children().iter().find(|c| c.id() == id)
}
fn child_int(n: &xml::Node, id: &str) -> Option<i64> {
    child(n, id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_int())
}
fn child_str(n: &xml::Node, id: &str) -> String {
    child(n, id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
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

/// Rohwert samt Typ — gerade der Typ ist die Frage („3/4" als String?).
fn roh(v: Option<&xml::Value>) -> String {
    match v {
        Some(xml::Value::Integer(i)) => format!("Int {i}"),
        Some(xml::Value::String(s)) => format!("Str \"{s}\""),
        Some(xml::Value::Float(f)) => format!("Float {f}"),
        Some(xml::Value::Bool(b)) => format!("Bool {b}"),
        Some(xml::Value::DateTime(d)) => format!("DateTime {d:?}"),
        None => "Gruppe".into(),
    }
}

fn ist_seed_feld(id: &str) -> bool {
    id.to_ascii_lowercase().contains("seed")
}

/// Sammelt jedes Feld mit „seed" im Namen über den ganzen Baum:
/// Pfad (Container-Kette, ohne Listen-Indizes) → (Anzahl, Werte-Beispiele).
fn sammle_seed_felder(
    nodes: &[xml::Node],
    pfad: &mut Vec<String>,
    out: &mut BTreeMap<String, (usize, BTreeSet<String>)>,
) {
    for n in nodes {
        if ist_seed_feld(n.id()) {
            let key = format!("{}/{}", pfad.join("/"), n.id());
            let e = out.entry(key).or_default();
            e.0 += 1;
            if e.1.len() < 12 {
                e.1.insert(roh(n.value()));
            }
        }
        if !n.children().is_empty() {
            pfad.push(n.id().to_string());
            sammle_seed_felder(n.children(), pfad, out);
            pfad.pop();
        }
    }
}

fn stage_label(ty: Option<i64>) -> &'static str {
    match ty {
        Some(1) => "Hauptfeld",
        Some(2) => "Qualifikation",
        Some(8) => "Playoff",
        Some(9998) => "Reserve",
        Some(9999) => "Ausschließen",
        Some(_) => "sonstiger Typ",
        None => "ohne Stage",
    }
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn where_are_the_seeds() {
    let pw = password();
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(pw.as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");

    // 1. Alle Seed-Felder im ganzen Baum.
    println!("\n=== Felder mit „seed“ im Namen (Pfad · Anzahl · Werte-Beispiele) ===");
    let mut felder = BTreeMap::new();
    sammle_seed_felder(&nodes, &mut Vec::new(), &mut felder);
    if felder.is_empty() {
        println!("  (keine — BTP liefert in diesem Turnier gar kein Seed-Feld)");
    }
    for (pfad, (n, werte)) in &felder {
        let w: Vec<&str> = werte.iter().map(String::as_str).collect();
        println!("  {pfad:<45} {n:>5}×   {}", w.join(" | "));
    }

    // 2. Alle Felder an StageEntry — falls die Setzung anders heißt.
    println!("\n=== StageEntry: alle Felder (Name : Anzahl) ===");
    let mut se_felder: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(g) = find(&nodes, "StageEntries") {
        for se in g.children() {
            for c in se.children() {
                *se_felder.entry(c.id().to_string()).or_insert(0) += 1;
            }
        }
    }
    for (name, n) in &se_felder {
        println!("  {name:<20} {n}");
    }

    // Nachschlagetabellen.
    let stage_info: HashMap<i64, (Option<i64>, String)> = find(&nodes, "Stages")
        .map(|g| {
            g.children()
                .iter()
                .filter_map(|s| {
                    Some((
                        child_int(s, "ID")?,
                        (child_int(s, "StageType"), child_str(s, "Name")),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let event_name: HashMap<i64, String> = find(&nodes, "Events")
        .map(|g| {
            g.children()
                .iter()
                .filter_map(|e| Some((child_int(e, "ID")?, child_str(e, "Name"))))
                .collect()
        })
        .unwrap_or_default();
    let entry_event: HashMap<i64, i64> = find(&nodes, "Entries")
        .map(|g| {
            g.children()
                .iter()
                .filter_map(|e| Some((child_int(e, "ID")?, child_int(e, "EventID")?)))
                .collect()
        })
        .unwrap_or_default();
    let draw_name: HashMap<i64, String> = find(&nodes, "Draws")
        .map(|g| {
            g.children()
                .iter()
                .filter_map(|d| Some((child_int(d, "ID")?, child_str(d, "Name"))))
                .collect()
        })
        .unwrap_or_default();
    // In welchen Draws taucht eine Meldung in einem Match-Slot auf?
    let mut entry_draws: HashMap<i64, BTreeSet<String>> = HashMap::new();
    if let Some(g) = find(&nodes, "Matches") {
        for m in g.children() {
            if let (Some(entry), Some(draw)) = (child_int(m, "EntryID"), child_int(m, "DrawID")) {
                entry_draws.entry(entry).or_default().insert(
                    draw_name
                        .get(&draw)
                        .cloned()
                        .unwrap_or(format!("Draw {draw}")),
                );
            }
        }
    }

    // 3. Gesetzte Meldungen je Konkurrenz: alle StageEntries, bei denen
    //    irgendein Seed-Feld einen Wert ungleich 0/leer trägt.
    println!("\n=== Gesetzte Meldungen je Konkurrenz ===");
    let mut je_event: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ungesetzt_mit_feld = 0usize;
    let mut ungesetzt_ohne_feld = 0usize;
    if let Some(g) = find(&nodes, "StageEntries") {
        for se in g.children() {
            let seeds: Vec<String> = se
                .children()
                .iter()
                .filter(|c| ist_seed_feld(c.id()))
                .map(|c| format!("{}={}", c.id(), roh(c.value())))
                .collect();
            let gesetzt = se.children().iter().any(|c| {
                ist_seed_feld(c.id())
                    && match c.value() {
                        Some(xml::Value::Integer(i)) => *i != 0,
                        Some(xml::Value::String(s)) => !s.trim().is_empty() && s.trim() != "0",
                        Some(_) => true,
                        None => false,
                    }
            });
            if !gesetzt {
                if seeds.is_empty() {
                    ungesetzt_ohne_feld += 1;
                } else {
                    ungesetzt_mit_feld += 1;
                }
                continue;
            }
            let entry = child_int(se, "EntryID");
            let stage = child_int(se, "StageID");
            let (sty, sname) = stage
                .and_then(|s| stage_info.get(&s).cloned())
                .unwrap_or((None, String::new()));
            let ev = entry
                .and_then(|e| entry_event.get(&e))
                .and_then(|ev| event_name.get(ev).cloned())
                .unwrap_or_else(|| "(Event unbekannt)".into());
            let draws = entry
                .and_then(|e| entry_draws.get(&e))
                .map(|d| d.iter().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "–".into());
            je_event.entry(ev).or_default().push(format!(
                "Entry={:<6} Stage={} „{}“ Status={:<4} {}   Draws: {}",
                entry.map(|v| v.to_string()).unwrap_or("-".into()),
                stage_label(sty),
                sname,
                child_int(se, "Status")
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                seeds.join("  "),
                draws,
            ));
        }
    }
    if je_event.is_empty() {
        println!("  (keine gesetzte Meldung gefunden)");
    }
    for (ev, zeilen) in &je_event {
        println!("  {ev}  ({} gesetzt)", zeilen.len());
        for z in zeilen {
            println!("      {z}");
        }
    }
    println!(
        "\nUngesetzt: {ungesetzt_mit_feld} StageEntries mit Seed-Feld = 0/leer, \
         {ungesetzt_ohne_feld} ganz ohne Seed-Feld."
    );
}
