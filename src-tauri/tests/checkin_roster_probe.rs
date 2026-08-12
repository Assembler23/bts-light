//! **Messwerkzeug, kein Regressionstest.** Zeigt je Spielklasse, wie viele
//! Meldungen roh in BTP stehen und wie viele nach dem Hauptfeld-Filter
//! (`non_main_stage_entries`, seit v0.9.185) auf der Check-In-Liste landen —
//! samt Aufschlüsselung nach Stage-Typ (Hauptfeld / Quali / Reserve /
//! Ausschließen).
//!
//! Zweck: die am Mitschnitt gemessenen `StageType`-Werte (1/2/8/9998/9999)
//! gegen ein echtes Turnier gegenprüfen und die erwarteten Zahlen bestätigen
//! (Beispiel HE-C: 26 Hauptfeld, 1 Reserve, 4 Ausschließen).
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein laufendes BTP:
//!
//! ```text
//! cargo test -p bts-light --test checkin_roster_probe -- --ignored --nocapture
//! ```
//!
//! Read-only: nur `SENDTOURNAMENTINFO`. Keine Spielernamen in der Ausgabe.

use bts_light_lib::btp::{client, proto, xml};
use std::collections::{BTreeMap, HashMap};

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

fn child_int(n: &xml::Node, id: &str) -> Option<i64> {
    n.children()
        .iter()
        .find(|c| c.id() == id)
        .and_then(|c| c.value())
        .and_then(|v| v.as_int())
}
fn child_str(n: &xml::Node, id: &str) -> String {
    n.children()
        .iter()
        .find(|c| c.id() == id)
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
async fn checkin_roster_counts_per_event() {
    let pw = password();
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(pw.as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");

    // Stage-Typen roh anzeigen — bestätigt, ob die Konstanten passen.
    println!("\n=== Stages (ID · StageType · Name) ===");
    let mut stage_type: HashMap<i64, i64> = HashMap::new();
    if let Some(g) = find(&nodes, "Stages") {
        for s in g.children() {
            let id = child_int(s, "ID");
            let ty = child_int(s, "StageType");
            println!(
                "  ID={:<5} Type={:<6} ({}) {}",
                id.map(|v| v.to_string()).unwrap_or("-".into()),
                ty.map(|v| v.to_string()).unwrap_or("-".into()),
                stage_label(ty),
                child_str(s, "Name")
            );
            if let (Some(id), Some(ty)) = (id, ty) {
                stage_type.insert(id, ty);
            }
        }
    } else {
        println!("  (kein Stages-Container — dann kann nichts gefiltert werden!)");
    }

    // Event-Namen.
    let mut event_name: HashMap<i64, String> = HashMap::new();
    if let Some(g) = find(&nodes, "Events") {
        for e in g.children() {
            if let Some(id) = child_int(e, "ID") {
                event_name.insert(id, child_str(e, "Name"));
            }
        }
    }

    // Roh-Gesamt je Event (alle Meldungen).
    let mut roh_total: BTreeMap<i64, i64> = BTreeMap::new();
    if let Some(g) = find(&nodes, "Entries") {
        for e in g.children() {
            if let Some(ev) = child_int(e, "EventID") {
                *roh_total.entry(ev).or_insert(0) += 1;
            }
        }
    }
    // Stage-Typ-Verteilung je Event über StageEntries (die maßgebliche
    // Zuordnung; Entry.StageID ist in echten Daten leer).
    let entry_event: HashMap<i64, i64> = find(&nodes, "Entries")
        .map(|g| {
            g.children()
                .iter()
                .filter_map(|e| Some((child_int(e, "ID")?, child_int(e, "EventID")?)))
                .collect()
        })
        .unwrap_or_default();
    let mut roh: BTreeMap<i64, BTreeMap<&'static str, i64>> = BTreeMap::new();
    if let Some(g) = find(&nodes, "StageEntries") {
        for se in g.children() {
            let (Some(entry), Some(stage)) = (child_int(se, "EntryID"), child_int(se, "StageID"))
            else {
                continue;
            };
            let Some(&ev) = entry_event.get(&entry) else {
                continue;
            };
            let ty = stage_type.get(&stage).copied();
            *roh.entry(ev)
                .or_default()
                .entry(stage_label(ty))
                .or_insert(0) += 1;
        }
    }

    // Gefiltert: das, was bts-light nach dem Hauptfeld-Filter tatsächlich
    // auf die Check-In-Liste schickt (parse_snapshot wendet ihn an).
    let snap = bts_light_lib::btp::model::parse_snapshot(&nodes).expect("Snapshot");
    let mut gefiltert: BTreeMap<i64, i64> = BTreeMap::new();
    for entry in &snap.entries {
        *gefiltert.entry(entry.event_id).or_insert(0) += 1;
    }

    println!("\n=== Meldungen je Klasse: roh → Check-In-Liste ===");
    println!(
        "{:<28} {:>5} {:>9} {:>8}   Aufschlüsselung roh",
        "Klasse", "roh", "Check-In", "raus"
    );
    let mut events: Vec<i64> = roh_total.keys().copied().collect();
    events.sort_by_key(|ev| event_name.get(ev).cloned().unwrap_or_default());
    for ev in events {
        let total = *roh_total.get(&ev).unwrap_or(&0);
        let g = *gefiltert.get(&ev).unwrap_or(&0);
        let name = event_name
            .get(&ev)
            .cloned()
            .unwrap_or_else(|| format!("Event {ev}"));
        let auf: Vec<String> = roh
            .get(&ev)
            .map(|m| m.iter().map(|(k, v)| format!("{v}× {k}")).collect())
            .unwrap_or_default();
        println!(
            "{:<28} {:>5} {:>9} {:>8}   {}",
            name,
            total,
            g,
            total - g,
            auf.join(", ")
        );
    }

    println!(
        "\nGesamt: {} Meldungen roh, {} auf der Check-In-Liste.",
        snap.entries
            .len()
            .max(roh_total.values().sum::<i64>() as usize),
        snap.entries.len()
    );
}

/// **Struktur-Dump — kein Test.** Sucht, WO BTP die Reserve-/Ausschließen-
/// Zugehörigkeit einer Meldung ablegt, wenn nicht an `Entry.StageID`.
///
/// ```text
/// cargo test -p bts-light --test checkin_roster_probe -- --ignored --nocapture where_is_the_stage_membership
/// ```
#[tokio::test]
#[ignore = "braucht ein laufendes BTP"]
async fn where_is_the_stage_membership() {
    let pw = password();
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(pw.as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    let tournament = find(&nodes, "Tournament").expect("Tournament");

    // 1. Alle Top-Level-Container unter Tournament (Name + Kinderzahl) —
    //    ein unbeachteter Container fällt hier auf.
    println!("\n=== Container unter Tournament ===");
    for c in tournament.children() {
        println!("  {:<22} {} Kinder", c.id(), c.children().len());
    }

    // 2. Feldnamen einer Meldung (über alle Entries gesammelt) — steht hier
    //    ein Stage-/Reserve-/Ausschließen-Bezug?
    println!("\n=== Felder an Entries (Name : Anzahl) ===");
    let mut felder: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(g) = find(&nodes, "Entries") {
        for e in g.children() {
            for c in e.children() {
                *felder.entry(c.id().to_string()).or_insert(0) += 1;
            }
        }
    }
    for (name, n) in &felder {
        println!("  {name:<20} {n}");
    }

    // 3. Draws: ID, StageID, EventID, Name, Größe — welche Draws hängen an
    //    Reserve-/Ausschließen-Stages, und stehen dort Meldungen drin?
    println!("\n=== Draws (ID · StageID · EventID · Größe · Name) ===");
    if let Some(g) = find(&nodes, "Draws") {
        for d in g.children() {
            println!(
                "  ID={:<5} Stage={:<5} Event={:<5} Größe={:<4} {}",
                child_int(d, "ID")
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                child_int(d, "StageID")
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                child_int(d, "EventID")
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                child_int(d, "Size")
                    .map(|v| v.to_string())
                    .unwrap_or("-".into()),
                child_str(d, "Name"),
            );
        }
    }

    // 4. StageEntries — der mutmaßliche Ort der Zuordnung Meldung → Stage.
    println!("\n=== StageEntries: Felder (Name : Anzahl) ===");
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
    println!("\n=== StageEntries: erste 5 Einträge samt Werten ===");
    if let Some(g) = find(&nodes, "StageEntries") {
        for se in g.children().iter().take(5) {
            let felder: Vec<String> = se
                .children()
                .iter()
                .map(|c| {
                    let v = match c.value() {
                        Some(xml::Value::Integer(i)) => i.to_string(),
                        Some(xml::Value::String(s)) => format!("\"{s}\""),
                        Some(other) => format!("{other:?}"),
                        None => "grp".into(),
                    };
                    format!("{}={v}", c.id())
                })
                .collect();
            println!("  {}", felder.join("  "));
        }
    }

    // 5. HE C (Event 11) über StageEntries aufschlüsseln — wenn das die
    //    richtige Quelle ist, muss hier 26 Hauptfeld / 1 Reserve / 4
    //    Ausschließen herauskommen.
    let stage_type: HashMap<i64, i64> = find(&nodes, "Stages")
        .map(|g| {
            g.children()
                .iter()
                .filter_map(|s| Some((child_int(s, "ID")?, child_int(s, "StageType")?)))
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
    println!("\n=== HE C (Event 11) je Stage-Typ laut StageEntries ===");
    let mut he_c: BTreeMap<&'static str, i64> = BTreeMap::new();
    if let Some(g) = find(&nodes, "StageEntries") {
        for se in g.children() {
            let (Some(entry), Some(stage)) = (child_int(se, "EntryID"), child_int(se, "StageID"))
            else {
                continue;
            };
            if entry_event.get(&entry) == Some(&11) {
                let ty = stage_type.get(&stage).copied();
                *he_c.entry(stage_label(ty)).or_insert(0) += 1;
            }
        }
    }
    for (k, v) in &he_c {
        println!("  {v:>3}× {k}");
    }
}
