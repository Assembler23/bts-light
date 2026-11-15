//! Handle-Durchlauf über `btp.exe` (ADR 0096) — der einzige Teil der Suche,
//! der Windows braucht. Vorbild und unabhängige Gegenmessung ist die Sonde
//! `tests/btp_tp_pfad_probe.rs` (rohes FFI, am echten BTP gemessen).
//!
//! Verfahren: Handle-Werte sind Vielfache von 4. Jeder Wert wird mit
//! `DuplicateHandle` in den eigenen Prozess geholt; gelingt das und ist es
//! eine Datei auf einem Datenträger, nennt `GetFinalPathNameByHandleW` den
//! Pfad. Nur dokumentierte Aufrufe.
//!
//! **Read-only gegenüber BTP:** Die Kopien tragen keinerlei Zugriffsrecht
//! (Zugriff 0), nie `DUPLICATE_CLOSE_SOURCE`; geschlossen werden nur das
//! eigene Prozess-Handle und die eigenen Kopien.
//!
//! **Frist:** `GetFileType` kann an einem Pipe-Handle blockieren. Die
//! Abfrage läuft deshalb in einem Arbeiter-Thread; meldet er sich
//! [`FRIST`] lang nicht, gilt das Handle als Hänger und ein neuer Arbeiter
//! macht dahinter weiter. Der zweite Hänger beendet die Suche
//! ([`super::HAENGER_JE_SUCHE`]).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_HANDLE_OPTIONS, ERROR_ACCESS_DENIED, HANDLE,
};
use windows::Win32::Storage::FileSystem::{
    GetFileType, GetFinalPathNameByHandleW, FILE_TYPE_DISK, GETFINALPATHNAMEBYHANDLE_FLAGS,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

use super::{HaengerDeckel, RohBefund, HAENGER_JE_SUCHE};

/// Obergrenze der durchprobierten Handle-Werte. BTP hält rund 400 Handles
/// (gemessen); ein Fehlgriff kostet einen Systemaufruf, der ganze Durchlauf
/// rund 70 ms.
const GRENZE: usize = 0x10_0000;
/// Lebenszeichen des Arbeiters alle so viele Handle-Werte (Zweierpotenz).
const PULS: usize = 0x4000;
/// Deckel für die gesammelten Pfade — BTP hält rund 20 Dateien offen; ein
/// Prozess mit Tausenden soll den Speicher nicht füllen.
const MAX_PFADE: usize = 4096;
/// So lange darf ein einzelnes Handle brauchen.
const FRIST: Duration = Duration::from_secs(3);

/// PIDs aller laufenden `btp.exe`.
fn btp_prozesse() -> Vec<u32> {
    let mut pids = Vec::new();
    // SAFETY: Der Schnappschuss wird geprüft und am Ende geschlossen;
    // `eintrag` trägt die verlangte Strukturgröße.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };
        let mut eintrag = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut weiter = Process32FirstW(snap, &mut eintrag).is_ok();
        while weiter {
            let ende = eintrag
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(eintrag.szExeFile.len());
            let name = String::from_utf16_lossy(&eintrag.szExeFile[..ende]);
            if name.eq_ignore_ascii_case("btp.exe") {
                pids.push(eintrag.th32ProcessID);
            }
            weiter = Process32NextW(snap, &mut eintrag).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    pids
}

enum Meldung {
    /// Arbeiter lebt, ist bei diesem Handle-Wert.
    Puls(usize),
    /// Handle dupliziert, die Typabfrage steht bevor (Hänger-Kandidat).
    Geoeffnet(usize),
    /// Abfrage dieses Handles beendet.
    Erledigt,
    /// Datei auf einem Datenträger samt rohem Pfad.
    Datei(String),
    /// `OpenProcess` gescheitert; `true` = Zugriff verweigert.
    KeinProzess(bool),
    Fertig,
}

/// Geht die Handle-Werte ab `ab` durch. `lauf` ist die Generation: Erhöht
/// der Aufrufer `aktuell`, hat er diesen Arbeiter als Hänger gemeldet — wacht
/// der doch noch auf, meldet er sich beim Deckel ab und hört auf.
fn arbeiter(
    pid: u32,
    ab: usize,
    lauf: u32,
    aktuell: Arc<AtomicU32>,
    deckel: &'static HaengerDeckel,
    tx: mpsc::Sender<(u32, Meldung)>,
) {
    let melde = |m: Meldung| tx.send((lauf, m)).is_ok();
    let gilt = || aktuell.load(Ordering::Acquire) == lauf;
    // SAFETY: Jedes Handle wird geprüft; geschlossen werden nur das eigene
    // Prozess-Handle und die eigenen Kopien, nie ein Handle von BTP.
    unsafe {
        let prozess = match OpenProcess(PROCESS_DUP_HANDLE, false, pid) {
            Ok(p) => p,
            Err(e) => {
                melde(Meldung::KeinProzess(
                    e.code() == ERROR_ACCESS_DENIED.to_hresult(),
                ));
                return;
            }
        };
        let ich = GetCurrentProcess();
        let mut puffer = vec![0u16; 32768];
        let mut h = ab;
        while h <= GRENZE && gilt() {
            if h & (PULS - 1) == 0 && !melde(Meldung::Puls(h)) {
                break;
            }
            let mut kopie = HANDLE::default();
            // Kopie **ohne Rechte** (Zugriff 0, keine Optionen): Für Typ und
            // Pfad genügt das (gemessen 15.11.2026, gleiche 19 Dateien wie
            // mit BTPs Rechten). So hält bts-light nie BTPs Schreibrecht auf
            // die Datenbank, auch nicht kurz. Und nie DUPLICATE_CLOSE_SOURCE
            // — das nähme BTP das Handle weg.
            let geholt = DuplicateHandle(
                prozess,
                HANDLE(h as *mut core::ffi::c_void),
                ich,
                &mut kopie,
                0,
                false,
                DUPLICATE_HANDLE_OPTIONS(0),
            );
            if geholt.is_ok() {
                melde(Meldung::Geoeffnet(h));
                if GetFileType(kopie) == FILE_TYPE_DISK {
                    let n = GetFinalPathNameByHandleW(
                        kopie,
                        &mut puffer,
                        GETFINALPATHNAMEBYHANDLE_FLAGS(0),
                    ) as usize;
                    if n > 0 && n < puffer.len() {
                        melde(Meldung::Datei(String::from_utf16_lossy(&puffer[..n])));
                    }
                }
                let _ = CloseHandle(kopie);
                melde(Meldung::Erledigt);
            }
            h += 4;
        }
        let _ = CloseHandle(prozess);
    }
    if gilt() {
        melde(Meldung::Fertig);
    } else {
        // Als Hänger gemeldet und doch zurückgekehrt.
        deckel.aufgewacht();
    }
}

/// Liest die offenen Dateien eines Prozesses in `befund`. Liefert `false`,
/// wenn die Suche wegen Hängern abgebrochen wurde.
fn handles_lesen(pid: u32, deckel: &'static HaengerDeckel, befund: &mut RohBefund) -> bool {
    let aktuell = Arc::new(AtomicU32::new(0));
    let (tx, rx) = mpsc::channel();
    let starte = |ab: usize, lauf: u32| {
        let (aktuell, tx) = (aktuell.clone(), tx.clone());
        std::thread::spawn(move || arbeiter(pid, ab, lauf, aktuell, deckel, tx));
    };
    let mut lauf = 0u32;
    let mut stand = 4usize;
    let mut offen: Option<usize> = None;
    let mut haenger = 0usize;
    starte(stand, lauf);
    loop {
        match rx.recv_timeout(FRIST) {
            Ok((l, _)) if l != lauf => {}
            Ok((_, Meldung::Puls(h))) => stand = h,
            Ok((_, Meldung::Geoeffnet(h))) => {
                stand = h;
                offen = Some(h);
            }
            Ok((_, Meldung::Erledigt)) => offen = None,
            Ok((_, Meldung::Datei(p))) => {
                if befund.pfade.len() < MAX_PFADE {
                    befund.pfade.push(p);
                }
            }
            Ok((_, Meldung::KeinProzess(verweigert))) => {
                befund.kein_zugriff |= verweigert;
                return true;
            }
            Ok((_, Meldung::Fertig)) => return true,
            Err(_) => {
                // Der Arbeiter dieser Generation hängt: beim Deckel melden
                // und die Generation erhöhen — genau das Paar, auf das sich
                // `arbeiter` beim Abmelden verlässt.
                deckel.haenger_gemeldet();
                lauf += 1;
                aktuell.store(lauf, Ordering::Release);
                haenger += 1;
                if haenger >= HAENGER_JE_SUCHE || !deckel.suche_erlaubt() {
                    befund.haenger = true;
                    return false;
                }
                stand = offen.take().unwrap_or(stand) + 4;
                starte(stand, lauf);
            }
        }
    }
}

/// Durchsucht alle laufenden `btp.exe`. Blockiert; im schlimmsten Fall
/// [`HAENGER_JE_SUCHE`] × [`FRIST`].
pub(super) fn befund(deckel: &'static HaengerDeckel) -> RohBefund {
    let pids = btp_prozesse();
    let mut befund = RohBefund {
        prozesse: pids.len(),
        ..RohBefund::default()
    };
    for pid in pids {
        if !handles_lesen(pid, deckel, &mut befund) {
            break;
        }
    }
    befund
}
