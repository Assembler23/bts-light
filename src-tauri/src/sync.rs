//! Sync-Engine: ein Poll-Push-Zyklus BTP → Badhub.
//!
//! Strategie bei Push-Fehlern (Resend-on-failure): Es gibt keine
//! persistente Outbox. Schlägt ein Push fehl, wird der zuletzt gesendete
//! Stand verworfen – der nächste Zyklus sendet dann einen vollen `tset`
//! mit dem aktuellen Komplettstand. Die Turnierdaten liegen ohnehin in
//! BTP und werden bei jedem Zyklus neu abgefragt.

use crate::badhub::diff::{diff, Update};
use crate::badhub::push;
use crate::btp::client;
use crate::btp::model::BtpSnapshot;
use crate::config::AppConfig;

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
        }
    }

    /// Führt einen vollständigen Poll-Push-Zyklus aus.
    pub async fn run_once(&mut self, config: &AppConfig, http: &reqwest::Client) -> SyncOutcome {
        let snapshot = match client::fetch_snapshot(
            &config.btp.host,
            config.btp.port,
            config.btp.password.as_deref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(e) => return SyncOutcome::BtpError(e.to_string()),
        };

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
    use crate::btp::model::{BtpMatch, BtpPlayer, MatchStatus};

    fn snapshot() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            matches: vec![BtpMatch {
                id: 1,
                draw_name: "HE".to_string(),
                round_name: "G1".to_string(),
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
                status: MatchStatus::OnCourt,
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
