//! Punktverlauf-Speicher des Hosts (Spec `docs/features/punktverlauf-graph.md`,
//! ADR 0014/0015).
//!
//! Hält je Match den Ballwechsel-Verlauf (`MatchTimeline` aus `relay-proto`)
//! im RAM und persistiert ihn **je Turnier** als eine JSON-Datei
//! `punktverlauf/<slug>.json` — dauerhaft, ohne Namen, in genau der Form,
//! die später der badhub-Push liest.
//!
//! Grundsätze:
//! - **Best effort wie `persist_scores`**: Ein Schreibfehler kostet den
//!   Graphen, nie das Zählen oder ein Ergebnis.
//! - **Verwerfen statt raten**: Ein Rally-Frame, der nicht lückenlos an den
//!   bekannten Verlauf passt (Nummer, Satz, Stand), wird verworfen — der
//!   nächste `RallySync` des Tablets ersetzt den Stand komplett und heilt
//!   jede Lücke (Offline-Phase, Host-Neustart, verlorene Frames).
//! - **BTP bleibt die Wahrheit** (R2): Der Verlauf ist reine
//!   Anzeige-Information; Abweichungen vom gewerteten Ergebnis kennzeichnet
//!   die Anzeige, nie umgekehrt.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use relay_proto::{MatchTimeline, MAX_RALLIES_PER_SET, MAX_TIMELINE_SETS};
use serde::{Deserialize, Serialize};

/// Debounce fürs Schreiben: Rallies kommen im Sekundentakt, die Datei muss
/// nicht jeden einzelnen sehen. Satzende/Resync/Finalisierung schreiben
/// sofort; die Lücke eines Absturzes heilt der Tablet-Resync (ADR 0015).
const PERSIST_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(3);

/// Dateiform: Turnier-Kopf + Verläufe. Bewusst **ohne Namen** (Datenschutz):
/// nur Kennungen und Punktfolgen.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TimelineFile {
    /// BTP-Turniername (Setting 1001) — Teil des Speicherschlüssels.
    #[serde(default)]
    tournament: String,
    /// Erstsichtungs-Datum (`YYYY-MM-DD`). BTP liefert kein Startdatum
    /// (Befund in `docs/btp_protocol.md`) — der Host stempelt selbst und
    /// behält den Stempel über Neustarts (er steht in der Datei).
    #[serde(default)]
    first_seen: String,
    /// turnier.de-GUID aus der Check-In-Config, falls gepflegt — die
    /// Brücke für den späteren badhub-Push. Leer = nicht konfiguriert.
    #[serde(default)]
    guid: String,
    #[serde(default)]
    matches: HashMap<i64, MatchTimeline>,
}

#[derive(Default)]
struct Inner {
    file: TimelineFile,
    /// Dateiname (Slug) des aktuellen Turniers. Leer = noch kein Turnier
    /// gesehen → nichts wird geschrieben.
    slug: String,
    dirty: bool,
    last_write: Option<Instant>,
}

/// Der Punktverlauf-Speicher. Lebt im [`TabletState`](super::state::TabletState)
/// und wird von LAN-Server, Relay-Client und Tauri-Commands geteilt.
#[derive(Default)]
pub struct TimelineStore {
    /// Ablage-Verzeichnis (`<app_data>/punktverlauf`). `None` = Persistenz
    /// aus (Tests, Slave-Betrieb).
    dir: RwLock<Option<PathBuf>>,
    /// GUID aus der Config — kommt beim Start, das Turnier erst mit dem
    /// ersten Snapshot.
    guid: RwLock<String>,
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `scores_persist_lock`).
    persist_lock: Mutex<()>,
}

impl TimelineStore {
    /// Ablage-Verzeichnis setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_dir(&self, dir: PathBuf) {
        let _ = std::fs::create_dir_all(&dir);
        *self.dir.write().unwrap() = Some(dir);
    }

    /// turnier.de-GUID aus der Config (leer = nicht gepflegt).
    pub fn set_guid(&self, guid: &str) {
        *self.guid.write().unwrap() = guid.trim().to_string();
        let mut inner = self.inner.lock().unwrap();
        if !inner.slug.is_empty() {
            let guid = self.guid.read().unwrap().clone();
            if inner.file.guid != guid {
                inner.file.guid = guid;
                inner.dirty = true;
            }
        }
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot). Beim ersten
    /// Mal bzw. bei Turnierwechsel wird die zugehörige Datei geladen oder
    /// angelegt; der bisherige Stand wird vorher gesichert.
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let slug = slugify(name);
        {
            let inner = self.inner.lock().unwrap();
            if inner.slug == slug {
                return; // unverändert — der Normalfall jedes Sync-Ticks
            }
        }
        // Turnierwechsel: alten Stand sichern, neuen laden/anlegen.
        self.persist(true);
        let loaded = self.load_file(&slug);
        let mut inner = self.inner.lock().unwrap();
        inner.slug = slug;
        inner.dirty = false;
        inner.last_write = None;
        inner.file = loaded.unwrap_or_else(|| TimelineFile {
            tournament: name.to_string(),
            first_seen: today(),
            guid: self.guid.read().unwrap().clone(),
            matches: HashMap::new(),
        });
    }

    /// Einen Ballwechsel anhängen. `false` = verworfen (Lücke, Fremd-Match,
    /// unplausibler Stand, Deckel) — dann heilt der nächste `RallySync`.
    pub fn apply_rally(
        &self,
        match_id: i64,
        set: i64,
        n: i64,
        winner: &str,
        score_a: i64,
        score_b: i64,
    ) -> bool {
        if match_id <= 0 {
            return false;
        }
        let w = match winner {
            "A" => 'A',
            "B" => 'B',
            _ => return false,
        };
        let mut inner = self.inner.lock().unwrap();
        let tl = inner.file.matches.entry(match_id).or_default();
        if tl.finished {
            // Nachzügler eines abgeschlossenen Spiels — der finalisierte
            // Stand bleibt (AK-14).
            return false;
        }
        // Sätze wachsen nur lückenlos: der Frame gehört in den letzten
        // Satz oder eröffnet genau den nächsten.
        let set = usize::try_from(set).unwrap_or(0);
        if set == 0 || set > MAX_TIMELINE_SETS {
            return false;
        }
        if set == tl.sets.len() + 1 {
            tl.sets.push(relay_proto::TimelineSet::default());
        }
        if set != tl.sets.len() {
            return false;
        }
        let s = tl.sets.last_mut().expect("Satz existiert nach Prüfung");
        if s.points.len() >= MAX_RALLIES_PER_SET {
            return false;
        }
        // Laufende Nummer lückenlos? (1-basiert je Satz-Aufzeichnung)
        if n != s.points.len() as i64 + 1 {
            return false;
        }
        // Stand-Plausibilität: Startstand + gezählte Gewinne muss den
        // mitgeschickten Stand ergeben — sonst ist irgendwo ein Frame
        // verloren gegangen und der Verlauf würde lügen.
        let a_now =
            s.start_a + s.points.chars().filter(|&c| c == 'A').count() as i64 + i64::from(w == 'A');
        let b_now =
            s.start_b + s.points.chars().filter(|&c| c == 'B').count() as i64 + i64::from(w == 'B');
        if a_now != score_a || b_now != score_b {
            return false;
        }
        s.points.push(w);
        inner.dirty = true;
        drop(inner);
        self.persist(false);
        true
    }

    /// Kompletter Resync: ersetzt den Verlauf des Matches. `false` =
    /// verworfen (ungültig, oder ein bereits finalisierter Stand würde
    /// von einem nicht-finalen Nachzügler überschrieben, AK-14).
    pub fn apply_sync(&self, match_id: i64, timeline: MatchTimeline) -> bool {
        if match_id <= 0 || !timeline.is_valid() {
            return false;
        }
        let mut inner = self.inner.lock().unwrap();
        if let Some(alt) = inner.file.matches.get(&match_id) {
            if alt.finished && !timeline.finished {
                return false;
            }
        }
        inner.file.matches.insert(match_id, timeline);
        inner.dirty = true;
        drop(inner);
        self.persist(true);
        true
    }

    /// Match abschließen (aus `process_result`): keine weiteren Rallies,
    /// Sonderausgang gekennzeichnet. Ohne aufgezeichneten Verlauf (Papier,
    /// Walkover) entsteht bewusst **kein** Eintrag.
    pub fn finalize(&self, match_id: i64, retired: bool) {
        let mut inner = self.inner.lock().unwrap();
        let Some(tl) = inner.file.matches.get_mut(&match_id) else {
            return;
        };
        tl.finished = true;
        if retired {
            tl.retired = true;
        }
        inner.dirty = true;
        drop(inner);
        self.persist(true);
    }

    /// Gibt es zu diesem Match einen zeigbaren Verlauf? Grundlage der
    /// `has_timeline`-Flags — kein „Klick ins Leere" bei Papier-Spielen.
    pub fn has_timeline(&self, match_id: i64) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.file.matches.get(&match_id).is_some_and(|tl| {
            tl.sets
                .iter()
                .any(|s| !s.points.is_empty() || s.start_a > 0 || s.start_b > 0)
        })
    }

    /// Verlauf eines Matches als JSON — die Form, die Command, LAN-Route
    /// und Relay-Antwort gleichermaßen ausliefern.
    pub fn timeline_json(&self, match_id: i64) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        let tl = inner.file.matches.get(&match_id)?;
        serde_json::to_string(tl).ok()
    }

    /// Verlauf typisiert (fürs Desktop-Command).
    pub fn timeline(&self, match_id: i64) -> Option<MatchTimeline> {
        let inner = self.inner.lock().unwrap();
        inner.file.matches.get(&match_id).cloned()
    }

    // ── Persistenz ────────────────────────────────────────────────────

    fn load_file(&self, slug: &str) -> Option<TimelineFile> {
        let dir = self.dir.read().unwrap().clone()?;
        let text = std::fs::read_to_string(dir.join(format!("{slug}.json"))).ok()?;
        match serde_json::from_str::<TimelineFile>(&text) {
            Ok(file) => Some(file),
            Err(_) => {
                tracing::warn!("Punktverlauf-Datei {slug}.json unlesbar – beginne leer");
                None
            }
        }
    }

    /// Schreiben, wenn fällig (`force` = sofort). Best effort: Fehler
    /// stören das Zählen nie.
    fn persist(&self, force: bool) {
        let Some(dir) = self.dir.read().unwrap().clone() else {
            return;
        };
        let data = {
            let mut inner = self.inner.lock().unwrap();
            if inner.slug.is_empty() || !inner.dirty {
                return;
            }
            if !force
                && inner
                    .last_write
                    .is_some_and(|t| t.elapsed() < PERSIST_DEBOUNCE)
            {
                return;
            }
            inner.dirty = false;
            inner.last_write = Some(Instant::now());
            (inner.slug.clone(), inner.file.clone())
        };
        let _guard = self.persist_lock.lock().unwrap();
        let path = dir.join(format!("{}.json", data.0));
        if let Ok(json) = serde_json::to_string(&data.1) {
            // Atomar wie `persist_scores`: erst Temp-Datei, dann Umbenennen —
            // ein Absturz mitten im Schreiben hinterlässt nie eine halbe Datei.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

/// Dateiname aus dem Turniernamen: Whitelist `[a-z0-9-]`, Umlaute
/// transliteriert, alles andere wird `-`. Der Name kommt aus BTP (fremde
/// Eingabe) und landet in einem Pfad — ohne Whitelist wäre das ein
/// Path-Traversal-Kandidat.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        match c {
            'a'..='z' | '0'..='9' => out.push(c),
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            _ => {
                if !out.ends_with('-') {
                    out.push('-');
                }
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    let mut out: String = out.chars().take(60).collect();
    if out.is_empty() {
        out = "turnier".to_string();
    }
    out
}

/// Heutiges Datum als `YYYY-MM-DD` (lokale Zeit — Turniere sind lokal).
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::TimelineSet;

    fn store_mit_dir(dir: &Path) -> TimelineStore {
        let store = TimelineStore::default();
        store.set_dir(dir.to_path_buf());
        store.set_tournament("Test BTS Light");
        store
    }

    fn sync_beispiel() -> MatchTimeline {
        MatchTimeline {
            sets: vec![TimelineSet {
                start_a: 0,
                start_b: 0,
                points: "AAB".to_string(),
            }],
            mid_game: false,
            retired: false,
            finished: false,
        }
    }

    #[test]
    fn rally_appends_in_order_and_rejects_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(store.apply_rally(5, 1, 1, "A", 1, 0));
        assert!(store.apply_rally(5, 1, 2, "B", 1, 1));
        // Lücke in der laufenden Nummer → verworfen, Stand unverändert.
        assert!(!store.apply_rally(5, 1, 4, "A", 3, 1));
        // Unplausibler Stand → verworfen.
        assert!(!store.apply_rally(5, 1, 3, "A", 9, 1));
        // Fremdzeichen → verworfen.
        assert!(!store.apply_rally(5, 1, 3, "X", 2, 1));
        // match_id 0 (alte Tablet-Seite) → immer verworfen.
        assert!(!store.apply_rally(0, 1, 3, "A", 2, 1));
        let tl = store.timeline(5).unwrap();
        assert_eq!(tl.sets[0].points, "AB");
    }

    #[test]
    fn rally_opens_next_set_only_in_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(store.apply_rally(5, 1, 1, "A", 1, 0));
        // Satz 3 ohne Satz 2 → Lücke, verworfen.
        assert!(!store.apply_rally(5, 3, 1, "A", 1, 0));
        // Satz 2 direkt anschließend → ok.
        assert!(store.apply_rally(5, 2, 1, "B", 0, 1));
        // Nachzügler für Satz 1, obwohl Satz 2 läuft → verworfen (heilt
        // der nächste Sync).
        assert!(!store.apply_rally(5, 1, 2, "A", 2, 0));
        let tl = store.timeline(5).unwrap();
        assert_eq!(tl.sets.len(), 2);
        assert_eq!(tl.sets[1].points, "B");
    }

    #[test]
    fn sync_replaces_completely_and_shortens_after_undo() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        for (n, w, a, b) in [(1, "A", 1, 0), (2, "A", 2, 0), (3, "B", 2, 1)] {
            assert!(store.apply_rally(5, 1, n, w, a, b));
        }
        // Undo am Tablet → Sync mit gekürztem Verlauf ersetzt komplett.
        let mut kurz = sync_beispiel();
        kurz.sets[0].points = "AA".to_string();
        assert!(store.apply_sync(5, kurz));
        assert_eq!(store.timeline(5).unwrap().sets[0].points, "AA");
        // Danach zählt es lückenlos weiter (n = 3).
        assert!(store.apply_rally(5, 1, 3, "B", 2, 1));
    }

    #[test]
    fn invalid_sync_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        let mut kaputt = sync_beispiel();
        kaputt.sets[0].points = "AXB".to_string();
        assert!(!store.apply_sync(5, kaputt));
        assert!(store.timeline(5).is_none());
    }

    #[test]
    fn late_sync_cannot_unfinalize_a_finished_match() {
        // AK-14: Offline-Nachzügler eines längst abgeschlossenen Spiels
        // darf den finalisierten Stand nicht wiederbeleben.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(store.apply_sync(5, sync_beispiel()));
        store.finalize(5, false);
        let spaet = sync_beispiel();
        assert!(!store.apply_sync(5, spaet));
        assert!(store.timeline(5).unwrap().finished);
        // Ein finaler Nachzügler (z. B. korrigierter Endstand) darf.
        let mut fertig = sync_beispiel();
        fertig.finished = true;
        assert!(store.apply_sync(5, fertig));
    }

    #[test]
    fn finalize_marks_retired_and_blocks_further_rallies() {
        // AK-13: Aufgabe mitten im Satz — Teil-Satz bleibt, gekennzeichnet.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(store.apply_rally(5, 1, 1, "A", 1, 0));
        store.finalize(5, true);
        let tl = store.timeline(5).unwrap();
        assert!(tl.finished && tl.retired);
        assert_eq!(tl.sets[0].points, "A");
        assert!(!store.apply_rally(5, 1, 2, "B", 1, 1));
    }

    #[test]
    fn finalize_without_recording_creates_no_entry() {
        // Papier-/Walkover-Spiele bekommen keinen leeren Geister-Eintrag.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        store.finalize(99, false);
        assert!(store.timeline(99).is_none());
        assert!(!store.has_timeline(99));
    }

    #[test]
    fn midgame_sync_keeps_start_score_and_marker() {
        // Zwischenstand-Einstieg (AK-7): Startstand + Kennzeichnung bleiben.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        let mitte = MatchTimeline {
            sets: vec![TimelineSet {
                start_a: 7,
                start_b: 5,
                points: String::new(),
            }],
            mid_game: true,
            retired: false,
            finished: false,
        };
        assert!(store.apply_sync(5, mitte));
        assert!(store.has_timeline(5)); // Startstand zählt als Verlauf
                                        // Erster gezählter Ballwechsel nach dem Einstieg: n=1, Stand 8:5.
        assert!(store.apply_rally(5, 1, 1, "A", 8, 5));
        let tl = store.timeline(5).unwrap();
        assert!(tl.mid_game);
        assert_eq!(tl.sets[0].points, "A");
    }

    #[test]
    fn persist_and_reload_across_restart() {
        // AK-9: Host-Neustart — die Datei bringt den Stand zurück, die
        // Erstsichtung bleibt gestempelt.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(store.apply_sync(5, sync_beispiel()));
        let first_seen = {
            let inner = store.inner.lock().unwrap();
            inner.file.first_seen.clone()
        };
        assert!(!first_seen.is_empty());

        // „Neustart": frischer Store, gleiches Verzeichnis, gleiches Turnier.
        let store2 = TimelineStore::default();
        store2.set_dir(dir.path().to_path_buf());
        store2.set_tournament("Test BTS Light");
        assert_eq!(store2.timeline(5).unwrap().sets[0].points, "AAB");
        let wieder = {
            let inner = store2.inner.lock().unwrap();
            inner.file.first_seen.clone()
        };
        assert_eq!(wieder, first_seen);
    }

    #[test]
    fn switching_tournament_opens_new_file_and_keeps_old() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(store.apply_sync(5, sync_beispiel()));
        store.set_tournament("Anderes Turnier");
        assert!(store.timeline(5).is_none());
        assert!(store.apply_sync(7, sync_beispiel()));
        // Zurück zum ersten Turnier: alter Stand liegt noch da.
        store.set_tournament("Test BTS Light");
        assert_eq!(store.timeline(5).unwrap().sets[0].points, "AAB");
        assert!(store.timeline(7).is_none());
        // Beide Dateien existieren.
        assert!(dir.path().join("test-bts-light.json").exists());
        assert!(dir.path().join("anderes-turnier.json").exists());
    }

    #[test]
    fn tournament_name_is_sanitized_for_filename() {
        // Der Name kommt aus BTP und landet im Pfad — Traversal-Versuche
        // und Sonderzeichen werden zur Whitelist plattgemacht.
        assert_eq!(slugify("../../etc/passwd"), "etc-passwd");
        assert_eq!(
            slugify("Köpi-Cup 2026 (Halle Süd)"),
            "koepi-cup-2026-halle-sued"
        );
        assert_eq!(slugify("///"), "turnier");
        assert_eq!(slugify(""), "turnier");
        let lang = "x".repeat(200);
        assert!(slugify(&lang).len() <= 60);
    }

    #[test]
    fn has_timeline_is_false_without_recorded_points() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_dir(dir.path());
        assert!(!store.has_timeline(5));
        assert!(store.apply_rally(5, 1, 1, "A", 1, 0));
        assert!(store.has_timeline(5));
    }

    #[test]
    fn guid_lands_in_the_file_header_when_configured() {
        let dir = tempfile::tempdir().unwrap();
        let store = TimelineStore::default();
        store.set_dir(dir.path().to_path_buf());
        store.set_guid("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        store.set_tournament("Test BTS Light");
        assert!(store.apply_sync(5, sync_beispiel()));
        let text = std::fs::read_to_string(dir.path().join("test-bts-light.json")).unwrap();
        assert!(text.contains("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"));
        // Und: keine Spielernamen in der Datei — es gibt schlicht kein
        // Namensfeld. Der Kopf trägt Turniername, Datum, GUID, sonst IDs.
        let file: TimelineFile = serde_json::from_str(&text).unwrap();
        assert_eq!(file.tournament, "Test BTS Light");
    }
}
