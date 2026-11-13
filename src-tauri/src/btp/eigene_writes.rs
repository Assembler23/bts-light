//! Merkstand der eigenen Match-Writes (Messung 09.11.2026,
//! `tests/btp_highlight_sr_write_probe.rs`).
//!
//! BTP übernimmt bei **jedem** Match-Write die Planungsfelder `CourtID`,
//! `Official1ID`, `Official2ID`, `Highlight` und — gemessen 13.11.2026,
//! `tests/btp_shuttles_probe.rs` — die Ballzahl `Shuttles` aus dem Knoten;
//! ein fehlendes Element heißt 0. Wer nur eines davon ändern will, muss die anderen drei
//! deshalb mit dem Wert mitschicken, den BTP gerade hat — die Write-Funktionen
//! lesen dafür unmittelbar vorher frisch.
//!
//! Frisch gelesen heißt aber nicht übernommen: BTP zeigt einen eigenen Write
//! erst nach 350–840 ms beim Nachlesen (gemessen). Schreibt bts-light zwei
//! Felder desselben Spiels kurz hintereinander (Aufruf-Gelb, dann SR/AR im
//! selben Sync-Lauf; Farb-Tipp im TL-Web neben einem Schiedsrichter-Write),
//! läse der zweite Write den alten Wert und löschte den ersten wieder.
//!
//! Darum merkt sich jeder Write **vor dem Senden**, welche vier Werte er in
//! BTP setzt, und hält sie [`HALTE_MS`] lang über jeden frisch gelesenen
//! Stand. Scheitert der Write, wird der Eintrag zurückgenommen. Prozessweit
//! eine Instanz ([`EIGENE`]), weil die Write-Funktionen nur `&AppConfig`
//! kennen (Muster `write_gate::GATE`).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// So lange gilt ein eigener Write als „noch nicht sicher in BTP sichtbar".
/// Gemessen höchstens 840 ms; der Rest ist Reserve für ein ausgelastetes BTP.
/// Länger ist nicht besser: Ändert die Turnierleitung in dieser Zeit in BTP
/// von Hand dasselbe Spiel, schriebe ein folgender Write ihren Wert zurück.
pub const HALTE_MS: u64 = 5_000;

/// Die vier Felder, die BTP aus jedem Match-Write übernimmt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Planung {
    pub court_id: i64,
    pub officials: (i64, i64),
    pub highlight: u8,
    /// Ballzahl `Match.Shuttles` — gehört seit der Messung vom 13.11.2026
    /// in dieselbe Klasse: fehlt das Element, setzt BTP die Zahl auf 0.
    pub shuttles: i64,
}

/// Merkstand je Match-ID: was zuletzt geschrieben wurde und wann (Unix-ms).
#[derive(Debug, Default)]
pub struct EigeneWrites {
    eintraege: Mutex<HashMap<i64, (Planung, u64)>>,
    /// Spiele, an die gerade ein Ergebnis ging, mit Zeitpunkt (Unix-ms).
    gewertet: Mutex<HashMap<i64, u64>>,
}

impl EigeneWrites {
    /// Vor dem Senden: Diese Werte stehen gleich in BTP.
    pub fn merken(&self, match_id: i64, planung: Planung, jetzt_ms: u64) {
        let mut g = self.eintraege.lock().unwrap();
        // Abgelaufene Einträge nebenbei wegräumen — die Tabelle bleibt so
        // auf die Spiele der letzten Sekunden begrenzt.
        g.retain(|_, (_, seit)| frisch_genug(*seit, jetzt_ms));
        g.insert(match_id, (planung, jetzt_ms));
    }

    /// Write gescheitert: Eintrag zurücknehmen — aber nur, wenn inzwischen
    /// kein anderer Write dasselbe Spiel neu gemerkt hat.
    pub fn vergessen(&self, match_id: i64, planung: Planung, seit_ms: u64) {
        let mut g = self.eintraege.lock().unwrap();
        if g.get(&match_id) == Some(&(planung, seit_ms)) {
            g.remove(&match_id);
        }
    }

    /// Der eigene Stand eines Spiels, solange er jünger als [`HALTE_MS`] ist.
    pub fn aktuell(&self, match_id: i64, jetzt_ms: u64) -> Option<Planung> {
        let g = self.eintraege.lock().unwrap();
        g.get(&match_id)
            .filter(|(_, seit)| frisch_genug(*seit, jetzt_ms))
            .map(|(p, _)| *p)
    }

    /// BTP zeigt beim Lesen schon genau den gemerkten Stand: Eintrag
    /// verwerfen. Ab jetzt gilt wieder BTP — eine Hand-Änderung danach soll
    /// kein späterer eigener Write mit dem Merkstand zurückschreiben.
    pub fn abgleichen(&self, match_id: i64, in_btp: Planung) {
        let mut g = self.eintraege.lock().unwrap();
        if g.get(&match_id).is_some_and(|(p, _)| *p == in_btp) {
            g.remove(&match_id);
        }
    }

    /// Ein anderer Write (Ergebnis) hat das Spiel geändert, ohne die vier
    /// Felder bekannt zu geben: Merkstand verwerfen, ab jetzt gilt BTP.
    pub fn loeschen(&self, match_id: i64) {
        self.eintraege.lock().unwrap().remove(&match_id);
    }

    /// Vor dem Senden eines Ergebnisses: Dieses Spiel ist gleich gewertet.
    ///
    /// Ein Farb- oder Schiedsrichter-Write an ein gewertetes Spiel nimmt ihm
    /// in BTP den Sieger (Messung 13.11.2026, `tests/btp_shuttles_probe.rs`)
    /// — und BTP zeigt die Wertung erst nach einigen hundert Millisekunden.
    /// Bis dahin hält dieser Merker solche Writes zurück.
    pub fn wertung_merken(&self, match_id: i64, jetzt_ms: u64) {
        let mut g = self.gewertet.lock().unwrap();
        g.retain(|_, seit| frisch_genug(*seit, jetzt_ms));
        g.insert(match_id, jetzt_ms);
    }

    /// Ging in den letzten [`HALTE_MS`] ein Ergebnis an dieses Spiel?
    pub fn kuerzlich_gewertet(&self, match_id: i64, jetzt_ms: u64) -> bool {
        self.gewertet
            .lock()
            .unwrap()
            .get(&match_id)
            .is_some_and(|seit| frisch_genug(*seit, jetzt_ms))
    }
}

/// Jünger als [`HALTE_MS`]? Ein Zeitpunkt in der Zukunft (Systemuhr
/// zurückgestellt) gilt als abgelaufen — sonst hielte ein Uhrsprung um eine
/// Stunde einen alten Stand eine Stunde lang über BTP.
fn frisch_genug(seit_ms: u64, jetzt_ms: u64) -> bool {
    seit_ms <= jetzt_ms && jetzt_ms - seit_ms < HALTE_MS
}

/// Die prozessweite Instanz.
pub static EIGENE: LazyLock<EigeneWrites> = LazyLock::new(EigeneWrites::default);

/// Reiht die Match-Writes (Feld, Schiedsrichter, Farbe) hintereinander, vom
/// frischen Lesen bis zur Antwort von BTP. Das Schreib-Gate (`write_gate`)
/// lässt Writes parallel laufen; zwei parallele Match-Writes desselben Spiels
/// lasen aber beide den Stand vor dem anderen, und wessen `SENDUPDATE` später
/// ankam, setzte das Feld des anderen zurück (Review 09.11.2026: TL-Farb-Tipp
/// neben dem Schiedsrichter-Write des Sync-Laufs). Reihenfolge der Sperren:
/// immer erst das Gate, dann diese.
pub static SERIELL: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[cfg(test)]
mod tests {
    #[test]
    fn eine_wertung_haelt_folge_writes_nur_kurz_zurueck() {
        // Messung 13.11.2026: Ein Farb- oder Schiedsrichter-Write an ein
        // gewertetes Spiel nimmt ihm den Sieger. BTP zeigt die Wertung erst
        // nach einigen hundert ms — bis dahin gilt der Merker.
        let e = super::EigeneWrites::default();
        assert!(!e.kuerzlich_gewertet(10, 1_000));
        e.wertung_merken(10, 1_000);
        assert!(e.kuerzlich_gewertet(10, 1_000));
        assert!(e.kuerzlich_gewertet(10, 1_000 + super::HALTE_MS - 1));
        assert!(!e.kuerzlich_gewertet(10, 1_000 + super::HALTE_MS));
        assert!(!e.kuerzlich_gewertet(11, 1_000), "nur dieses Spiel");
        // Uhr zurückgestellt: gilt als abgelaufen, nicht als ewig frisch.
        assert!(!e.kuerzlich_gewertet(10, 500));
    }

    use super::*;

    const P: Planung = Planung {
        court_id: 4,
        officials: (7, 0),
        highlight: 1,
        shuttles: 0,
    };

    #[test]
    fn ein_eigener_write_gilt_genau_die_haltezeit() {
        let e = EigeneWrites::default();
        e.merken(10, P, 1_000);
        assert_eq!(e.aktuell(10, 1_000), Some(P));
        assert_eq!(e.aktuell(10, 1_000 + HALTE_MS - 1), Some(P));
        assert_eq!(e.aktuell(10, 1_000 + HALTE_MS), None, "danach gilt BTP");
        assert_eq!(e.aktuell(11, 1_000), None, "anderes Spiel");
    }

    #[test]
    fn der_juengste_write_gewinnt() {
        let e = EigeneWrites::default();
        e.merken(10, P, 1_000);
        let neu = Planung { highlight: 0, ..P };
        e.merken(10, neu, 1_200);
        assert_eq!(e.aktuell(10, 1_300), Some(neu));
    }

    #[test]
    fn ein_gescheiterter_write_nimmt_nur_den_eigenen_eintrag_zurueck() {
        let e = EigeneWrites::default();
        e.merken(10, P, 1_000);
        e.vergessen(10, P, 1_000);
        assert_eq!(e.aktuell(10, 1_100), None);

        // Hat ein späterer Write dasselbe Spiel schon neu gemerkt, bleibt
        // dessen Eintrag stehen.
        e.merken(10, P, 1_000);
        let neu = Planung { court_id: 0, ..P };
        e.merken(10, neu, 1_050);
        e.vergessen(10, P, 1_000);
        assert_eq!(e.aktuell(10, 1_100), Some(neu));
    }

    #[test]
    fn ein_uhrsprung_zurueck_laesst_den_merkstand_ablaufen() {
        let e = EigeneWrites::default();
        e.merken(10, P, 1_000_000);
        assert_eq!(e.aktuell(10, 1_000_000 - 60_000), None);
    }

    #[test]
    fn bestaetigt_btp_den_stand_gilt_wieder_btp() {
        let e = EigeneWrites::default();
        e.merken(10, P, 1_000);
        e.abgleichen(10, Planung { highlight: 0, ..P });
        assert_eq!(
            e.aktuell(10, 1_100),
            Some(P),
            "anderer Stand: noch nicht übernommen"
        );
        e.abgleichen(10, P);
        assert_eq!(e.aktuell(10, 1_100), None);
        e.merken(11, P, 1_000);
        e.loeschen(11);
        assert_eq!(e.aktuell(11, 1_100), None);
    }

    #[test]
    fn abgelaufene_eintraege_werden_beim_merken_weggeraeumt() {
        let e = EigeneWrites::default();
        e.merken(10, P, 1_000);
        e.merken(11, P, 1_000 + HALTE_MS);
        assert_eq!(e.eintraege.lock().unwrap().len(), 1);
    }
}
