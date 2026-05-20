//! Sync-Engine: ein Poll-Push-Zyklus BTP → Badhub.
//!
//! Strategie bei Push-Fehlern (Resend-on-failure): Es gibt keine
//! persistente Outbox. Schlägt ein Push fehl, wird der zuletzt gesendete
//! Stand verworfen – der nächste Zyklus sendet dann einen vollen `tset`
//! mit dem aktuellen Komplettstand. Die Turnierdaten liegen ohnehin in
//! BTP und werden bei jedem Zyklus neu abgefragt.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::badhub::diff::{diff, Update};
use crate::badhub::push;
use crate::btp::client;
use crate::btp::model::{BtpSnapshot, MatchStatus};
use crate::config::AppConfig;

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
        }
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

    /// Führt einen vollständigen Poll-Push-Zyklus aus.
    pub async fn run_once(&mut self, config: &AppConfig, http: &reqwest::Client) -> SyncOutcome {
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

        self.stamp_finished(&mut snapshot);
        let update = self.plan(&snapshot);
        match push::push_update(http, &config.badhub.url, &config.badhub.password, &update).await {
            Ok(()) => {
                let outcome = match update {
                    Update::Full(_) => SyncOutcome::PushedFull,
                    Update::Single(_) => SyncOutcome::PushedUpdate,
                    Update::None => SyncOutcome::Idle,
                };
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
        diff(self.last_pushed.as_ref(), current, self.rid, now_ms())
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
    use crate::btp::model::{BtpMatch, BtpPlayer, MatchResult, MatchStatus};

    fn snapshot() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            matches: vec![BtpMatch {
                id: 1,
                draw_name: "HE".to_string(),
                round_name: "G1".to_string(),
                match_num: Some(1),
                team1: vec![BtpPlayer {
                    name: "A".to_string(),
                    member_id: None,
                    nationality: None,
                }],
                team2: vec![BtpPlayer {
                    name: "B".to_string(),
                    member_id: None,
                    nationality: None,
                }],
                court: Some("1".to_string()),
                sets: vec![(5, 3)],
                winner: None,
                result: MatchResult::Normal,
                status: MatchStatus::OnCourt,
                finished_at: None,
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
}
