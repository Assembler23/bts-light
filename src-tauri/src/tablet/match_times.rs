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

/// **Die** Plausibilitätsregel für alle Dauer-Berechnungen (BTP-`Duration`,
/// Statistik, Ist-Zeiten der Beendet-Liste): ganze Minuten, jenseits von
/// [`MAX_PLAUSIBLE_BRUTTO_MIN`] „unbekannt" (`None`). Eine Stelle statt
/// dreier handgerollter Kopien (Review 2026-08-16, F10).
pub fn plausible_duration_mins(start_ms: u64, end_ms: u64) -> Option<i64> {
    let mins = (end_ms.saturating_sub(start_ms) / 60_000) as i64;
    (mins <= MAX_PLAUSIBLE_BRUTTO_MIN).then_some(mins)
}

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
    /// Halle der **ersten** Feldzuweisung, wie `class_label` mitgestempelt
    /// (ADR 0036). Leer bei Ein-Hallen-Turnieren und in Messwerten, die vor
    /// v0.9.231 entstanden sind — die stehen in der Hallen-Auswertung dann
    /// in einer eigenen Zeile „ohne Halle".
    ///
    /// Bewusst hier und nicht zur Anzeigezeit nachgeschlagen: Sobald BTP
    /// ein beendetes Spiel vom Feld nimmt, ist die Zuordnung
    /// Match → Feld → Halle nicht mehr auflösbar — und die Statistik ist
    /// per Definition Rückschau über beendete Spiele.
    #[serde(default)]
    pub hall: String,
    /// E11: regulär über den Tablet-Pfad beendet (ScoreStatus 0) — nur
    /// solche Spiele liefern Messwerte für die Prognose-Statistik.
    #[serde(default)]
    pub regular: bool,
    /// Zeitpunkt, zu dem das Match **im BTP-Snapshot** erstmals als beendet
    /// auftauchte. Bewusst getrennt von `finished_ms`: Der zählt den
    /// Host-Eingang des Ergebnisses und speist die Prognose-Statistik —
    /// hier steht dagegen auch das Ende eines Spiels, dessen Ergebnis
    /// jemand direkt in BTP eingetragen hat.
    ///
    /// Zweck: die Mindestpause der Spieler und die Endezeit der
    /// Beendet-Liste. Vor v0.9.253 lag dieser Stempel nur im Arbeitsspeicher
    /// der Sync-Engine — nach jedem „Übertragung stoppen/starten" (also bei
    /// jedem Speichern der Einstellungen) und nach jedem App-Neustart galten
    /// deshalb schlagartig **alle** beendeten Spiele als soeben beendet, und
    /// jeder Spieler begann seine Pause von vorn (Feldtest 22.08.2026).
    #[serde(default)]
    pub finished_seen_ms: Option<u64>,
    /// Seit wann steht dieses Spiel nach seinen Sätzen als **entschieden** da,
    /// ohne dass ein Ergebnis angekommen wäre? (Spec
    /// `tl-warnung-fertiges-spiel`.) `None` = läuft normal.
    ///
    /// Bewusst hier und nicht im Arbeitsspeicher: Beim App-Start lädt der
    /// Host die Live-Stände aus `scores.json` zurück. Ein RAM-Merker sähe den
    /// entschiedenen Stand dann sofort wieder als „gerade zuerst gesehen" und
    /// ließe die Uhr neu laufen — die Warnung verschwände ausgerechnet für
    /// eine Minute nach einem Neustart, also genau dann, wenn die
    /// Turnierleitung am dringendsten hinschaut.
    #[serde(default)]
    pub decided_seen_ms: Option<u64>,
    /// Die Pflichtpause (Millisekunden), die **beim Spielende** galt —
    /// zusammen mit `finished_seen_ms` eingefroren. Eine später geänderte
    /// Pausenzeit greift dadurch nur für Spiele, die danach enden; die
    /// schon laufenden Pausen bleiben, wie die Turnierleitung sie beim
    /// Aufruf angekündigt hat. `None` = Altbestand, dann gilt der aktuelle
    /// Wert.
    #[serde(default)]
    pub pause_ms: Option<u64>,
    /// E4-Reset-Zähler: aufeinanderfolgende Snapshots ohne Feld. Reiner
    /// Entprell-Zustand — bewusst NICHT persistiert (`skip`), sonst würde
    /// ein App-Neustart mitten im Flackern die 3-Poll-Garantie brechen.
    #[serde(skip)]
    pub off_court_polls: u32,
    /// Zähler für „BTP führt das Match noch als laufend, obwohl hier ein
    /// Ende gestempelt ist" — nach [`DEASSIGN_CONFIRM_POLLS`] Polls gilt
    /// das Ergebnis als in BTP gelöscht und der Ende-Stempel wird geräumt
    /// (Review 2026-08-16, F3). Entprell-Zustand wie `off_court_polls`.
    #[serde(skip)]
    pub finished_conflict_polls: u32,
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
    /// Zählt bei jeder ECHTEN Stempel-Änderung hoch — der TimeStats-Cache
    /// in `tl::build_state_limited` rechnet nur neu, wenn sich hier etwas
    /// getan hat (Review 2026-08-16, F8), statt je Poll die ganze Map zu
    /// klonen und zu sortieren.
    generation: std::sync::atomic::AtomicU64,
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
                        tracing::warn!("match-times.json bleibt unlesbar – beginne leer");
                        inner.loaded = true;
                    }
                }
            }
            inner.file = MatchTimesFile {
                tournament: name.to_string(),
                ..Default::default()
            };
        }
        self.bump_generation();
        self.persist();
    }

    /// Aktuell gebundenes Turnier (leer, solange keins gebunden ist).
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    /// Je Sync-Poll: Zuweisungen stempeln und Feldabnahmen zählen.
    ///
    /// `assigned` = alle OnCourt-Matches mit Feld als
    /// `(match_id, class_label, discipline, hall)`; `deassigned` = Matches mit
    /// gesetztem Zuweisungs-Stempel, die der Snapshot als `Scheduled`
    /// **ohne** Feld führt (nicht Finished — Beendete zählen nie).
    /// Liefert die in DIESEM Poll frisch gestempelten Match-IDs — der
    /// Sync-Loop vergleicht sie fürs Diagnose-Log mit der zuletzt
    /// publizierten Prognose (Erfolgsmaß E12).
    pub fn reconcile(
        &self,
        assigned: &[(i64, &str, &str, &str)],
        deassigned: &HashSet<i64>,
        now: u64,
    ) -> Vec<i64> {
        // Persistiert wird nur bei STEMPEL-Änderungen — der Abnahme-Zähler
        // ist RAM-Entprellung (siehe `off_court_polls`) und darf weder die
        // Datei je Poll neu schreiben noch je persistiert werden.
        let (stamped, fresh) = {
            let mut inner = self.inner.lock().unwrap();
            let mut stamped = false;
            let mut fresh: Vec<i64> = Vec::new();
            for &(match_id, class_label, discipline, hall) in assigned {
                let e = inner.file.entries.entry(match_id).or_default();
                e.off_court_polls = 0;
                if e.first_assigned_ms.is_none() {
                    e.first_assigned_ms = Some(now);
                    e.class_label = class_label.to_string();
                    e.discipline = discipline.to_string();
                    // Nur im Erststempel-Zweig — die Halle ist damit genauso
                    // immun gegen einen späteren Wechsel wie die Startzeit
                    // (ADR 0036): Gemessen wird, wo das Spiel ANGEFANGEN hat.
                    e.hall = hall.to_string();
                    stamped = true;
                    fresh.push(match_id);
                }
            }
            // Abnahme-Zähler: nur Matches mit Stempel, die der Snapshot
            // ausdrücklich als „Scheduled ohne Feld" führt, zählen hoch.
            // Alles andere (wieder zugewiesen, Finished, verschwunden)
            // setzt zurück — im Zweifel Stempel behalten.
            let assigned_ids: HashSet<i64> = assigned.iter().map(|(id, _, _, _)| *id).collect();
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
            (stamped, fresh)
        };
        if stamped {
            self.bump_generation();
            self.persist();
        }
        fresh
    }

    /// Je Sync-Poll: Ende-Stempel gegen die BTP-Wahrheit halten (Review
    /// 2026-08-16, F3). `on_court` = Match-IDs, die BTP gerade als laufend
    /// führt; `retry_pending` = Ergebnisse, die noch in der Nachschub-Queue
    /// liegen (ADR 0018 — BTP kennt sie nur noch nicht, das ist KEIN
    /// Löschfall). Führt BTP ein Match mit Ende-Stempel über
    /// [`DEASSIGN_CONFIRM_POLLS`] Polls als laufend, wurde das Ergebnis in
    /// BTP gelöscht (das Spiel läuft weiter) — Ende und Einstufung werden
    /// geräumt, damit das ECHTE Ende wieder stempeln kann und keine
    /// vergiftete Messung im Median landet. Bruttostart/Nettostart bleiben.
    pub fn reconcile_finished_conflicts(
        &self,
        on_court: &HashSet<i64>,
        retry_pending: &HashSet<i64>,
    ) {
        let cleared = {
            let mut inner = self.inner.lock().unwrap();
            let mut cleared = false;
            for (id, e) in inner.file.entries.iter_mut() {
                let conflict = (e.finished_ms.is_some() || e.finished_seen_ms.is_some())
                    && on_court.contains(id)
                    && !retry_pending.contains(id);
                if !conflict {
                    e.finished_conflict_polls = 0;
                    continue;
                }
                e.finished_conflict_polls += 1;
                if e.finished_conflict_polls >= DEASSIGN_CONFIRM_POLLS {
                    e.finished_ms = None;
                    e.regular = false;
                    // Der Snapshot-Stempel hängt an derselben Wahrheit und
                    // wird mit derselben Entprellung geräumt — sonst bekäme
                    // das neu angesetzte Spiel beim zweiten Ende den
                    // Zeitpunkt des ersten (und damit eine Pause, die längst
                    // abgelaufen ist).
                    e.finished_seen_ms = None;
                    e.pause_ms = None;
                    e.finished_conflict_polls = 0;
                    cleared = true;
                }
            }
            cleared
        };
        if cleared {
            self.bump_generation();
            self.persist();
        }
    }

    /// Stand der Messwerte — steigt bei jeder echten Stempel-Änderung.
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }

    fn bump_generation(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
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
            self.bump_generation();
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
            self.bump_generation();
            self.persist();
        }
    }

    /// Das im BTP-Snapshot erstmals gesehene Spielende stempeln, samt der
    /// dabei geltenden Pflichtpause — beides nur, solange noch kein Stempel
    /// steht. Liefert den gültigen Stempel zurück (den alten, wenn schon
    /// einer stand), damit der Aufrufer ihn in den Snapshot schreiben kann.
    pub fn stamp_finished_seen(&self, match_id: i64, pause_ms: u64, now: u64) -> (u64, u64) {
        let (wert, changed) = {
            let mut inner = self.inner.lock().unwrap();
            let e = inner.file.entries.entry(match_id).or_default();
            match e.finished_seen_ms {
                Some(alt) => ((alt, e.pause_ms.unwrap_or(pause_ms)), false),
                None => {
                    e.finished_seen_ms = Some(now);
                    e.pause_ms = Some(pause_ms);
                    ((now, pause_ms), true)
                }
            }
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
        wert
    }

    /// Stempelt „seit jetzt sieht dieses Spiel fertig aus" — genau einmal je
    /// Episode. Liefert den geltenden Stempel zurück.
    pub fn stamp_decided_seen(&self, match_id: i64, now: u64) -> u64 {
        let (wert, changed) = {
            let mut inner = self.inner.lock().unwrap();
            let e = inner.file.entries.entry(match_id).or_default();
            match e.decided_seen_ms {
                Some(alt) => (alt, false),
                None => {
                    e.decided_seen_ms = Some(now);
                    (now, true)
                }
            }
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
        wert
    }

    /// Nimmt den Stempel zurück — das Spiel sieht nicht mehr fertig aus (der
    /// Stand wurde korrigiert) oder das Ergebnis ist da. Beim nächsten Ende
    /// beginnt die Uhr von vorn.
    pub fn clear_decided_seen(&self, match_id: i64) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match inner.file.entries.get_mut(&match_id) {
                Some(e) if e.decided_seen_ms.is_some() => {
                    e.decided_seen_ms = None;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
    }

    /// Seit wann sieht dieses Spiel fertig aus? `None` = tut es nicht.
    pub fn decided_seen_ms(&self, match_id: i64) -> Option<u64> {
        self.inner
            .lock()
            .unwrap()
            .file
            .entries
            .get(&match_id)
            .and_then(|e| e.decided_seen_ms)
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

    /// Bruttostart- und Erster-Punkt-Stempel eines Matches mit EINEM Lock
    /// und ohne Entry-Klon (Review 2026-08-17): Der TL-State-Bau liest
    /// beide je belegtem Feld alle ~2 Sekunden — `first_assigned_ms` +
    /// `entry` wären zwei Sperren plus zwei String-Klone pro Lesung.
    pub fn stamps(&self, match_id: i64) -> (Option<u64>, Option<u64>) {
        self.inner
            .lock()
            .unwrap()
            .file
            .entries
            .get(&match_id)
            .map(|e| (e.first_assigned_ms, e.first_point_ms))
            .unwrap_or((None, None))
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        let e = store.entry(7).unwrap();
        assert_eq!(e.first_assigned_ms, Some(1_000));
        assert_eq!(e.class_label, "A");
        assert_eq!(e.discipline, "HE");
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn der_erststempel_haelt_auch_die_halle_fest() {
        // Spec `tl-sicht-feinschliff` A1.7: Die Statistik soll nach Halle
        // auswertbar sein. Die Halle kommt beim Erststempel mit, damit die
        // Auswertung sie später nicht im Snapshot nachschlagen muss — dort
        // ist sie weg, sobald BTP das Feld freigibt (ADR 0036).
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "Halle B")], &keine(), 1_000);
        assert_eq!(store.entry(7).unwrap().hall, "Halle B");
    }

    #[test]
    fn ein_hallenwechsel_aendert_den_hallenstempel_nicht() {
        // A1.7, Kehrseite: Der Stempel ist immun gegen einen späteren
        // Wechsel — wie `first_assigned_ms`, `class_label` und
        // `discipline`. Gemessen wird, wo das Spiel ANGEFANGEN hat.
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "Halle B")], &keine(), 1_000);
        store.reconcile(&[(7, "A", "HE", "Halle A")], &keine(), 5_000);
        assert_eq!(store.entry(7).unwrap().hall, "Halle B");
    }

    #[test]
    fn nach_dem_e4_reset_stempelt_die_neue_halle() {
        // Wird die Zuweisung bestätigt zurückgenommen, fällt der ganze
        // Eintrag (E4-Reset). Eine Neuansetzung misst komplett frisch —
        // also auch mit der Halle, in der sie diesmal stattfindet.
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "Halle B")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        for t in [2_000, 3_000, 4_000, 5_000] {
            store.reconcile(&[], &weg, t);
        }
        assert!(store.entry(7).is_none(), "Eintrag ist geräumt");

        store.reconcile(&[(7, "A", "HE", "Halle A")], &keine(), 9_000);
        assert_eq!(store.entry(7).unwrap().hall, "Halle A");
    }

    #[test]
    fn ein_ein_hallen_turnier_stempelt_eine_leere_halle() {
        // `court_location_name` gibt bei einem Ein-Hallen-Turnier bewusst
        // einen leeren String zurück. Der Messwert trägt ihn genauso —
        // die Hallen-Achse wird dort gar nicht erst angeboten (A1.6).
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        assert_eq!(store.entry(7).unwrap().hall, "");
    }

    #[test]
    fn ein_alter_stand_ohne_halle_bleibt_lesbar() {
        // A1.9: Wer mitten im Turnier aktualisiert, hat eine
        // `match-times.json` ohne `hall`. Sie muss weiter laden — die
        // Messwerte davor tragen dann eben keine Halle (A1.8).
        let alt =
            r#"{"first_assigned_ms":1000,"class_label":"A","discipline":"HE","regular":true}"#;
        let e: MatchTimeEntry = serde_json::from_str(alt).expect("alter Stand bleibt lesbar");
        assert_eq!(e.first_assigned_ms, Some(1_000));
        assert_eq!(e.hall, "", "ohne Angabe bleibt die Halle leer");
    }

    #[test]
    fn ein_feldwechsel_aendert_den_erststempel_nicht() {
        // Feldwechsel: das Match bleibt zugewiesen (anderes Feld ist für
        // den Store unsichtbar — es zählt nur „zugewiesen ja/nein").
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 5_000);
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn ein_app_neustart_aendert_den_erststempel_nicht() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);

        // Neustart: neuer Store, gleiche Datei — der nächste Poll würde
        // mit der Neustart-Zeit stempeln wollen.
        let neu = store_mit_datei(dir.path());
        neu.reconcile(&[(7, "A", "HE", "")], &keine(), 999_000);
        assert_eq!(neu.first_assigned_ms(7), Some(1_000));
    }

    #[test]
    fn ein_bis_zwei_polls_ohne_feld_sind_flackern_und_loeschen_nichts() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.stamp_first_point(7, 2_000);
        store.stamp_finished(7, true, 9_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 10_000);
        store.reconcile(&[], &weg, 11_000);
        store.reconcile(&[], &weg, 12_000);
        assert_eq!(store.entry(7), None, "kompletter Eintrag verworfen");

        // Die Neuansetzung misst frisch — inklusive neuem Ende.
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 20_000);
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 3_000);
        store.reconcile(&[], &weg, 4_000);
        store.reconcile(&[], &weg, 5_000);
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 9_000);
        assert_eq!(store.first_assigned_ms(7), Some(9_000));
    }

    #[test]
    fn ein_wieder_erscheinen_setzt_den_abnahme_zaehler_zurueck() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        let weg: HashSet<i64> = [7].into_iter().collect();
        store.reconcile(&[], &weg, 2_000);
        store.reconcile(&[], &weg, 3_000);
        // Wieder auf dem Feld: Zähler zurück auf 0 …
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 4_000);
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.stamp_first_point(7, 2_000);
        store.stamp_first_point(7, 8_000);
        assert_eq!(store.entry(7).unwrap().first_point_ms, Some(2_000));
    }

    #[test]
    fn das_spielende_stempelt_nur_einmal_und_haelt_regular_fest() {
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.stamp_finished(7, true, 9_000);
        // Eine spätere Korrektur (auch mit anderem regular) ändert nichts.
        store.stamp_finished(7, false, 20_000);
        let e = store.entry(7).unwrap();
        assert_eq!(e.finished_ms, Some(9_000));
        assert!(e.regular);
    }

    #[test]
    fn ein_geloeschtes_ergebnis_raeumt_den_ende_stempel_nach_drei_polls() {
        // Review-Befund 2026-08-16 (F3): Löscht die TL ein irrtümliches
        // Ergebnis in BTP, während das Spiel auf dem Feld WEITERLÄUFT,
        // greift der E4-Reset nie (das Match wird nie „Scheduled ohne
        // Feld") — der stale Ende-Stempel würde die echte End-Duration
        // verfälschen und die vergiftete Messung in den Median tragen.
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.stamp_finished(7, true, 9_000);

        let on_court: HashSet<i64> = [7].into_iter().collect();
        // Zwei Polls „läuft noch in BTP" sind Schwebe (Write unterwegs) …
        store.reconcile_finished_conflicts(&on_court, &keine());
        store.reconcile_finished_conflicts(&on_court, &keine());
        assert_eq!(store.entry(7).unwrap().finished_ms, Some(9_000));
        // … der dritte bestätigt: BTP kennt kein Ergebnis mehr.
        store.reconcile_finished_conflicts(&on_court, &keine());
        let e = store.entry(7).unwrap();
        assert_eq!(e.finished_ms, None, "staler Ende-Stempel geräumt");
        assert!(!e.regular);
        assert_eq!(e.first_assigned_ms, Some(1_000), "Bruttostart bleibt");

        // Das echte Ende stempelt danach wieder frisch.
        store.stamp_finished(7, true, 30_000);
        assert_eq!(store.entry(7).unwrap().finished_ms, Some(30_000));
    }

    #[test]
    fn ein_ergebnis_in_der_nachschub_queue_zaehlt_nicht_als_konflikt() {
        // ADR 0018: Ein Ergebnis kann minutenlang in der Retry-Queue
        // liegen, während BTP das Match noch als laufend führt — das ist
        // KEIN gelöschtes Ergebnis und darf den Stempel nicht räumen.
        let store = MatchTimesStore::default();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.stamp_finished(7, true, 9_000);

        let on_court: HashSet<i64> = [7].into_iter().collect();
        let queued: HashSet<i64> = [7].into_iter().collect();
        for _ in 0..5 {
            store.reconcile_finished_conflicts(&on_court, &queued);
        }
        assert_eq!(store.entry(7).unwrap().finished_ms, Some(9_000));
    }

    #[test]
    fn die_generation_steigt_nur_bei_echten_stempel_aenderungen() {
        // Grundlage des TimeStats-Caches (Review F8): Nur wenn sich an den
        // Messwerten etwas ändert, muss die Statistik neu gerechnet werden.
        let store = MatchTimesStore::default();
        let g0 = store.generation();
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        let g1 = store.generation();
        assert_ne!(g0, g1, "Erststempel ändert die Generation");
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 2_000);
        assert_eq!(store.generation(), g1, "unveränderter Poll nicht");
        store.stamp_finished(7, true, 9_000);
        assert_ne!(store.generation(), g1);
    }

    #[test]
    fn die_plausibilitaets_regel_gilt_ueberall_gleich() {
        assert_eq!(
            plausible_duration_mins(1_000, 1_000 + 40 * 60_000),
            Some(40)
        );
        assert_eq!(plausible_duration_mins(1_000, 1_000), Some(0));
        assert_eq!(
            plausible_duration_mins(0, (MAX_PLAUSIBLE_BRUTTO_MIN as u64) * 60_000),
            Some(MAX_PLAUSIBLE_BRUTTO_MIN)
        );
        assert_eq!(
            plausible_duration_mins(0, (MAX_PLAUSIBLE_BRUTTO_MIN as u64 + 1) * 60_000),
            None,
            "über Nacht geparkt → unbekannt"
        );
    }

    #[test]
    fn der_stand_ueberlebt_einen_app_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
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
        store.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);

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
        vorlage.reconcile(&[(7, "A", "HE", "")], &keine(), 1_000);
        store.set_tournament("Test BTS Light");
        assert_eq!(store.first_assigned_ms(7), Some(1_000));
    }
}
