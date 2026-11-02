//! Meetingpoint-Zustand: Vorbereitungs-Aufrufe, turniergebunden auf der
//! Platte (Spec `docs/features/meetingpoint-spielbereit.md`, ADR 0071).
//!
//! Bis hierher lebten die „in Vorbereitung"-Aufrufe nur im Arbeitsspeicher:
//! Nach jedem App-Neustart — also auch nach jedem Update im Turnierbetrieb —
//! waren sie weg, und das BTP-Gelb (P1) blieb verwaist stehen. Der Meetingpoint
//! hängt künftig die Check-ins der Beteiligten an den Aufruf; beides muss
//! einen Neustart überleben.
//!
//! **Lebenszyklus eines Aufrufs** (Spec, Entscheidung 4):
//! - Neuaufruf in **dieselbe** Halle behält den ersten Aufrufzeitpunkt (die
//!   Uhr für Fehlende läuft weiter); Neuaufruf in eine **andere** Halle ersetzt
//!   den Aufruf samt Zeitpunkt.
//! - Kommt das Spiel aufs Feld, wird der Aufruf **geparkt**, nicht gelöscht.
//!   Führt BTP es danach [`DEASSIGN_CONFIRM_POLLS`] Abrufe in Folge wieder als
//!   geplant (= vom Feld genommen), lebt er wieder auf — egal auf welchem Weg
//!   das Spiel vom Feld kam (Desktop, TL-Web, BTP). Ein einzelnes Flackern
//!   reaktiviert nicht, sonst schriebe der P1-Abgleich zweimal nach BTP.
//! - Spielende, verschwundenes Match oder fehlende Paarung löschen ihn.
//!
//! Grundsätze (Muster ADR 0022, wie `wish_court.rs`): turniergebunden,
//! außerhalb der `AppConfig`, best effort, unlesbare Datei → leerer Stand.
//! Kein Rückschreiben nach BTP (R2).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::btp::model::{BtpSnapshot, MatchStatus};
use crate::tablet::match_times::DEASSIGN_CONFIRM_POLLS;
use crate::tablet::state::PreparationCall;

/// Phase eines Aufrufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Gerufen und wartend — zählt als Aufruf (Anzeige, Ansage, P1-Gelb).
    #[default]
    Aktiv,
    /// Das Spiel steht auf dem Feld; der Aufruf ruht, bis es wieder
    /// vom Feld genommen wird.
    Geparkt,
    /// Unbekannter Wert aus einer neueren Version — wie geparkt behandeln
    /// (lieber stumm als ein Geister-Aufruf).
    #[serde(other)]
    Unbekannt,
}

/// Ein gespeicherter Aufruf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredCall {
    match_id: i64,
    #[serde(default)]
    location_id: Option<i64>,
    called_at_ms: u64,
    #[serde(default)]
    phase: Phase,
}

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MeetingpointFile {
    #[serde(default)]
    tournament: String,
    /// In Aufrufreihenfolge.
    #[serde(default)]
    calls: Vec<StoredCall>,
}

/// Ergebnis eines Ladeversuchs (Muster `WishCourtStore`).
enum Ladung {
    Stand(MeetingpointFile),
    Leer,
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: MeetingpointFile,
    loaded: bool,
    /// Abrufe in Folge, in denen ein geparktes Spiel wieder als geplant
    /// geführt wurde. Bewusst nicht auf der Platte: Nach einem Neustart
    /// beginnt die Bestätigung von vorn, das kostet höchstens drei Abrufe.
    off_court_polls: HashMap<i64, u32>,
}

/// Der Meetingpoint-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit Commands, TL-Web und
/// Sync-Lauf denselben Stand sehen.
#[derive(Default)]
pub struct MeetingpointStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests).
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    persist_lock: Mutex<()>,
}

impl MeetingpointStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (je Snapshot). Beim ersten Mal wird der
    /// passende Stand geladen; bei Turnierwechsel verworfen.
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
                            // Aufrufe, die im RAM entstanden sind, bevor das
                            // Turnier feststand, gehen nicht verloren.
                            let frueh = std::mem::take(&mut inner.file.calls);
                            inner.file = file;
                            if frueh.is_empty() {
                                return;
                            }
                            for c in frueh {
                                upsert(&mut inner.file.calls, c);
                            }
                            // Frühe Aufrufe gleich sichern, nicht erst mit
                            // der nächsten Änderung (Review 02.11.2026).
                            drop(inner);
                            self.persist();
                            return;
                        }
                    }
                    Ladung::Leer => inner.loaded = true,
                    Ladung::Unlesbar => return,
                }
            }
            if inner.file.tournament.is_empty() {
                // Erstbindung: frühe RAM-Aufrufe gehören zu diesem Turnier.
                inner.file.tournament = name.to_string();
            } else if inner.file.tournament != name {
                // Turnierwechsel: Match-IDs gelten nur innerhalb eines Turniers.
                inner.file = MeetingpointFile {
                    tournament: name.to_string(),
                    ..Default::default()
                };
                inner.off_court_polls.clear();
            }
        }
        self.persist();
    }

    /// Aktuell gebundenes Turnier.
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    /// Ruft ein Spiel (Hand oder Automatik). Gleiche Halle wie ein schon
    /// bestehender Aufruf → erster Zeitpunkt bleibt, ein geparkter Aufruf
    /// wird wieder aktiv; andere Halle → Aufruf wird ersetzt und rückt ans
    /// Ende der Reihenfolge.
    pub fn call(&self, call: PreparationCall) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            inner.off_court_polls.remove(&call.match_id);
            upsert(
                &mut inner.file.calls,
                StoredCall {
                    match_id: call.match_id,
                    location_id: call.location_id,
                    called_at_ms: call.called_at_ms,
                    phase: Phase::Aktiv,
                },
            )
        };
        if changed {
            self.persist();
        }
    }

    /// Nimmt den Aufruf eines Spiels zurück (löscht ihn ganz).
    pub fn retract(&self, match_id: i64) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            inner.off_court_polls.remove(&match_id);
            let before = inner.file.calls.len();
            inner.file.calls.retain(|c| c.match_id != match_id);
            before != inner.file.calls.len()
        };
        if changed {
            self.persist();
        }
    }

    /// Alle **aktiven** Aufrufe in Aufrufreihenfolge — geparkte zählen
    /// nicht (sonst bliebe ein Spiel auf dem Feld gelb oder angesagt).
    pub fn active_calls(&self) -> Vec<PreparationCall> {
        self.inner
            .lock()
            .unwrap()
            .file
            .calls
            .iter()
            .filter(|c| c.phase == Phase::Aktiv)
            .map(|c| PreparationCall {
                match_id: c.match_id,
                location_id: c.location_id,
                called_at_ms: c.called_at_ms,
            })
            .collect()
    }

    /// Phase eines Aufrufs (für Tests und spätere Anzeigen).
    pub fn phase(&self, match_id: i64) -> Option<Phase> {
        self.inner
            .lock()
            .unwrap()
            .file
            .calls
            .iter()
            .find(|c| c.match_id == match_id)
            .map(|c| c.phase)
    }

    /// Gleicht die Aufrufe mit dem BTP-Stand ab (je Sync-Lauf): parken,
    /// reaktivieren, aufräumen. Liefert die Match-IDs der **gelöschten**
    /// Aufrufe (für das Vergessen der Nachruf-Stufen); leer, wenn nichts
    /// gelöscht wurde.
    pub fn reconcile(&self, snapshot: &BtpSnapshot) -> Vec<i64> {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let Inner {
                file,
                off_court_polls,
                ..
            } = &mut *inner;
            let mut changed = false;
            let mut geloescht: Vec<i64> = Vec::new();
            let mut keep: Vec<StoredCall> = Vec::with_capacity(file.calls.len());
            for mut c in file.calls.drain(..) {
                let Some(m) = snapshot.matches.iter().find(|m| m.id == c.match_id) else {
                    off_court_polls.remove(&c.match_id);
                    changed = true;
                    geloescht.push(c.match_id);
                    continue; // verschwunden
                };
                if m.status == MatchStatus::Finished || m.team1.is_empty() || m.team2.is_empty() {
                    off_court_polls.remove(&c.match_id);
                    changed = true;
                    geloescht.push(c.match_id);
                    continue; // beendet oder Paarung weg
                }
                match (&m.status, c.phase) {
                    (&MatchStatus::OnCourt, Phase::Geparkt) => {
                        off_court_polls.remove(&c.match_id);
                    }
                    (&MatchStatus::OnCourt, _) => {
                        c.phase = Phase::Geparkt;
                        off_court_polls.remove(&c.match_id);
                        changed = true;
                    }
                    (&MatchStatus::Scheduled, Phase::Aktiv) => {}
                    (&MatchStatus::Scheduled, _) => {
                        let n = off_court_polls.entry(c.match_id).or_insert(0);
                        *n += 1;
                        if *n >= DEASSIGN_CONFIRM_POLLS {
                            off_court_polls.remove(&c.match_id);
                            c.phase = Phase::Aktiv;
                            changed = true;
                        }
                    }
                    (&MatchStatus::Finished, _) => unreachable!("oben aussortiert"),
                }
                keep.push(c);
            }
            file.calls = keep;
            (changed, geloescht)
        };
        let (changed, geloescht) = changed;
        if changed {
            self.persist();
        }
        geloescht
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
                tracing::warn!("meetingpoint.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<MeetingpointFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("meetingpoint.json unlesbar – beginne leer");
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
                return; // ohne Turnier-Kopf nicht zuzuordnen
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

/// Fügt einen Aufruf ein bzw. aktualisiert ihn nach den Halle-gleich/anders-
/// Regeln. Liefert `true`, wenn sich etwas geändert hat.
fn upsert(calls: &mut Vec<StoredCall>, neu: StoredCall) -> bool {
    if let Some(alt) = calls.iter_mut().find(|c| c.match_id == neu.match_id) {
        if alt.location_id == neu.location_id {
            // Gleiche Halle: Zeitpunkt und Platz in der Reihenfolge bleiben.
            let changed = alt.phase != neu.phase;
            alt.phase = neu.phase;
            return changed;
        }
    }
    calls.retain(|c| c.match_id != neu.match_id);
    calls.push(neu);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::BtpMatch;
    use std::path::Path;

    const TURNIER: &str = "Test BTS Light";

    fn aufruf(match_id: i64, halle: Option<i64>, ts: u64) -> PreparationCall {
        PreparationCall {
            match_id,
            location_id: halle,
            called_at_ms: ts,
        }
    }

    fn store_mit_datei(dir: &Path) -> MeetingpointStore {
        let store = MeetingpointStore::default();
        store.set_path(dir.join("meetingpoint.json"));
        store.set_tournament(TURNIER);
        store
    }

    fn spiel(id: i64, status: MatchStatus) -> BtpMatch {
        crate::tablet::state::tests::match_on(id, None, status)
    }

    fn snap(matches: Vec<BtpMatch>) -> BtpSnapshot {
        let mut s = crate::tablet::state::tests::snapshot(matches, vec![]);
        s.tournament_name = TURNIER.into();
        s
    }

    fn ids(store: &MeetingpointStore) -> Vec<i64> {
        store.active_calls().iter().map(|c| c.match_id).collect()
    }

    #[test]
    fn aufrufe_ueberleben_einen_neustart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, Some(1), 1000));
            store.call(aufruf(11, None, 2000));
        }
        let neu = store_mit_datei(dir.path());
        assert_eq!(
            neu.active_calls(),
            vec![aufruf(10, Some(1), 1000), aufruf(11, None, 2000)]
        );
    }

    #[test]
    fn ein_turnierwechsel_verwirft_die_aufrufe() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, Some(1), 1000));
        }
        let neu = MeetingpointStore::default();
        neu.set_path(dir.path().join("meetingpoint.json"));
        neu.set_tournament("Ein anderes Turnier");
        assert!(neu.active_calls().is_empty());
        // Und auch im laufenden Betrieb.
        neu.call(aufruf(12, None, 3000));
        neu.set_tournament("Noch ein Turnier");
        assert!(neu.active_calls().is_empty());
    }

    #[test]
    fn eine_unlesbare_datei_beginnt_leer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("meetingpoint.json"), "{kaputt").unwrap();
        let store = store_mit_datei(dir.path());
        assert!(store.active_calls().is_empty());
        store.call(aufruf(10, None, 1));
        assert_eq!(ids(&store), vec![10]);
    }

    #[test]
    fn ein_aufruf_vor_der_turnierbindung_bleibt_erhalten() {
        // Startphase: Die TL ruft, bevor der erste Snapshot das Turnier
        // gemeldet hat. Der Aufruf darf beim Binden nicht verschwinden.
        let dir = tempfile::tempdir().unwrap();
        let store = MeetingpointStore::default();
        store.set_path(dir.path().join("meetingpoint.json"));
        store.call(aufruf(10, None, 1));
        store.set_tournament(TURNIER);
        assert_eq!(ids(&store), vec![10]);
        let neu = store_mit_datei(dir.path());
        assert_eq!(ids(&neu), vec![10], "und landet auf der Platte");
    }

    #[test]
    fn neuaufruf_gleiche_halle_behaelt_zeitpunkt_und_platz() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1000));
        store.call(aufruf(11, Some(1), 2000));
        store.call(aufruf(10, Some(1), 5000));
        assert_eq!(
            store.active_calls(),
            vec![aufruf(10, Some(1), 1000), aufruf(11, Some(1), 2000)]
        );
    }

    #[test]
    fn neuaufruf_andere_halle_ersetzt_und_rueckt_ans_ende() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1000));
        store.call(aufruf(11, Some(1), 2000));
        store.call(aufruf(10, Some(2), 5000));
        assert_eq!(
            store.active_calls(),
            vec![aufruf(11, Some(1), 2000), aufruf(10, Some(2), 5000)]
        );
    }

    #[test]
    fn ruecknahme_loescht_den_aufruf() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.retract(10);
        assert!(store.active_calls().is_empty());
        assert_eq!(store.phase(10), None);
    }

    #[test]
    fn auf_dem_feld_wird_geparkt_und_nach_bestaetigter_abnahme_reaktiviert() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1000));
        assert!(store
            .reconcile(&snap(vec![spiel(10, MatchStatus::OnCourt)]))
            .is_empty());
        assert_eq!(store.phase(10), Some(Phase::Geparkt));
        assert!(store.active_calls().is_empty(), "kein Aufruf am Feldspiel");

        let geplant = snap(vec![spiel(10, MatchStatus::Scheduled)]);
        for _ in 1..DEASSIGN_CONFIRM_POLLS {
            store.reconcile(&geplant);
            assert_eq!(store.phase(10), Some(Phase::Geparkt));
        }
        store.reconcile(&geplant);
        assert_eq!(store.active_calls(), vec![aufruf(10, Some(1), 1000)]);
    }

    #[test]
    fn ein_flackern_reaktiviert_nicht() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        let feld = snap(vec![spiel(10, MatchStatus::OnCourt)]);
        let geplant = snap(vec![spiel(10, MatchStatus::Scheduled)]);
        store.reconcile(&feld);
        for _ in 0..5 {
            store.reconcile(&geplant); // ein Abruf „geplant"
            store.reconcile(&feld); // und wieder auf dem Feld
        }
        assert_eq!(store.phase(10), Some(Phase::Geparkt));
    }

    #[test]
    fn spielende_verschwinden_und_fehlende_paarung_loeschen() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.call(aufruf(11, None, 2));
        store.call(aufruf(12, None, 3));
        store.call(aufruf(13, None, 4));
        let mut ohne_paarung = spiel(12, MatchStatus::Scheduled);
        ohne_paarung.team2.clear();
        let geloescht = store.reconcile(&snap(vec![
            spiel(10, MatchStatus::Finished),
            ohne_paarung,
            spiel(13, MatchStatus::Scheduled),
        ]));
        assert_eq!(geloescht, vec![10, 11, 12]);
        assert_eq!(ids(&store), vec![13]);
    }

    #[test]
    fn eine_ergebniskorrektur_holt_den_aufruf_nicht_zurueck() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.reconcile(&snap(vec![spiel(10, MatchStatus::Finished)]));
        for _ in 0..DEASSIGN_CONFIRM_POLLS {
            store.reconcile(&snap(vec![spiel(10, MatchStatus::Scheduled)]));
        }
        assert!(store.active_calls().is_empty());
    }

    #[test]
    fn ein_geparkter_aufruf_ueberlebt_den_neustart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, None, 1));
            store.reconcile(&snap(vec![spiel(10, MatchStatus::OnCourt)]));
        }
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.phase(10), Some(Phase::Geparkt));
        assert!(neu.active_calls().is_empty());
    }

    #[test]
    fn eine_unbekannte_phase_gilt_nicht_als_aktiv() {
        let file: MeetingpointFile = serde_json::from_str(
            r#"{"tournament":"T","calls":[{"match_id":1,"called_at_ms":5,"phase":"kuenftig"}]}"#,
        )
        .unwrap();
        assert_eq!(file.calls[0].phase, Phase::Unbekannt);
    }

    #[test]
    fn ein_zweiter_start_ohne_datei_verwirft_nichts() {
        // `set_path` erneut (zweites `start_sync`), Datei fehlt: Der Stand im
        // RAM gehört zum selben Turnier und darf nicht verschwinden.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.call(aufruf(10, None, 1));
        std::fs::remove_file(dir.path().join("meetingpoint.json")).unwrap();
        store.set_path(dir.path().join("meetingpoint.json"));
        store.set_tournament(TURNIER);
        assert_eq!(ids(&store), vec![10]);
    }

    #[test]
    fn ohne_turnier_wird_nichts_geschrieben() {
        let dir = tempfile::tempdir().unwrap();
        let store = MeetingpointStore::default();
        store.set_path(dir.path().join("meetingpoint.json"));
        store.call(aufruf(10, None, 1));
        assert!(!dir.path().join("meetingpoint.json").exists());
    }
}
