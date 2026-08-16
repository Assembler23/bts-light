//! Automatische Hallen-Vorverteilung (Spec
//! `docs/features/hallen-vorverteilung.md`, ADR 0029/0030): hält die von
//! der Automatik gesetzten Hallen je Match (turniergebunden, Datei
//! `auto-halls.json`) und rechnet die Verteilung selbst — beides in einem
//! Modul, weil die Verteil-Funktion ohne den Store nie aufgerufen wird.
//!
//! Grundsätze (Muster ADR 0022, wie `match_times.rs`):
//! - **Turniergebunden**: Turnier-Kopf, Wechsel verwirft den Stand.
//! - **Insert-only** (B2 „fest = fest"): [`AutoHallStore::insert_many`]
//!   überschreibt nie. Damit ist der Verteil-Lauf von selbst idempotent —
//!   gleiche Eingaben erzeugen keine Änderung, keine Persistenz und keine
//!   TL-Revision (bewusst KEIN Fingerprint-Mechanismus, ADR 0029).
//! - **Kein Personendatum**: nur Match-IDs und Hallennamen.
//! - Aufgeräumt wird vom Sync-Loop (`reconcile_auto_halls`), beim
//!   Vorbereitungs-Aufruf (E3), beim Hand-Eingriff (`SetHall`) und über
//!   den Massen-Rücknahme-Knopf (E10) — dieses Modul kennt selbst keinen
//!   Snapshot.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AutoHallFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Match-ID → Hallenname (in BTP-Schreibweise, kanonisiert).
    #[serde(default)]
    entries: HashMap<i64, String>,
}

/// Ergebnis eines Ladeversuchs — Muster `match_times.rs::Ladung`.
enum Ladung {
    Stand(AutoHallFile),
    Leer,
    Unlesbar,
}

/// Wie oft ein unlesbarer Bestand geschont wird, bevor leer begonnen wird
/// (Muster `match_times.rs`).
const MAX_LOAD_ATTEMPTS: u32 = 3;

#[derive(Default)]
struct Inner {
    file: AutoHallFile,
    loaded: bool,
    load_attempts: u32,
}

/// Der Speicher der automatisch verteilten Hallen. Lebt im
/// [`TabletState`](super::state::TabletState), damit Sync-Loop, TL-Web und
/// Desktop denselben Stand sehen.
#[derive(Default)]
pub struct AutoHallStore {
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    persist_lock: Mutex<()>,
    /// Steigt bei jeder echten Änderung — für Anzeige-Caches und Tests.
    generation: std::sync::atomic::AtomicU64,
}

impl AutoHallStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Poll).
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
                        tracing::warn!("auto-halls.json bleibt unlesbar – beginne leer");
                        inner.loaded = true;
                    }
                }
            }
            inner.file = AutoHallFile {
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

    /// Automatisch gesetzte Halle eines Matches.
    pub fn hall(&self, match_id: i64) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .file
            .entries
            .get(&match_id)
            .cloned()
    }

    /// Alle Auto-Zuordnungen (Kopie) — Zubringer für die Kaskade
    /// (`assign::hall_for_match`), Muster `manual_halls()`.
    pub fn halls(&self) -> HashMap<i64, String> {
        self.inner.lock().unwrap().file.entries.clone()
    }

    /// Neue Zuordnungen eintragen — **Insert-only** (B2): vorhandene
    /// Einträge werden nie überschrieben. Liefert `true` bei Änderung.
    pub fn insert_many(&self, pairs: &[(i64, String)]) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for (id, hall) in pairs {
                if hall.trim().is_empty() {
                    continue;
                }
                if let std::collections::hash_map::Entry::Vacant(e) = inner.file.entries.entry(*id)
                {
                    e.insert(hall.trim().to_string());
                    changed = true;
                }
            }
            changed
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
        changed
    }

    /// Eine Auto-Zuordnung entfernen (E3 Vorbereitungs-Aufruf, Hand-
    /// Eingriff via `SetHall`). Liefert `true`, wenn es sie gab.
    pub fn remove(&self, match_id: i64) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            inner.file.entries.remove(&match_id).is_some()
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
        changed
    }

    /// Nur Matches aus `keep` behalten (Aufräumen je Sync-Poll: vergeben,
    /// beendet, verschwunden, Paarung offen). Liefert `true` bei Änderung.
    pub fn retain(&self, keep: &HashSet<i64>) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let before = inner.file.entries.len();
            inner.file.entries.retain(|id, _| keep.contains(id));
            inner.file.entries.len() != before
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
        changed
    }

    /// Zuordnungen auf Hallen außerhalb von `valid` verwerfen (E12:
    /// Halle existiert nicht mehr / hat keine Felder). Liefert die
    /// betroffenen Match-IDs — sie werden im selben Lauf neu verteilt.
    /// Vergleich ohne Groß-/Kleinschreibung wie die Vergabe.
    pub fn remove_where_hall_not_in(&self, valid: &HashSet<String>) -> Vec<i64> {
        let lower: HashSet<String> = valid.iter().map(|v| v.trim().to_lowercase()).collect();
        let removed = {
            let mut inner = self.inner.lock().unwrap();
            let ids: Vec<i64> = inner
                .file
                .entries
                .iter()
                .filter(|(_, hall)| !lower.contains(&hall.trim().to_lowercase()))
                .map(|(id, _)| *id)
                .collect();
            for id in &ids {
                inner.file.entries.remove(id);
            }
            ids
        };
        if !removed.is_empty() {
            self.bump_generation();
            self.persist();
        }
        removed
    }

    /// Alle Auto-Zuordnungen auf einmal verwerfen (E10, Massen-Rücknahme).
    pub fn clear_all(&self) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            if inner.file.entries.is_empty() {
                false
            } else {
                inner.file.entries.clear();
                true
            }
        };
        if changed {
            self.bump_generation();
            self.persist();
        }
        changed
    }

    /// Stand der Zuordnungen — steigt bei jeder echten Änderung.
    pub fn generation(&self) -> u64 {
        self.generation.load(std::sync::atomic::Ordering::Acquire)
    }

    fn bump_generation(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
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
                tracing::warn!("auto-halls.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<AutoHallFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("auto-halls.json unlesbar – beginne leer");
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

/// Wirksame Fenstergröße (B4): `window == 0` heißt „automatisch" =
/// Gesamtzahl der Spielfelder; geklemmt auf 1..=120 (Wartelisten-Limit,
/// `tl::QUEUE_LIMIT`). Geteilt von Sync-Loop und TL-Anzeige.
pub fn effective_window(configured: u32, total_courts: usize) -> usize {
    let basis = if configured == 0 {
        total_courts
    } else {
        configured as usize
    };
    basis.clamp(1, 120)
}

/// Ein Platz im Vorverteilungs-Fenster (die vordersten x Spiele der
/// globalen Warteliste), wie ihn der Sync-Loop aufbereitet.
#[derive(Debug, Clone)]
pub struct PrefillSlot {
    pub match_id: i64,
    /// Bereits aufgelöste Halle (egal welcher Herkunft, kanonisiert);
    /// `None` = ohne Halle — nur solche Plätze füllt die Automatik.
    pub hall: Option<String>,
    /// Gerufene zählen auf die Quote, bekommen aber nie einen Eintrag (E8).
    pub called: bool,
}

/// Verteilt die Lücken des Fensters auf die Hallen (Spec
/// `hallen-vorverteilung`, E6/B5) — **rein und deterministisch**.
///
/// `halls`: `(Name, entsperrte Feldzahl)` in BTP-Locations-Reihenfolge
/// (deterministischer Gleichstands-Brecher). Hallen mit 0 Feldern gehen
/// leer aus (A4/E11).
///
/// Verfahren: (1) Quoten je Halle über die Fenstergröße nach **größtem
/// Rest** aus den Feldzahlen. (2) Bereits zugeordnete Plätze (auch
/// gerufene) werden angerechnet; Überschuss erschöpft nur die Quote der
/// eigenen Halle. (3) Lücken in Listenreihenfolge per
/// **Höchstzahl-Verfahren** (`felder/(vergeben+1)`, Gleichstand →
/// Hallen-Reihenfolge) — das ergibt das gemischte Muster (2:1 ⇒
/// A, A, B, A, A, B) statt Blöcken. Sind alle Quoten erschöpft (massiver
/// Überschuss), füllt das Höchstzahl-Verfahren ohne Deckel weiter, damit
/// keine Lücke offen bleibt.
///
/// Rückgabe: nur die NEUEN Paare für vorher leere, nicht gerufene Plätze.
/// Zusammen mit dem Insert-only-Store ist der Lauf idempotent (ADR 0029).
pub fn distribute(window: &[PrefillSlot], halls: &[(String, usize)]) -> Vec<(i64, String)> {
    let n = window.len();
    let aktive: Vec<(&str, usize)> = halls
        .iter()
        .filter(|(_, felder)| *felder > 0)
        .map(|(h, felder)| (h.as_str(), *felder))
        .collect();
    if n == 0 || aktive.is_empty() {
        return Vec::new();
    }
    let total: usize = aktive.iter().map(|(_, f)| f).sum();

    // (1) Quoten nach größtem Rest über die Fenstergröße.
    let mut quota: Vec<usize> = aktive.iter().map(|(_, f)| n * f / total).collect();
    let mut rests: Vec<(usize, usize)> = aktive
        .iter()
        .enumerate()
        .map(|(i, (_, f))| (i, (n * f) % total))
        .collect();
    // Größter Rest zuerst; Gleichstand → Hallen-Reihenfolge.
    rests.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut uebrig = n - quota.iter().sum::<usize>();
    for (i, _) in rests {
        if uebrig == 0 {
            break;
        }
        quota[i] += 1;
        uebrig -= 1;
    }

    // (2) Anrechnung vorhandener Zuordnungen (Vergleich wie die Vergabe:
    // ohne Groß-/Kleinschreibung). Überschuss erschöpft nur die eigene
    // Quote — `assigned` darf über `quota` hinauswachsen.
    let index_of = |hall: &str| {
        aktive
            .iter()
            .position(|(h, _)| h.trim().eq_ignore_ascii_case(hall.trim()))
    };
    let mut assigned: Vec<usize> = vec![0; aktive.len()];
    for s in window {
        if let Some(h) = s.hall.as_deref() {
            if let Some(i) = index_of(h) {
                assigned[i] += 1;
            }
        }
    }

    // (3) Lücken in Listenreihenfolge per Höchstzahl-Verfahren — erst
    // unterhalb der Quoten, bei Total-Erschöpfung (massiver Überschuss)
    // ohne Deckel, damit keine Lücke offen bleibt.
    let mut out = Vec::new();
    for s in window {
        if s.hall.is_some() || s.called {
            continue;
        }
        let pick = hoechstzahl(&aktive, &assigned, Some(&quota))
            .or_else(|| hoechstzahl(&aktive, &assigned, None));
        let Some(i) = pick else { break };
        out.push((s.match_id, aktive[i].0.to_string()));
        assigned[i] += 1;
    }
    out
}

/// Halle mit dem größten Höchstzahl-Wert `felder / (vergeben + 1)` —
/// Gleichstand entscheidet die Listenreihenfolge. Mit `quota` werden
/// erschöpfte Hallen übersprungen. Bruchfrei per Kreuzmultiplikation.
fn hoechstzahl(
    aktive: &[(&str, usize)],
    assigned: &[usize],
    quota: Option<&[usize]>,
) -> Option<usize> {
    let mut best: Option<usize> = None;
    for i in 0..aktive.len() {
        if let Some(q) = quota {
            if assigned[i] >= q[i] {
                continue;
            }
        }
        match best {
            None => best = Some(i),
            Some(b) => {
                let links = aktive[i].1 * (assigned[b] + 1);
                let rechts = aktive[b].1 * (assigned[i] + 1);
                if links > rechts {
                    best = Some(i);
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn slot(id: i64, hall: Option<&str>, called: bool) -> PrefillSlot {
        PrefillSlot {
            match_id: id,
            hall: hall.map(str::to_string),
            called,
        }
    }

    fn halls(spec: &[(&str, usize)]) -> Vec<(String, usize)> {
        spec.iter().map(|(h, f)| (h.to_string(), *f)).collect()
    }

    fn zuteilung(neu: &[(i64, String)], id: i64) -> Option<&str> {
        neu.iter().find(|(m, _)| *m == id).map(|(_, h)| h.as_str())
    }

    // ── Verteil-Funktion (E6/B5) ────────────────────────────────────────

    #[test]
    fn das_misch_muster_verzahnt_die_hallen() {
        // B5-Anker (Nutzer-Beispiel): 2:1 Felder → A, A, B, A, A, B —
        // keine Blöcke.
        let window: Vec<PrefillSlot> = (1..=6).map(|i| slot(i, None, false)).collect();
        let neu = distribute(&window, &halls(&[("Halle A", 2), ("Halle B", 1)]));
        let muster: Vec<&str> = (1..=6).map(|i| zuteilung(&neu, i).unwrap()).collect();
        assert_eq!(
            muster,
            vec!["Halle A", "Halle A", "Halle B", "Halle A", "Halle A", "Halle B"]
        );
    }

    #[test]
    fn die_quoten_folgen_dem_groessten_rest() {
        // 5:4:3 Felder, 10 Plätze → Quoten 4/3/3 (größter Rest: C bekommt
        // den letzten Sitz, nicht die größte Halle).
        let window: Vec<PrefillSlot> = (1..=10).map(|i| slot(i, None, false)).collect();
        let neu = distribute(&window, &halls(&[("A", 5), ("B", 4), ("C", 3)]));
        let count = |h: &str| neu.iter().filter(|(_, x)| x == h).count();
        assert_eq!(count("A"), 4);
        assert_eq!(count("B"), 3);
        assert_eq!(count("C"), 3);
    }

    #[test]
    fn vorhandene_hallen_werden_angerechnet() {
        // B1: Regel-/Hand-/Aufruf-Hallen im Fenster zählen auf die Quote —
        // 2:1 über 6 Plätze, 2 davon schon in A ⇒ nur noch 2×A und 2×B neu.
        let window = vec![
            slot(1, Some("Halle A"), false),
            slot(2, None, false),
            slot(3, Some("Halle A"), false),
            slot(4, None, false),
            slot(5, None, false),
            slot(6, None, false),
        ];
        let neu = distribute(&window, &halls(&[("Halle A", 2), ("Halle B", 1)]));
        assert_eq!(neu.len(), 4);
        let count = |h: &str| neu.iter().filter(|(_, x)| x == h).count();
        assert_eq!(count("Halle A"), 2);
        assert_eq!(count("Halle B"), 2);
    }

    #[test]
    fn ueberschuss_einer_halle_nimmt_anderen_nichts_weg() {
        // 4 von 6 Plätzen schon in B (weit über Bs Quote von 2): Die zwei
        // Lücken gehen beide nach A — Bs Überschuss erschöpft nur B.
        let window = vec![
            slot(1, Some("Halle B"), false),
            slot(2, Some("Halle B"), false),
            slot(3, Some("Halle B"), false),
            slot(4, Some("Halle B"), false),
            slot(5, None, false),
            slot(6, None, false),
        ];
        let neu = distribute(&window, &halls(&[("Halle A", 2), ("Halle B", 1)]));
        assert_eq!(neu.len(), 2);
        assert_eq!(zuteilung(&neu, 5), Some("Halle A"));
        assert_eq!(zuteilung(&neu, 6), Some("Halle A"));
    }

    #[test]
    fn gerufene_zaehlen_bekommen_aber_nichts() {
        // E8: Ein gerufenes Spiel mit Halle belegt seine Quote; ein
        // gerufenes ohne Halle bleibt unangetastet (der Aufruf regelt es).
        let window = vec![
            slot(1, Some("Halle A"), true),
            slot(2, None, true),
            slot(3, None, false),
        ];
        let neu = distribute(&window, &halls(&[("Halle A", 1), ("Halle B", 1)]));
        assert_eq!(zuteilung(&neu, 1), None);
        assert_eq!(zuteilung(&neu, 2), None, "gerufen ohne Halle: kein Eintrag");
        assert_eq!(
            zuteilung(&neu, 3),
            Some("Halle B"),
            "A ist durch Slot 1 belegt"
        );
    }

    #[test]
    fn eine_halle_ohne_entsperrte_felder_geht_leer_aus() {
        let window: Vec<PrefillSlot> = (1..=4).map(|i| slot(i, None, false)).collect();
        let neu = distribute(&window, &halls(&[("Halle A", 2), ("Halle B", 0)]));
        assert!(neu.iter().all(|(_, h)| h == "Halle A"));
        assert_eq!(neu.len(), 4);
    }

    #[test]
    fn der_lauf_ist_idempotent() {
        // ADR 0029: Ergebnis einspielen ⇒ zweiter Lauf liefert nichts.
        let mut window: Vec<PrefillSlot> = (1..=6).map(|i| slot(i, None, false)).collect();
        let neu = distribute(&window, &halls(&[("Halle A", 2), ("Halle B", 1)]));
        for (id, hall) in &neu {
            window[(*id - 1) as usize].hall = Some(hall.clone());
        }
        let zweiter = distribute(&window, &halls(&[("Halle A", 2), ("Halle B", 1)]));
        assert!(zweiter.is_empty());
    }

    #[test]
    fn gleichstand_entscheidet_die_hallen_reihenfolge() {
        // 1:1 Felder, 2 Plätze → erst A (Listenreihenfolge), dann B.
        let window: Vec<PrefillSlot> = (1..=2).map(|i| slot(i, None, false)).collect();
        let neu = distribute(&window, &halls(&[("Halle A", 1), ("Halle B", 1)]));
        assert_eq!(zuteilung(&neu, 1), Some("Halle A"));
        assert_eq!(zuteilung(&neu, 2), Some("Halle B"));
    }

    fn store_mit_datei(dir: &Path) -> AutoHallStore {
        let store = AutoHallStore::default();
        store.set_path(dir.join("auto-halls.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    #[test]
    fn insert_only_ueberschreibt_nie() {
        // B2 „fest = fest": Die Automatik zieht nie um — und genau dieses
        // Verhalten macht den Verteil-Lauf idempotent (ADR 0029).
        let store = AutoHallStore::default();
        assert!(store.insert_many(&[(7, "Halle A".into())]));
        assert!(!store.insert_many(&[(7, "Halle B".into())]));
        assert_eq!(store.hall(7), Some("Halle A".to_string()));
        // Leere Namen werden ignoriert.
        assert!(!store.insert_many(&[(8, "  ".into())]));
        assert_eq!(store.hall(8), None);
    }

    #[test]
    fn die_generation_steigt_nur_bei_echten_aenderungen() {
        let store = AutoHallStore::default();
        let g0 = store.generation();
        store.insert_many(&[(7, "Halle A".into())]);
        let g1 = store.generation();
        assert_ne!(g0, g1);
        store.insert_many(&[(7, "Halle A".into())]); // No-Op
        assert_eq!(store.generation(), g1);
    }

    #[test]
    fn remove_und_clear_raeumen_nur_autos() {
        let store = AutoHallStore::default();
        store.insert_many(&[(7, "Halle A".into()), (8, "Halle B".into())]);
        assert!(store.remove(7));
        assert!(!store.remove(7));
        assert_eq!(store.hall(8), Some("Halle B".to_string()));
        assert!(store.clear_all());
        assert!(!store.clear_all());
        assert_eq!(store.hall(8), None);
    }

    #[test]
    fn retain_entfernt_verschwundene_spiele() {
        let store = AutoHallStore::default();
        store.insert_many(&[(7, "Halle A".into()), (8, "Halle B".into())]);
        let keep: HashSet<i64> = [8].into_iter().collect();
        assert!(store.retain(&keep));
        assert_eq!(store.hall(7), None);
        assert!(!store.retain(&keep));
    }

    #[test]
    fn zuordnungen_auf_verschwundene_hallen_fliegen_raus() {
        // E12: Halle ohne Felder → Eintrag weg, das Spiel wird im selben
        // Lauf neu verteilt. Vergleich case-insensitiv wie die Vergabe.
        let store = AutoHallStore::default();
        store.insert_many(&[(7, "Halle A".into()), (8, "Halle B".into())]);
        let valid: HashSet<String> = ["halle a".to_string()].into_iter().collect();
        let removed = store.remove_where_hall_not_in(&valid);
        assert_eq!(removed, vec![8]);
        assert_eq!(store.hall(7), Some("Halle A".to_string()));
    }

    #[test]
    fn der_stand_ueberlebt_einen_app_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.insert_many(&[(7, "Halle A".into())]);

        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.hall(7), Some("Halle A".to_string()));
    }

    #[test]
    fn ein_turnierwechsel_verwirft_den_stand() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.insert_many(&[(7, "Halle A".into())]);

        store.set_tournament("Ganz anderes Turnier");
        assert_eq!(store.hall(7), None);
        assert_eq!(store.tournament(), "Ganz anderes Turnier");

        let neu = AutoHallStore::default();
        neu.set_path(dir.path().join("auto-halls.json"));
        neu.set_tournament("Test BTS Light");
        assert_eq!(neu.hall(7), None);
    }

    #[test]
    fn eine_dauerhaft_unlesbare_datei_blockiert_nicht_ewig() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("auto-halls.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = AutoHallStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Cup A");
        store.set_tournament("Cup A");
        assert_eq!(store.tournament(), "", "erste Versuche warten ab");
        store.set_tournament("Cup A");
        assert_eq!(store.tournament(), "Cup A", "dann leer beginnen");
        assert!(pfad.is_dir(), "der vorhandene Stand bleibt unangetastet");
    }
}
