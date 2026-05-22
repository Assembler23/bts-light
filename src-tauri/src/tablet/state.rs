//! Geteilter Zustand zwischen Sync-Loop und Tablet-Server.
//!
//! Der Sync-Loop legt hier den jeweils neuesten BTP-Snapshot ab, der
//! Tablet-Server pflegt die laufenden Court-Sessions. Beide Seiten teilen
//! sich ein `Arc<TabletState>`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use serde::Serialize;

use relay_proto::{MonitorCommand, MonitorCommandKind, MonitorDeviceInfo};

use crate::btp::model::{BtpMatch, BtpSnapshot, Discipline, MatchStatus};

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
    pub court: String,
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

/// Geteilt zwischen Sync-Loop und Tablet-Server (`Arc<TabletState>`).
#[derive(Default)]
pub struct TabletState {
    snapshot: RwLock<Option<BtpSnapshot>>,
    courts: RwLock<HashMap<String, CourtSession>>,
    /// Court → Token des aktuell schiedsenden Tablets (LAN-Tablet-Übernahme).
    /// Fehlt der Eintrag, ist der Court frei.
    active: RwLock<HashMap<String, u64>>,
    /// Fortlaufender Zähler, vergibt eindeutige Court-Tokens.
    token_seq: AtomicU64,
    /// Court → gespiegelter Spielzustand (JSON) des aktiven Tablets –
    /// wird einem übernehmenden Gerät übergeben.
    court_state: RwLock<HashMap<String, String>>,
    /// Offene Walkover-Vorschläge nach Aufgaben (je EntryID höchstens einer).
    walkovers: RwLock<Vec<WalkoverProposal>>,
    /// Geräte-ID → Live-Zustand der Court-Monitore (zuletzt gesehen +
    /// offener Fernbefehl). Im LAN-Modus vom Server gepflegt.
    monitor_live: RwLock<HashMap<String, MonitorLive>>,
    /// Im Cloud-Modus die vom Relay gemeldete Monitor-Geräteliste – der
    /// Relay-Client hält sie aktuell, die „Court-Monitore"-Seite liest sie.
    relay_monitor_devices: RwLock<Vec<MonitorDeviceInfo>>,
}

impl TabletState {
    /// Den neuesten BTP-Snapshot ablegen (vom Sync-Loop aufgerufen).
    pub fn set_snapshot(&self, snapshot: BtpSnapshot) {
        *self.snapshot.write().unwrap() = Some(snapshot);
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

    /// Alle Court-Namen des Turniers (für die Tablet-Adressen/QR-Codes).
    pub fn court_names(&self) -> Vec<String> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.courts.clone())
            .unwrap_or_default()
    }

    /// Das Match, das BTP gerade diesem Court zugewiesen hat.
    pub fn match_for_court(&self, court: &str) -> Option<BtpMatch> {
        let guard = self.snapshot.read().unwrap();
        let snap = guard.as_ref()?;
        snap.matches
            .iter()
            .find(|m| m.status == MatchStatus::OnCourt && m.court.as_deref() == Some(court))
            .cloned()
    }

    /// Tablet hat sich für einen Court verbunden. `match_id` startet auf 0 –
    /// den echten Wert setzt der erste `record_score`.
    pub fn attach_tablet(&self, court: &str) {
        self.courts
            .write()
            .unwrap()
            .entry(court.to_string())
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

    /// Tablet-WebSocket für einen Court ist geschlossen.
    pub fn detach_tablet(&self, court: &str) {
        if let Some(session) = self.courts.write().unwrap().get_mut(court) {
            session.connected = false;
        }
    }

    /// Satzstand vom Tablet übernehmen.
    pub fn record_score(&self, court: &str, match_id: i64, sets: Vec<(i64, i64)>) {
        let mut courts = self.courts.write().unwrap();
        let session = courts.entry(court.to_string()).or_insert(CourtSession {
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

    /// Akkustand des Tablets an einem Court übernehmen.
    pub fn record_battery(&self, court: &str, percent: i64, charging: bool) {
        let mut courts = self.courts.write().unwrap();
        courts
            .entry(court.to_string())
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

    /// Meldungs-Zustand (Verletzung / Turnierleitung gerufen) des Courts setzen.
    pub fn record_alert(&self, court: &str, injury: bool, official: bool) {
        let mut courts = self.courts.write().unwrap();
        let session = courts.entry(court.to_string()).or_insert(CourtSession {
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

    /// Beansprucht den Court für ein Tablet und gibt dessen Token zurück.
    /// Ein bereits aktives Tablet wird dadurch abgelöst (Tablet-Übernahme).
    pub fn claim_court(&self, court: &str) -> u64 {
        let token = self.token_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.active
            .write()
            .unwrap()
            .insert(court.to_string(), token);
        token
    }

    /// Ist `token` noch das aktive Tablet dieses Courts?
    pub fn is_court_active(&self, court: &str, token: u64) -> bool {
        self.active.read().unwrap().get(court) == Some(&token)
    }

    /// Wird der Court bereits von einem Tablet geschiedst?
    pub fn court_occupied(&self, court: &str) -> bool {
        self.active.read().unwrap().contains_key(court)
    }

    /// Gibt den Court frei – nur, wenn `token` noch der aktive ist.
    pub fn release_court(&self, court: &str, token: u64) {
        let mut active = self.active.write().unwrap();
        if active.get(court) == Some(&token) {
            active.remove(court);
        }
    }

    /// Spiegelt den Spielzustand des aktiven Tablets am Court.
    pub fn set_court_state(&self, court: &str, state: String) {
        self.court_state
            .write()
            .unwrap()
            .insert(court.to_string(), state);
    }

    /// Liefert den gespiegelten Spielzustand eines Courts (für die Übernahme).
    pub fn court_state(&self, court: &str) -> Option<String> {
        self.court_state.read().unwrap().get(court).cloned()
    }

    /// Court-Session entfernen (nach übermitteltem Ergebnis).
    pub fn clear_court(&self, court: &str) {
        self.courts.write().unwrap().remove(court);
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

    /// Courts mit verbundenem Tablet – diese treiben ihren Live-Score selbst.
    pub fn active_courts(&self) -> Vec<String> {
        self.courts
            .read()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.connected)
            .map(|(c, _)| c.clone())
            .collect()
    }

    /// Überschreibt im Snapshot die Sätze jedes tablet-getriebenen Matches
    /// mit dem Tablet-Stand. So pusht die Liveticker-Pipeline den
    /// Tablet-Score statt BTPs veraltetem Poll-Wert. Greift nur, wenn die
    /// Session zum selben Match gehört (Schutz gegen Match-Wechsel).
    pub fn apply_tablet_scores(&self, snapshot: &mut BtpSnapshot) {
        let courts = self.courts.read().unwrap();
        for m in &mut snapshot.matches {
            let Some(court) = m.court.as_deref() else {
                continue;
            };
            if let Some(session) = courts.get(court) {
                if session.connected && session.match_id == m.id {
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
        snap.courts
            .iter()
            .map(|court| {
                let m = snap.matches.iter().find(|m| {
                    m.status == MatchStatus::OnCourt && m.court.as_deref() == Some(court.as_str())
                });
                let session = courts.get(court);
                let tablet_connected = session.map(|s| s.connected).unwrap_or(false);
                // Satzstand vom Tablet, falls aktiv und auf dasselbe Match.
                let sets = match (session, m) {
                    (Some(s), Some(mm)) if s.connected && s.match_id == mm.id => s.sets.clone(),
                    (_, Some(mm)) => mm.sets.clone(),
                    _ => Vec::new(),
                };
                let nationalities = |team: &[crate::btp::model::BtpPlayer]| {
                    team.iter()
                        .map(|p| p.nationality.clone().unwrap_or_default())
                        .collect::<Vec<String>>()
                };
                CourtOverview {
                    court: court.clone(),
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
                }
            })
            .collect()
    }

    /// Monitor-relevante Daten eines Feldes: das aktuelle Match mit
    /// effektivem Satzstand (Tablet-getrieben falls aktiv, sonst aus BTP)
    /// und der gespiegelte Tablet-Spielzustand (Aufschlag/Pause). Vom
    /// Court-Monitor-Endpunkt genutzt.
    pub fn monitor_court(&self, court: &str) -> MonitorCourt {
        let guard = self.snapshot.read().unwrap();
        let tournament_name = guard
            .as_ref()
            .map(|s| s.tournament_name.clone())
            .unwrap_or_default();
        let current_match = guard.as_ref().and_then(|snap| {
            snap.matches
                .iter()
                .find(|m| m.status == MatchStatus::OnCourt && m.court.as_deref() == Some(court))
                .cloned()
        });
        drop(guard);
        let sets = match &current_match {
            Some(mm) => {
                let courts = self.courts.read().unwrap();
                match courts.get(court) {
                    Some(s) if s.connected && s.match_id == mm.id => s.sets.clone(),
                    _ => mm.sets.clone(),
                }
            }
            None => Vec::new(),
        };
        MonitorCourt {
            tournament_name,
            current_match,
            sets,
            court_state: self.court_state(court),
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
            member_id: None,
            nationality: None,
        }
    }

    fn match_on(id: i64, court: Option<&str>, status: MatchStatus) -> BtpMatch {
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
            court: court.map(String::from),
            sets: vec![(5, 3)],
            winner: None,
            result: MatchResult::Normal,
            status,
            finished_at: None,
        }
    }

    fn snapshot(matches: Vec<BtpMatch>, courts: Vec<&str>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            matches,
            courts: courts.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn match_for_court_finds_the_on_court_match() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some("Court 1"), MatchStatus::OnCourt),
                match_on(2, None, MatchStatus::Scheduled),
            ],
            vec!["Court 1", "Court 2"],
        ));
        assert_eq!(st.match_for_court("Court 1").unwrap().id, 1);
        assert!(st.match_for_court("Court 2").is_none());
    }

    #[test]
    fn apply_tablet_scores_overrides_only_active_matching_court() {
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(1, Some("Court 1"), MatchStatus::OnCourt)],
            vec!["Court 1"],
        );
        st.set_snapshot(snap.clone());
        st.record_score("Court 1", 1, vec![(21, 19), (8, 6)]);
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 19), (8, 6)]);
    }

    #[test]
    fn apply_tablet_scores_ignores_session_for_other_match() {
        // Court hat inzwischen ein anderes Match – der Tablet-Stand darf
        // nicht aufs neue Match durchschlagen.
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(9, Some("Court 1"), MatchStatus::OnCourt)],
            vec!["Court 1"],
        );
        st.record_score("Court 1", 1, vec![(21, 0)]);
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(5, 3)]);
    }

    #[test]
    fn detached_tablet_is_not_active() {
        let st = TabletState::default();
        st.attach_tablet("Court 1");
        assert_eq!(st.active_courts(), vec!["Court 1".to_string()]);
        st.detach_tablet("Court 1");
        assert!(st.active_courts().is_empty());
    }

    #[test]
    fn overview_lists_each_court_with_its_match() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some("Court 1"), MatchStatus::OnCourt)],
            vec!["Court 1", "Court 2"],
        ));
        st.record_score("Court 1", 1, vec![(15, 12)]);
        st.attach_tablet("Court 1");
        let ov = st.overview();
        assert_eq!(ov.len(), 2);
        let c1 = ov.iter().find(|o| o.court == "Court 1").unwrap();
        assert_eq!(c1.team1, vec!["Anna".to_string()]);
        assert_eq!(c1.sets, vec![(15, 12)]);
        assert!(c1.tablet_connected);
        let c2 = ov.iter().find(|o| o.court == "Court 2").unwrap();
        assert_eq!(c2.match_name, "");
        assert!(!c2.tablet_connected);
    }

    #[test]
    fn monitor_court_returns_match_with_effective_sets() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some("Court 1"), MatchStatus::OnCourt)],
            vec!["Court 1", "Court 2"],
        ));
        // Ohne Tablet: Satzstand aus BTP (match_on setzt sets = [(5,3)]).
        let mc = st.monitor_court("Court 1");
        assert_eq!(mc.tournament_name, "T");
        assert_eq!(mc.current_match.as_ref().unwrap().id, 1);
        assert_eq!(mc.sets, vec![(5, 3)]);
        assert!(mc.court_state.is_none());
        // Mit Tablet-Score: der Satzstand kommt vom Tablet.
        st.record_score("Court 1", 1, vec![(21, 19), (8, 4)]);
        assert_eq!(st.monitor_court("Court 1").sets, vec![(21, 19), (8, 4)]);
        // Leeres Feld: kein Match.
        assert!(st.monitor_court("Court 2").current_match.is_none());
    }

    #[test]
    fn walkover_candidates_lists_scheduled_matches_of_the_entry() {
        let st = TabletState::default();
        // match_on setzt entry1_id = 10, entry2_id = 20.
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some("Court 1"), MatchStatus::OnCourt), // läuft – kein Kandidat
                match_on(2, None, MatchStatus::Scheduled),
                match_on(3, None, MatchStatus::Scheduled),
            ],
            vec!["Court 1"],
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
