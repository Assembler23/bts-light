//! Druck-Gedächtnis des Autodrucks (Spec
//! `docs/features/schiedsrichterzettel-autodruck.md`, E5).
//!
//! Merkt sich, für welches Spiel schon ein Zettel gedruckt wurde. Im
//! Modus „beim Aufruf" (Spec `zettel-druck-bei-aufruf`, ADR 0082) dazu,
//! **welcher Schiedsrichter** auf dem Vorabzettel steht — als Official-ID,
//! ohne Namen. Die Datei bleibt lokal auf dem Turnier-PC und verfällt mit
//! dem Turnier (wie `officials-state.json`).
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
//!
//! **Modus „beim Aufruf" (ADR 0082)** — die Datei bleibt abwärtskompatibel:
//! - `printed` behält seine Bedeutung („für dieses Spiel kommt nichts mehr
//!   von selbst"), ergänzt um `zettel` (Vorabzettel, deren Spiel noch nicht
//!   auf dem Feld war) und `modus` (zuletzt wirksamer Druckzeitpunkt).
//!   Regel: Ein Spiel in `printed` **ohne** `zettel`-Eintrag ist erledigt.
//!   Eine ältere Version liest nur `printed` — nach einem Rollback druckt
//!   sie deshalb nichts doppelt.
//! - Die 20-s-Frist eines Schiedsrichterwechsels liegt nur im Speicher: Sie
//!   übersteht das Speichern der Einstellungen (der Store lebt im
//!   `TabletState`, nicht in der SyncEngine), ein App-Neustart beginnt sie neu.
//! - Die ganze Entscheidung je Spiel ([`PrintLogStore::aufruf_schritt`])
//!   läuft unter **einem** Schloss — wie `merken`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::config::PrintTrigger;

/// Wie lange ein neuer Schiedsrichter unverändert stehen muss, bevor der
/// Vorabzettel nachgedruckt wird (ADR 0082). Fängt schnelles Umteilen ab.
pub const NACHDRUCK_FRIST_MS: u64 = 20_000;

/// Dateiform. Im Kopf steht das Turnier — passt es nicht zum laufenden,
/// wird der Inhalt verworfen (ADR 0022: lieber verwerfen als falsch
/// zuordnen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PrintLogFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Match-IDs, für die ein Zettel bereits erzeugt wurde (oder die beim
    /// Umschalten vorgemerkt wurden).
    #[serde(default)]
    printed: HashSet<i64>,
    /// Vorabzettel, deren Spiel noch nicht auf dem Feld war (Teilmenge von
    /// `printed`). Fehlt in Dateien bis v0.9.372.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    zettel: HashMap<i64, ZettelStand>,
    /// Zuletzt wirksamer Druckzeitpunkt; `None` = Autodruck aus oder Datei
    /// aus einer älteren Version. Ein Wechsel auf `Aufruf` merkt vor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modus: Option<PrintTrigger>,
}

/// Stand eines Vorabzettels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZettelStand {
    /// Official-ID des Schiedsrichters auf dem Blatt.
    pub sr: i64,
    /// Der Schiedsrichter wurde entfernt, die Warnung ist schon raus.
    #[serde(default)]
    pub ungueltig: bool,
}

/// Was der Sync-Lauf über ein Spiel weiß — die Eingabe des Zustandsautomaten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AufrufLage {
    /// Das Spiel wartet (Status Scheduled).
    Wartend {
        /// Aktiver Aufruf im Aufruf-Speicher des Turnier-PCs.
        gerufen: bool,
        /// Beide Seiten der Paarung stehen fest.
        voll: bool,
        /// SR in der Sicht „Hand vor BTP"; 0 = keiner.
        sr: i64,
    },
    /// Das Spiel steht auf einem Feld.
    AmFeld {
        /// SR in der wirksamen Sicht (BTP gewinnt); 0 = keiner.
        sr: i64,
        /// BTP führt denselben SR wie der lokale Schreibwert. Erst dann ist
        /// die Besetzung am Feld entschieden — vorher schwankt sie um einen
        /// Takt (Rotation löst lokal, BTP führt noch den alten).
        bestaetigt: bool,
    },
}

/// Was zu tun ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AufrufAktion {
    Nichts,
    /// Vorabzettel ohne Feld drucken.
    DruckVorab {
        vorher: Vorher,
    },
    /// Zettel mit Feld drucken.
    DruckFeld {
        vorher: Vorher,
    },
    /// „Zettel für Spiel N ungültig – Schiedsrichter entfernt".
    Ungueltig,
}

/// Stand vor einem Druckauftrag — damit [`PrintLogStore::zuruecksetzen`]
/// ihn wiederherstellen kann, wenn gar kein Blatt entsteht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vorher {
    printed: bool,
    zettel: Option<ZettelStand>,
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
    /// Laufende Nachdruck-Fristen: Spiel → (neuer SR, seit). Nur im RAM.
    kandidat: HashMap<i64, (i64, u64)>,
    /// Spiele, deren Feld-Besetzung BTP noch nicht bestätigt hat und für
    /// die das schon im Log steht (einmal je Spiel). Nur im RAM.
    unbestaetigt_gemeldet: HashSet<i64>,
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
            inner.kandidat.clear();
            inner.unbestaetigt_gemeldet.clear();
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

    /// Stand des Vorabzettels eines Spiels (Diagnose, Tests).
    pub fn zettel(&self, match_id: i64) -> Option<ZettelStand> {
        self.inner
            .lock()
            .unwrap()
            .file
            .zettel
            .get(&match_id)
            .copied()
    }

    /// Den wirksamen Druckzeitpunkt abgleichen (`None` = Autodruck aus).
    ///
    /// Wird `Aufruf` wirksam — umgeschaltet, eingeschaltet, neues Turnier
    /// oder Datei einer älteren Version —, gelten die Spiele aus
    /// `vormerken` als erledigt: Sie bekommen keinen Zettel, auch nicht am
    /// Feld (kein Druckstoß, ADR 0082). Zustand statt Ereignis: Der
    /// Vergleich läuft gegen den Stand in der Datei, deshalb greift er auch
    /// nach einem Neustart und nie zweimal.
    ///
    /// Erst wirken lassen, wenn das Turnier gebunden ist — sonst lädt
    /// `set_tournament` danach die Datei und überschreibt das Vormerken.
    pub fn modus_angleichen(
        &self,
        modus: Option<PrintTrigger>,
        vormerken: impl FnOnce() -> Vec<i64>,
    ) {
        let geaendert = {
            let mut inner = self.inner.lock().unwrap();
            if inner.file.modus == modus {
                false
            } else {
                if modus == Some(PrintTrigger::Aufruf) {
                    for id in vormerken().into_iter().filter(|id| *id > 0) {
                        if !inner.file.zettel.contains_key(&id) {
                            inner.file.printed.insert(id);
                        }
                    }
                }
                inner.file.modus = modus;
                inner.kandidat.clear();
                true
            }
        };
        if geaendert {
            self.persist();
        }
    }

    /// **Ein Schritt des Zustandsautomaten** im Modus „beim Aufruf"
    /// (Spec `zettel-druck-bei-aufruf`, Tabelle „Zustände je Spiel").
    ///
    /// Zustände: *neu* (kein Vermerk), *vorab(X)* (`zettel` mit SR X),
    /// *ungültig(X)* (dasselbe mit `ungueltig`), *fertig* (in `printed`
    /// ohne `zettel`). Prüfen und Setzen in einem Zug — ein Druckauftrag
    /// ist zugleich beansprucht.
    pub fn aufruf_schritt(&self, match_id: i64, lage: AufrufLage, jetzt_ms: u64) -> AufrufAktion {
        if match_id <= 0 {
            return AufrufAktion::Nichts;
        }
        let (aktion, geaendert) = {
            let mut inner = self.inner.lock().unwrap();
            let vorher = Vorher {
                printed: inner.file.printed.contains(&match_id),
                zettel: inner.file.zettel.get(&match_id).copied(),
            };
            schritt(&mut inner, match_id, lage, jetzt_ms, vorher)
        };
        if geaendert {
            self.persist();
        }
        aktion
    }

    /// Einen Druckauftrag rückgängig machen, aus dem kein Blatt wurde.
    pub fn zuruecksetzen(&self, match_id: i64, vorher: Vorher) {
        {
            let mut inner = self.inner.lock().unwrap();
            if vorher.printed {
                inner.file.printed.insert(match_id);
            } else {
                inner.file.printed.remove(&match_id);
            }
            match vorher.zettel {
                Some(z) => inner.file.zettel.insert(match_id, z),
                None => inner.file.zettel.remove(&match_id),
            };
        }
        self.persist();
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

/// Der Automat selbst — `(Aktion, Datei geändert)`.
fn schritt(
    inner: &mut Inner,
    id: i64,
    lage: AufrufLage,
    jetzt_ms: u64,
    vorher: Vorher,
) -> (AufrufAktion, bool) {
    let file = &mut inner.file;
    let erledigt = vorher.printed && vorher.zettel.is_none();
    if erledigt {
        inner.kandidat.remove(&id);
        return (AufrufAktion::Nichts, false);
    }
    match (vorher.zettel, lage) {
        // ── neu ──
        (None, AufrufLage::Wartend { gerufen, voll, sr }) => {
            if gerufen && voll && sr > 0 {
                file.printed.insert(id);
                file.zettel.insert(
                    id,
                    ZettelStand {
                        sr,
                        ungueltig: false,
                    },
                );
                (AufrufAktion::DruckVorab { vorher }, true)
            } else {
                (AufrufAktion::Nichts, false)
            }
        }
        // Rückfall: ohne Vorabzettel aufs Feld — wie bei der Feldvergabe.
        (None, AufrufLage::AmFeld { sr, .. }) => {
            if sr > 0 {
                file.printed.insert(id);
                (AufrufAktion::DruckFeld { vorher }, true)
            } else {
                (AufrufAktion::Nichts, false)
            }
        }
        // ── vorab(X) / ungültig(X), Spiel wartet ──
        (Some(z), AufrufLage::Wartend { gerufen, voll, sr }) => {
            if sr == z.sr {
                inner.kandidat.remove(&id);
                if z.ungueltig {
                    // Derselbe SR ist zurück — das Blatt stimmt wieder.
                    file.zettel.insert(
                        id,
                        ZettelStand {
                            sr,
                            ungueltig: false,
                        },
                    );
                    return (AufrufAktion::Nichts, true);
                }
                return (AufrufAktion::Nichts, false);
            }
            if sr == 0 {
                inner.kandidat.remove(&id);
                if z.ungueltig {
                    return (AufrufAktion::Nichts, false);
                }
                file.zettel.insert(
                    id,
                    ZettelStand {
                        ungueltig: true,
                        ..z
                    },
                );
                return (AufrufAktion::Ungueltig, true);
            }
            // Ein anderer SR: Frist laufen lassen (bei neuem SR neu starten).
            let seit = match inner.kandidat.get(&id) {
                Some((k_sr, seit)) if *k_sr == sr => *seit,
                _ => {
                    inner.kandidat.insert(id, (sr, jetzt_ms));
                    jetzt_ms
                }
            };
            if jetzt_ms.saturating_sub(seit) >= NACHDRUCK_FRIST_MS && gerufen && voll {
                inner.kandidat.remove(&id);
                file.zettel.insert(
                    id,
                    ZettelStand {
                        sr,
                        ungueltig: false,
                    },
                );
                return (AufrufAktion::DruckVorab { vorher }, true);
            }
            (AufrufAktion::Nichts, false)
        }
        // ── vorab(X) / ungültig(X), Spiel auf dem Feld ──
        (Some(z), AufrufLage::AmFeld { sr, bestaetigt }) => {
            inner.kandidat.remove(&id);
            if !bestaetigt {
                // Bewusst still (lieber kein Blatt als ein falsches) — aber
                // einmal im Log, damit der Zustand auffindbar ist.
                if inner.unbestaetigt_gemeldet.insert(id) {
                    tracing::info!(
                        "Zettel-Autodruck: Spiel {id} am Feld, BTP hat den Schiedsrichter \
                         noch nicht bestätigt – Entscheidung wartet"
                    );
                }
                return (AufrufAktion::Nichts, false);
            }
            // Ab hier ist das Spiel fertig: kein weiteres Blatt.
            file.zettel.remove(&id);
            let aktion = if sr == z.sr {
                AufrufAktion::Nichts
            } else if sr > 0 {
                AufrufAktion::DruckFeld { vorher }
            } else if z.ungueltig {
                AufrufAktion::Nichts
            } else {
                AufrufAktion::Ungueltig
            };
            (aktion, true)
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

    // ── Modus „beim Aufruf" (Spec `zettel-druck-bei-aufruf`, ADR 0082) ──

    use crate::config::PrintTrigger;

    fn wartend(sr: i64) -> AufrufLage {
        AufrufLage::Wartend {
            gerufen: true,
            voll: true,
            sr,
        }
    }

    fn am_feld(sr: i64) -> AufrufLage {
        AufrufLage::AmFeld {
            sr,
            bestaetigt: true,
        }
    }

    fn ist_vorab(a: AufrufAktion) -> bool {
        matches!(a, AufrufAktion::DruckVorab { .. })
    }

    fn ist_feld(a: AufrufAktion) -> bool {
        matches!(a, AufrufAktion::DruckFeld { .. })
    }

    #[test]
    fn alte_datei_ohne_zettel_ist_lesbar() {
        let f: PrintLogFile =
            serde_json::from_str(r#"{"tournament":"T","printed":[1,2]}"#).unwrap();
        assert!(f.zettel.is_empty());
        assert_eq!(f.modus, None);
        assert_eq!(f.printed.len(), 2);
    }

    /// Rollback: Eine ältere Version kennt nur `tournament` und `printed`
    /// — sie muss die neue Datei lesen können, sonst druckte sie das ganze
    /// Turnier noch einmal.
    #[test]
    fn neue_datei_ist_fuer_alte_struktur_lesbar() {
        #[derive(Deserialize)]
        struct Alt {
            tournament: String,
            printed: HashSet<i64>,
        }
        let store = PrintLogStore::default();
        store.set_tournament("T");
        store.modus_angleichen(Some(PrintTrigger::Aufruf), Vec::new);
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(1), 0)));
        let json = serde_json::to_string(&store.inner.lock().unwrap().file).unwrap();
        assert!(json.contains("zettel") && json.contains("modus"), "{json}");
        let alt: Alt = serde_json::from_str(&json).unwrap();
        assert_eq!(alt.tournament, "T");
        assert!(
            alt.printed.contains(&7),
            "vorab Gedrucktes zählt als gedruckt"
        );
    }

    #[test]
    fn printed_ohne_zettel_gilt_als_abgeschlossen() {
        let store = PrintLogStore::default();
        assert!(store.merken(7));
        assert_eq!(store.aufruf_schritt(7, wartend(1), 0), AufrufAktion::Nichts);
        assert_eq!(store.aufruf_schritt(7, am_feld(2), 0), AufrufAktion::Nichts);
    }

    #[test]
    fn zettel_und_modus_ueberleben_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("gedruckt.json");
        let vorher = PrintLogStore::default();
        vorher.set_path(pfad.clone());
        vorher.set_tournament("T");
        vorher.modus_angleichen(Some(PrintTrigger::Aufruf), Vec::new);
        assert!(ist_vorab(vorher.aufruf_schritt(7, wartend(1), 0)));
        drop(vorher);

        let nachher = PrintLogStore::default();
        nachher.set_path(pfad);
        nachher.set_tournament("T");
        assert_eq!(
            nachher.zettel(7),
            Some(ZettelStand {
                sr: 1,
                ungueltig: false
            })
        );
        // Der Modus steht schon — kein zweites Vormerken.
        nachher.modus_angleichen(Some(PrintTrigger::Aufruf), || vec![8]);
        assert!(!nachher.ist_gedruckt(8));
        assert_eq!(
            nachher.aufruf_schritt(7, wartend(1), 0),
            AufrufAktion::Nichts
        );
    }

    #[test]
    fn turnierwechsel_verwirft_zettel_und_kandidaten() {
        let store = PrintLogStore::default();
        store.set_tournament("A");
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(1), 0)));
        // Frist für SR 2 läuft …
        store.aufruf_schritt(7, wartend(2), 0);
        store.set_tournament("B");
        assert_eq!(store.zettel(7), None);
        assert!(store.inner.lock().unwrap().kandidat.is_empty());
        // Im neuen Turnier ist die 7 ein neues Spiel.
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(1), 0)));
    }

    #[test]
    fn beanspruche_vorab_nur_einmal() {
        let store = PrintLogStore::default();
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(1), 0)));
        assert_eq!(store.aufruf_schritt(7, wartend(1), 1), AufrufAktion::Nichts);
    }

    #[test]
    fn ohne_aufruf_ohne_paarung_ohne_sr_kein_vorabdruck() {
        let store = PrintLogStore::default();
        for lage in [
            AufrufLage::Wartend {
                gerufen: false,
                voll: true,
                sr: 1,
            },
            AufrufLage::Wartend {
                gerufen: true,
                voll: false,
                sr: 1,
            },
            AufrufLage::Wartend {
                gerufen: true,
                voll: true,
                sr: 0,
            },
        ] {
            assert_eq!(store.aufruf_schritt(7, lage, 0), AufrufAktion::Nichts);
        }
        assert!(!store.ist_gedruckt(7));
    }

    #[test]
    fn nachdruck_erst_nach_der_frist() {
        let store = PrintLogStore::default();
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(1), 0)));
        assert_eq!(
            store.aufruf_schritt(7, wartend(2), 1_000),
            AufrufAktion::Nichts
        );
        assert_eq!(
            store.aufruf_schritt(7, wartend(2), 1_000 + NACHDRUCK_FRIST_MS - 1),
            AufrufAktion::Nichts
        );
        assert!(ist_vorab(store.aufruf_schritt(
            7,
            wartend(2),
            1_000 + NACHDRUCK_FRIST_MS
        )));
        assert_eq!(store.zettel(7).unwrap().sr, 2);
    }

    #[test]
    fn kandidat_startet_neu_bei_anderem_sr() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        store.aufruf_schritt(7, wartend(2), 0);
        // Nach 15 s kommt C — die Frist beginnt für C neu.
        store.aufruf_schritt(7, wartend(3), 15_000);
        assert_eq!(
            store.aufruf_schritt(7, wartend(3), 25_000),
            AufrufAktion::Nichts
        );
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(3), 35_000)));
        assert_eq!(store.zettel(7).unwrap().sr, 3);
    }

    #[test]
    fn kandidat_verschwindet_bei_rueckkehr_zum_gedruckten() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        store.aufruf_schritt(7, wartend(2), 0);
        store.aufruf_schritt(7, wartend(1), 10_000);
        // Selbst wenn danach wieder 2 kommt, beginnt die Frist neu.
        assert_eq!(
            store.aufruf_schritt(7, wartend(2), 25_000),
            AufrufAktion::Nichts
        );
        assert_eq!(store.zettel(7).unwrap().sr, 1);
    }

    #[test]
    fn nachdruck_wartet_auf_aufruf_und_paarung() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        let ohne_aufruf = AufrufLage::Wartend {
            gerufen: false,
            voll: true,
            sr: 2,
        };
        store.aufruf_schritt(7, ohne_aufruf, 0);
        assert_eq!(
            store.aufruf_schritt(7, ohne_aufruf, 60_000),
            AufrufAktion::Nichts
        );
        assert!(ist_vorab(store.aufruf_schritt(7, wartend(2), 60_001)));
    }

    #[test]
    fn sr_entfernt_warnt_einmal() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        assert_eq!(
            store.aufruf_schritt(7, wartend(0), 1),
            AufrufAktion::Ungueltig
        );
        assert_eq!(store.aufruf_schritt(7, wartend(0), 2), AufrufAktion::Nichts);
        assert!(store.zettel(7).unwrap().ungueltig);
        // Derselbe SR kommt zurück: Blatt stimmt wieder, kein Druck …
        assert_eq!(store.aufruf_schritt(7, wartend(1), 3), AufrufAktion::Nichts);
        assert!(!store.zettel(7).unwrap().ungueltig);
        // … und ein erneutes Entfernen warnt wieder.
        assert_eq!(
            store.aufruf_schritt(7, wartend(0), 4),
            AufrufAktion::Ungueltig
        );
    }

    #[test]
    fn nach_ungueltig_druckt_ein_neuer_sr_nach_der_frist() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        store.aufruf_schritt(7, wartend(0), 0);
        store.aufruf_schritt(7, wartend(2), 1_000);
        assert!(ist_vorab(store.aufruf_schritt(
            7,
            wartend(2),
            1_000 + NACHDRUCK_FRIST_MS
        )));
        assert_eq!(
            store.zettel(7),
            Some(ZettelStand {
                sr: 2,
                ungueltig: false
            })
        );
    }

    #[test]
    fn am_feld_mit_gedrucktem_sr_wird_fertig() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        assert_eq!(store.aufruf_schritt(7, am_feld(1), 1), AufrufAktion::Nichts);
        assert_eq!(store.zettel(7), None, "fertig");
        assert!(store.ist_gedruckt(7));
        // Kein weiteres Blatt — weder am Feld noch nach Rücknahme vom Feld.
        assert_eq!(store.aufruf_schritt(7, am_feld(2), 2), AufrufAktion::Nichts);
        assert_eq!(
            store.aufruf_schritt(7, wartend(3), 99_999),
            AufrufAktion::Nichts
        );
    }

    #[test]
    fn am_feld_mit_anderem_sr_sofort_nachdruck() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        // Ohne 20 s: Die Feldvergabe ist die letzte Gelegenheit.
        assert!(ist_feld(store.aufruf_schritt(7, am_feld(2), 1)));
        assert_eq!(store.aufruf_schritt(7, am_feld(3), 2), AufrufAktion::Nichts);
    }

    #[test]
    fn am_feld_wartet_bis_btp_bestaetigt() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        let offen = AufrufLage::AmFeld {
            sr: 1,
            bestaetigt: false,
        };
        for t in 1..5 {
            assert_eq!(store.aufruf_schritt(7, offen, t), AufrufAktion::Nichts);
        }
        assert!(store.zettel(7).is_some(), "noch nicht entschieden");
        assert_eq!(
            store.aufruf_schritt(7, am_feld(0), 6),
            AufrufAktion::Ungueltig
        );
        assert_eq!(store.zettel(7), None);
    }

    #[test]
    fn am_feld_ungueltig_warnt_nicht_zweimal() {
        let store = PrintLogStore::default();
        store.aufruf_schritt(7, wartend(1), 0);
        store.aufruf_schritt(7, wartend(0), 1);
        assert_eq!(store.aufruf_schritt(7, am_feld(0), 2), AufrufAktion::Nichts);
    }

    #[test]
    fn ohne_vorabzettel_druckt_das_feld() {
        let store = PrintLogStore::default();
        assert_eq!(store.aufruf_schritt(7, am_feld(0), 0), AufrufAktion::Nichts);
        assert!(ist_feld(store.aufruf_schritt(7, am_feld(1), 1)));
        assert_eq!(store.aufruf_schritt(7, am_feld(2), 2), AufrufAktion::Nichts);
    }

    #[test]
    fn zuruecksetzen_stellt_vorstand_wieder_her() {
        let store = PrintLogStore::default();
        let AufrufAktion::DruckVorab { vorher } = store.aufruf_schritt(7, wartend(1), 0) else {
            panic!("erwartet Vorabdruck");
        };
        store.zuruecksetzen(7, vorher);
        assert!(!store.ist_gedruckt(7));
        assert_eq!(store.zettel(7), None);

        store.aufruf_schritt(7, wartend(1), 0);
        let AufrufAktion::DruckFeld { vorher } = store.aufruf_schritt(7, am_feld(2), 1) else {
            panic!("erwartet Felddruck");
        };
        store.zuruecksetzen(7, vorher);
        assert_eq!(
            store.zettel(7),
            Some(ZettelStand {
                sr: 1,
                ungueltig: false
            })
        );
    }

    #[test]
    fn modus_angleichen_merkt_einmal_vor() {
        let store = PrintLogStore::default();
        store.set_tournament("T");
        store.modus_angleichen(Some(PrintTrigger::Feldvergabe), || vec![1]);
        assert!(!store.ist_gedruckt(1), "Feldvergabe merkt nichts vor");
        store.modus_angleichen(Some(PrintTrigger::Aufruf), || vec![1, 2]);
        assert!(store.ist_gedruckt(1) && store.ist_gedruckt(2));
        store.modus_angleichen(Some(PrintTrigger::Aufruf), || vec![3]);
        assert!(!store.ist_gedruckt(3), "nur beim Wechsel");
        // Ausschalten und im Aufruf-Modus wieder einschalten = Umschalten.
        store.modus_angleichen(None, Vec::new);
        store.modus_angleichen(Some(PrintTrigger::Aufruf), || vec![3]);
        assert!(store.ist_gedruckt(3));
    }

    #[test]
    fn vormerken_laesst_offene_vorabzettel_in_ruhe() {
        let store = PrintLogStore::default();
        store.modus_angleichen(Some(PrintTrigger::Aufruf), Vec::new);
        store.aufruf_schritt(7, wartend(1), 0);
        store.modus_angleichen(None, Vec::new);
        store.modus_angleichen(Some(PrintTrigger::Aufruf), || vec![7]);
        assert!(store.zettel(7).is_some(), "Vorabzettel bleibt offen");
    }
}
