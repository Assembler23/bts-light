//! Datenformat-Wächter (Spec `hotfix-ohne-neustart`, ADR 0101).
//!
//! Alte und neue Version teilen sich `config.json` und die Stores im
//! App-Datenverzeichnis. Ob man nach einem Update zur vorigen Version
//! **zurück** darf, hängt davon ab, ob die die Daten der neuen noch lesen
//! kann. Das sagt `release/datenformat.json` (`rueckwaerts_lesbar_ab`), und
//! der Release-Job schreibt es in den signierten Kern-Kopf.
//!
//! Damit diese Angabe nicht still veraltet, bildet dieser Test einen
//! **Abdruck** aller gespeicherten Formate und vergleicht ihn mit dem in
//! derselben Datei. Ändert sich ein Format, ist die CI rot, bis jemand
//! Abdruck **und** Grenze gemeinsam angefasst hat — der Reviewer sieht die
//! Entscheidung dann im selben Diff.
//!
//! Der Abdruck geht über die Definitionen aller Typen, die `Deserialize`
//! ableiten, in den Dateien der Liste unten (Kommentare und Leerraum
//! zählen nicht). Er erkennt Struktur — neue, entfernte, umbenannte Felder
//! und Varianten, geänderte serde-Attribute. Eine geänderte **Bedeutung**
//! bei gleicher Form erkennt er nicht; die bleibt Sache des Reviews.

use std::path::{Path, PathBuf};

/// Dateien mit gespeicherten Formaten, relativ zu `src-tauri/src/`. Wer
/// einen neuen Store anlegt, trägt seine Datei hier ein.
const DATEIEN: &[&str] = &[
    "config.rs",
    "seiten_paket.rs",
    "tablet/exclusion.rs",
    "tablet/hall_assign.rs",
    "tablet/match_times.rs",
    "tablet/meetingpoint.rs",
    "tablet/monitor.rs",
    "tablet/officials.rs",
    "tablet/print_log.rs",
    "tablet/queue_order.rs",
    "tablet/sheet.rs",
    "tablet/state.rs",
    "tablet/timeline.rs",
    "tablet/wish_court.rs",
    "tts_verbrauch.rs",
    "update.rs",
];

/// Dateien mit `Deserialize`-Typen, die **nicht** gespeichert werden
/// (Antworten von badhub, Anfragen, Wire-Hilfen). Jede Datei unter `src/`,
/// die `Deserialize` ableitet, muss in einer der beiden Listen stehen — so
/// kann kein neuer Store unbemerkt am Wächter vorbei entstehen.
const NICHT_GESPEICHERT: &[&str] = &[
    "ansage_ton/stimme.rs",
    "badhub/checkin_state.rs",
    "badhub/tp_upload.rs",
    "commands.rs",
    "seiten_angebot.rs",
    "tablet/assign.rs",
    "tablet/server.rs",
    "tablet/slave_bridge.rs",
    "tablet/tl.rs",
    "tablet/zeitplan.rs",
    "uebergabe.rs",
];

/// Typen aus `relay-proto`, die in gespeicherten Dateien stecken
/// (Punktverlauf, Zettel-Ereignisse, Meetingpoint). Bewusst eine
/// Namensliste statt der ganzen Datei: Die übrigen Wire-Typen ändern sich
/// oft und berühren keine Platte.
const RELAY_PROTO_TYPEN: &[&str] = &[
    "EventKind",
    "MatchEvent",
    "MatchTimeline",
    "MonitorStil",
    "MpOrt",
    "MpPlatz",
    "Phase",
    "PrepCallSide",
    "SetAb",
    "TimelineSet",
];

fn wurzel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Entfernt Zeilenkommentare und den Test-Teil einer Quelldatei.
fn ohne_kommentare_und_tests(quelle: &str) -> String {
    // Zeilenenden vereinheitlichen: Unter Windows checkt git mit CRLF aus —
    // der Abdruck muss dort derselbe sein wie unter Linux.
    let quelle = quelle.replace("\r\n", "\n");
    // Ab dem Test-Modul abschneiden — auch wenn es `pub(crate) mod` heißt.
    let marke = "\n#[cfg(test)]\n";
    let schnitt = quelle.match_indices(marke).find_map(|(i, _)| {
        let danach = quelle[i + marke.len()..].trim_start();
        ["mod ", "pub mod ", "pub(crate) mod ", "pub(super) mod "]
            .iter()
            .any(|anfang| danach.starts_with(anfang))
            .then_some(i)
    });
    let ohne_tests = match schnitt {
        Some(i) => &quelle[..i],
        None => &quelle,
    };
    ohne_tests
        .lines()
        .map(|zeile| match zeile.find("//") {
            Some(i) => &zeile[..i],
            None => zeile,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Die Definitionen aller Typen, die `Deserialize` ableiten — jeweils vom
/// `#[derive(…)]` bis zum Ende des Typs, Leerraum zusammengezogen.
fn definitionen(quelle: &str) -> Vec<String> {
    let text = ohne_kommentare_und_tests(quelle);
    let mut gefunden = Vec::new();
    let mut ab = 0;
    while let Some(rel) = text[ab..].find("#[derive(") {
        let start = ab + rel;
        let attr_ende = start
            + text[start..]
                .find(")]")
                .map_or(text.len() - start, |i| i + 2);
        ab = attr_ende;
        if !text[start..attr_ende].contains("Deserialize") {
            continue;
        }
        // Ende des Typs: die schließende Klammer zur ersten `{` — oder das
        // erste `;`, wenn es davor kommt (Tupel- und Einheits-Typen).
        let rest = &text[attr_ende..];
        let auf = rest.find('{');
        let semikolon = rest.find(';');
        let ende = match (auf, semikolon) {
            (Some(a), Some(s)) if s < a => attr_ende + s + 1,
            (Some(a), _) => {
                let mut tiefe = 0usize;
                let mut ende = text.len();
                for (i, c) in rest[a..].char_indices() {
                    match c {
                        '{' => tiefe += 1,
                        '}' => {
                            tiefe -= 1;
                            if tiefe == 0 {
                                ende = attr_ende + a + i + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                ende
            }
            (None, Some(s)) => attr_ende + s + 1,
            (None, None) => text.len(),
        };
        gefunden.push(
            text[start..ende]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        );
        ab = ende;
    }
    gefunden
}

fn sha256_hex(text: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, text.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn abdruck_von(dateien: &[(String, String)]) -> String {
    sha256_hex(&abdruck_text(dateien))
}

/// Der Text, über den der Abdruck gebildet wird.
fn abdruck_text(dateien: &[(String, String)]) -> String {
    let mut alles = String::new();
    for (name, quelle) in dateien {
        alles.push_str("== ");
        alles.push_str(name);
        alles.push('\n');
        for definition in definitionen(quelle) {
            alles.push_str(&definition);
            alles.push('\n');
        }
    }
    alles
}

/// Name des Typs einer Definition (`… struct Name {` / `… enum Name {`).
fn typname(definition: &str) -> Option<&str> {
    let mut worte = definition.split_whitespace();
    while let Some(wort) = worte.next() {
        if wort == "struct" || wort == "enum" {
            let name = worte.next()?;
            let ende = name
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(name.len());
            return Some(&name[..ende]);
        }
    }
    None
}

/// Die gespeicherten `relay-proto`-Typen als eigene „Datei" des Abdrucks.
fn relay_proto_auszug() -> (String, String) {
    let pfad = wurzel()
        .join("..")
        .join("relay-proto")
        .join("src")
        .join("lib.rs");
    let quelle = std::fs::read_to_string(&pfad)
        .unwrap_or_else(|e| panic!("{} nicht lesbar: {e}", pfad.display()));
    let alle = definitionen(&quelle);
    let mut auszug = String::new();
    for name in RELAY_PROTO_TYPEN {
        let treffer: Vec<&String> = alle.iter().filter(|d| typname(d) == Some(name)).collect();
        assert_eq!(
            treffer.len(),
            1,
            "relay-proto: Typ {name} steht in der Liste, wurde aber {}-mal gefunden",
            treffer.len()
        );
        auszug.push_str(treffer[0]);
        auszug.push('\n');
    }
    // Als fertige Definitionen einreihen: `abdruck_von` soll sie nicht noch
    // einmal zerlegen, deshalb unter einem eigenen Namen als Rohtext.
    ("relay-proto (Auswahl)".to_string(), auszug)
}

fn aktueller_abdruck() -> String {
    let mut dateien: Vec<(String, String)> = DATEIEN
        .iter()
        .map(|name| {
            let pfad = wurzel().join("src").join(name);
            let quelle = std::fs::read_to_string(&pfad)
                .unwrap_or_else(|e| panic!("{} nicht lesbar: {e}", pfad.display()));
            assert!(
                !definitionen(&quelle).is_empty(),
                "{name} steht in der Liste, enthält aber kein gespeichertes Format mehr"
            );
            (name.to_string(), quelle)
        })
        .collect();
    // Die Auswahl aus relay-proto geht als Rohtext in den Hash ein.
    let (name, auszug) = relay_proto_auszug();
    let mut abdruck_eingabe = abdruck_text(&dateien);
    abdruck_eingabe.push_str("== ");
    abdruck_eingabe.push_str(&name);
    abdruck_eingabe.push('\n');
    abdruck_eingabe.push_str(&auszug);
    dateien.clear();
    sha256_hex(&abdruck_eingabe)
}

#[test]
fn abdruck_weicht_ab_macht_rot() {
    let pfad = wurzel().join("..").join("release").join("datenformat.json");
    let datei: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&pfad).expect("release/datenformat.json lesbar"))
            .expect("release/datenformat.json ist JSON");
    let hinterlegt = datei["abdruck"].as_str().unwrap_or_default();
    let aktuell = aktueller_abdruck();
    assert_eq!(
        hinterlegt, aktuell,
        "\n\nEin gespeichertes Format hat sich geändert (config.json oder ein Store).\n\
         Bitte in release/datenformat.json ZWEI Dinge entscheiden und eintragen:\n\
         \n\
         1. \"abdruck\": \"{aktuell}\"\n\
         2. \"rueckwaerts_lesbar_ab\": Kann die VORIGE Version die Daten dieser\n\
            Version noch lesen und fachlich richtig arbeiten? Dass sie neue\n\
            Einstellungen nicht kennt und beim Speichern verliert, ist erlaubt.\n\
            - Ja  → Grenze lassen.\n\
            - Nein (neue Enum-Variante, die gespeichert wird; Feld umbenannt oder\n\
              mit neuer Bedeutung; Einweg-Migration) → Grenze auf DIESE Version setzen.\n\
            Dazu eine Zeile in \"begruendung\".\n\
         \n\
         Hintergrund: docs/adr/0101-rueckweg-und-datenformat-regel.md\n"
    );
}

#[test]
fn der_abdruck_folgt_der_struktur_nicht_den_kommentaren() {
    let basis = r#"
/// Doku.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct A {
    /// Feld.
    pub x: u32, // Anmerkung
    pub y: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NichtGespeichert { pub z: u8 }

#[derive(Deserialize)]
pub enum E { Eins, Zwei { tief: bool } }

#[derive(Deserialize)]
pub struct Tupel(pub u32);

#[cfg(test)]
mod tests {
    #[derive(Deserialize)]
    struct NurImTest { a: u8 }
}
"#;
    let d = definitionen(basis);
    assert_eq!(d.len(), 3, "{d:?}");
    assert!(d[0].contains("pub struct A {") && d[0].contains("pub y: Vec<String>, }"));
    assert!(d[0].contains("#[serde(default)]"));
    assert!(d[1].ends_with("Zwei { tief: bool } }"), "{}", d[1]);
    assert!(d[2].ends_with("pub struct Tupel(pub u32);"), "{}", d[2]);

    let hash = |q: &str| abdruck_von(&[("a.rs".into(), q.into())]);
    let h = hash(basis);
    // Kommentare, Leerraum, ungespeicherte Typen und Tests zählen nicht.
    let kosmetik = basis
        .replace("/// Feld.", "/// Ganz anderer Text.")
        .replace("// Anmerkung", "")
        .replace("    pub y", "        pub   y")
        .replace("pub z: u8", "pub z: u16")
        .replace("a: u8", "a: u64");
    assert_eq!(hash(&kosmetik), h);
    // Struktur zählt: neues Feld, neuer Typ, neue Variante, serde-Attribut.
    for (alt, neu) in [
        ("pub x: u32,", "pub x: u32,\n    pub neu: bool,"),
        ("pub x: u32,", "pub x: u64,"),
        ("pub x: u32,", "pub umbenannt: u32,"),
        ("Eins, Zwei", "Eins, Drei, Zwei"),
        (
            "#[serde(default)]",
            "#[serde(default, deny_unknown_fields)]",
        ),
        ("Tupel(pub u32)", "Tupel(pub u64)"),
    ] {
        assert!(basis.contains(alt));
        assert_ne!(hash(&basis.replace(alt, neu)), h, "{alt} → {neu}");
    }
    // Windows-Zeilenenden ändern nichts.
    assert_eq!(hash(&basis.replace('\n', "\r\n")), h);
    // Auch der Dateiname gehört dazu (ein Typ wandert in eine andere Datei).
    assert_ne!(abdruck_von(&[("b.rs".into(), basis.into())]), h);
}

/// Jede Quelldatei mit einem `Deserialize`-Typ ist entweder als gespeichert
/// oder ausdrücklich als nicht gespeichert geführt. Ein neuer Store in einer
/// neuen Datei macht diesen Test rot, statt still am Wächter vorbeizugehen.
#[test]
fn jede_datei_mit_deserialize_ist_eingeordnet() {
    fn sammeln(dir: &Path, basis: &Path, gefunden: &mut Vec<String>) {
        for eintrag in std::fs::read_dir(dir).unwrap().flatten() {
            let pfad = eintrag.path();
            if pfad.is_dir() {
                sammeln(&pfad, basis, gefunden);
            } else if pfad.extension().is_some_and(|e| e == "rs") {
                let quelle = std::fs::read_to_string(&pfad).unwrap();
                if !definitionen(&quelle).is_empty() {
                    let rel = pfad.strip_prefix(basis).unwrap();
                    gefunden.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let src = wurzel().join("src");
    let mut gefunden = Vec::new();
    sammeln(&src, &src, &mut gefunden);
    gefunden.sort();

    let mut erwartet: Vec<String> = DATEIEN
        .iter()
        .chain(NICHT_GESPEICHERT)
        .map(|s| s.to_string())
        .collect();
    erwartet.sort();
    assert_eq!(
        gefunden, erwartet,
        "\n\nEine Datei mit einem `Deserialize`-Typ ist nicht eingeordnet (oder steht in der \
         Liste, hat aber keinen mehr).\nSchreibt oder liest sie eine Datei im App-Datenverzeichnis? \
         → in DATEIEN aufnehmen.\nSonst → in NICHT_GESPEICHERT.\n"
    );
    for name in DATEIEN {
        assert!(
            !NICHT_GESPEICHERT.contains(name),
            "{name} steht in beiden Listen"
        );
    }
}

#[test]
fn test_modul_wird_auch_mit_sichtbarkeit_abgeschnitten() {
    for kopf in ["mod tests", "pub mod tests", "pub(crate) mod tests"] {
        let quelle = format!(
            "#[derive(Deserialize)]\nstruct A {{ x: u8 }}\n\n#[cfg(test)]\n{kopf} {{\n    \
             #[derive(Deserialize)]\n    struct NurTest {{ y: u8 }}\n}}\n"
        );
        assert_eq!(definitionen(&quelle).len(), 1, "{kopf}");
    }
    // Ein `#[cfg(test)]` vor etwas anderem als einem Modul schneidet nicht.
    let quelle = "#[cfg(test)]\nuse x::y;\n\n#[derive(Deserialize)]\nstruct A { x: u8 }\n";
    assert_eq!(definitionen(quelle).len(), 1);
    assert_eq!(
        typname("#[derive(Deserialize)] pub struct Abc<T> { x: T }"),
        Some("Abc")
    );
    assert_eq!(
        typname("#[derive(Deserialize)] pub(crate) enum E { A }"),
        Some("E")
    );
}

#[test]
fn die_grenze_ist_eine_version_und_liegt_nicht_in_der_zukunft() {
    let pfad = wurzel().join("..").join("release").join("datenformat.json");
    let datei: serde_json::Value = serde_json::from_slice(&std::fs::read(&pfad).unwrap()).unwrap();
    let grenze = bts_light_lib::uebergabe::Version::lesen(
        datei["rueckwaerts_lesbar_ab"].as_str().unwrap_or_default(),
    )
    .expect("rueckwaerts_lesbar_ab ist eine Version X.Y.Z");
    let eigene = bts_light_lib::uebergabe::Version::lesen(env!("CARGO_PKG_VERSION")).unwrap();
    assert!(
        grenze <= eigene,
        "die Grenze kann nicht nach der eigenen Version liegen"
    );
    assert!(
        datei["begruendung"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .len()
            > 10,
        "die Grenze braucht eine Begründung"
    );
}
