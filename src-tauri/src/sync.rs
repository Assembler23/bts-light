//! Sync-Engine: ein Poll-Push-Zyklus BTP → Badhub.
//!
//! Strategie bei Push-Fehlern (Resend-on-failure): Es gibt keine
//! persistente Outbox. Schlägt ein Push fehl, wird der zuletzt gesendete
//! Stand verworfen – der nächste Zyklus sendet dann einen vollen `tset`
//! mit dem aktuellen Komplettstand. Die Turnierdaten liegen ohnehin in
//! BTP und werden bei jedem Zyklus neu abgefragt.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::badhub::diff::{diff, Update};
use crate::badhub::push;
use crate::btp::client;
use crate::btp::model::{BtpSnapshot, MatchStatus};
use crate::config::AppConfig;
use crate::tablet::state::TabletState;

/// Abstand der Heartbeats: Hat sich am Turnierstand >60 s nichts geändert,
/// sendet die Sync-Engine trotzdem einen vollen `tset` als Lebenszeichen.
/// So erkennt badhub.de ein laufendes Turnier als „live", auch wenn gerade
/// keine Punkte fallen — und meldet es als beendet, sobald bts-light
/// schließt und die Heartbeats ausbleiben.
const HEARTBEAT_AFTER: Duration = Duration::from_secs(60);

/// Aktuelle Zeit in Unix-Millisekunden.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Ergebnis eines Sync-Zyklus.
#[derive(Debug)]
pub enum SyncOutcome {
    /// Ein vollständiger `tset` wurde gesendet.
    PushedFull,
    /// Ein kleines `tupdate_match` wurde gesendet.
    PushedUpdate,
    /// Keine Änderung – nichts gesendet.
    Idle,
    /// BTP nicht erreichbar oder Antwort unbrauchbar.
    BtpError(String),
    /// Push an Badhub fehlgeschlagen.
    PushError(String),
}

/// Hält den Zustand zwischen den Zyklen.
pub struct SyncEngine {
    /// Zuletzt erfolgreich gesendeter Stand; `None` erzwingt einen vollen
    /// `tset` (erster Lauf oder nach einem Push-Fehler).
    last_pushed: Option<BtpSnapshot>,
    /// Fortlaufende Request-ID für Badhub.
    rid: u64,
    /// Match-ID → Zeitpunkt, zu dem das Match erstmals als beendet
    /// erkannt wurde. BTP liefert keinen End-Zeitstempel, deshalb wird er
    /// hier über die Zyklen hinweg gemerkt.
    finished_at: HashMap<i64, u64>,
    /// Zeitpunkt des letzten tatsächlich gesendeten Pushes (echtes Update
    /// oder Heartbeat). Steuert, wann das nächste Lebenszeichen fällig ist.
    last_push_at: Option<Instant>,
    /// Zuletzt geloggte Turnier-Topologie (Hallen, Felder, Matches) –
    /// das Diagnose-Log nennt sie nur bei Änderung, nicht jeden Zyklus.
    last_topology: Option<(usize, usize, usize)>,
    /// CourtID → Match-ID des im letzten Zyklus dort OnCourt gewesenen
    /// Spiels. Wechselt das (Spiel verlässt das Feld) und ist es beendet,
    /// merkt sich der State den Verlierer als Zähltafelbediener fürs Feld.
    oncourt_prev: HashMap<i64, i64>,
}

impl Default for SyncEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            last_pushed: None,
            rid: 1,
            finished_at: HashMap::new(),
            last_push_at: None,
            last_topology: None,
            oncourt_prev: HashMap::new(),
        }
    }

    /// Ist ein Heartbeat fällig? `true`, wenn noch nie gepusht wurde oder
    /// der letzte Push länger als [`HEARTBEAT_AFTER`] zurückliegt.
    fn heartbeat_due(&self) -> bool {
        self.last_push_at
            .is_none_or(|t| t.elapsed() >= HEARTBEAT_AFTER)
    }

    /// Stempelt beendete Matches: Beim ersten Erkennen eines Siegers wird
    /// der aktuelle Zeitpunkt gemerkt und in jedes beendete Match
    /// zurückgeschrieben (stabil über alle folgenden Zyklen).
    fn stamp_finished(&mut self, snapshot: &mut BtpSnapshot) {
        let now = now_ms();
        for m in &mut snapshot.matches {
            if m.status == MatchStatus::Finished {
                m.finished_at = Some(*self.finished_at.entry(m.id).or_insert(now));
            }
        }
    }

    /// Verfolgt den Zähltafelbediener je Feld: Verlässt das im letzten
    /// Zyklus auf einem Feld OnCourt gewesene Spiel das Feld und ist es
    /// beendet, merkt sich der TabletState den Verlierer als
    /// Zähltafelbediener fürs nächste Spiel auf diesem Feld. BTP behält die
    /// Feld-Zuordnung beendeter Spiele nicht zuverlässig — daher tracken
    /// wir den Übergang selbst über die Zyklen.
    fn track_scorekeepers(&mut self, snapshot: &BtpSnapshot, tablet: &TabletState) {
        let oncourt_now: HashMap<i64, i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::OnCourt)
            .filter_map(|m| m.court_id.map(|c| (c, m.id)))
            .collect();
        for (&court_id, &prev_match_id) in &self.oncourt_prev {
            // Steht auf dem Feld jetzt ein anderes (oder kein) Spiel?
            if oncourt_now.get(&court_id) == Some(&prev_match_id) {
                continue;
            }
            // Das vorige Spiel hat das Feld verlassen — beendet + mit Sieger?
            if let Some(fm) = snapshot.matches.iter().find(|m| m.id == prev_match_id) {
                if fm.status == MatchStatus::Finished {
                    if let Some(w) = fm.winner {
                        let loser = if w == 1 { &fm.team2 } else { &fm.team1 };
                        let names: Vec<String> = loser.iter().map(|p| p.name.clone()).collect();
                        if !names.is_empty() {
                            tablet.set_scorekeeper(court_id, names);
                        }
                    }
                }
            }
        }
        self.oncourt_prev = oncourt_now;
    }

    /// Führt einen vollständigen Poll-Push-Zyklus aus.
    ///
    /// `tablet` bekommt den frischen BTP-Snapshot (Court→Match-Auflösung für
    /// den Tablet-Server); Courts mit aktivem Tablet treiben anschließend
    /// ihren Live-Score selbst – ihr Satzstand überschreibt den BTP-Poll.
    pub async fn run_once(
        &mut self,
        config: &AppConfig,
        http: &reqwest::Client,
        tablet: &TabletState,
    ) -> SyncOutcome {
        let mut snapshot = match client::fetch_snapshot(
            &config.btp.host,
            config.btp.port,
            config.btp.password.as_deref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(e) => return SyncOutcome::BtpError(e.to_string()),
        };

        // Turnier-Topologie ins Diagnose-Log – nur bei Änderung, damit es
        // den Log nicht jeden Poll-Zyklus flutet. Zeigt u. a., ob ein
        // Mehr-Hallen-Turnier korrekt erkannt wurde.
        let topology = (
            snapshot.locations.len(),
            snapshot.court_infos.len(),
            snapshot.matches.len(),
        );
        if self.last_topology != Some(topology) {
            tracing::info!(
                "BTP-Snapshot: {} Hallen, {} Felder, {} Matches",
                topology.0,
                topology.1,
                topology.2
            );
            self.last_topology = Some(topology);
        }

        self.stamp_finished(&mut snapshot);
        self.track_scorekeepers(&snapshot, tablet);
        // Rohen BTP-Stand dem Tablet-Server geben, dann die Sätze
        // tablet-getriebener Courts überschreiben.
        tablet.set_snapshot(snapshot.clone());
        tablet.apply_tablet_scores(&mut snapshot);
        // „In Vorbereitung" gerufene Spiele in den Snapshot stempeln, damit
        // der Aufruf-Zeitstempel im nächsten Push an badhub.de mitgeht.
        tablet.apply_preparation_calls(&mut snapshot);
        // Heartbeat: Ist regulär nichts zu senden, aber seit dem letzten
        // Push >60 s vergangen, wird ein voller `tset` als Lebenszeichen
        // erzwungen (Diff gegen `None`). badhub frischt damit `updated_at`
        // auf und erkennt das Turnier als aktiv.
        let update = match self.plan(&snapshot) {
            Update::None if self.heartbeat_due() => diff(None, &snapshot, self.rid),
            other => other,
        };
        let sent_something = !matches!(update, Update::None);
        match push::push_update(http, &config.badhub.url, &config.badhub.password, &update).await {
            Ok(()) => {
                let outcome = match update {
                    Update::Full(_) => SyncOutcome::PushedFull,
                    Update::Single(_) => SyncOutcome::PushedUpdate,
                    Update::None => SyncOutcome::Idle,
                };
                if sent_something {
                    self.last_push_at = Some(Instant::now());
                }
                self.on_success(snapshot);
                outcome
            }
            Err(e) => {
                self.on_failure();
                SyncOutcome::PushError(e.to_string())
            }
        }
    }

    /// Plant das nächste Update gegen den zuletzt gesendeten Stand.
    fn plan(&self, current: &BtpSnapshot) -> Update {
        diff(self.last_pushed.as_ref(), current, self.rid)
    }

    /// Nach erfolgreichem Push: Stand merken, Request-ID erhöhen.
    fn on_success(&mut self, pushed: BtpSnapshot) {
        self.last_pushed = Some(pushed);
        self.rid += 1;
    }

    /// Nach fehlgeschlagenem Push: gemerkten Stand verwerfen, damit der
    /// nächste Zyklus einen vollen `tset` sendet.
    fn on_failure(&mut self) {
        self.last_pushed = None;
        self.rid += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpMatch, BtpPlayer, Discipline, MatchResult, MatchStatus};

    fn snapshot() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: vec![BtpMatch {
                id: 1,
                draw_id: 1,
                planning_id: 1001,
                draw_name: "HE".to_string(),
                discipline: Discipline::MensSingles,
                round_name: "G1".to_string(),
                match_num: Some(1),
                team1: vec![BtpPlayer {
                    name: "A".to_string(),
                    first: String::new(),
                    last: "A".to_string(),
                    member_id: None,
                    nationality: None,
                }],
                team2: vec![BtpPlayer {
                    name: "B".to_string(),
                    first: String::new(),
                    last: "B".to_string(),
                    member_id: None,
                    nationality: None,
                }],
                entry1_id: 0,
                entry2_id: 0,
                court: Some("1".to_string()),
                court_id: None,
                sets: vec![(5, 3)],
                winner: None,
                result: MatchResult::Normal,
                status: MatchStatus::OnCourt,
                finished_at: None,
                preparation_call_ts: None,
                preparation_hall: None,
            }],
        }
    }

    #[test]
    fn first_plan_is_always_full() {
        let engine = SyncEngine::new();
        assert!(matches!(engine.plan(&snapshot()), Update::Full(_)));
    }

    #[test]
    fn unchanged_snapshot_after_success_plans_nothing() {
        let mut engine = SyncEngine::new();
        engine.on_success(snapshot());
        assert!(matches!(engine.plan(&snapshot()), Update::None));
    }

    #[test]
    fn after_failure_next_plan_is_full_again() {
        let mut engine = SyncEngine::new();
        engine.on_success(snapshot());
        // Ohne Fehler wäre ein unveränderter Snapshot ein No-op …
        assert!(matches!(engine.plan(&snapshot()), Update::None));
        // … nach einem Push-Fehler aber wird wieder voll gesendet.
        engine.on_failure();
        assert!(matches!(engine.plan(&snapshot()), Update::Full(_)));
    }

    #[test]
    fn heartbeat_due_until_a_push_happened() {
        let mut engine = SyncEngine::new();
        // Noch nie gepusht → Heartbeat fällig.
        assert!(engine.heartbeat_due());
        // Direkt nach einem Push → noch kein Heartbeat fällig.
        engine.last_push_at = Some(Instant::now());
        assert!(!engine.heartbeat_due());
    }
}
