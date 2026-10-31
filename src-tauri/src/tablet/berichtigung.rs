//! „Mannschaften berichtigt" deuten (ADR 0065, Spec
//! `tablet-seiten-korrigieren`).
//!
//! Stand am Tablet die falsche Mannschaft auf der falschen Seite, sind alle
//! Ereignisse **vor** der Berichtigung in vertauschten A/B-Koordinaten
//! erfasst. Der Bestand ist append-only (ADR 0038) — umgeschrieben wird
//! nichts. Stattdessen rechnet diese Funktion beim Lesen jedes Ereignis in
//! die **Endkoordinaten** um. Zettel und badhub-Verlauf nutzen sie gleich;
//! das Tablet deutet nicht selbst, den Punktverlauf kippt es eigenhändig.

use std::cmp::Ordering;
use std::collections::HashSet;

use relay_proto::{EventKind, MatchEvent};

/// Kanonische Ordnung des Bestands — dieselbe wie im `SheetStore`.
fn ordnung(a: &MatchEvent, b: &MatchEvent) -> Ordering {
    (a.set, a.after_n, a.seq, &a.id).cmp(&(b.set, b.after_n, b.seq, &b.id))
}

/// Ereignisse in Endkoordinaten, in der Reihenfolge der Eingabe.
///
/// **Kipp-Punkte** sind jedes `teams_corrected` und jede **erste** Rücknahme
/// eines solchen — die Rücknahme dreht die Namen ja zurück, und was
/// zwischen Berichtigung und Rücknahme erfasst wurde, stand in den
/// berichtigten Koordinaten. Ein Ereignis wird gekippt (`team`,
/// `receiver_team`, `score_a`↔`score_b`), wenn **nach** ihm eine ungerade
/// Zahl von Kipp-Punkten liegt. Spielerindizes bleiben: Wer innerhalb der
/// Mannschaft aufschlug, weiß nach einer Berichtigung niemand (Grenze der
/// Spec, im Doppel fragt das Tablet neu ab).
pub fn in_endkoordinaten(events: &[MatchEvent]) -> Vec<MatchEvent> {
    let korrekturen: HashSet<&str> = events
        .iter()
        .filter(|e| e.kind == EventKind::TeamsCorrected)
        .map(|e| e.id.as_str())
        .collect();
    if korrekturen.is_empty() {
        return events.to_vec();
    }

    // Erste Rücknahme je Berichtigung in kanonischer Ordnung — eine doppelt
    // empfangene oder doppelt erfasste Rücknahme dreht nicht zweimal.
    let mut sortiert: Vec<usize> = (0..events.len()).collect();
    sortiert.sort_by(|&a, &b| ordnung(&events[a], &events[b]));
    let mut schon_zurueck: HashSet<&str> = HashSet::new();
    let mut kipp_punkt = vec![false; events.len()];
    for &i in &sortiert {
        let e = &events[i];
        kipp_punkt[i] = match e.kind {
            EventKind::TeamsCorrected => true,
            EventKind::Retract => {
                korrekturen.contains(e.retracts.as_str())
                    && schon_zurueck.insert(e.retracts.as_str())
            }
            _ => false,
        };
    }

    // Rückwärts zählen: `danach` = Kipp-Punkte strikt nach dem Ereignis.
    let mut gekippt = vec![false; events.len()];
    let mut danach = 0usize;
    for &i in sortiert.iter().rev() {
        gekippt[i] = danach % 2 == 1;
        if kipp_punkt[i] {
            danach += 1;
        }
    }

    events
        .iter()
        .zip(gekippt)
        .map(|(e, kippen)| {
            let mut e = e.clone();
            // Berichtigung und Rücknahme tragen keine Mannschaft.
            let ohne_team = matches!(e.kind, EventKind::TeamsCorrected | EventKind::Retract);
            if kippen && !ohne_team {
                e.team = 1 - e.team.clamp(0, 1);
                e.receiver_team = 1 - e.receiver_team.clamp(0, 1);
                std::mem::swap(&mut e.score_a, &mut e.score_b);
            }
            e
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::Phase;

    fn ev(id: &str, kind: EventKind, set: i64, after_n: i64, team: i64) -> MatchEvent {
        MatchEvent {
            id: id.into(),
            seq: 0,
            set,
            after_n,
            score_a: 0,
            score_b: 0,
            ts_ms: 0,
            kind,
            team,
            player: 0,
            receiver_team: 1 - team,
            receiver_player: 0,
            phase: Phase::Play,
            retracts: String::new(),
        }
    }

    fn rueck(id: &str, ziel: &str, set: i64, after_n: i64) -> MatchEvent {
        MatchEvent {
            retracts: ziel.into(),
            ..ev(id, EventKind::Retract, set, after_n, 0)
        }
    }

    fn teams(v: &[MatchEvent]) -> Vec<i64> {
        v.iter().map(|e| e.team).collect()
    }

    #[test]
    fn ohne_berichtigung_bleibt_alles_wie_es_ist() {
        let v = vec![
            ev("01", EventKind::ServeStart, 1, 0, 0),
            ev("02", EventKind::CardYellow, 1, 5, 1),
        ];
        assert_eq!(in_endkoordinaten(&v), v);
    }

    #[test]
    fn berichtigung_kippt_alles_davor_ueber_saetze_hinweg() {
        let mut karte = ev("02", EventKind::CardYellow, 1, 5, 1);
        karte.score_a = 3;
        karte.score_b = 2;
        let v = vec![
            ev("01", EventKind::ServeStart, 1, 0, 0),
            karte,
            ev("03", EventKind::ServeStart, 2, 0, 0),
            ev("04", EventKind::TeamsCorrected, 2, 7, 0),
            ev("05", EventKind::CardYellow, 2, 9, 0),
        ];
        let r = in_endkoordinaten(&v);
        assert_eq!(teams(&r), vec![1, 0, 1, 0, 0]);
        assert_eq!(r[0].receiver_team, 0, "Empfänger kippt mit");
        assert_eq!((r[1].score_a, r[1].score_b), (2, 3), "Stand kippt mit");
    }

    #[test]
    fn zwei_berichtigungen_heben_sich_auf() {
        let v = vec![
            ev("01", EventKind::ServeStart, 1, 0, 0),
            ev("02", EventKind::TeamsCorrected, 1, 3, 0),
            ev("03", EventKind::CardYellow, 1, 4, 1),
            ev("04", EventKind::TeamsCorrected, 1, 6, 0),
        ];
        // 01 sieht zwei Kipp-Punkte, 03 einen.
        assert_eq!(teams(&in_endkoordinaten(&v)), vec![0, 0, 0, 0]);
    }

    #[test]
    fn karte_zwischen_berichtigung_und_ruecknahme_landet_richtig() {
        // Berichtigt, Karte in den berichtigten Koordinaten, dann die
        // Berichtigung zurückgenommen: Die Karte muss zurückgekippt werden,
        // alles vor der Berichtigung bleibt (zwei Kipp-Punkte).
        let v = vec![
            ev("01", EventKind::ServeStart, 1, 0, 0),
            ev("02", EventKind::TeamsCorrected, 1, 3, 0),
            ev("03", EventKind::CardYellow, 1, 3, 1),
            rueck("04", "02", 1, 3),
        ];
        let mut v = v;
        v[0].seq = 0;
        v[1].seq = 1;
        v[2].seq = 2;
        v[3].seq = 3;
        assert_eq!(teams(&in_endkoordinaten(&v)), vec![0, 0, 0, 0]);
    }

    #[test]
    fn doppelte_ruecknahme_kippt_nur_einmal() {
        let v = vec![
            ev("01", EventKind::ServeStart, 1, 0, 0),
            ev("02", EventKind::TeamsCorrected, 1, 3, 0),
            rueck("03", "02", 1, 4),
            rueck("04", "02", 1, 5),
        ];
        assert_eq!(in_endkoordinaten(&v)[0].team, 0);
    }

    #[test]
    fn ruecknahme_eines_anderen_ereignisses_kippt_nicht() {
        let v = vec![
            ev("01", EventKind::ServeStart, 1, 0, 0),
            ev("02", EventKind::CardYellow, 1, 2, 0),
            rueck("03", "02", 1, 3),
        ];
        assert_eq!(in_endkoordinaten(&v), v);
    }

    #[test]
    fn an_gleicher_stelle_entscheidet_seq() {
        // Aufschlagfolge der Doppel-Neuabfrage nach der Berichtigung an
        // derselben Stelle: Sie steht schon in Endkoordinaten.
        let mut k = ev("0a", EventKind::TeamsCorrected, 1, 5, 0);
        k.seq = 4;
        let mut s = ev("09", EventKind::ServeStart, 1, 5, 1);
        s.seq = 5;
        let mut vorher = ev("0b", EventKind::CardYellow, 1, 5, 0);
        vorher.seq = 3;
        let r = in_endkoordinaten(&[s, k, vorher]);
        assert_eq!(teams(&r), vec![1, 0, 1]);
    }
}
