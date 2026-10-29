//! Punktverlauf und Aufschlag an badhub (`tverlauf`, badhub ADR-0060).
//!
//! Je Match geht der **ganze** Punktverlauf des Tablets ([`MatchTimeline`],
//! ADR 0014/0015) samt den `serve_start`-Ereignissen des Zettels (ADR 0037)
//! an badhub. badhub rechnet daraus Aufschläger, Aufschlagfeld und Annehmer
//! je Ballwechsel selbst nach — deshalb gehen nur Satzbeginn und Punktfolge
//! über den Draht, keine Aufstellung je Ballwechsel.
//!
//! Grundsätze:
//! - **Ersetzen statt anhängen**: jeder Eintrag ersetzt bei badhub den
//!   gespeicherten Verlauf (wie `rally_sync` zwischen Tablet und Host). Ein
//!   Undo, ein Host-Neustart oder ein verlorener Push heilt sich damit.
//! - **Nur Geändertes**: [`VerlaufStand`] merkt sich je Match den Hash des
//!   zuletzt **angenommenen** Eintrags. Ein abgelehnter Push wird im nächsten
//!   Takt erneut versucht.
//! - **Datenschutz**: keine Namen (nur Team 0/1 und Spielerindex). Vom Zettel
//!   gehen **ausschließlich** `serve_start` mit — keine Sanktionen
//!   ([`EventKind::is_sanction`]), keine Verletzungen. Pausen nur als
//!   11er-/Satzpause, keine Behandlungspause. badhub prüft das ein zweites
//!   Mal (Positivliste in `Badhub\Live\Verlauf::bereinigen()`).

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use relay_proto::{EventKind, MatchEvent, MatchTimeline};
use serde::Serialize;

use crate::btp::model::BtpSnapshot;
use crate::tablet::sheet::MatchSheet;

/// Höchstzahl Matches je Nachricht — badhub nimmt nicht mehr an.
pub const MAX_SPIELE_JE_NACHRICHT: usize = 64;

/// Pausenarten, die an badhub gehen (keine Behandlungspause: Gesundheitsdaten).
const PAUSEN: [&str; 2] = ["eleven", "game"];

#[derive(Debug, Serialize, PartialEq)]
pub struct VerlaufMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub rid: u64,
    /// turnier.de-GUID (kanonisch) — badhub ordnet den Push damit dem
    /// Kind-Turnier zu, wie bei `tset`/`sched`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_uuid: Option<String>,
    pub matches: Vec<VerlaufEintrag>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VerlaufEintrag {
    #[serde(rename = "_id")]
    pub id: String,
    pub doubles: bool,
    pub fmt: VerlaufFormat,
    pub timeline: MatchTimeline,
    pub serve: Vec<Aufschlag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause: Option<Pause>,
}

/// Zählweise — badhub braucht sie für Satzende, Deckel und Pause bei 11.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VerlaufFormat {
    pub target: i64,
    pub cap: i64,
    #[serde(rename = "bestOf")]
    pub best_of: i64,
    #[serde(rename = "intervalAt", skip_serializing_if = "Option::is_none")]
    pub interval_at: Option<i64>,
}

/// Ein `serve_start` in Wire-Form: **nur** die Aufstellungsfelder, kein
/// Zeitstempel, keine Ereigniskennung.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Aufschlag {
    pub kind: &'static str,
    pub set: i64,
    #[serde(rename = "afterN")]
    pub after_n: i64,
    pub team: i64,
    pub player: i64,
    #[serde(rename = "receiverTeam")]
    pub receiver_team: i64,
    #[serde(rename = "receiverPlayer")]
    pub receiver_player: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pause {
    pub kind: String,
    #[serde(rename = "endsAt", skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<u64>,
}

/// Die gültigen `serve_start` eines Zettels: zurückgenommene fallen weg,
/// alle anderen Arten (Karten, Verletzung, Schiri-Ruf …) ohnehin.
pub fn aufschlaege(sheet: &MatchSheet) -> Vec<Aufschlag> {
    let zurueck: HashSet<&str> = sheet
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Retract)
        .map(|e| e.retracts.as_str())
        .collect();
    sheet
        .events
        .iter()
        .filter(|e| e.kind == EventKind::ServeStart && !zurueck.contains(e.id.as_str()))
        .map(aufschlag_wire)
        .collect()
}

fn aufschlag_wire(e: &MatchEvent) -> Aufschlag {
    Aufschlag {
        kind: "serve_start",
        set: e.set,
        after_n: e.after_n,
        team: e.team,
        player: e.player,
        receiver_team: e.receiver_team,
        receiver_player: e.receiver_player,
    }
}

/// Laufende Pause aus dem Tablet-Zustand (`STATE.pause` = `{kind, endsAt}`),
/// nur 11er- und Satzpause.
pub fn pause_aus_tablet(roh: Option<&serde_json::Value>) -> Option<Pause> {
    let p = roh?;
    let kind = p.get("kind")?.as_str()?;
    if !PAUSEN.contains(&kind) {
        return None;
    }
    Some(Pause {
        kind: kind.to_string(),
        ends_at: p.get("endsAt").and_then(|v| v.as_u64()),
    })
}

/// Einträge für alle Matches des Snapshots, zu denen es einen Punktverlauf
/// oder ein Aufschlagereignis gibt. Matches ohne beides (Papier-Spiele)
/// fehlen — für sie hat badhub nichts zu zeigen.
pub fn eintraege(
    snapshot: &BtpSnapshot,
    timeline: impl Fn(i64) -> Option<MatchTimeline>,
    sheet: impl Fn(i64) -> Option<MatchSheet>,
    pausen: &HashMap<i64, serde_json::Value>,
) -> Vec<VerlaufEintrag> {
    let mut out = Vec::new();
    for m in &snapshot.matches {
        let tl = timeline(m.id);
        let serve = sheet(m.id).map(|s| aufschlaege(&s)).unwrap_or_default();
        if tl.is_none() && serve.is_empty() {
            continue;
        }
        out.push(VerlaufEintrag {
            id: format!("btp_{}", m.id),
            doubles: m.team1.len() >= 2 || m.team2.len() >= 2,
            fmt: VerlaufFormat {
                target: m.scoring.target_score,
                cap: m.scoring.cap_score,
                best_of: m.scoring.best_of,
                interval_at: m.scoring.interval_at,
            },
            timeline: tl.unwrap_or_default(),
            serve,
            pause: pause_aus_tablet(pausen.get(&m.id)),
        });
    }
    out
}

/// Merkt sich je Match, was badhub zuletzt angenommen hat.
#[derive(Debug, Default)]
pub struct VerlaufStand {
    gesendet: HashMap<String, u64>,
}

impl VerlaufStand {
    /// Die geänderten Einträge, höchstens [`MAX_SPIELE_JE_NACHRICHT`]. Der
    /// Rest kommt im nächsten Takt — nach einem Neustart also gestaffelt.
    pub fn geaendert(&self, eintraege: Vec<VerlaufEintrag>) -> Vec<VerlaufEintrag> {
        eintraege
            .into_iter()
            .filter(|e| self.gesendet.get(&e.id) != Some(&hash(e)))
            .take(MAX_SPIELE_JE_NACHRICHT)
            .collect()
    }

    /// Nach einem angenommenen Push: diese Stände gelten als gesendet.
    pub fn angenommen(&mut self, eintraege: &[VerlaufEintrag]) {
        for e in eintraege {
            self.gesendet.insert(e.id.clone(), hash(e));
        }
    }

    /// Neues Turnier → alles neu senden.
    pub fn leeren(&mut self) {
        self.gesendet.clear();
    }
}

fn hash(e: &VerlaufEintrag) -> u64 {
    let mut h = DefaultHasher::new();
    serde_json::to_string(e).unwrap_or_default().hash(&mut h);
    h.finish()
}

pub fn build_verlauf(
    matches: Vec<VerlaufEintrag>,
    rid: u64,
    tournament_uuid: Option<String>,
) -> VerlaufMessage {
    VerlaufMessage {
        kind: "tverlauf",
        rid,
        tournament_uuid,
        matches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{
        BtpMatch, BtpPlayer, Discipline, MatchResult, MatchStatus, ScoringFormat,
    };
    use relay_proto::{Phase, TimelineSet};

    fn spieler(name: &str) -> BtpPlayer {
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

    fn spiel(id: i64, doppel: bool) -> BtpMatch {
        let team = |a: &str, b: &str| {
            if doppel {
                vec![spieler(a), spieler(b)]
            } else {
                vec![spieler(a)]
            }
        };
        BtpMatch {
            display_order: None,
            highlight: 0,
            from1: None,
            from2: None,
            id,
            draw_id: 1,
            planning_id: 1000 + id,
            draw_name: "HD".to_string(),
            discipline: Discipline::MensDoubles,
            class_label: String::new(),
            round_name: "R1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: team("Anna Müller", "Clara Kurz"),
            team2: team("Ben Schmidt", "Dora Lang"),
            entry1_id: 0,
            entry2_id: 0,
            court: Some("1".to_string()),
            court_id: Some(1),
            location_id: None,
            sets: vec![],
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::OnCourt,
            finished_at: None,
            pause_ms: None,
            preparation_call_ts: None,
            preparation_hall: None,
            official1_id: None,
            official2_id: None,
            scoring: ScoringFormat {
                best_of: 3,
                target_score: 21,
                cap_score: 30,
                interval_at: Some(11),
            },
            team1_seed: None,
            team2_seed: None,
        }
    }

    fn snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "Test".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches,
        }
    }

    fn ereignis(id: &str, kind: EventKind) -> MatchEvent {
        MatchEvent {
            id: id.to_string(),
            seq: 1,
            set: 1,
            after_n: 0,
            score_a: 0,
            score_b: 0,
            ts_ms: 1_755_600_000_000,
            kind,
            team: 0,
            player: 1,
            receiver_team: 1,
            receiver_player: 0,
            phase: Phase::PreMatch,
            retracts: String::new(),
        }
    }

    fn verlauf(points: &str) -> MatchTimeline {
        MatchTimeline {
            sets: vec![TimelineSet {
                start_a: 0,
                start_b: 0,
                points: points.to_string(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn nur_gueltige_serve_start_gehen_mit_keine_sanktionen() {
        let mut zurueck = ereignis("aa02", EventKind::Retract);
        zurueck.retracts = "aa01".to_string();
        let sheet = MatchSheet {
            events: vec![
                ereignis("aa01", EventKind::ServeStart),
                zurueck,
                ereignis("aa03", EventKind::ServeStart),
                ereignis("aa04", EventKind::CardYellow),
                ereignis("aa05", EventKind::CardRed),
                ereignis("aa06", EventKind::Disqualified),
                ereignis("aa07", EventKind::InjuryStart),
            ],
            ..Default::default()
        };
        let a = aufschlaege(&sheet);
        assert_eq!(a.len(), 1, "nur der nicht zurückgenommene serve_start");
        assert_eq!(a[0].player, 1);
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("card") && !json.contains("injury") && !json.contains("ts"));
    }

    #[test]
    fn pause_nur_elf_und_satz() {
        let p = |k: &str| serde_json::json!({"kind": k, "endsAt": 1_755_600_060_000u64});
        assert_eq!(pause_aus_tablet(Some(&p("eleven"))).unwrap().kind, "eleven");
        assert_eq!(
            pause_aus_tablet(Some(&p("game"))).unwrap().ends_at,
            Some(1_755_600_060_000)
        );
        assert!(pause_aus_tablet(Some(&p("injury"))).is_none());
        assert!(pause_aus_tablet(None).is_none());
    }

    #[test]
    fn eintrag_traegt_vertrag_mit_badhub_und_keine_namen() {
        let snap = snapshot(vec![spiel(7, true), spiel(8, false)]);
        let mut pausen = HashMap::new();
        pausen.insert(7, serde_json::json!({"kind": "eleven", "endsAt": 5u64}));
        let e = eintraege(
            &snap,
            |id| (id == 7).then(|| verlauf("ABBA")),
            |id| {
                (id == 7).then(|| MatchSheet {
                    events: vec![ereignis("aa01", EventKind::ServeStart)],
                    ..Default::default()
                })
            },
            &pausen,
        );
        assert_eq!(e.len(), 1, "Spiel ohne Verlauf und ohne Aufschlag fehlt");
        let json = serde_json::to_value(&e[0]).unwrap();
        assert_eq!(json["_id"], "btp_7");
        assert_eq!(json["doubles"], true);
        assert_eq!(json["fmt"]["target"], 21);
        assert_eq!(json["fmt"]["intervalAt"], 11);
        assert_eq!(json["timeline"]["sets"][0]["points"], "ABBA");
        assert_eq!(json["timeline"]["sets"][0]["startA"], 0);
        assert_eq!(json["serve"][0]["kind"], "serve_start");
        assert_eq!(json["serve"][0]["afterN"], 0);
        assert_eq!(json["serve"][0]["receiverTeam"], 1);
        assert_eq!(json["pause"]["kind"], "eleven");
        let text = json.to_string();
        assert!(
            !text.contains("Anna") && !text.contains("Müller"),
            "keine Namen"
        );

        let msg = build_verlauf(e, 3, Some("GUID".to_string()));
        let m = serde_json::to_value(&msg).unwrap();
        assert_eq!(m["type"], "tverlauf");
        assert_eq!(m["tournament_uuid"], "GUID");
    }

    #[test]
    fn stand_sendet_nur_geaendertes_und_wiederholt_abgelehntes() {
        let snap = snapshot(vec![spiel(7, true), spiel(8, true)]);
        let mit = |p7: &'static str| {
            eintraege(
                &snap,
                move |id| Some(verlauf(if id == 7 { p7 } else { "A" })),
                |_| None,
                &HashMap::new(),
            )
        };
        let mut stand = VerlaufStand::default();
        let erst = stand.geaendert(mit("AB"));
        assert_eq!(erst.len(), 2, "beim ersten Mal alles");
        // Abgelehnt → nichts angenommen → nächster Takt schickt wieder beide.
        assert_eq!(stand.geaendert(mit("AB")).len(), 2);
        stand.angenommen(&erst);
        assert!(
            stand.geaendert(mit("AB")).is_empty(),
            "unverändert → kein Push"
        );
        let neu = stand.geaendert(mit("ABA"));
        assert_eq!(neu.len(), 1);
        assert_eq!(neu[0].id, "btp_7");
        stand.leeren();
        assert_eq!(
            stand.geaendert(mit("ABA")).len(),
            2,
            "neues Turnier → alles neu"
        );
    }

    #[test]
    fn hoechstens_64_spiele_je_nachricht() {
        let snap = snapshot((1..=70).map(|i| spiel(i, false)).collect());
        let e = eintraege(&snap, |_| Some(verlauf("A")), |_| None, &HashMap::new());
        assert_eq!(
            VerlaufStand::default().geaendert(e).len(),
            MAX_SPIELE_JE_NACHRICHT
        );
    }
}
