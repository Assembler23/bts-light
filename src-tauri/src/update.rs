//! Update im Turnierbetrieb (Spec `docs/features/update-im-turnierbetrieb.md`,
//! ADR 0057): der Tauri-freie Kern des Update-Ablaufs.
//!
//! Drei Bausteine, alle ohne Tauri testbar:
//!
//! - **Wiederanlauf-Marker** (`update-resume.json`): Vor dem Neustart durch
//!   den Installer merkt sich die App, ob die Übertragung lief. Die neue
//!   Version findet den Marker, löscht ihn und startet die Übertragung von
//!   selbst — die Lücke bleibt eine Sache von Sekunden, und niemand muss
//!   daran denken, „Starten" zu drücken. Der Marker gilt nur kurz
//!   ([`RESUME_MAX_AGE_MS`]): Ein liegen gebliebener Marker (Installer
//!   abgebrochen, PC ausgeschaltet) darf Tage später nichts von selbst
//!   starten.
//! - **Stiller Installer** beim Beenden: Der Updater von Tauri startet den
//!   NSIS-Installer immer mit `/R` (App danach neu starten). Wer das Update
//!   erst beim Feierabend einbauen will, will danach aber KEINE laufende App.
//!   Deshalb ruft die App den Installer in diesem Fall selbst auf — stumm
//!   (`/S`) und als Update (`/UPDATE`), ohne `/R`.
//! - **Phasen-Verwaltung** ([`UpdateManager`]): eine Zustandsmaschine für das
//!   Banner, die das Paket nach dem Prüfen sofort lädt und bereithält —
//!   der Einbau selbst wird dann zur Frage des richtigen Moments.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Dateiname des Wiederanlauf-Markers im App-Datenverzeichnis.
pub const RESUME_FILE: &str = "update-resume.json";

/// So lange gilt ein Marker (15 min). Ein Update dauert Sekunden; alles
/// darüber hinaus ist ein liegen gebliebener Rest, kein Wiederanlauf.
pub const RESUME_MAX_AGE_MS: u64 = 15 * 60 * 1000;

/// Wiederanlauf-Marker: geschrieben unmittelbar vor dem Neustart durch den
/// Installer, gelesen und gelöscht vom nächsten App-Start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeMarker {
    /// Lief die Übertragung, als das Update ausgelöst wurde?
    pub running: bool,
    /// Schreibzeitpunkt (Unix-ms) — Grundlage der Frische-Prüfung.
    pub written_ms: u64,
    /// Version, aus der heraus aktualisiert wurde (nur fürs Log).
    pub from_version: String,
}

/// Gilt der Marker noch? Nur ein frischer Marker einer laufenden
/// Übertragung löst den Wiederanlauf aus. Eine Uhr, die nach dem Neustart
/// hinter dem Schreibzeitpunkt liegt (Sommerzeit, NTP-Sprung), zählt als
/// frisch — lieber einmal zu viel starten als nach einem echten Update
/// stehen bleiben.
pub fn resume_gilt(marker: &ResumeMarker, now_ms: u64) -> bool {
    marker.running && now_ms.saturating_sub(marker.written_ms) <= RESUME_MAX_AGE_MS
}

/// Schreibt den Marker atomar (Temp-Datei + Umbenennen), damit nie ein
/// halb geschriebener Marker liegt.
pub fn write_resume(path: &Path, marker: &ResumeMarker) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(marker).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Löscht den Marker, falls vorhanden (nach einem gescheiterten Einbau).
pub fn remove_resume(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Liest den Marker, **löscht ihn in jedem Fall** und sagt, ob die
/// Übertragung von selbst wieder anlaufen soll. Ein unlesbarer oder
/// abgelaufener Marker verschwindet damit ebenfalls — er soll keinen
/// späteren Start mehr beeinflussen.
pub fn take_resume(path: &Path, now_ms: u64) -> bool {
    let inhalt = std::fs::read(path);
    remove_resume(path);
    let Ok(bytes) = inhalt else {
        return false;
    };
    match serde_json::from_slice::<ResumeMarker>(&bytes) {
        Ok(marker) => resume_gilt(&marker, now_ms),
        Err(_) => false,
    }
}

/// Argumente des stillen Einbaus beim Beenden: stumm, als Update erkannt,
/// **ohne** `/R` — die App soll danach nicht wieder aufgehen.
pub const SILENT_INSTALLER_ARGS: [&str; 2] = ["/S", "/UPDATE"];

/// Dateiname des zwischengespeicherten Installers. Die Version wird auf
/// harmlose Zeichen reduziert — sie stammt aus dem Manifest, also von außen.
pub fn installer_dateiname(version: &str) -> String {
    let sauber: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .take(40)
        .collect();
    format!("bts-light-update-{sauber}-setup.exe")
}

/// Legt den Installer im Verzeichnis ab und liefert den Pfad. Getrennt vom
/// Start, damit das Ablegen testbar bleibt.
///
/// Die Bytes sind **signaturgeprüft**: `Update::download()` des Updater-
/// Plugins verifiziert die minisign-Signatur aus `latest.json` gegen den
/// eingebauten Public Key, bevor es die Bytes herausgibt — der stille Pfad
/// baut also exakt dasselbe ein wie der Plugin-Pfad. Zusätzlich muss es
/// eine Windows-Programmdatei sein (`MZ`): Zeigte das Manifest je auf ein
/// Zip, „startete" der stille Pfad sonst ein Archiv und das Update ginge
/// nur mit einer Log-Zeile verloren.
pub fn installer_ablegen(dir: &Path, version: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    if !bytes.starts_with(b"MZ") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Update-Paket ist keine Windows-Programmdatei (kein NSIS-Installer)",
        ));
    }
    std::fs::create_dir_all(dir)?;
    let pfad = dir.join(installer_dateiname(version));
    std::fs::write(&pfad, bytes)?;
    Ok(pfad)
}

/// Startet den abgelegten Installer stumm. Der Installer läuft per-user
/// (siehe `installer/firewall-hooks.nsh`), braucht also keine Erhöhung;
/// er beendet eine noch laufende App selbst. Der Aufrufer beendet sich
/// unmittelbar danach.
pub fn installer_stumm_starten(pfad: &Path) -> std::io::Result<()> {
    std::process::Command::new(pfad)
        .args(SILENT_INSTALLER_ARGS)
        .spawn()
        .map(|_| ())
}

/// Phase des Update-Ablaufs, so wie das Banner sie sieht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Noch nicht geprüft.
    Idle,
    /// Manifest wird abgefragt.
    Checking,
    /// Neuere Version gefunden, Paket wird geladen.
    Downloading {
        version: String,
        notes: Option<String>,
    },
    /// Paket liegt bereit; Einbau auf Knopfdruck oder beim Beenden.
    Ready {
        version: String,
        notes: Option<String>,
    },
    /// Einbau läuft, die App endet gleich.
    Installing { version: String },
    /// Es gibt nichts Neueres.
    Current,
    /// Prüfen oder Laden gescheitert (offline ist der Normalfall).
    Error(String),
}

/// Das geladene Paket: Version und die Installer-Bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paket {
    pub version: String,
    pub bytes: Vec<u8>,
}

/// Anzeige-Stand für das Frontend (`update_info`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    /// `idle` | `checking` | `downloading` | `ready` | `installing` |
    /// `current` | `error`.
    pub phase: String,
    /// Angebotene Version (ab `downloading`).
    pub version: Option<String>,
    /// „Was ist neu" aus dem Manifest.
    pub notes: Option<String>,
    /// Fehlertext (nur `error`).
    pub message: Option<String>,
    /// Anzahl der Felder, auf denen gerade ein Spiel steht — die
    /// Entscheidungsgrundlage für „jetzt" oder „später".
    pub occupied_courts: usize,
    /// Läuft die Übertragung?
    pub sync_running: bool,
    /// Ist der Einbau beim Beenden vorgemerkt?
    pub install_on_exit: bool,
}

/// Zustandsmaschine des Update-Ablaufs. Lebt im `AppState`; die
/// Tauri-Commands schalten die Phasen, das Frontend liest sie im Takt.
#[derive(Debug, Default)]
pub struct UpdateManager {
    phase: Mutex<Option<Phase>>,
    paket: Mutex<Option<Paket>>,
    install_on_exit: AtomicBool,
}

impl UpdateManager {
    pub fn phase(&self) -> Phase {
        self.phase.lock().unwrap().clone().unwrap_or(Phase::Idle)
    }

    pub fn set_phase(&self, phase: Phase) {
        *self.phase.lock().unwrap() = Some(phase);
    }

    /// Ein Prüflauf beginnt — Prüfung und Umschalten unter EINEM Lock, damit
    /// Auto-Check beim Start und Klick auf der Wartungsseite nicht beide
    /// durchkommen und zweimal laden. `None` = läuft schon (Prüfen, Laden,
    /// Einbauen). `Some(bereit)` = angenommen; `bereit` nennt ein bereits
    /// geladenes Paket (Version, Notizen), das der Lauf nicht verlieren darf.
    pub fn begin_check(&self) -> Option<Option<(String, Option<String>)>> {
        let mut phase = self.phase.lock().unwrap();
        let bereit = match phase.as_ref().unwrap_or(&Phase::Idle) {
            Phase::Checking | Phase::Downloading { .. } | Phase::Installing { .. } => return None,
            Phase::Ready { version, notes } => Some((version.clone(), notes.clone())),
            _ => None,
        };
        *phase = Some(Phase::Checking);
        Some(bereit)
    }

    /// Paket und Vormerkung verwerfen (Manifest bietet die Version nicht
    /// mehr an). Die Phase setzt der Aufrufer.
    pub fn paket_verwerfen(&self) {
        *self.paket.lock().unwrap() = None;
        self.install_on_exit.store(false, Ordering::SeqCst);
    }

    /// Paket ablegen und auf `Ready` schalten.
    pub fn paket_bereit(&self, version: String, notes: Option<String>, bytes: Vec<u8>) {
        *self.paket.lock().unwrap() = Some(Paket {
            version: version.clone(),
            bytes,
        });
        self.set_phase(Phase::Ready { version, notes });
    }

    /// Das bereitliegende Paket (Kopie) — nur in `Ready`.
    pub fn paket(&self) -> Option<Paket> {
        match self.phase() {
            Phase::Ready { .. } => self.paket.lock().unwrap().clone(),
            _ => None,
        }
    }

    /// Einbau beim Beenden vormerken. Nur mit bereitliegendem Paket — ein
    /// Häkchen ohne Paket wäre ein leeres Versprechen.
    pub fn set_install_on_exit(&self, an: bool) -> bool {
        let erlaubt = !an || matches!(self.phase(), Phase::Ready { .. });
        if erlaubt {
            self.install_on_exit.store(an, Ordering::SeqCst);
        }
        erlaubt
    }

    pub fn install_on_exit(&self) -> bool {
        self.install_on_exit.load(Ordering::SeqCst)
    }

    /// Das Paket für den Einbau beim Beenden — nur wenn vorgemerkt UND
    /// bereit. Beides zusammen in einer Abfrage, damit der Schließpfad
    /// keine halbe Wahrheit sieht.
    pub fn paket_fuer_beenden(&self) -> Option<Paket> {
        if !self.install_on_exit() {
            return None;
        }
        self.paket()
    }

    pub fn info(&self, occupied_courts: usize, sync_running: bool) -> UpdateInfo {
        let (phase, version, notes, message) = match self.phase() {
            Phase::Idle => ("idle", None, None, None),
            Phase::Checking => ("checking", None, None, None),
            Phase::Downloading { version, notes } => ("downloading", Some(version), notes, None),
            Phase::Ready { version, notes } => ("ready", Some(version), notes, None),
            Phase::Installing { version } => ("installing", Some(version), None, None),
            Phase::Current => ("current", None, None, None),
            Phase::Error(m) => ("error", None, None, Some(m)),
        };
        UpdateInfo {
            phase: phase.to_string(),
            version,
            notes,
            message,
            occupied_courts,
            sync_running,
            install_on_exit: self.install_on_exit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(running: bool, written_ms: u64) -> ResumeMarker {
        ResumeMarker {
            running,
            written_ms,
            from_version: "0.9.278".into(),
        }
    }

    #[test]
    fn marker_gilt_nur_frisch_und_nur_bei_laufender_uebertragung() {
        assert!(resume_gilt(&marker(true, 1_000), 1_000 + RESUME_MAX_AGE_MS));
        assert!(!resume_gilt(
            &marker(true, 1_000),
            1_001 + RESUME_MAX_AGE_MS
        ));
        assert!(!resume_gilt(&marker(false, 1_000), 2_000));
        // Uhr nach dem Neustart hinter dem Schreibzeitpunkt: gilt trotzdem.
        assert!(resume_gilt(&marker(true, 5_000), 4_000));
    }

    #[test]
    fn take_resume_liest_einmal_und_loescht_immer() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join(RESUME_FILE);
        write_resume(&pfad, &marker(true, 10_000)).unwrap();
        assert!(pfad.exists());
        assert!(!dir.path().join("update-resume.json.tmp").exists());

        assert!(take_resume(&pfad, 20_000));
        assert!(!pfad.exists(), "Marker muss nach dem Lesen weg sein");
        // Zweiter Start ohne Marker: kein Wiederanlauf.
        assert!(!take_resume(&pfad, 21_000));
    }

    #[test]
    fn abgelaufener_oder_kaputter_marker_startet_nichts_und_verschwindet() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join(RESUME_FILE);
        write_resume(&pfad, &marker(true, 0)).unwrap();
        assert!(!take_resume(&pfad, RESUME_MAX_AGE_MS + 1));
        assert!(!pfad.exists());

        std::fs::write(&pfad, b"{ kaputt").unwrap();
        assert!(!take_resume(&pfad, 0));
        assert!(!pfad.exists());
    }

    #[test]
    fn stiller_installer_ohne_neustart_flag() {
        assert_eq!(SILENT_INSTALLER_ARGS, ["/S", "/UPDATE"]);
        assert!(!SILENT_INSTALLER_ARGS.contains(&"/R"));
    }

    #[test]
    fn installer_dateiname_filtert_fremde_zeichen() {
        assert_eq!(
            installer_dateiname("0.9.279"),
            "bts-light-update-0.9.279-setup.exe"
        );
        assert_eq!(
            installer_dateiname("../1.0\\x"),
            "bts-light-update-..1.0x-setup.exe"
        );
    }

    #[test]
    fn installer_ablegen_schreibt_bytes_unter_versionsnamen() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = installer_ablegen(dir.path(), "1.2.3", b"MZ-test").unwrap();
        assert!(pfad.starts_with(dir.path()));
        assert_eq!(
            pfad.file_name().unwrap(),
            "bts-light-update-1.2.3-setup.exe"
        );
        assert_eq!(std::fs::read(&pfad).unwrap(), b"MZ-test");
    }

    #[test]
    fn manager_phasen_und_paket() {
        let m = UpdateManager::default();
        assert_eq!(m.phase(), Phase::Idle);
        assert_eq!(m.paket(), None);

        m.set_phase(Phase::Downloading {
            version: "1.0.0".into(),
            notes: None,
        });
        // Beim Beenden vormerken geht erst mit Paket.
        assert!(!m.set_install_on_exit(true));
        assert!(!m.install_on_exit());

        m.paket_bereit("1.0.0".into(), Some("Neu".into()), vec![1, 2, 3]);
        assert_eq!(
            m.phase(),
            Phase::Ready {
                version: "1.0.0".into(),
                notes: Some("Neu".into())
            }
        );
        assert_eq!(m.paket().map(|p| p.bytes), Some(vec![1, 2, 3]));
        assert_eq!(m.paket_fuer_beenden(), None, "nicht vorgemerkt");

        assert!(m.set_install_on_exit(true));
        assert_eq!(
            m.paket_fuer_beenden().map(|p| p.version),
            Some("1.0.0".into())
        );
        assert!(m.set_install_on_exit(false));
        assert_eq!(m.paket_fuer_beenden(), None);

        // Nach dem Umschalten auf „Installing" gibt es kein Paket mehr nach
        // außen — der Schließpfad darf nicht ein zweites Mal installieren.
        m.set_install_on_exit(true);
        m.set_phase(Phase::Installing {
            version: "1.0.0".into(),
        });
        assert_eq!(m.paket(), None);
        assert_eq!(m.paket_fuer_beenden(), None);
        assert_eq!(m.info(0, false).version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn begin_check_laesst_nur_einen_lauf_zu_und_nennt_das_bereite_paket() {
        let m = UpdateManager::default();
        assert_eq!(m.begin_check(), Some(None));
        assert_eq!(m.phase(), Phase::Checking);
        // Zweiter Aufruf während des Laufs: abgelehnt.
        assert_eq!(m.begin_check(), None);

        m.paket_bereit("1.0.0".into(), Some("Neu".into()), vec![b'M', b'Z']);
        assert_eq!(
            m.begin_check(),
            Some(Some(("1.0.0".into(), Some("Neu".into()))))
        );
        m.set_phase(Phase::Installing {
            version: "1.0.0".into(),
        });
        assert_eq!(m.begin_check(), None);
    }

    #[test]
    fn paket_verwerfen_raeumt_paket_und_vormerkung() {
        let m = UpdateManager::default();
        m.paket_bereit("1.0.0".into(), None, vec![b'M', b'Z']);
        assert!(m.set_install_on_exit(true));
        m.paket_verwerfen();
        m.set_phase(Phase::Current);
        assert_eq!(m.paket(), None);
        assert!(!m.install_on_exit());
        assert_eq!(m.paket_fuer_beenden(), None);
    }

    #[test]
    fn installer_ablegen_verweigert_alles_ausser_programmdateien() {
        let dir = tempfile::tempdir().unwrap();
        let err = installer_ablegen(dir.path(), "1.2.3", b"PK\x03\x04zip").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            std::fs::read_dir(dir.path()).unwrap().next().is_none(),
            "nichts abgelegt"
        );
    }

    #[test]
    fn info_spiegelt_phase_und_umfeld() {
        let m = UpdateManager::default();
        m.set_phase(Phase::Error("offline".into()));
        let i = m.info(3, true);
        assert_eq!(i.phase, "error");
        assert_eq!(i.message.as_deref(), Some("offline"));
        assert_eq!(i.occupied_courts, 3);
        assert!(i.sync_running);
        assert!(!i.install_on_exit);

        m.paket_bereit("2.0.0".into(), None, vec![]);
        m.set_install_on_exit(true);
        let i = m.info(0, false);
        assert_eq!(i.phase, "ready");
        assert_eq!(i.version.as_deref(), Some("2.0.0"));
        assert!(i.install_on_exit);
    }
}
