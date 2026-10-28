//! Ereignis-Auslöser für den TP-Upload (Spec `docs/features/tp-ereignis-upload.md`,
//! ADR 0062): Welche Änderung im BTP-Snapshot rechtfertigt es, den nächsten
//! Takt-Termin vorzuziehen?
//!
//! Der Takt (ADR 0061) prüft nach der Uhr; was er nicht weiß, ist, **wann**
//! sich etwas ändert. Der Sync-Lauf sieht jede Änderung ohnehin — hier
//! wird daraus ein Kern je Spiel gebildet (Auslosung und Paarung) und
//! zwischen zwei Abrufen verglichen.
//!
//! Was bewusst **nicht** zählt (Messung badhub `LiveVerschmelzung`,
//! 28.10.2026): Ergebnisse, Zwischenstände und Feldzuweisungen übernimmt
//! die Turnierseite aus dem Liveticker-Stand; Aufrufe, Schiedsrichter und
//! Farben kennt die Datei gar nicht. Namen, Paarungen und neue Spiele
//! dagegen kommen **nur** aus der Datei — genau die lösen aus.

use std::collections::BTreeMap;

use crate::btp::model::BtpSnapshot;

/// Das, was von einem Spiel nur über die Datei nach badhub kommt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchKern {
    pub draw_id: i64,
    pub entry1_id: i64,
    pub entry2_id: i64,
}

/// Match-ID → Kern, sortiert (deterministisch, vergleichbar).
pub type Kern = BTreeMap<i64, MatchKern>;

/// Projektion des Snapshots auf den Kern.
pub fn kern(snapshot: &BtpSnapshot) -> Kern {
    snapshot
        .matches
        .iter()
        .map(|m| {
            (
                m.id,
                MatchKern {
                    draw_id: m.draw_id,
                    entry1_id: m.entry1_id,
                    entry2_id: m.entry2_id,
                },
            )
        })
        .collect()
}

/// Fortschreiben über zwei Abrufe: liefert den zu merkenden Kern und ein
/// etwaiges Ereignis. Drei Fälle sind **kein** Ereignis:
/// - der erste Abruf (`alt = None`) — Ausgangslage, auch nach dem
///   Sync-Neustart beim Speichern;
/// - ein leerer Stand, wenn vorher Spiele da waren (BTP-Neustart, Turnier
///   neu geladen) — der alte Kern bleibt, damit das Wiederauffüllen nicht
///   als „neue Spiele“ zählt;
/// - ein Stand ohne gemeinsame Match-ID mit dem alten (anderes Turnier in
///   BTP geöffnet) — die Datei gehört dann zu einem anderen Turnier, das
///   fängt der GUID-Abgleich vor dem Senden.
pub fn fortschreiben(alt: Option<Kern>, neu: Kern) -> (Kern, Option<&'static str>) {
    let Some(alt) = alt else {
        return (neu, None);
    };
    if neu.is_empty() && !alt.is_empty() {
        return (alt, None);
    }
    if !alt.is_empty() && !neu.keys().any(|id| alt.contains_key(id)) {
        return (neu, None);
    }
    let ereignis = vergleiche(&alt, &neu);
    (neu, ereignis)
}

/// Grund für einen vorgezogenen Upload — oder `None`, wenn nichts dabei ist,
/// was nur die Datei nach badhub bringt. Vorrang: neue Spiele (Auslosung)
/// vor entfernten vor geänderter Paarung — für den Text, nicht fürs Senden.
pub fn vergleiche(alt: &Kern, neu: &Kern) -> Option<&'static str> {
    if neu.keys().any(|id| !alt.contains_key(id)) {
        return Some("neue Spiele");
    }
    if alt.keys().any(|id| !neu.contains_key(id)) {
        return Some("Spielplan geändert");
    }
    if neu.iter().any(|(id, k)| alt.get(id) != Some(k)) {
        return Some("Paarung");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(entry1: i64, entry2: i64) -> MatchKern {
        MatchKern {
            draw_id: 1,
            entry1_id: entry1,
            entry2_id: entry2,
        }
    }

    fn kern_aus(eintraege: &[(i64, MatchKern)]) -> Kern {
        eintraege.iter().cloned().collect()
    }

    #[test]
    fn gleicher_kern_ist_kein_ereignis() {
        let a = kern_aus(&[(1, k(10, 11)), (2, k(12, 13))]);
        assert_eq!(vergleiche(&a, &a.clone()), None);
    }

    #[test]
    fn neue_spiele_sind_eine_auslosung() {
        let alt = kern_aus(&[(1, k(10, 11))]);
        let neu = kern_aus(&[(1, k(10, 11)), (2, k(0, 0))]);
        assert_eq!(vergleiche(&alt, &neu), Some("neue Spiele"));
    }

    #[test]
    fn gefuellter_platz_ist_eine_paarung() {
        let alt = kern_aus(&[(1, k(10, 0))]);
        let neu = kern_aus(&[(1, k(10, 11))]);
        assert_eq!(vergleiche(&alt, &neu), Some("Paarung"));
    }

    #[test]
    fn entfernte_spiele_sind_ein_geaenderter_spielplan() {
        let alt = kern_aus(&[(1, k(10, 11)), (2, k(12, 13))]);
        let neu = kern_aus(&[(1, k(10, 11))]);
        assert_eq!(vergleiche(&alt, &neu), Some("Spielplan geändert"));
    }

    #[test]
    fn neue_spiele_haben_vorrang_vor_paarung() {
        let alt = kern_aus(&[(1, k(10, 0))]);
        let neu = kern_aus(&[(1, k(10, 11)), (2, k(0, 0))]);
        assert_eq!(vergleiche(&alt, &neu), Some("neue Spiele"));
    }

    fn spiel(id: i64, entry1: i64, entry2: i64) -> crate::btp::model::BtpMatch {
        use crate::btp::model::{BtpMatch, Discipline, MatchResult, MatchStatus};
        BtpMatch {
            display_order: None,
            highlight: 0,
            from1: None,
            from2: None,
            id,
            draw_id: 1,
            planning_id: 1000 + id,
            draw_name: "HE".to_string(),
            discipline: Discipline::MensSingles,
            class_label: String::new(),
            round_name: "R1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: Vec::new(),
            team2: Vec::new(),
            entry1_id: entry1,
            entry2_id: entry2,
            court: None,
            court_id: None,
            location_id: None,
            sets: Vec::new(),
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::Scheduled,
            finished_at: None,
            pause_ms: None,
            preparation_call_ts: None,
            preparation_hall: None,
            official1_id: None,
            official2_id: None,
            scoring: crate::btp::model::ScoringFormat::default(),
            team1_seed: None,
            team2_seed: None,
        }
    }

    fn snap(matches: Vec<crate::btp::model::BtpMatch>) -> BtpSnapshot {
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
    fn ergebnis_feld_aufruf_und_farbe_loesen_nichts_aus() {
        use crate::btp::model::MatchStatus;
        let vorher = snap(vec![spiel(1, 10, 11)]);
        let mut m = spiel(1, 10, 11);
        m.sets = vec![(21, 15), (21, 17)];
        m.winner = Some(1);
        m.status = MatchStatus::Finished;
        m.court = Some("3".into());
        m.court_id = Some(3);
        m.highlight = 2;
        m.preparation_call_ts = Some(123);
        m.official1_id = Some(7);
        let nachher = snap(vec![m]);
        assert_eq!(vergleiche(&kern(&vorher), &kern(&nachher)), None);
    }

    #[test]
    fn kern_aus_snapshot_erkennt_paarung_und_neue_spiele() {
        let vorher = snap(vec![spiel(1, 10, 0)]);
        let gefuellt = snap(vec![spiel(1, 10, 11)]);
        assert_eq!(
            vergleiche(&kern(&vorher), &kern(&gefuellt)),
            Some("Paarung")
        );
        let ausgelost = snap(vec![spiel(1, 10, 11), spiel(2, 0, 0)]);
        assert_eq!(
            vergleiche(&kern(&gefuellt), &kern(&ausgelost)),
            Some("neue Spiele")
        );
    }

    #[test]
    fn fortschreiben_erster_stand_ist_ausgangslage() {
        let (k1, e) = fortschreiben(None, kern_aus(&[(1, k(10, 11))]));
        assert_eq!(e, None);
        assert_eq!(k1.len(), 1);
    }

    #[test]
    fn fortschreiben_leerer_stand_wird_ignoriert_und_ruft_nichts_aus() {
        // BTP liefert kurz nichts (Neustart): kein „Spielplan geändert“,
        // und beim Wiederauffüllen kein „neue Spiele“ — der alte Kern bleibt.
        let alt = kern_aus(&[(1, k(10, 11))]);
        let (gemerkt, e) = fortschreiben(Some(alt.clone()), Kern::new());
        assert_eq!(e, None);
        assert_eq!(gemerkt, alt);
        let (_, e) = fortschreiben(Some(gemerkt), alt.clone());
        assert_eq!(e, None);
    }

    #[test]
    fn fortschreiben_erste_auslosung_auf_leerem_turnier_loest_aus() {
        let (leer, _) = fortschreiben(None, Kern::new());
        let (_, e) = fortschreiben(Some(leer), kern_aus(&[(1, k(0, 0))]));
        assert_eq!(e, Some("neue Spiele"));
    }

    #[test]
    fn fortschreiben_turnierwechsel_ohne_gemeinsame_spiele_ist_ausgangslage() {
        // Anderes Turnier in BTP geöffnet: alle IDs neu — kein Ereignis; die
        // Datei passt dann ohnehin nicht mehr (GUID-Abgleich vor dem Senden).
        let alt = kern_aus(&[(1, k(10, 11)), (2, k(12, 13))]);
        let neu = kern_aus(&[(500, k(1, 2)), (501, k(3, 4))]);
        let (gemerkt, e) = fortschreiben(Some(alt), neu.clone());
        assert_eq!(e, None);
        assert_eq!(gemerkt, neu);
    }
}
