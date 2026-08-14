//! Ausnahmeliste für die automatische Feldvergabe (Spec
//! `docs/features/feldvergabe-ausnahme.md`).
//!
//! Enthält ausschließlich Match-IDs, die die Turnierleitung von
//! `sync.rs::auto_assign` ausgenommen hat — kein Personendatum. Manuelles
//! Zuweisen bleibt davon unberührt (R2: BTP bleibt für Court-Zuordnungen die
//! Wahrheit, das ist ein rein lokaler Bedienzustand).
//!
//! Grundsätze (Muster ADR 0022, wie `officials.rs`, aber ohne dessen
//! Rotation/Feldschalter):
//! - **Turniergebunden**: eigene Datei im App-Datenverzeichnis, im Kopf das
//!   Turnier. Wechselt das Turnier, wird der Stand verworfen — eine
//!   Match-ID gilt nur innerhalb eines Turniers.
//! - **Außerhalb der `AppConfig`**: kein zusätzlicher globaler Schalter,
//!   die Datei bleibt vom Config-Export unberührt.
//! - **Best effort**: Ein Schreibfehler kostet höchstens die Einteilung,
//!   nie ein Ergebnis.
//! - **Aufräumen bei Spielende**: `sync.rs::reconcile_auto_assign_exclusions`
//!   entfernt einen Eintrag, sobald das Match `Finished` ist oder aus dem
//!   Snapshot verschwindet — dieses Modul selbst kennt kein Spielende.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022: lieber verwerfen
/// als falsch zuordnen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ExclusionFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Match-IDs, die von der Auto-Vergabe ausgenommen sind.
    #[serde(default)]
    excluded: HashSet<i64>,
}

/// Ergebnis eines Ladeversuchs. Der Unterschied zwischen „gibt es nicht" und
/// „komme gerade nicht dran" entscheidet, ob überschrieben werden darf.
enum Ladung {
    /// Ein gelesener Stand (kann zu einem anderen Turnier gehören).
    Stand(ExclusionFile),
    /// Keine Datei (oder Persistenz aus, oder kaputter Inhalt) ⇒ leer starten.
    Leer,
    /// Datei vorhanden, aber nicht lesbar ⇒ nicht anfassen, später erneut.
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: ExclusionFile,
    /// Wurde die Datei schon einmal gelesen? Der Pfad steht beim Start fest,
    /// das Turnier kommt erst mit dem ersten Snapshot — geladen wird deshalb
    /// beim ersten `set_tournament` (Muster `OfficialsStore`).
    loaded: bool,
}

/// Der Ausnahme-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit TL-Web-Actions und
/// Tauri-Commands denselben Stand sehen.
#[derive(Default)]
pub struct AutoAssignExclusionStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests, Slave-Betrieb).
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `OfficialsStore`).
    persist_lock: Mutex<()>,
}

impl AutoAssignExclusionStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        // Ein neuer Pfad ist ungelesen — sonst hinge die Richtigkeit daran,
        // dass `set_path` immer VOR dem ersten Snapshot läuft; in der
        // umgekehrten Reihenfolge überschriebe der leere Stand die Datei.
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot). Beim ersten
    /// Mal wird der passende Stand geladen; bei Turnierwechsel verworfen.
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return; // Startphase ohne Namen ändert nichts
        }
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.loaded && inner.file.tournament == name {
                return; // unverändert — der Normalfall jedes Sync-Ticks
            }
            if !inner.loaded {
                match self.load_file() {
                    // Erststart: Nur ein Stand DESSELBEN Turniers wird
                    // übernommen — sonst gehörten fremde Match-IDs zum
                    // neuen Turnier (ADR 0022).
                    Ladung::Stand(file) => {
                        inner.loaded = true;
                        if file.tournament == name {
                            inner.file = file;
                            return;
                        }
                    }
                    Ladung::Leer => inner.loaded = true,
                    // Datei da, aber gerade nicht lesbar (Virenscanner,
                    // hängendes Handle): NICHTS anfassen und NICHT schreiben
                    // — der nächste Snapshot versucht es erneut.
                    Ladung::Unlesbar => return,
                }
            }
            // Turnierwechsel (oder fremder Dateistand): verwerfen.
            inner.file = ExclusionFile {
                tournament: name.to_string(),
                ..Default::default()
            };
        }
        // Der leere Stand geht sofort auf die Platte: Ein Absturz direkt
        // nach dem Wechsel darf die Ausnahmen des alten Turniers nicht
        // wieder auferstehen lassen.
        self.persist();
    }

    /// Aktuell gebundenes Turnier (leer, solange keins gebunden ist).
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    /// Ist dieses Match gerade von der Auto-Vergabe ausgenommen?
    pub fn is_excluded(&self, match_id: i64) -> bool {
        self.inner.lock().unwrap().file.excluded.contains(&match_id)
    }

    /// Ausnahme setzen oder zurücknehmen.
    pub fn set_excluded(&self, match_id: i64, excluded: bool) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            if excluded {
                inner.file.excluded.insert(match_id)
            } else {
                inner.file.excluded.remove(&match_id)
            }
        };
        if changed {
            self.persist();
        }
    }

    /// Alle Match-IDs entfernen, die nicht mehr in `keep` stehen (Aufräumen
    /// bei Spielende, aus `sync.rs::reconcile_auto_assign_exclusions`).
    /// Liefert `true`, wenn sich etwas geändert hat.
    pub fn retain(&self, keep: &HashSet<i64>) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let before = inner.file.excluded.len();
            inner.file.excluded.retain(|id| keep.contains(id));
            before != inner.file.excluded.len()
        };
        if changed {
            self.persist();
        }
        changed
    }

    // ── Persistenz ────────────────────────────────────────────────────

    fn load_file(&self) -> Ladung {
        let Some(path) = self.path.read().unwrap().clone() else {
            return Ladung::Leer; // Persistenz aus — sauberer Start im RAM
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ladung::Leer,
            Err(e) => {
                // Alles andere (Sperre, Rechte, defekter Datenträger) heißt:
                // Es gibt einen Stand, wir kommen nur gerade nicht dran.
                tracing::warn!(
                    "excluded-matches.json nicht lesbar ({e}) – Stand bleibt unberührt"
                );
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<ExclusionFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                // Kaputter Inhalt ist nicht zu retten — hier ist Überschreiben
                // richtig, anders als beim Lesefehler.
                tracing::warn!("excluded-matches.json unlesbar – beginne leer");
                Ladung::Leer
            }
        }
    }

    /// Schreiben (best effort, atomar). Fehler kosten höchstens die
    /// Einteilung, nie ein Ergebnis.
    fn persist(&self) {
        let Some(path) = self.path.read().unwrap().clone() else {
            return; // Persistenz aus (Tests, Slave-Betrieb)
        };
        // Schreib-Guard VOR dem Schnappschuss (Muster `OfficialsStore`):
        // sonst könnten zwei Schreiber ihre Klone in vertauschter
        // Reihenfolge auf die Platte bringen.
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let inner = self.inner.lock().unwrap();
            if inner.file.tournament.is_empty() {
                return; // ohne Turnier-Kopf wäre der Stand nicht zuzuordnen
            }
            inner.file.clone()
        };
        if let Ok(json) = serde_json::to_string(&data) {
            // Erst Temp-Datei, dann Umbenennen — ein Absturz mitten im
            // Schreiben hinterlässt nie eine halbe Datei.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn store_mit_datei(dir: &Path) -> AutoAssignExclusionStore {
        let store = AutoAssignExclusionStore::default();
        store.set_path(dir.join("excluded-matches.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    #[test]
    fn ausnahme_setzen_und_zuruecknehmen() {
        let store = AutoAssignExclusionStore::default(); // ohne Persistenz
        assert!(!store.is_excluded(10));
        store.set_excluded(10, true);
        assert!(store.is_excluded(10));
        store.set_excluded(10, false);
        assert!(!store.is_excluded(10));
    }

    #[test]
    fn stand_ueberlebt_app_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_excluded(10, true);
        store.set_excluded(11, true);

        // „Neustart": frischer Speicher, gleiche Datei, gleiches Turnier.
        let neu = store_mit_datei(dir.path());
        assert!(neu.is_excluded(10));
        assert!(neu.is_excluded(11));
        assert!(!neu.is_excluded(12));
    }

    #[test]
    fn turnierwechsel_verwirft_den_stand() {
        // ADR 0022: Match-IDs gelten nur im Turnier — beim Wechsel wird
        // verworfen, nicht mitgeschleppt.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_excluded(10, true);

        store.set_tournament("Ganz anderes Turnier");
        assert!(!store.is_excluded(10));
        assert_eq!(store.tournament(), "Ganz anderes Turnier");

        // Und der verworfene Stand ist auch auf der Platte weg — ein
        // Neustart darf ihn nicht zurückholen.
        let neu = AutoAssignExclusionStore::default();
        neu.set_path(dir.path().join("excluded-matches.json"));
        neu.set_tournament("Test BTS Light");
        assert!(!neu.is_excluded(10));
    }

    #[test]
    fn fremder_dateistand_wird_beim_start_verworfen() {
        let dir = tempfile::tempdir().unwrap();
        let alt = store_mit_datei(dir.path());
        alt.set_excluded(10, true);

        let neu = AutoAssignExclusionStore::default();
        neu.set_path(dir.path().join("excluded-matches.json"));
        neu.set_tournament("Anderes Turnier");
        assert!(!neu.is_excluded(10));
    }

    #[test]
    fn eine_voruebergehend_unlesbare_datei_wird_nicht_ueberschrieben() {
        // Ersatz für die Sperre im Test: ein Verzeichnis unter dem
        // Dateinamen — lesbar ist das ebenso wenig, und es ist auch kein
        // `NotFound`.
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("excluded-matches.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = AutoAssignExclusionStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Cup A");
        // Kein Turnier gebunden ⇒ es wird nichts geschrieben …
        assert_eq!(store.tournament(), "");
        assert!(pfad.is_dir(), "der vorhandene Stand bleibt unangetastet");
        assert!(
            !pfad.with_extension("json.tmp").exists(),
            "auch keine halbe Schreiboperation"
        );
        // … der Betrieb läuft trotzdem weiter (im RAM).
        store.set_excluded(1, true);
        assert!(store.is_excluded(1));

        // Sobald die Datei wieder lesbar ist, holt der nächste Snapshot den
        // Stand nach — ohne App-Neustart.
        std::fs::remove_dir(&pfad).unwrap();
        let vorlage = store_mit_datei(dir.path());
        vorlage.set_excluded(4, true);
        store.set_tournament("Test BTS Light");
        assert!(store.is_excluded(4));
        assert_eq!(store.tournament(), "Test BTS Light");
    }

    #[test]
    fn retain_entfernt_nur_was_fehlt_und_meldet_die_aenderung() {
        let store = AutoAssignExclusionStore::default();
        store.set_excluded(10, true);
        store.set_excluded(11, true);
        store.set_excluded(12, true);

        let keep: HashSet<i64> = [11].into_iter().collect();
        assert!(store.retain(&keep));
        assert!(!store.is_excluded(10));
        assert!(store.is_excluded(11));
        assert!(!store.is_excluded(12));

        // Nochmal derselbe Aufruf ändert nichts mehr.
        assert!(!store.retain(&keep));
    }
}
