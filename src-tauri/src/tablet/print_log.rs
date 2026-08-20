//! Druck-Gedächtnis des Autodrucks (Spec
//! `docs/features/schiedsrichterzettel-autodruck.md`, E5).
//!
//! Merkt sich, für welches Spiel schon ein Zettel gedruckt wurde. Enthält
//! ausschließlich Match-IDs — kein Personendatum.
//!
//! **Warum überhaupt eine Datei?** Nach einem App-Neustart mitten im
//! Turnier sieht jedes belegte Feld wie frisch zugewiesen aus — dieselbe
//! Falle, die `officials::rotate_court` schon kennt („dort sieht jedes
//! belegte Feld wie neu belegt aus"). Ohne persistenten Vermerk kämen bei
//! zwanzig Feldern zwanzig Blatt aus dem Drucker. Der Vermerk ist deshalb
//! kein Beiwerk, sondern der Grund, warum die Automatik im Turnier
//! benutzbar ist.
//!
//! Grundsätze (Muster ADR 0022, wie `exclusion.rs`):
//! - **Turniergebunden**: eigene Datei im App-Datenverzeichnis, im Kopf
//!   das Turnier. Turnierwechsel verwirft — eine Match-ID gilt nur
//!   innerhalb eines Turniers.
//! - **Der Vermerk steht VOR dem Druckversuch** ([`PrintLogStore::merken`]
//!   prüft und setzt in einem Zug). Ein fehlgeschlagener Druck wiederholt
//!   sich deshalb nicht endlos, und ein Feld- oder Schiedsrichterwechsel
//!   erzeugt kein zweites Blatt.
//! - **Best effort**: Ein Schreibfehler kostet höchstens ein doppeltes
//!   Blatt nach einem Neustart, nie ein Ergebnis.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Dateiform. Im Kopf steht das Turnier — passt es nicht zum laufenden,
/// wird der Inhalt verworfen (ADR 0022: lieber verwerfen als falsch
/// zuordnen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PrintLogFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Match-IDs, für die ein Zettel bereits erzeugt wurde.
    #[serde(default)]
    printed: HashSet<i64>,
}

/// Ergebnis eines Ladeversuchs — der Unterschied zwischen „gibt es nicht"
/// und „komme gerade nicht dran" entscheidet, ob überschrieben werden darf.
enum Ladung {
    Stand(PrintLogFile),
    Leer,
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: PrintLogFile,
    loaded: bool,
}

/// Der Druck-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit Sync-Loop und
/// Commands denselben Stand sehen.
#[derive(Default)]
pub struct PrintLogStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests, Slave-Betrieb).
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    persist_lock: Mutex<()>,
}

impl PrintLogStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot). Beim ersten
    /// Mal wird der passende Stand geladen; bei Turnierwechsel verworfen.
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
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
                    // Datei da, aber gerade nicht lesbar: NICHTS anfassen
                    // und NICHT schreiben. Hier wiegt das besonders — ein
                    // voreilig geleerter Stand druckte das ganze Turnier
                    // noch einmal.
                    Ladung::Unlesbar => return,
                }
            }
            inner.file = PrintLogFile {
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

    /// Wurde für dieses Spiel schon gedruckt?
    pub fn ist_gedruckt(&self, match_id: i64) -> bool {
        self.inner.lock().unwrap().file.printed.contains(&match_id)
    }

    /// **Vormerken und melden, ob es neu war.**
    ///
    /// `true` = dieser Aufruf hat den Vermerk gesetzt, es ist also der
    /// Auftrag zu drucken. `false` = ein früherer Lauf war schneller, es
    /// passiert nichts.
    ///
    /// Prüfen und Setzen liegen bewusst in **einem** Zug unter demselben
    /// Schloss: Zwei Läufe, die dasselbe Spiel gleichzeitig sehen, dürfen
    /// nicht beide „neu" bekommen.
    pub fn merken(&self, match_id: i64) -> bool {
        if match_id <= 0 {
            return false;
        }
        let neu = {
            let mut inner = self.inner.lock().unwrap();
            inner.file.printed.insert(match_id)
        };
        if neu {
            self.persist();
        }
        neu
    }

    /// Vermerk zurücknehmen — für den Fall, dass gar kein Druckversuch
    /// zustande kam (etwa weil sich kein Blatt erzeugen ließ). Ein
    /// **gescheiterter** Druck bleibt dagegen vermerkt: Er soll sich nicht
    /// im Sekundentakt wiederholen.
    pub fn vergessen(&self, match_id: i64) {
        let weg = {
            let mut inner = self.inner.lock().unwrap();
            inner.file.printed.remove(&match_id)
        };
        if weg {
            self.persist();
        }
    }

    /// Anzahl der vermerkten Spiele (Diagnose).
    pub fn anzahl(&self) -> usize {
        self.inner.lock().unwrap().file.printed.len()
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
                tracing::warn!("gedruckt.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<PrintLogFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("gedruckt.json unlesbar – beginne leer");
                Ladung::Leer
            }
        }
    }

    /// Schreiben (best effort, atomar).
    fn persist(&self) {
        let Some(path) = self.path.read().unwrap().clone() else {
            return;
        };
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let inner = self.inner.lock().unwrap();
            if inner.file.tournament.is_empty() {
                return; // ohne Turnier-Kopf wäre der Stand nicht zuzuordnen
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

    #[test]
    fn merken_meldet_nur_beim_ersten_mal_neu() {
        let store = PrintLogStore::default();
        assert!(store.merken(42), "erster Aufruf ist der Druckauftrag");
        assert!(
            !store.merken(42),
            "der zweite darf nicht noch einmal drucken"
        );
        assert!(store.ist_gedruckt(42));
        assert!(!store.ist_gedruckt(43));
    }

    /// Ein Feld- oder Schiedsrichterwechsel darf kein zweites Blatt
    /// erzeugen — es gibt höchstens einen Zettel je Spiel.
    #[test]
    fn feldwechsel_druckt_kein_zweites_blatt() {
        let store = PrintLogStore::default();
        assert!(store.merken(7));
        // Dasselbe Spiel taucht auf einem anderen Feld wieder auf.
        assert!(!store.merken(7));
    }

    #[test]
    fn unsinnige_kennungen_werden_nicht_vermerkt() {
        let store = PrintLogStore::default();
        assert!(!store.merken(0));
        assert!(!store.merken(-3));
        assert_eq!(store.anzahl(), 0);
    }

    /// **Der Kern der Etappe:** Nach einem App-Neustart mitten im Turnier
    /// darf für schon laufende Spiele nichts nachgedruckt werden.
    #[test]
    fn neustart_druckt_nicht_nach() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("gedruckt.json");

        let vorher = PrintLogStore::default();
        vorher.set_path(pfad.clone());
        vorher.set_tournament("Jux-Turnier");
        for id in 1..=20 {
            assert!(vorher.merken(id));
        }
        drop(vorher);

        // Neustart: derselbe Pfad, dasselbe Turnier.
        let nachher = PrintLogStore::default();
        nachher.set_path(pfad);
        nachher.set_tournament("Jux-Turnier");
        assert_eq!(nachher.anzahl(), 20, "der Stand kommt zurück");
        for id in 1..=20 {
            assert!(
                !nachher.merken(id),
                "Spiel {id} würde nach dem Neustart ein zweites Blatt drucken"
            );
        }
    }

    /// Ein Turnierwechsel beginnt frisch — Match-IDs gelten nur innerhalb
    /// eines Turniers — und lässt die alte Datei nicht wieder auferstehen.
    #[test]
    fn ein_turnierwechsel_beginnt_frisch() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("gedruckt.json");

        let store = PrintLogStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Turnier A");
        assert!(store.merken(5));

        store.set_tournament("Turnier B");
        assert_eq!(store.anzahl(), 0);
        assert!(
            store.merken(5),
            "im neuen Turnier ist die 5 ein anderes Spiel"
        );

        // Und der neue Stand steht auch auf der Platte.
        let neu = PrintLogStore::default();
        neu.set_path(pfad);
        neu.set_tournament("Turnier B");
        assert!(!neu.merken(5));
    }

    /// Ein unlesbarer Bestand wird **nicht** überschrieben — sonst
    /// druckte ein hängendes Datei-Handle das halbe Turnier noch einmal.
    #[test]
    fn ein_unlesbarer_stand_wird_nicht_geleert() {
        let dir = tempfile::tempdir().unwrap();
        // Ein Verzeichnis an der Stelle der Datei ist lesbar-unmöglich.
        let pfad = dir.path().join("gedruckt.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = PrintLogStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Jux-Turnier");
        assert!(store.tournament().is_empty(), "kein Turnier gebunden");
        assert!(pfad.is_dir(), "der vorhandene Stand bleibt unangetastet");
    }

    /// Ohne Turnier-Kopf wird nichts geschrieben — ein Stand ohne
    /// Zuordnung wäre beim nächsten Start nicht einzuordnen.
    #[test]
    fn ohne_turnier_kein_schreiben() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("gedruckt.json");
        let store = PrintLogStore::default();
        store.set_path(pfad.clone());
        assert!(store.merken(1));
        assert!(!pfad.exists());
    }
}
