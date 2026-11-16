//! Messwerkzeug: Trägt der Kernwechsel ohne Installer auf diesem Rechner?
//! (Spec `hotfix-ohne-neustart`, ADR 0100, PR 7a.)
//!
//! Der Tausch der Programmdatei steht auf Annahmen über Windows, die sich im
//! Repo nicht prüfen lassen:
//!
//! 1. Eine **laufende** Programmdatei lässt sich umbenennen.
//! 2. Der **Wächter** überlebt das Ende der App und erkennt es am Schließen
//!    der stdin-Leitung.
//! 3. Der **erste Start** einer frisch abgelegten Programmdatei dauert unter
//!    dem Virenscanner nicht so lange, dass die Frist des Wächters reißt.
//! 4. Das **Installationsverzeichnis** ist für den angemeldeten Benutzer
//!    beschreibbar.
//!
//! Die Sonde spielt den Ablauf mit der **echten** Programmdatei in einem
//! Wegwerf-Verzeichnis durch — Erfolg, Fehlstart und Hänger — und druckt die
//! gemessenen Zeiten. Alte und neue App sind dabei **gespielt** (Rollen
//! derselben Programmdatei): Die alte hält einen Port und endet verzögert,
//! die neue beendet sich sofort, wenn der Port noch belegt ist. Die volle
//! App mit Fenster und Einzelinstanz-Sperre startet die Sonde nie — ob der
//! Wechsel mit ihr trägt, zeigt erst der Feldtest nach dem Anschluss. Sie fasst die installierte App nicht an
//! (nur ein Schreibtest und eine Prüfsumme) und startet kein Fenster.
//!
//! Standardmäßig übersprungen (`#[ignore]`):
//!
//! ```text
//! cargo test -p bts-light --test uebergabe_probe -- --ignored --nocapture
//! ```
//!
//! **Ohne `--release`:** Die Rollen, die die Sonde spielt, gibt es nur im
//! Debug-Build; das ausgelieferte Programm kennt sie nicht. Der erste Start
//! der (größeren) Debug-Datei dauert unter dem Virenscanner eher länger als
//! der der ausgelieferten — die Messung ist also auf der sicheren Seite.
//!
//! ## Was am Windows-PC zu messen ist
//!
//! | Lauf | Frage |
//! |---|---|
//! | Normal, Defender aktiv | Gehen alle Schritte durch? Wie lang ist der erste Start? |
//! | Zweimal hintereinander | Ist der zweite „erste Start" schneller (Scan-Cache)? |
//! | Als anderer Benutzer als der Installierende | Ist das Installationsverzeichnis beschreibbar? |
//! | Mit laufender installierter bts-light | Stört die Einzelinstanz-Sperre? (Sollte nicht: die Sonde startet nie die volle App.) |
//! | Auf dem langsamsten Turnier-PC | Reicht die Frist von 15 s mit Abstand? |
//!
//! Die Zeilen `MESSWERT …` gehören in ADR 0100 („Ergebnis der Messung").
//! Schlägt ein Schritt fehl, ist **das** das Ergebnis: Dann bleibt der
//! Kernwechsel beim Installer-Weg.
//!
//! Nicht von der Sonde abgedeckt, von Hand zu prüfen: Reste nach der
//! Deinstallation (`bts-light.vorher.exe` u. ä.) und ob die Prüfsumme der
//! installierten Datei der aus `kern/kern-<version>.json` gleicht.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use bts_light_lib::tausch::{self, Lage, Pfade, WaechterArgs, Zustand, ARG_PROBE, ARG_SELBSTTEST};

/// So lange wartet die Sonde höchstens auf einen Probelauf. Weit über der
/// Frist der App — die Sonde soll die Zeit messen, nicht an ihr scheitern.
const MESS_OBERGRENZE: Duration = Duration::from_secs(300);

/// Die frisch gebaute Programmdatei dieses Workspaces.
fn programm() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_bts-light"))
}

fn sha256(pfad: &Path) -> String {
    ring::digest::digest(&ring::digest::SHA256, &std::fs::read(pfad).unwrap())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn warte_bis(frist: Duration, mut fertig: impl FnMut() -> bool) -> Option<Duration> {
    let start = Instant::now();
    while start.elapsed() < frist {
        if fertig() {
            return Some(start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Ein Wegwerf-„Installationsverzeichnis" mit aktiver und neuer Datei.
struct Aufbau {
    _dir: tempfile::TempDir,
    pfade: Pfade,
    daten: PathBuf,
}

/// Wie [`aufbau`], aber beide Dateien sind schon einmal gelaufen — so wie
/// im Betrieb: Die alte läuft seit Stunden, die neue hat ihren Probelauf
/// hinter sich. Ohne das mäßen die Schritte danach vor allem die Erstprüfung
/// frischer Dateien durch den Virenscanner, nicht den Wechsel.
fn aufbau_vorgewaermt() -> Aufbau {
    let a = aufbau();
    for datei in [&a.pfade.aktiv, &a.pfade.neu] {
        tausch::probelauf_mit(datei, &[ARG_SELBSTTEST], MESS_OBERGRENZE)
            .expect("Vorwärmen: Probelauf");
    }
    a
}

/// Zeit zwischen zwei Zeilen im Protokoll des Wächters (Millisekunden).
fn zwischen(protokoll: &str, von: &str, bis: &str) -> Option<u64> {
    let zeit = |muster: &str| -> Option<u64> {
        protokoll
            .lines()
            .find(|z| z.contains(muster))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    };
    Some(zeit(bis)?.saturating_sub(zeit(von)?))
}

fn aufbau() -> Aufbau {
    let dir = tempfile::tempdir().unwrap();
    let name = if cfg!(windows) {
        "bts-light.exe"
    } else {
        "bts-light"
    };
    let pfade = Pfade::neben(&dir.path().join(name)).unwrap();
    std::fs::copy(programm(), &pfade.aktiv).unwrap();
    std::fs::copy(programm(), &pfade.neu).unwrap();
    let daten = dir.path().join("daten");
    std::fs::create_dir_all(&daten).unwrap();
    Aufbau {
        _dir: dir,
        pfade,
        daten,
    }
}

/// Ein gerade freier Port auf diesem Rechner.
fn freier_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// So lange läuft die gespielte alte App noch, nachdem sie den Wächter
/// gestartet hat — und hält dabei ihren Port.
const NACHLAUF_MS: u64 = 800;

/// Spielt A: startet aus der Vorher-Datei die Rolle „eltern". Die hält
/// einen Port (wie die App Port 8088 und ihre Einzelinstanz-Sperre),
/// startet den Wächter, wie es die App tun wird, läuft noch `NACHLAUF_MS`
/// weiter und endet. Das gespielte N beendet sich sofort, wenn der Port
/// noch belegt ist — der Erfolg beweist also, dass der Wächter wirklich auf
/// das Ende von A wartet **und** den Port nicht selbst geerbt hat.
fn uebergabe_starten(a: &Aufbau, frist: Duration, kind_rolle: &str) -> Instant {
    let port = freier_port();
    let args = WaechterArgs {
        daten: a.daten.clone(),
        von: "alt".into(),
        nach: env!("CARGO_PKG_VERSION").into(),
        frist,
        eltern_pid: None,
        kind_args: vec![
            ARG_PROBE.into(),
            kind_rolle.into(),
            a.daten.to_string_lossy().into_owned(),
            port.to_string(),
        ],
        // Nach einem Rückfall NICHT die volle App starten, nur den Probelauf.
        rueckfall_args: vec![ARG_SELBSTTEST.into()],
    };
    let status = Command::new(&a.pfade.vorher)
        .arg(ARG_PROBE)
        .arg("eltern")
        .arg(port.to_string())
        .arg(NACHLAUF_MS.to_string())
        .args(args.als_args())
        .stdin(Stdio::null())
        .status()
        .expect("die Vorher-Datei muss sich starten lassen");
    assert!(status.success(), "Rolle „eltern“ endete mit {status}");
    Instant::now()
}

fn protokoll(daten: &Path) -> String {
    std::fs::read_to_string(daten.join(tausch::WAECHTER_LOG)).unwrap_or_default()
}

#[test]
#[ignore = "Messwerkzeug — von Hand am Windows-PC: -- --ignored --nocapture"]
fn uebergabe_probe() {
    println!("\n=== Übergabe-Sonde ({}) ===", std::env::consts::OS);
    println!("Programmdatei: {}", programm().display());

    // ── 1. Erster Start einer frisch abgelegten Datei ────────────────────
    let a = aufbau();
    // Mit weit gefasster Obergrenze MESSEN statt an der Frist der App
    // abzubrechen: Wie lange ein Virenscanner eine frische Programmdatei
    // festhält, ist gerade die Zahl, um die es geht.
    let messen = |was: &str, datei: &Path| -> Duration {
        match tausch::probelauf_mit(datei, &[ARG_SELBSTTEST], MESS_OBERGRENZE) {
            Ok(dauer) => dauer,
            Err(e) => panic!(
                "{was}: Der Probelauf {e} (Obergrenze {} s). Dann läuft die Programmdatei \
                 auf diesem Rechner gar nicht durch — das ist das Ergebnis.",
                MESS_OBERGRENZE.as_secs()
            ),
        }
    };
    let vorhanden = messen("vorhandene Datei", &programm());
    println!(
        "MESSWERT start-der-vorhandenen-datei-ms {}",
        vorhanden.as_millis()
    );
    let erster = messen("frisch abgelegte Datei", &a.pfade.neu);
    let zweiter = messen("zweiter Start", &a.pfade.neu);
    println!("MESSWERT erster-start-ms {}", erster.as_millis());
    println!("MESSWERT zweiter-start-ms {}", zweiter.as_millis());
    if erster > tausch::PROBELAUF_FRIST {
        println!(
            "HINWEIS: Der erste Start liegt ÜBER der Probelauf-Frist der App ({} s). \
             Mit dieser Frist würde der Wechsel auf diesem Rechner nie angeboten.",
            tausch::PROBELAUF_FRIST.as_secs()
        );
    }
    // Die Fristen der folgenden Schritte richten sich nach dem Gemessenen —
    // jeder Aufbau legt wieder frische Dateien ab, die erneut geprüft werden.
    let frist = tausch::ANKUNFT_FRIST_START.max(erster * 3);
    let geduld = frist + tausch::ELTERN_FRIST + Duration::from_secs(15);
    println!(
        "Frist des Wächters in dieser Sonde: {} s (Startwert der App: {} s)",
        frist.as_secs(),
        tausch::ANKUNFT_FRIST_START.as_secs()
    );

    // ── 2. Laufende Programmdatei umbenennen ─────────────────────────────
    let mut laeuft = Command::new(&a.pfade.aktiv)
        .args([ARG_PROBE, "kind-haengt"])
        .stdin(Stdio::null())
        .spawn()
        .expect("aktive Datei starten");
    std::thread::sleep(Duration::from_millis(300));
    let original = sha256(&a.pfade.aktiv);
    let start = Instant::now();
    let tausch_ergebnis = tausch::tauschen(&a.pfade);
    let tausch_dauer = start.elapsed();
    let _ = laeuft.kill();
    let _ = laeuft.wait();
    tausch_ergebnis.expect(
        "ANNAHME 1 VERLETZT: Die laufende Programmdatei ließ sich nicht umbenennen — \
         dann bleibt der Kernwechsel beim Installer-Weg",
    );
    println!(
        "MESSWERT tausch-bei-laufender-datei-us {}",
        tausch_dauer.as_micros()
    );
    assert_eq!(sha256(&a.pfade.vorher), original, "vorher = die alte Datei");
    assert!(!a.pfade.neu.exists());

    // ── 3. Erfolg: Wächter überlebt A, startet N, N kommt an ─────────────
    // Ein „angekommen" von einer FRÜHEREN Übergabe liegt noch da. Es darf
    // nicht als Ankunft dieser gelten — der Wächter schreibt den Zustand
    // selbst neu.
    tausch::zustand_schreiben(
        &a.daten,
        &Zustand {
            von: "uralt".into(),
            nach: "alt".into(),
            zeit_ms: 1,
            lage: Lage::Angekommen,
            grund: None,
        },
    )
    .unwrap();
    let seit = uebergabe_starten(&a, frist, "kind-ok");
    let angekommen = warte_bis(geduld, || {
        tausch::zustand_lesen(&a.daten)
            .is_some_and(|z| z.lage == Lage::Angekommen && z.nach == env!("CARGO_PKG_VERSION"))
    });
    println!(
        "--- Protokoll des Wächters (Erfolg) ---\n{}",
        protokoll(&a.daten)
    );
    let angekommen = angekommen.expect(
        "ANNAHME 2 VERLETZT: Der Wächter hat das Ende der App nicht überlebt, hat \
         nicht auf ihr Ende gewartet oder hält ihren Port fest — die neue Version \
         wurde nie als angekommen gemeldet (siehe Protokoll oben)",
    );
    assert!(
        angekommen >= Duration::from_millis(NACHLAUF_MS / 2),
        "die neue Version darf erst starten, wenn die alte weg ist"
    );
    let _ = seit;
    println!(
        "MESSWERT ende-der-app-bis-ankunft-ms {}",
        angekommen.as_millis()
    );
    if let Some(ms) = zwischen(
        &protokoll(&a.daten),
        "Wächter gestartet",
        "neue Version gestartet",
    ) {
        println!("MESSWERT waechter-start-bis-neue-version-laeuft-ms {ms}");
    }
    assert!(
        warte_bis(Duration::from_secs(5), || protokoll(&a.daten)
            .contains("angekommen nach"))
        .is_some(),
        "der Wächter protokolliert die Ankunft"
    );
    assert!(a.pfade.vorher.exists(), "die Vorversion bleibt als Rückweg");
    assert!(!a.pfade.defekt.exists());

    // ── 4. Fehlstart: N stirbt sofort → Rückfall ─────────────────────────
    let b = aufbau_vorgewaermt();
    let alt = sha256(&b.pfade.aktiv);
    tausch::tauschen(&b.pfade).unwrap();
    let seit = uebergabe_starten(&b, frist, "kind-stirbt");
    let zurueck = warte_bis(geduld, || {
        tausch::zustand_lesen(&b.daten).is_some_and(|z| z.lage == Lage::Zurueckgefallen)
    });
    println!(
        "--- Protokoll des Wächters (Fehlstart) ---\n{}",
        protokoll(&b.daten)
    );
    zurueck.expect("nach einem Fehlstart muss der Wächter zurückfallen");
    println!(
        "MESSWERT fehlstart-bis-rueckfall-ms {}",
        seit.elapsed().as_millis()
    );
    if let Some(ms) = zwischen(
        &protokoll(&b.daten),
        "Wächter gestartet",
        "neue Version gestartet",
    ) {
        println!("MESSWERT fehlstart-waechter-start-bis-neue-version-laeuft-ms {ms}");
    }
    let z = tausch::zustand_lesen(&b.daten).unwrap();
    println!("Grund: {}", z.grund.clone().unwrap_or_default());
    assert!(z.grund.unwrap_or_default().contains("Code 3"));
    assert_eq!(
        sha256(&b.pfade.aktiv),
        alt,
        "die alte Datei ist wieder aktiv"
    );
    assert!(b.pfade.defekt.exists(), "die neue liegt als defekt daneben");
    assert!(
        b.daten.join("update-resume.json").exists(),
        "frischer Wiederanlauf-Marker für die alte Version"
    );
    assert!(
        warte_bis(geduld, || protokoll(&b.daten)
            .contains("alte Version wieder gestartet"))
        .is_some(),
        "die alte Version wird wieder gestartet"
    );

    // ── 5. Hänger: N meldet sich nie → Frist → Rückfall ──────────────────
    let c = aufbau_vorgewaermt();
    let alt = sha256(&c.pfade.aktiv);
    tausch::tauschen(&c.pfade).unwrap();
    let frist = Duration::from_millis(1500);
    let seit = uebergabe_starten(&c, frist, "kind-haengt");
    let zurueck = warte_bis(geduld, || {
        tausch::zustand_lesen(&c.daten).is_some_and(|z| z.lage == Lage::Zurueckgefallen)
    });
    println!(
        "--- Protokoll des Wächters (Hänger) ---\n{}",
        protokoll(&c.daten)
    );
    zurueck.expect("nach Ablauf der Frist muss der Wächter zurückfallen");
    let dauer = seit.elapsed();
    println!(
        "MESSWERT haenger-bis-rueckfall-ms {} (Frist 1500)",
        dauer.as_millis()
    );
    assert!(dauer >= frist, "nicht vor der Frist");
    // Die Frist zählt ab dem START der neuen Version, nicht ab dem Versuch,
    // sie zu starten: Blockiert der Virenscanner das Erzeugen des Prozesses,
    // darf diese Zeit die Frist nicht aufzehren.
    let gewartet = zwischen(
        &protokoll(&c.daten),
        "neue Version gestartet",
        "Rückfall: die neue Version meldete sich nicht",
    )
    .expect("Protokoll nennt Start und Rückfall");
    println!("MESSWERT haenger-start-bis-rueckfall-ms {gewartet} (Frist 1500)");
    assert!(
        gewartet + 50 >= frist.as_millis() as u64,
        "der Wächter hat nur {gewartet} ms auf die neue Version gewartet"
    );
    let z = tausch::zustand_lesen(&c.daten).unwrap();
    assert_eq!(
        (z.von.as_str(), z.nach.as_str()),
        ("alt", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(sha256(&c.pfade.aktiv), alt);
    assert!(tausch::zustand_lesen(&c.daten)
        .unwrap()
        .grund
        .unwrap_or_default()
        .contains("meldete sich nicht"));

    // ── 6. Die installierte App (nur lesen und Schreibtest) ──────────────
    installation_ansehen();

    println!(
        "\n=== Alle Schritte gingen durch. Die MESSWERT-Zeilen bitte in ADR 0100 eintragen. ==="
    );
}

/// Sieht sich die installierte bts-light an, ohne sie zu verändern.
fn installation_ansehen() {
    let kandidaten: Vec<PathBuf> = ["LOCALAPPDATA", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .flat_map(|basis| {
            ["BTS Light", "bts-light"]
                .iter()
                .map(|ordner| Path::new(&basis).join(ordner).join("bts-light.exe"))
                .collect::<Vec<_>>()
        })
        .collect();
    let Some(exe) = kandidaten.iter().find(|p| p.is_file()) else {
        println!("Installierte bts-light nicht gefunden (gesucht: {kandidaten:?}) — Schritt übersprungen.");
        return;
    };
    println!("Installiert: {}", exe.display());
    println!("MESSWERT installierte-exe-sha256 {}", sha256(exe));
    let pfade = Pfade::neben(exe).unwrap();
    match tausch::tausch_moeglich(&pfade, cfg!(windows)) {
        Ok(()) => println!("MESSWERT installationsverzeichnis beschreibbar"),
        Err(grund) => println!("MESSWERT installationsverzeichnis NICHT nutzbar: {grund}"),
    }
    for rest in [&pfade.neu, &pfade.vorher, &pfade.defekt] {
        if rest.exists() {
            println!(
                "Hinweis: {} liegt im Installationsverzeichnis.",
                rest.display()
            );
        }
    }
}
