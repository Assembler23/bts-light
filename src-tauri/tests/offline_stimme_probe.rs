//! Messwerkzeug: Wie schnell, wie groß und wie verständlich ist die
//! Offline-Stimme? (Spec `ansage-offline-stimme`, Etappe E0, ADR 0104.)
//!
//! **Messwerkzeug, kein Regressionstest.** Vor dem Bau von E2 sind diese
//! Annahmen zu prüfen:
//!
//! 1. Das Modell `thorsten-medium` lädt in **unter 2 s**.
//! 2. Eine Ansage von etwa 4 s Ton rechnet in **0,2–0,6 s** auf einem
//!    Turnier-PC (Zeit bis zum ersten Ton zählt, nicht die ganze Ansage).
//! 3. Der Arbeitsspeicher wächst um **150–250 MB** und bleibt im Dauerlauf
//!    stehen (2.000 Ansagen ohne Absturz, ohne Wachstum).
//! 4. Die Programmdatei wächst um etwa **20 MB** (über 40 MB: neu entscheiden).
//! 5. Der Weg V1 (Lexikon + Schätzer, `offline_stimme::laute`) klingt gut
//!    genug — **Hörvergleich** der 25 Ansagen unten, kein Wort verschluckt.
//!
//! Gebaut wird es nur mit dem Feature `offline-probe`; die CI und die App
//! berühren die Engine nicht.
//!
//! ## Vorbereitung am Windows-PC (einmalig)
//!
//! Drei Dateien laden und ihre Prüfsumme vergleichen — die Werte stehen
//! unten in [`ARCHIVE`] und werden vom Werkzeug beim Start noch einmal
//! geprüft. In der PowerShell, in einem leeren Ordner:
//!
//! ```text
//! $r = "https://github.com/k2-fsa/sherpa-onnx/releases/download"
//! curl.exe -L -o engine.tar.bz2  "$r/v2.0.1/sherpa-onnx-v2.0.1-win-x64-static-MD-Release-lib.tar.bz2"
//! curl.exe -L -o stimme.tar.bz2  "$r/tts-models/vits-piper-de_DE-thorsten-medium.tar.bz2"
//! curl.exe -L -o de.txt "https://raw.githubusercontent.com/open-dict-data/ipa-dict/master/data/de.txt"
//! Get-FileHash engine.tar.bz2, stimme.tar.bz2, de.txt
//! tar -xf engine.tar.bz2
//! tar -xf stimme.tar.bz2
//! ```
//!
//! Die Stimme bringt einen Ordner `espeak-ng-data` mit — der wird **nicht**
//! benutzt (GPL, ADR 0104). Dann im Repo (Pfade anpassen):
//!
//! ```text
//! $env:SHERPA_ONNX_LIB_DIR = "C:\mess\sherpa-onnx-v2.0.1-win-x64-static-MD-Release-lib\lib"
//! $env:BTS_STIMME_DIR = "C:\mess\vits-piper-de_DE-thorsten-medium"
//! $env:BTS_LEXIKON = "C:\mess\de.txt"
//! cargo test --release -p bts-light --features offline-probe --test offline_stimme_probe -- --ignored --nocapture
//! ```
//!
//! **`--release`** ist Pflicht: Nur so stimmen Rechenzeit und Dateigröße.
//! Zum Vergleich der Größe vorher einmal das Ausfall-Werkzeug ebenso bauen
//! (`cargo test --release -p bts-light --test tts_ausfall_probe --no-run`):
//! Beide Testprogramme enthalten dieselbe App, nur dieses hier die Engine.
//! Das Werkzeug sucht das andere selbst und meldet die Differenz.
//!
//! ## Läufe
//!
//! | Lauf | Wie | Frage |
//! |---|---|---|
//! | `kurz` | ohne weitere Variable (etwa eine Minute) | Ladezeit, Rechenzeit je Ansage mit 1 und 2 Threads, Speicher, Größe, Hörproben |
//! | `dauer` | zusätzlich `$env:BTS_PROBE_DAUERLAUF="2000"` | 2.000 Ansagen am Stück: Absturz? Speicher wächst? |
//! | `schwach` | Lauf `kurz` auf dem schwächsten Turnier-PC | Reicht die Rechenzeit dort? |
//!
//! `BTS_PROBE_LAGE` benennt den Lauf in der Ergebnisdatei.
//!
//! Alle Zeilen landen zusätzlich in `%TEMP%\bts-light-offline-stimme-messung.txt`
//! (jeder Lauf hängt an), die Hörproben in
//! `%TEMP%\bts-light-offline-stimme-proben\` (dazu `proben.txt` mit Text und
//! Lautschrift je Probe). Jeder Lauf löscht die alten WAV-Dateien dort.
//! **Die Ergebnisdatei** gehört in ADR 0104 („Ergebnis der Messung"), nicht
//! die kopierte Konsole — cargo druckt dort Pfade des Rechners mit.
//!
//! ## Hörvergleich (Runde 2, ab 18.11.2026)
//!
//! Je Ansage drei Fassungen derselben Laute (siehe [`VARIANTEN`]):
//! `probe-01-a.wav` (Tempo normal), `-b` (langsamer), `-c` (langsamer und
//! ruhiger). Alle mit ungekürzten Pausen und Stille am Ende — Runde 1 hatte
//! die Pausen der Engine auf ein Fünftel gekürzt und keine Stille am Ende.
//! Je Probe notieren: welche Fassung am besten, verständlich ja/nein,
//! welcher Name falsch klingt.
//!
//! Die Ansagen 1–20 sind die aus Runde 1; nach ihr sind Laut-Baustein und
//! die Ausnahmeliste unten abgestimmt. **Die Ansagen 21–25 sind neu** und
//! ohne Abstimmung — sie zeigen, wie es bei unbekannten Namen klingt.
//! Die Zeile `quellen=` zeigt, woher die Laute kamen (`ausnahme`,
//! `funktionswort`, `lexikon`, `zusammengesetzt`, `geschaetzt`).
//!
//! Die Namen sind ausgedacht (deutsch, polnisch, vietnamesisch, englisch).
//!
//! ## Gegenprobe mit espeak (Fassung `-e`)
//!
//! Die Stimme ist mit Lautschrift von espeak-ng trainiert. Klingt sie mit
//! espeak-Lauten deutlich besser als mit unseren, liegt es am Laut-Baustein;
//! klingt sie ähnlich, ist die Stimme selbst die Grenze. espeak ist GPL und
//! kommt **nie** in die App — es läuft nur hier am Mess-PC als eigenes
//! Programm. Ablauf:
//!
//! 1. espeak-ng installieren (`espeak-ng.msi` aus den Releases von
//!    `github.com/espeak-ng/espeak-ng`).
//! 2. Einen Lauf `kurz` machen — er legt je Ansage `probe-NN-<kennung>.txt`
//!    in den Unterordner `espeak\` der Hörproben (die Kennung hängt am
//!    Text: Ändert sich eine Ansage, passt eine alte Lautschrift nicht mehr).
//! 3. Lautschrift erzeugen:
//!
//! ```text
//! $e = "C:\Program Files\eSpeak NG\espeak-ng.exe"
//! Get-ChildItem "$env:TEMP\bts-light-offline-stimme-proben\espeak\probe-*.txt" | ForEach-Object {
//!   $o = $_.FullName -replace '\.txt$', '.ipa'
//!   & $e -v de -q -b 1 --ipa -f $_.FullName "--phonout=$o"
//! }
//! ```
//!
//! 4. Den Lauf `kurz` wiederholen: Wo eine passende `.ipa` liegt, schreibt
//!    er zusätzlich `probe-NN-e.wav` (Einstellungen wie Fassung `b`). Die
//!    Zeile `hoerproben=` nennt, wie viele es waren und wie viele Zeichen
//!    der espeak-Lautschrift die Stimme nicht kennt (`espeak_unbekannt`).

use bts_light_lib::offline_stimme::laute::{espeak_ausgabe_zu_saetzen, Lautierer, Lexikon};
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Erwartete Downloads: Name, SHA-256 (Stand 17.11.2026; Engine und Stimme
/// laut GitHub-Release, das Lexikon am Stand von `ipa-dict`). Zum Vergleich
/// mit `Get-FileHash`; das Werkzeug druckt die Liste zu Beginn.
const ARCHIVE: &[(&str, &str)] = &[
    (
        "engine.tar.bz2 (sherpa-onnx-v2.0.1-win-x64-static-MD-Release-lib)",
        "7c1f3ef15487f2a06625ef06685a9f96f03c0a1f0b3ad52de9e3bf6af6c1be39",
    ),
    (
        "stimme.tar.bz2 (vits-piper-de_DE-thorsten-medium)",
        "50487d9c95fdf2191f31d2588569381063ba1591dcd4c7d4bdd30f12b2191714",
    ),
    (
        "de.txt (ipa-dict, Deutsch)",
        "d36d11dbcc226468a22f0fb3a0794ee0ce142bf277e26e517a8bfd0de1ec2653",
    ),
];

/// Modelldatei im Archiv `thorsten-medium`.
const MODELL_DATEI: &str = "de_DE-thorsten-medium.onnx";

/// Fassungen der Hörproben: Kennbuchstabe, Tempo (`speed`, kleiner =
/// langsamer), Pausen-Faktor (`silence_scale`, 1 = ungekürzt), Rauschanteil
/// (`noise_scale`, kleiner = ruhiger, gleichförmiger), Beschreibung.
/// Die Zeitmessung und der Dauerlauf rechnen mit Fassung `b`.
const VARIANTEN: [(&str, f32, f32, f32, &str); 3] = [
    ("a", 1.0, 1.0, 0.667, "Tempo normal"),
    ("b", 0.85, 1.0, 0.667, "langsamer"),
    ("c", 0.85, 1.0, 0.4, "langsamer, ruhiger"),
];
const MESS_VARIANTE: usize = 1;

/// Stille vor und nach jeder Hörprobe in Sekunden — manche Abspieler
/// schneiden sonst den Anfang oder das letzte Wort ab.
const STILLE_VORN_S: f32 = 0.15;
const STILLE_HINTEN_S: f32 = 0.6;

/// Ausnahmeliste der Messung (klein geschrieben → Laute), wie sie die
/// Turnierleitung in E2 für schwierige Namen pflegen würde. Abgestimmt nach
/// dem Hörvergleich der Runde 1 — nur für Namen der Ansagen 1–20, die nach
/// deutschen Regeln falsch klangen (englisch, vietnamesisch, polnisch).
const AUSNAHMEN: &[(&str, &str)] = &[
    ("anna", "ˈana"),
    ("bennett", "bˈɛnət"),
    ("clarke", "klˈaːk"),
    ("emily", "ˈɛmɪliː"),
    ("fairbanks", "fˈɛːɐbɛŋks"),
    ("fitzgerald", "fɪtsdʒˈɛʁəlt"),
    ("george", "dʒˈɔːdʒ"),
    ("harrison", "hˈɛʁɪsən"),
    ("hughes", "hjˈuːs"),
    ("huy", "hˈuːi"),
    ("huynh", "hˈuːɪn"),
    ("james", "dʒˈeːɪms"),
    ("meetingpoint", "mˈiːtɪŋpˌɔønt"),
    ("nguyen", "ŋˈuːjɛn"),
    ("olivia", "olˈiːvia"),
    ("piotr", "pjˈɔtʁ"),
    ("ryan", "ʁˈaɪən"),
    ("sophie", "zofˈiː"),
    ("thompson", "tˈɔmsən"),
    ("william", "vˈɪljəm"),
    ("wisniewski", "vɪʃnjˈɛfskiː"),
    ("wright", "ʁˈaɪt"),
];

/// Die 20 Ansagen des Hörvergleichs — so, wie die Windows-Stimme sie heute
/// bekommt (Sprech-Abschnitte, ohne SSML).
const ANSAGEN: [&str; 25] = [
    "Feld 3. Herrendoppel A, Halbfinale. Jonas Becker und Lukas Hoffmann gegen Piotr Kowalczyk und Tomasz Wiśniewski.",
    "Feld 12. Dameneinzel B, Runde 2. Lena Schröder gegen Nguyễn Thị Lan.",
    "Feld 7. Mixed C, Gruppe 1. Tim Wagner und Sophie Krüger gegen James Whitfield und Emily Clarke.",
    "Feld 1. Herreneinzel A, Finale. Maximilian Schäfer gegen Trần Văn Minh. Schiedsrichter: Andreas Vogel.",
    "Feld 4. Damendoppel A, Viertelfinale. Agnieszka Zielińska und Katarzyna Wójcik gegen Hannah Fischer und Mia Zimmermann.",
    "Feld 9. Herrendoppel C, Runde 1. Lê Hoàng Nam und Phạm Đức Anh gegen Oliver Bennett und George Harrison-Wright.",
    "Feld 2. Mixed A, Halbfinale. Felix Neumann und Laura Schwarz gegen Michał Lewandowski und Joanna Kamińska. Aufschlagrichter: Bernd Krause.",
    "Feld 15. Dameneinzel C, Gruppe 3. Charlotte Hughes gegen Đỗ Thu Hà.",
    "Feld 6. Herreneinzel B, Runde 3. Grzegorz Brzęczyszczykiewicz gegen Niklas Hartmann.",
    "Feld 11. Mixed B, Finale. Paul Richter und Emma Wolf gegen Võ Minh Tuấn und Huỳnh Ngọc Mai.",
    "In Vorbereitung: Herrendoppel A, Finale. Jonas Becker und Lukas Hoffmann gegen Daniel Thompson und Ryan O'Connor. Bitte zum Meetingpoint.",
    "Zähltafel Feld 5: Marie Lehmann.",
    "Feld 8. Damendoppel B, Runde 1. Małgorzata Szymańska und Ewa Dąbrowska gegen Bùi Thị Hồng und Ngô Phương Linh.",
    "Feld 10. Herrendoppel B, Halbfinale. Sebastian Krämer und Florian Weiß gegen William Ashworth und Thomas Fairbanks.",
    "Feld 13. Herreneinzel C, Runde 2. Jakub Wróblewski gegen Phan Quốc Bảo.",
    "Feld 14. Dameneinzel A, Halbfinale. Anna-Lena Köhler gegen Olivia Fitzgerald.",
    "Spiel 47 bitte zur Turnierleitung. Feld 3 ist frei.",
    "Feld 16. Mixed C, Runde 1. Dawid Szczepański und Zofia Jabłońska gegen Leon Schulze und Clara Baumann.",
    "Feld 1. Herrendoppel A, Spiel um Platz 3. Hoàng Gia Huy und Đặng Quang Vinh gegen Moritz Bauer und Jan Albrecht. Schiedsrichter: Sabine Kühn.",
    "Achtung, eine Durchsage der Turnierleitung: Die Siegerehrung beginnt um 18 Uhr 30 in Halle 2.",
    // Ab hier neu in Runde 2, nicht abgestimmt.
    "Feld 5. Herrendoppel B, Viertelfinale. Matthias Engel und Stefan Kaiser gegen Łukasz Nowak und Marcin Zając.",
    "Feld 17. Dameneinzel A, Runde 1. Johanna Berger gegen Vũ Thanh Hương.",
    "Feld 3. Mixed A, Viertelfinale. Henry Collins und Grace Morgan gegen Tobias Schmitt und Lea Hofmann.",
    "Feld 21. Herreneinzel C, Gruppe 2. Dominik Kaczmarek gegen Lý Minh Khôi. Schiedsrichter: Ute Brandt.",
    "In Vorbereitung: Damendoppel B, Finale. Katharina Wolff und Julia Lorenz gegen Sarah Mitchell und Hoàng Thị Thu.",
];

fn umgebung(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

/// Arbeitsspeicher des Prozesses in MB: (jetzt, Höchststand).
#[cfg(windows)]
fn speicher_mb() -> (f64, f64) {
    use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut z = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: Struktur mit korrekter Größe, Pseudo-Handle des eigenen Prozesses.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut z, z.cb) }.as_bool();
    if !ok {
        return (f64::NAN, f64::NAN);
    }
    let mb = |b: usize| b as f64 / 1_048_576.0;
    (mb(z.WorkingSetSize), mb(z.PeakWorkingSetSize))
}

#[cfg(not(windows))]
fn speicher_mb() -> (f64, f64) {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let wert = |schluessel: &str| {
        text.lines()
            .find(|l| l.starts_with(schluessel))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|kb| kb / 1024.0)
            .unwrap_or(f64::NAN)
    };
    (wert("VmRSS:"), wert("VmHWM:"))
}

/// Schreibt Zeilen auf die Konsole und in die Ergebnisdatei.
struct Protokoll {
    datei: Option<std::fs::File>,
    lage: String,
}

impl Protokoll {
    fn zeile(&mut self, text: &str) {
        let z = format!("lage={} {text}", self.lage);
        println!("{z}");
        if let Some(d) = &mut self.datei {
            let _ = writeln!(d, "{z}");
        }
    }
}

fn sha256_datei(p: &Path) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    let d = ring::digest::digest(&ring::digest::SHA256, &bytes);
    Some(d.as_ref().iter().map(|b| format!("{b:02x}")).collect())
}

fn wav_schreiben(pfad: &Path, ton: &[f32], rate: i32) {
    let vorn = vec![0.0f32; (rate as f32 * STILLE_VORN_S) as usize];
    let hinten = vec![0.0f32; (rate as f32 * STILLE_HINTEN_S) as usize];
    let samples: Vec<f32> = vorn.iter().chain(ton).chain(&hinten).copied().collect();
    let mut daten = Vec::with_capacity(44 + samples.len() * 2);
    let n = (samples.len() * 2) as u32;
    daten.extend_from_slice(b"RIFF");
    daten.extend_from_slice(&(36 + n).to_le_bytes());
    daten.extend_from_slice(b"WAVEfmt ");
    daten.extend_from_slice(&16u32.to_le_bytes());
    daten.extend_from_slice(&1u16.to_le_bytes());
    daten.extend_from_slice(&1u16.to_le_bytes());
    daten.extend_from_slice(&(rate as u32).to_le_bytes());
    daten.extend_from_slice(&(rate as u32 * 2).to_le_bytes());
    daten.extend_from_slice(&2u16.to_le_bytes());
    daten.extend_from_slice(&16u16.to_le_bytes());
    daten.extend_from_slice(b"data");
    daten.extend_from_slice(&n.to_le_bytes());
    for s in &samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        daten.extend_from_slice(&v.to_le_bytes());
    }
    let _ = std::fs::write(pfad, daten);
}

/// Zeichen, die das Modell kennt (erste Spalte von `tokens.txt`).
fn modell_zeichen(dir: &Path) -> HashSet<char> {
    let text = std::fs::read_to_string(dir.join("tokens.txt")).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            // Zeile „<zeichen> <id>"; das Leerzeichen selbst steht als „  3".
            let (z, _) = l.rsplit_once(' ')?;
            let mut c = z.chars();
            match (c.next(), c.next()) {
                (Some(x), None) => Some(x),
                (None, _) => Some(' '),
                _ => None,
            }
        })
        .collect()
}

fn engine_laden(dir: &Path, threads: i32, rauschen: f32) -> Option<(OfflineTts, Duration)> {
    let mut cfg = OfflineTtsConfig::default();
    cfg.model.vits.noise_scale = rauschen;
    cfg.model.vits.model = Some(dir.join(MODELL_DATEI).to_string_lossy().into_owned());
    cfg.model.vits.tokens = Some(dir.join("tokens.txt").to_string_lossy().into_owned());
    cfg.model.num_threads = threads;
    cfg.model.provider = Some("cpu".into());
    let start = Instant::now();
    let tts = OfflineTts::create(&cfg)?;
    Some((tts, start.elapsed()))
}

/// Eine Ansage, fertig für die Engine: Zeichen je Satz, Lautschrift,
/// Herkunft der Laute.
type Probe = (Vec<Vec<i32>>, String, BTreeMap<&'static str, usize>);

/// Ergebnis einer Vertonung.
struct Vertonung {
    samples: Vec<f32>,
    rate: i32,
    gesamt: Duration,
    erster_ton: Duration,
}

fn vertonen(tts: &OfflineTts, saetze: &[Vec<i32>], variante: usize) -> Option<Vertonung> {
    let (_, tempo, pausen, _, _) = VARIANTEN[variante];
    let g = GenerationConfig {
        phoneme_codepoints: Some(saetze.to_vec()),
        speed: tempo,
        silence_scale: pausen,
        ..Default::default()
    };
    let erster: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let e2 = Arc::clone(&erster);
    let start = Instant::now();
    let audio = tts.generate_with_config(
        "",
        &g,
        Some(move |s: &[f32], _: f32| {
            let mut e = e2.lock().unwrap();
            if e.is_none() && !s.is_empty() {
                *e = Some(Instant::now());
            }
            true
        }),
    )?;
    let gesamt = start.elapsed();
    let erster_ton = erster.lock().unwrap().map(|t| t - start).unwrap_or(gesamt);
    Some(Vertonung {
        samples: audio.samples().to_vec(),
        rate: audio.sample_rate(),
        gesamt,
        erster_ton,
    })
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

/// Größe des Ausfall-Werkzeugs (Name beginnt mit `tts_ausfall_probe-`,
/// ohne `.d`/`.pdb`) — der Vergleich für den Zuwachs. Gesucht wird im
/// eigenen Ordner und, wo cargo jedes Testprogramm in einen eigenen
/// Unterordner legt, in den Geschwistern (`…/<hash>/out/`).
fn vergleichsgroesse(eigene: &Path) -> Option<u64> {
    let dir = eigene.parent()?;
    let mut ordner = vec![dir.to_path_buf()];
    if let Some(oben) = dir.parent().and_then(Path::parent) {
        if let Ok(geschwister) = std::fs::read_dir(oben) {
            ordner.extend(
                geschwister
                    .filter_map(Result::ok)
                    .map(|e| e.path().join("out")),
            );
        }
    }
    ordner
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("tts_ausfall_probe-")
                && !n.ends_with(".d")
                && !n.ends_with(".pdb")
                && (cfg!(not(windows)) || n.ends_with(".exe"))
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len()))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, l)| l)
}

/// Laute als Zeichenfolgen je Satz für die Engine; Unbekanntes fällt weg.
fn fuer_engine(saetze: &[String], zeichen: &HashSet<char>) -> Vec<Vec<i32>> {
    saetze
        .iter()
        .map(|s| {
            s.chars()
                .filter(|c| zeichen.is_empty() || zeichen.contains(c))
                .map(|c| c as i32)
                .collect()
        })
        .collect()
}

/// Dateiname der espeak-Ein- und -Ausgabe einer Ansage, ohne Endung: Nummer
/// und eine Kennung aus dem Text, damit eine alte Lautschrift nach einer
/// Textänderung nicht mehr greift.
fn espeak_stamm(nr: usize, text: &str) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, text.as_bytes());
    let kennung: String = d.as_ref()[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("probe-{nr:02}-{kennung}")
}

/// Hörproben aller Fassungen, `proben.txt` und die espeak-Gegenprobe.
fn hoerproben_schreiben(
    p: &mut Protokoll,
    dir: &Path,
    proben: &[Probe],
    zeichen: &HashSet<char>,
    proben_dir: &Path,
    espeak_dir: &Path,
) {
    let mut liste = String::from("Fassungen:\n");
    for (k, tempo, pausen, rauschen, text) in VARIANTEN {
        liste.push_str(&format!(
            "  -{k}: {text} (speed={tempo}, silence_scale={pausen}, noise_scale={rauschen})\n"
        ));
    }
    liste.push_str("  -e: espeak-Laute, Einstellungen wie -b (nur wenn probe-NN.ipa vorliegt)\n\n");
    for (i, (_, laute, quellen)) in proben.iter().enumerate() {
        let q: Vec<String> = quellen.iter().map(|(k, n)| format!("{k}:{n}")).collect();
        let neu = if i >= 20 {
            " (neu, nicht abgestimmt)"
        } else {
            ""
        };
        liste.push_str(&format!(
            "probe-{:02}{neu}\n  text:  {}\n  laute: {laute}\n  quellen: {}\n",
            i + 1,
            ANSAGEN[i],
            q.join(", ")
        ));
        // Eingabe für espeak: der Text, wie er angesagt wird.
        let stamm = espeak_stamm(i + 1, ANSAGEN[i]);
        let _ = std::fs::write(espeak_dir.join(format!("{stamm}.txt")), ANSAGEN[i]);
        if let Ok(ipa) = std::fs::read_to_string(espeak_dir.join(format!("{stamm}.ipa"))) {
            let saetze = espeak_ausgabe_zu_saetzen(&ipa);
            liste.push_str(&format!("  espeak: {}\n", saetze.join(" ")));
        }
        liste.push('\n');
    }
    let _ = std::fs::write(proben_dir.join("proben.txt"), liste);

    let mut geschrieben = 0usize;
    for (v, (k, _, _, rauschen, _)) in VARIANTEN.iter().enumerate() {
        let Some((tts, _)) = engine_laden(dir, 1, *rauschen) else {
            p.zeile(&format!("hoerproben fassung={k} fehler=engine"));
            continue;
        };
        for (i, (cps, _, _)) in proben.iter().enumerate() {
            if let Some(a) = vertonen(&tts, cps, v) {
                wav_schreiben(
                    &proben_dir.join(format!("probe-{:02}-{k}.wav", i + 1)),
                    &a.samples,
                    a.rate,
                );
                geschrieben += 1;
            }
        }
    }

    // Gegenprobe: dieselbe Stimme mit den Lauten von espeak.
    let mut espeak = 0usize;
    let mut unbekannt = 0usize;
    if let Some((tts, _)) = engine_laden(dir, 1, VARIANTEN[MESS_VARIANTE].3) {
        for (i, text) in ANSAGEN.iter().enumerate().take(proben.len()) {
            let stamm = espeak_stamm(i + 1, text);
            let Ok(ipa) = std::fs::read_to_string(espeak_dir.join(format!("{stamm}.ipa"))) else {
                continue;
            };
            let saetze = espeak_ausgabe_zu_saetzen(&ipa);
            unbekannt += saetze
                .iter()
                .flat_map(|s| s.chars())
                .filter(|c| !zeichen.is_empty() && !zeichen.contains(c))
                .count();
            let cps = fuer_engine(&saetze, zeichen);
            if let Some(a) = vertonen(&tts, &cps, MESS_VARIANTE) {
                wav_schreiben(
                    &proben_dir.join(format!("probe-{:02}-e.wav", i + 1)),
                    &a.samples,
                    a.rate,
                );
                espeak += 1;
            }
        }
    }
    p.zeile(&format!(
        "hoerproben=geschrieben dateien={geschrieben} espeak={espeak} espeak_unbekannt={unbekannt}"
    ));
}

#[test]
#[ignore = "Messwerkzeug: braucht Engine, Stimme und Lexikon (siehe Kopf der Datei)"]
fn offline_stimme_messen() {
    let lage = umgebung("BTS_PROBE_LAGE").unwrap_or_else(|| "kurz".into());
    let tmp = std::env::temp_dir();
    let datei = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(tmp.join("bts-light-offline-stimme-messung.txt"))
        .ok();
    let mut p = Protokoll { datei, lage };
    p.zeile(&format!(
        "start os={} cpus={}",
        std::env::consts::OS,
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    ));

    // --- Programmgröße ----------------------------------------------------
    if let Ok(exe) = std::env::current_exe() {
        let groesse = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
        let profil = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        match vergleichsgroesse(&exe) {
            Some(v) => p.zeile(&format!(
                "messung=groesse profil={profil} exe_mb={:.1} vergleich_mb={:.1} zuwachs_mb={:.1}",
                groesse as f64 / 1_048_576.0,
                v as f64 / 1_048_576.0,
                (groesse as f64 - v as f64) / 1_048_576.0
            )),
            None => p.zeile(&format!(
                "messung=groesse profil={profil} exe_mb={:.1} vergleich=fehlt",
                groesse as f64 / 1_048_576.0
            )),
        }
    }

    // --- Dateien ------------------------------------------------------------
    let Some(dir) = umgebung("BTS_STIMME_DIR").map(PathBuf::from) else {
        p.zeile("abbruch=stimme_dir_fehlt");
        panic!("BTS_STIMME_DIR fehlt (siehe Kopf der Datei)");
    };
    let modell = dir.join(MODELL_DATEI);
    let modell_mb = std::fs::metadata(&modell)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(0.0);
    p.zeile(&format!(
        "messung=modell datei_mb={modell_mb:.1} sha256={}",
        sha256_datei(&modell).unwrap_or_else(|| "fehlt".into())
    ));
    for (name, sha) in ARCHIVE {
        p.zeile(&format!("erwartet datei=\"{name}\" sha256={sha}"));
    }
    let zeichen = modell_zeichen(&dir);
    p.zeile(&format!("messung=modell zeichen={}", zeichen.len()));

    // --- Lexikon --------------------------------------------------------------
    let vor_lex = speicher_mb().0;
    let start = Instant::now();
    let lexikon = match umgebung("BTS_LEXIKON") {
        Some(pfad) => match std::fs::read_to_string(&pfad) {
            Ok(t) => {
                p.zeile(&format!(
                    "messung=lexikon sha256={}",
                    sha256_datei(Path::new(&pfad)).unwrap_or_default()
                ));
                Lexikon::aus_tsv(&t)
            }
            Err(_) => {
                p.zeile("messung=lexikon fehler=unlesbar");
                Lexikon::default()
            }
        },
        None => {
            p.zeile("messung=lexikon fehler=nicht_gesetzt");
            Lexikon::default()
        }
    };
    p.zeile(&format!(
        "messung=lexikon eintraege={} laden_ms={} speicher_mb={:.0}",
        lexikon.len(),
        ms(start.elapsed()),
        speicher_mb().0 - vor_lex
    ));
    let ausnahmen: HashMap<String, String> = AUSNAHMEN
        .iter()
        .map(|(w, l)| (w.to_string(), l.to_string()))
        .collect();
    let lautierer = Lautierer {
        lexikon: &lexikon,
        ausnahmen: &ausnahmen,
    };

    // --- Laute aller Ansagen ---------------------------------------------------
    let mut proben: Vec<Probe> = Vec::new();
    let mut fremd_gesamt = 0usize;
    let start = Instant::now();
    for text in ANSAGEN {
        let saetze = lautierer.saetze(text);
        let mut quellen: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut cps = Vec::new();
        let mut laute = Vec::new();
        for s in &saetze {
            for q in &s.quellen {
                *quellen.entry(q.wort()).or_default() += 1;
            }
            // Zeichen, die das Modell nicht kennt, fallen weg — gezählt.
            let gefiltert: Vec<i32> = s
                .laute
                .chars()
                .filter(|c| {
                    let ok = zeichen.is_empty() || zeichen.contains(c);
                    if !ok {
                        fremd_gesamt += 1;
                    }
                    ok
                })
                .map(|c| c as i32)
                .collect();
            cps.push(gefiltert);
            laute.push(s.laute.clone());
        }
        proben.push((cps, laute.join(" "), quellen));
    }
    p.zeile(&format!(
        "messung=lautieren ansagen={} gesamt_ms={} fremde_zeichen={fremd_gesamt}",
        ANSAGEN.len(),
        ms(start.elapsed())
    ));

    // --- Engine: Laden und Rechnen je Thread-Zahl ------------------------------
    let threads: Vec<i32> = umgebung("BTS_PROBE_THREADS")
        .unwrap_or_else(|| "1,2".into())
        .split(',')
        .filter_map(|t| t.trim().parse().ok())
        .collect();
    let proben_dir = tmp.join("bts-light-offline-stimme-proben");
    let espeak_dir = proben_dir.join("espeak");
    let _ = std::fs::create_dir_all(&espeak_dir);
    // Alte espeak-Eingaben weg (werden gleich neu geschrieben). Vorhandene
    // `.ipa` bleiben — die Kennung im Namen sorgt dafür, dass nur passende
    // greifen.
    if let Ok(alt) = std::fs::read_dir(&espeak_dir) {
        for e in alt.filter_map(Result::ok) {
            if e.path().extension().is_some_and(|x| x == "txt") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    // Alte Hörproben weg, damit keine Fassung einer früheren Runde liegen bleibt.
    if let Ok(alt) = std::fs::read_dir(&proben_dir) {
        for e in alt.filter_map(Result::ok) {
            if e.path().extension().is_some_and(|x| x == "wav") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let mut dauer_engine: Option<OfflineTts> = None;
    let mess_rauschen = VARIANTEN[MESS_VARIANTE].3;
    for &t in &threads {
        // Die vorige Engine erst freigeben, sonst misst der Speicher beide.
        dauer_engine = None;
        let vorher = speicher_mb().0;
        let Some((tts, laden)) = engine_laden(&dir, t, mess_rauschen) else {
            p.zeile(&format!("messung=laden threads={t} fehler=engine"));
            continue;
        };
        p.zeile(&format!(
            "messung=laden threads={t} laden_ms={} speicher_mb={:.0} rate={}",
            ms(laden),
            speicher_mb().0 - vorher,
            tts.sample_rate()
        ));
        let mut erste: Vec<u128> = Vec::new();
        let mut rtf: Vec<f64> = Vec::new();
        for (i, (cps, _, quellen)) in proben.iter().enumerate() {
            let Some(v) = vertonen(&tts, cps, MESS_VARIANTE) else {
                p.zeile(&format!(
                    "messung=rechnen threads={t} probe={:02} fehler=engine",
                    i + 1
                ));
                continue;
            };
            let ton_s = v.samples.len() as f64 / v.rate as f64;
            let r = v.gesamt.as_secs_f64() / ton_s.max(0.01);
            erste.push(ms(v.erster_ton));
            rtf.push(r);
            let q: Vec<String> = quellen.iter().map(|(k, n)| format!("{k}:{n}")).collect();
            p.zeile(&format!(
                "messung=rechnen threads={t} probe={:02} ton_s={ton_s:.2} erster_ton_ms={} gesamt_ms={} rtf={r:.3} quellen={}",
                i + 1,
                ms(v.erster_ton),
                ms(v.gesamt),
                q.join(",")
            ));
        }
        erste.sort_unstable();
        rtf.sort_by(f64::total_cmp);
        if !erste.is_empty() {
            let mitte = erste.len() / 2;
            p.zeile(&format!(
                "messung=zusammenfassung threads={t} erster_ton_ms_median={} erster_ton_ms_max={} rtf_median={:.3} rtf_max={:.3} speicher_mb={:.0} hoechst_mb={:.0}",
                erste[mitte],
                erste[erste.len() - 1],
                rtf[mitte],
                rtf[rtf.len() - 1],
                speicher_mb().0,
                speicher_mb().1
            ));
        }
        dauer_engine = Some(tts);
    }
    hoerproben_schreiben(&mut p, &dir, &proben, &zeichen, &proben_dir, &espeak_dir);

    // --- Dauerlauf ---------------------------------------------------------------
    let n: usize = umgebung("BTS_PROBE_DAUERLAUF")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if n > 0 {
        let Some(tts) = dauer_engine else {
            p.zeile("messung=dauerlauf fehler=keine_engine");
            return;
        };
        let start = Instant::now();
        let mut fehler = 0usize;
        let mut groesste = Duration::ZERO;
        let (anfang, _) = speicher_mb();
        for i in 0..n {
            let (cps, _, _) = &proben[i % proben.len()];
            let t0 = Instant::now();
            match vertonen(&tts, cps, MESS_VARIANTE) {
                Some(_) => groesste = groesste.max(t0.elapsed()),
                None => fehler += 1,
            }
            if (i + 1) % 250 == 0 {
                let (jetzt, hoch) = speicher_mb();
                p.zeile(&format!(
                    "messung=dauerlauf stand={} speicher_mb={jetzt:.0} hoechst_mb={hoch:.0} fehler={fehler}",
                    i + 1
                ));
            }
        }
        let (ende, hoch) = speicher_mb();
        p.zeile(&format!(
            "messung=dauerlauf ansagen={n} fehler={fehler} gesamt_s={} laengste_ms={} speicher_anfang_mb={anfang:.0} speicher_ende_mb={ende:.0} hoechst_mb={hoch:.0}",
            start.elapsed().as_secs(),
            ms(groesste)
        ));
    }
    p.zeile("ende");
}
