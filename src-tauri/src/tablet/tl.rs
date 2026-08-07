//! Der Anzeige-Zustand der Turnierleitungs-Oberfläche.
//!
//! Ein einziger Abruf füllt die ganze Seite: Felder, Spielliste, Zeiten,
//! Rahmendaten. Gebaut wird er **am Host** — im LAN-Betrieb liefert ihn der
//! eingebettete Server direkt aus, im Cloud-Betrieb schiebt der Host ihn als
//! opakes JSON zum Relay, der ihn unverändert weiterreicht. So bleibt die
//! Turnierlogik vollständig hier (R5), und beide Wege zeigen dasselbe.
//!
//! **Datensparsamkeit ist hier Teil der Funktion**, nicht nur eine Regel im
//! Kopf: Diese Daten laufen über eine aus dem Internet erreichbare Seite.
//! Deshalb enthält der Zustand keine Lizenznummern (die Spieler-Identität
//! bleibt am Host; nach außen geht nur das *Ergebnis* der Prüfung), keine
//! Nationalitäten (die existieren allein für die Sprachwahl der Ansage, und
//! diese Seite spricht nicht) und keine Akkustände. Ein Test wacht darüber.
//!
//! Details: `docs/features/turnierleitung-web.md`.

use crate::config::AppConfig;
use crate::tablet::assign::{self, Blocked, HallSource, PlayerAvailability};
use crate::tablet::state::TabletState;

/// Der komplette Anzeige-Zustand.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlState {
    /// Steigt nur bei echter Änderung — daran erkennt ein abrufendes Gerät,
    /// ob sich etwas getan hat, ohne den ganzen Stand zu vergleichen.
    pub rev: u64,
    /// Server-Zeit beim Bauen. Die Geräte rechnen daraus ihren Zeit-Versatz
    /// aus und zeigen dieselbe verstrichene Zeit, egal wie falsch ihre eigene
    /// Uhr geht.
    pub server_now_ms: u64,
    pub tournament: String,
    /// Mehr-Hallen-Turnier? Nur dann bietet die Seite einen Hallenfilter an.
    pub multi_hall: bool,
    /// Alle Hallennamen des Turniers, alphabetisch.
    pub halls: Vec<String>,
    pub auto_assign: TlAutoAssign,
    /// Schwellen des Aufruf-Timers, damit die Seite die Aufruf-Stufe
    /// genauso einfärbt wie die Desktop-Oberfläche.
    pub call_timer: TlCallTimer,
    /// Pflichtpause zwischen zwei Spielen eines Spielers (Minuten), aus der
    /// Konfiguration oder aus BTP. `None` = keine.
    pub rest_minutes: Option<i64>,
    pub courts: Vec<TlCourt>,
    /// Die Spielliste, **bereits sortiert** — dieselbe Reihenfolge, nach der
    /// die automatische Vergabe arbeitet. Bewusst serverseitig sortiert:
    /// Sonst zeigten zwei Geräte zwei Reihenfolgen.
    pub queue: Vec<TlMatch>,
    /// Hallen, deren Warteliste gekappt wurde (leerer Name = Spiele ohne
    /// Hallenzuordnung). Leer = nichts gekappt.
    ///
    /// Gekappt wird **je Halle**, nicht über das ganze Turnier: Global
    /// gekappt könnte die Sortierung eine komplette Halle verdrängen, und
    /// das Gerät dort sähe eine leere Liste, obwohl hundert Spiele warten.
    pub truncated_halls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlAutoAssign {
    pub enabled: bool,
    pub wait_minutes: f64,
    /// Tages-Halle; leer = alle.
    pub active_hall: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlCallTimer {
    pub enabled: bool,
    pub second_call_minutes: f64,
    pub third_call_minutes: f64,
}

/// Ein Feld mit dem, was gerade darauf läuft.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlCourt {
    pub court_id: i64,
    pub court: String,
    /// Hallenname; leer bei Ein-Hallen-Turnieren.
    pub location: String,
    /// 0 = kein Spiel auf dem Feld.
    pub match_id: i64,
    pub match_name: String,
    pub round_name: String,
    pub class_label: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    pub sets: Vec<(i64, i64)>,
    pub tablet_connected: bool,
    /// Verletzung/Behandlung läuft — die Turnierleitung will das sehen.
    pub injury: bool,
    /// Das Feld hat die Turnierleitung gerufen.
    pub official_call: bool,
    /// Laufende Pause am Feld. Bewusst **typisiert** statt roh
    /// durchgereicht: Der Block kommt vom Zähltablett, und was von dort
    /// kommt, darf nicht ungeprüft an alle Turnierleitungs-Geräte und durch
    /// den Relay wandern.
    pub pause: Option<TlPause>,
    pub scorekeeper: Vec<String>,
    pub scorekeeper_assigned: bool,
    pub locked: bool,
    /// Ein **beendetes** Spiel hält dieses Feld noch, weil BTP es nicht
    /// abgeräumt hat. `match_id` ist dann 0 (es läuft ja nichts mehr), das
    /// Feld ist aber trotzdem nicht belegbar. Ohne diese Angabe zeigte die
    /// Seite ein freies Feld, auf das keine Zuweisung möglich ist.
    pub clearing: Option<i64>,
    /// Seit wann das Spiel auf dem Feld steht (= 1. Aufruf). Grundlage der
    /// hochzählenden Uhr und der Aufruf-Stufe.
    pub on_court_since_ms: Option<u64>,
    /// Zählformat, damit die Seite Satz- und Matchball anzeigen kann.
    pub best_of: i64,
    pub target_score: i64,
    pub cap_score: i64,
}

/// Ein Spiel in der Warteliste.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlMatch {
    pub match_id: i64,
    /// Die Nummer aus dem Papierplan — danach sucht der Helfer.
    pub match_num: Option<i64>,
    /// Angesetzte Zeit als `YYYYMMDDHHMM`, wie BTP sie führt.
    pub planned_time: Option<i64>,
    pub draw_name: String,
    pub round_name: String,
    pub class_label: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// In welche Halle das Spiel gehört, und woher wir das wissen.
    pub hall: String,
    pub hall_source: HallSource,
    /// Bereits in die Vorbereitung gerufen?
    pub prep_call: Option<TlPrepCall>,
    /// Warum das Spiel gerade nicht aufs Feld kann; `None` = spielbereit.
    pub blocked: Option<TlBlocked>,
}

/// Eine laufende Pause am Feld (BWF-Intervall, Satzpause, Behandlung).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlPause {
    /// Art der Pause, wie das Zähltablett sie meldet.
    pub kind: String,
    /// Ende der Pause in Server-Zeit.
    pub ends_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlPrepCall {
    /// Halle, in die gerufen wurde; leer = ohne Hallenangabe.
    pub hall: String,
    pub called_at_ms: u64,
}

/// Warum ein Spiel wartet — mit Namen, denn „gesperrt" ohne Namen ist eine
/// Blackbox, der die Turnierleitung zu Recht misstraut.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum TlBlocked {
    /// Mindestens ein Spieler steht gerade auf einem Feld.
    Playing { players: Vec<String> },
    /// Mindestens ein Spieler ist noch in seiner Pause.
    Pause {
        /// Ab wann der Letzte wieder darf — damit der Helfer planen kann,
        /// statt zu raten.
        until_ms: u64,
        players: Vec<String>,
    },
}

impl From<Blocked> for TlBlocked {
    fn from(b: Blocked) -> Self {
        match b {
            Blocked::Playing { players } => TlBlocked::Playing { players },
            Blocked::Pause { until_ms, players } => TlBlocked::Pause { until_ms, players },
        }
    }
}

/// Wie viele wartende Spiele **je Halle** höchstens ausgeliefert werden.
///
/// Bei großen Turnieren stehen mehrere hundert Spiele an; alle zu übertragen
/// kostet bei jedem Abruf und auf jedem Gerät. Die Liste ist nach
/// Dringlichkeit sortiert, die vorderen sind die, um die es geht — was
/// wegfällt, meldet `truncated_halls` ehrlich.
const QUEUE_LIMIT_PER_HALL: usize = 120;

/// Ordnungsschlüssel eines wartenden Spiels samt dem Spiel selbst und seiner
/// Halle — die Zwischenform, in der sortiert und gekappt wird, bevor die
/// teuren Zeichenketten der Anzeige entstehen.
type OrderedMatch<'a> = (
    (bool, i64, i64, i64),
    &'a crate::btp::model::BtpMatch,
    String,
);

/// Baut den Anzeige-Zustand aus dem aktuellen BTP-Stand und dem, was der Host
/// selbst verwaltet (Aufrufe, Sperren, Live-Spielstände).
///
/// `rev` gibt der Aufrufer vor — er entscheidet, ob sich gegenüber dem
/// zuletzt ausgelieferten Stand überhaupt etwas geändert hat.
pub fn build_state(tablet: &TabletState, config: &AppConfig, now_ms: u64, rev: u64) -> TlState {
    let Some(snap) = tablet.snapshot_clone() else {
        // Noch kein Turnier geladen: leerer, aber gültiger Zustand — die
        // Seite zeigt „warte auf Turnierdaten" statt eines Fehlers.
        return TlState {
            rev,
            server_now_ms: now_ms,
            tournament: String::new(),
            multi_hall: false,
            halls: Vec::new(),
            auto_assign: auto_assign_view(config),
            call_timer: call_timer_view(config),
            rest_minutes: None,
            courts: Vec::new(),
            queue: Vec::new(),
            truncated_halls: Vec::new(),
        };
    };

    // Felder und Warteliste stammen aus **demselben** Schnappschuss. Zwei
    // getrennte Lesevorgänge könnten den Sync-Lauf dazwischen erwischen —
    // dann beschrieben Felder und Liste zwei verschiedene Turnierstände.
    let courts: Vec<TlCourt> = tablet
        .overview_from(&snap)
        .into_iter()
        .map(|c| {
            let clearing = clearing_match(&snap, c.court_id, c.match_id);
            court_view(c, clearing)
        })
        .collect();

    // Aufrufe einmal auflösen: Match-ID → Halle des Aufrufs.
    let calls = tablet.preparation_calls();
    let called_hall = |match_id: i64| -> Option<(String, u64)> {
        calls.iter().find(|c| c.match_id == match_id).map(|c| {
            let hall = c
                .location_id
                .and_then(|id| snap.locations.iter().find(|l| l.id == id))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            (hall, c.called_at_ms)
        })
    };

    let availability = PlayerAvailability::from_snapshot(&snap, config);

    // Spielbereite Spiele — dieselbe Bedingung wie bei der automatischen
    // Vergabe: geplant und mit feststehender Paarung. Spiele, deren Gegner
    // noch aus einem Vorspiel kommt, könnte niemand sinnvoll vergeben.
    // Erst nur die Ordnungsschlüssel sammeln — die teuren Zeichenketten
    // entstehen später und nur für die Spiele, die auch ausgeliefert werden.
    let mut ordered: Vec<OrderedMatch> = snap
        .matches
        .iter()
        .filter(|m| {
            m.status == crate::btp::model::MatchStatus::Scheduled
                && !m.team1.is_empty()
                && !m.team2.is_empty()
        })
        .map(|m| {
            let call = called_hall(m.id);
            let (hall, _) =
                assign::hall_for_match(config, &snap, m, call.as_ref().map(|(h, _)| h.as_str()));
            (assign::sort_key(m, call.is_some()), m, hall)
        })
        .collect();
    ordered.sort_by_key(|(key, _, _)| *key);

    // Je Halle kappen, nicht über das ganze Turnier.
    let mut per_hall: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut truncated: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue: Vec<TlMatch> = Vec::new();
    for (_, m, hall) in ordered {
        let count = per_hall.entry(hall.clone()).or_insert(0);
        if *count >= QUEUE_LIMIT_PER_HALL {
            truncated.insert(hall);
            continue;
        }
        *count += 1;
        let call = called_hall(m.id);
        let (_, hall_source) =
            assign::hall_for_match(config, &snap, m, call.as_ref().map(|(h, _)| h.as_str()));
        queue.push(TlMatch {
            match_id: m.id,
            match_num: m.match_num,
            planned_time: m.planned_time,
            draw_name: m.draw_name.clone(),
            round_name: m.round_name.clone(),
            class_label: m.class_label.clone(),
            team1: m.team1.iter().map(|p| p.name.clone()).collect(),
            team2: m.team2.iter().map(|p| p.name.clone()).collect(),
            hall,
            hall_source,
            prep_call: call.map(|(hall, called_at_ms)| TlPrepCall { hall, called_at_ms }),
            blocked: availability.blocked(m, now_ms).map(TlBlocked::from),
        });
    }

    let mut halls: Vec<String> = snap
        .locations
        .iter()
        .map(|l| l.name.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    halls.sort_by_key(|h| h.to_lowercase());
    halls.dedup();

    TlState {
        rev,
        server_now_ms: now_ms,
        tournament: snap.tournament_name.clone(),
        multi_hall: snap.is_multi_hall(),
        halls,
        auto_assign: auto_assign_view(config),
        call_timer: call_timer_view(config),
        // Genau der Wert, nach dem auch die Blockier-Zeiten in diesem
        // Datensatz gerechnet sind: Konfiguration schlägt BTP-Einstellung.
        // Ein abweichender Anzeigewert ließe die Seite sich selbst
        // widersprechen.
        rest_minutes: effective_rest_minutes(&snap, config),
        courts,
        queue,
        truncated_halls: truncated.into_iter().collect(),
    }
}

/// Die tatsächlich geltende Pflichtpause in Minuten — dieselbe Regel, nach
/// der [`PlayerAvailability`] rechnet: Ein gesetzter Wert in der
/// Konfiguration schlägt die BTP-Einstellung.
fn effective_rest_minutes(
    snap: &crate::btp::model::BtpSnapshot,
    config: &AppConfig,
) -> Option<i64> {
    if config.auto_assign.pause_minutes > 0.0 {
        Some(config.auto_assign.pause_minutes as i64)
    } else {
        snap.rest_minutes.filter(|m| *m > 0)
    }
}

/// Welches **beendete** Spiel hält dieses Feld noch? `None`, wenn das Feld
/// wirklich frei ist oder ein laufendes Spiel darauf steht.
fn clearing_match(
    snap: &crate::btp::model::BtpSnapshot,
    court_id: i64,
    running_match_id: i64,
) -> Option<i64> {
    if running_match_id != 0 {
        return None;
    }
    assign::court_occupied_by(snap, court_id)
}

fn auto_assign_view(config: &AppConfig) -> TlAutoAssign {
    TlAutoAssign {
        enabled: config.auto_assign.enabled,
        wait_minutes: config.auto_assign.wait_minutes,
        active_hall: config.auto_assign.active_hall.clone(),
    }
}

fn call_timer_view(config: &AppConfig) -> TlCallTimer {
    TlCallTimer {
        enabled: config.call_timer.enabled,
        second_call_minutes: config.call_timer.second_call_minutes,
        third_call_minutes: config.call_timer.third_call_minutes,
    }
}

/// Beschneidet die Feld-Übersicht auf das, was die Turnierleitung braucht.
///
/// Bewusst **weggelassen**: Nationalitäten (nur für die Sprachwahl der
/// Ansage, und diese Seite spricht nicht), Akkustand (keine Geräte-Übersicht
/// in diesem Feature) und die Aufschlag-Anzeige (Zählhilfe, keine
/// Vergabehilfe).
/// Beschneidet die Feld-Übersicht auf das, was die Turnierleitung braucht.
///
/// Bewusst **weggelassen**: Nationalitäten (nur für die Sprachwahl der
/// Ansage, und diese Seite spricht nicht), Akkustand (keine Geräte-Übersicht
/// in diesem Feature) und die Aufschlag-Anzeige (Zählhilfe, keine
/// Vergabehilfe).
fn court_view(c: crate::tablet::state::CourtOverview, clearing: Option<i64>) -> TlCourt {
    // Aus dem rohen Tablet-JSON nur die zwei bekannten Angaben übernehmen.
    // Alles andere bliebe ungeprüfter Fremdinhalt auf einer aus dem Internet
    // erreichbaren Seite.
    let pause = c.pause.as_ref().and_then(|v| {
        Some(TlPause {
            kind: v.get("kind")?.as_str()?.to_string(),
            ends_at_ms: v.get("endsAt")?.as_u64()?,
        })
    });
    TlCourt {
        clearing,
        pause,
        court_id: c.court_id,
        court: c.court,
        location: c.location,
        match_id: c.match_id,
        match_name: c.match_name,
        round_name: c.round_name,
        class_label: c.class_label,
        team1: c.team1,
        team2: c.team2,
        sets: c.sets,
        tablet_connected: c.tablet_connected,
        injury: c.injury,
        official_call: c.official_call,
        scorekeeper: c.scorekeeper,
        scorekeeper_assigned: c.scorekeeper_assigned,
        locked: c.locked,
        on_court_since_ms: c.on_court_since_ms,
        best_of: c.best_of,
        target_score: c.target_score,
        cap_score: c.cap_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{
        BtpCourt, BtpLocation, BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult,
        MatchStatus,
    };
    use crate::config::DisciplineHallRule;
    use crate::tablet::state::PreparationCall;

    /// Spieler ohne Lizenznummer — die Identität läuft dann über den Namen,
    /// und verschiedene Namen sind verschiedene Personen. (Mit derselben
    /// Lizenz für alle wären sie es nicht, was Verfügbarkeitsprüfungen
    /// unbrauchbar machte.)
    fn player(name: &str) -> BtpPlayer {
        BtpPlayer {
            id: 0,
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: Some("GER".to_string()),
            club: None,
        }
    }

    /// Spieler mit Lizenznummer — nur für den Datensparsamkeits-Test, der
    /// belegen muss, dass die Nummer den Host nicht verlässt.
    fn licensed_player(name: &str, license: &str) -> BtpPlayer {
        BtpPlayer {
            member_id: Some(license.to_string()),
            ..player(name)
        }
    }

    fn a_match(id: i64) -> BtpMatch {
        BtpMatch {
            id,
            draw_id: 1,
            planning_id: id,
            draw_name: "HE A".to_string(),
            discipline: Discipline::MensSingles,
            class_label: "A".to_string(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: vec![player("Müller")],
            team2: vec![player("Schmidt")],
            entry1_id: 0,
            entry2_id: 0,
            court: None,
            court_id: None,
            sets: Vec::new(),
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::Scheduled,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    fn snap(courts: Vec<BtpCourt>, matches: Vec<BtpMatch>, locs: Vec<BtpLocation>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "Testturnier".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: locs,
            court_infos: courts,
            matches,
            events: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn a_court(id: i64, location_id: Option<i64>) -> BtpCourt {
        BtpCourt {
            id,
            name: format!("Feld {id}"),
            location_id,
            sort_order: id,
        }
    }

    fn state_with(snapshot: BtpSnapshot, config: &AppConfig) -> TlState {
        let tablet = TabletState::default();
        tablet.set_snapshot(snapshot);
        build_state(&tablet, config, 1_000_000, 7)
    }

    #[test]
    fn without_a_tournament_the_state_is_empty_but_valid() {
        // Die Seite soll „warte auf Turnierdaten" zeigen können, statt auf
        // einen Fehler zu laufen — bts-light startet regelmäßig, bevor BTP
        // etwas geladen hat.
        let tablet = TabletState::default();
        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 3);
        assert_eq!(s.rev, 3);
        assert_eq!(s.server_now_ms, 1_000_000);
        assert!(s.courts.is_empty());
        assert!(s.queue.is_empty());
        assert!(!s.multi_hall);
    }

    #[test]
    fn the_queue_holds_only_playable_matches() {
        // Dieselbe Bedingung wie bei der automatischen Vergabe. Ein Spiel,
        // dessen Gegner noch aus einem Vorspiel kommt, könnte niemand
        // sinnvoll vergeben — es gehört nicht in die Liste.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let mut done = a_match(2);
        done.status = MatchStatus::Finished;
        let mut open = a_match(3);
        open.team2 = Vec::new();

        let s = state_with(
            snap(
                vec![a_court(1, None)],
                vec![running, done, open, a_match(4)],
                Vec::new(),
            ),
            &AppConfig::default(),
        );
        let ids: Vec<i64> = s.queue.iter().map(|m| m.match_id).collect();
        assert_eq!(ids, vec![4]);
    }

    #[test]
    fn the_queue_is_sorted_like_the_automatic_assignment() {
        // Gerufene zuerst, dann nach Ansetzung. Zeigte die Liste eine andere
        // Reihenfolge als die Automatik benutzt, verlöre die Turnierleitung
        // das Vertrauen in beide.
        let mut early = a_match(1);
        early.planned_time = Some(202_608_081_200);
        let mut late = a_match(2);
        late.planned_time = Some(202_608_081_600);
        let mut called = a_match(3);
        called.planned_time = Some(202_608_081_800);

        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![early, late, called], Vec::new()));
        tablet.add_preparation_call(PreparationCall {
            match_id: 3,
            location_id: None,
            called_at_ms: 500,
        });

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let ids: Vec<i64> = s.queue.iter().map(|m| m.match_id).collect();
        assert_eq!(ids, vec![3, 1, 2], "gerufen zuerst, dann nach Ansetzung");
    }

    #[test]
    fn a_called_match_carries_its_call_and_hall() {
        // „In Vorbereitung seit X Minuten" ist die wichtigste Wartezeit der
        // ganzen Ansicht.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            vec![a_match(7)],
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
        ));
        tablet.add_preparation_call(PreparationCall {
            match_id: 7,
            location_id: Some(2),
            called_at_ms: 900_000,
        });

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let m = &s.queue[0];
        assert_eq!(
            m.prep_call,
            Some(TlPrepCall {
                hall: "Halle B".to_string(),
                called_at_ms: 900_000
            })
        );
        assert_eq!(m.hall, "Halle B");
        assert_eq!(m.hall_source, HallSource::Call);
        assert!(s.multi_hall);
        assert_eq!(s.halls, vec!["Halle A".to_string(), "Halle B".to_string()]);
    }

    #[test]
    fn a_blocked_match_says_who_blocks_it_and_until_when() {
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![player("Müller")];
        running.team2 = vec![player("Gegner")];
        let mut waiting = a_match(2);
        waiting.team1 = vec![player("Müller")];
        waiting.team2 = vec![player("Frei")];

        let s = state_with(
            snap(vec![a_court(1, None)], vec![running, waiting], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(
            s.queue[0].blocked,
            Some(TlBlocked::Playing {
                players: vec!["Müller".to_string()]
            })
        );
    }

    #[test]
    fn courts_carry_the_running_match_and_its_clock() {
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let s = state_with(
            snap(vec![a_court(1, None)], vec![running], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts.len(), 1);
        assert_eq!(s.courts[0].court_id, 1);
        assert_eq!(s.courts[0].match_id, 7);
        assert_eq!(s.courts[0].team1, vec!["Müller".to_string()]);
    }

    #[test]
    fn the_state_carries_the_frame_the_page_needs() {
        let mut cfg = AppConfig::default();
        cfg.auto_assign.enabled = true;
        cfg.auto_assign.wait_minutes = 2.5;
        cfg.call_timer.enabled = true;
        let mut sn = snap(vec![a_court(1, None)], vec![a_match(1)], Vec::new());
        sn.rest_minutes = Some(20);

        let s = state_with(sn, &cfg);
        assert_eq!(s.tournament, "Testturnier");
        assert!(s.auto_assign.enabled);
        assert_eq!(s.auto_assign.wait_minutes, 2.5);
        assert!(s.call_timer.enabled);
        assert_eq!(s.rest_minutes, Some(20));
    }

    #[test]
    fn a_single_hall_tournament_offers_no_hall_filter() {
        let s = state_with(
            snap(
                vec![a_court(1, Some(1))],
                vec![a_match(1)],
                vec![BtpLocation {
                    id: 1,
                    name: "Main Location".to_string(),
                }],
            ),
            &AppConfig::default(),
        );
        assert!(!s.multi_hall, "eine Halle → kein Filter");
    }

    #[test]
    fn a_very_long_queue_is_capped_and_says_so() {
        // Große Turniere haben mehrere hundert wartende Spiele. Die Liste
        // ist nach Dringlichkeit sortiert; was hinten wegfällt, wird
        // gemeldet statt still unterschlagen.
        let matches: Vec<BtpMatch> = (1..=QUEUE_LIMIT_PER_HALL as i64 + 5).map(a_match).collect();
        let s = state_with(snap(Vec::new(), matches, Vec::new()), &AppConfig::default());
        assert_eq!(s.queue.len(), QUEUE_LIMIT_PER_HALL);
        // Ohne Hallenzuordnung ist die Gruppe der leere Name — auch sie
        // meldet ihre Kappung, statt sie zu verschweigen.
        assert_eq!(s.truncated_halls, vec![String::new()]);
    }

    #[test]
    fn a_court_still_held_by_a_finished_match_is_not_shown_as_free() {
        // Sonst zeigt die Seite ein leeres Feld an, das die Vergabe-Prüfung
        // als belegt ablehnt — der Helfer tippt gegen eine unsichtbare Wand.
        // Der Zustand ist normal: BTP räumt das Feld erst nach einigen
        // Abfragen ab.
        let mut done = a_match(9);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = state_with(
            snap(vec![a_court(1, None)], vec![done, a_match(2)], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts[0].match_id, 0, "kein LAUFENDES Spiel");
        assert_eq!(
            s.courts[0].clearing,
            Some(9),
            "aber das Feld ist noch nicht frei"
        );
    }

    #[test]
    fn a_genuinely_free_court_says_so() {
        let s = state_with(
            snap(vec![a_court(1, None)], vec![a_match(2)], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts[0].match_id, 0);
        assert_eq!(s.courts[0].clearing, None);
    }

    #[test]
    fn the_queue_cap_applies_per_hall_not_globally() {
        // Global gekappt könnte eine ganze Halle wegfallen: Die Sortierung
        // zieht die frühen Runden der ersten Halle nach vorn, und das Gerät
        // in Halle C sähe eine leere Liste, obwohl dort hundert Spiele
        // warten. Das verletzt „nie stillschweigend ausgeblendet".
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: "HE A".to_string(),
            hall: "Halle A".to_string(),
        });
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: "HE B".to_string(),
            hall: "Halle B".to_string(),
        });

        let mut matches = Vec::new();
        for i in 1..=(QUEUE_LIMIT_PER_HALL as i64 + 10) {
            let mut m = a_match(i);
            m.draw_name = "HE A".to_string();
            m.planned_time = Some(202_608_080_800 + i); // Halle A zuerst
            matches.push(m);
        }
        for i in 1..=5 {
            let mut m = a_match(1000 + i);
            m.draw_name = "HE B".to_string();
            m.planned_time = Some(202_608_081_800 + i); // Halle B später
            matches.push(m);
        }

        let s = state_with(
            snap(
                vec![a_court(1, Some(1)), a_court(2, Some(2))],
                matches,
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
            ),
            &cfg,
        );

        let in_b = s.queue.iter().filter(|m| m.hall == "Halle B").count();
        assert_eq!(in_b, 5, "Halle B darf nicht von Halle A verdrängt werden");
        assert_eq!(
            s.truncated_halls,
            vec!["Halle A".to_string()],
            "nur Halle A wurde gekappt, und das steht dort"
        );
    }

    #[test]
    fn the_hall_name_is_canonicalised_to_the_btp_spelling() {
        // Die Regel wird von Hand getippt; die Vergabe vergleicht sie
        // ohne Rücksicht auf Groß-/Kleinschreibung. Gäbe die Anzeige die
        // getippte Schreibweise aus, fände der Hallenfilter das Spiel
        // nicht — und weil es eine Halle *hat*, landete es auch nicht im
        // Abschnitt „ohne Hallenzuordnung". Es verschwände lautlos.
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "halle b".to_string(), // klein getippt
        });
        let s = state_with(
            snap(
                vec![a_court(1, Some(1)), a_court(2, Some(2))],
                vec![a_match(7)],
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
            ),
            &cfg,
        );
        assert_eq!(s.queue[0].hall, "Halle B", "Schreibweise aus BTP");
        assert!(s.halls.contains(&s.queue[0].hall));
    }

    #[test]
    fn the_rest_time_matches_the_one_actually_enforced() {
        // Der angezeigte Pausenwert muss der sein, nach dem auch die
        // Blockier-Zeiten im selben Datensatz gerechnet sind — sonst
        // widerspricht die Seite sich selbst.
        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 30.0;
        let mut sn = snap(Vec::new(), vec![a_match(1)], Vec::new());
        sn.rest_minutes = Some(20); // BTP sagt etwas anderes

        let s = state_with(sn, &cfg);
        assert_eq!(
            s.rest_minutes,
            Some(30),
            "die Konfiguration schlägt BTP — wie bei der Vergabe"
        );
    }

    #[test]
    fn the_pause_is_republished_as_known_fields_only() {
        // Der Pausen-Block kommt roh vom Zähltablett. Würde er unverändert
        // weitergereicht, könnte ein Tablett beliebigen Inhalt an alle
        // Turnierleitungs-Geräte und durch den Relay schicken.
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"game","endsAt":1700000000000,"heimlich":"streng geheim"}}"#
                .to_string(),
        );

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let pause = s.courts[0].pause.as_ref().expect("Pause vorhanden");
        assert_eq!(pause.kind, "game");
        assert_eq!(pause.ends_at_ms, 1_700_000_000_000);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("heimlich"),
            "unbekannte Felder des Tabletts dürfen nicht weiterwandern: {json}"
        );
    }

    /// Sammelt alle Feldnamen eines JSON-Baums.
    fn field_names(v: &serde_json::Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, val) in map {
                    out.insert(k.clone());
                    field_names(val, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|i| field_names(i, out)),
            _ => {}
        }
    }

    #[test]
    fn every_published_field_is_deliberately_allowed() {
        // Der strukturelle Wächter: Statt nach verbotenen Wörtern zu
        // suchen (was nur findet, woran jemand gedacht hat), wird JEDES
        // ausgelieferte Feld gegen eine bewusst gepflegte Liste geprüft.
        // Wer ein Feld hinzufügt, muss es hier eintragen — und dabei
        // begründen, warum es nach außen darf.
        const ERLAUBT: &[&str] = &[
            // Rahmen
            "rev",
            "server_now_ms",
            "tournament",
            "multi_hall",
            "halls",
            "rest_minutes",
            "auto_assign",
            "call_timer",
            "enabled",
            "wait_minutes",
            "active_hall",
            "second_call_minutes",
            "third_call_minutes",
            "courts",
            "queue",
            "truncated_halls",
            // Feld
            "court_id",
            "court",
            "location",
            "match_id",
            "match_name",
            "round_name",
            "class_label",
            "team1",
            "team2",
            "sets",
            "tablet_connected",
            "injury",
            "official_call",
            "pause",
            "kind",
            "ends_at_ms",
            "scorekeeper",
            "scorekeeper_assigned",
            "locked",
            "clearing",
            "on_court_since_ms",
            "best_of",
            "target_score",
            "cap_score",
            // Warteliste
            "match_num",
            "planned_time",
            "draw_name",
            "hall",
            "hall_source",
            "prep_call",
            "called_at_ms",
            "blocked",
            "reason",
            "players",
            "until_ms",
        ];

        let tablet = TabletState::default();
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![running, a_match(2)],
            Vec::new(),
        ));
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"game","endsAt":1700000000000}}"#.to_string(),
        );
        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);

        let value = serde_json::to_value(&s).unwrap();
        let mut names = std::collections::BTreeSet::new();
        field_names(&value, &mut names);
        let unerlaubt: Vec<&String> = names
            .iter()
            .filter(|n| !ERLAUBT.contains(&n.as_str()))
            .collect();
        assert!(
            unerlaubt.is_empty(),
            "Nicht freigegebene Felder im Anzeige-Zustand: {unerlaubt:?} — \
             eintragen und begründen, warum sie nach außen dürfen"
        );
    }

    #[test]
    fn the_state_never_carries_personal_data_beyond_its_purpose() {
        // Diese Daten laufen über eine aus dem Internet erreichbare Seite.
        // Der Test schlägt fehl, sobald jemand ein Feld nachrüstet, das
        // Lizenznummer, Geburtsjahr oder Nationalität transportiert — er
        // macht die Datenschutzregel durchsetzbar statt nur dokumentiert.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        running.team2 = vec![licensed_player("Gegner", "08-005678")];
        let mut waiting = a_match(2);
        waiting.team1 = vec![licensed_player("Weber", "08-009999")];
        waiting.team2 = vec![licensed_player("Fischer", "08-004321")];

        let s = state_with(
            snap(vec![a_court(1, None)], vec![running, waiting], Vec::new()),
            &AppConfig::default(),
        );
        let json = serde_json::to_string(&s).unwrap().to_lowercase();

        for verboten in [
            "08-001234",  // die Lizenznummer aus dem Fixture
            "member",     // Lizenznummer-Feld
            "nationalit", // Nationalität (nur für die Sprachwahl der Ansage)
            "ger",        // deren Wert aus dem Fixture
            "birth",      // Geburtsjahr — laut Projektregel nirgends
            "geburt",
            "battery", // Akkustand: keine Geräte-Übersicht in diesem Feature
            "serving", // Aufschlag: Zählhilfe, keine Vergabehilfe
        ] {
            assert!(
                !json.contains(verboten),
                "'{verboten}' darf nicht im Anzeige-Zustand stehen: {json}"
            );
        }
        // Gegenprobe: Die Namen, die die Turnierleitung zum Arbeiten braucht,
        // sind sehr wohl da — sonst prüfte der Test nur einen leeren Zustand.
        assert!(json.contains("müller"));
    }
}
