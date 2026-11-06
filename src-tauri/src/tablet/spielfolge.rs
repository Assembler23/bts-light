//! Spielfolge-Anzeige (Spec `docs/features/spielfolge-anzeige.md`): laufende
//! Spiele plus die nächsten N Spiele der TL-Warteliste für einen Hallen-TV.
//!
//! Reine Projektion. Die Reihenfolge rechnet allein der TL-Zustandsbau
//! (`tl::build_state`) — eine zweite Sortierlogik gäbe es sonst, und die
//! Halle sähe eine andere Reihenfolge als die Turnierleitung.
//!
//! Datenschutz (ADR 0076): Die Ausgabe ist eine **Positivliste**. Keine
//! Lizenznummern (`_ids`), kein Geburtsjahr, keine SR- oder Check-In-Daten,
//! keine internen Marken. Nation und Verein nur nach den Monitor-Schaltern.

use crate::config::CourtMonitorConfig;
use crate::tablet::state::CourtOverview;
use crate::tablet::tl::{TlHall, TlMatch, TlOpenMatch, TlState};

/// Standard und Deckel der kommenden Spiele (Spec: Standard 19, 1–60).
pub const ANZAHL_STANDARD: usize = 19;
pub const ANZAHL_MAX: usize = 60;

/// Was der TV über ein laufendes Spiel erfährt.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Laufend {
    pub court: String,
    pub hall: String,
    pub hall_color: Option<String>,
    pub class_label: String,
    pub round_name: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    pub team1_nat: Vec<String>,
    pub team2_nat: Vec<String>,
    pub team1_club: Vec<String>,
    pub team2_club: Vec<String>,
    pub team1_seed: Option<String>,
    pub team2_seed: Option<String>,
    pub sets: Vec<(i64, i64)>,
}

/// Eine Zeile der kommenden Spiele.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Kommend {
    /// 1, 2, 3 … innerhalb der angezeigten (ggf. hallengefilterten) Liste.
    pub platz: usize,
    pub class_label: String,
    pub round_name: String,
    /// Feststehende Seite: Namen. Offene Seite: leer, Text in `offen*`.
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Label einer offenen Seite („A oder B", „aus Spiel 42", „noch offen")
    /// — wie im TL-Web, nie „Sieger aus" (Platzierungsspiele!).
    pub offen1: Option<String>,
    pub offen2: Option<String>,
    pub team1_nat: Vec<String>,
    pub team2_nat: Vec<String>,
    pub team1_club: Vec<String>,
    pub team2_club: Vec<String>,
    /// Setzplatz — nie an Kandidaten einer offenen Paarung (CLAUDE.md).
    pub team1_seed: Option<String>,
    pub team2_seed: Option<String>,
    pub hall: String,
    pub hall_color: Option<String>,
    /// In Vorbereitung gerufen.
    pub gerufen: bool,
    /// Wunschfeld als Anzeigename („Feld 3").
    pub wunschfeld: Option<String>,
    pub prognose_ms: Option<u64>,
    pub prognose_unsicher: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SpielfolgeView {
    #[serde(rename = "serverNowMs")]
    pub server_now_ms: u64,
    pub laufend: Vec<Laufend>,
    pub kommend: Vec<Kommend>,
}

/// Anzahl aus dem URL-Parameter: fehlend/unlesbar → Standard, sonst 1–60.
pub fn anzahl_aus(roh: Option<&str>) -> usize {
    roh.and_then(|s| s.trim().parse::<usize>().ok())
        .map(|n| n.clamp(1, ANZAHL_MAX))
        .unwrap_or(ANZAHL_STANDARD)
}

fn passt(halle: &str, hall: &str) -> bool {
    relay_proto::halle_passt(halle, hall)
}

fn wenn(an: bool, v: &[String]) -> Vec<String> {
    if an {
        v.to_vec()
    } else {
        Vec::new()
    }
}

/// Baut die Anzeige. `overview` ist bereits durch `monitor::anzeige_filtern`
/// gelaufen; für die Warteliste filtert diese Funktion selbst nach denselben
/// Schaltern.
pub fn spielfolge_view(
    tl: &TlState,
    overview: &[CourtOverview],
    cm: &CourtMonitorConfig,
    halle: &str,
    anzahl: usize,
    now_ms: u64,
) -> SpielfolgeView {
    let felder: Vec<(i64, String)> = tl
        .courts
        .iter()
        .map(|c| (c.court_id, c.court.clone()))
        .collect();
    let teile = Teile {
        queue: &tl.queue,
        open_queue: &tl.open_queue,
        halls: &tl.halls,
        felder: &felder,
    };
    bauen(&teile, overview, cm, halle, anzahl, now_ms)
}

/// Die Teile des TL-Zustands, die die Anzeige braucht (testbar ohne den
/// vollen `TlState`).
struct Teile<'a> {
    queue: &'a [TlMatch],
    open_queue: &'a [TlOpenMatch],
    halls: &'a [TlHall],
    felder: &'a [(i64, String)],
}

fn bauen(
    tl: &Teile,
    overview: &[CourtOverview],
    cm: &CourtMonitorConfig,
    halle: &str,
    anzahl: usize,
    now_ms: u64,
) -> SpielfolgeView {
    let nation = cm.nation_an();
    let verein = cm.verein_an();
    let farbe = |hall: &str| -> Option<String> {
        tl.halls
            .iter()
            .find(|h| !hall.is_empty() && h.name.trim().eq_ignore_ascii_case(hall.trim()))
            .and_then(|h| h.color.clone())
    };

    let laufend = overview
        .iter()
        .filter(|c| c.match_id > 0 && !(c.team1.is_empty() && c.team2.is_empty()))
        .filter(|c| passt(halle, &c.location))
        .map(|c| Laufend {
            court: c.court.clone(),
            hall: c.location.clone(),
            hall_color: c.hall_color.clone(),
            class_label: if c.class_label.is_empty() {
                c.match_name.clone()
            } else {
                c.class_label.clone()
            },
            round_name: c.round_name.clone(),
            team1: c.team1.clone(),
            team2: c.team2.clone(),
            team1_nat: c.team1_nationalities.clone(),
            team2_nat: c.team2_nationalities.clone(),
            team1_club: c.team1_clubs.clone(),
            team2_club: c.team2_clubs.clone(),
            team1_seed: c.team1_seed.clone(),
            team2_seed: c.team2_seed.clone(),
            sets: c.sets.clone(),
        })
        .collect();

    let feldname = |id: i64| -> Option<String> {
        tl.felder
            .iter()
            .find(|(cid, _)| *cid == id)
            .map(|(_, name)| name.clone())
    };

    // Arbeitsliste und offene Paarungen mischen — wie `offeneSpiele.mjs`:
    // `queue_index` = wie viele echte Wartespiele davor stehen.
    enum Eintrag<'a> {
        Fest(&'a TlMatch),
        Offen(&'a TlOpenMatch),
    }
    let mut gemischt: Vec<Eintrag> = Vec::new();
    let mut n = 0usize;
    for o in tl.open_queue {
        let ziel = (o.queue_index as usize).min(tl.queue.len());
        while n < ziel {
            gemischt.push(Eintrag::Fest(&tl.queue[n]));
            n += 1;
        }
        gemischt.push(Eintrag::Offen(o));
    }
    while n < tl.queue.len() {
        gemischt.push(Eintrag::Fest(&tl.queue[n]));
        n += 1;
    }

    // Der TL-Zustand kann bis zu 3 s älter sein als die Feldbelegung: Ein
    // gerade aufs Feld gelegtes Spiel stünde sonst doppelt da (Grill-Fund).
    let auf_feld: std::collections::HashSet<i64> = overview
        .iter()
        .filter(|c| c.match_id > 0)
        .map(|c| c.match_id)
        .collect();

    // Reihenfolge: mischen → Halle filtern → N nehmen (wie die TL-Liste).
    let mut kommend = Vec::new();
    for e in gemischt {
        if kommend.len() >= anzahl {
            break;
        }
        let platz = kommend.len() + 1;
        let zeile = match e {
            Eintrag::Fest(m) => {
                if !passt(halle, &m.hall) || auf_feld.contains(&m.match_id) {
                    continue;
                }
                Kommend {
                    platz,
                    class_label: m.class_label.clone(),
                    round_name: m.round_name.clone(),
                    team1: m.team1.clone(),
                    team2: m.team2.clone(),
                    offen1: None,
                    offen2: None,
                    team1_nat: wenn(nation, &m.team1_nat),
                    team2_nat: wenn(nation, &m.team2_nat),
                    team1_club: wenn(verein, &m.team1_club),
                    team2_club: wenn(verein, &m.team2_club),
                    team1_seed: m.team1_seed.clone(),
                    team2_seed: m.team2_seed.clone(),
                    hall: m.hall.clone(),
                    hall_color: farbe(&m.hall),
                    gerufen: m.prep_call.is_some(),
                    wunschfeld: m.wish_court.and_then(feldname),
                    // Wie im TL-Web: gerufene Spiele ohne Prognose-Zeit.
                    prognose_ms: m.predicted_start_ms.filter(|_| m.prep_call.is_none()),
                    prognose_unsicher: m.predicted_uncertain,
                }
            }
            Eintrag::Offen(o) => {
                if !passt(halle, &o.hall) || auf_feld.contains(&o.match_id) {
                    continue;
                }
                let label = |s: &str| {
                    if s.trim().is_empty() {
                        "noch offen".to_string()
                    } else {
                        s.to_string()
                    }
                };
                let offen1 = o.team1.is_empty().then(|| label(&o.open_slot1_label));
                let offen2 = o.team2.is_empty().then(|| label(&o.open_slot2_label));
                Kommend {
                    platz,
                    class_label: o.class_label.clone(),
                    round_name: o.round_name.clone(),
                    team1: o.team1.clone(),
                    team2: o.team2.clone(),
                    offen1,
                    offen2,
                    team1_nat: Vec::new(),
                    team2_nat: Vec::new(),
                    team1_club: Vec::new(),
                    team2_club: Vec::new(),
                    team1_seed: None,
                    team2_seed: None,
                    hall: o.hall.clone(),
                    hall_color: farbe(&o.hall),
                    gerufen: false,
                    wunschfeld: o.wish_court.and_then(feldname),
                    prognose_ms: None,
                    prognose_unsicher: false,
                }
            }
        };
        kommend.push(zeile);
    }

    SpielfolgeView {
        server_now_ms: now_ms,
        laufend,
        kommend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl_match(id: i64, hall: &str) -> TlMatch {
        serde_json::from_value(serde_json::json!({
            "match_id": id, "match_num": id, "planned_time": null,
            "draw_name": "HD", "round_name": "R1", "class_label": "HD-A",
            "discipline": "HD",
            "team1": [format!("A{id}")], "team2": [format!("B{id}")],
            "team1_nat": ["GER"], "team2_nat": ["DEN"],
            "team1_club": ["TSV"], "team2_club": ["SG"],
            "team1_ids": ["06-123"], "team2_ids": ["06-456"],
            "team1_seed": "1",
            "hall": hall, "hall_source": "none",
            "prep_call": null, "blocked": null,
            "predicted_start_ms": 1000, "predicted_uncertain": true
        }))
        .expect("TlMatch")
    }

    fn tl_open(id: i64, idx: u32) -> TlOpenMatch {
        serde_json::from_value(serde_json::json!({
            "match_id": id, "match_num": id, "planned_time": null,
            "draw_name": "HD", "round_name": "VF", "class_label": "HD-A",
            "discipline": "HD", "team1": ["X"], "team2": [],
            "open_slot1_label": "", "open_slot2_label": "Müller oder Meier",
            "hall": "", "hall_source": "none", "queue_index": idx
        }))
        .expect("TlOpenMatch")
    }

    fn sicht(
        queue: Vec<TlMatch>,
        open: Vec<TlOpenMatch>,
        nation: bool,
        halle: &str,
        anzahl: usize,
    ) -> SpielfolgeView {
        let teile = Teile {
            queue: &queue,
            open_queue: &open,
            halls: &[],
            felder: &[],
        };
        bauen(&teile, &[], &cm(nation), halle, anzahl, 0)
    }

    fn cm(nation: bool) -> CourtMonitorConfig {
        let mut c = CourtMonitorConfig::default();
        if !nation {
            c.name_davor = crate::config::NamenDavor::Ohne;
            c.name_darunter = crate::config::NamenDarunter::Ohne;
        }
        c
    }

    #[test]
    fn anzahl_standard_und_deckel() {
        assert_eq!(anzahl_aus(None), 19);
        assert_eq!(anzahl_aus(Some("x")), 19);
        assert_eq!(anzahl_aus(Some("0")), 1);
        assert_eq!(anzahl_aus(Some("500")), 60);
        assert_eq!(anzahl_aus(Some(" 25 ")), 25);
    }

    #[test]
    fn mischt_offene_an_ihrer_position_und_deckelt() {
        let v = sicht(
            vec![tl_match(1, ""), tl_match(2, ""), tl_match(3, "")],
            vec![tl_open(9, 1)],
            true,
            "",
            3,
        );
        let ids: Vec<_> = v.kommend.iter().map(|k| k.team1.first().cloned()).collect();
        assert_eq!(
            ids,
            vec![Some("A1".into()), Some("X".into()), Some("A2".into())]
        );
        assert_eq!(v.kommend[1].offen2.as_deref(), Some("Müller oder Meier"));
        assert_eq!(v.kommend[1].offen1, None);
        assert_eq!(v.kommend[1].team1_seed, None);
        assert_eq!(
            v.kommend.iter().map(|k| k.platz).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn hallenfilter_laesst_spiele_ohne_halle_durch() {
        let v = sicht(
            vec![
                tl_match(1, "Halle 1"),
                tl_match(2, "Halle 2"),
                tl_match(3, ""),
            ],
            vec![],
            true,
            "halle 2",
            19,
        );
        let ids: Vec<_> = v.kommend.iter().map(|k| k.team1[0].clone()).collect();
        assert_eq!(ids, vec!["A2", "A3"]);
        assert_eq!(v.kommend[0].platz, 1);
    }

    #[test]
    fn prognose_reist_mit() {
        let v = sicht(vec![tl_match(1, "")], vec![], true, "", 19);
        assert_eq!(v.kommend[0].prognose_ms, Some(1000));
        assert!(v.kommend[0].prognose_unsicher);
    }

    /// Datenschutz-Wächter (ADR 0076): keine Lizenznummer im JSON, Nation
    /// und Verein nur mit Schalter.
    #[test]
    fn traegt_nie_lizenznummern_und_nation_nur_mit_schalter() {
        let aus = serde_json::to_string(&sicht(
            vec![tl_match(1, "")],
            vec![tl_open(9, 0)],
            false,
            "",
            19,
        ))
        .unwrap();
        assert!(!aus.contains("06-123") && !aus.contains("_ids"));
        assert!(!aus.contains("GER") && !aus.contains("TSV"));
        assert!(!aus.contains("birth") && !aus.contains("geburt"));
    }
}
