//! Manuelle Spielreihenfolge je Halle (Spec
//! `docs/features/spielliste-manuelle-reihenfolge.md`, ADR 0023).
//!
//! Enthält ausschließlich Match-IDs in ihrer manuell gezogenen Reihenfolge
//! — kein Personendatum. R2 bleibt gewahrt: das ist ein zusätzlicher,
//! rein lokal geprüfter Sortierschlüssel, keine Court-Zuordnung.
//!
//! **Präfix-Mechanik:** Ein Zug speichert nicht nur das gezogene Match,
//! sondern die komplette effektive Reihenfolge **vom Anfang der Halle bis
//! zum neuen Platz** des gezogenen Matches (Nutzer-Idee 14.08.2026:
//! „speicher dir die Reihenfolge bis zu dem Spiel wo wir sortiert haben").
//! Dadurch bleibt der manuell sortierte Block immer ein zusammenhängender
//! Anfangsabschnitt, auch wenn ein Match vor ein bislang nie angefasstes
//! (rein BTP-sortiertes) Ziel gezogen wird — ohne das über eine
//! „unbekanntes Ziel ⇒ nichts tun"-Regel wie beim Schiedsrichter-Reorder
//! (`officials.rs::reorder`) zu verlieren, das nur eine bereits
//! vollständige Liste (den kompletten Roster) kennt.
//!
//! Grundsätze (Muster ADR 0022, wie `exclusion.rs`/`officials.rs`):
//! - **Turniergebunden**: eigene Datei im App-Datenverzeichnis, im Kopf
//!   das Turnier. Turnierwechsel verwirft den Stand.
//! - **Außerhalb der `AppConfig`**: kein zusätzlicher globaler Schalter.
//! - **Best effort**: ein Schreibfehler kostet höchstens die Anzeige-
//!   Reihenfolge, nie ein Ergebnis.
//! - **Aufräumen**: `sync.rs::reconcile_queue_order` entfernt einen
//!   Eintrag, sobald sein Match zugewiesen/beendet/verschwunden ist oder
//!   die Halle gewechselt hat — dieses Modul kennt selbst keinen Snapshot.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct QueueOrderFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Hallenname (wie `assign::hall_for_match` ihn liefert) → Match-IDs
    /// in ihrer manuellen Reihenfolge. Fehlt eine Halle, hat sie (noch)
    /// keinen Präfix.
    #[serde(default)]
    order: HashMap<String, Vec<i64>>,
}

/// Ergebnis eines Ladeversuchs — Muster `exclusion.rs::Ladung`.
enum Ladung {
    Stand(QueueOrderFile),
    Leer,
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: QueueOrderFile,
    loaded: bool,
}

/// Der Reihenfolge-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit TL-Web-Actions und
/// Tauri-Commands denselben Stand sehen.
#[derive(Default)]
pub struct QueueOrderStore {
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    persist_lock: Mutex<()>,
}

impl QueueOrderStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot).
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.loaded && inner.file.tournament == name {
                return;
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
                    Ladung::Unlesbar => return,
                }
            }
            inner.file = QueueOrderFile {
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

    /// Rang eines Matches im Präfix seiner Halle (0 = ganz vorn), `None`
    /// wenn das Match nicht im Präfix steht.
    pub fn rank(&self, hall: &str, match_id: i64) -> Option<usize> {
        self.inner
            .lock()
            .unwrap()
            .file
            .order
            .get(hall)
            .and_then(|ids| ids.iter().position(|id| *id == match_id))
    }

    /// Ein Match vor ein anderes ziehen (`before = None` heißt „ans Ende
    /// des aktuell sichtbaren Präfix-Blocks"). `effective_order` ist die
    /// aktuelle, vollständig sortierte Match-ID-Liste dieser Halle
    /// (BTP-Reihenfolge + bisheriger Präfix kombiniert, wie sie die
    /// Anzeige im Moment des Zugs zeigt) — daraus wird der neue Präfix
    /// abgeleitet: alles vom Anfang bis zum neuen Platz des gezogenen
    /// Matches (siehe Modul-Kommentar).
    pub fn reorder(&self, hall: &str, effective_order: &[i64], match_id: i64, before: Option<i64>) {
        if before == Some(match_id) {
            return; // vor sich selbst ziehen ist keine Bewegung
        }
        if !effective_order.contains(&match_id) {
            return; // unbekanntes Match ⇒ lieber nichts tun
        }
        let mut liste: Vec<i64> = effective_order
            .iter()
            .copied()
            .filter(|id| *id != match_id)
            .collect();
        let ziel = match before {
            Some(b) => match liste.iter().position(|id| *id == b) {
                Some(p) => p,
                None => return, // unbekanntes Ziel ⇒ lieber nichts tun
            },
            None => liste.len(),
        };
        liste.insert(ziel, match_id);
        let neuer_praefix: Vec<i64> = liste.into_iter().take(ziel + 1).collect();
        {
            let mut inner = self.inner.lock().unwrap();
            inner.file.order.insert(hall.to_string(), neuer_praefix);
        }
        self.persist();
    }

    /// Aus jeder Hallen-Liste alle Match-IDs entfernen, die nicht mehr im
    /// jeweiligen `keep`-Set stehen (Aufräumen bei Zuweisung/Spielende/
    /// Hallenwechsel, aus `sync.rs::reconcile_queue_order`). Eine Halle,
    /// die in `keep_by_hall` fehlt, wird komplett geleert. Liefert `true`,
    /// wenn sich etwas geändert hat.
    pub fn retain(&self, keep_by_hall: &HashMap<String, HashSet<i64>>) -> bool {
        let leer = HashSet::new();
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for (hall, ids) in inner.file.order.iter_mut() {
                let keep = keep_by_hall.get(hall).unwrap_or(&leer);
                let before = ids.len();
                ids.retain(|id| keep.contains(id));
                if ids.len() != before {
                    changed = true;
                }
            }
            changed
        };
        if changed {
            self.persist();
        }
        changed
    }

    /// Globaler Reset: die manuelle Reihenfolge **aller** Hallen auf
    /// einmal verwerfen. Bewusst ohne Hallen-Parameter — kein Reset je
    /// einzelne Halle (Nicht-Ziel der Spec).
    pub fn reset_all(&self) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            if inner.file.order.is_empty() {
                false
            } else {
                inner.file.order.clear();
                true
            }
        };
        if changed {
            self.persist();
        }
    }

    // ── Persistenz ────────────────────────────────────────────────────

    fn load_file(&self) -> Ladung {
        let Some(path) = self.path.read().unwrap().clone() else {
            return Ladung::Leer;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ladung::Leer,
            Err(e) => {
                tracing::warn!("queue-order.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<QueueOrderFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("queue-order.json unlesbar – beginne leer");
                Ladung::Leer
            }
        }
    }

    fn persist(&self) {
        let Some(path) = self.path.read().unwrap().clone() else {
            return;
        };
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let inner = self.inner.lock().unwrap();
            if inner.file.tournament.is_empty() {
                return;
            }
            inner.file.clone()
        };
        if let Ok(json) = serde_json::to_string(&data) {
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

    fn store_mit_datei(dir: &Path) -> QueueOrderStore {
        let store = QueueOrderStore::default();
        store.set_path(dir.join("queue-order.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    #[test]
    fn ein_nie_angefasstes_match_wird_vor_ein_ziel_gezogen_und_backfillt_den_block() {
        // BTP-Reihenfolge dieser Halle: 1, 2, 3, 4, 5. Match 4 wird vor
        // Match 2 gezogen — der neue Präfix speichert den Anfang bis zum
        // neuen Platz von 4: [1, 4]. Match 2 selbst braucht KEINEN
        // expliziten Rang — es fällt beim nächsten Sortierlauf ohnehin
        // direkt hinter den Präfix (BTP-Reihenfolge unter den nicht
        // eingereihten Spielen bleibt 2, 3, 5), landet also weiterhin
        // unmittelbar nach 4.
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3, 4, 5], 4, Some(2));
        assert_eq!(store.rank("Halle A", 1), Some(0));
        assert_eq!(store.rank("Halle A", 4), Some(1));
        assert_eq!(store.rank("Halle A", 2), None);
        assert_eq!(store.rank("Halle A", 3), None);
        assert_eq!(store.rank("Halle A", 5), None);
    }

    #[test]
    fn ein_zweiter_zug_verschiebt_innerhalb_des_bestehenden_blocks() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3, 4, 5], 4, Some(2)); // -> [1,4]
        // Effektive Reihenfolge jetzt: 1,4,2,3,5 (Präfix [1,4] + Rest in
        // BTP-Reihenfolge) — Match 1 vor Match 2 ziehen.
        store.reorder("Halle A", &[1, 4, 2, 3, 5], 1, Some(2));
        assert_eq!(store.rank("Halle A", 4), Some(0));
        assert_eq!(store.rank("Halle A", 1), Some(1));
        assert_eq!(store.rank("Halle A", 2), None);
    }

    #[test]
    fn vor_sich_selbst_ziehen_ist_keine_bewegung() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 2, Some(1)); // -> [2]
        store.reorder("Halle A", &[2, 1, 3], 2, Some(2)); // No-Op
        assert_eq!(store.rank("Halle A", 2), Some(0));
        assert_eq!(store.rank("Halle A", 1), None);
    }

    #[test]
    fn unbekanntes_ziel_ist_ein_no_op() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 1, Some(99));
        assert_eq!(store.rank("Halle A", 1), None);
    }

    #[test]
    fn unbekanntes_match_ist_ein_no_op() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 99, Some(2));
        assert_eq!(store.rank("Halle A", 99), None);
    }

    #[test]
    fn before_none_zieht_ans_ende_des_sichtbaren_blocks_und_uebernimmt_die_ganze_liste() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 1, None);
        assert_eq!(store.rank("Halle A", 2), Some(0));
        assert_eq!(store.rank("Halle A", 3), Some(1));
        assert_eq!(store.rank("Halle A", 1), Some(2));
    }

    #[test]
    fn hallen_sind_unabhaengig_voneinander() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 3, Some(1));
        store.reorder("Halle B", &[10, 20, 30], 30, Some(10));
        assert_eq!(store.rank("Halle A", 3), Some(0));
        assert_eq!(store.rank("Halle B", 30), Some(0));
        assert_eq!(store.rank("Halle B", 3), None);
    }

    #[test]
    fn stand_ueberlebt_app_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reorder("Halle A", &[1, 2, 3], 3, Some(1));

        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.rank("Halle A", 3), Some(0));
        assert_eq!(neu.rank("Halle A", 1), None);
    }

    #[test]
    fn turnierwechsel_verwirft_den_stand() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reorder("Halle A", &[1, 2, 3], 3, Some(1));

        store.set_tournament("Ganz anderes Turnier");
        assert_eq!(store.rank("Halle A", 3), None);
        assert_eq!(store.tournament(), "Ganz anderes Turnier");

        let neu = QueueOrderStore::default();
        neu.set_path(dir.path().join("queue-order.json"));
        neu.set_tournament("Test BTS Light");
        assert_eq!(neu.rank("Halle A", 3), None);
    }

    #[test]
    fn eine_voruebergehend_unlesbare_datei_wird_nicht_ueberschrieben() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("queue-order.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = QueueOrderStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Cup A");
        assert_eq!(store.tournament(), "");
        assert!(pfad.is_dir(), "der vorhandene Stand bleibt unangetastet");
        assert!(!pfad.with_extension("json.tmp").exists());

        std::fs::remove_dir(&pfad).unwrap();
        let vorlage = store_mit_datei(dir.path());
        vorlage.reorder("Halle A", &[1, 2], 2, Some(1));
        store.set_tournament("Test BTS Light");
        assert_eq!(store.rank("Halle A", 2), Some(0));
    }

    #[test]
    fn retain_entfernt_je_halle_nur_was_fehlt() {
        let store = QueueOrderStore::default();
        // Zwei Züge je Halle bauen einen echten Zwei-Elemente-Präfix auf.
        store.reorder("Halle A", &[1, 2, 3, 4], 2, Some(1)); // -> A: [2]
        store.reorder("Halle A", &[2, 1, 3, 4], 3, Some(1)); // -> A: [2,3]
        store.reorder("Halle B", &[10, 20, 30], 20, Some(10)); // -> B: [20]
        store.reorder("Halle B", &[20, 10, 30], 30, Some(10)); // -> B: [20,30]
        assert_eq!(store.rank("Halle A", 2), Some(0));
        assert_eq!(store.rank("Halle A", 3), Some(1));
        assert_eq!(store.rank("Halle B", 20), Some(0));
        assert_eq!(store.rank("Halle B", 30), Some(1));

        let mut keep = HashMap::new();
        keep.insert("Halle A".to_string(), [2].into_iter().collect());
        keep.insert("Halle B".to_string(), [20, 30].into_iter().collect());

        assert!(store.retain(&keep));
        assert_eq!(store.rank("Halle A", 2), Some(0));
        assert_eq!(store.rank("Halle A", 3), None);
        assert_eq!(store.rank("Halle B", 20), Some(0));
        assert_eq!(store.rank("Halle B", 30), Some(1));

        // Nochmal derselbe Aufruf ändert nichts mehr.
        assert!(!store.retain(&keep));
    }

    #[test]
    fn hallenwechsel_entfernt_aus_der_alten_halle_ohne_auto_insert_in_der_neuen() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 3, Some(1)); // -> A: [3,1]

        // Match 3 wechselt faktisch nach Halle B — keep_by_hall spiegelt
        // das: Halle A darf 3 nicht mehr enthalten, Halle B taucht gar
        // nicht erst auf (kein Auto-Insert).
        let mut keep = HashMap::new();
        keep.insert("Halle A".to_string(), [1].into_iter().collect());

        assert!(store.retain(&keep));
        assert_eq!(store.rank("Halle A", 3), None);
        assert_eq!(store.rank("Halle B", 3), None);
    }

    #[test]
    fn reset_all_leert_alle_hallen_auf_einmal() {
        let store = QueueOrderStore::default();
        store.reorder("Halle A", &[1, 2, 3], 3, Some(1));
        store.reorder("Halle B", &[10, 20], 20, Some(10));

        store.reset_all();
        assert_eq!(store.rank("Halle A", 3), None);
        assert_eq!(store.rank("Halle B", 20), None);
    }
}
