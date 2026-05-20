//! Geteilter Zustand zwischen Sync-Loop und Tablet-Server.
//!
//! Der Sync-Loop legt hier den jeweils neuesten BTP-Snapshot ab, der
//! Tablet-Server pflegt die laufenden Court-Sessions. Beide Seiten teilen
//! sich ein `Arc<TabletState>`.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpSnapshot, MatchStatus};

/// Laufende Tablet-Sitzung an einem Court.
#[derive(Debug, Clone)]
struct CourtSession {
    /// BTP-Match-ID, das dieses Tablet zählt (0 = noch keins).
    match_id: i64,
    /// Zuletzt vom Tablet gemeldeter Satzstand (Team1, Team2).
    sets: Vec<(i64, i64)>,
    /// Ist die WebSocket-Verbindung des Tablets offen?
    connected: bool,
}

/// Eine Court-Zeile für die Felder-Übersicht der Turnierleitung.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CourtOverview {
    pub court: String,
    /// Anzeigename des Matches, z. B. "HE G1"; leer wenn kein Match.
    pub match_name: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Aktueller Satzstand – vom Tablet, falls aktiv, sonst aus BTP.
    pub sets: Vec<(i64, i64)>,
    pub tablet_connected: bool,
}

/// Geteilt zwischen Sync-Loop und Tablet-Server (`Arc<TabletState>`).
#[derive(Default)]
pub struct TabletState {
    snapshot: RwLock<Option<BtpSnapshot>>,
    courts: RwLock<HashMap<String, CourtSession>>,
}

impl TabletState {
    /// Den neuesten BTP-Snapshot ablegen (vom Sync-Loop aufgerufen).
    pub fn set_snapshot(&self, snapshot: BtpSnapshot) {
        *self.snapshot.write().unwrap() = Some(snapshot);
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

    /// Tablet hat sich für einen Court verbunden.
    pub fn attach_tablet(&self, court: &str) {
        let match_id = self.match_for_court(court).map(|m| m.id).unwrap_or(0);
        let mut courts = self.courts.write().unwrap();
        courts
            .entry(court.to_string())
            .or_insert(CourtSession {
                match_id,
                sets: Vec::new(),
                connected: true,
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
        });
        session.match_id = match_id;
        session.sets = sets;
    }

    /// Court-Session entfernen (nach übermitteltem Ergebnis).
    pub fn clear_court(&self, court: &str) {
        self.courts.write().unwrap().remove(court);
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
                CourtOverview {
                    court: court.clone(),
                    match_name: m
                        .map(|mm| {
                            format!("{} {}", mm.draw_name, mm.round_name)
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default(),
                    team1: m
                        .map(|mm| mm.team1.iter().map(|p| p.name.clone()).collect())
                        .unwrap_or_default(),
                    team2: m
                        .map(|mm| mm.team2.iter().map(|p| p.name.clone()).collect())
                        .unwrap_or_default(),
                    sets,
                    tablet_connected,
                }
            })
            .collect()
    }
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
            round_name: "G1".to_string(),
            match_num: Some(id),
            team1: vec![player("Anna")],
            team2: vec![player("Ben")],
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
}
