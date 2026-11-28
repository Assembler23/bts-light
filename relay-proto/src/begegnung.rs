//! Monitoransicht „Begegnung“ für Punktspiele (Spec `punktspiel-monitoransicht`,
//! ADR 0109).
//!
//! Der Host baut aus dem Punktspiel-Paket eine [`BegegnungSicht`] — eine
//! Positivliste ohne Personen-ID und ohne Lizenznummer — und schickt sie dem
//! Relay. Was der Bildschirm zeigt, rechnet **eine** Funktion für Host und
//! Relay: [`begegnung_anzeige`] mischt den Live-Stand der Felder in die
//! Zeilen und zählt den Gesamtstand. So gibt es keine zweite Zählregel, die
//! auseinanderlaufen könnte.
//!
//! Hier liegen auch die Satzregeln ([`satz_ist_fertig`],
//! [`spiel_entschieden`]), weil der Relay sie für den Zustand „Ergebnis wird
//! übermittelt“ braucht. Der Host ruft sie über dünne Hüllen in
//! `tablet/server.rs` weiter auf.

use serde::{Deserialize, Serialize};

use crate::SetAb;

/// Höchstzahl der Spiele in einer Sicht. Die Seite zeigt zwölf; der Rest ist
/// Reserve für ungewöhnliche Spielsysteme.
pub const MAX_BEGEGNUNG_SPIELE: usize = 16;
/// Obergrenze der serialisierten Sicht. Darüber verwirft der Relay sie.
pub const MAX_BEGEGNUNG_LEN: usize = 16 * 1024;
/// Länge von Mannschaftsname und Staffel.
pub const MAX_BEGEGNUNG_TEXT: usize = 80;
/// Länge eines Spielernamens.
pub const MAX_BEGEGNUNG_NAME: usize = 60;
/// Länge einer Disziplin („1.HD“).
pub const MAX_BEGEGNUNG_DISZIPLIN: usize = 12;
/// Länge des Ausgangs („disqualifikation“).
pub const MAX_BEGEGNUNG_AUSGANG: usize = 20;
/// Höchster Punktwert eines Satzes und höchste Spiel- und Feldnummer, die
/// der Relay annimmt. Weit über allem, was vorkommt — die Grenze hält nur
/// unsinnige Zahlen eines fremden Hosts aus den Summen heraus.
pub const MAX_BEGEGNUNG_PUNKTE: i64 = 99;
pub const MAX_BEGEGNUNG_NUMMER: i64 = 999;
/// Sätze je Spiel.
pub const MAX_BEGEGNUNG_SAETZE: usize = 5;

// ── Satzregeln ──────────────────────────────────────────────────────────────

/// Ist ein Satz `(a, b)` regulär zu Ende gespielt? Erreicht bei `target` mit
/// zwei Punkten Vorsprung, spätestens beim `cap` (dort reicht ein Punkt).
/// Fehlt das Format, gilt 21/30.
pub fn satz_ist_fertig(a: i64, b: i64, target: i64, cap: i64) -> bool {
    let target = if target > 0 { target } else { 21 };
    let cap = if cap >= target { cap } else { target.max(30) };
    let hi = a.max(b);
    let lo = a.min(b);
    if hi < target || hi > cap {
        return false; // Ziel nicht erreicht oder über dem Deckel
    }
    // am Deckel reicht 1 Punkt, sonst 2 Vorsprung. Sättigend: Die Werte
    // kommen am Relay ungeprüft von außen.
    hi >= cap || hi.saturating_sub(lo) >= 2
}

/// Ist ein Spiel nach seinen Sätzen bereits entschieden?
///
/// Eine Seite hat die Satzmehrheit (`best_of / 2 + 1`) aus regulär zu Ende
/// gespielten Sätzen. Ein 0:0 (Geistersatz nach dem Match-Ende) wird
/// übersprungen; ein angefangener Satz heißt, dass noch gespielt wird. Ohne
/// aufgelöste Zählweise (`target <= 0`) gilt nichts als entschieden.
/// Aufgabe, kampflos und Disqualifikation erkennt die Funktion bewusst nicht.
pub fn spiel_entschieden(sets: &[(i64, i64)], best_of: i64, target: i64, cap: i64) -> bool {
    if target <= 0 {
        return false;
    }
    let noetig = (best_of / 2 + 1).max(1);
    let (mut a, mut b) = (0i64, 0i64);
    for &(x, y) in sets {
        if x == 0 && y == 0 {
            continue;
        }
        if !satz_ist_fertig(x, y, target, cap) {
            return false;
        }
        if x > y {
            a += 1;
        } else {
            b += 1;
        }
    }
    a >= noetig || b >= noetig
}

// ── Sicht (Host → Relay) ────────────────────────────────────────────────────

/// Seite einer Begegnung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BegegnungSeite {
    Heim,
    Gast,
}

/// Status eines Spiels, wie badhub ihn führt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BegegnungStatus {
    #[default]
    Offen,
    Laeuft,
    Beendet,
}

/// Die Begegnung, wie der Host sie dem Relay schickt. **Positivliste:** nur
/// Namen, nie Personen-ID, Lizenznummer, Verein oder Nation je Person.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BegegnungSicht {
    #[serde(default)]
    pub heim: String,
    #[serde(default)]
    pub gast: String,
    #[serde(default)]
    pub staffel: String,
    /// Verein der Heimmannschaft in badhub — öffentlicher Schlüssel für das
    /// Vereinslogo (`/info/club-logo?id=`), ohne Personenbezug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heim_verein_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gast_verein_id: Option<u32>,
    /// Zählweise — Grundlage für „Ergebnis wird übermittelt“.
    #[serde(default)]
    pub best_of: i64,
    #[serde(default)]
    pub ziel: i64,
    #[serde(default)]
    pub kappe: i64,
    #[serde(default)]
    pub spiele: Vec<BegegnungZeile>,
}

/// Ein Spiel der Begegnung.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BegegnungZeile {
    /// Spielnummer — zugleich die Match-ID am Feld.
    pub nr: i64,
    #[serde(default)]
    pub disziplin: String,
    /// Doppel? Dann erwartet die Seite zwei Namen je Seite und nennt einen
    /// fehlenden Platz „noch nicht aufgestellt“.
    #[serde(default)]
    pub doppel: bool,
    #[serde(default)]
    pub heim: Vec<String>,
    #[serde(default)]
    pub gast: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feld: Option<i64>,
    #[serde(default)]
    pub status: BegegnungStatus,
    #[serde(default)]
    pub saetze: Vec<SetAb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sieger: Option<BegegnungSeite>,
    /// `normal`, `kampflos`, `aufgabe` oder `disqualifikation`.
    #[serde(default)]
    pub ausgang: String,
}

fn kuerze(text: &mut String, max: usize) {
    if text.chars().count() > max {
        *text = text.chars().take(max).collect();
    }
}

impl BegegnungSicht {
    /// Kürzt Texte und kappt Listen auf die Deckel. Der Host ruft das vor
    /// dem Senden, der Relay nach dem Empfang (er vertraut dem Host nicht).
    pub fn deckeln(&mut self) {
        kuerze(&mut self.heim, MAX_BEGEGNUNG_TEXT);
        kuerze(&mut self.gast, MAX_BEGEGNUNG_TEXT);
        kuerze(&mut self.staffel, MAX_BEGEGNUNG_TEXT);
        self.best_of = self.best_of.clamp(1, 7);
        self.ziel = self.ziel.clamp(0, MAX_BEGEGNUNG_PUNKTE);
        self.kappe = self.kappe.clamp(0, MAX_BEGEGNUNG_PUNKTE);
        self.spiele.truncate(MAX_BEGEGNUNG_SPIELE);
        for z in &mut self.spiele {
            z.nr = z.nr.clamp(0, MAX_BEGEGNUNG_NUMMER);
            z.feld = z.feld.map(|f| f.clamp(0, MAX_BEGEGNUNG_NUMMER));
            kuerze(&mut z.disziplin, MAX_BEGEGNUNG_DISZIPLIN);
            kuerze(&mut z.ausgang, MAX_BEGEGNUNG_AUSGANG);
            for namen in [&mut z.heim, &mut z.gast] {
                namen.truncate(2);
                namen.iter_mut().for_each(|n| kuerze(n, MAX_BEGEGNUNG_NAME));
            }
            z.saetze.truncate(MAX_BEGEGNUNG_SAETZE);
            for s in &mut z.saetze {
                s.a = s.a.clamp(0, MAX_BEGEGNUNG_PUNKTE);
                s.b = s.b.clamp(0, MAX_BEGEGNUNG_PUNKTE);
            }
        }
    }

    /// Passt die Sicht unter [`MAX_BEGEGNUNG_LEN`]? Nach [`Self::deckeln`]
    /// ist das immer so; die Prüfung fängt einen Fehler in den Deckeln ab.
    pub fn passt(&self) -> bool {
        serde_json::to_string(self).is_ok_and(|s| s.len() <= MAX_BEGEGNUNG_LEN)
    }
}

// ── Anzeige (Antwort der Stand-Route) ───────────────────────────────────────

/// Live-Stand eines laufenden Spiels vom Feld.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BegegnungLive {
    pub feld: Option<i64>,
    pub saetze: Vec<SetAb>,
}

/// Zustand einer Zeile auf dem Bildschirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BegegnungZustand {
    #[default]
    Offen,
    Laeuft,
    /// Am Tablet zu Ende gespielt, von badhub noch nicht bestätigt. Zählt
    /// nicht zum Gesamtstand.
    WirdUebermittelt,
    Beendet,
}

/// Zeile der Anzeige.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BegegnungAnzeigeZeile {
    pub nr: i64,
    pub disziplin: String,
    pub doppel: bool,
    pub heim: Vec<String>,
    pub gast: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feld: Option<i64>,
    pub zustand: BegegnungZustand,
    pub saetze: Vec<SetAb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sieger: Option<BegegnungSeite>,
    pub ausgang: String,
}

/// Summen einer Seite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BegegnungSumme {
    pub spiele: i64,
    pub saetze: i64,
    pub punkte: i64,
}

/// Gesamtstand beider Seiten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BegegnungStand {
    pub heim: BegegnungSumme,
    pub gast: BegegnungSumme,
}

/// Was die Seite `/info/begegnung` zeigt. `aktiv = false`: kein Punktspiel —
/// die Seite zeigt dann nur einen Hinweis.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BegegnungAnzeige {
    pub aktiv: bool,
    pub heim: String,
    pub gast: String,
    pub staffel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heim_verein_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gast_verein_id: Option<u32>,
    pub spiele: Vec<BegegnungAnzeigeZeile>,
    pub stand: BegegnungStand,
    /// Alle Spiele beendet.
    pub endstand: bool,
}

/// Baut die Anzeige aus der Sicht und dem Live-Stand der Felder.
///
/// `live(nr)` liefert den Stand des Feldes, auf dem das Spiel mit dieser
/// Nummer gerade läuft, sonst `None`.
///
/// **Zählregel** (wörtlich wie badhub): Ein Spiel zählt für den Sieger,
/// sobald es beendet ist und einen Sieger hat — egal ob normal, kampflos,
/// Aufgabe oder Disqualifikation. Sätze und Punkte der Nebenzeile kommen aus
/// den gemeldeten Sätzen genau dieser Spiele. Ein laufendes Spiel zählt
/// nirgends, auch dann nicht, wenn sein Live-Stand schon entschieden ist:
/// Bis badhub das Ergebnis bestätigt, heißt die Zeile „wird übermittelt“.
pub fn begegnung_anzeige(
    sicht: Option<&BegegnungSicht>,
    live: impl Fn(i64) -> Option<BegegnungLive>,
) -> BegegnungAnzeige {
    let Some(sicht) = sicht else {
        return BegegnungAnzeige::default();
    };
    let mut stand = BegegnungStand::default();
    let mut spiele = Vec::with_capacity(sicht.spiele.len());
    for z in &sicht.spiele {
        let mut zeile = BegegnungAnzeigeZeile {
            nr: z.nr,
            disziplin: z.disziplin.clone(),
            doppel: z.doppel,
            heim: z.heim.clone(),
            gast: z.gast.clone(),
            feld: None,
            zustand: BegegnungZustand::Offen,
            saetze: Vec::new(),
            sieger: None,
            ausgang: String::new(),
        };
        match z.status {
            BegegnungStatus::Offen => {}
            BegegnungStatus::Laeuft => {
                zeile.zustand = BegegnungZustand::Laeuft;
                zeile.feld = z.feld;
                zeile.saetze = z.saetze.clone();
                if let Some(l) = live(z.nr) {
                    zeile.feld = l.feld.or(z.feld);
                    let paare: Vec<(i64, i64)> = l.saetze.iter().map(|s| (s.a, s.b)).collect();
                    zeile.saetze = l.saetze;
                    if spiel_entschieden(&paare, sicht.best_of, sicht.ziel, sicht.kappe) {
                        zeile.zustand = BegegnungZustand::WirdUebermittelt;
                        // Der Geistersatz 0:0 nach dem Match-Ende gehört
                        // nicht auf den Bildschirm.
                        zeile.saetze.retain(|s| s.a != 0 || s.b != 0);
                    }
                }
            }
            BegegnungStatus::Beendet => {
                zeile.zustand = BegegnungZustand::Beendet;
                zeile.saetze = z.saetze.clone();
                zeile.sieger = z.sieger;
                zeile.ausgang = z.ausgang.clone();
                if let Some(sieger) = z.sieger {
                    match sieger {
                        BegegnungSeite::Heim => stand.heim.spiele += 1,
                        BegegnungSeite::Gast => stand.gast.spiele += 1,
                    }
                    for s in &z.saetze {
                        // Sättigend und nie negativ: Der Host ruft diese
                        // Funktion mit ungedeckelten Werten aus dem Paket.
                        stand.heim.punkte = stand.heim.punkte.saturating_add(s.a.max(0));
                        stand.gast.punkte = stand.gast.punkte.saturating_add(s.b.max(0));
                        // Ein Satz mit Gleichstand (gibt es regulär nicht)
                        // zählt für niemanden.
                        match s.a.cmp(&s.b) {
                            std::cmp::Ordering::Greater => stand.heim.saetze += 1,
                            std::cmp::Ordering::Less => stand.gast.saetze += 1,
                            std::cmp::Ordering::Equal => {}
                        }
                    }
                }
            }
        }
        spiele.push(zeile);
    }
    let endstand = !spiele.is_empty()
        && spiele
            .iter()
            .all(|z| z.zustand == BegegnungZustand::Beendet);
    BegegnungAnzeige {
        aktiv: true,
        heim: sicht.heim.clone(),
        gast: sicht.gast.clone(),
        staffel: sicht.staffel.clone(),
        heim_verein_id: sicht.heim_verein_id,
        gast_verein_id: sicht.gast_verein_id,
        spiele,
        stand,
        endstand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: i64, b: i64) -> SetAb {
        SetAb { a, b }
    }

    fn zeile(nr: i64, status: BegegnungStatus) -> BegegnungZeile {
        BegegnungZeile {
            nr,
            disziplin: format!("{nr}.HE"),
            doppel: false,
            heim: vec!["Hans Heim".into()],
            gast: vec!["Gerd Gast".into()],
            status,
            ausgang: "normal".into(),
            ..Default::default()
        }
    }

    fn beendet(nr: i64, sieger: Option<BegegnungSeite>, saetze: &[(i64, i64)]) -> BegegnungZeile {
        BegegnungZeile {
            saetze: saetze.iter().map(|&(a, b)| s(a, b)).collect(),
            sieger,
            ..zeile(nr, BegegnungStatus::Beendet)
        }
    }

    fn sicht(spiele: Vec<BegegnungZeile>) -> BegegnungSicht {
        BegegnungSicht {
            heim: "TV Heim 2".into(),
            gast: "SV Gast 1".into(),
            staffel: "Bezirksklasse A".into(),
            heim_verein_id: Some(17),
            gast_verein_id: None,
            best_of: 3,
            ziel: 21,
            kappe: 30,
            spiele,
        }
    }

    fn kein_live(_: i64) -> Option<BegegnungLive> {
        None
    }

    #[test]
    fn ohne_sicht_ist_die_anzeige_inaktiv() {
        let a = begegnung_anzeige(None, kein_live);
        assert!(!a.aktiv);
        assert!(a.spiele.is_empty());
        assert!(!a.endstand);
    }

    #[test]
    fn beendet_mit_sieger_zaehlt_bei_jedem_ausgang() {
        let mut spiele = Vec::new();
        for (nr, ausgang) in ["normal", "kampflos", "aufgabe", "disqualifikation"]
            .iter()
            .enumerate()
        {
            let mut z = beendet(nr as i64 + 1, Some(BegegnungSeite::Heim), &[]);
            z.ausgang = (*ausgang).into();
            spiele.push(z);
        }
        spiele.push(beendet(
            5,
            Some(BegegnungSeite::Gast),
            &[(10, 21), (12, 21)],
        ));
        let a = begegnung_anzeige(Some(&sicht(spiele)), kein_live);
        assert_eq!(a.stand.heim.spiele, 4);
        assert_eq!(a.stand.gast.spiele, 1);
        assert_eq!(a.spiele[1].ausgang, "kampflos");
        assert!(a.endstand);
    }

    #[test]
    fn beendet_ohne_sieger_zaehlt_fuer_niemanden() {
        let a = begegnung_anzeige(
            Some(&sicht(vec![beendet(1, None, &[(21, 10), (21, 12)])])),
            kein_live,
        );
        assert_eq!(a.stand, BegegnungStand::default());
        assert_eq!(a.spiele[0].zustand, BegegnungZustand::Beendet);
    }

    #[test]
    fn nebenzeile_summiert_nur_beendete_spiele_wie_gemeldet() {
        let mut laeuft = zeile(3, BegegnungStatus::Laeuft);
        laeuft.saetze = vec![s(21, 5)];
        let a = begegnung_anzeige(
            Some(&sicht(vec![
                beendet(
                    1,
                    Some(BegegnungSeite::Heim),
                    &[(21, 15), (18, 21), (21, 19)],
                ),
                // Aufgabe mitten im Satz: Punkte zählen, wie sie stehen.
                beendet(2, Some(BegegnungSeite::Gast), &[(21, 10), (5, 3)]),
                laeuft,
                zeile(4, BegegnungStatus::Offen),
            ])),
            kein_live,
        );
        assert_eq!(a.stand.heim.spiele, 1);
        assert_eq!(a.stand.gast.spiele, 1);
        assert_eq!(a.stand.heim.saetze, 2 + 2);
        assert_eq!(a.stand.gast.saetze, 1);
        assert_eq!(a.stand.heim.punkte, 60 + 26);
        assert_eq!(a.stand.gast.punkte, 55 + 13);
        assert!(!a.endstand);
    }

    #[test]
    fn laufendes_spiel_zeigt_den_live_stand_und_zaehlt_nicht() {
        let mut z = zeile(1, BegegnungStatus::Laeuft);
        z.feld = Some(2);
        z.saetze = vec![s(3, 1)];
        let a = begegnung_anzeige(Some(&sicht(vec![z])), |nr| {
            (nr == 1).then(|| BegegnungLive {
                feld: Some(2),
                saetze: vec![s(21, 18), s(7, 9)],
            })
        });
        assert_eq!(a.spiele[0].zustand, BegegnungZustand::Laeuft);
        assert_eq!(a.spiele[0].saetze, vec![s(21, 18), s(7, 9)]);
        assert_eq!(a.spiele[0].feld, Some(2));
        assert_eq!(a.stand, BegegnungStand::default());
    }

    #[test]
    fn laufendes_spiel_ohne_live_stand_behaelt_die_paket_saetze() {
        let mut z = zeile(1, BegegnungStatus::Laeuft);
        z.feld = Some(1);
        z.saetze = vec![s(11, 9)];
        let a = begegnung_anzeige(Some(&sicht(vec![z])), kein_live);
        assert_eq!(a.spiele[0].saetze, vec![s(11, 9)]);
        assert_eq!(a.spiele[0].feld, Some(1));
    }

    #[test]
    fn entschiedener_live_stand_heisst_wird_uebermittelt_und_zaehlt_nicht() {
        let z = zeile(1, BegegnungStatus::Laeuft);
        let a = begegnung_anzeige(Some(&sicht(vec![z])), |_| {
            Some(BegegnungLive {
                feld: Some(1),
                // Mit dem Geistersatz, den das Tablet nach dem Ende schickt.
                saetze: vec![s(21, 18), s(21, 9), s(0, 0)],
            })
        });
        assert_eq!(a.spiele[0].zustand, BegegnungZustand::WirdUebermittelt);
        assert_eq!(a.spiele[0].saetze, vec![s(21, 18), s(21, 9)]);
        assert_eq!(a.stand, BegegnungStand::default());
        assert!(!a.endstand);
    }

    #[test]
    fn offenes_spiel_hat_keine_saetze_und_kein_feld() {
        let mut z = zeile(1, BegegnungStatus::Offen);
        z.saetze = vec![s(1, 2)];
        z.feld = Some(3);
        let a = begegnung_anzeige(Some(&sicht(vec![z])), |_| {
            Some(BegegnungLive {
                feld: Some(3),
                saetze: vec![s(5, 5)],
            })
        });
        assert_eq!(a.spiele[0].zustand, BegegnungZustand::Offen);
        assert!(a.spiele[0].saetze.is_empty());
        assert_eq!(a.spiele[0].feld, None);
    }

    #[test]
    fn deckeln_kuerzt_texte_und_kappt_listen() {
        let mut lang = zeile(1, BegegnungStatus::Beendet);
        lang.disziplin = "x".repeat(50);
        lang.heim = vec!["ä".repeat(200), "b".into(), "c".into()];
        lang.saetze = vec![s(1, 2); 9];
        let mut sicht = sicht(vec![lang; 30]);
        sicht.heim = "h".repeat(500);
        sicht.deckeln();
        assert_eq!(sicht.heim.chars().count(), MAX_BEGEGNUNG_TEXT);
        assert_eq!(sicht.spiele.len(), MAX_BEGEGNUNG_SPIELE);
        let z = &sicht.spiele[0];
        assert_eq!(z.disziplin.chars().count(), MAX_BEGEGNUNG_DISZIPLIN);
        assert_eq!(z.heim.len(), 2);
        assert_eq!(z.heim[0].chars().count(), MAX_BEGEGNUNG_NAME);
        assert_eq!(z.saetze.len(), MAX_BEGEGNUNG_SAETZE);
        // Auch die größte gedeckelte Sicht passt unter die Obergrenze.
        assert!(sicht.passt());
    }

    /// Unsinnige Zahlen eines fremden Hosts: geklemmt, nichts läuft über.
    #[test]
    fn deckeln_klemmt_zahlen_und_die_summen_laufen_nicht_ueber() {
        let mut z = beendet(i64::MAX, Some(BegegnungSeite::Heim), &[]);
        z.saetze = vec![s(i64::MAX, i64::MIN); 5];
        z.feld = Some(i64::MIN);
        let mut v = sicht(vec![z; 16]);
        v.best_of = i64::MAX;
        v.ziel = i64::MIN;
        v.kappe = i64::MAX;
        // Ungedeckelt (so ruft der Host): kein Überlauf, nichts Negatives.
        let roh = begegnung_anzeige(Some(&v), kein_live);
        assert_eq!(roh.stand.heim.punkte, i64::MAX);
        assert_eq!(roh.stand.gast.punkte, 0);
        // Die Differenz liefe ohne Sättigung über.
        assert!(satz_ist_fertig(25, i64::MIN, 21, i64::MAX));
        v.deckeln();
        assert_eq!((v.best_of, v.ziel, v.kappe), (7, 0, MAX_BEGEGNUNG_PUNKTE));
        let z = &v.spiele[0];
        assert_eq!(z.nr, MAX_BEGEGNUNG_NUMMER);
        assert_eq!(z.feld, Some(0));
        assert_eq!(z.saetze[0], s(MAX_BEGEGNUNG_PUNKTE, 0));
        let a = begegnung_anzeige(Some(&v), kein_live);
        assert_eq!(a.stand.heim.punkte, 16 * 5 * MAX_BEGEGNUNG_PUNKTE);
        assert_eq!(a.stand.heim.saetze, 80);
    }

    #[test]
    fn satz_mit_gleichstand_zaehlt_fuer_niemanden() {
        let a = begegnung_anzeige(
            Some(&sicht(vec![beendet(
                1,
                Some(BegegnungSeite::Heim),
                &[(21, 10), (5, 5)],
            )])),
            kein_live,
        );
        assert_eq!((a.stand.heim.saetze, a.stand.gast.saetze), (1, 0));
        assert_eq!((a.stand.heim.punkte, a.stand.gast.punkte), (26, 15));
    }

    #[test]
    fn sicht_roundtrip_und_alte_form_bleibt_lesbar() {
        let v = sicht(vec![
            beendet(1, Some(BegegnungSeite::Gast), &[(10, 21), (12, 21)]),
            zeile(2, BegegnungStatus::Offen),
        ]);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<BegegnungSicht>(&json).unwrap(), v);
        assert!(json.contains("\"heimVereinId\":17"));
        assert!(!json.contains("gastVereinId"));
        // Eine Sicht mit nur den Pflichtangaben einer Zeile bleibt lesbar.
        let knapp: BegegnungSicht = serde_json::from_str(r#"{"spiele":[{"nr":3}]}"#).unwrap();
        assert_eq!(knapp.spiele[0].status, BegegnungStatus::Offen);
    }

    /// Datenschutz-Wächter: Sicht und Anzeige kennen keine Felder für
    /// Lizenznummer, Personen-ID, Verein oder Nation je Person.
    #[test]
    fn sicht_und_anzeige_tragen_keine_personenschluessel() {
        let v = sicht(vec![
            beendet(1, Some(BegegnungSeite::Heim), &[(21, 10), (21, 12)]),
            zeile(2, BegegnungStatus::Laeuft),
        ]);
        let anzeige = begegnung_anzeige(Some(&v), kein_live);
        for json in [
            serde_json::to_value(&v).unwrap(),
            serde_json::to_value(&anzeige).unwrap(),
        ] {
            let text = json.to_string();
            for verboten in [
                "lizenz",
                "memberId",
                "member_id",
                "\"id\"",
                "club",
                "nation",
                "geburt",
            ] {
                assert!(!text.contains(verboten), "{verboten} in {text}");
            }
            // Namen sind schlichte Texte, keine Objekte.
            for z in json["spiele"].as_array().unwrap() {
                assert!(z["heim"].as_array().unwrap().iter().all(|n| n.is_string()));
                assert!(z["gast"].as_array().unwrap().iter().all(|n| n.is_string()));
            }
        }
    }

    #[test]
    fn satzregeln_wie_bisher_am_host() {
        assert!(satz_ist_fertig(21, 19, 21, 30));
        assert!(!satz_ist_fertig(21, 20, 21, 30));
        assert!(satz_ist_fertig(30, 29, 21, 30));
        assert!(!satz_ist_fertig(31, 29, 21, 30));
        assert!(satz_ist_fertig(21, 15, 0, 0));
        assert!(spiel_entschieden(&[(21, 10), (21, 15)], 3, 21, 30));
        assert!(!spiel_entschieden(&[(21, 10), (15, 21)], 3, 21, 30));
        assert!(!spiel_entschieden(&[(21, 10), (21, 15)], 3, 0, 0));
        assert!(!spiel_entschieden(&[(21, 10), (15, 3)], 3, 21, 30));
    }
}
