//! Cross-Site-Regressionstest der manuellen Spielreihenfolge (Spec
//! `docs/features/spielliste-manuelle-reihenfolge.md`, ADR 0023, Blocker 4).
//!
//! `docs/btp_protocol.md` warnt ausdrücklich davor, dass die Sortier-Logik
//! an mehreren Stellen dupliziert leicht auseinanderlaufen kann — „sonst
//! zeigt jede Ansicht eine andere ‚nächste Begegnung', und niemand weiß
//! mehr, welche stimmt". Dieser Test hält TL-Web (`tablet::tl::build_state`),
//! Desktop (`commands::preparation_candidates_for`) und den Liveticker
//! (`badhub::payload::build_tset`) für **dieselben** Testdaten gegeneinander
//! fest: alle drei müssen bei aktivem manuellen Präfix exakt dieselbe
//! Match-Reihenfolge liefern.
//!
//! Nicht Teil dieses Tests: `sync::auto_assign` (privat, eigener Test
//! `auto_assign_prefers_a_manually_advanced_match_over_the_earlier_schedule`
//! in `sync.rs` deckt denselben gemeinsamen Helfer bereits ab) und
//! `tablet::server::info_preparation_state` (HTTP-Handler, strukturell
//! identisch zu `preparation_candidates_for` — dieselben zwei Aufrufe von
//! `assign::hall_for_match`/`resolve_and_sort_key`, per Code-Review
//! sichergestellt statt per Axum-Testaufbau).

use bts_light_lib::badhub::payload::{build_tset, LivetickerContext};
use bts_light_lib::btp::model::{
    BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult, MatchStatus, ScoringFormat,
};
use bts_light_lib::commands::preparation_candidates_for;
use bts_light_lib::config::AppConfig;
use bts_light_lib::tablet::state::TabletState;
use bts_light_lib::tablet::tl;

fn player(name: &str) -> BtpPlayer {
    BtpPlayer {
        id: 0,
        name: name.to_string(),
        first: String::new(),
        last: name.to_string(),
        member_id: None,
        nationality: None,
        club: None,
    }
}

fn a_match(id: i64, match_num: i64, planned_time: i64) -> BtpMatch {
    BtpMatch {
        id,
        draw_id: 1,
        planning_id: id,
        display_order: None,
        from1: None,
        from2: None,
        draw_name: "HE A".to_string(),
        discipline: Discipline::MensSingles,
        class_label: "A".to_string(),
        round_name: "G1".to_string(),
        match_num: Some(match_num),
        planned_time: Some(planned_time),
        team1: vec![player("Mueller")],
        team2: vec![player("Schmidt")],
        entry1_id: 0,
        entry2_id: 0,
        court: None,
        court_id: None,
        location_id: None,
        official1_id: None,
        official2_id: None,
        sets: Vec::new(),
        winner: None,
        result: MatchResult::Normal,
        status: MatchStatus::Scheduled,
        finished_at: None,
        preparation_call_ts: None,
        preparation_hall: None,
        scoring: ScoringFormat::default(),
    }
}

fn snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
    BtpSnapshot {
        tournament_name: "T".to_string(),
        rest_minutes: None,
        matches,
        courts: Vec::new(),
        locations: Vec::new(),
        court_infos: Vec::new(),
        events: Vec::new(),
        entries: Vec::new(),
        officials: Vec::new(),
    }
}

#[test]
fn tl_web_desktop_and_liveticker_agree_on_the_manual_prefix() {
    // BTP-Zeitplan: 1 vor 2 vor 3. Match 3 wird manuell vor Match 1
    // gezogen — der neue Präfix ist [1, 3] (siehe queue_order.rs-Doku: das
    // Zielmatch braucht keinen eigenen Rang, es folgt automatisch).
    let matches = vec![
        a_match(1, 1, 202_608_071_200),
        a_match(2, 2, 202_608_071_300),
        a_match(3, 3, 202_608_071_400),
    ];
    let snap = snapshot(matches);
    let config = AppConfig::default();

    let tablet = TabletState::default();
    tablet.set_snapshot(snap.clone());
    // Keine Locations im Turnier gepflegt ⇒ alle Matches lösen auf die
    // leere Halle "" auf (wie in `assign.rs`/`queue_order.rs` getestet).
    tablet.queue_order_store().reorder("", &[1, 2, 3], 3, Some(1));

    let tl_ids: Vec<i64> = tl::build_state(&tablet, &config, 0, 1)
        .queue
        .iter()
        .map(|m| m.match_id)
        .collect();
    assert_eq!(tl_ids, vec![3, 1, 2], "TL-Web: Präfix schlägt Ansetzung");

    let desktop_ids: Vec<i64> = preparation_candidates_for(&tablet, &config)
        .candidates
        .iter()
        .map(|c| c.match_id)
        .collect();
    assert_eq!(
        desktop_ids, tl_ids,
        "Desktop zeigt eine andere Reihenfolge als TL-Web"
    );

    let ctx = LivetickerContext::new(&config, tablet.manual_halls(), tablet.queue_order_store());
    let live_ids: Vec<i64> = build_tset(&snap, 1, &ctx)
        .event
        .upcoming_matches
        .iter()
        .map(|m| {
            m.id.strip_prefix("btp_")
                .and_then(|s| s.parse::<i64>().ok())
                .expect("Match-ID im erwarteten Format")
        })
        .collect();
    assert_eq!(
        live_ids, tl_ids,
        "Liveticker zeigt eine andere Reihenfolge als TL-Web/Desktop"
    );
}

#[test]
fn all_three_views_fall_back_to_the_btp_schedule_without_a_prefix() {
    // Rückwärtskompatibilität: ohne jeden manuellen Zug bleibt es bei der
    // reinen BTP-Ansetzungsreihenfolge — an allen drei Stellen gleich.
    let matches = vec![
        a_match(1, 1, 202_608_071_200),
        a_match(2, 2, 202_608_071_300),
        a_match(3, 3, 202_608_071_400),
    ];
    let snap = snapshot(matches);
    let config = AppConfig::default();

    let tablet = TabletState::default();
    tablet.set_snapshot(snap.clone());

    let tl_ids: Vec<i64> = tl::build_state(&tablet, &config, 0, 1)
        .queue
        .iter()
        .map(|m| m.match_id)
        .collect();
    let desktop_ids: Vec<i64> = preparation_candidates_for(&tablet, &config)
        .candidates
        .iter()
        .map(|c| c.match_id)
        .collect();
    let ctx = LivetickerContext::new(&config, tablet.manual_halls(), tablet.queue_order_store());
    let live_ids: Vec<i64> = build_tset(&snap, 1, &ctx)
        .event
        .upcoming_matches
        .iter()
        .map(|m| {
            m.id.strip_prefix("btp_")
                .and_then(|s| s.parse::<i64>().ok())
                .expect("Match-ID im erwarteten Format")
        })
        .collect();

    assert_eq!(tl_ids, vec![1, 2, 3]);
    assert_eq!(desktop_ids, tl_ids);
    assert_eq!(live_ids, tl_ids);
}
