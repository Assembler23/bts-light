//! Kernwechsel ohne Installer: Tausch der Programmdatei am Ort, Probelauf
//! und Start-Wächter (Spec `hotfix-ohne-neustart`, ADR 0100).
//!
//! **Stand: Messwerkzeug.** Die Bausteine hier sind fertig und getestet, aber
//! noch **nicht** an Banner und Update-Ablauf angeschlossen — das folgt erst,
//! wenn `tests/uebergabe_probe.rs` am Windows-PC gezeigt hat, dass die
//! Annahmen tragen (laufende Programmdatei umbenennbar, Wächter überlebt das
//! Ende der App, Dauer des ersten Starts unter dem Virenscanner).
//!
//! Der Ablauf, für den das hier gebaut ist (A = alte App, W = Wächter,
//! N = neue App):
//!
//! 1. A legt die neue Programmdatei als `bts-light.neu.exe` neben sich und
//!    lässt sie zur Probe laufen (`--selbsttest`).
//! 2. A tauscht per Umbenennen — **noch bei laufender Übertragung**, denn
//!    das Umbenennen kann unter Schutzsoftware sekundenlang blockieren
//!    (ADR 0100, vierter Lauf) und soll keine Lücke kosten. Erst danach hält
//!    A die Übertragung an und startet W — die **alte** Programmdatei mit
//!    `--waechter`, ohne Fenster.
//! 3. W wartet auf das Ende von A, startet N und wartet auf dessen
//!    Ankunftsmeldung. Kommt sie nicht, stellt W die alte Datei wieder her
//!    und startet sie.
//!
//! Warum der Wächter die alte Datei ist: Läuft die neue gar nicht an
//! (Virenscanner, beschädigte Datei), kann nur etwas zurücktauschen, das
//! davon unabhängig ist.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::update::ResumeMarker;

/// Probelauf der neuen Programmdatei: endet vor dem Fensteraufbau.
pub const ARG_SELBSTTEST: &str = "--selbsttest";
/// Wächter-Modus der alten Programmdatei.
pub const ARG_WAECHTER: &str = "--waechter";
/// Nur für `tests/uebergabe_probe.rs`: Rollen, die das Messwerkzeug braucht.
pub const ARG_PROBE: &str = "--uebergabe-probe";

/// Datei im App-Datenverzeichnis, über die A, W und N sich verständigen.
pub const ZUSTAND_DATEI: &str = "uebergabe.json";
/// Protokoll des Wächters (er läuft ohne das Logging der App).
pub const WAECHTER_LOG: &str = "uebergabe-waechter.log";

/// So lange darf der Probelauf dauern, bevor kein Wechsel angeboten wird.
///
/// Bewusst großzügig: Der Probelauf läuft im Hintergrund, **bevor** etwas
/// angeboten wird, und kostet keine Lücke. Gemessen am 16.11.2026 auf einem
/// Windows-11-Rechner mit Unternehmens-Schutzsoftware: Der erste Start einer
/// frisch abgelegten Programmdatei dauerte 15 s, jeder weitere 0,6 s. Der
/// Probelauf ist damit zugleich das „Vorwärmen" — die Prüfung der neuen
/// Datei ist erledigt, bevor jemand auf „Jetzt wechseln" klickt.
pub const PROBELAUF_FRIST: Duration = Duration::from_secs(120);
/// So lange wartet der Wächter auf das Ende der alten App, bevor er sie
/// beendet. Großzügig: Die App sichert beim Beenden Stände und schließt
/// Verbindungen — ein Abschuss mittendrin wäre schlimmer als ein paar
/// Sekunden mehr Lücke.
pub const ELTERN_FRIST: Duration = Duration::from_secs(20);
/// Luft nach dem Ende der alten App, bis Port und Einzelinstanz-Sperre
/// wirklich frei sind.
const ELTERN_NACHLAUF: Duration = Duration::from_millis(400);
/// Obergrenze des Wächter-Protokolls.
const WAECHTER_LOG_MAX: u64 = 256 * 1024;
/// Startwert der Frist für die Ankunftsmeldung. Der endgültige Wert folgt
/// aus der Messung am Windows-PC und wird in ADR 0100 festgehalten.
pub const ANKUNFT_FRIST_START: Duration = Duration::from_secs(15);

/// Die vier Dateinamen des Tauschs, alle im selben Verzeichnis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pfade {
    /// Die Programmdatei, die läuft bzw. laufen soll (`bts-light.exe`).
    pub aktiv: PathBuf,
    /// Die vorgeladene neue Version (`bts-light.neu.exe`).
    pub neu: PathBuf,
    /// Die vorige Version, Rückweg und Wächter (`bts-light.vorher.exe`).
    pub vorher: PathBuf,
    /// Eine Version, die nicht anlief (`bts-light.defekt.exe`).
    pub defekt: PathBuf,
}

impl Pfade {
    /// Leitet die Nachbar-Namen aus dem Pfad der aktiven Datei ab:
    /// `x.exe` → `x.neu.exe`, ohne Endung `x` → `x.neu`.
    pub fn neben(aktiv: &Path) -> Option<Self> {
        let dir = aktiv.parent()?;
        let name = aktiv.file_name()?.to_str()?;
        let (stamm, endung) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
            _ => (name, String::new()),
        };
        let mit = |zusatz: &str| dir.join(format!("{stamm}.{zusatz}{endung}"));
        Some(Self {
            aktiv: aktiv.to_path_buf(),
            neu: mit("neu"),
            vorher: mit("vorher"),
            defekt: mit("defekt"),
        })
    }

    /// Aus der Sicht des Wächters: Er läuft als `x.vorher.exe` und findet
    /// daraus die übrigen Namen.
    pub fn aus_vorher(vorher: &Path) -> Option<Self> {
        let name = vorher.file_name()?.to_str()?;
        let aktiv_name = match name.rsplit_once('.') {
            Some((s, e)) if s.ends_with(".vorher") => {
                format!("{}.{e}", s.strip_suffix(".vorher")?)
            }
            _ => name.strip_suffix(".vorher")?.to_string(),
        };
        Self::neben(&vorher.parent()?.join(aktiv_name))
    }
}

/// Warum der Tausch scheiterte.
#[derive(Debug)]
pub enum TauschFehler {
    /// Schon die aktive Datei ließ sich nicht beiseitelegen — nichts ist
    /// verändert.
    Beiseitelegen(std::io::Error),
    /// Die neue Datei ließ sich nicht an die Stelle setzen. `zurueck` sagt,
    /// ob die alte wieder an ihrem Platz ist.
    Einsetzen {
        fehler: std::io::Error,
        zurueck: bool,
    },
}

impl std::fmt::Display for TauschFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TauschFehler::Beiseitelegen(e) => {
                write!(f, "die laufende Programmdatei ließ sich nicht umbenennen ({e})")
            }
            TauschFehler::Einsetzen { fehler, zurueck: true } => write!(
                f,
                "die neue Programmdatei ließ sich nicht einsetzen ({fehler}); die alte ist wieder an ihrem Platz"
            ),
            TauschFehler::Einsetzen { fehler, zurueck: false } => write!(
                f,
                "die neue Programmdatei ließ sich nicht einsetzen ({fehler}), und die alte ließ sich nicht zurücklegen"
            ),
        }
    }
}

fn entferne_wenn_da(pfad: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(pfad) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Umbenennen mit Geduld: Unter Windows hält ein Virenscanner eine frisch
/// geschriebene oder eben gestartete Programmdatei oft sekundenlang
/// fest. Ein einzelner Versuch scheiterte dann an einer Sperre, die gleich
/// wieder weg ist. „Datei fehlt" wird nicht wiederholt.
fn umbenennen(von: &Path, nach: &Path) -> std::io::Result<()> {
    // Rund 6 s Geduld. Gemessen am 16.11.2026: Eine eben gestartete,
    // frische Programmdatei ließ sich unter Schutzsoftware erst nach 2,4 s
    // umbenennen — mit den früheren 2 s wäre das knapp gescheitert.
    const VERSUCHE: u32 = 30;
    let mut letzter = None;
    for versuch in 0..VERSUCHE {
        match std::fs::rename(von, nach) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(e) => letzter = Some(e),
        }
        if versuch + 1 < VERSUCHE {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    Err(letzter.unwrap_or_else(|| std::io::Error::other("Umbenennen gescheitert")))
}

/// Der Tausch: `aktiv` → `vorher`, `neu` → `aktiv`. Zwei Umbenennungen;
/// scheitert die zweite, wird die erste zurückgenommen.
///
/// Zwischen den beiden gibt es ein Fenster von Millisekunden, in dem keine
/// `aktiv`-Datei existiert. Fällt genau dann der Strom aus, hilft nur eine
/// Neuinstallation (bekannte Grenze, ADR 0100).
pub fn tauschen(pfade: &Pfade) -> Result<(), TauschFehler> {
    // Eine noch ältere Vorversion weicht — es gibt genau einen Rückweg. Ein
    // Rest, der sich nicht löschen lässt, scheitert gleich beim Umbenennen.
    let _ = entferne_wenn_da(&pfade.vorher);
    if !pfade.neu.is_file() {
        // Ohne neue Datei gar nicht erst anfangen — nichts wird verändert.
        return Err(TauschFehler::Einsetzen {
            fehler: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "die neue Programmdatei fehlt",
            ),
            zurueck: true,
        });
    }
    umbenennen(&pfade.aktiv, &pfade.vorher).map_err(TauschFehler::Beiseitelegen)?;
    if let Err(fehler) = umbenennen(&pfade.neu, &pfade.aktiv) {
        let zurueck = umbenennen(&pfade.vorher, &pfade.aktiv).is_ok();
        return Err(TauschFehler::Einsetzen { fehler, zurueck });
    }
    Ok(())
}

/// Der Rückfall: `aktiv` (lief nicht an) → `defekt`, `vorher` → `aktiv`.
///
/// Ohne Vorher-Datei wird **nichts** angefasst: Die nicht anlaufende Datei
/// beiseitezulegen, ohne etwas an ihre Stelle setzen zu können, wäre
/// schlimmer als sie liegen zu lassen.
pub fn zurueckfallen(pfade: &Pfade) -> std::io::Result<()> {
    if !pfade.vorher.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "die vorige Programmdatei fehlt",
        ));
    }
    // Liegt schon eine „defekt"-Datei da und lässt sich nicht löschen,
    // bekommt die neue einen eigenen Namen — der Rückfall scheitert nicht
    // an einem Rest von früher.
    let defekt = match entferne_wenn_da(&pfade.defekt) {
        Ok(()) => pfade.defekt.clone(),
        Err(_) => {
            let mut name = pfade.defekt.clone().into_os_string();
            name.push(format!(".{}", jetzt_ms()));
            PathBuf::from(name)
        }
    };
    // Fehlt die aktive Datei (halb getauscht), ist das kein Hindernis.
    match umbenennen(&pfade.aktiv, &defekt) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    umbenennen(&pfade.vorher, &pfade.aktiv)
}

/// Warum der Tausch gar nicht erst angeboten wird — dann bleibt der
/// bisherige Installer-Weg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hindernis {
    /// Der Kernwechsel ist nur für Windows gebaut und gemessen.
    NichtWindows,
    /// Im Installationsverzeichnis lässt sich nicht schreiben.
    KeinSchreibrecht(String),
}

impl std::fmt::Display for Hindernis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hindernis::NichtWindows => f.write_str("nur unter Windows"),
            Hindernis::KeinSchreibrecht(e) => {
                write!(f, "kein Schreibrecht im Installationsverzeichnis ({e})")
            }
        }
    }
}

/// Schreibtest im Verzeichnis: Datei anlegen, schreiben, wieder löschen.
pub fn schreibbar(dir: &Path) -> std::io::Result<()> {
    // Neu anlegen statt öffnen, mit nicht vorhersagbarem Namen: Eine schon
    // vorhandene Datei (oder ein Verweis) unter dem Namen wird so nie
    // gekürzt.
    let probe = dir.join(format!(
        ".bts-light-schreibtest-{}-{}",
        std::process::id(),
        jetzt_ms()
    ));
    let ergebnis = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .and_then(|mut f| f.write_all(b"x"));
    if ergebnis.is_ok() {
        let _ = std::fs::remove_file(&probe);
    }
    ergebnis
}

/// Kann hier überhaupt getauscht werden? `windows` wird hereingereicht,
/// damit beide Antworten testbar sind.
pub fn tausch_moeglich(pfade: &Pfade, windows: bool) -> Result<(), Hindernis> {
    if !windows {
        return Err(Hindernis::NichtWindows);
    }
    let dir = pfade.aktiv.parent().unwrap_or(Path::new("."));
    schreibbar(dir).map_err(|e| Hindernis::KeinSchreibrecht(e.to_string()))
}

/// Stand der Übergabe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lage {
    /// A hat getauscht und den Wächter gestartet.
    Gestartet,
    /// N läuft und hat sich gemeldet.
    Angekommen,
    /// N lief nicht an; die alte Version ist wieder aktiv.
    Zurueckgefallen,
}

/// Inhalt von `uebergabe.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zustand {
    /// Version, von der gewechselt wurde.
    pub von: String,
    /// Version, zu der gewechselt wurde.
    pub nach: String,
    /// Unix-ms des letzten Schreibens.
    pub zeit_ms: u64,
    pub lage: Lage,
    /// Warum zurückgefallen wurde.
    #[serde(default)]
    pub grund: Option<String>,
    /// Hat die Turnierleitung die Meldung dazu im Banner geschlossen? Sonst
    /// käme sie nach jedem Programmstart wieder.
    #[serde(default)]
    pub gesehen: bool,
}

/// Schreibt den Zustand über eine Nebendatei, damit nie ein halber liegt.
pub fn zustand_schreiben(daten: &Path, zustand: &Zustand) -> std::io::Result<()> {
    std::fs::create_dir_all(daten)?;
    let ziel = daten.join(ZUSTAND_DATEI);
    let neben = daten.join(format!("{ZUSTAND_DATEI}.neu"));
    std::fs::write(
        &neben,
        serde_json::to_vec_pretty(zustand).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(&neben, &ziel)
}

pub fn zustand_lesen(daten: &Path) -> Option<Zustand> {
    zustand_lesen_genau(daten).ok().flatten()
}

/// Wie [`zustand_lesen`], unterscheidet aber „es gibt keine Datei"
/// (`Ok(None)`) von „sie ließ sich gerade nicht lesen" (`Err`). Wer daraus
/// eine endgültige Entscheidung ableitet, braucht den Unterschied: Eine
/// kurz gesperrte Datei ist kein „keine Übergabe im Gang".
pub fn zustand_lesen_genau(daten: &Path) -> Result<Option<Zustand>, std::io::Error> {
    match std::fs::read(daten.join(ZUSTAND_DATEI)) {
        Ok(roh) => serde_json::from_slice(&roh)
            .map(Some)
            .map_err(std::io::Error::other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Merkt in der Datei, dass die Meldung zur letzten Übergabe gesehen wurde.
pub fn meldung_gesehen(daten: &Path) {
    if let Some(mut z) = zustand_lesen(daten) {
        if !z.gesehen {
            z.gesehen = true;
            let _ = zustand_schreiben(daten, &z);
        }
    }
}

/// N meldet sich an: Aus „gestartet" wird „angekommen" — aber nur, wenn die
/// Übergabe **zu dieser Version** führt. Alles andere (keine Übergabe im
/// Gang, schon zurückgefallen, eine andere Zielversion) bleibt, wie es ist.
///
/// `false` kann auch heißen, dass die Datei gerade gesperrt war: Der
/// Aufrufer versucht es dann noch einmal.
pub fn ankunft_melden(daten: &Path, jetzt_ms: u64, eigene_version: &str) -> bool {
    match zustand_lesen(daten) {
        Some(mut z) if z.lage == Lage::Gestartet && z.nach == eigene_version => {
            z.lage = Lage::Angekommen;
            z.zeit_ms = jetzt_ms;
            zustand_schreiben(daten, &z).is_ok()
        }
        _ => false,
    }
}

/// Was `--selbsttest` prüft, bevor irgendein Fenster entsteht: Die
/// Programmdatei lässt sich starten, ihre eingebetteten Seiten und der
/// Prüfschlüssel sind lesbar. Mehr soll es nicht sein — der Probelauf beweist
/// vor allem, dass das Betriebssystem (Virenscanner!) die Datei ausführt.
pub fn selbsttest() -> Result<(), String> {
    let stand = crate::tablet::assets::eingebauter_stand();
    if stand.marke().is_empty() {
        return Err("eingebettete Seiten ohne Marke".into());
    }
    crate::signatur::updater_schluessel().map_err(|e| e.to_string())?;
    Ok(())
}

/// Warum der Probelauf scheiterte.
#[derive(Debug)]
pub enum ProbeFehler {
    /// Die Datei ließ sich nicht starten.
    Start(std::io::Error),
    /// Sie endete nicht rechtzeitig und wurde beendet.
    Frist,
    /// Sie endete mit einem anderen Code als 0.
    Code(Option<i32>),
}

impl std::fmt::Display for ProbeFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeFehler::Start(e) => write!(f, "ließ sich nicht starten ({e})"),
            ProbeFehler::Frist => f.write_str("endete nicht rechtzeitig"),
            ProbeFehler::Code(Some(c)) => write!(f, "endete mit Code {c}"),
            ProbeFehler::Code(None) => f.write_str("wurde abgebrochen"),
        }
    }
}

/// Startet `exe` mit `args` und wartet höchstens `frist` auf Code 0.
/// Rückgabe: wie lange es dauerte — beim ersten Start einer neuen Datei ist
/// das im Wesentlichen die Prüfzeit des Virenscanners.
pub fn probelauf_mit(exe: &Path, args: &[&str], frist: Duration) -> Result<Duration, ProbeFehler> {
    let start = Instant::now();
    let mut kind = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(ProbeFehler::Start)?;
    loop {
        match kind.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(start.elapsed()),
            Ok(Some(status)) => return Err(ProbeFehler::Code(status.code())),
            Ok(None) if start.elapsed() >= frist => {
                let _ = kind.kill();
                let _ = kind.wait();
                return Err(ProbeFehler::Frist);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = kind.kill();
                let _ = kind.wait();
                return Err(ProbeFehler::Start(e));
            }
        }
    }
}

/// Der Probelauf der neuen Programmdatei.
pub fn probelauf(exe: &Path) -> Result<Duration, ProbeFehler> {
    probelauf_mit(exe, &[ARG_SELBSTTEST], PROBELAUF_FRIST)
}

/// Wie das Warten des Wächters ausging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaechterEnde {
    /// N hat sich gemeldet.
    Angekommen,
    /// N endete, bevor es sich meldete.
    KindEndete(Option<i32>),
    /// Die Frist lief ab; N wurde beendet.
    Frist,
}

/// Was der Wächter von N braucht — als Trait, damit das Warten ohne echten
/// Prozess testbar ist.
pub trait Kind {
    /// `Some(code)`, wenn der Prozess geendet hat.
    fn geendet(&mut self) -> Option<Option<i32>>;
    fn beenden(&mut self);
}

impl Kind for Child {
    fn geendet(&mut self) -> Option<Option<i32>> {
        match self.try_wait() {
            Ok(Some(status)) => Some(status.code()),
            Ok(None) => None,
            // Lässt sich der Prozess nicht mehr befragen, gilt er als weg.
            Err(_) => Some(None),
        }
    }

    fn beenden(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
    }
}

/// Wartet, bis N angekommen ist, endet oder die Frist abläuft. `vergangen`
/// liefert die seit dem Start von N verstrichene Zeit, `pause` schläft einen
/// Takt — beides hereingereicht, damit der Test keine echte Zeit braucht.
///
/// Reihenfolge je Takt: erst die Ankunft prüfen, dann das Ende. Meldet sich
/// N und endet im selben Takt, zählt die Ankunft.
pub fn warte_auf_ankunft(
    kind: &mut dyn Kind,
    angekommen: &mut dyn FnMut() -> bool,
    frist: Duration,
    vergangen: &mut dyn FnMut() -> Duration,
    pause: &mut dyn FnMut(),
) -> WaechterEnde {
    loop {
        if angekommen() {
            return WaechterEnde::Angekommen;
        }
        if let Some(code) = kind.geendet() {
            return WaechterEnde::KindEndete(code);
        }
        if vergangen() >= frist {
            kind.beenden();
            return WaechterEnde::Frist;
        }
        pause();
    }
}

/// Der vollständige Rückfall. **Kritisch** ist nur das Zurücktauschen der
/// Dateien; scheitert das, kommt der Fehler zurück. Grund und
/// Wiederanlauf-Marker sind Zugabe: Lassen sie sich nicht schreiben (Platte
/// voll, Datei gesperrt), wird das gemeldet, aber der Rückfall gilt — die
/// alte Version muss in jedem Fall wieder starten können.
///
/// Rückgabe: Meldungen über das, was nicht geschrieben werden konnte.
pub fn rueckfall(
    pfade: &Pfade,
    daten: &Path,
    grund: &str,
    jetzt_ms: u64,
    von: &str,
    nach: &str,
) -> std::io::Result<Vec<String>> {
    zurueckfallen(pfade)?;
    let mut maengel = Vec::new();
    if let Err(e) = zustand_schreiben(
        daten,
        &Zustand {
            von: von.to_string(),
            nach: nach.to_string(),
            zeit_ms: jetzt_ms,
            lage: Lage::Zurueckgefallen,
            grund: Some(grund.to_string()),
            gesehen: false,
        },
    ) {
        maengel.push(format!("Zustand nicht geschrieben ({e})"));
    }
    // Frischer Marker: Die alte Version soll die Übertragung nach ihrem
    // Start von selbst wieder aufnehmen — aber nur, wenn sie vor dem Wechsel
    // lief. Das steht im Marker, den die App vor dem Wechsel geschrieben
    // hat; fehlt er (oder ist er unlesbar), gilt „lief".
    let marker_pfad = daten.join(crate::update::RESUME_FILE);
    let lief = std::fs::read(&marker_pfad)
        .ok()
        .and_then(|roh| serde_json::from_slice::<ResumeMarker>(&roh).ok())
        .is_none_or(|m| m.running);
    if let Err(e) = crate::update::write_resume(
        &marker_pfad,
        &ResumeMarker {
            running: lief,
            written_ms: jetzt_ms,
            from_version: von.to_string(),
        },
    ) {
        maengel.push(format!("Wiederanlauf-Marker nicht geschrieben ({e})"));
    }
    Ok(maengel)
}

/// Aufrufzeile des Wächters. Die aktive Programmdatei steht bewusst **nicht**
/// darin: Der Wächter leitet sie aus seinem eigenen Namen ab
/// (`x.vorher.exe` → `x.exe`) und tut nichts, wenn er nicht so heißt. So
/// lässt er sich nicht auf beliebige Dateien ansetzen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaechterArgs {
    /// App-Datenverzeichnis (Zustand, Marker, Protokoll).
    pub daten: PathBuf,
    /// Version, von der gewechselt wird (die des Wächters).
    pub von: String,
    /// Version, zu der gewechselt wird.
    pub nach: String,
    pub frist: Duration,
    /// Prozessnummer der alten App. Der Wächter öffnet den Prozess gleich
    /// bei seinem Start und wartet auf **dessen** Ende.
    pub eltern_pid: Option<u32>,
    /// Nur für das Messwerkzeug (Debug-Build): Argumente für N.
    pub kind_args: Vec<String>,
    /// Nur für das Messwerkzeug (Debug-Build): Argumente für die alte App
    /// nach einem Rückfall.
    pub rueckfall_args: Vec<String>,
}

impl WaechterArgs {
    /// Als Argumentliste hinter `--waechter`.
    pub fn als_args(&self) -> Vec<String> {
        let mut a = vec![
            "--daten".to_string(),
            self.daten.to_string_lossy().into_owned(),
            "--von".to_string(),
            self.von.clone(),
            "--nach".to_string(),
            self.nach.clone(),
            "--frist-ms".to_string(),
            self.frist.as_millis().to_string(),
        ];
        if let Some(pid) = self.eltern_pid {
            a.extend(["--eltern-pid".to_string(), pid.to_string()]);
        }
        for arg in &self.kind_args {
            a.extend(["--kind-arg".to_string(), arg.clone()]);
        }
        for arg in &self.rueckfall_args {
            a.extend(["--rueckfall-arg".to_string(), arg.clone()]);
        }
        a
    }

    /// Liest die Argumentliste; `None` bei allem, was nicht passt.
    /// `mit_probe` lässt die Argumente des Messwerkzeugs zu — im
    /// ausgelieferten Programm sind sie abgeschaltet.
    pub fn lesen(args: &[String], mit_probe: bool) -> Option<Self> {
        let mut daten = None;
        let mut von = None;
        let mut nach = None;
        let mut frist = None;
        let mut eltern_pid = None;
        let mut kind_args = Vec::new();
        let mut rueckfall_args = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let wert = args.get(i + 1)?;
            match args[i].as_str() {
                "--daten" => daten = Some(PathBuf::from(wert)),
                "--von" => von = Some(wert.clone()),
                "--nach" => nach = Some(wert.clone()),
                "--frist-ms" => frist = Some(Duration::from_millis(wert.parse().ok()?)),
                "--eltern-pid" => eltern_pid = Some(wert.parse().ok()?),
                "--kind-arg" if mit_probe => kind_args.push(wert.clone()),
                "--rueckfall-arg" if mit_probe => rueckfall_args.push(wert.clone()),
                _ => return None,
            }
            i += 2;
        }
        Some(Self {
            daten: daten?,
            von: von?,
            nach: nach?,
            frist: frist?,
            eltern_pid,
            kind_args,
            rueckfall_args,
        })
    }
}

/// Richtet einen Prozess so ein, dass er das Ende des Aufrufers überlebt:
/// ohne Fenster, in eigener Prozessgruppe. `ausbrechen` versucht unter
/// Windows zusätzlich, aus einem Job des Aufrufers auszubrechen — steckt die
/// App in einem Job, der beim Schließen alle Prozesse beendet, stürbe der
/// Wächter sonst mit ihr.
fn losgeloest(befehl: &mut Command, ausbrechen: bool) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let mut flags = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
        if ausbrechen {
            flags |= CREATE_BREAKAWAY_FROM_JOB;
        }
        befehl.creation_flags(flags);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = ausbrechen;
        befehl.process_group(0);
    }
    befehl
}

/// Ein gestarteter Wächter. **Bis zum eigenen Prozessende halten:** Fällt
/// der Wert vorher weg, schließt sich die stdin-Leitung des Wächters, und er
/// hält die App für beendet, während sie noch läuft.
#[must_use = "bis zum Prozessende halten – sonst hält der Wächter die App zu früh für beendet"]
pub struct LaufenderWaechter(Child);

impl LaufenderWaechter {
    pub fn pid(&self) -> u32 {
        self.0.id()
    }
}

/// Startet den Wächter aus `vorher_exe`. Erst mit Ausbruch aus einem Job;
/// erlaubt der Job das nicht, ohne.
pub fn waechter_starten(
    vorher_exe: &Path,
    args: &WaechterArgs,
) -> std::io::Result<LaufenderWaechter> {
    let starte = |ausbrechen: bool| {
        losgeloest(
            Command::new(vorher_exe)
                .arg(ARG_WAECHTER)
                .args(args.als_args())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
            ausbrechen,
        )
        .spawn()
    };
    starte(true)
        .or_else(|_| starte(false))
        .map(LaufenderWaechter)
}

/// Zugriff auf den Prozess der alten App. Gleich beim Start des Wächters
/// geöffnet — dann zeigt er sicher auf **diesen** Prozess, auch wenn das
/// System die Nummer später neu vergibt.
struct Eltern {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    pid: u32,
}

#[cfg(windows)]
impl Eltern {
    fn oeffnen(pid: u32) -> Option<Self> {
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        };
        // SAFETY: reiner Systemaufruf; ein ungültiger Prozess ergibt Err.
        let handle =
            unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid) }.ok()?;
        Some(Self { handle })
    }

    /// `true`, sobald der Prozess geendet hat.
    fn warte_auf_ende(&self, frist: Duration) -> bool {
        use windows::Win32::Foundation::WAIT_OBJECT_0;
        use windows::Win32::System::Threading::WaitForSingleObject;
        let ms = frist.as_millis().min(u32::MAX as u128 - 1) as u32;
        // SAFETY: `handle` ist offen, solange `self` lebt.
        unsafe { WaitForSingleObject(self.handle, ms) == WAIT_OBJECT_0 }
    }

    fn beenden(&self) {
        use windows::Win32::System::Threading::TerminateProcess;
        // SAFETY: wie oben; das Handle trägt PROCESS_TERMINATE.
        unsafe {
            let _ = TerminateProcess(self.handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for Eltern {
    fn drop(&mut self) {
        // SAFETY: Das Handle gehört uns und wird genau einmal geschlossen.
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(windows))]
impl Eltern {
    fn oeffnen(pid: u32) -> Option<Self> {
        Some(Self { pid })
    }

    fn lebt(&self) -> bool {
        Command::new("kill")
            .args(["-0", &self.pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn warte_auf_ende(&self, frist: Duration) -> bool {
        let start = Instant::now();
        loop {
            if !self.lebt() {
                return true;
            }
            if start.elapsed() >= frist {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn beenden(&self) {
        let _ = Command::new("kill")
            .args(["-9", &self.pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn jetzt_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn protokoll(daten: &Path, zeile: &str) {
    let _ = std::fs::create_dir_all(daten);
    let pfad = daten.join(WAECHTER_LOG);
    // Kein endloses Wachsen: Über dem Deckel beginnt die Datei neu.
    if std::fs::metadata(&pfad).is_ok_and(|m| m.len() > WAECHTER_LOG_MAX) {
        let _ = std::fs::remove_file(&pfad);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&pfad)
    {
        let _ = writeln!(f, "{} {zeile}", jetzt_ms());
    }
}

/// Wartet darauf, dass die stdin-Leitung sich schließt — das Zeichen, dass
/// die alte App geendet hat. `false`, wenn die Frist vorher abläuft.
fn warte_auf_eltern_ende(frist: Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut puffer = [0u8; 64];
        let mut stdin = std::io::stdin();
        // Liest bis zum Ende der Leitung; Fehler zählen wie ein Ende.
        while matches!(stdin.read(&mut puffer), Ok(n) if n > 0) {}
        let _ = tx.send(());
    });
    rx.recv_timeout(frist).is_ok()
}

/// Wartet, bis die alte App wirklich weg ist — erst dann sind Port und
/// Einzelinstanz-Sperre frei. Startete N vorher, beendete es sich sofort
/// wieder („läuft schon"), und der Wächter hielte ein gutes Update für
/// gescheitert.
///
/// Zwei Zeichen, beide müssen stimmen: Die stdin-Leitung schließt sich
/// (schnell, aber auch dann, wenn die App den Wächter nur fallen lässt), und
/// der Prozess selbst hat geendet (verlässlich, braucht `--eltern-pid`).
fn warte_auf_alte_app(daten: &Path, eltern: Option<&Eltern>) {
    let start = Instant::now();
    let leitung_zu = warte_auf_eltern_ende(ELTERN_FRIST);
    match eltern {
        Some(eltern) => {
            let rest = ELTERN_FRIST.saturating_sub(start.elapsed());
            if !eltern.warte_auf_ende(rest) {
                protokoll(daten, "alte App endet nicht von selbst – wird beendet");
                eltern.beenden();
                eltern.warte_auf_ende(Duration::from_secs(5));
            }
        }
        None if !leitung_zu => {
            protokoll(daten, "kein Zeichen vom Ende der alten App – fahre fort");
        }
        None => {}
    }
    std::thread::sleep(ELTERN_NACHLAUF);
}

/// Wie [`warte_auf_ankunft`], wenn sich **keine** Ankunft melden lässt
/// (Zustand nicht schreibbar): Dann gilt als Erfolg, dass N die Frist
/// überlebt. Besser als ein gutes Update zurückzurollen, nur weil die
/// Meldung nicht ankommen konnte.
fn warte_ob_es_lebt(kind: &mut dyn Kind, frist: Duration) -> WaechterEnde {
    let start = Instant::now();
    loop {
        if let Some(code) = kind.geendet() {
            return WaechterEnde::KindEndete(code);
        }
        if start.elapsed() >= frist {
            return WaechterEnde::Angekommen;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Das Hauptprogramm des Wächters. Rückgabe: der Exit-Code (0 = neue
/// Version läuft, 1 = zurückgefallen und alte gestartet, 2 = falscher
/// Aufruf, 3 = Rückfall gescheitert).
pub fn waechter_hauptprogramm(args: &WaechterArgs) -> i32 {
    let daten = &args.daten;
    // Der Wächter läuft als `x.vorher.exe` und bewacht `x.exe` daneben —
    // nichts anderes.
    let Some(pfade) = std::env::current_exe()
        .ok()
        .and_then(|ich| Pfade::aus_vorher(&ich))
    else {
        return 2;
    };
    protokoll(daten, "Wächter gestartet");
    // Den Prozess der alten App SOFORT öffnen, solange er sicher noch lebt.
    {
        let eltern = args.eltern_pid.and_then(Eltern::oeffnen);
        warte_auf_alte_app(daten, eltern.as_ref());
    }

    // Den Zustand schreibt der Wächter selbst, unmittelbar vor dem Start
    // von N: Ein „angekommen" von einer FRÜHEREN Übergabe darf nie als
    // Ankunft dieser gelten.
    let ueberwacht = match zustand_schreiben(
        daten,
        &Zustand {
            von: args.von.clone(),
            nach: args.nach.clone(),
            zeit_ms: jetzt_ms(),
            lage: Lage::Gestartet,
            grund: None,
            gesehen: false,
        },
    ) {
        Ok(()) => true,
        Err(e) => {
            protokoll(
                daten,
                &format!(
                    "Zustand nicht schreibbar ({e}) – es zählt nur, ob die neue Version läuft"
                ),
            );
            false
        }
    };

    let grund = match Command::new(&pfade.aktiv)
        .args(&args.kind_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Err(e) => format!("die neue Version ließ sich nicht starten ({e})"),
        Ok(mut kind) => {
            // Die Frist läuft ERST AB HIER. Der Start selbst kann lange
            // dauern — unter einem Virenscanner blockiert schon das
            // Erzeugen des Prozesses, bis die Datei geprüft ist (gemessen
            // am 16.11.2026: bis zu 9 s). Zählte diese Zeit mit, wäre die
            // Frist abgelaufen, bevor die neue Version eine Zeile Code
            // ausgeführt hat, und ein gutes Update würde zurückgerollt.
            let start = Instant::now();
            protokoll(daten, "neue Version gestartet");
            let ende = if ueberwacht {
                warte_auf_ankunft(
                    &mut kind,
                    &mut || zustand_lesen(daten).is_some_and(|z| z.lage == Lage::Angekommen),
                    args.frist,
                    &mut || start.elapsed(),
                    &mut || std::thread::sleep(Duration::from_millis(100)),
                )
            } else {
                warte_ob_es_lebt(&mut kind, args.frist)
            };
            match ende {
                WaechterEnde::Angekommen => {
                    protokoll(
                        daten,
                        &format!(
                            "neue Version angekommen nach {} ms",
                            start.elapsed().as_millis()
                        ),
                    );
                    return 0;
                }
                WaechterEnde::KindEndete(code) => match code {
                    Some(c) => {
                        format!("die neue Version endete mit Code {c}, bevor sie sich meldete")
                    }
                    None => "die neue Version wurde beendet, bevor sie sich meldete".to_string(),
                },
                WaechterEnde::Frist => format!(
                    "die neue Version meldete sich nicht binnen {} ms",
                    args.frist.as_millis()
                ),
            }
        }
    };

    protokoll(daten, &format!("Rückfall: {grund}"));
    match rueckfall(&pfade, daten, &grund, jetzt_ms(), &args.von, &args.nach) {
        Ok(maengel) => {
            for mangel in maengel {
                protokoll(daten, &format!("Rückfall: {mangel}"));
            }
        }
        Err(e) => {
            protokoll(daten, &format!("Rückfall gescheitert: {e}"));
            return 3;
        }
    }
    // Die alte Version startet in JEDEM Fall, sobald die Dateien wieder an
    // ihrem Platz sind.
    let starte = |ausbrechen: bool| {
        losgeloest(
            Command::new(&pfade.aktiv)
                .args(&args.rueckfall_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
            ausbrechen,
        )
        .spawn()
    };
    match starte(true).or_else(|_| starte(false)) {
        Ok(_) => {
            protokoll(daten, "alte Version wieder gestartet");
            1
        }
        Err(e) => {
            protokoll(daten, &format!("alte Version ließ sich nicht starten: {e}"));
            3
        }
    }
}

/// Rollen des Messwerkzeugs (`--uebergabe-probe <rolle> …`), nur im
/// Debug-Build. Sie spielen A und N mit derselben Programmdatei nach — samt
/// dem, woran der echte Ablauf scheitern kann: A hält einen Port und endet
/// erst nach einer Weile; N beendet sich sofort, wenn der Port noch belegt
/// ist (wie die volle App an ihrer Einzelinstanz-Sperre).
fn probe_rolle(rolle: &str, rest: &[String]) -> i32 {
    match rolle {
        // N, das ankommt: `kind-ok <daten> <port>`.
        "kind-ok" => {
            let (Some(daten), Some(port)) = (
                rest.first().map(PathBuf::from),
                rest.get(1).and_then(|p| p.parse::<u16>().ok()),
            ) else {
                return 2;
            };
            // Wie die volle App: Ist „ihr" Port noch belegt, läuft die alte
            // noch (oder der Wächter hält ihn fest) — sofort enden.
            let _port = if port == 0 {
                None
            } else {
                match std::net::TcpListener::bind(("127.0.0.1", port)) {
                    Ok(l) => Some(l),
                    Err(_) => return 0,
                }
            };
            std::thread::sleep(Duration::from_millis(300));
            ankunft_melden(&daten, jetzt_ms(), env!("CARGO_PKG_VERSION"));
            std::thread::sleep(Duration::from_secs(2));
            0
        }
        // N, das sofort stirbt.
        "kind-stirbt" => 3,
        // N, das hängt und sich nie meldet.
        "kind-haengt" => {
            std::thread::sleep(Duration::from_secs(120));
            0
        }
        // A: `eltern <port> <nachlauf-ms> <Wächter-Argumente…>`. Hält den
        // Port, startet den Wächter, wie es die App tun wird, läuft noch
        // eine Weile und endet dann.
        "eltern" => {
            let (Some(port), Some(nachlauf)) = (
                rest.first().and_then(|p| p.parse::<u16>().ok()),
                rest.get(1).and_then(|p| p.parse::<u64>().ok()),
            ) else {
                return 2;
            };
            let Some(args) = WaechterArgs::lesen(&rest[2..], true) else {
                return 2;
            };
            let Ok(ich) = std::env::current_exe() else {
                return 2;
            };
            let _port = if port == 0 {
                None
            } else {
                match std::net::TcpListener::bind(("127.0.0.1", port)) {
                    Ok(l) => Some(l),
                    Err(_) => return 5,
                }
            };
            let args = WaechterArgs {
                eltern_pid: Some(std::process::id()),
                ..args
            };
            let Ok(_waechter) = waechter_starten(&ich, &args) else {
                return 4;
            };
            std::thread::sleep(Duration::from_millis(nachlauf));
            0
        }
        _ => 2,
    }
}

/// Einstieg aus `main`: Erkennt die Sonderaufrufe, **bevor** Tauri startet.
/// `Some(code)` heißt: sofort mit diesem Code enden. `None`: normale App.
///
/// Die Rollen und Zusatzargumente des Messwerkzeugs gibt es nur im
/// Debug-Build; das ausgelieferte Programm kennt sie nicht.
pub fn einstieg(args: &[String]) -> Option<i32> {
    let mit_probe = cfg!(debug_assertions);
    match args.first().map(String::as_str) {
        Some(ARG_SELBSTTEST) => Some(match selbsttest() {
            Ok(()) => 0,
            Err(_) => 1,
        }),
        Some(ARG_WAECHTER) => Some(match WaechterArgs::lesen(&args[1..], mit_probe) {
            Some(a) => waechter_hauptprogramm(&a),
            None => 2,
        }),
        Some(ARG_PROBE) if mit_probe => Some(match args.get(1) {
            Some(rolle) => probe_rolle(rolle, &args[2..]),
            None => 2,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pfade_in(dir: &Path) -> Pfade {
        Pfade::neben(&dir.join("bts-light.exe")).unwrap()
    }

    fn inhalt(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn nachbarnamen_folgen_dem_namen_der_aktiven_datei() {
        let p = Pfade::neben(Path::new("/x/bts-light.exe")).unwrap();
        assert_eq!(p.neu, Path::new("/x/bts-light.neu.exe"));
        assert_eq!(p.vorher, Path::new("/x/bts-light.vorher.exe"));
        assert_eq!(p.defekt, Path::new("/x/bts-light.defekt.exe"));
        // Ohne Endung (Linux).
        let p = Pfade::neben(Path::new("/x/bts-light")).unwrap();
        assert_eq!(p.neu, Path::new("/x/bts-light.neu"));
        // Der Wächter findet von seinem eigenen Namen zurück.
        assert_eq!(
            Pfade::aus_vorher(Path::new("/x/bts-light.vorher.exe"))
                .unwrap()
                .aktiv,
            Path::new("/x/bts-light.exe")
        );
        assert_eq!(
            Pfade::aus_vorher(Path::new("/x/bts-light.vorher"))
                .unwrap()
                .aktiv,
            Path::new("/x/bts-light")
        );
        assert!(Pfade::aus_vorher(Path::new("/x/bts-light.exe")).is_none());
    }

    #[test]
    fn tausch_plan_reihenfolge() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "alt").unwrap();
        std::fs::write(&p.neu, "neu").unwrap();
        std::fs::write(&p.vorher, "uralt").unwrap();

        tauschen(&p).unwrap();
        assert_eq!(inhalt(&p.aktiv), "neu");
        assert_eq!(
            inhalt(&p.vorher),
            "alt",
            "die uralte Vorversion ist gewichen"
        );
        assert!(!p.neu.exists());

        // Und zurück: die neue wird „defekt", die alte wieder aktiv.
        zurueckfallen(&p).unwrap();
        assert_eq!(inhalt(&p.aktiv), "alt");
        assert_eq!(inhalt(&p.defekt), "neu");
        assert!(!p.vorher.exists());
    }

    #[test]
    fn zweites_umbenennen_scheitert_rollt_zurueck() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "alt").unwrap();
        // Keine neue Datei: gar nicht erst anfangen.
        let fehler = tauschen(&p).unwrap_err();
        assert!(
            matches!(fehler, TauschFehler::Einsetzen { zurueck: true, .. }),
            "{fehler}"
        );
        assert_eq!(inhalt(&p.aktiv), "alt");
        assert!(!p.vorher.exists());

        // Die neue „Datei" ist ein Ordner, an der Stelle der aktiven liegt
        // nach dem ersten Schritt nichts — das Einsetzen scheitert erst
        // beim zweiten Umbenennen … nein: ein Ordner lässt sich umbenennen.
        // Deshalb den echten Fehlschlag über ein belegtes Ziel erzwingen:
        // `aktiv` ist nach Schritt 1 frei, also muss `neu` verschwinden,
        // BEVOR Schritt 2 läuft. Das geht nur von außen — hier genügt der
        // Nachweis der Rücknahme über die Bausteine.
        std::fs::write(&p.neu, "neu").unwrap();
        umbenennen(&p.aktiv, &p.vorher).unwrap();
        std::fs::remove_file(&p.neu).unwrap();
        assert!(umbenennen(&p.neu, &p.aktiv).is_err());
        umbenennen(&p.vorher, &p.aktiv).unwrap();
        assert_eq!(
            inhalt(&p.aktiv),
            "alt",
            "die alte Datei ist wieder an ihrem Platz"
        );

        // Fehlt schon die aktive Datei, ist nichts verändert.
        std::fs::remove_file(&p.aktiv).unwrap();
        std::fs::write(&p.neu, "neu").unwrap();
        assert!(matches!(tauschen(&p), Err(TauschFehler::Beiseitelegen(_))));
        assert_eq!(inhalt(&p.neu), "neu");
    }

    #[test]
    fn rueckfall_ohne_aktive_datei_stellt_die_alte_trotzdem_her() {
        // Halb getauscht (Stromausfall zwischen den Umbenennungen).
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.vorher, "alt").unwrap();
        zurueckfallen(&p).unwrap();
        assert_eq!(inhalt(&p.aktiv), "alt");
    }

    /// Ohne Vorher-Datei wird die aktive NICHT beiseitegelegt — sonst läge
    /// danach gar keine Programmdatei mehr an ihrem Platz.
    #[test]
    fn rueckfall_ohne_vorige_datei_fasst_nichts_an() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "neu").unwrap();
        assert!(zurueckfallen(&p).is_err());
        assert_eq!(inhalt(&p.aktiv), "neu");
        assert!(!p.defekt.exists());
    }

    /// Ein nicht löschbarer Rest unter dem „defekt"-Namen (hier ein
    /// gefüllter Ordner) hält den Rückfall nicht auf.
    #[test]
    fn rueckfall_trotz_rest_unter_dem_defekt_namen() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "neu").unwrap();
        std::fs::write(&p.vorher, "alt").unwrap();
        std::fs::create_dir(&p.defekt).unwrap();
        std::fs::write(p.defekt.join("x"), "x").unwrap();

        zurueckfallen(&p).unwrap();
        assert_eq!(inhalt(&p.aktiv), "alt");
        let beiseite: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("bts-light.defekt.exe."))
            .collect();
        assert_eq!(
            beiseite.len(),
            1,
            "die neue liegt unter eigenem Namen: {beiseite:?}"
        );
    }

    #[test]
    fn nicht_windows_meldet_grund() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        assert_eq!(tausch_moeglich(&p, false), Err(Hindernis::NichtWindows));
        assert_eq!(Hindernis::NichtWindows.to_string(), "nur unter Windows");
        assert_eq!(tausch_moeglich(&p, true), Ok(()));
        // Der Schreibtest hinterlässt nichts.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn kein_schreibrecht_waehlt_installer_weg() {
        // Das „Verzeichnis" gibt es nicht: Schreiben scheitert.
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(&dir.path().join("gibt-es-nicht"));
        assert!(matches!(
            tausch_moeglich(&p, true),
            Err(Hindernis::KeinSchreibrecht(_))
        ));
    }

    /// Ein Kind, das der Test steuert.
    struct Spielkind {
        endet_nach: Option<(u32, Option<i32>)>,
        abfragen: u32,
        beendet: bool,
    }

    impl Kind for Spielkind {
        fn geendet(&mut self) -> Option<Option<i32>> {
            self.abfragen += 1;
            match self.endet_nach {
                Some((n, code)) if self.abfragen > n => Some(code),
                _ => None,
            }
        }
        fn beenden(&mut self) {
            self.beendet = true;
        }
    }

    fn warte(kind: &mut Spielkind, ankunft_nach: Option<u32>, frist_takte: u32) -> WaechterEnde {
        let takt = std::cell::Cell::new(0u32);
        warte_auf_ankunft(
            kind,
            &mut || ankunft_nach.is_some_and(|n| takt.get() >= n),
            Duration::from_millis(100 * frist_takte as u64),
            &mut || Duration::from_millis(100 * takt.get() as u64),
            &mut || takt.set(takt.get() + 1),
        )
    }

    #[test]
    fn waechter_erfolg_bei_ankunft() {
        let mut kind = Spielkind {
            endet_nach: None,
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(warte(&mut kind, Some(3), 50), WaechterEnde::Angekommen);
        assert!(!kind.beendet, "ein angekommenes Kind läuft weiter");

        // Ankunft und Ende im selben Takt: Die Ankunft zählt.
        let mut kind = Spielkind {
            endet_nach: Some((0, Some(0))),
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(warte(&mut kind, Some(0), 50), WaechterEnde::Angekommen);
    }

    #[test]
    fn waechter_rueckfall_bei_kind_ende() {
        let mut kind = Spielkind {
            endet_nach: Some((2, Some(3))),
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(
            warte(&mut kind, None, 50),
            WaechterEnde::KindEndete(Some(3))
        );
        // Auch ein „sauberes" Ende vor der Meldung ist ein Fehlstart.
        let mut kind = Spielkind {
            endet_nach: Some((0, Some(0))),
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(
            warte(&mut kind, Some(9), 50),
            WaechterEnde::KindEndete(Some(0))
        );
    }

    #[test]
    fn waechter_rueckfall_bei_frist() {
        let mut kind = Spielkind {
            endet_nach: None,
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(warte(&mut kind, None, 5), WaechterEnde::Frist);
        assert!(kind.beendet, "ein hängendes Kind wird beendet");
        // Eine Ankunft NACH der Frist zählt nicht mehr.
        let mut kind = Spielkind {
            endet_nach: None,
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(warte(&mut kind, Some(9), 5), WaechterEnde::Frist);
    }

    #[test]
    fn rueckfall_schreibt_frischen_marker_und_grund() {
        let dir = tempfile::tempdir().unwrap();
        let daten = dir.path().join("daten");
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "neu").unwrap();
        std::fs::write(&p.vorher, "alt").unwrap();

        let maengel = rueckfall(&p, &daten, "meldete sich nicht", 9_000, "1.2.3", "1.2.4").unwrap();
        assert!(maengel.is_empty(), "{maengel:?}");

        assert_eq!(inhalt(&p.aktiv), "alt");
        assert_eq!(inhalt(&p.defekt), "neu");
        let z = zustand_lesen(&daten).unwrap();
        assert_eq!(z.lage, Lage::Zurueckgefallen);
        assert_eq!(z.grund.as_deref(), Some("meldete sich nicht"));
        assert_eq!((z.von.as_str(), z.nach.as_str()), ("1.2.3", "1.2.4"));
        assert_eq!(z.zeit_ms, 9_000);
        // Der Marker ist frisch: Die alte Version nimmt die Übertragung von
        // selbst wieder auf.
        let marker =
            crate::update::take_resume_marker(&daten.join(crate::update::RESUME_FILE), 9_500)
                .expect("frischer Marker");
        assert!(marker.running);
        assert_eq!(marker.written_ms, 9_000);
        assert_eq!(marker.from_version, "1.2.3");
    }

    /// Lief die Übertragung vor dem Wechsel NICHT, startet die alte Version
    /// sie nach einem Rückfall auch nicht von selbst. Das steht im Marker,
    /// den die App vor dem Wechsel geschrieben hat.
    #[test]
    fn rueckfall_uebernimmt_ob_die_uebertragung_lief() {
        for lief in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let daten = dir.path().join("daten");
            let p = pfade_in(dir.path());
            std::fs::write(&p.aktiv, "neu").unwrap();
            std::fs::write(&p.vorher, "alt").unwrap();
            let marker = daten.join(crate::update::RESUME_FILE);
            crate::update::write_resume(
                &marker,
                &ResumeMarker {
                    running: lief,
                    written_ms: 1_000,
                    from_version: "1.2.3".into(),
                },
            )
            .unwrap();

            rueckfall(&p, &daten, "x", 9_000, "1.2.3", "1.2.4").unwrap();
            let gelesen: ResumeMarker =
                serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
            assert_eq!(gelesen.running, lief, "lief = {lief}");
            assert_eq!(gelesen.written_ms, 9_000, "der Marker ist frisch");
        }
    }

    #[test]
    fn schreibtest_kuerzt_keine_vorhandene_datei() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wichtig.txt"), "inhalt").unwrap();
        schreibbar(dir.path()).unwrap();
        schreibbar(dir.path()).unwrap();
        let namen: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            namen,
            vec!["wichtig.txt"],
            "der Schreibtest hinterlässt nichts"
        );
    }

    /// Lässt sich im Datenverzeichnis nichts schreiben (hier: Es ist eine
    /// Datei), gilt der Rückfall trotzdem — die Dateien sind zurück, und der
    /// Aufrufer startet die alte Version. Ohne das stünde die Turnierleitung
    /// nach einem Fehlstart ganz ohne App da.
    #[test]
    fn rueckfall_gilt_auch_ohne_schreibbares_datenverzeichnis() {
        let dir = tempfile::tempdir().unwrap();
        let daten = dir.path().join("daten");
        std::fs::write(&daten, "keine Verzeichnis").unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "neu").unwrap();
        std::fs::write(&p.vorher, "alt").unwrap();

        let maengel = rueckfall(&p, &daten, "x", 1, "1.2.3", "1.2.4").expect("Rückfall gilt");
        assert_eq!(maengel.len(), 2, "{maengel:?}");
        assert!(maengel[0].contains("Zustand nicht geschrieben"));
        assert!(maengel[1].contains("Marker nicht geschrieben"));
        assert_eq!(inhalt(&p.aktiv), "alt");
    }

    #[test]
    fn ankunft_gilt_nur_waehrend_einer_uebergabe() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            !ankunft_melden(dir.path(), 1, "2"),
            "ohne Übergabe nichts zu melden"
        );
        let mut z = Zustand {
            von: "1".into(),
            nach: "2".into(),
            zeit_ms: 1,
            lage: Lage::Gestartet,
            grund: None,
            gesehen: false,
        };
        zustand_schreiben(dir.path(), &z).unwrap();
        // Eine ANDERE Version als das Ziel meldet nichts an.
        assert!(!ankunft_melden(dir.path(), 5, "3"));
        assert_eq!(zustand_lesen(dir.path()).unwrap().lage, Lage::Gestartet);
        assert!(ankunft_melden(dir.path(), 7, "2"));
        let gelesen = zustand_lesen(dir.path()).unwrap();
        assert_eq!((gelesen.lage, gelesen.zeit_ms), (Lage::Angekommen, 7));
        // Schon angekommen: kein zweites Mal.
        assert!(!ankunft_melden(dir.path(), 8, "2"));
        // Nach einem Rückfall bleibt es dabei — eine späte Meldung ändert
        // nichts mehr.
        z.lage = Lage::Zurueckgefallen;
        zustand_schreiben(dir.path(), &z).unwrap();
        assert!(!ankunft_melden(dir.path(), 9, "2"));
        assert_eq!(
            zustand_lesen(dir.path()).unwrap().lage,
            Lage::Zurueckgefallen
        );
        // Die Datei trägt die Lage klein geschrieben.
        assert!(inhalt(&dir.path().join(ZUSTAND_DATEI)).contains("\"zurueckgefallen\""));
    }

    /// Ohne schreibbaren Zustand zählt, ob die neue Version die Frist
    /// überlebt — nicht eine Meldung, die nie ankommen könnte.
    #[test]
    fn ohne_zustand_zaehlt_ob_die_neue_version_lebt() {
        let mut lebt = Spielkind {
            endet_nach: None,
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(
            warte_ob_es_lebt(&mut lebt, Duration::from_millis(150)),
            WaechterEnde::Angekommen
        );
        assert!(!lebt.beendet);
        let mut stirbt = Spielkind {
            endet_nach: Some((0, Some(1))),
            abfragen: 0,
            beendet: false,
        };
        assert_eq!(
            warte_ob_es_lebt(&mut stirbt, Duration::from_secs(5)),
            WaechterEnde::KindEndete(Some(1))
        );
    }

    #[test]
    fn waechter_args_ueberstehen_hin_und_zurueck() {
        let args = WaechterArgs {
            daten: PathBuf::from("/d mit leer/zeichen"),
            von: "1.2.3".into(),
            nach: "1.2.4".into(),
            frist: Duration::from_millis(15_000),
            eltern_pid: Some(4711),
            kind_args: vec!["--a".into(), "b".into()],
            rueckfall_args: vec![ARG_SELBSTTEST.into()],
        };
        assert_eq!(
            WaechterArgs::lesen(&args.als_args(), true),
            Some(args.clone())
        );
        // Das ausgelieferte Programm kennt die Argumente des Messwerkzeugs
        // nicht: Mit ihnen tut der Wächter nichts.
        assert_eq!(WaechterArgs::lesen(&args.als_args(), false), None);
        // Ohne die optionalen Teile — so ruft die App.
        let knapp = WaechterArgs {
            eltern_pid: None,
            kind_args: vec![],
            rueckfall_args: vec![],
            ..args
        };
        assert_eq!(
            WaechterArgs::lesen(&knapp.als_args(), false),
            Some(knapp.clone())
        );
        // Unbekanntes, Unvollständiges, Fehlendes: lieber nichts tun.
        let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let voll = [
            "--daten",
            "/d",
            "--von",
            "1",
            "--nach",
            "2",
            "--frist-ms",
            "1",
        ];
        assert!(WaechterArgs::lesen(&s(&voll), false).is_some());
        for fehlt in 0..4 {
            let mut ohne = voll.to_vec();
            ohne.drain(fehlt * 2..fehlt * 2 + 2);
            assert_eq!(WaechterArgs::lesen(&s(&ohne), false), None, "{ohne:?}");
        }
        assert_eq!(WaechterArgs::lesen(&s(&voll[..7]), false), None);
        let mut krumm = voll.to_vec();
        krumm[7] = "bald";
        assert_eq!(WaechterArgs::lesen(&s(&krumm), false), None);
        // Der Pfad der Programmdatei lässt sich NICHT vorgeben.
        let mut fremd = voll.to_vec();
        fremd.extend(["--aktiv", "/x/boese.exe"]);
        assert_eq!(WaechterArgs::lesen(&s(&fremd), true), None);
    }

    #[test]
    fn einstieg_erkennt_nur_die_sonderaufrufe() {
        let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(einstieg(&[]), None, "normaler Start");
        assert_eq!(einstieg(&s(&["--irgendwas"])), None);
        assert_eq!(einstieg(&s(&[ARG_SELBSTTEST])), Some(0));
        // Wächter ohne brauchbare Argumente tut nichts.
        assert_eq!(einstieg(&s(&[ARG_WAECHTER])), Some(2));
        // Mit brauchbaren Argumenten, aber nicht als `….vorher` gestartet
        // (dieser Test läuft als Test-Programm): ebenfalls nichts.
        let dir = tempfile::tempdir().unwrap();
        let daten = dir.path().to_string_lossy().into_owned();
        assert_eq!(
            einstieg(&s(&[
                ARG_WAECHTER,
                "--daten",
                &daten,
                "--von",
                "1",
                "--nach",
                "2",
                "--frist-ms",
                "1"
            ])),
            Some(2)
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "ein falsch gestarteter Wächter schreibt nichts"
        );
        // Tests laufen im Debug-Build: Die Rollen des Messwerkzeugs gibt es.
        assert_eq!(einstieg(&s(&[ARG_PROBE])), Some(2));
        assert_eq!(einstieg(&s(&[ARG_PROBE, "kind-stirbt"])), Some(3));
        assert_eq!(einstieg(&s(&[ARG_PROBE, "unbekannt"])), Some(2));
    }

    #[test]
    fn selbsttest_besteht_mit_den_eingebetteten_seiten() {
        assert_eq!(selbsttest(), Ok(()));
    }

    #[cfg(unix)]
    #[test]
    fn probelauf_gescheitert_kein_tausch_angebot() {
        let sh = Path::new("/bin/sh");
        // Code 0: bestanden.
        assert!(probelauf_mit(sh, &["-c", "exit 0"], Duration::from_secs(5)).is_ok());
        // Anderer Code: gescheitert.
        assert!(matches!(
            probelauf_mit(sh, &["-c", "exit 7"], Duration::from_secs(5)),
            Err(ProbeFehler::Code(Some(7)))
        ));
        // Hängt: nach der Frist beendet.
        let start = Instant::now();
        assert!(matches!(
            probelauf_mit(sh, &["-c", "sleep 30"], Duration::from_millis(300)),
            Err(ProbeFehler::Frist)
        ));
        assert!(start.elapsed() < Duration::from_secs(5));
        // Lässt sich nicht starten (Virenscanner sperrt, Datei fehlt).
        assert!(matches!(
            probelauf_mit(Path::new("/gibt/es/nicht"), &[], Duration::from_secs(1)),
            Err(ProbeFehler::Start(_))
        ));
    }
}
