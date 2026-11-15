//! Messwerkzeug: Lässt sich der Pfad der geöffneten `.TP`-Datei aus dem
//! laufenden BTP-Prozess ablesen? Grundlage für eine Automatik „TP-Datei
//! suchen" im Setup (noch ohne Spec — erst messen, dann `/idee`).
//!
//! Bekannt seit 26.10.2026 (`docs/btp_protocol.md`): Der Prozess heißt
//! `btp.exe`, der Fenstertitel `Badminton Turnier Planer - <Dateiname ohne
//! .TP>`, und BTP hält die Datei dauerhaft offen. Die Sonde prüft, ob wir
//! daraus den **vollen Pfad** gewinnen:
//!
//! 1. **Kommandozeile** des Prozesses (trägt den Pfad nur nach Doppelklick
//!    auf die Datei, nicht nach „Datei → Öffnen").
//! 2. **Fenstertitel** (liefert nur den Dateinamen — die Gegenprobe).
//! 3. **Offene Datei-Handles**: jedes Handle des Prozesses in unseren
//!    duplizieren und nach seinem Pfad fragen. Nur dokumentierte Aufrufe
//!    (`DuplicateHandle`, `GetFileType`, `GetFinalPathNameByHandleW`).
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht Windows und ein
//! laufendes BTP mit geöffnetem Turnier:
//!
//! ```text
//! cargo test -p bts-light --test btp_tp_pfad_probe -- --ignored --nocapture
//! ```
//!
//! Read-only: Die Sonde dupliziert Handles **ohne** `DUPLICATE_CLOSE_SOURCE`
//! und schließt nur die eigenen Kopien; BTP verliert nichts. Aus der Datei
//! liest sie 19 Bytes (Jet-Kopf).
//!
//! ## Was zu messen ist (bitte jeden Fall einmal laufen lassen)
//!
//! | Fall | Frage |
//! |---|---|
//! | BTP per Doppelklick auf die `.TP` gestartet | Steht der Pfad in der Kommandozeile? |
//! | BTP gestartet, dann „Datei → Öffnen" | Finden die Handles die Datei trotzdem? |
//! | BTP „Als Administrator", Sonde normal | Meldet die Sonde „kein Zugriff"? |
//! | Datei auf Netzlaufwerk / USB-Stick | Ist der gelieferte Pfad lesbar (UNC)? |
//! | Zwei BTP-Fenster mit zwei Turnieren | Ordnet der Fenstertitel richtig zu? |
//! | Virenscanner (Defender) aktiv | Meldung oder Blockade? |
//!
//! Dazu die Zeilen „Hänger" und „Dauer" der Ausgabe notieren: Ein Hänger
//! heißt, dass `GetFileType` an einem Handle (typisch: Pipe) blockiert hat —
//! die Automatik bräuchte dann dieselbe Frist wie die Sonde.
//!
//! Ergebnisse gehören nach `docs/btp_protocol.md` (Abschnitt „Match-ID und
//! Turnier-GUID gegen die TP-Datei", Punkt „BTP-Prozess").
#![cfg(windows)]

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

type Handle = isize;
type Bool = i32;

const TH32CS_SNAPPROCESS: u32 = 0x2;
const INVALID_HANDLE_VALUE: Handle = -1;
const PROCESS_DUP_HANDLE: u32 = 0x0040;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
/// Bewusst **nur** dieses Flag — nie `DUPLICATE_CLOSE_SOURCE` (0x1), das
/// würde BTP das Handle wegnehmen.
const DUPLICATE_SAME_ACCESS: u32 = 0x2;
const FILE_TYPE_DISK: u32 = 0x1;
const ERROR_ACCESS_DENIED: u32 = 5;

/// Handle-Werte sind Vielfache von 4. Statt der undokumentierten
/// Systemtabelle (`NtQuerySystemInformation`) probiert die Sonde alle Werte
/// bis zu dieser Grenze durch; ein Fehlgriff kostet einen Systemaufruf.
const GRENZE: usize = 0x10_0000;
/// Lebenszeichen des Arbeiters alle so viele Handle-Werte (Zweierpotenz).
const PULS: usize = 0x4000;
/// So lange darf ein einzelnes Handle brauchen, bevor es als Hänger zählt.
const FRIST: Duration = Duration::from_secs(3);

#[repr(C)]
struct ProcessEntry32W {
    size: u32,
    usage: u32,
    process_id: u32,
    default_heap_id: usize,
    module_id: u32,
    threads: u32,
    parent_process_id: u32,
    pri_class_base: i32,
    flags: u32,
    exe_file: [u16; 260],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> Bool;
    fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> Bool;
    fn CloseHandle(handle: Handle) -> Bool;
    fn OpenProcess(access: u32, inherit: Bool, process_id: u32) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn DuplicateHandle(
        source_process: Handle,
        source: Handle,
        target_process: Handle,
        target: *mut Handle,
        access: u32,
        inherit: Bool,
        options: u32,
    ) -> Bool;
    fn GetFileType(file: Handle) -> u32;
    fn GetFinalPathNameByHandleW(file: Handle, path: *mut u16, len: u32, flags: u32) -> u32;
    fn GetProcessHandleCount(process: Handle, count: *mut u32) -> Bool;
    fn GetLastError() -> u32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(
        callback: unsafe extern "system" fn(Handle, isize) -> Bool,
        param: isize,
    ) -> Bool;
    fn GetWindowThreadProcessId(window: Handle, process_id: *mut u32) -> u32;
    fn GetWindowTextW(window: Handle, text: *mut u16, max: i32) -> i32;
    fn IsWindowVisible(window: Handle) -> Bool;
}

/// PIDs aller laufenden `btp.exe`.
fn btp_prozesse() -> Vec<u32> {
    let mut pids = Vec::new();
    // SAFETY: Schnappschuss wird geprüft und am Ende geschlossen; `entry`
    // trägt die verlangte Strukturgröße.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return pids;
        }
        let mut entry: ProcessEntry32W = std::mem::zeroed();
        entry.size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut weiter = Process32FirstW(snap, &mut entry);
        while weiter != 0 {
            let ende = entry.exe_file.iter().position(|&c| c == 0).unwrap_or(260);
            let name = String::from_utf16_lossy(&entry.exe_file[..ende]);
            if name.eq_ignore_ascii_case("btp.exe") {
                pids.push(entry.process_id);
            }
            weiter = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    pids
}

struct Fenster {
    pid: u32,
    titel: Vec<String>,
}

unsafe extern "system" fn fenster_sammeln(fenster: Handle, param: isize) -> Bool {
    // SAFETY: `param` ist der Zeiger auf das `Fenster` aus `fenstertitel`,
    // das den synchronen `EnumWindows`-Aufruf überlebt.
    unsafe {
        let ziel = &mut *(param as *mut Fenster);
        let mut pid = 0u32;
        GetWindowThreadProcessId(fenster, &mut pid);
        if pid == ziel.pid && IsWindowVisible(fenster) != 0 {
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(fenster, buf.as_mut_ptr(), buf.len() as i32);
            if n > 0 {
                ziel.titel
                    .push(String::from_utf16_lossy(&buf[..n as usize]));
            }
        }
    }
    1
}

/// Titel aller sichtbaren Fenster des Prozesses.
fn fenstertitel(pid: u32) -> Vec<String> {
    let mut ziel = Fenster {
        pid,
        titel: Vec::new(),
    };
    // SAFETY: Der Rückruf läuft nur während dieses Aufrufs.
    unsafe {
        EnumWindows(fenster_sammeln, &mut ziel as *mut Fenster as isize);
    }
    ziel.titel
}

/// Dateiname aus dem Titel `Badminton Turnier Planer - <Name>`.
fn name_aus_titel(titel: &str) -> Option<&str> {
    titel
        .split_once(" - ")
        .map(|(_, n)| n.trim())
        .filter(|n| !n.is_empty())
}

/// Kommandozeile über PowerShell/CIM — für die Sonde genügt das; die
/// Automatik würde sie ohne Fremdprozess lesen.
fn kommandozeile(pid: u32) -> String {
    let skript = format!("(Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\").CommandLine");
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &skript])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(e) => format!("<PowerShell nicht startbar: {e}>"),
    }
}

/// Zahl der offenen Handles; `None`, wenn schon die schwächste Abfrage
/// verweigert wird.
fn handle_zahl(pid: u32) -> Option<u32> {
    // SAFETY: Prozess-Handle wird geprüft und geschlossen.
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if p == 0 {
            return None;
        }
        let mut n = 0u32;
        let ok = GetProcessHandleCount(p, &mut n);
        CloseHandle(p);
        (ok != 0).then_some(n)
    }
}

enum Meldung {
    /// Arbeiter lebt, ist bei diesem Handle-Wert.
    Puls(usize),
    /// Handle dupliziert, die Typabfrage steht bevor (Hänger-Kandidat).
    Geoeffnet(usize),
    /// Typabfrage beendet.
    Art(u32),
    /// Datei auf einem Datenträger samt Pfad.
    Datei(String),
    /// `OpenProcess` verweigert, Windows-Fehlercode.
    KeinZugriff(u32),
    Fertig,
}

/// Geht die Handle-Werte ab `ab` durch. `lauf` ist die Generation: Ein
/// Arbeiter, der nach einem Hänger doch noch aufwacht, sieht die höhere
/// Zahl in `aktuell` und hört auf, statt doppelt zu melden.
fn arbeiter(
    pid: u32,
    ab: usize,
    lauf: u32,
    aktuell: Arc<AtomicU32>,
    tx: mpsc::Sender<(u32, Meldung)>,
) {
    let melde = |m: Meldung| tx.send((lauf, m)).is_ok();
    // SAFETY: Jedes Handle wird geprüft; geschlossen werden nur das eigene
    // Prozess-Handle und die eigenen Kopien, nie ein Handle von BTP.
    unsafe {
        let prozess = OpenProcess(PROCESS_DUP_HANDLE, 0, pid);
        if prozess == 0 {
            melde(Meldung::KeinZugriff(GetLastError()));
            return;
        }
        let ich = GetCurrentProcess();
        let mut buf = vec![0u16; 32768];
        let mut h = ab;
        while h <= GRENZE && aktuell.load(Ordering::Relaxed) == lauf {
            if h & (PULS - 1) == 0 && !melde(Meldung::Puls(h)) {
                break;
            }
            let mut kopie: Handle = 0;
            let ok = DuplicateHandle(
                prozess,
                h as Handle,
                ich,
                &mut kopie,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            );
            if ok != 0 {
                melde(Meldung::Geoeffnet(h));
                let art = GetFileType(kopie);
                if art == FILE_TYPE_DISK {
                    let n = GetFinalPathNameByHandleW(kopie, buf.as_mut_ptr(), buf.len() as u32, 0)
                        as usize;
                    if n > 0 && n < buf.len() {
                        melde(Meldung::Datei(String::from_utf16_lossy(&buf[..n])));
                    }
                }
                CloseHandle(kopie);
                melde(Meldung::Art(art));
            }
            h += 4;
        }
        CloseHandle(prozess);
    }
    melde(Meldung::Fertig);
}

#[derive(Default)]
struct Befund {
    dateien: Vec<String>,
    /// Zahl der duplizierten Handles je `GetFileType`-Wert.
    arten: BTreeMap<u32, u32>,
    /// Handle-Werte, an denen die Typabfrage die Frist riss.
    haenger: Vec<usize>,
    kein_zugriff: Option<u32>,
}

/// Lässt Arbeiter laufen und startet nach einem Hänger hinter dem
/// blockierenden Handle einen neuen.
fn handles_lesen(pid: u32) -> Befund {
    let mut befund = Befund::default();
    let aktuell = Arc::new(AtomicU32::new(0));
    let (tx, rx) = mpsc::channel();
    let starte = |ab: usize, lauf: u32| {
        let (aktuell, tx) = (aktuell.clone(), tx.clone());
        std::thread::spawn(move || arbeiter(pid, ab, lauf, aktuell, tx));
    };
    let mut lauf = 0u32;
    let mut stand = 4usize;
    let mut offen: Option<usize> = None;
    starte(stand, lauf);
    loop {
        match rx.recv_timeout(FRIST) {
            Ok((l, _)) if l != lauf => {}
            Ok((_, Meldung::Puls(h))) => stand = h,
            Ok((_, Meldung::Geoeffnet(h))) => {
                stand = h;
                offen = Some(h);
            }
            Ok((_, Meldung::Art(art))) => {
                offen = None;
                *befund.arten.entry(art).or_default() += 1;
            }
            Ok((_, Meldung::Datei(p))) => befund.dateien.push(p),
            Ok((_, Meldung::KeinZugriff(code))) => {
                befund.kein_zugriff = Some(code);
                break;
            }
            Ok((_, Meldung::Fertig)) => break,
            Err(_) => {
                let bei = offen.take().unwrap_or(stand);
                befund.haenger.push(bei);
                stand = bei + 4;
                lauf += 1;
                aktuell.store(lauf, Ordering::Relaxed);
                starte(stand, lauf);
            }
        }
    }
    befund
}

/// `\\?\C:\x` → `C:\x`, `\\?\UNC\server\x` → `\\server\x`.
fn lesbarer_pfad(roh: &str) -> String {
    if let Some(rest) = roh.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        roh.strip_prefix(r"\\?\").unwrap_or(roh).to_string()
    }
}

fn endung(pfad: &str) -> String {
    std::path::Path::new(pfad)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Liest wie `tp_file_vorpruefung` die ersten 19 Bytes: Jet-Kopf?
fn jet_kopf(pfad: &str) -> Result<bool, std::io::Error> {
    let mut kopf = [0u8; 19];
    std::fs::File::open(pfad)?.read_exact(&mut kopf)?;
    Ok(&kopf[4..] == b"Standard Jet DB")
}

#[test]
#[ignore = "braucht Windows und ein laufendes BTP mit geöffnetem Turnier"]
fn tp_pfad_aus_dem_btp_prozess() {
    let pids = btp_prozesse();
    assert!(
        !pids.is_empty(),
        "kein btp.exe gefunden — BTP starten und ein Turnier öffnen"
    );
    println!("btp.exe läuft {}×: {pids:?}", pids.len());

    for pid in pids {
        println!("\n=== btp.exe, PID {pid} ===");

        let zeile = kommandozeile(pid);
        println!("Kommandozeile: {zeile}");
        println!(
            "  → enthält .tp: {}",
            zeile.to_ascii_lowercase().contains(".tp")
        );

        let titel = fenstertitel(pid);
        println!("Fenstertitel: {titel:?}");
        let namen: Vec<String> = titel
            .iter()
            .filter_map(|t| name_aus_titel(t))
            .map(str::to_lowercase)
            .collect();

        match handle_zahl(pid) {
            Some(n) => println!("Offene Handles laut Windows: {n}"),
            None => println!("Handle-Zahl nicht lesbar (Prozess gehört anderem Nutzer?)"),
        }

        let start = Instant::now();
        let befund = handles_lesen(pid);
        println!("Dauer des Durchgangs: {:?}", start.elapsed());

        if let Some(code) = befund.kein_zugriff {
            let grund = if code == ERROR_ACCESS_DENIED {
                " — läuft BTP als Administrator?"
            } else {
                ""
            };
            println!("KEIN ZUGRIFF auf die Handles (Fehler {code}){grund}");
            continue;
        }
        println!(
            "Duplizierte Handles je Art (0=unbekannt, 1=Datei, 2=Zeichen, 3=Pipe): {:?}",
            befund.arten
        );
        println!("Hänger (Frist {FRIST:?}) an Handle: {:x?}", befund.haenger);

        let mut je_endung: BTreeMap<String, u32> = BTreeMap::new();
        for d in &befund.dateien {
            *je_endung.entry(endung(d)).or_default() += 1;
        }
        println!("Offene Dateien je Endung: {je_endung:?}");

        let mut treffer = 0;
        for roh in &befund.dateien {
            let art = endung(roh);
            if art != "tp" && art != "ldb" {
                continue;
            }
            let pfad = lesbarer_pfad(roh);
            println!("  {art}: {pfad}");
            println!("      roh: {roh}");
            if art != "tp" {
                continue;
            }
            treffer += 1;
            let stamm = std::path::Path::new(&pfad)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            println!("      passt zum Fenstertitel: {}", namen.contains(&stamm));
            match jet_kopf(&pfad) {
                Ok(ja) => println!("      lesbar, Jet-Kopf: {ja}"),
                Err(e) => println!("      NICHT lesbar: {e}"),
            }
        }
        println!("ERGEBNIS: {treffer} offene .tp-Datei(en) gefunden");
    }
}
