//! Schiedsrichter-Roster des Hosts (Spec
//! `docs/features/schiedsrichter-management.md`, ADR 0021/0022).
//!
//! BTP liefert die Schiedsrichter-Stammliste (`Officials`) — sie ist die
//! Wahrheit (R2) und wird hier **nicht** gespeichert. Dieser Speicher hält
//! ausschließlich die Zusatzdaten, die BTP nicht kennt: Rotationsreihenfolge,
//! Pausen, Stammverein (BTP überträgt keinen — Messung 13.08.2026),
//! Sperrlisten, feldweise Schalter und die lokalen SR/AR-Zuweisungen.
//!
//! Grundsätze:
//! - **Turniergebunden** (ADR 0022): Alles steht in einer eigenen Datei im
//!   App-Datenverzeichnis, im Kopf das Turnier. Wechselt das Turnier, wird
//!   der Stand verworfen — BTP-IDs gelten nur innerhalb eines Turniers, und
//!   Sperrlisten sind Personendaten, die nicht über Turniere hinweg
//!   weiterleben dürfen.
//! - **Außerhalb der `AppConfig`**: `export_identity` nimmt die komplette
//!   Konfiguration mit; Sperrlisten haben dort nichts zu suchen.
//! - **Best effort** wie `persist_scores`: Ein Schreibfehler kostet
//!   höchstens die Einteilung, nie ein Ergebnis.
//! - **Zuweisungen hängen am Match, nicht am Feld**: Nach Spielende bleiben
//!   sie liegen — sie sind die Grundlage der Einsatz-Ableitung (Spec Nr. 11,
//!   keine eigene Historien-Datenhaltung).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

/// Dienst eines Officials an einem Spiel. BTP: `Official1ID` = SR,
/// `Official2ID` = AR (an der BTP-Maske verifiziert, Messung 13.08.2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OfficialRole {
    /// Schiedsrichter (`Official1ID`).
    Sr,
    /// Aufschlagrichter (`Official2ID`).
    Ar,
}

/// Zusatzdaten je Official — alles, was BTP nicht liefert.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OfficialExtra {
    /// Pausiert (Pause, kommt später, geht früher)? Die Rotation überspringt
    /// ihn, seine Position in der Reihenfolge bleibt erhalten.
    pub paused: bool,
    /// Stammverein — in BTS Light gepflegt, weil BTP am Official keinen
    /// Verein überträgt (Messung 13.08.2026). Grundlage der Konflikt-Warnung
    /// „Verein".
    pub club: String,
    /// Zusätzlich gesperrte Vereine (Personendaten — nie im Broadcast-State).
    pub blocked_clubs: Vec<String>,
    /// Gesperrte Spieler (BTP-`PlayerID`; Personendaten).
    pub blocked_players: Vec<i64>,
}

impl OfficialExtra {
    /// Trägt der Eintrag überhaupt etwas? Leere Einträge werden nicht
    /// aufbewahrt — sonst wüchse die Datei mit jedem Blick in einen Dialog.
    fn is_empty(&self) -> bool {
        !self.paused
            && self.club.is_empty()
            && self.blocked_clubs.is_empty()
            && self.blocked_players.is_empty()
    }
}

/// Lokale SR/AR-Zuweisung eines Spiels (Overlay; trägt das BTP-Match eigene
/// `Official1ID`/`Official2ID`, gewinnt BTP — Spec „Konfliktregel").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchOfficials {
    pub sr: Option<i64>,
    pub ar: Option<i64>,
}

impl MatchOfficials {
    fn is_empty(&self) -> bool {
        self.sr.is_none() && self.ar.is_none()
    }
}

/// Feldweise Schalter (Spec Nr. 6): SR-Rotation, AR-Rotation und
/// Tabletbediener-Vergabe je Feld — daraus ergeben sich die drei
/// Betriebsformen. Default **alles aktiv**, damit sich das Verhalten
/// bestehender Installationen (Bediener-Vergabe) nicht ändert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CourtSwitches {
    /// SR-Rotation auf diesem Feld aktiv?
    pub sr: bool,
    /// AR-Rotation auf diesem Feld aktiv?
    pub ar: bool,
    /// Zähltafelbediener-Vergabe auf diesem Feld aktiv?
    pub operator: bool,
}

impl Default for CourtSwitches {
    fn default() -> Self {
        Self {
            sr: true,
            ar: true,
            operator: true,
        }
    }
}

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022: lieber verwerfen
/// als falsch zuordnen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OfficialsFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Rotationsreihenfolge (Official-IDs).
    #[serde(default)]
    order: Vec<i64>,
    /// Official-ID → Zusatzdaten.
    #[serde(default)]
    extras: HashMap<i64, OfficialExtra>,
    /// Match-ID → lokale Zuweisung (bleibt nach Spielende liegen).
    #[serde(default)]
    assignments: HashMap<i64, MatchOfficials>,
    /// CourtID → feldweise Schalter (fehlt = Default „alles aktiv").
    #[serde(default)]
    courts: HashMap<i64, CourtSwitches>,
}

/// Ergebnis eines Ladeversuchs. Der Unterschied zwischen „gibt es nicht"
/// und „komme gerade nicht dran" entscheidet, ob überschrieben werden darf.
enum Ladung {
    /// Ein gelesener Stand (kann zu einem anderen Turnier gehören).
    Stand(OfficialsFile),
    /// Keine Datei (oder Persistenz aus, oder kaputter Inhalt) ⇒ leer starten.
    Leer,
    /// Datei vorhanden, aber nicht lesbar ⇒ nicht anfassen, später erneut.
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: OfficialsFile,
    /// Wurde die Datei schon einmal gelesen? Der Pfad steht beim Start fest,
    /// das Turnier kommt erst mit dem ersten Snapshot — geladen wird deshalb
    /// beim ersten `set_tournament` (Muster BTP-Nachschub-Queue, ADR 0018).
    loaded: bool,
}

/// Der Roster-Speicher. Lebt im [`TabletState`](super::state::TabletState),
/// damit LAN-Server, Relay-Client und Tauri-Commands denselben Stand sehen.
#[derive(Default)]
pub struct OfficialsStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests, Slave-Betrieb).
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `scores_persist_lock`).
    persist_lock: Mutex<()>,
}

impl OfficialsStore {
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
                    // übernommen — sonst gehörten fremde BTP-IDs und fremde
                    // Sperrlisten zum neuen Turnier (ADR 0022).
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
                    // — der nächste Snapshot versucht es erneut. Andernfalls
                    // löschte ein Moment Pech die Einteilung des Turniers.
                    Ladung::Unlesbar => return,
                }
            }
            // Turnierwechsel (oder fremder Dateistand): verwerfen.
            inner.file = OfficialsFile {
                tournament: name.to_string(),
                ..Default::default()
            };
        }
        // Der leere Stand geht sofort auf die Platte: Ein Absturz direkt
        // nach dem Wechsel darf die Personendaten des alten Turniers nicht
        // wieder auferstehen lassen.
        self.persist();
    }

    /// Turnier, zu dem der aktuelle Stand gehört (leer = noch keins).
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    // ── Zusatzdaten je Official ───────────────────────────────────────

    /// Zusatzdaten eines Officials (nie `None` — ein unbekannter Official
    /// hat schlicht Default-Zusatzdaten).
    pub fn extra(&self, official_id: i64) -> OfficialExtra {
        self.inner
            .lock()
            .unwrap()
            .file
            .extras
            .get(&official_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Pausiert-Schalter setzen.
    pub fn set_paused(&self, official_id: i64, paused: bool) {
        self.mit_extra(official_id, |e| e.paused = paused);
    }

    /// Stammverein pflegen (leer = keiner).
    pub fn set_club(&self, official_id: i64, club: &str) {
        let club = club.trim().to_string();
        self.mit_extra(official_id, |e| e.club = club);
    }

    /// Sperrlisten setzen (ersetzen beide Listen).
    pub fn set_blocklists(&self, official_id: i64, clubs: Vec<String>, players: Vec<i64>) {
        let clubs: Vec<String> = clubs
            .into_iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        self.mit_extra(official_id, |e| {
            e.blocked_clubs = clubs;
            e.blocked_players = players;
        });
    }

    /// Zusatzdaten ändern und speichern. Ein Eintrag, der nichts mehr trägt,
    /// wird entfernt — die Datei soll nicht mit Leereinträgen wachsen.
    fn mit_extra(&self, official_id: i64, f: impl FnOnce(&mut OfficialExtra)) {
        {
            let mut inner = self.inner.lock().unwrap();
            let e = inner.file.extras.entry(official_id).or_default();
            f(e);
            if e.is_empty() {
                inner.file.extras.remove(&official_id);
            }
        }
        self.persist();
    }

    // ── Rotationsreihenfolge ──────────────────────────────────────────

    /// Die gespeicherte Rotationsreihenfolge.
    pub fn order(&self) -> Vec<i64> {
        self.inner.lock().unwrap().file.order.clone()
    }

    /// Reihenfolge komplett setzen (manuelles Umsortieren).
    pub fn set_order(&self, order: Vec<i64>) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.file.order = order;
        }
        self.persist();
    }

    /// Roster mit der BTP-Liste abgleichen: neue Officials hinten anhängen,
    /// bekannte in ihrer Position lassen. Officials, die aus BTP
    /// verschwinden, bleiben in der Reihenfolge stehen — kehren sie zurück,
    /// gelten Position und Zusatzdaten wieder (Spec „Fehlerfälle").
    pub fn sync_roster(&self, btp_ids: &[i64]) {
        {
            let mut inner = self.inner.lock().unwrap();
            let mut neu: Vec<i64> = Vec::new();
            for id in btp_ids.iter().copied() {
                // Auch gegen `neu` prüfen: Ein doppelt gelisteter Official
                // bekäme sonst zwei Plätze und käme doppelt so oft dran.
                if id > 0 && !inner.file.order.contains(&id) && !neu.contains(&id) {
                    neu.push(id);
                }
            }
            if neu.is_empty() {
                return;
            }
            inner.file.order.extend(neu);
        }
        self.persist();
    }

    // ── Zuweisungen (am Match, nicht am Feld) ─────────────────────────

    /// Lokale Zuweisung eines Spiels.
    pub fn assignment(&self, match_id: i64) -> MatchOfficials {
        self.inner
            .lock()
            .unwrap()
            .file
            .assignments
            .get(&match_id)
            .copied()
            .unwrap_or_default()
    }

    /// Alle lokalen Zuweisungen (Grundlage der Einsatz-Ableitung).
    pub fn assignments(&self) -> HashMap<i64, MatchOfficials> {
        self.inner.lock().unwrap().file.assignments.clone()
    }

    /// Official einem Spiel zuweisen.
    pub fn assign(&self, match_id: i64, role: OfficialRole, official_id: i64) {
        if match_id <= 0 || official_id <= 0 {
            return;
        }
        self.mit_zuweisung(match_id, |a| match role {
            OfficialRole::Sr => a.sr = Some(official_id),
            OfficialRole::Ar => a.ar = Some(official_id),
        });
    }

    /// Zuweisung eines Spiels lösen.
    pub fn clear_assignment(&self, match_id: i64, role: OfficialRole) {
        self.mit_zuweisung(match_id, |a| match role {
            OfficialRole::Sr => a.sr = None,
            OfficialRole::Ar => a.ar = None,
        });
    }

    /// Alle Zuweisungen räumen — beim Abschalten von `officials.enabled`
    /// mitten im Turnier (Spec Nr. 1, analog `clear_scorekeeper_assignments`).
    pub fn clear_assignments(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.file.assignments.is_empty() {
                return;
            }
            inner.file.assignments.clear();
        }
        self.persist();
    }

    fn mit_zuweisung(&self, match_id: i64, f: impl FnOnce(&mut MatchOfficials)) {
        {
            let mut inner = self.inner.lock().unwrap();
            let a = inner.file.assignments.entry(match_id).or_default();
            f(a);
            if a.is_empty() {
                inner.file.assignments.remove(&match_id);
            }
        }
        self.persist();
    }

    // ── Feldweise Schalter ────────────────────────────────────────────

    /// Schalter eines Felds (fehlt ein Eintrag: alles aktiv).
    pub fn court_switches(&self, court_id: i64) -> CourtSwitches {
        self.inner
            .lock()
            .unwrap()
            .file
            .courts
            .get(&court_id)
            .copied()
            .unwrap_or_default()
    }

    /// Schalter eines Felds setzen.
    pub fn set_court_switches(&self, court_id: i64, switches: CourtSwitches) {
        {
            let mut inner = self.inner.lock().unwrap();
            if switches == CourtSwitches::default() {
                // Default braucht keinen Eintrag — hält die Datei klein und
                // die Regel „ohne Eintrag alles aktiv" eindeutig.
                inner.file.courts.remove(&court_id);
            } else {
                inner.file.courts.insert(court_id, switches);
            }
        }
        self.persist();
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
                tracing::warn!("officials-state.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<OfficialsFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                // Kaputter Inhalt ist nicht zu retten — hier ist Überschreiben
                // richtig, anders als beim Lesefehler.
                tracing::warn!("officials-state.json unlesbar – beginne leer");
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
        // Schreib-Guard VOR dem Schnappschuss (Muster `TimelineStore`):
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

    fn store_mit_datei(dir: &Path) -> OfficialsStore {
        let store = OfficialsStore::default();
        store.set_path(dir.join("officials-state.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    #[test]
    fn zusatzdaten_werden_je_official_gehalten() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(7, true);
        store.set_club(7, "TSV Musterstadt");
        store.set_blocklists(7, vec!["SC Nachbar".into()], vec![42, 43]);

        let e = store.extra(7);
        assert!(e.paused);
        assert_eq!(e.club, "TSV Musterstadt");
        assert_eq!(e.blocked_clubs, vec!["SC Nachbar".to_string()]);
        assert_eq!(e.blocked_players, vec![42, 43]);
        // Ein unbekannter Official hat schlicht leere Zusatzdaten.
        assert_eq!(store.extra(999), OfficialExtra::default());
        // Pause wieder aufheben.
        store.set_paused(7, false);
        assert!(!store.extra(7).paused);
    }

    #[test]
    fn stand_ueberlebt_app_neustart() {
        // Spec Nr. 10: Reihenfolge, Pausen, Sperrlisten, Schalter und
        // Zuweisungen überleben Neustart/Absturz.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_order(vec![3, 1, 2]);
        store.set_paused(1, true);
        store.set_blocklists(2, vec!["SC Nachbar".into()], vec![42]);
        store.assign(500, OfficialRole::Sr, 3);
        store.assign(500, OfficialRole::Ar, 1);
        store.set_court_switches(
            9,
            CourtSwitches {
                sr: true,
                ar: false,
                operator: false,
            },
        );

        // „Neustart": frischer Speicher, gleiche Datei, gleiches Turnier.
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.order(), vec![3, 1, 2]);
        assert!(neu.extra(1).paused);
        assert_eq!(neu.extra(2).blocked_players, vec![42]);
        assert_eq!(
            neu.assignment(500),
            MatchOfficials {
                sr: Some(3),
                ar: Some(1)
            }
        );
        assert_eq!(
            neu.court_switches(9),
            CourtSwitches {
                sr: true,
                ar: false,
                operator: false
            }
        );
    }

    #[test]
    fn turnierwechsel_verwirft_den_stand() {
        // ADR 0022: BTP-IDs gelten nur im Turnier, Sperrlisten sind
        // Personendaten — beim Wechsel wird verworfen, nicht mitgeschleppt.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(1, true);
        store.set_blocklists(1, vec!["SC Nachbar".into()], vec![42]);
        store.assign(500, OfficialRole::Sr, 1);
        store.set_order(vec![1, 2]);

        store.set_tournament("Ganz anderes Turnier");
        assert!(!store.extra(1).paused);
        assert!(store.extra(1).blocked_players.is_empty());
        assert_eq!(store.assignment(500), MatchOfficials::default());
        assert!(store.order().is_empty());
        assert_eq!(store.tournament(), "Ganz anderes Turnier");

        // Und der verworfene Stand ist auch auf der Platte weg — ein
        // Neustart darf ihn nicht zurückholen.
        let neu = OfficialsStore::default();
        neu.set_path(dir.path().join("officials-state.json"));
        neu.set_tournament("Test BTS Light");
        assert!(!neu.extra(1).paused);
        assert!(neu.order().is_empty());
    }

    #[test]
    fn fremder_dateistand_wird_beim_start_verworfen() {
        // Datei aus einem früheren Turnier: Der erste Snapshot des neuen
        // Turniers wirft sie weg, statt fremde IDs zu übernehmen.
        let dir = tempfile::tempdir().unwrap();
        let alt = store_mit_datei(dir.path());
        alt.set_blocklists(1, vec!["SC Nachbar".into()], vec![42]);

        let neu = OfficialsStore::default();
        neu.set_path(dir.path().join("officials-state.json"));
        neu.set_tournament("Nächstes Wochenende");
        assert!(neu.extra(1).blocked_clubs.is_empty());
        assert_eq!(neu.tournament(), "Nächstes Wochenende");
    }

    #[test]
    fn reihenfolge_ergaenzt_neue_und_haelt_verschwundene_inert() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[5, 3]);
        assert_eq!(store.order(), vec![5, 3]);
        // Neuer Official aus BTP kommt hinten dazu, Reihenfolge bleibt.
        store.sync_roster(&[5, 3, 8]);
        assert_eq!(store.order(), vec![5, 3, 8]);
        // Manuell vorgezogen — der nächste Abgleich rührt das nicht an.
        store.set_order(vec![8, 5, 3]);
        store.sync_roster(&[5, 3, 8]);
        assert_eq!(store.order(), vec![8, 5, 3]);
        // Verschwindet einer aus BTP, bleibt seine Position stehen: kehrt er
        // zurück, gilt sie wieder (Spec „Fehlerfälle").
        store.set_paused(5, true);
        store.sync_roster(&[8, 3]);
        assert_eq!(store.order(), vec![8, 5, 3]);
        assert!(store.extra(5).paused);
    }

    #[test]
    fn zuweisungen_bleiben_dem_match_erhalten_und_sind_einzeln_loesbar() {
        // Spec Nr. 11: Zuweisungen hängen am Match und bleiben nach
        // Spielende liegen — sie sind die Grundlage der Einsatz-Ableitung.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.assign(500, OfficialRole::Ar, 2);
        store.assign(501, OfficialRole::Sr, 3);
        assert_eq!(store.assignments().len(), 2);

        store.clear_assignment(500, OfficialRole::Ar);
        assert_eq!(
            store.assignment(500),
            MatchOfficials {
                sr: Some(1),
                ar: None
            }
        );
        // Letzte Rolle gelöst ⇒ kein leerer Rest-Eintrag.
        store.clear_assignment(500, OfficialRole::Sr);
        assert_eq!(store.assignment(500), MatchOfficials::default());
        assert_eq!(store.assignments().len(), 1);

        // Globales Abschalten räumt alles (Spec Nr. 1).
        store.clear_assignments();
        assert!(store.assignments().is_empty());
    }

    #[test]
    fn feldschalter_sind_standardmaessig_alle_aktiv() {
        // Bestandsverhalten der Zähltafelbediener-Vergabe bleibt unverändert:
        // ohne Eintrag ist jedes Feld aktiv.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert_eq!(store.court_switches(1), CourtSwitches::default());
        assert!(store.court_switches(1).operator);

        store.set_court_switches(
            1,
            CourtSwitches {
                sr: false,
                ar: false,
                operator: false,
            },
        );
        assert!(!store.court_switches(1).operator);
        // Andere Felder bleibt davon unberührt.
        assert!(store.court_switches(2).operator);
    }

    #[test]
    fn eine_voruebergehend_unlesbare_datei_wird_nicht_ueberschrieben() {
        // Windows-Realität: Virenscanner oder ein hängendes Handle sperren
        // die Datei für einen Moment. Ein „nicht lesbar" ist NICHT dasselbe
        // wie „gibt es nicht" — würde der Speicher hier leer starten und
        // schreiben, wäre die komplette Einteilung des Turniers still weg.
        // Ersatz für die Sperre im Test: ein Verzeichnis unter dem
        // Dateinamen — lesbar ist das ebenso wenig, und es ist auch kein
        // `NotFound`.
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("officials-state.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = OfficialsStore::default();
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
        store.set_paused(1, true);
        assert!(store.extra(1).paused);

        // Sobald die Datei wieder lesbar ist, holt der nächste Snapshot den
        // Stand nach — ohne App-Neustart.
        std::fs::remove_dir(&pfad).unwrap();
        let vorlage = store_mit_datei(dir.path());
        vorlage.set_order(vec![4, 2]);
        store.set_tournament("Test BTS Light");
        assert_eq!(store.order(), vec![4, 2]);
        assert_eq!(store.tournament(), "Test BTS Light");
    }

    #[test]
    fn ein_doppelt_gelisteter_official_bekommt_nur_einen_platz() {
        // Ein doppelter Eintrag im BTP-Container (oder ein künftiger
        // Aufrufer, der zwei Hallen-Listen aneinanderhängt) darf niemandem
        // zwei Plätze in der Rotation geben — er käme doppelt so oft dran.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[5, 3, 5]);
        assert_eq!(store.order(), vec![5, 3]);
    }

    #[test]
    fn ein_spaeter_gesetzter_pfad_laedt_den_stand_noch() {
        // Der Speicher darf nicht davon abhängen, dass `set_path` vor dem
        // ersten Snapshot läuft: Wer die Reihenfolge umdreht, bekäme sonst
        // einen leeren Stand, der den vorhandenen beim ersten Klick
        // überschreibt.
        let dir = tempfile::tempdir().unwrap();
        let alt = store_mit_datei(dir.path());
        alt.set_order(vec![7, 1]);

        let store = OfficialsStore::default();
        store.set_tournament("Test BTS Light"); // Snapshot zuerst …
        store.set_path(dir.path().join("officials-state.json")); // … Pfad danach
        store.set_tournament("Test BTS Light");
        assert_eq!(store.order(), vec![7, 1]);
    }

    #[test]
    fn ohne_pfad_bleibt_alles_im_ram() {
        // Slave-Betrieb/Tests: kein Pfad gesetzt ⇒ keine Datei, aber der
        // Speicher funktioniert.
        let store = OfficialsStore::default();
        store.set_tournament("Ohne Ablage");
        store.set_paused(1, true);
        assert!(store.extra(1).paused);
    }

    #[test]
    fn leerer_turniername_aendert_nichts() {
        // Ein Snapshot ohne Turniernamen (Startphase) darf den Stand nicht
        // wegwerfen.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(1, true);
        store.set_tournament("");
        store.set_tournament("   ");
        assert!(store.extra(1).paused);
        assert_eq!(store.tournament(), "Test BTS Light");
    }
}
