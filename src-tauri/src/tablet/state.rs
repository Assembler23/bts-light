//! Geteilter Zustand zwischen Sync-Loop und Tablet-Server.
//!
//! Der Sync-Loop legt hier den jeweils neuesten BTP-Snapshot ab, der
//! Tablet-Server pflegt die laufenden Court-Sessions. Beide Seiten teilen
//! sich ein `Arc<TabletState>`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use relay_proto::{MonitorCommand, MonitorCommandKind, MonitorDeviceInfo};

use crate::btp::model::{BtpCourt, BtpMatch, BtpSnapshot, Discipline, MatchStatus};

/// Aktuelle Unix-Zeit in Millisekunden.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Obergrenze verfolgter Monitor-Geräte (Missbrauchs-Schutz). Bei
/// Überschreitung wird das am längsten nicht gesehene Gerät verdrängt.
const MAX_MONITOR_DEVICES: usize = 128;

/// Flüchtiger Live-Zustand eines Court-Monitor-Geräts (nicht persistiert –
/// die Feld-Zuweisungen liegen in `monitor-assignments.json`).
#[derive(Debug, Clone, Default)]
struct MonitorLive {
    /// Zeitpunkt des letzten Polls (Unix-ms) – für den Online-Status.
    last_seen_ms: u64,
    /// Offener Fernbefehl (Neu laden / Identifizieren).
    command: Option<MonitorCommand>,
}

/// Akkustand eines Tablets. Liefern nur Android-/Chrome-Tablets – iPads
/// (Safari) geben den Akkustand grundsätzlich nicht her.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TabletBattery {
    /// Ladestand in Prozent (0–100).
    pub percent: i64,
    /// Lädt das Tablet gerade?
    pub charging: bool,
}

/// Laufende Tablet-Sitzung an einem Court.
#[derive(Debug, Clone)]
struct CourtSession {
    /// BTP-Match-ID, das dieses Tablet zählt (0 = noch keins).
    match_id: i64,
    /// Zuletzt vom Tablet gemeldeter Satzstand (Team1, Team2).
    sets: Vec<(i64, i64)>,
    /// Ist die WebSocket-Verbindung des Tablets offen?
    connected: bool,
    /// Zuletzt gemeldeter Akkustand (falls das Tablet ihn liefert).
    battery: Option<TabletBattery>,
    /// Verletzung/Behandlung – das Tablet hat das Spiel unterbrochen.
    injury: bool,
    /// Die Turnierleitung wurde an dieses Feld gerufen.
    official: bool,
}

/// Eine Court-Zeile für die Felder-Übersicht der Turnierleitung.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CourtOverview {
    /// Stabile BTP-CourtID des Felds – die Identität. Feldnamen wiederholen
    /// sich bei Mehr-Hallen-Turnieren, die CourtID nicht.
    pub court_id: i64,
    /// Feldname (Anzeige), z. B. „1" oder „Feld 3".
    pub court: String,
    /// Hallenname (BTP-`Location`) des Felds – Grundlage der hallenweisen
    /// Gruppierung im Frontend. Leerer String bei Ein-Hallen-Turnieren
    /// oder wenn das Feld keiner auflösbaren Halle zugeordnet ist.
    pub location: String,
    /// BTP-Match-ID des aktuellen Spiels (0 = kein Match). Damit erkennt
    /// die Oberfläche, wenn ein Feld ein neues Spiel bekommt (Sprachansage).
    pub match_id: i64,
    /// Anzeigename des Matches, z. B. "HE G1"; leer wenn kein Match.
    pub match_name: String,
    /// Disziplin des aktuellen Matches (für die Sprachansage).
    pub discipline: Discipline,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Nationalitäten von Team 1, parallel zu `team1` (leerer String,
    /// wenn unbekannt) – Grundlage der automatischen DE/EN-Ansage.
    pub team1_nationalities: Vec<String>,
    pub team2_nationalities: Vec<String>,
    /// Aktueller Satzstand – vom Tablet, falls aktiv, sonst aus BTP.
    pub sets: Vec<(i64, i64)>,
    pub tablet_connected: bool,
    /// Akkustand des Tablets, falls es ihn liefert (Android/Chrome).
    pub battery: Option<TabletBattery>,
    /// Verletzung/Behandlung läuft an diesem Court.
    pub injury: bool,
    /// Die Turnierleitung wurde an diesen Court gerufen.
    pub official_call: bool,
    /// Welches Team schlägt gerade auf? 1 = team1, 2 = team2, None =
    /// unbekannt. Abgeleitet aus dem Tablet-`court_state`.
    pub serving_team: Option<u8>,
    /// Index (0/1) des konkret aufschlagenden Spielers innerhalb seines
    /// Teams (BWF-Doppelregel; vom Tablet berechnet). None bei Einzel oder
    /// altem Tablet-Stand ohne diese Info.
    pub serving_player: Option<u8>,
    /// Voraussichtlicher Zähltafelbediener für das aktuelle Spiel: die
    /// Namen des Verlierer-Teams des zuletzt auf diesem Feld beendeten
    /// Spiels. Leer, wenn es kein Vorspiel auf dem Feld gab.
    pub scorekeeper: Vec<String>,
    /// Feld vom Operator gesperrt (bts-light-seitig): wird nicht automatisch
    /// belegt und im UI rot markiert. BTP kennt keinen Sperr-Zustand.
    pub locked: bool,
}

/// Ein noch nicht gespieltes Match, das nach einer Aufgabe kampflos
/// (Walkover) für den Gegner gewertet werden kann.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WalkoverCandidate {
    /// BTP-Match-ID.
    pub match_id: i64,
    /// Draw des Matches (`Match.DrawID`) – fürs Zurückschreiben nach BTP.
    pub draw_id: i64,
    /// Planungsposition im Draw (`Match.PlanningID`).
    pub planning_id: i64,
    /// Runden-/Spielbezeichnung, z. B. "G3".
    pub round_name: String,
    /// Anzeigename des Gegners, der den kampflosen Sieg erhielte.
    pub opponent: String,
    /// Steht die aufgebende Mannschaft auf Seite 1 des Matches? Bestimmt
    /// den Sieger des Walkovers (immer die jeweils andere Seite).
    pub retired_is_team1: bool,
}

/// Vorschlag, nach einer Aufgabe die restlichen Spiele derselben
/// Mannschaft in derselben Disziplin kampflos zu werten. Die konkreten
/// Kandidaten-Spiele werden bei Bedarf frisch aus dem Snapshot ermittelt
/// ([`TabletState::walkover_candidates`]) – so fallen bereits gewertete
/// Spiele von selbst heraus.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WalkoverProposal {
    /// Stabile ID des Vorschlags (= EntryID der aufgebenden Mannschaft).
    pub id: String,
    /// EntryID der aufgebenden Mannschaft.
    pub entry_id: i64,
    /// Anzeigename der aufgebenden Mannschaft.
    pub retired_team: String,
    /// Name der Disziplin/Auslosung, in der aufgegeben wurde, z. B. "HE".
    pub draw_name: String,
    /// Zeitpunkt der Aufgabe (Unix-Millisekunden).
    pub created_at_ms: u64,
}

/// Ein von der Turnierleitung „in Vorbereitung" gerufenes Spiel. BTP kennt
/// keinen Vorbereitungs-Zustand – bts-light verwaltet ihn selbst, genau wie
/// die Walkover-Vorschläge. Je Match gibt es höchstens einen Aufruf.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreparationCall {
    /// BTP-Match-ID des gerufenen Spiels.
    pub match_id: i64,
    /// LocationID der Halle, für die gerufen wurde; `None` bei einem
    /// hallenunabhängigen Aufruf (Ein-Hallen-Turnier).
    pub location_id: Option<i64>,
    /// Zeitpunkt des Aufrufs (Unix-Millisekunden).
    pub called_at_ms: u64,
}

/// Geteilt zwischen Sync-Loop und Tablet-Server (`Arc<TabletState>`).
#[derive(Default)]
pub struct TabletState {
    snapshot: RwLock<Option<BtpSnapshot>>,
    /// CourtID → laufende Tablet-Session des Felds.
    courts: RwLock<HashMap<i64, CourtSession>>,
    /// CourtID → Token des aktuell schiedsenden Tablets (LAN-Tablet-
    /// Übernahme). Fehlt der Eintrag, ist der Court frei.
    active: RwLock<HashMap<i64, u64>>,
    /// Fortlaufender Zähler, vergibt eindeutige Court-Tokens.
    token_seq: AtomicU64,
    /// CourtID → gespiegelter Spielzustand (JSON) des aktiven Tablets –
    /// wird einem übernehmenden Gerät übergeben.
    court_state: RwLock<HashMap<i64, String>>,
    /// Offene Walkover-Vorschläge nach Aufgaben (je EntryID höchstens einer).
    walkovers: RwLock<Vec<WalkoverProposal>>,
    /// „In Vorbereitung" gerufene Spiele (je Match-ID höchstens einer).
    preparation_calls: RwLock<Vec<PreparationCall>>,
    /// Geräte-ID → Live-Zustand der Court-Monitore (zuletzt gesehen +
    /// offener Fernbefehl). Im LAN-Modus vom Server gepflegt.
    monitor_live: RwLock<HashMap<String, MonitorLive>>,
    /// Im Cloud-Modus die vom Relay gemeldete Monitor-Geräteliste – der
    /// Relay-Client hält sie aktuell, die „Court-Monitore"-Seite liest sie.
    relay_monitor_devices: RwLock<Vec<MonitorDeviceInfo>>,
    /// CourtID → Namen des Verlierer-Teams des zuletzt auf diesem Feld
    /// beendeten Spiels (= voraussichtlicher Zähltafelbediener fürs nächste
    /// Spiel). Vom Sync-Loop beim Übergang OnCourt→Finished gepflegt, weil
    /// BTP beendete Spiele nicht zuverlässig dem Feld zugeordnet behält.
    scorekeeper_by_court: RwLock<HashMap<i64, Vec<String>>>,
    /// Pfad der `live-scores.json` (CourtID → Match-ID + Satzstand). Beim
    /// Start gesetzt; jeder `record_score`/`clear_court` schreibt die Datei,
    /// damit ein App-Neustart den laufenden Live-Stand nicht verliert (sonst
    /// fiele der TV auf BTPs 0:0 zurück). `None` = Persistenz aus.
    scores_path: RwLock<Option<PathBuf>>,
    /// Serialisiert die Schreibvorgänge auf `live-scores.json` – mehrere
    /// Felder können (LAN, mehrere WS-Handler) gleichzeitig zählen; ohne das
    /// Lock könnten sich die Schreiber gegenseitig die Datei abschneiden.
    scores_persist_lock: Mutex<()>,
    /// Vom Operator gesperrte Felder (CourtID). bts-light-seitig (BTP kennt das
    /// nicht): gesperrte Felder werden nicht automatisch belegt und rot
    /// markiert. Beim Start aus der Config geseedet, bei Änderung persistiert.
    locked_courts: RwLock<HashSet<i64>>,
}

/// Auf Platte gesicherter Live-Stand eines Felds (für den App-Neustart).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedScore {
    match_id: i64,
    sets: Vec<(i64, i64)>,
}

impl TabletState {
    /// Den neuesten BTP-Snapshot ablegen (vom Sync-Loop aufgerufen).
    pub fn set_snapshot(&self, snapshot: BtpSnapshot) {
        *self.snapshot.write().unwrap() = Some(snapshot);
    }

    /// Merkt den Zähltafelbediener (Verlierer-Team-Namen) für ein Feld.
    /// Vom Sync-Loop beim Spielende auf dem Feld gesetzt.
    pub fn set_scorekeeper(&self, court_id: i64, loser_names: Vec<String>) {
        self.scorekeeper_by_court
            .write()
            .unwrap()
            .insert(court_id, loser_names);
    }

    /// Gesperrte Felder beim Start aus der Config übernehmen.
    pub fn set_locked_courts(&self, ids: impl IntoIterator<Item = i64>) {
        *self.locked_courts.write().unwrap() = ids.into_iter().collect();
    }

    /// Feld sperren (`true`) oder entsperren (`false`).
    pub fn set_court_locked(&self, court_id: i64, locked: bool) {
        let mut set = self.locked_courts.write().unwrap();
        if locked {
            set.insert(court_id);
        } else {
            set.remove(&court_id);
        }
    }

    /// Aktuelle Sperrliste (für Persistenz + Auto-Vergabe).
    pub fn locked_courts(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.locked_courts.read().unwrap().iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// Ist das Feld gesperrt?
    pub fn is_court_locked(&self, court_id: i64) -> bool {
        self.locked_courts.read().unwrap().contains(&court_id)
    }

    /// Voraussichtlicher Zähltafelbediener eines Felds (leer, wenn keiner).
    pub fn scorekeeper(&self, court_id: i64) -> Vec<String> {
        self.scorekeeper_by_court
            .read()
            .unwrap()
            .get(&court_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Kopie des aktuellen BTP-Snapshots (oder `None`, falls noch keiner
    /// geladen ist) – für Commands, die den Stand frisch auswerten.
    pub fn snapshot_clone(&self) -> Option<BtpSnapshot> {
        self.snapshot.read().unwrap().clone()
    }

    /// Turniername des aktuellen Snapshots (leer, falls noch keiner geladen
    /// ist) – für die Leerlauf-Anzeige des Court-Monitors.
    pub fn tournament_name(&self) -> String {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.tournament_name.clone())
            .unwrap_or_default()
    }

    /// Alle Court-Namen des Turniers (BTP-Reihenfolge) – nur für Tests und
    /// Anzeigen, die keine Identität brauchen. Adressen/QR-Codes nutzen
    /// [`TabletState::courts`].
    pub fn court_names(&self) -> Vec<String> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_infos.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Alle Felder des Turniers mit Identität (CourtID) und Anzeigenamen –
    /// Grundlage der Tablet-Adressen, QR-Codes und Monitor-Zuordnungen.
    pub fn courts(&self) -> Vec<BtpCourt> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_infos.clone())
            .unwrap_or_default()
    }

    /// CourtID → Feldname aller Felder (für die Monitor-Geräteliste).
    pub fn court_name_map(&self) -> HashMap<i64, String> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| {
                s.court_infos
                    .iter()
                    .map(|c| (c.id, c.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Anzeige-Bezeichnung eines Felds für Monitore und Tablets. Bei einem
    /// Mehr-Hallen-Turnier `"{Halle} · {Feld}"`, sonst nur der Feldname.
    /// Leer, wenn die CourtID kein bekanntes Feld ist.
    pub fn court_display_label(&self, court_id: i64) -> String {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_display_label(court_id))
            .unwrap_or_default()
    }

    /// Das Match, das BTP gerade diesem Feld (per CourtID) zugewiesen hat.
    pub fn match_for_court(&self, court_id: i64) -> Option<BtpMatch> {
        let guard = self.snapshot.read().unwrap();
        let snap = guard.as_ref()?;
        snap.matches
            .iter()
            .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court_id))
            .cloned()
    }

    /// Tablet hat sich für ein Feld verbunden. `match_id` startet auf 0 –
    /// den echten Wert setzt der erste `record_score`.
    pub fn attach_tablet(&self, court_id: i64) {
        self.courts
            .write()
            .unwrap()
            .entry(court_id)
            .or_insert(CourtSession {
                match_id: 0,
                sets: Vec::new(),
                connected: true,
                battery: None,
                injury: false,
                official: false,
            })
            .connected = true;
    }

    /// Tablet-WebSocket für ein Feld ist geschlossen.
    pub fn detach_tablet(&self, court_id: i64) {
        if let Some(session) = self.courts.write().unwrap().get_mut(&court_id) {
            session.connected = false;
        }
    }

    /// Satzstand vom Tablet übernehmen.
    pub fn record_score(&self, court_id: i64, match_id: i64, sets: Vec<(i64, i64)>) {
        {
            let mut courts = self.courts.write().unwrap();
            let session = courts.entry(court_id).or_insert(CourtSession {
                match_id,
                sets: Vec::new(),
                connected: true,
                battery: None,
                injury: false,
                official: false,
            });
            session.match_id = match_id;
            session.sets = sets;
        }
        // Stand auf Platte sichern, damit ein App-Neustart ihn behält.
        self.persist_scores();
    }

    /// Pfad der Live-Score-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_scores_path(&self, path: PathBuf) {
        *self.scores_path.write().unwrap() = Some(path);
    }

    /// Live-Stände beim Start aus der Datei laden. Die wiederhergestellten
    /// Sessions sind `connected: false` (kein Tablet-WebSocket offen) – der
    /// Stand wird trotzdem angezeigt/gepusht (siehe `apply_tablet_scores`),
    /// bis das Tablet zurückkehrt oder das Match wechselt.
    pub fn load_scores(&self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return; // keine Datei (erster Start) → nichts zu tun
        };
        let Ok(data) = serde_json::from_str::<HashMap<i64, PersistedScore>>(&text) else {
            tracing::warn!("live-scores.json unlesbar – ignoriere");
            return;
        };
        let mut courts = self.courts.write().unwrap();
        for (court_id, ps) in data {
            courts.entry(court_id).or_insert(CourtSession {
                match_id: ps.match_id,
                sets: ps.sets,
                connected: false,
                battery: None,
                injury: false,
                official: false,
            });
        }
    }

    /// Aktuellen Live-Stand aller Felder in die Datei schreiben (best effort:
    /// Schreibfehler dürfen das Zählen nie stören). No-op, wenn kein Pfad
    /// gesetzt ist (z. B. in Tests).
    fn persist_scores(&self) {
        let Some(path) = self.scores_path.read().unwrap().clone() else {
            return;
        };
        // Schreiber serialisieren: verhindert, dass zwei gleichzeitige
        // record_score den Temp-Pfad oder die Zieldatei gegenseitig zerlegen.
        let _guard = self.scores_persist_lock.lock().unwrap();
        let data: HashMap<i64, PersistedScore> = {
            let courts = self.courts.read().unwrap();
            courts
                .iter()
                .filter(|(_, s)| !s.sets.is_empty())
                .map(|(c, s)| {
                    (
                        *c,
                        PersistedScore {
                            match_id: s.match_id,
                            sets: s.sets.clone(),
                        },
                    )
                })
                .collect()
        };
        if let Ok(json) = serde_json::to_string(&data) {
            // Atomar schreiben: erst in eine Temp-Datei, dann umbenennen –
            // so liegt nie eine halb geschriebene live-scores.json vor (ein
            // Absturz mitten im Schreiben würde sie sonst korrumpieren).
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Akkustand des Tablets an einem Feld übernehmen.
    pub fn record_battery(&self, court_id: i64, percent: i64, charging: bool) {
        let mut courts = self.courts.write().unwrap();
        courts
            .entry(court_id)
            .or_insert(CourtSession {
                match_id: 0,
                sets: Vec::new(),
                connected: true,
                battery: None,
                injury: false,
                official: false,
            })
            .battery = Some(TabletBattery { percent, charging });
    }

    /// Meldungs-Zustand (Verletzung / Turnierleitung gerufen) des Felds setzen.
    pub fn record_alert(&self, court_id: i64, injury: bool, official: bool) {
        let mut courts = self.courts.write().unwrap();
        let session = courts.entry(court_id).or_insert(CourtSession {
            match_id: 0,
            sets: Vec::new(),
            connected: true,
            battery: None,
            injury: false,
            official: false,
        });
        session.injury = injury;
        session.official = official;
    }

    /// Beansprucht das Feld für ein Tablet und gibt dessen Token zurück.
    /// Ein bereits aktives Tablet wird dadurch abgelöst (Tablet-Übernahme).
    pub fn claim_court(&self, court_id: i64) -> u64 {
        let token = self.token_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.active.write().unwrap().insert(court_id, token);
        token
    }

    /// Ist `token` noch das aktive Tablet dieses Felds?
    pub fn is_court_active(&self, court_id: i64, token: u64) -> bool {
        self.active.read().unwrap().get(&court_id) == Some(&token)
    }

    /// Wird das Feld bereits von einem Tablet geschiedst?
    pub fn court_occupied(&self, court_id: i64) -> bool {
        self.active.read().unwrap().contains_key(&court_id)
    }

    /// Gibt das Feld frei – nur, wenn `token` noch der aktive ist.
    pub fn release_court(&self, court_id: i64, token: u64) {
        let mut active = self.active.write().unwrap();
        if active.get(&court_id) == Some(&token) {
            active.remove(&court_id);
        }
    }

    /// Spiegelt den Spielzustand des aktiven Tablets am Feld.
    pub fn set_court_state(&self, court_id: i64, state: String) {
        self.court_state.write().unwrap().insert(court_id, state);
    }

    /// Liefert den gespiegelten Spielzustand eines Felds (für die Übernahme).
    pub fn court_state(&self, court_id: i64) -> Option<String> {
        self.court_state.read().unwrap().get(&court_id).cloned()
    }

    /// Court-Session entfernen (nach übermitteltem Ergebnis).
    pub fn clear_court(&self, court_id: i64) {
        self.courts.write().unwrap().remove(&court_id);
        // Entfernten Stand auch aus der Datei nehmen.
        self.persist_scores();
    }

    /// Hinterlegt einen Walkover-Vorschlag. Je EntryID gibt es höchstens
    /// einen – ein erneuter für dieselbe Mannschaft ersetzt den alten.
    pub fn add_walkover_proposal(&self, proposal: WalkoverProposal) {
        let mut list = self.walkovers.write().unwrap();
        list.retain(|p| p.entry_id != proposal.entry_id);
        list.push(proposal);
    }

    /// Alle offenen Walkover-Vorschläge.
    pub fn walkover_proposals(&self) -> Vec<WalkoverProposal> {
        self.walkovers.read().unwrap().clone()
    }

    /// Entfernt einen Walkover-Vorschlag (umgesetzt oder verworfen).
    pub fn remove_walkover_proposal(&self, id: &str) {
        self.walkovers.write().unwrap().retain(|p| p.id != id);
    }

    /// Noch nicht gespielte Matches einer Mannschaft (per EntryID) – die
    /// Kandidaten für eine kampflose Wertung nach deren Aufgabe. Nur Spiele
    /// mit bereits feststehendem Gegner; offene KO-Plätze bleiben außen vor.
    pub fn walkover_candidates(&self, entry_id: i64) -> Vec<WalkoverCandidate> {
        if entry_id == 0 {
            return Vec::new();
        }
        let guard = self.snapshot.read().unwrap();
        let Some(snap) = guard.as_ref() else {
            return Vec::new();
        };
        snap.matches
            .iter()
            .filter(|m| m.status == MatchStatus::Scheduled)
            .filter_map(|m| {
                let retired_is_team1 = m.entry1_id == entry_id;
                if !retired_is_team1 && m.entry2_id != entry_id {
                    return None;
                }
                let opponent = if retired_is_team1 { &m.team2 } else { &m.team1 };
                if opponent.is_empty() {
                    return None; // Gegner steht noch nicht fest
                }
                Some(WalkoverCandidate {
                    match_id: m.id,
                    draw_id: m.draw_id,
                    planning_id: m.planning_id,
                    round_name: m.round_name.clone(),
                    opponent: opponent
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    retired_is_team1,
                })
            })
            .collect()
    }

    // ─────────────────────────── Spiele in Vorbereitung ───────────────────

    /// Hinterlegt einen „in Vorbereitung"-Aufruf. Je Match-ID gibt es
    /// höchstens einen – ein erneuter für dasselbe Match ersetzt den alten.
    pub fn add_preparation_call(&self, call: PreparationCall) {
        let mut list = self.preparation_calls.write().unwrap();
        list.retain(|c| c.match_id != call.match_id);
        list.push(call);
    }

    /// Alle aktuell gerufenen „in Vorbereitung"-Spiele.
    pub fn preparation_calls(&self) -> Vec<PreparationCall> {
        self.preparation_calls.read().unwrap().clone()
    }

    /// Entfernt den Aufruf eines Matches (zurückgenommen).
    pub fn remove_preparation_call(&self, match_id: i64) {
        self.preparation_calls
            .write()
            .unwrap()
            .retain(|c| c.match_id != match_id);
    }

    /// Stempelt die aktiven Vorbereitungs-Aufrufe in den Snapshot. Aufrufe,
    /// deren Match nicht mehr ruf-bar ist (auf Court gewechselt, beendet,
    /// verschwunden oder eine Mannschaft nicht mehr gesetzt), werden dabei
    /// verworfen – so bleiben keine Geister-Aufrufe stehen. Für jeden
    /// überlebenden Aufruf werden die transienten Felder
    /// `preparation_call_ts` und `preparation_hall` des zugehörigen Matches
    /// gesetzt.
    pub fn apply_preparation_calls(&self, snapshot: &mut BtpSnapshot) {
        let mut calls = self.preparation_calls.write().unwrap();
        // Match-IDs, die im Snapshot noch ruf-bar sind: eingeplant und mit
        // zwei feststehenden Mannschaften – dieselbe Bedingung wie die
        // Kandidaten-Liste, damit kein Aufruf ohne sichtbares Match bleibt.
        let callable: std::collections::HashSet<i64> = snapshot
            .matches
            .iter()
            .filter(|m| {
                m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
            })
            .map(|m| m.id)
            .collect();
        // Aufrufe ohne (noch) ruf-bares Match fallen heraus.
        calls.retain(|c| callable.contains(&c.match_id));
        for call in calls.iter() {
            // Hallenname aus der LocationID auflösen (None → kein Halleneintrag).
            let hall = call.location_id.and_then(|lid| {
                snapshot
                    .locations
                    .iter()
                    .find(|l| l.id == lid)
                    .map(|l| l.name.clone())
            });
            if let Some(m) = snapshot.matches.iter_mut().find(|m| m.id == call.match_id) {
                m.preparation_call_ts = Some(call.called_at_ms);
                m.preparation_hall = hall;
            }
        }
    }

    /// Felder (CourtIDs) mit verbundenem Tablet – diese treiben ihren
    /// Live-Score selbst.
    pub fn active_courts(&self) -> Vec<i64> {
        self.courts
            .read()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.connected)
            .map(|(c, _)| *c)
            .collect()
    }

    /// Überschreibt im Snapshot die Sätze jedes tablet-getriebenen Matches
    /// mit dem Tablet-Stand. So pusht die Liveticker-Pipeline den
    /// Tablet-Score statt BTPs veraltetem Poll-Wert. Greift, sobald eine
    /// Session zum selben Match einen Stand hat – BEWUSST OHNE
    /// `connected`-Prüfung: Ein kurzer WebSocket-Aussetzer (Router weg,
    /// Display gesperrt) oder ein App-Neustart (Stand aus `live-scores.json`
    /// wiederhergestellt, Tablet noch nicht zurück) darf den Liveticker
    /// nicht auf BTPs 0:0 zurückwerfen. `match_id == m.id` schützt gegen
    /// Match-Wechsel, `!is_empty()` gegen das Überschreiben mit Leerstand.
    pub fn apply_tablet_scores(&self, snapshot: &mut BtpSnapshot) {
        let courts = self.courts.read().unwrap();
        for m in &mut snapshot.matches {
            let Some(court_id) = m.court_id else {
                continue;
            };
            if let Some(session) = courts.get(&court_id) {
                if session.match_id == m.id && !session.sets.is_empty() {
                    m.sets = session.sets.clone();
                }
            }
        }
    }

    /// Felder-Übersicht für die Turnierleitung – je Court das aktuelle
    /// Match mit Live-Satzstand und Tablet-Status.
    pub fn overview(&self) -> Vec<CourtOverview> {
        let guard = self.snapshot.read().unwrap();
        let Some(snap) = guard.as_ref() else {
            return Vec::new();
        };
        let courts = self.courts.read().unwrap();
        snap.court_infos
            .iter()
            .map(|court| {
                let m = snap
                    .matches
                    .iter()
                    .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court.id));
                let session = courts.get(&court.id);
                let tablet_connected = session.map(|s| s.connected).unwrap_or(false);
                // Satzstand vom Tablet, sobald dessen Session zum selben Match
                // einen Stand hat – BEWUSST OHNE `connected`-Prüfung (wie
                // `monitor_court`/`apply_tablet_scores`): ein kurzer Aussetzer
                // oder ein App-Neustart darf die Übersicht nicht auf BTPs 0:0
                // zurückwerfen. `tablet_connected` bleibt rein der Online-Indikator.
                let sets = match (session, m) {
                    (Some(s), Some(mm)) if s.match_id == mm.id && !s.sets.is_empty() => {
                        s.sets.clone()
                    }
                    (_, Some(mm)) => mm.sets.clone(),
                    _ => Vec::new(),
                };
                let nationalities = |team: &[crate::btp::model::BtpPlayer]| {
                    team.iter()
                        .map(|p| p.nationality.clone().unwrap_or_default())
                        .collect::<Vec<String>>()
                };
                // Aufschlag-Info aus dem Tablet-court_state: (team 1/2,
                // optional Spieler-Index 0/1). Bevorzugt das Tablet-berechnete
                // `serving:{team,index}`; Fallback auf servingSide/teamOnSide.
                let serving_info: Option<(u8, Option<u8>)> = self
                    .court_state
                    .read()
                    .unwrap()
                    .get(&court.id)
                    .and_then(|cs| {
                        let v: serde_json::Value = serde_json::from_str(cs).ok()?;
                        if let Some(s) = v.get("serving").filter(|s| !s.is_null()) {
                            let team = if s.get("team")?.as_str()? == "a" {
                                1u8
                            } else {
                                2u8
                            };
                            let idx = s.get("index").and_then(|i| i.as_u64()).map(|i| i as u8);
                            return Some((team, idx));
                        }
                        // Fallback (altes Tablet ohne `serving`): nur Team.
                        let serving = v.get("servingSide")?.as_str()?;
                        let team_a = v.get("teamOnSide")?.get("a")?.as_str()?;
                        Some((if serving == team_a { 1u8 } else { 2u8 }, None))
                    });
                CourtOverview {
                    court_id: court.id,
                    court: court.name.clone(),
                    // Hallenname nur bei Mehr-Hallen-Turnieren; sonst leer.
                    location: snap.court_location_name(court.id),
                    match_id: m.map(|mm| mm.id).unwrap_or(0),
                    match_name: m
                        .map(|mm| {
                            format!("{} {}", mm.draw_name, mm.round_name)
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default(),
                    discipline: m.map(|mm| mm.discipline).unwrap_or(Discipline::Unknown),
                    team1: m
                        .map(|mm| mm.team1.iter().map(|p| p.name.clone()).collect())
                        .unwrap_or_default(),
                    team2: m
                        .map(|mm| mm.team2.iter().map(|p| p.name.clone()).collect())
                        .unwrap_or_default(),
                    team1_nationalities: m.map(|mm| nationalities(&mm.team1)).unwrap_or_default(),
                    team2_nationalities: m.map(|mm| nationalities(&mm.team2)).unwrap_or_default(),
                    sets,
                    tablet_connected,
                    battery: session.and_then(|s| s.battery),
                    injury: session.map(|s| s.injury).unwrap_or(false),
                    official_call: session.map(|s| s.official).unwrap_or(false),
                    // Aufschlagendes Team + konkreter Spieler aus dem
                    // Tablet-court_state. Bevorzugt das vom Tablet berechnete
                    // `serving: {team, index}` (BWF-Doppelregel, Spieler-genau);
                    // fällt sonst auf die servingSide/teamOnSide-Ableitung
                    // zurück (nur Team, für alte Tablet-Stände).
                    serving_team: serving_info.map(|(t, _)| t),
                    serving_player: serving_info.and_then(|(_, p)| p),
                    // Zähltafelbediener = Verlierer des zuletzt auf diesem
                    // Feld beendeten Spiels. Wird vom Sync-Loop getrackt
                    // (BTP behält die Feld-Zuordnung beendeter Spiele nicht
                    // zuverlässig). Nur zeigen, wenn gerade ein Spiel läuft.
                    scorekeeper: if m.is_some() {
                        self.scorekeeper_by_court
                            .read()
                            .unwrap()
                            .get(&court.id)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    },
                    locked: self.locked_courts.read().unwrap().contains(&court.id),
                }
            })
            .collect()
    }

    /// Monitor-relevante Daten eines Feldes: das aktuelle Match mit
    /// effektivem Satzstand (Tablet-getrieben falls aktiv, sonst aus BTP)
    /// und der gespiegelte Tablet-Spielzustand (Aufschlag/Pause). Vom
    /// Court-Monitor-Endpunkt genutzt.
    pub fn monitor_court(&self, court_id: i64) -> MonitorCourt {
        let guard = self.snapshot.read().unwrap();
        let tournament_name = guard
            .as_ref()
            .map(|s| s.tournament_name.clone())
            .unwrap_or_default();
        let current_match = guard.as_ref().and_then(|snap| {
            snap.matches
                .iter()
                .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court_id))
                .cloned()
        });
        drop(guard);
        // Satzstand vom Tablet, sobald dessen Session zum selben Match
        // gehört – bewusst OHNE `connected`-Prüfung: ein kurzer Tablet-
        // Aussetzer (Browser zu, Display gesperrt) soll den Monitor nicht
        // auf 0:0 zurückwerfen; der zuletzt bekannte Stand bleibt stehen.
        let sets = match &current_match {
            Some(mm) => {
                let courts = self.courts.read().unwrap();
                match courts.get(&court_id) {
                    Some(s) if s.match_id == mm.id && !s.sets.is_empty() => s.sets.clone(),
                    _ => mm.sets.clone(),
                }
            }
            None => Vec::new(),
        };
        MonitorCourt {
            tournament_name,
            current_match,
            sets,
            court_state: self.court_state(court_id),
        }
    }

    // ─────────────────────────── Court-Monitor-Geräte ─────────────────────

    /// Registriert einen Monitor-Poll (setzt „zuletzt gesehen") und liefert
    /// den offenen Fernbefehl des Geräts zurück. Bei erreichter Obergrenze
    /// wird das am längsten nicht gesehene Gerät verdrängt.
    pub fn record_monitor_poll(&self, device_id: &str) -> Option<MonitorCommand> {
        let mut live = self.monitor_live.write().unwrap();
        if !live.contains_key(device_id) && live.len() >= MAX_MONITOR_DEVICES {
            if let Some(oldest) = live
                .iter()
                .min_by_key(|(_, l)| l.last_seen_ms)
                .map(|(id, _)| id.clone())
            {
                live.remove(&oldest);
            }
        }
        let entry = live.entry(device_id.to_string()).or_default();
        entry.last_seen_ms = now_ms();
        entry.command
    }

    /// Hinterlegt einen Fernbefehl für ein Gerät. Die `id` zählt je Gerät
    /// hoch, damit der Monitor jeden Befehl genau einmal ausführt.
    pub fn set_monitor_command(&self, device_id: &str, kind: MonitorCommandKind) {
        let mut live = self.monitor_live.write().unwrap();
        let entry = live.entry(device_id.to_string()).or_default();
        let next_id = entry.command.map(|c| c.id + 1).unwrap_or(1);
        entry.command = Some(MonitorCommand { id: next_id, kind });
    }

    /// Ist das Gerät aktuell online (letzter Poll innerhalb des
    /// Online-Fensters)? Unbekannte Geräte gelten als offline.
    pub fn is_monitor_online(&self, device_id: &str, now_ms: u64) -> bool {
        self.monitor_live
            .read()
            .unwrap()
            .get(device_id)
            .map(|l| now_ms.saturating_sub(l.last_seen_ms) <= relay_proto::MONITOR_ONLINE_WINDOW_MS)
            .unwrap_or(false)
    }

    /// Entfernt ein Gerät aus dem Live-State (vergisst es). Damit
    /// verschwindet es aus der Geräteliste, sofern es auch keine
    /// Zuweisung mehr hat (die räumt der Aufrufer separat ab).
    pub fn forget_monitor(&self, device_id: &str) {
        self.monitor_live.write().unwrap().remove(device_id);
    }

    /// Geräte-ID → letzter Poll (ms) aller bekannten Monitor-Geräte.
    pub fn monitor_live_seen(&self) -> HashMap<String, u64> {
        self.monitor_live
            .read()
            .unwrap()
            .iter()
            .map(|(id, l)| (id.clone(), l.last_seen_ms))
            .collect()
    }

    /// Geräte-ID → offener Fernbefehl (für den Cloud-Push zum Relay).
    pub fn monitor_commands(&self) -> HashMap<String, MonitorCommand> {
        self.monitor_live
            .read()
            .unwrap()
            .iter()
            .filter_map(|(id, l)| l.command.map(|c| (id.clone(), c)))
            .collect()
    }

    /// Übernimmt die vom Relay gemeldete Geräteliste (Cloud-Modus).
    pub fn set_relay_monitor_devices(&self, devices: Vec<MonitorDeviceInfo>) {
        *self.relay_monitor_devices.write().unwrap() = devices;
    }

    /// Vom Relay gemeldete Monitor-Geräteliste (Cloud-Modus).
    pub fn relay_monitor_devices(&self) -> Vec<MonitorDeviceInfo> {
        self.relay_monitor_devices.read().unwrap().clone()
    }
}

/// Monitor-relevante Daten eines Feldes (Rückgabe von
/// [`TabletState::monitor_court`]). Reiner Transport – nicht serialisiert.
pub struct MonitorCourt {
    /// Turniername (für die Werbe-/Leerlauf-Anzeige).
    pub tournament_name: String,
    /// Aktuelles Match auf dem Feld, falls eines zugewiesen ist.
    pub current_match: Option<BtpMatch>,
    /// Effektiver Satzstand (Tablet-getrieben falls aktiv, sonst BTP).
    pub sets: Vec<(i64, i64)>,
    /// Gespiegelter Tablet-Spielzustand (JSON-String), falls vorhanden.
    pub court_state: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpPlayer, MatchResult};

    fn player(name: &str) -> BtpPlayer {
        BtpPlayer {
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: None,
        }
    }

    /// Baut ein Match, das (per CourtID) einem Feld zugewiesen ist. `court`
    /// ist die CourtID des Felds (`None` = kein Feld).
    fn match_on(id: i64, court: Option<i64>, status: MatchStatus) -> BtpMatch {
        BtpMatch {
            id,
            draw_id: 1,
            planning_id: 1000 + id,
            draw_name: "HE".to_string(),
            discipline: Discipline::MensSingles,
            round_name: "G1".to_string(),
            match_num: Some(id),
            team1: vec![player("Anna")],
            team2: vec![player("Ben")],
            entry1_id: 10,
            entry2_id: 20,
            // Court-Name spielt für die Identität keine Rolle – die
            // CourtID ist maßgeblich. Wir setzen einen Platzhalter-Namen.
            court: court.map(|cid| format!("C{cid}")),
            court_id: court,
            sets: vec![(5, 3)],
            winner: None,
            result: MatchResult::Normal,
            status,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
        }
    }

    /// Baut einen Snapshot. `courts` ist eine Liste `(CourtID, Feldname)`.
    fn snapshot(matches: Vec<BtpMatch>, courts: Vec<(i64, &str)>) -> BtpSnapshot {
        let court_infos: Vec<BtpCourt> = courts
            .iter()
            .enumerate()
            .map(|(i, (id, name))| BtpCourt {
                id: *id,
                name: name.to_string(),
                location_id: Some(1),
                sort_order: i as i64,
            })
            .collect();
        BtpSnapshot {
            tournament_name: "T".to_string(),
            matches,
            courts: courts.into_iter().map(|(_, n)| n.to_string()).collect(),
            locations: Vec::new(),
            court_infos,
        }
    }

    #[test]
    fn match_for_court_finds_the_on_court_match() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        assert_eq!(st.match_for_court(101).unwrap().id, 1);
        assert!(st.match_for_court(102).is_none());
    }

    /// Regression Mehr-Hallen-Turnier: zwei Felder heißen beide „1", haben
    /// aber verschiedene CourtIDs. Ein Tablet auf dem einen Feld darf den
    /// anderen Court nicht beeinflussen – ohne CourtID-Keying kollidierten
    /// beide auf demselben Namen.
    #[test]
    fn courts_with_same_name_but_different_id_do_not_collide() {
        let st = TabletState::default();
        // Halle 1 · Feld „1" (CourtID 101) und Halle 2 · Feld „1" (CourtID 401).
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, Some(401), MatchStatus::OnCourt),
            ],
            vec![(101, "1"), (401, "1")],
        ));
        // Jedes Feld findet sein eigenes Match.
        assert_eq!(st.match_for_court(101).unwrap().id, 1);
        assert_eq!(st.match_for_court(401).unwrap().id, 2);
        // Tablet bindet sich nur an Feld 101 und zählt dort.
        st.attach_tablet(101);
        st.record_score(101, 1, vec![(21, 5)]);
        // Feld 401 bleibt unberührt: keine Session, kein Satzstand.
        assert_eq!(st.active_courts(), vec![101]);
        let ov = st.overview();
        let c101 = ov.iter().find(|o| o.court_id == 101).unwrap();
        let c401 = ov.iter().find(|o| o.court_id == 401).unwrap();
        assert!(c101.tablet_connected);
        assert_eq!(c101.sets, vec![(21, 5)]);
        assert!(!c401.tablet_connected);
        assert_eq!(c401.sets, vec![(5, 3)]); // BTP-Stand, kein Tablet
                                             // Beide Felder tragen denselben Anzeigenamen.
        assert_eq!(c101.court, "1");
        assert_eq!(c401.court, "1");
    }

    #[test]
    fn apply_tablet_scores_overrides_only_active_matching_court() {
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.set_snapshot(snap.clone());
        st.record_score(101, 1, vec![(21, 19), (8, 6)]);
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 19), (8, 6)]);
    }

    #[test]
    fn apply_tablet_scores_ignores_session_for_other_match() {
        // Court hat inzwischen ein anderes Match – der Tablet-Stand darf
        // nicht aufs neue Match durchschlagen.
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(9, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.record_score(101, 1, vec![(21, 0)]);
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(5, 3)]);
    }

    #[test]
    fn detached_tablet_is_not_active() {
        let st = TabletState::default();
        st.attach_tablet(101);
        assert_eq!(st.active_courts(), vec![101]);
        st.detach_tablet(101);
        assert!(st.active_courts().is_empty());
    }

    #[test]
    fn overview_lists_each_court_with_its_match() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        st.record_score(101, 1, vec![(15, 12)]);
        st.attach_tablet(101);
        let ov = st.overview();
        assert_eq!(ov.len(), 2);
        let c1 = ov.iter().find(|o| o.court_id == 101).unwrap();
        assert_eq!(c1.court, "Court 1");
        assert_eq!(c1.team1, vec!["Anna".to_string()]);
        assert_eq!(c1.sets, vec![(15, 12)]);
        assert!(c1.tablet_connected);
        let c2 = ov.iter().find(|o| o.court_id == 102).unwrap();
        assert_eq!(c2.match_name, "");
        assert!(!c2.tablet_connected);
    }

    #[test]
    fn overview_fills_location_only_for_multi_hall_tournaments() {
        use crate::btp::model::BtpLocation;
        // Ein-Hallen-Turnier (snapshot()-Helfer setzt locations leer):
        // location bleibt überall leer.
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        for c in st.overview() {
            assert_eq!(c.location, "");
        }
        // Mehr-Hallen-Turnier: Feld 101 in „Halle 1", Feld 401 in „Halle 2".
        let mut snap = snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, Some(401), MatchStatus::OnCourt),
            ],
            vec![(101, "1"), (401, "1")],
        );
        snap.locations = vec![
            BtpLocation {
                id: 1,
                name: "Halle 1".to_string(),
            },
            BtpLocation {
                id: 2,
                name: "Halle 2".to_string(),
            },
        ];
        // court_infos[1] ist Feld 401 → der Halle 2 zuordnen.
        snap.court_infos[1].location_id = Some(2);
        st.set_snapshot(snap);
        let ov = st.overview();
        assert_eq!(
            ov.iter().find(|o| o.court_id == 101).unwrap().location,
            "Halle 1"
        );
        assert_eq!(
            ov.iter().find(|o| o.court_id == 401).unwrap().location,
            "Halle 2"
        );
        // Das Komposit-Label kombiniert Halle und Feldname.
        assert_eq!(st.court_display_label(401), "Halle 2 · 1");
    }

    #[test]
    fn monitor_court_returns_match_with_effective_sets() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        // Ohne Tablet: Satzstand aus BTP (match_on setzt sets = [(5,3)]).
        let mc = st.monitor_court(101);
        assert_eq!(mc.tournament_name, "T");
        assert_eq!(mc.current_match.as_ref().unwrap().id, 1);
        assert_eq!(mc.sets, vec![(5, 3)]);
        assert!(mc.court_state.is_none());
        // Mit Tablet-Score: der Satzstand kommt vom Tablet.
        st.record_score(101, 1, vec![(21, 19), (8, 4)]);
        assert_eq!(st.monitor_court(101).sets, vec![(21, 19), (8, 4)]);
        // Tablet getrennt (Browser zu): der zuletzt bekannte Stand bleibt
        // stehen – der Monitor fällt NICHT auf den BTP-Stand zurück.
        st.detach_tablet(101);
        assert_eq!(st.monitor_court(101).sets, vec![(21, 19), (8, 4)]);
        // Leeres Feld: kein Match.
        assert!(st.monitor_court(102).current_match.is_none());
    }

    #[test]
    fn tablet_score_is_trusted_even_when_disconnected() {
        // Regression: Ein kurzer WS-Aussetzer (connected=false) darf weder
        // den Liveticker-Push (apply_tablet_scores) noch die Übersicht
        // (overview) auf BTPs 0:0/Poll-Stand zurückwerfen.
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.set_snapshot(snap.clone());
        st.record_score(101, 1, vec![(21, 19), (8, 4)]);
        st.detach_tablet(101); // Browser zu / Netz weg → connected=false
                               // apply_tablet_scores überschreibt den BTP-Stand trotzdem.
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 19), (8, 4)]);
        // overview zeigt den Tablet-Stand, markiert das Tablet aber als offline.
        let ov = st.overview();
        let c = ov.iter().find(|o| o.court_id == 101).unwrap();
        assert_eq!(c.sets, vec![(21, 19), (8, 4)]);
        assert!(!c.tablet_connected);
    }

    #[test]
    fn live_scores_persist_and_reload_across_restart() {
        // Simuliert einen App-Neustart: Stand sichern, neue Instanz lädt ihn
        // und zeigt ihn (auch ohne verbundenes Tablet) statt BTPs 0:0.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live-scores.json");

        let st = TabletState::default();
        st.set_scores_path(path.clone());
        st.record_score(101, 7, vec![(21, 5), (2, 9)]);
        assert!(path.exists());

        // „Neustart": frische Instanz, gleiches Match noch OnCourt.
        let st2 = TabletState::default();
        st2.load_scores(&path);
        let mut snap = snapshot(
            vec![match_on(7, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st2.set_snapshot(snap.clone());
        // monitor_court + apply_tablet_scores liefern den wiederhergestellten Stand.
        assert_eq!(st2.monitor_court(101).sets, vec![(21, 5), (2, 9)]);
        st2.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 5), (2, 9)]);

        // clear_court entfernt den Stand auch aus der Datei.
        st2.set_scores_path(path.clone());
        st2.clear_court(101);
        let st3 = TabletState::default();
        st3.load_scores(&path);
        st3.set_snapshot(snapshot(
            vec![match_on(7, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        ));
        assert_eq!(st3.monitor_court(101).sets, vec![(5, 3)]); // wieder BTP-Stand
    }

    #[test]
    fn walkover_candidates_lists_scheduled_matches_of_the_entry() {
        let st = TabletState::default();
        // match_on setzt entry1_id = 10, entry2_id = 20.
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt), // läuft – kein Kandidat
                match_on(2, None, MatchStatus::Scheduled),
                match_on(3, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1")],
        ));
        let cands = st.walkover_candidates(10);
        let ids: Vec<i64> = cands.iter().map(|c| c.match_id).collect();
        assert_eq!(ids, vec![2, 3]);
        assert!(cands.iter().all(|c| c.retired_is_team1));
        assert_eq!(cands[0].opponent, "Ben");
        // Fremde Entry → keine Kandidaten; Entry 0 (unaufgelöst) ebenfalls.
        assert!(st.walkover_candidates(999).is_empty());
        assert!(st.walkover_candidates(0).is_empty());
    }

    /// Regression: Eine Aufgabe gilt NUR für die Disziplin, in der aufgegeben
    /// wurde (die EntryID ist pro Draw/Disziplin eindeutig). Annas Einzel und
    /// das andere Doppel ihrer Partnerin dürfen NICHT als Walkover-Kandidaten
    /// auftauchen, wenn das gemeinsame Herrendoppel aufgibt.
    #[test]
    fn walkover_candidates_stay_within_the_retiring_entrys_discipline() {
        // Übriges HD-Spiel des Paares "Anna / Cara" (EntryID 100) → Kandidat.
        let mut hd_next = match_on(1, None, MatchStatus::Scheduled);
        hd_next.draw_name = "HD".into();
        hd_next.discipline = Discipline::MensDoubles;
        hd_next.team1 = vec![player("Anna"), player("Cara")];
        hd_next.entry1_id = 100;
        hd_next.team2 = vec![player("Eva"), player("Fia")];
        hd_next.entry2_id = 110;

        // Annas Einzel (EntryID 200) → KEIN Kandidat (andere Disziplin/Entry).
        let mut he_anna = match_on(2, None, MatchStatus::Scheduled);
        he_anna.draw_name = "HE".into();
        he_anna.team1 = vec![player("Anna")];
        he_anna.entry1_id = 200;
        he_anna.team2 = vec![player("Gustav")];
        he_anna.entry2_id = 300;

        // Caras anderes Doppel (EntryID 400) → KEIN Kandidat.
        let mut cara_other = match_on(3, None, MatchStatus::Scheduled);
        cara_other.draw_name = "DD".into();
        cara_other.discipline = Discipline::WomensDoubles;
        cara_other.team1 = vec![player("Cara"), player("Hanna")];
        cara_other.entry1_id = 400;
        cara_other.team2 = vec![player("Ida"), player("Jana")];
        cara_other.entry2_id = 410;

        let st = TabletState::default();
        st.set_snapshot(snapshot(vec![hd_next, he_anna, cara_other], vec![]));

        // Aufgabe des HD-Paares (EntryID 100): nur dessen übriges HD-Spiel.
        let cands = st.walkover_candidates(100);
        let ids: Vec<i64> = cands.iter().map(|c| c.match_id).collect();
        assert_eq!(ids, vec![1]); // NICHT 2 (HE) und NICHT 3 (DD)
        assert!(cands[0].retired_is_team1);
        assert_eq!(cands[0].opponent, "Eva / Fia");

        // Gegenprobe: Annas Einzel-Entry ist davon unabhängig (nur ihr HE).
        let he_ids: Vec<i64> = st
            .walkover_candidates(200)
            .iter()
            .map(|c| c.match_id)
            .collect();
        assert_eq!(he_ids, vec![2]);
    }

    #[test]
    fn preparation_call_is_unique_per_match_and_removable() {
        let st = TabletState::default();
        let mk = |match_id: i64| PreparationCall {
            match_id,
            location_id: None,
            called_at_ms: 1000,
        };
        st.add_preparation_call(mk(5));
        st.add_preparation_call(mk(5)); // ersetzt – kein Duplikat
        assert_eq!(st.preparation_calls().len(), 1);
        st.add_preparation_call(mk(6));
        assert_eq!(st.preparation_calls().len(), 2);
        st.remove_preparation_call(5);
        let rest = st.preparation_calls();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].match_id, 6);
    }

    #[test]
    fn apply_preparation_calls_drops_calls_for_non_scheduled_matches() {
        use crate::btp::model::BtpLocation;
        let st = TabletState::default();
        // Match 2 ist eingeplant, Match 1 läuft (OnCourt).
        let mut snap = snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1")],
        );
        snap.locations = vec![BtpLocation {
            id: 7,
            name: "Halle A".to_string(),
        }];
        // Aufruf für ein laufendes Match (1) und ein eingeplantes (2).
        st.add_preparation_call(PreparationCall {
            match_id: 1,
            location_id: None,
            called_at_ms: 1000,
        });
        st.add_preparation_call(PreparationCall {
            match_id: 2,
            location_id: Some(7),
            called_at_ms: 2000,
        });
        st.apply_preparation_calls(&mut snap);
        // Aufruf für Match 1 fällt heraus (kein Geister-Aufruf).
        let remaining: Vec<i64> = st.preparation_calls().iter().map(|c| c.match_id).collect();
        assert_eq!(remaining, vec![2]);
        // Match 1 trägt keinen Stempel, Match 2 schon.
        let m1 = snap.matches.iter().find(|m| m.id == 1).unwrap();
        let m2 = snap.matches.iter().find(|m| m.id == 2).unwrap();
        assert_eq!(m1.preparation_call_ts, None);
        assert_eq!(m2.preparation_call_ts, Some(2000));
        assert_eq!(m2.preparation_hall.as_deref(), Some("Halle A"));
    }

    #[test]
    fn apply_preparation_calls_leaves_hall_none_without_location() {
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(3, None, MatchStatus::Scheduled)],
            vec![(101, "Court 1")],
        );
        st.add_preparation_call(PreparationCall {
            match_id: 3,
            location_id: None,
            called_at_ms: 500,
        });
        st.apply_preparation_calls(&mut snap);
        let m = &snap.matches[0];
        assert_eq!(m.preparation_call_ts, Some(500));
        assert_eq!(m.preparation_hall, None);
    }

    #[test]
    fn walkover_proposal_is_unique_per_entry_and_removable() {
        let st = TabletState::default();
        let mk = |entry: i64| WalkoverProposal {
            id: entry.to_string(),
            entry_id: entry,
            retired_team: "X".to_string(),
            draw_name: "HE".to_string(),
            created_at_ms: 0,
        };
        st.add_walkover_proposal(mk(10));
        st.add_walkover_proposal(mk(10)); // ersetzt – kein Duplikat
        assert_eq!(st.walkover_proposals().len(), 1);
        st.add_walkover_proposal(mk(20));
        assert_eq!(st.walkover_proposals().len(), 2);
        st.remove_walkover_proposal("10");
        let rest = st.walkover_proposals();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].entry_id, 20);
    }
}
