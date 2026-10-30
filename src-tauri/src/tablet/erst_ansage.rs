//! Erst-Ansage im Kern (Spec `ansage-im-browser`, Etappe E2, ADR 0064).
//!
//! Erkennt, auf welchem Feld seit dem letzten Sync-Lauf ein **neues** Spiel
//! steht — die Entscheidung hinter dem automatischen Aufruf „Feld 3.
//! Herreneinzel A. Müller gegen Schmidt.". Bis v0.9.313 traf sie die
//! Desktop-Oberfläche (`MatchAnnouncer` + `src/io/announceBaseline.mjs`);
//! dort erreichte sie aber nur den Lautsprecher am PC, nie die Ansage-Seiten
//! im Browser. Jetzt fällt sie einmal im Kern, und daraus entsteht ein
//! Ansage-Auftrag `court_assigned` für alle Verbraucher.
//!
//! Der Stand liegt im `TabletState`, **nicht** in der `SyncEngine`: Die wird
//! beim Speichern der Einstellungen neu gebaut, und ein Merker dort hätte
//! danach alle laufenden Spiele als neu angesagt.
//!
//! Die Regeln sind 1:1 aus `announceBaseline.mjs` übernommen, samt der
//! Fehler, gegen die sie gewachsen sind (siehe Tests unten).

use std::collections::HashMap;

/// Binnen dieser Zeit wird dasselbe Spiel nicht erneut angesagt — etwa wenn
/// es kurz vom Feld genommen und gleich wieder daraufgelegt wird.
pub const ERST_ANSAGE_SPERRE_MS: u64 = 5_000;

/// Ein Feld im aktuellen BTP-Stand. `match_id == 0` heißt: frei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feld {
    pub court_id: i64,
    pub match_id: i64,
    /// Halle des Felds (leer bei Ein-Hallen-Turnieren) — Ziel des Auftrags.
    pub hall: String,
}

/// Was der Kern über die Feldbelegung weiß.
#[derive(Debug, Default)]
pub struct ErstAnsageStand {
    /// CourtID → zuletzt gesehene Match-ID (0 = frei).
    baseline: HashMap<i64, i64>,
    /// Steht die Bestandsaufnahme? Erst danach wird angesagt.
    hat_baseline: bool,
    /// Turnier, zu dem die Bestandsaufnahme gehört.
    turnier: Option<String>,
    /// Match-ID → Zeitpunkt der letzten Erst-Ansage (Sperre gegen Doppel).
    zuletzt: HashMap<i64, u64>,
}

impl ErstAnsageStand {
    /// Vergleicht den aktuellen Stand mit dem zuletzt gesehenen und liefert
    /// die Felder, deren neues Spiel jetzt angesagt werden soll.
    ///
    /// **Bestandsaufnahme.** Beim ersten brauchbaren Stand wird nichts
    /// angesagt — sonst riefe ein App-Start mitten im Turnier alle
    /// laufenden Spiele erneut aus. Brauchbar heißt: Es gibt überhaupt
    /// Felder. Ein **leerer** Stand taugt nicht (gemeldeter Fehler
    /// 14.08.2026: der erste Abruf kam vor dem ersten BTP-Schnappschuss);
    /// Felder ohne Spiel dagegen schon — am Turniermorgen ist das der
    /// Normalfall, und der erste echte Aufruf soll erklingen.
    ///
    /// **Turnierwechsel** ist eine neue Bestandsaufnahme: Das neue Turnier
    /// hat andere Felder und Spiele, ein Vergleich mit dem alten würde alles
    /// belegte für neu halten.
    ///
    /// Gefiltert nach Halle wird hier **nicht** — das macht die Zustellung
    /// an das einzelne Gerät. So merkt sich die Bestandsaufnahme alle Felder,
    /// und ein Umschalten der Halle am Gerät holt nichts nach.
    pub fn vergleiche(&mut self, turnier: &str, felder: &[Feld], now_ms: u64) -> Vec<Feld> {
        if self.turnier.as_deref() != Some(turnier) {
            if self.turnier.is_some() {
                tracing::info!("Erst-Ansage: Turnierwechsel — neue Bestandsaufnahme");
            }
            self.turnier = Some(turnier.to_string());
            self.baseline.clear();
            self.hat_baseline = false;
            self.zuletzt.clear();
        }

        let mut neue = Vec::new();
        for feld in felder {
            let vorher = self
                .baseline
                .insert(feld.court_id, feld.match_id)
                .unwrap_or(0);
            if self.hat_baseline && feld.match_id != 0 && feld.match_id != vorher {
                neue.push(feld.clone());
            }
        }
        // Ohne Felder ist nichts zu sehen — und nichts zu merken.
        if !felder.is_empty() {
            self.hat_baseline = true;
        }

        self.zuletzt
            .retain(|_, t| now_ms.saturating_sub(*t) < ERST_ANSAGE_SPERRE_MS);
        neue.retain(|f| {
            if self.zuletzt.contains_key(&f.match_id) {
                return false;
            }
            self.zuletzt.insert(f.match_id, now_ms);
            true
        });
        neue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port von scripts/test-announce-baseline.mjs — dieselben Fälle, damit
    // keiner der dort festgehaltenen Fehler beim Umzug in den Kern
    // zurückkommt.

    fn feld(court_id: i64, match_id: i64) -> Feld {
        Feld {
            court_id,
            match_id,
            hall: String::new(),
        }
    }

    fn feld_in(court_id: i64, match_id: i64, hall: &str) -> Feld {
        Feld {
            court_id,
            match_id,
            hall: hall.into(),
        }
    }

    fn ids(v: &[Feld]) -> Vec<i64> {
        v.iter().map(|f| f.match_id).collect()
    }

    #[test]
    fn start_mitten_im_turnier_sagt_nichts_an() {
        let mut s = ErstAnsageStand::default();
        assert!(s
            .vergleiche("T", &[feld(1, 100), feld(2, 200)], 0)
            .is_empty());
        assert!(s.hat_baseline);
        assert!(s
            .vergleiche("T", &[feld(1, 100), feld(2, 200)], 1_000)
            .is_empty());
        let neu = s.vergleiche("T", &[feld(1, 101), feld(2, 200)], 2_000);
        assert_eq!(ids(&neu), vec![101]);
    }

    #[test]
    fn leerer_erster_stand_ist_keine_bestandsaufnahme() {
        // Der gemeldete Fehler vom 14.08.2026.
        let mut s = ErstAnsageStand::default();
        assert!(s.vergleiche("T", &[], 0).is_empty());
        assert!(!s.hat_baseline);
        let voll = s.vergleiche("T", &[feld(1, 100), feld(2, 200), feld(3, 300)], 1_000);
        assert!(voll.is_empty(), "das war der Fehler");
        assert!(s.hat_baseline);
        let neu = s.vergleiche("T", &[feld(1, 100), feld(2, 201), feld(3, 300)], 2_000);
        assert_eq!(ids(&neu), vec![201]);
    }

    #[test]
    fn freie_felder_sind_gueltiger_anfangsstand() {
        let mut s = ErstAnsageStand::default();
        assert!(s.vergleiche("T", &[feld(1, 0), feld(2, 0)], 0).is_empty());
        assert!(s.hat_baseline);
        let erster = s.vergleiche("T", &[feld(1, 100), feld(2, 0)], 1_000);
        assert_eq!(ids(&erster), vec![100], "der erste Aufruf des Tages");
    }

    #[test]
    fn neues_spiel_ergibt_genau_einen_treffer_mit_halle() {
        let mut s = ErstAnsageStand::default();
        s.vergleiche(
            "T",
            &[feld_in(1, 0, "Halle A"), feld_in(2, 0, "Halle B")],
            0,
        );
        let beide = s.vergleiche(
            "T",
            &[feld_in(1, 100, "Halle A"), feld_in(2, 200, "Halle B")],
            1_000,
        );
        // Keine Hallen-Filterung im Kern: beide Felder, jedes mit seiner Halle.
        assert_eq!(beide.len(), 2);
        assert_eq!(beide[0].hall, "Halle A");
        assert_eq!(beide[1].hall, "Halle B");
        // Und kein zweites Mal beim nächsten Lauf.
        assert!(s
            .vergleiche(
                "T",
                &[feld_in(1, 100, "Halle A"), feld_in(2, 200, "Halle B")],
                10_000
            )
            .is_empty());
    }

    #[test]
    fn feld_wird_frei_und_neu_belegt() {
        let mut s = ErstAnsageStand::default();
        s.vergleiche("T", &[feld(1, 100)], 0);
        assert!(s.vergleiche("T", &[feld(1, 0)], 1_000).is_empty());
        assert_eq!(ids(&s.vergleiche("T", &[feld(1, 101)], 2_000)), vec![101]);
    }

    #[test]
    fn dasselbe_spiel_binnen_fuenf_sekunden_nicht_erneut() {
        let mut s = ErstAnsageStand::default();
        s.vergleiche("T", &[feld(1, 0), feld(2, 0)], 0);
        assert_eq!(
            ids(&s.vergleiche("T", &[feld(1, 100), feld(2, 0)], 1_000)),
            vec![100]
        );
        // Umgelegt auf Feld 2, zwei Sekunden später: kein zweiter Aufruf.
        assert!(s
            .vergleiche("T", &[feld(1, 0), feld(2, 100)], 3_000)
            .is_empty());
        // Zurück auf Feld 1 nach Ablauf der Sperre: wieder ein Aufruf.
        assert_eq!(
            ids(&s.vergleiche(
                "T",
                &[feld(1, 100), feld(2, 0)],
                1_000 + ERST_ANSAGE_SPERRE_MS
            )),
            vec![100]
        );
    }

    #[test]
    fn turnierwechsel_setzt_bestandsaufnahme_zurueck() {
        let mut s = ErstAnsageStand::default();
        s.vergleiche("Alt", &[feld(1, 100), feld(2, 200)], 0);
        // Neues Turnier, alle Felder belegt, andere IDs: nichts ansagen.
        assert!(s
            .vergleiche("Neu", &[feld(1, 900), feld(2, 901), feld(3, 902)], 1_000)
            .is_empty());
        // Auch ein leerer erster Stand des neuen Turniers zählt nicht.
        let mut s = ErstAnsageStand::default();
        s.vergleiche("Alt", &[feld(1, 100)], 0);
        assert!(s.vergleiche("Neu", &[], 1_000).is_empty());
        assert!(s.vergleiche("Neu", &[feld(1, 900)], 2_000).is_empty());
        assert_eq!(ids(&s.vergleiche("Neu", &[feld(1, 901)], 3_000)), vec![901]);
    }

    #[test]
    fn burst_von_26_feldern_wird_vollstaendig_gemeldet() {
        // Turniermorgen: Die Feldvergabe belegt alle Felder in einem Lauf.
        let mut s = ErstAnsageStand::default();
        let frei: Vec<Feld> = (1..=26).map(|c| feld(c, 0)).collect();
        s.vergleiche("T", &frei, 0);
        let voll: Vec<Feld> = (1..=26).map(|c| feld(c, 1000 + c)).collect();
        assert_eq!(s.vergleiche("T", &voll, 1_000).len(), 26);
    }
}
