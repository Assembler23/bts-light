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
//! - **Nur Geändertes**: [`VerlaufStand`] merkt sich je Match die
//!   Änderungsmarke des zuletzt **angenommenen** Eintrags; gebaut wird nur,
//!   was sich geändert hat ([`kandidaten`]). badhub verwirft einzelne
//!   Einträge mit Status 200 und nennt nur ihre Zahl — dann gilt das Paket
//!   nicht als angenommen, und nach [`MAX_VERSUCHE`] Versuchen wird geparkt.
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

use crate::btp::model::{BtpMatch, BtpSnapshot};
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
///
/// Berichtigte Mannschaften (ADR 0067) sind schon eingerechnet: badhub
/// nimmt nur `serve_start` an und bekommt sie deshalb in Endkoordinaten.
pub fn aufschlaege(sheet: &MatchSheet) -> Vec<Aufschlag> {
    let events = crate::tablet::berichtigung::in_endkoordinaten(&sheet.events);
    let zurueck: HashSet<&str> = events
        .iter()
        .filter(|e| e.kind == EventKind::Retract)
        .map(|e| e.retracts.as_str())
        .collect();
    events
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
    snapshot
        .matches
        .iter()
        .filter_map(|m| eintrag_fuer(m, &timeline, &sheet, pausen))
        .collect()
}

/// Der Eintrag eines Spiels — `None`, wenn es weder Punktverlauf noch
/// Aufschlagereignis gibt.
pub fn eintrag_fuer(
    m: &BtpMatch,
    timeline: impl Fn(i64) -> Option<MatchTimeline>,
    sheet: impl Fn(i64) -> Option<MatchSheet>,
    pausen: &HashMap<i64, serde_json::Value>,
) -> Option<VerlaufEintrag> {
    let tl = timeline(m.id);
    let serve = sheet(m.id).map(|s| aufschlaege(&s)).unwrap_or_default();
    if tl.is_none() && serve.is_empty() {
        return None;
    }
    Some(VerlaufEintrag {
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
    })
}

/// Änderungsmarke eines Spiels: (Verlauf, Zettel, Rest). Die ersten beiden
/// kommen aus den Speichern (`0` = kein Eintrag, sonst Marke + 1); die
/// dritte ist ein Hash über alles, was sonst noch im Eintrag steht und sich
/// ohne ein Schreiben in den Speichern ändern kann: Pausenzustand, Zählweise
/// und Einzel/Doppel aus dem BTP-Snapshot.
pub type Marke = (u64, u64, u64);

/// Ein Spiel, zu dem es etwas zu senden geben könnte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kandidat {
    pub match_id: i64,
    /// Kennung bei badhub (`btp_<id>`).
    pub id: String,
    pub marke: Marke,
}

/// Die Spiele des Snapshots mit Verlauf oder Zettel, samt ihrer
/// Änderungsmarke — **ohne** einen Verlauf zu klonen oder zu serialisieren.
/// Bis v0.9.382 baute und hashte jeder 5-s-Takt alle Einträge aller Spiele.
pub fn kandidaten(
    snapshot: &BtpSnapshot,
    verlauf_marke: impl Fn(i64) -> Option<u64>,
    zettel_marke: impl Fn(i64) -> Option<u64>,
    pausen: &HashMap<i64, serde_json::Value>,
) -> Vec<Kandidat> {
    let mit_eintrag = |m: Option<u64>| m.map(|x| x + 1).unwrap_or(0);
    snapshot
        .matches
        .iter()
        .filter_map(|m| {
            let (v, z) = (verlauf_marke(m.id), zettel_marke(m.id));
            if v.is_none() && z.is_none() {
                return None;
            }
            let mut h = DefaultHasher::new();
            pause_aus_tablet(pausen.get(&m.id))
                .map(|p| (p.kind, p.ends_at))
                .hash(&mut h);
            // Zählweise und Einzel/Doppel: Eine Korrektur in BTP an einem
            // schon beendeten Spiel schriebe sonst nie mehr in einen Speicher
            // — und bliebe bei badhub für immer falsch.
            (
                m.scoring.target_score,
                m.scoring.cap_score,
                m.scoring.best_of,
                m.scoring.interval_at,
                m.team1.len() >= 2 || m.team2.len() >= 2,
            )
                .hash(&mut h);
            Some(Kandidat {
                match_id: m.id,
                id: format!("btp_{}", m.id),
                marke: (mit_eintrag(v), mit_eintrag(z), h.finish()),
            })
        })
        .collect()
}

/// So oft wird ein von badhub verworfener Eintrag versucht, bevor er
/// geparkt wird.
pub const MAX_VERSUCHE: u8 = 3;

/// Merkt sich je Match, was badhub zuletzt angenommen hat — und was es
/// verworfen hat.
#[derive(Debug, Default)]
pub struct VerlaufStand {
    /// Angenommene Marke je Spiel.
    gesendet: HashMap<String, Marke>,
    /// Verworfene Versuche je Spiel, zur Marke dieser Versuche. badhub
    /// nennt nur die ANZAHL der verworfenen Einträge eines Pakets, nicht
    /// welche — deshalb zählt jeder Eintrag des Pakets einen Versuch.
    versuche: HashMap<String, (Marke, u8)>,
    /// Verschiebung der Auswahl nach einer Ganz-Ablehnung.
    start: usize,
    /// Wann zuletzt entparkt wurde.
    entparkt: Option<std::time::Instant>,
}

/// Mindestabstand zwischen zwei Entparken bei einem bloßen Lebenszeichen.
/// Der volle `tset` kommt mindestens jede Minute — entparkte er jedes Mal,
/// ginge ein Verlauf, den badhub auf Dauer verwirft (ein Spiel außerhalb
/// seines Turniers), dreimal je Minute hinaus, für immer.
pub const ENTPARK_ABSTAND: std::time::Duration = std::time::Duration::from_secs(600);

impl VerlaufStand {
    /// Die Spiele, die jetzt zu senden sind: geändert und nicht geparkt,
    /// höchstens [`MAX_SPIELE_JE_NACHRICHT`]. Der Rest kommt im nächsten
    /// Takt — nach einem Neustart also gestaffelt.
    pub fn auswahl(&self, kandidaten: &[Kandidat]) -> Vec<Kandidat> {
        let offen: Vec<&Kandidat> = kandidaten
            .iter()
            .filter(|k| self.gesendet.get(&k.id) != Some(&k.marke))
            .filter(|k| !self.geparkt(k))
            .collect();
        if offen.is_empty() {
            return Vec::new();
        }
        // Nach einer Ganz-Ablehnung beginnt die Auswahl woanders: Sonst
        // blockierte ein Eintrag, an dem badhub scheitert, auf Dauer
        // dieselben ersten 64 — und alle dahinter kämen nie dran.
        let ab = self.start % offen.len();
        offen
            .iter()
            .cycle()
            .skip(ab)
            .take(offen.len().min(MAX_SPIELE_JE_NACHRICHT))
            .map(|k| (*k).clone())
            .collect()
    }

    /// Geparkt = mit dieser Marke schon [`MAX_VERSUCHE`]-mal verworfen. Eine
    /// neue Marke (das Spiel hat sich geändert) zählt von vorn.
    fn geparkt(&self, k: &Kandidat) -> bool {
        self.versuche
            .get(&k.id)
            .is_some_and(|(marke, n)| *marke == k.marke && *n >= MAX_VERSUCHE)
    }

    /// Nach einem angenommenen Push (badhub hat nichts verworfen): diese
    /// Stände gelten als gesendet.
    pub fn angenommen(&mut self, wahl: &[Kandidat]) {
        for k in wahl {
            self.gesendet.insert(k.id.clone(), k.marke);
            self.versuche.remove(&k.id);
        }
    }

    /// badhub hat mit 200 geantwortet, aber Einträge verworfen: Nichts gilt
    /// als gesendet, jeder Eintrag des Pakets zählt einen Versuch. Liefert
    /// die Zahl der dadurch NEU geparkten Einträge (fürs Log).
    pub fn verworfen(&mut self, wahl: &[Kandidat]) -> usize {
        let mut neu_geparkt = 0;
        for k in wahl {
            let n = match self.versuche.get(&k.id) {
                Some((marke, n)) if *marke == k.marke => n.saturating_add(1),
                _ => 1,
            };
            self.versuche.insert(k.id.clone(), (k.marke, n));
            if n == MAX_VERSUCHE {
                neu_geparkt += 1;
            }
        }
        neu_geparkt
    }

    /// Nach einer Ganz-Ablehnung (5xx, 413, 409): Die nächste Auswahl
    /// beginnt hinter dieser.
    pub fn rotieren(&mut self, um: usize) {
        self.start = self.start.wrapping_add(um.max(1));
    }

    /// Ein neuer voller `tset` ist bestätigt: badhub kennt jetzt womöglich
    /// Spiele, deren Verlauf es vorher verwarf — Geparktes darf wieder.
    ///
    /// `struktur` = der `tset` ging wegen einer Änderung hinaus (Feld,
    /// Aufruf, mehrere Spiele) oder nach einer Störung: Dann sofort. Bei
    /// einem bloßen Lebenszeichen höchstens alle [`ENTPARK_ABSTAND`].
    pub fn entparken(&mut self, struktur: bool, jetzt: std::time::Instant) {
        let faellig = self
            .entparkt
            .is_none_or(|t| jetzt.saturating_duration_since(t) >= ENTPARK_ABSTAND);
        if struktur || faellig {
            self.versuche.clear();
            self.entparkt = Some(jetzt);
        }
    }

    /// Neues Turnier → alles neu senden.
    pub fn leeren(&mut self) {
        *self = Self::default();
    }
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
            shuttles: 0,
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
            preparation_call_rank: None,
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

    /// Berichtigte Mannschaften (ADR 0067): badhub kennt nur `serve_start`,
    /// also gehen sie gekippt mit; eine Rücknahme der Berichtigung hebt das
    /// Kippen wieder auf.
    #[test]
    fn berichtigung_kippt_den_aufschlag_vor_dem_senden() {
        let mut k = ereignis("aa02", EventKind::TeamsCorrected);
        k.after_n = 4;
        k.seq = 2;
        let mut sheet = MatchSheet {
            events: vec![ereignis("aa01", EventKind::ServeStart), k],
            ..Default::default()
        };
        let a = aufschlaege(&sheet);
        assert_eq!(a.len(), 1, "die Berichtigung selbst geht nicht mit");
        assert_eq!((a[0].team, a[0].receiver_team), (1, 0));
        assert_eq!(a[0].player, 1, "Spielerindex bleibt");

        let mut zurueck = ereignis("aa03", EventKind::Retract);
        zurueck.retracts = "aa02".to_string();
        zurueck.after_n = 5;
        sheet.events.push(zurueck);
        let a = aufschlaege(&sheet);
        assert_eq!((a[0].team, a[0].receiver_team), (0, 1));
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

    /// Kandidaten für die Spiele 1..=n, alle mit derselben Verlaufs-Marke.
    fn kand(n: i64, marke: u64) -> Vec<Kandidat> {
        (1..=n)
            .map(|id| Kandidat {
                match_id: id,
                id: format!("btp_{id}"),
                marke: (marke, 0, 0),
            })
            .collect()
    }

    fn ids(wahl: &[Kandidat]) -> Vec<i64> {
        wahl.iter().map(|k| k.match_id).collect()
    }

    #[test]
    fn stand_sendet_nur_geaendertes_und_wiederholt_abgelehntes() {
        let mut stand = VerlaufStand::default();
        let erst = stand.auswahl(&kand(2, 1));
        assert_eq!(erst.len(), 2, "beim ersten Mal alles");
        // Abgelehnt → nichts angenommen → nächster Takt schickt wieder beide.
        assert_eq!(stand.auswahl(&kand(2, 1)).len(), 2);
        stand.angenommen(&erst);
        assert!(
            stand.auswahl(&kand(2, 1)).is_empty(),
            "unverändert → kein Push"
        );
        // Spiel 2 ändert sich.
        let mut k = kand(2, 1);
        k[1].marke = (2, 0, 0);
        assert_eq!(ids(&stand.auswahl(&k)), vec![2]);
    }

    #[test]
    fn turnierwechsel_leert_den_stand() {
        let mut stand = VerlaufStand::default();
        let alle = stand.auswahl(&kand(3, 1));
        stand.angenommen(&alle);
        stand.verworfen(&kand(1, 9));
        stand.rotieren(5);
        stand.leeren();
        assert_eq!(
            ids(&stand.auswahl(&kand(3, 1))),
            vec![1, 2, 3],
            "neues Turnier → alles neu, von vorn"
        );
    }

    #[test]
    fn hoechstens_64_spiele_je_nachricht() {
        assert_eq!(
            VerlaufStand::default().auswahl(&kand(70, 1)).len(),
            MAX_SPIELE_JE_NACHRICHT
        );
    }

    #[test]
    fn verworfen_groesser_null_stempelt_nichts() {
        // badhub antwortet mit 200, hat aber Einträge verworfen: Nichts gilt
        // als gesendet — der nächste Takt versucht es wieder.
        let mut stand = VerlaufStand::default();
        let wahl = stand.auswahl(&kand(2, 1));
        assert_eq!(stand.verworfen(&wahl), 0);
        assert_eq!(stand.auswahl(&kand(2, 1)).len(), 2);
    }

    #[test]
    fn nach_drei_versuchen_wird_geparkt() {
        let mut stand = VerlaufStand::default();
        for versuch in 1..=MAX_VERSUCHE {
            let wahl = stand.auswahl(&kand(2, 1));
            assert_eq!(wahl.len(), 2, "Versuch {versuch}");
            let neu_geparkt = stand.verworfen(&wahl);
            assert_eq!(neu_geparkt, if versuch == MAX_VERSUCHE { 2 } else { 0 });
        }
        assert!(stand.auswahl(&kand(2, 1)).is_empty(), "geparkt");
        // Ändert sich ein Spiel, zählt es von vorn — das andere bleibt
        // geparkt.
        let mut k = kand(2, 1);
        k[0].marke = (2, 0, 0);
        assert_eq!(ids(&stand.auswahl(&k)), vec![1]);
        // Ein dritter, unbeteiligter Eintrag ist nie betroffen.
        assert_eq!(ids(&stand.auswahl(&kand(3, 1))), vec![3]);
    }

    #[test]
    fn geparkter_eintrag_kommt_nach_neuem_tset_wieder() {
        let mut stand = VerlaufStand::default();
        for _ in 0..MAX_VERSUCHE {
            let wahl = stand.auswahl(&kand(1, 1));
            stand.verworfen(&wahl);
        }
        assert!(stand.auswahl(&kand(1, 1)).is_empty());
        // Ein neuer voller Stand ist bestätigt: badhub kennt das Spiel jetzt
        // womöglich.
        let jetzt = std::time::Instant::now();
        stand.entparken(true, jetzt);
        let wahl = stand.auswahl(&kand(1, 1));
        assert_eq!(ids(&wahl), vec![1]);
        stand.angenommen(&wahl);
        assert!(stand.auswahl(&kand(1, 1)).is_empty());
    }

    #[test]
    fn lebenszeichen_entparkt_hoechstens_alle_zehn_minuten() {
        // Ein Verlauf, den badhub auf Dauer verwirft, darf nicht mit jedem
        // minütlichen Lebenszeichen wieder dreimal hinausgehen.
        let mut stand = VerlaufStand::default();
        let parken = |stand: &mut VerlaufStand| {
            for _ in 0..MAX_VERSUCHE {
                let wahl = stand.auswahl(&kand(1, 1));
                stand.verworfen(&wahl);
            }
            assert!(stand.auswahl(&kand(1, 1)).is_empty());
        };
        let jetzt = std::time::Instant::now();
        parken(&mut stand);
        // Das erste Lebenszeichen entparkt (noch nie entparkt) …
        stand.entparken(false, jetzt);
        assert_eq!(stand.auswahl(&kand(1, 1)).len(), 1);
        parken(&mut stand);
        // … die folgenden neun Minuten lang keines mehr …
        stand.entparken(
            false,
            jetzt + ENTPARK_ABSTAND - std::time::Duration::from_secs(1),
        );
        assert!(stand.auswahl(&kand(1, 1)).is_empty());
        // … eine Strukturänderung aber sofort.
        stand.entparken(true, jetzt + std::time::Duration::from_secs(60));
        assert_eq!(stand.auswahl(&kand(1, 1)).len(), 1);
    }

    #[test]
    fn ein_verworfener_eintrag_haelt_die_guten_nicht_auf() {
        // badhub nennt nur die ZAHL der verworfenen Einträge. Ein Paket mit
        // einem schlechten und zwei guten Spielen wird deshalb als Ganzes
        // gezählt — die guten bekommen spätere Änderungen trotzdem hinaus.
        let mut stand = VerlaufStand::default();
        for _ in 0..MAX_VERSUCHE {
            let wahl = stand.auswahl(&kand(3, 1));
            stand.verworfen(&wahl);
        }
        assert!(stand.auswahl(&kand(3, 1)).is_empty(), "alle drei geparkt");
        // Spiel 2 bekommt einen Ballwechsel: Es geht allein hinaus und wird
        // angenommen.
        let mut k = kand(3, 1);
        k[1].marke = (2, 0, 0);
        let wahl = stand.auswahl(&k);
        assert_eq!(ids(&wahl), vec![2]);
        stand.angenommen(&wahl);
        assert!(stand.auswahl(&k).is_empty());
    }

    #[test]
    fn zaehlweise_und_doppel_stecken_in_der_marke() {
        let marke = |m: BtpMatch| {
            kandidaten(&snapshot(vec![m]), |_| Some(1), |_| None, &HashMap::new())[0].marke
        };
        let einzel = marke(spiel(7, false));
        assert_ne!(einzel, marke(spiel(7, true)), "Einzel/Doppel");
        let mut anders = spiel(7, false);
        anders.scoring.target_score += 4;
        assert_ne!(einzel, marke(anders), "Zählweise");
        assert_eq!(einzel, marke(spiel(7, false)), "unverändert bleibt gleich");
    }

    #[test]
    fn ganz_ablehnung_rotiert_die_auswahl() {
        // 100 offene Spiele, badhub lehnt das ganze Paket ab (5xx): Der
        // nächste Versuch trägt andere Spiele — sonst kämen 65..100 nie dran.
        let mut stand = VerlaufStand::default();
        let erste = stand.auswahl(&kand(100, 1));
        assert_eq!(ids(&erste)[..3], [1, 2, 3]);
        stand.rotieren(erste.len());
        let zweite = stand.auswahl(&kand(100, 1));
        assert_eq!(zweite.len(), MAX_SPIELE_JE_NACHRICHT);
        assert_eq!(ids(&zweite)[..3], [65, 66, 67]);
        assert_eq!(ids(&zweite)[36..39], [1, 2, 3], "läuft im Kreis weiter");
        // Bei nur einem offenen Spiel bleibt es dieses.
        let mut einzel = VerlaufStand::default();
        einzel.rotieren(1);
        assert_eq!(ids(&einzel.auswahl(&kand(1, 1))), vec![1]);
    }

    #[test]
    fn unveraenderte_spiele_werden_nicht_gebaut() {
        // Der Takt fragt nur die Marken ab. Gebaut (Verlauf geklont und
        // serialisiert) wird allein, was die Auswahl verlangt.
        let snap = snapshot((1..=5).map(|i| spiel(i, false)).collect());
        let gebaut = std::cell::Cell::new(0);
        let durchgang = |stand: &mut VerlaufStand, marke_von_3: u64| {
            let kandidaten = kandidaten(
                &snap,
                |id| Some(if id == 3 { marke_von_3 } else { 1 }),
                |_| None,
                &HashMap::new(),
            );
            let wahl = stand.auswahl(&kandidaten);
            for k in &wahl {
                let m = snap.matches.iter().find(|m| m.id == k.match_id).unwrap();
                let eintrag = eintrag_fuer(
                    m,
                    |_| {
                        gebaut.set(gebaut.get() + 1);
                        Some(verlauf("A"))
                    },
                    |_| None,
                    &HashMap::new(),
                );
                assert!(eintrag.is_some());
            }
            stand.angenommen(&wahl);
        };
        let mut stand = VerlaufStand::default();
        durchgang(&mut stand, 1);
        assert_eq!(gebaut.get(), 5, "beim ersten Mal alle");
        durchgang(&mut stand, 1);
        durchgang(&mut stand, 1);
        assert_eq!(gebaut.get(), 5, "ohne Änderung wird nichts gebaut");
        durchgang(&mut stand, 2);
        assert_eq!(gebaut.get(), 6, "nur das geänderte Spiel");
    }

    #[test]
    fn marke_steigt_bei_rally_sync_ereignis_und_pause() {
        let verlaeufe = crate::tablet::timeline::TimelineStore::default();
        let zettel = crate::tablet::sheet::SheetStore::default();
        assert_eq!(verlaeufe.marke(7), None, "ohne Verlauf keine Marke");
        assert_eq!(zettel.marke(7), None);

        // Ballwechsel, Voll-Abgleich und Spielende heben die Marke.
        assert!(verlaeufe.apply_rally(7, 1, 1, "A", 1, 0));
        let nach_rally = verlaeufe.marke(7).unwrap();
        assert!(verlaeufe.apply_sync(7, verlauf("AB")));
        let nach_sync = verlaeufe.marke(7).unwrap();
        assert!(nach_sync > nach_rally);
        verlaeufe.finalize(7, false);
        let nach_ende = verlaeufe.marke(7).unwrap();
        assert!(nach_ende > nach_sync);
        // Auch ein Nachzügler nach dem Spielende, der etwas ergänzt.
        assert!(verlaeufe.apply_nachzuegler(7, verlauf("ABA")));
        assert!(verlaeufe.marke(7).unwrap() > nach_ende);
        // Ein anderes Spiel bleibt unberührt.
        assert_eq!(verlaeufe.marke(8), None);

        // Ein Zettel-Ereignis hebt die Marke des Zettels.
        assert!(zettel.apply_event(7, ereignis("e1", EventKind::ServeStart)));
        let nach_ereignis = zettel.marke(7).unwrap();
        assert!(zettel.apply_event(7, ereignis("e2", EventKind::ServeStart)));
        let nach_zweitem = zettel.marke(7).unwrap();
        assert!(nach_zweitem > nach_ereignis);
        // Der Voll-Abgleich des Zettels ebenso.
        assert!(zettel.apply_event_sync(
            7,
            vec![
                ereignis("e1", EventKind::ServeStart),
                ereignis("e2", EventKind::ServeStart),
                ereignis("e3", EventKind::ServeStart),
            ]
        ));
        assert!(zettel.marke(7).unwrap() > nach_zweitem);

        // Ein Turnierwechsel verwirft die Marken (Match-IDs beginnen neu).
        verlaeufe.set_tournament("Anderes Turnier");
        assert_eq!(verlaeufe.marke(7), None);

        // Und eine Pause ändert die Marke des Kandidaten, ohne dass ein
        // Speicher geschrieben hätte.
        let snap = snapshot(vec![spiel(7, false)]);
        let marke = |pause: Option<serde_json::Value>| {
            let pausen: HashMap<i64, serde_json::Value> =
                pause.into_iter().map(|p| (7, p)).collect();
            kandidaten(
                &snap,
                |id| verlaeufe.marke(id),
                |id| zettel.marke(id),
                &pausen,
            )[0]
            .marke
        };
        let ohne = marke(None);
        let mit = marke(Some(serde_json::json!({"kind": "eleven", "endsAt": 1000})));
        assert_ne!(ohne, mit);
        assert_ne!(
            mit,
            marke(Some(serde_json::json!({"kind": "game", "endsAt": 1000})))
        );
        // Eine Behandlungspause geht nicht an badhub — also auch keine Änderung.
        assert_eq!(
            ohne,
            marke(Some(serde_json::json!({"kind": "medical", "endsAt": 1000})))
        );
    }

    #[test]
    fn spiele_ohne_verlauf_und_zettel_sind_keine_kandidaten() {
        let snap = snapshot(vec![spiel(1, false), spiel(2, false)]);
        let k = kandidaten(
            &snap,
            |id| (id == 2).then_some(0),
            |_| None,
            &HashMap::new(),
        );
        assert_eq!(ids(&k), vec![2]);
        // Marke 0 des Speichers (aus der Datei geladen, unberührt) heißt
        // „vorhanden" — und wird beim ersten Mal gesendet.
        assert_eq!(VerlaufStand::default().auswahl(&k).len(), 1);
    }
}
