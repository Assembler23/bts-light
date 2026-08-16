//! Spielzeiten-Messung je Match (Spec `docs/features/spielzeiten-prognose.md`,
//! ADR 0027): erste Feldzuweisung (Bruttostart), erster Punkt (Nettostart)
//! und Spielende — alle **host-seitig** gestempelt, eine Uhr für LAN und
//! Cloud. Grundlage der BTP-`Duration` auf den früheren 0-Pfaden und der
//! Startzeit-Prognose (Etappe B).
//!
//! Enthält ausschließlich Match-IDs, Zeitstempel, Klassen-Label und
//! Disziplin — kein Personendatum.
//!
//! Grundsätze (Muster ADR 0022, wie `queue_order.rs`/`exclusion.rs`):
//! - **Turniergebunden**: eigene Datei `match-times.json` im
//!   App-Datenverzeichnis, im Kopf das Turnier. Turnierwechsel verwirft.
//! - **Der Store ist die Wahrheit** der Zeitmessung; `on_court_since`
//!   (RAM) bleibt reiner Zubringer für den Aufruf-Timer. Deshalb sind die
//!   Stempel gegen Feldwechsel und App-Neustart immun („nur wenn leer").
//! - **E4-Reset**: Nimmt BTP ein Spiel wirklich vom Feld (drei
//!   aufeinanderfolgende Snapshots `Scheduled` ohne Feld, nicht Finished),
//!   werden Zuweisungs- und Punktstempel verworfen — eine spätere
//!   Neuansetzung misst frisch. Poll-basiert statt zeitbasiert, weil
//!   deterministisch testbar; 1–2 Polls (Sync-Flackern) ändern nichts.
//! - **Best effort**: ein Schreibfehler kostet höchstens Messwerte,
//!   nie ein Ergebnis.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Nach wie vielen aufeinanderfolgenden Snapshots „Scheduled ohne Feld"
/// eine Feldabnahme als echt gilt (Muster `EMPTY_CONFIRM_POLLS`, sync.rs).
pub const DEASSIGN_CONFIRM_POLLS: u32 = 3;

/// Plausibilitätsgrenze der Bruttozeit in Minuten (6 h). Ein Spiel, das
/// über Nacht auf dem Feld „geparkt" war (Mehrtages-Turnier), meldet sonst
/// absurde Dauern nach BTP bzw. vergiftet den Prognose-Median — jenseits
/// dieser Grenze gilt die Dauer als unbekannt.
pub const MAX_PLAUSIBLE_BRUTTO_MIN: i64 = 360;

/// Zeiten eines Matches. Alle Stempel Unix-ms, Host-Uhr.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchTimeEntry {
    /// E4: erste Feldzuweisung (Bruttostart) — immun gegen Feldwechsel
    /// und App-Neustart, Reset nur nach bestätigter Abnahme.
    #[serde(default)]
    pub first_assigned_ms: Option<u64>,
    /// E2: erster beim Host eingegangener Punktestand > 0 (Nettostart).
    #[serde(default)]
    pub first_point_ms: Option<u64>,
    /// E3: Host-Eingang des Ergebnisses — wird nie überschrieben
    /// (Korrektur/Wiederholungs-POST ändern nichts).
    #[serde(default)]
    pub finished_ms: Option<u64>,
    /// Klassen-Kürzel des Matches, beim Zuweisungs-Stempel übernommen,
    /// damit die Statistik (Etappe B) ohne Snapshot-Lookup rechnet.
    #[serde(default)]
    pub class_label: String,
    /// Disziplin-Kürzel („HE", „DD" …), wie `class_label` mitgestempelt.
    #[serde(default)]
    pub discipline: String,
    /// E11: regulär über den Tablet-Pfad beendet (ScoreStatus 0) — nur
    /// solche Spiele liefern Messwerte für die Prognose-Statistik.
    #[serde(default)]
    pub regular: bool,
    /// E4-Reset-Zähler: aufeinanderfolgende Snapshots ohne Feld. Reiner
    /// Entprell-Zustand — bewusst NICHT persistiert (`skip`), sonst würde
    /// ein App-Neustart mitten im Flackern die 3-Poll-Garantie brechen.
    #[serde(skip)]
    pub off_court_polls: u32,
}

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MatchTimesFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Match-ID → Zeiten.
    #[serde(default)]
    entries: HashMap<i64, MatchTimeEntry>,
}

/// Ergebnis eines Ladeversuchs — Muster `exclusion.rs::Ladung`.
enum Ladung {
    Stand(MatchTimesFile),
    Leer,
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: MatchTimesFile,
    loaded: bool,
    /// Fehlgeschlagene Ladeversuche (nur Lese-, nicht Parse-Fehler). Nach
    /// [`MAX_LOAD_ATTEMPTS`] beginnt der Store leer — best effort, sonst
    /// bliebe er bei dauerhaft gesperrter Datei für immer ungebunden und
    /// kein Stempel überlebte einen Neustart.
    load_attempts: u32,
}

/// Wie oft ein unlesbarer Bestand geschont wird, bevor leer begonnen wird.
const MAX_LOAD_ATTEMPTS: u32 = 3;

/// Der Spielzeiten-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit Sync-Loop,
/// Ergebnis-Pfade und TL-Web denselben Stand sehen.
#[derive(Default)]
pub struct MatchTimesStore {
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    persist_lock: Mutex<()>,
}

impl MatchTimesStore {
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
                    Ladung::Unlesbar => {
                        inner.load_attempts += 1;
                        if inner.load_attempts < MAX_LOAD_ATTEMPTS {
                            return;
                        }
                        // Genug gewartet — leer beginnen (best effort),
                        // damit die Messung turniergebunden weiterläuft.
                        tracing::warn!(
                            "match-times.json bleibt unlesbar – beginne leer"
                        );
                        inner.loaded = true;
                    }
                }
            }
            inner.file = MatchTimesFile {
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

    /// Je Sync-Poll: Zuweisungen stempeln und Feldabnahmen zählen.
    ///
    /// `assigned` = alle OnCourt-Matches mit Feld als
    /// `(match_id, class_label, discipline)`; `deassigned` = Matches mit
    /// gesetztem Zuweisungs-Stempel, die der Snapshot als `Scheduled`
    /// **ohne** Feld führt (nicht Finished — Beendete zählen nie).
    pub fn reconcile(
        &self,
        assigned: &[(i64, &str, &str)],
        deassigned: &HashSet<i64>,
        now: u64,
    ) {
        // Persistiert wird nur bei STEMPEL-Änderungen — der Abnahme-Zähler
        // ist RAM-Entprellung (siehe `off_court_polls`) und darf weder die
        // Datei je Poll neu schreiben noch je persistiert werden.
        let stamped = {
            let mut inner = self.inner.lock().unwrap();
            let mut stamped = false;
            for &(match_id, class_label, discipline) in assigned {
                let e = inner.file.entries.entry(match_id).or_default();
                e.off_court_polls = 0;
                if e.first_assigned_ms.is_none() {
                    e.first_assigned_ms = Some(now);
                    e.class_label = class_label.to_string();
                    e.discipline = discipline.to_string();
                    stamped = true;
                }
            }
            // Abnahme-Zähler: nur Matches mit Stempel, die der Snapshot
            // ausdrücklich als „Scheduled ohne Feld" führt, zählen hoch.
            // Alles andere (wieder zugewiesen, Finished, verschwunden)
            // setzt zurück — im Zweifel Stempel behalten.
            let assigned_ids: HashSet<i64> = assigned.iter().map(|(id, _, _)| *id).collect();
            let mut verworfen: Vec<i64> = Vec::new();
            for (id, e) in inner.file.entries.iter_mut() {
                if e.first_assigned_ms.is_none() || assigned_ids.contains(id) {
                    continue;
                }
                if deassigned.contains(id) {
                    e.off_court_polls += 1;
                    if e.off_court_polls >= DEASSIGN_CONFIRM_POLLS {
                        verworfen.push(*id);
                    }
                } else {
                    e.off_court_polls = 0;
                }
            }
            for id in verworfen {
                // Bestätigte Feldabnahme (E4-Reset): der GANZE Eintrag
                // fällt — auch Ende und Einstufung. Ein in BTP
                // zurückgesetztes Spiel misst bei Neuansetzung komplett
                // frisch; ein halb geräumter Eintrag könnte nie wieder
                // ein korrektes Ende stempeln (Review 2026-08-16).
                inner.file.entries.remove(&id);
                stamped = true;
            }
            stamped
        };
        if stamped {
            self.persist();
        }
    }

    /// E2: ersten Punkt stempeln (nur wenn noch keiner steht).
    pub fn stamp_first_point(&self, match_id: i64, now: u64) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let e = inner.file.entries.entry(match_id).or_default();
            if e.first_point_ms.is_some() {
                false
            } else {
                e.first_point_ms = Some(now);
                true
            }
        };
        if changed {
            self.persist();
        }
    }

    /// E3/E11: Spielende stempeln (nur wenn noch keins steht). `regular`
    /// wird nur beim Erst-Stempel übernommen — eine Korrektur oder ein
    /// Wiederholungs-POST ändert weder Zeit noch Einstufung.
    pub fn stamp_finished(&self, match_id: i64, regular: bool, now: u64) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let e = inner.file.entries.entry(match_id).or_default();
            if e.finished_ms.is_some() {
                false
            } else {
                e.finished_ms = Some(now);
                e.regular = regular;
                true
            }
        };
        if changed {
            self.persist();
        }
    }

    /// Bruttostart eines Matches (E4), falls gestempelt.
    pub fn first_assigned_ms(&self, match_id: i64) -> Option<u64> {
        self.inner
            .lock()
            .unwrap()
            .file
            .entries
            .get(&match_id)
            .and_then(|e| e.first_assigned_ms)
    }

    /// Alle Zeiteinträge (Kopie) — Rohmaterial der Statistik
    /// (`predict::time_stats`). Klein genug zum Klonen: ein Turnier hat
    /// wenige hundert Matches.
    pub fn entries(&self) -> HashMap<i64, MatchTimeEntry> {
        self.inner.lock().unwrap().file.entries.clone()
    }

    /// Kompletter Zeiteintrag eines Matches (für Anzeige und Tests).
    pub fn entry(&self, match_id: i64) -> Option<MatchTimeEntry> {
        self.inner
            .lock()
            .unwrap()
            .file
            .entries
            .get(&match_id)
            .cloned()
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
                tracing::warn!("match-times.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<MatchTimesFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("match-times.json unlesbar – beginne leer");
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

    fn store_mit_datei(dir: &Path) -> MatchTimesStore {
        let store = MatchTimesStore::default();
        store.set_path(dir.join("match-times.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    fn keine() -> HashSet<i64> {
        HashSet::new()
    }

    #[test]
    fn der_erststempel_setzt_zuweisung_klasse_und_disziplin() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        let e = store.entry(7).unwrap();
        assert_eq!(e.first_assigned_ms, Some(1_000));
        assert_eq!(e.class_label, "A");
        assert_eq!(e.discipline, "HE");
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn ein_feldwechsel_aendert_den_erststempel_nicht() {
        // Feldwechsel: das Match bleibt zugewiesen (anderes Feld ist für
        // den Store unsichtbar — es zählt nur „zugewiesen ja/nein").
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.reconcile(&[(7, "A", "HE")], &keine(), 5_000);
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn ein_app_neustart_aendert_den_erststempel_nicht() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);

        // Neustart: neuer Store, gleiche Datei — der nächste Poll würde
        // mit der Neustart-Zeit stempeln wollen.
        let neu = store_mit_datei(dir.path());
        neu.reconcile(&[(7, "A", "HE")], &keine(), 999_000);
        assert_eq!(neu.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn ein_bis_zwei_polls_ohne_feld_sind_flackern_und_loeschen_nichts() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.stamp_first_point(7, 2_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 3_000);
        store.reconcile(&[], &weg, 4_000);
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
        assert_eq!(store.entry(7).unwrap().first_point_ms, Some(2_000));
    }

    #[test]
    fn drei_polls_ohne_feld_verwerfen_den_ganzen_eintrag() {
        // Review-Befund 2026-08-16: Der Reset muss auch `finished_ms` und
        // `regular` räumen — sonst kann ein irrtümlich gewertetes, in BTP
        // zurückgesetztes Spiel nie wieder ein korrektes Ende stempeln und
        // vergiftet mit negativer Dauer den Median.
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.stamp_first_point(7, 2_000);
        store.stamp_finished(7, true, 9_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 10_000);
        store.reconcile(&[], &weg, 11_000);
        store.reconcile(&[], &weg, 12_000);
        assert_eq!(store.entry(7), None, "kompletter Eintrag verworfen");

        // Die Neuansetzung misst frisch — inklusive neuem Ende.
        store.reconcile(&[(7, "A", "HE")], &keine(), 20_000);
        store.stamp_finished(7, true, 50_000);
        let e = store.entry(7).unwrap();
        assert_eq!(e.first_assigned_ms, Some(20_000));
        assert_eq!(e.finished_ms, Some(50_000));
    }

    #[test]
    fn ein_neustart_mitten_im_flackern_loescht_keinen_stempel() {
        // Review-Befund 2026-08-16: Der Abnahme-Zähler ist Entprell-Zustand
        // und darf NICHT persistiert werden — sonst reicht nach einem
        // App-Neustart ein einziger Flacker-Poll für den Reset.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 2_000);
        store.reconcile(&[], &weg, 3_000);

        // Neustart: der Zähler beginnt wieder bei 0 …
        let neu = store_mit_datei(dir.path());
        neu.reconcile(&[], &weg, 4_000);
        neu.reconcile(&[], &weg, 5_000);
        assert_eq!(neu.first_assigned_ms(7), Some(1_000), "2 Polls reichen nie");
        // … erst drei volle Polls nach dem Neustart räumen.
        neu.reconcile(&[], &weg, 6_000);
        assert_eq!(neu.entry(7), None);
    }

    #[test]
    fn eine_dauerhaft_unlesbare_datei_blockiert_die_messung_nicht_ewig() {
        // Review-Befund 2026-08-16: Bleibt die Datei unlesbar (Virenscanner,
        // Sync-Client), darf der Store nicht auf ewig ungebunden bleiben —
        // nach einer begrenzten Zahl Versuche beginnt er leer (best effort),
        // damit Stempel wieder turniergebunden (und persistierbar) sind.
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("match-times.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = MatchTimesStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Cup A");
        store.set_tournament("Cup A");
        assert_eq!(store.tournament(), "", "erste Versuche warten ab");
        store.set_tournament("Cup A");
        assert_eq!(
            store.tournament(),
            "Cup A",
            "nach dem letzten Versuch beginnt der Store leer"
        );
        assert!(pfad.is_dir(), "der vorhandene Stand wird nicht gelöscht");
    }

    #[test]
    fn nach_dem_reset_stempelt_eine_erneute_zuweisung_frisch() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 3_000);
        store.reconcile(&[], &weg, 4_000);
        store.reconcile(&[], &weg, 5_000);
        store.reconcile(&[(7, "A", "HE")], &keine(), 9_000);
        assert_eq!(store.first_assigned_ms(7), Some(9_000));
    }

    #[test]
    fn ein_wieder_erscheinen_setzt_den_abnahme_zaehler_zurueck() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 2_000);
        store.reconcile(&[], &weg, 3_000);
        // Wieder auf dem Feld: Zähler zurück auf 0 …
        store.reconcile(&[(7, "A", "HE")], &keine(), 4_000);
        // … zwei weitere Polls ohne Feld reichen dann nicht für den Reset.
        store.reconcile(&[], &weg, 5_000);
        store.reconcile(&[], &weg, 6_000);
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn ein_match_das_weder_zugewiesen_noch_abgenommen_ist_zaehlt_nicht_hoch() {
        // Finished-Matches (und aus dem Snapshot verschwundene) stehen in
        // KEINEM der beiden Sets — der Zähler fällt auf 0 zurück, der
        // Stempel bleibt.
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 2_000);
        store.reconcile(&[], &weg, 3_000);
        store.reconcile(&[], &keine(), 4_000); // z. B. Finished
        store.reconcile(&[], &weg, 5_000);
        store.reconcile(&[], &weg, 6_000);
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn der_erste_punktestand_stempelt_nur_einmal() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.stamp_first_point(7, 2_000);
        store.stamp_first_point(7, 8_000);
        assert_eq!(store.entry(7).unwrap().first_point_ms, Some(2_000));
    }

    #[test]
    fn das_spielende_stempelt_nur_einmal_und_haelt_regular_fest() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.stamp_finished(7, true, 9_000);
        // Eine spätere Korrektur (auch mit anderem regular) ändert nichts.
        store.stamp_finished(7, false, 20_000);
        let e = store.entry(7).unwrap();
        assert_eq!(e.finished_ms, Some(9_000));
        assert!(e.regular);
    }

    #[test]
    fn der_stand_ueberlebt_einen_app_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.stamp_first_point(7, 2_000);
        store.stamp_finished(7, true, 9_000);

        let neu = store_mit_datei(dir.path());
        let e = neu.entry(7).unwrap();
        assert_eq!(e.first_assigned_ms, Some(1_000));
        assert_eq!(e.first_point_ms, Some(2_000));
        assert_eq!(e.finished_ms, Some(9_000));
        assert!(e.regular);
        assert_eq!(e.class_label, "A");
    }

    #[test]
    fn ein_turnierwechsel_verwirft_den_stand() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reconcile(&[(7, "A", "HE")], &keine(), 1_000);

        store.set_tournament("Ganz anderes Turnier");
        assert_eq!(store.entry(7), None);
        assert_eq!(store.tournament(), "Ganz anderes Turnier");

        // Auch nach Neustart bleibt der alte Stand weg.
        let neu = MatchTimesStore::default();
        neu.set_path(dir.path().join("match-times.json"));
        neu.set_tournament("Test BTS Light");
        assert_eq!(neu.entry(7), None);
    }

    #[test]
    fn eine_voruebergehend_unlesbare_datei_wird_nicht_ueberschrieben() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("match-times.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = MatchTimesStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Cup A");
        assert_eq!(store.tournament(), "");
        assert!(pfad.is_dir(), "der vorhandene Stand bleibt unangetastet");
        assert!(!pfad.with_extension("json.tmp").exists());

        std::fs::remove_dir(&pfad).unwrap();
        let vorlage = store_mit_datei(dir.path());
        vorlage.reconcile(&[(7, "A", "HE")], &keine(), 1_000);
        store.set_tournament("Test BTS Light");
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }
}
