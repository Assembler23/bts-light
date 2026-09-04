//! Schreibt ein Muster des Hallen-Aushangs nach `aushang-probe.html`.
//!
//! Das Blatt ist auf 297 mm gerechnet, und ob es nach einer Textänderung
//! noch passt, sieht man erst im Browser. Rust-Tests prüfen Inhalt und
//! Escaping — die Höhe prüft dieses Muster:
//!
//! ```text
//! cargo run --example aushang_probe [zieldatei] [--mit-logo]
//! # dann die Datei im Browser öffnen und Strg+P (A4 hoch)
//! ```
//!
//! `--mit-logo` ist der **enge** Fall: ein breites Logo macht die Kopfzeile
//! 14 mm hoch und schiebt alles nach unten. Wer Texte oder Maße ändert,
//! prüft beide Varianten — ohne Logo passt fast immer, mit Logo ist die
//! Reserve klein.
//!
//! Doku: `docs/aushang.md`.

/// Breites Platzhalter-Logo (4:1) als `data:`-URI — reizt `max-width` und
/// die Kopfhöhe aus, ohne eine Bilddatei ins Repo zu legen.
const TEST_LOGO: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAQAAAABCAYAAAD5PA/NAAAAD0lEQVR4AWMUjSv9z4AEABpNAemNSDkeAAAAAElFTkSuQmCC";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mit_logo = args.iter().any(|a| a == "--mit-logo");
    let frei: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let ziel = frei
        .first()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "aushang-probe.html".to_string());
    // Bewusst lang: ein kurzer Name deckt den Umbruch im Kopf nicht auf.
    // Ein eigener Name als zweites Argument prüft den Deckel auf zwei Zeilen.
    let turnier = frei
        .get(1)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "BVBB Ranglistenturnier U19 – Berlin".to_string());

    let daten = bts_light_lib::aushang::daten_aus(
        "https://badhub.de/live?t=bvbb",
        &turnier,
        mit_logo.then(|| TEST_LOGO.to_string()),
        None,
    )
    .expect("Muster-URL ist auswertbar");
    let html = bts_light_lib::aushang::render_html(&daten).expect("Blatt lässt sich bauen");
    std::fs::write(&ziel, html).expect("Datei schreiben");
    println!("{ziel} geschrieben — im Browser öffnen und drucken (A4 hoch).");
}
