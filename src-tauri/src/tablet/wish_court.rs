//! Wunschfelder für die automatische Feldvergabe (Spec
//! `docs/features/tl-wunschfeld.md`).
//!
//! Enthält ausschließlich Match-ID → CourtID — kein Personendatum. Der Wunsch
//! ist bts-light-eigen; BTP kennt ihn nicht und bekommt ihn nie zu sehen (R2).
//!
//! **Was er bewirkt:** `sync.rs::auto_assign` legt ein Spiel mit Wunschfeld
//! **nur** auf dieses eine Feld. Ist es belegt, wartet das Spiel — auch wenn
//! ein anderes Feld früher frei wird. Genau dafür ist es gedacht: Das Endspiel
//! gehört auf das Hauptfeld, nicht dorthin, wo zufällig zuerst Platz ist.
//! Andere Spiele werden unterdessen normal weiterverteilt.
//!
//! Manuelles Zuweisen bleibt unberührt — der Wunsch steuert nur die Automatik.
//!
//! Grundsätze (Muster ADR 0022, wie `exclusion.rs`):
//! - **Turniergebunden**: eigene Datei im App-Datenverzeichnis, im Kopf das
//!   Turnier. Wechselt es, wird der Stand verworfen — Match- und CourtIDs
//!   gelten nur innerhalb eines Turniers.
//! - **Außerhalb der `AppConfig`**: kein globaler Schalter, die Datei bleibt
//!   vom Config-Export unberührt.
//! - **Best effort**: Ein Schreibfehler kostet höchstens die Einteilung, nie
//!   ein Ergebnis.
//! - **Aufräumen bei Spielende**: `sync.rs::reconcile_wish_courts` entfernt
//!   einen Eintrag, sobald das Match `Finished` ist oder aus dem Snapshot
//!   verschwindet — dieses Modul selbst kennt kein Spielende.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022: lieber verwerfen
/// als falsch zuordnen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WishFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Match-ID → gewünschte CourtID.
    #[serde(default)]
    wishes: HashMap<i64, i64>,
}

/// Ergebnis eines Ladeversuchs. Der Unterschied zwischen „gibt es nicht" und
/// „komme gerade nicht dran" entscheidet, ob überschrieben werden darf.
enum Ladung {
    Stand(WishFile),
    Leer,
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: WishFile,
    /// Wurde die Datei schon einmal gelesen? Der Pfad steht beim Start fest,
    /// das Turnier kommt erst mit dem ersten Snapshot — geladen wird deshalb
    /// beim ersten `set_tournament` (Muster `AutoAssignExclusionStore`).
    loaded: bool,
}

/// Der Wunschfeld-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit TL-Web-Actions und
/// Sync-Lauf denselben Stand sehen.
#[derive(Default)]
pub struct WishCourtStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests, Slave-Betrieb).
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `OfficialsStore`).
    persist_lock: Mutex<()>,
}

impl WishCourtStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        // Ein neuer Pfad ist ungelesen — sonst hinge die Richtigkeit daran,
        // dass `set_path` immer VOR dem ersten Snapshot läuft.
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot). Beim ersten Mal
    /// wird der passende Stand geladen; bei Turnierwechsel verworfen.
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
                    Ladung::Stand(file) => {
                        inner.loaded = true;
                        if file.tournament == name {
                            inner.file = file;
                            return;
                        }
                    }
                    Ladung::Leer => inner.loaded = true,
                    // Datei da, aber gerade nicht lesbar (Virenscanner,
                    // hängendes Handle): NICHTS anfassen, später erneut.
                    Ladung::Unlesbar => return,
                }
            }
            // Turnierwechsel (oder fremder Dateistand): verwerfen. Eine
            // CourtID aus dem Vortagesturnier zeigt auf ein anderes Feld.
            inner.file = WishFile {
                tournament: name.to_string(),
                ..Default::default()
            };
        }
        self.persist();
    }

    /// Aktuell gebundenes Turnier (leer, solange keins gebunden ist).
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    /// Wunschfeld dieses Spiels, falls eines gesetzt ist.
    pub fn wish(&self, match_id: i64) -> Option<i64> {
        self.inner
            .lock()
            .unwrap()
            .file
            .wishes
            .get(&match_id)
            .copied()
    }

    /// Alle Wünsche (für den TL-Zustand, eine Sperre statt vieler).
    pub fn all(&self) -> HashMap<i64, i64> {
        self.inner.lock().unwrap().file.wishes.clone()
    }

    /// Wunschfeld setzen (`Some`) oder aufheben (`None`).
    pub fn set_wish(&self, match_id: i64, court_id: Option<i64>) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match court_id {
                Some(c) => inner.file.wishes.insert(match_id, c) != Some(c),
                None => inner.file.wishes.remove(&match_id).is_some(),
            }
        };
        if changed {
            self.persist();
        }
    }

    /// Alle Match-IDs entfernen, die nicht mehr in `keep` stehen (Aufräumen
    /// bei Spielende, aus `sync.rs::reconcile_wish_courts`). Liefert `true`,
    /// wenn sich etwas geändert hat.
    pub fn retain(&self, keep: &HashSet<i64>) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let before = inner.file.wishes.len();
            inner.file.wishes.retain(|id, _| keep.contains(id));
            before != inner.file.wishes.len()
        };
        if changed {
            self.persist();
        }
        changed
    }

    /// Wünsche auf Felder entfernen, die es nicht (mehr) gibt. Ein Wunsch auf
    /// ein verschwundenes Feld ließe das Spiel für immer warten, ohne dass
    /// jemand die Ursache sähe.
    pub fn retain_courts(&self, valid: &HashSet<i64>) -> Vec<i64> {
        let entfernt = {
            let mut inner = self.inner.lock().unwrap();
            let ids: Vec<i64> = inner
                .file
                .wishes
                .iter()
                .filter(|(_, court)| !valid.contains(court))
                .map(|(id, _)| *id)
                .collect();
            for id in &ids {
                inner.file.wishes.remove(id);
            }
            ids
        };
        if !entfernt.is_empty() {
            self.persist();
        }
        entfernt
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
                tracing::warn!("wish-courts.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<WishFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("wish-courts.json unlesbar – beginne leer");
                Ladung::Leer
            }
        }
    }

    /// Schreiben (best effort, atomar).
    fn persist(&self) {
        let Some(path) = self.path.read().unwrap().clone() else {
            return; // Persistenz aus (Tests, Slave-Betrieb)
        };
        // Schreib-Guard VOR dem Schnappschuss (Muster `OfficialsStore`).
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

    fn store_mit_datei(dir: &Path) -> WishCourtStore {
        let store = WishCourtStore::default();
        store.set_path(dir.join("wish-courts.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    #[test]
    fn wunsch_setzen_und_aufheben() {
        let store = WishCourtStore::default(); // ohne Persistenz
        assert_eq!(store.wish(10), None);
        store.set_wish(10, Some(3));
        assert_eq!(store.wish(10), Some(3));
        // Umsetzen auf ein anderes Feld.
        store.set_wish(10, Some(1));
        assert_eq!(store.wish(10), Some(1));
        store.set_wish(10, None);
        assert_eq!(store.wish(10), None);
    }

    #[test]
    fn der_stand_ueberlebt_einen_neustart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.set_wish(10, Some(3));
            store.set_wish(11, Some(1));
        }
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.wish(10), Some(3));
        assert_eq!(neu.wish(11), Some(1));
    }

    #[test]
    fn ein_turnierwechsel_verwirft_die_wuensche() {
        // ADR 0022: Match- und CourtIDs gelten nur innerhalb eines Turniers.
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.set_wish(10, Some(3));
        }
        let neu = WishCourtStore::default();
        neu.set_path(dir.path().join("wish-courts.json"));
        neu.set_tournament("Ein anderes Turnier");
        assert_eq!(neu.wish(10), None, "fremder Stand wird verworfen");
        assert_eq!(neu.tournament(), "Ein anderes Turnier");
    }

    #[test]
    fn beendete_spiele_werden_aufgeraeumt() {
        let store = WishCourtStore::default();
        store.set_wish(10, Some(3));
        store.set_wish(11, Some(1));
        let keep: HashSet<i64> = [10].into_iter().collect();
        assert!(store.retain(&keep));
        assert_eq!(store.wish(10), Some(3));
        assert_eq!(store.wish(11), None);
        // Nochmal dasselbe ändert nichts mehr.
        assert!(!store.retain(&keep));
    }

    #[test]
    fn ein_wunsch_auf_ein_verschwundenes_feld_wird_geloest() {
        // Sonst wartete das Spiel für immer auf ein Feld, das es nicht mehr
        // gibt — und in der Liste stünde nur „wartet", ohne Grund.
        let store = WishCourtStore::default();
        store.set_wish(10, Some(3));
        store.set_wish(11, Some(99)); // Feld gibt es nicht mehr
        let felder: HashSet<i64> = [1, 2, 3].into_iter().collect();
        assert_eq!(store.retain_courts(&felder), vec![11]);
        assert_eq!(store.wish(10), Some(3));
        assert_eq!(store.wish(11), None);
        assert!(store.retain_courts(&felder).is_empty());
    }

    #[test]
    fn ohne_turnier_wird_nichts_geschrieben() {
        // Ein Stand ohne Turnier-Kopf wäre beim nächsten Start nicht
        // zuzuordnen — dann lieber gar keine Datei.
        let dir = tempfile::tempdir().unwrap();
        let store = WishCourtStore::default();
        store.set_path(dir.path().join("wish-courts.json"));
        store.set_wish(10, Some(3));
        assert!(!dir.path().join("wish-courts.json").exists());
    }
}
