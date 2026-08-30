//! Cross-Site-Regressionstest der manuellen Spielreihenfolge (Spec
//! `docs/features/spielliste-manuelle-reihenfolge.md`, ADR 0023 Blocker 4,
//! seit ADR 0026 mit **einer globalen** Reihenfolge statt einer je Halle).
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
//! **Ausnahme seit Spec `tl-offene-paarungen` (ADR 0053):** Spiele mit noch
//! offener Paarung zeigt **nur** TL-Web, und zwar in einer eigenen Liste
//! (`TlState::open_queue`). Desktop-Vorbereitung und Liveticker filtern sie
//! weiterhin weg — sie sollen niemandem eine Begegnung ankündigen, die noch
//! keine ist. Die Zusage dieses Tests lautet deshalb genauer: Die relative
//! Reihenfolge der **spielbereiten** Spiele ist überall dieselbe. Offene
//! Spiele gehören nicht zum Vergleich; dass sie die Reihenfolge der anderen
//! nicht verschieben, prüft `offene_spiele_im_praefix_aendern_die_reihenfolge_der_echten_spiele_nicht`.
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
    BtpLocation, BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult, MatchStatus,
    ScoringFormat,
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
        pause_ms: None,
        preparation_call_ts: None,
        preparation_hall: None,
        scoring: ScoringFormat::default(),
    }
}

fn snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
    snapshot_with_locations(matches, Vec::new())
}

fn snapshot_with_locations(matches: Vec<BtpMatch>, locations: Vec<BtpLocation>) -> BtpSnapshot {
    BtpSnapshot {
        tournament_name: "T".to_string(),
        rest_minutes: None,
        matches,
        courts: Vec::new(),
        locations,
        court_infos: Vec::new(),
        events: Vec::new(),
        entries: Vec::new(),
        officials: Vec::new(),
    }
}

#[test]
fn offene_spiele_im_praefix_aendern_die_reihenfolge_der_echten_spiele_nicht() {
    // ADR 0053: Offene Spiele nehmen an der globalen manuellen Reihenfolge
    // teil, erscheinen aber nur in TL-Web. Der Präfix darf deshalb ein
    // offenes Spiel enthalten, ohne dass Desktop und Liveticker davon etwas
    // merken — sonst zeigte eine der drei Ansichten eine andere
    // „nächste Begegnung", genau die Falle aus dem Modulkommentar.
    let mut offen = a_match(99, 99, 202_608_071_330);
    offen.team1 = Vec::new();
    offen.team2 = Vec::new();
    let matches = vec![
        a_match(1, 1, 202_608_071_200),
        offen,
        a_match(2, 2, 202_608_071_300),
        a_match(3, 3, 202_608_071_400),
    ];
    let snap = snapshot(matches);
    let config = AppConfig::default();

    let tablet = TabletState::default();
    tablet.set_snapshot(snap.clone());
    // Das offene Spiel ganz nach vorn ziehen — der Präfix trägt es damit.
    tablet
        .queue_order_store()
        .reorder(&[1, 2, 99, 3], 99, Some(1));

    let zustand = tl::build_state(&tablet, &config, 0, 1);
    assert_eq!(
        zustand
            .open_queue
            .iter()
            .map(|m| m.match_id)
            .collect::<Vec<_>>(),
        vec![99],
        "TL-Web führt das offene Spiel in seiner eigenen Liste"
    );
    let tl_ids: Vec<i64> = zustand.queue.iter().map(|m| m.match_id).collect();
    assert_eq!(
        tl_ids,
        vec![1, 2, 3],
        "die Arbeitsliste bleibt in der Reihenfolge der Ansetzung"
    );

    let desktop_ids: Vec<i64> = preparation_candidates_for(&tablet, &config)
        .candidates
        .iter()
        .map(|c| c.match_id)
        .collect();
    assert_eq!(
        desktop_ids, tl_ids,
        "der Desktop zeigt dieselben spielbereiten Spiele in derselben Folge"
    );

    let ctx = LivetickerContext::new(
        &config,
        tablet.manual_halls(),
        tablet.auto_hall_store().halls(),
        tablet.queue_order_store(),
    );
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
        "der Liveticker kündigt keine Begegnung an, die noch keine ist"
    );
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
    tablet.queue_order_store().reorder(&[1, 2, 3], 3, Some(1));

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

    let ctx = LivetickerContext::new(
        &config,
        tablet.manual_halls(),
        tablet.auto_hall_store().halls(),
        tablet.queue_order_store(),
    );
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
fn tl_web_desktop_and_liveticker_agree_across_two_halls() {
    // Der Fall, den ADR 0026 neu erlaubt und den der Test bis dahin nicht
    // abdeckte (`locations` war leer, also lief alles in der Halle ""):
    // Ein Zug wirkt jetzt über die Hallengrenze hinweg. Match 4 (Halle B,
    // spät angesetzt) wird vor Match 1 (Halle A, früh angesetzt) gezogen —
    // alle drei Ansichten müssen danach dieselbe Abfolge zeigen.
    let mut m1 = a_match(1, 1, 202_608_071_200);
    m1.location_id = Some(1); // Halle A
    let mut m2 = a_match(2, 2, 202_608_071_300);
    m2.location_id = Some(2); // Halle B
    let mut m3 = a_match(3, 3, 202_608_071_400);
    m3.location_id = Some(1); // Halle A
    let mut m4 = a_match(4, 4, 202_608_071_500);
    m4.location_id = Some(2); // Halle B

    let snap = snapshot_with_locations(
        vec![m1, m2, m3, m4],
        vec![
            BtpLocation {
                id: 1,
                name: "Halle A".to_string(),
            },
            BtpLocation {
                id: 2,
                name: "Halle B".to_string(),
            },
        ],
    );
    let config = AppConfig::default();

    let tablet = TabletState::default();
    tablet.set_snapshot(snap.clone());
    tablet
        .queue_order_store()
        .reorder(&[1, 2, 3, 4], 4, Some(1));

    let tl_queue = tl::build_state(&tablet, &config, 0, 1).queue;
    let tl_ids: Vec<i64> = tl_queue.iter().map(|m| m.match_id).collect();
    assert_eq!(
        tl_ids,
        vec![4, 1, 2, 3],
        "TL-Web: der Zug wirkt über die Hallengrenze"
    );
    // Und die Halle bleibt dabei korrekt aufgelöst — sie ist jetzt reine
    // Anzeige, kein Sortierkriterium mehr.
    let halls: Vec<&str> = tl_queue.iter().map(|m| m.hall.as_str()).collect();
    assert_eq!(halls, vec!["Halle B", "Halle A", "Halle B", "Halle A"]);

    let desktop_ids: Vec<i64> = preparation_candidates_for(&tablet, &config)
        .candidates
        .iter()
        .map(|c| c.match_id)
        .collect();
    assert_eq!(
        desktop_ids, tl_ids,
        "Desktop zeigt eine andere Reihenfolge als TL-Web"
    );

    let ctx = LivetickerContext::new(
        &config,
        tablet.manual_halls(),
        tablet.auto_hall_store().halls(),
        tablet.queue_order_store(),
    );
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
    let ctx = LivetickerContext::new(
        &config,
        tablet.manual_halls(),
        tablet.auto_hall_store().halls(),
        tablet.queue_order_store(),
    );
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
