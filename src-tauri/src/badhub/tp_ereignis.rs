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

    #[test]
    fn kern_projiziert_nur_auslosung_und_paarung() {
        // Ergebnis, Feld, Status zählen nicht: Die Turnierseite holt sie
        // aus dem Liveticker. Hier nur die Struktur des Typs — den Snapshot
        // baut der Parser, die Projektion ist eine Zeile je Feld.
        let k1 = k(10, 11);
        let mut k2 = k1.clone();
        k2.draw_id = 2;
        assert_ne!(k1, k2);
    }
}
