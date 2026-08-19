//! Projektion für den Schiedsrichterzettel (Spec
//! `docs/features/schiedsrichterzettel-druck.md`, ADR 0037/0039).
//!
//! Rechnet aus **zwei** Strömen ein Zellenraster: dem Punktverlauf
//! ([`super::timeline`], Ballwechsel) und den Ereignissen
//! ([`super::sheet`], Karten und Aufschlagfolge). Der Join geschieht erst
//! hier — die Ströme bleiben getrennt (ADR 0037).
//!
//! **Reine Funktion, kein Zustand.** Das Layout wird deshalb als
//! *Struktur* geprüft (Zellenzahl, Zeilenzuordnung, Umbruch), nicht als
//! Pixelvergleich.

use relay_proto::{EventKind, MatchEvent, MatchTimeline, Phase};

/// Spalten je Zeilengruppe. Ballwechsel 61–120 kommen als zweite Gruppe
/// darunter („Fortsetzung") — 2 × 60 deckt `MAX_RALLIES_PER_SET` (120) ab.
pub const SPALTEN_JE_GRUPPE: usize = 60;

/// Eine gefüllte Rasterzelle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zelle {
    /// `2 * team + player` im Doppel; `team` im degradierten Fall.
    pub row: usize,
    /// Ballwechsel-Nummer im Satz, **1-basiert**.
    pub col: usize,
    /// Zeilengruppe: 0 = Ballwechsel 1–60, 1 = 61–120.
    pub gruppe: usize,
    /// Neuer Punktstand der Seite, die gleich aufschlägt.
    pub wert: i64,
    /// Druckbar in Schwarz-Weiß: `V` Verwarnung, `F` rote Karte,
    /// `D` Disqualifikation.
    pub marker: Option<char>,
}

/// Ein Ereignis ohne Trägerballwechsel (Karte in der Satzpause, vor dem
/// ersten Aufschlag). Es steht als Marker am Blockrand statt in einer
/// Zelle — sonst müsste es sich eine Zelle mit einem Ballwechsel teilen,
/// zu dem es nicht gehört.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandMarker {
    pub row: usize,
    pub nach_ballwechsel: i64,
    pub marker: char,
}

/// Ein Satz-Block des Rasters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SatzBlock {
    /// 1-basierte Satznummer.
    pub satz: i64,
    /// Startstand — 0:0, außer beim Zwischenstand-Einstieg.
    pub start_a: i64,
    pub start_b: i64,
    pub end_a: i64,
    pub end_b: i64,
    pub zellen: Vec<Zelle>,
    pub rand_marker: Vec<RandMarker>,
}

/// Eine Zeile der Protokollspalte unter dem Raster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtokollZeile {
    pub nr: usize,
    pub ts_ms: u64,
    pub satz: i64,
    pub score_a: i64,
    pub score_b: i64,
    pub art: EventKind,
    pub phase: Phase,
    pub team: i64,
    pub player: i64,
    /// Zurückgenommen — erscheint **durchgestrichen** im Protokoll und
    /// **nicht** im Raster. Für einen Archivbeleg ist das ehrlicher als
    /// spurloses Verschwinden (ADR 0038).
    pub zurueckgenommen: bool,
}

/// Das fertige Raster.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Grid {
    /// 4 im Doppel mit aufgezeichneter Aufschlagfolge, sonst 2.
    pub zeilen: usize,
    pub blocks: Vec<SatzBlock>,
    /// Kein `serve_start` aufgezeichnet (Altbestand): Das Raster fällt auf
    /// zwei Zeilen zurück und sagt das auf dem Zettel dazu.
    pub aufschlagfolge_fehlt: bool,
    pub protokoll: Vec<ProtokollZeile>,
}

/// Zeichen für die Zelle. `None` = erscheint nur im Protokoll.
fn marker_fuer(art: EventKind) -> Option<char> {
    match art {
        EventKind::CardYellow => Some('V'),
        EventKind::CardRed => Some('F'),
        EventKind::CardBlack | EventKind::Disqualified => Some('D'),
        _ => None,
    }
}

/// Aufschlag-Geometrie eines Satzes.
///
/// Bildet die Doppel-Rotation nach: Gewinnt die aufschlagende Seite,
/// tauschen ihre beiden Spieler die Felder und **derselbe** schlägt
/// weiter auf; gewinnt die annehmende Seite, geht der Aufschlag über und
/// es schlägt auf, wer nach der neuen Punktzahl im richtigen Feld steht
/// (gerade = rechts).
struct Aufschlag {
    /// `feld[team][0]` = Spieler im rechten Feld, `[1]` = im linken.
    feld: [[i64; 2]; 2],
    team: i64,
    spieler: i64,
}

impl Aufschlag {
    /// Aus einem `serve_start` und dem Satz-Startstand aufbauen.
    fn neu(start: &MatchEvent, punkte: [i64; 2], doppel: bool) -> Self {
        let s_team = start.team.clamp(0, 1);
        let r_team = start.receiver_team.clamp(0, 1);
        let s_spieler = if doppel { start.player.clamp(0, 1) } else { 0 };
        let r_spieler = if doppel {
            start.receiver_player.clamp(0, 1)
        } else {
            0
        };
        // Gerader Stand → der Aufschläger steht rechts. Der Empfänger
        // steht diagonal, also im gleichnamigen Feld seiner Seite: Aus
        // dem rechten Feld wird ins rechte Feld aufgeschlagen.
        let seite = (punkte[s_team as usize] % 2) as usize;
        let mut feld = [[0i64; 2]; 2];
        feld[s_team as usize][seite] = s_spieler;
        feld[s_team as usize][1 - seite] = if doppel { 1 - s_spieler } else { 0 };
        feld[r_team as usize][seite] = r_spieler;
        feld[r_team as usize][1 - seite] = if doppel { 1 - r_spieler } else { 0 };
        Self {
            feld,
            team: s_team,
            spieler: s_spieler,
        }
    }

    /// Ohne aufgezeichnete Aufschlagfolge: Es zählt nur die Seite.
    fn degradiert() -> Self {
        Self {
            feld: [[0; 2]; 2],
            team: 0,
            spieler: 0,
        }
    }

    /// Einen Ballwechsel verarbeiten; liefert, wer danach aufschlägt.
    fn nach_ballwechsel(&mut self, sieger: i64, punkte: [i64; 2], doppel: bool) -> (i64, i64) {
        if sieger == self.team {
            // Eigener Punkt: Felder tauschen, derselbe schlägt weiter auf.
            if doppel {
                self.feld[sieger as usize].swap(0, 1);
            }
        } else {
            // Aufschlagwechsel: Es schlägt auf, wer im Feld zur neuen
            // Punktzahl steht.
            self.team = sieger;
            let seite = (punkte[sieger as usize] % 2) as usize;
            self.spieler = if doppel {
                self.feld[sieger as usize][seite]
            } else {
                0
            };
        }
        (self.team, self.spieler)
    }
}

/// Das Raster aus Punktverlauf und Ereignissen rechnen.
///
/// **Auslegung der Spec-Zeile „Zellinhalt = neuer Punktstand des
/// Aufschlägers":** Eine Zelle je Ballwechsel, gesetzt in der Zeile des
/// Spielers, der **anschließend** aufschlägt, mit dem neuen Punktstand
/// seiner Seite. Beide Lesarten fallen damit zusammen — gewinnt die
/// aufschlagende Seite, bleibt es derselbe Aufschläger und dieselbe
/// Zeile; bei Aufschlagwechsel wandert die Zelle zur neuen
/// aufschlagenden Seite. So füllt sich auch ein Bogen aus Papier.
pub fn sheet_grid(timeline: &MatchTimeline, events: &[MatchEvent], doppel: bool) -> Grid {
    // Zurückgenommenes gehört nicht ins Raster (ADR 0038).
    let zurueckgenommen: Vec<&str> = events
        .iter()
        .filter(|e| e.kind == EventKind::Retract)
        .map(|e| e.retracts.as_str())
        .collect();
    let ist_zurueckgenommen =
        |e: &MatchEvent| -> bool { zurueckgenommen.iter().any(|id| *id == e.id) };

    let wirksam: Vec<&MatchEvent> = events
        .iter()
        .filter(|e| e.kind != EventKind::Retract && !ist_zurueckgenommen(e))
        .collect();

    // Ohne aufgezeichnete Aufschlagfolge fällt das ganze Raster auf zwei
    // Zeilen zurück — ein halb aufgelöstes Doppel wäre irreführender als
    // ein ehrlich grobes Raster.
    let mit_folge: Vec<&&MatchEvent> = wirksam
        .iter()
        .filter(|e| e.kind == EventKind::ServeStart)
        .collect();
    let aufschlagfolge_fehlt = timeline
        .sets
        .iter()
        .enumerate()
        .any(|(i, s)| !s.points.is_empty() && !mit_folge.iter().any(|e| e.set == i as i64 + 1));
    let volles_raster = doppel && !aufschlagfolge_fehlt;

    let mut blocks = Vec::new();
    for (i, satz) in timeline.sets.iter().enumerate() {
        let nr = i as i64 + 1;
        let mut punkte = [satz.start_a, satz.start_b];
        let mut geo = wirksam
            .iter()
            .find(|e| e.kind == EventKind::ServeStart && e.set == nr)
            .filter(|_| !aufschlagfolge_fehlt)
            .map(|e| Aufschlag::neu(e, punkte, volles_raster))
            .unwrap_or_else(Aufschlag::degradiert);

        let mut zellen = Vec::new();
        for (n, c) in satz.points.chars().enumerate() {
            let sieger: i64 = if c == 'A' { 0 } else { 1 };
            punkte[sieger as usize] += 1;
            let (team, spieler) = geo.nach_ballwechsel(sieger, punkte, volles_raster);
            let col = n + 1;
            zellen.push(Zelle {
                row: if volles_raster {
                    (2 * team + spieler) as usize
                } else {
                    team as usize
                },
                col,
                gruppe: (col - 1) / SPALTEN_JE_GRUPPE,
                wert: punkte[team as usize],
                marker: None,
            });
        }

        // Ereignisse einhängen: `after_n` ist die Zahl der aufgezeichneten
        // Ballwechsel bei der Erfassung, also 1-basiert genau die Spalte.
        let mut rand_marker = Vec::new();
        for e in wirksam.iter().filter(|e| e.set == nr) {
            let Some(zeichen) = marker_fuer(e.kind) else {
                continue;
            };
            let row = if volles_raster {
                (2 * e.team.clamp(0, 1) + e.player.clamp(0, 1)) as usize
            } else {
                e.team.clamp(0, 1) as usize
            };
            let treffer = (e.phase == Phase::Play)
                .then(|| zellen.iter_mut().find(|z| z.col as i64 == e.after_n))
                .flatten();
            match treffer {
                // Genau EINE Zelle je Karte: die des Ballwechsels, den sie
                // erzeugt hat.
                Some(zelle) => zelle.marker = Some(zeichen),
                None => rand_marker.push(RandMarker {
                    row,
                    nach_ballwechsel: e.after_n,
                    marker: zeichen,
                }),
            }
        }

        blocks.push(SatzBlock {
            satz: nr,
            start_a: satz.start_a,
            start_b: satz.start_b,
            end_a: punkte[0],
            end_b: punkte[1],
            zellen,
            rand_marker,
        });
    }

    // Protokoll: alles, auch Zurückgenommenes — dort durchgestrichen.
    let mut protokoll: Vec<ProtokollZeile> = events
        .iter()
        .filter(|e| e.kind != EventKind::Retract)
        .enumerate()
        .map(|(i, e)| ProtokollZeile {
            nr: i + 1,
            ts_ms: e.ts_ms,
            satz: e.set,
            score_a: e.score_a,
            score_b: e.score_b,
            art: e.kind,
            phase: e.phase,
            team: e.team,
            player: e.player,
            zurueckgenommen: ist_zurueckgenommen(e),
        })
        .collect();
    for (i, z) in protokoll.iter_mut().enumerate() {
        z.nr = i + 1;
    }

    Grid {
        zeilen: if volles_raster { 4 } else { 2 },
        blocks,
        aufschlagfolge_fehlt,
        protokoll,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::TimelineSet;

    /// Die Zeilenformel des Rasters, sichtbar gemacht: `2 * team + player`.
    fn zeile(team: usize, player: usize) -> usize {
        2 * team + player
    }

    fn satz(start_a: i64, start_b: i64, points: &str) -> TimelineSet {
        TimelineSet {
            start_a,
            start_b,
            points: points.to_string(),
        }
    }

    fn tl(sets: Vec<TimelineSet>) -> MatchTimeline {
        MatchTimeline {
            sets,
            ..Default::default()
        }
    }

    fn ev(id: &str, set: i64, after_n: i64, kind: EventKind, team: i64, player: i64) -> MatchEvent {
        MatchEvent {
            id: id.to_string(),
            seq: after_n,
            set,
            after_n,
            score_a: 0,
            score_b: 0,
            ts_ms: 1_755_600_000_000,
            kind,
            team,
            player,
            receiver_team: 1 - team,
            receiver_player: 0,
            phase: Phase::Play,
            retracts: String::new(),
        }
    }

    fn aufschlag(set: i64, team: i64, player: i64, r_team: i64, r_player: i64) -> MatchEvent {
        let mut e = ev("a0", set, 0, EventKind::ServeStart, team, player);
        e.id = format!("a{set}{team}{player}");
        e.receiver_team = r_team;
        e.receiver_player = r_player;
        e
    }

    /// Einzel 21:19 — Zeilenzuordnung (nur zwei Zeilen) und Zellwerte.
    #[test]
    fn einzel_zeilen_und_zellwerte() {
        // A gewinnt drei, B zwei, A einen: 4:2.
        let grid = sheet_grid(
            &tl(vec![satz(0, 0, "AABBA")]),
            &[aufschlag(1, 0, 0, 1, 0)],
            false,
        );
        assert_eq!(grid.zeilen, 2);
        assert!(!grid.aufschlagfolge_fehlt);
        let b = &grid.blocks[0];
        assert_eq!(b.zellen.len(), 5, "eine Zelle je Ballwechsel");
        // Werte: A1, A2, B1, B2, A3 — jeweils der Stand der Seite, die
        // anschließend aufschlägt.
        let werte: Vec<(usize, i64)> = b.zellen.iter().map(|z| (z.row, z.wert)).collect();
        assert_eq!(werte, vec![(0, 1), (0, 2), (1, 1), (1, 2), (0, 3)]);
        assert_eq!((b.end_a, b.end_b), (3, 2));
    }

    /// Doppel: vier Zeilen, Partnerwechsel bei eigenem Punkt, Side-out
    /// wechselt das Team, Empfänger-Diagonale.
    #[test]
    fn doppel_partnerwechsel_und_side_out() {
        // Aufschlag: Team A, Spieler 0, gegen Team B, Spieler 0.
        // Stand 0:0 → Aufschläger rechts, Empfänger rechts.
        let grid = sheet_grid(
            &tl(vec![satz(0, 0, "AAB")]),
            &[aufschlag(1, 0, 0, 1, 0)],
            true,
        );
        assert_eq!(grid.zeilen, 4);
        let z = &grid.blocks[0].zellen;

        // Ballwechsel 1: A punktet, derselbe Spieler schlägt weiter auf
        // (er wechselt nur das Feld) → Zeile von A0, Stand 1.
        assert_eq!((z[0].row, z[0].wert), (zeile(0, 0), 1));
        // Ballwechsel 2: A punktet erneut, weiterhin Spieler 0.
        assert_eq!((z[1].row, z[1].wert), (zeile(0, 0), 2));
        // Ballwechsel 3: B punktet → Aufschlagwechsel. B steht bei 1
        // (ungerade) → es schlägt auf, wer links steht. B0 begann rechts
        // (Empfänger-Diagonale), also steht B1 links.
        assert_eq!((z[2].row, z[2].wert), (zeile(1, 1), 1));
    }

    /// Altbestand ohne `serve_start`: zwei Zeilen, Werte korrekt, Hinweis.
    #[test]
    fn ohne_aufschlagfolge_degradiert_das_raster() {
        let grid = sheet_grid(&tl(vec![satz(0, 0, "AABBA")]), &[], true);
        assert!(grid.aufschlagfolge_fehlt, "Hinweis muss gesetzt sein");
        assert_eq!(grid.zeilen, 2, "Degradation auf zwei Zeilen");
        let werte: Vec<(usize, i64)> = grid.blocks[0]
            .zellen
            .iter()
            .map(|z| (z.row, z.wert))
            .collect();
        assert_eq!(werte, vec![(0, 1), (0, 2), (1, 1), (1, 2), (0, 3)]);
    }

    /// Eine rote Karte belegt **genau eine** Zelle: die des Ballwechsels,
    /// den sie erzeugt hat.
    #[test]
    fn rote_karte_belegt_genau_eine_zelle() {
        // Ballwechsel 3 entstand durch die rote Karte gegen Team A.
        let karte = ev("cc", 1, 3, EventKind::CardRed, 0, 0);
        let grid = sheet_grid(
            &tl(vec![satz(0, 0, "AABBA")]),
            &[aufschlag(1, 0, 0, 1, 0), karte],
            false,
        );
        let mit_marker: Vec<&Zelle> = grid.blocks[0]
            .zellen
            .iter()
            .filter(|z| z.marker.is_some())
            .collect();
        assert_eq!(mit_marker.len(), 1, "genau eine Zelle");
        assert_eq!(mit_marker[0].col, 3);
        assert_eq!(mit_marker[0].marker, Some('F'));
        assert!(grid.blocks[0].rand_marker.is_empty());
    }

    /// Die Zellenzahl hängt am Punktverlauf, nie an der Ereigniszahl.
    #[test]
    fn zellenzahl_entspricht_immer_den_ballwechseln() {
        let punkte = "ABABABABAB";
        let viele: Vec<MatchEvent> = (0..20)
            .map(|i| ev(&format!("{i:04x}"), 1, 2, EventKind::CardYellow, 0, 0))
            .collect();
        let grid = sheet_grid(&tl(vec![satz(0, 0, punkte)]), &viele, false);
        assert_eq!(grid.blocks[0].zellen.len(), punkte.len());
    }

    /// Ereignisse ohne Trägerballwechsel (Satzpause, vor dem ersten
    /// Aufschlag) stehen am Blockrand, nicht in einer fremden Zelle.
    #[test]
    fn ereignis_ohne_ballwechsel_steht_am_rand() {
        let mut vor_dem_spiel = ev("dd", 1, 0, EventKind::CardYellow, 1, 0);
        vor_dem_spiel.phase = Phase::PreMatch;
        let mut in_der_pause = ev("ee", 1, 2, EventKind::CardYellow, 0, 0);
        in_der_pause.phase = Phase::BreakEleven;

        let grid = sheet_grid(
            &tl(vec![satz(0, 0, "AABBA")]),
            &[aufschlag(1, 0, 0, 1, 0), vor_dem_spiel, in_der_pause],
            false,
        );
        assert_eq!(grid.blocks[0].rand_marker.len(), 2);
        assert!(grid.blocks[0].zellen.iter().all(|z| z.marker.is_none()));
    }

    /// Umbruch in die zweite Zeilengruppe ab Ballwechsel 61.
    #[test]
    fn umbruch_ab_ballwechsel_61() {
        let punkte: String = std::iter::repeat_n("AB", 35).collect(); // 70
        let grid = sheet_grid(&tl(vec![satz(0, 0, &punkte)]), &[], false);
        let z = &grid.blocks[0].zellen;
        assert_eq!(z[59].col, 60);
        assert_eq!(z[59].gruppe, 0);
        assert_eq!(z[60].col, 61);
        assert_eq!(z[60].gruppe, 1, "ab 61 die zweite Zeilengruppe");
    }

    /// Ein `mid_game`-Satz beginnt beim eingetippten Zwischenstand.
    #[test]
    fn mid_game_satz_beginnt_beim_zwischenstand() {
        let grid = sheet_grid(&tl(vec![satz(7, 5, "AB")]), &[], false);
        let b = &grid.blocks[0];
        assert_eq!((b.start_a, b.start_b), (7, 5));
        assert_eq!(b.zellen[0].wert, 8, "A zählt von 7 weiter");
        assert_eq!(b.zellen[1].wert, 6, "B von 5");
        assert_eq!((b.end_a, b.end_b), (8, 6));
    }

    /// Ein zurückgenommenes Ereignis erscheint **nicht** im Raster, aber
    /// durchgestrichen im Protokoll (ADR 0038).
    #[test]
    fn zurueckgenommenes_steht_nur_im_protokoll() {
        let karte = ev("cc", 1, 3, EventKind::CardRed, 0, 0);
        let mut ruecknahme = ev("dd", 1, 3, EventKind::Retract, 0, 0);
        ruecknahme.retracts = "cc".to_string();

        let grid = sheet_grid(
            &tl(vec![satz(0, 0, "AABBA")]),
            &[aufschlag(1, 0, 0, 1, 0), karte, ruecknahme],
            false,
        );
        assert!(
            grid.blocks[0].zellen.iter().all(|z| z.marker.is_none()),
            "nicht im Raster"
        );
        assert!(grid.blocks[0].rand_marker.is_empty());

        let karte_im_protokoll = grid
            .protokoll
            .iter()
            .find(|z| z.art == EventKind::CardRed)
            .expect("im Protokoll");
        assert!(karte_im_protokoll.zurueckgenommen);
        // Die Rücknahme selbst ist keine eigene Protokollzeile — sie
        // streicht die bestehende durch.
        assert!(grid.protokoll.iter().all(|z| z.art != EventKind::Retract));
    }

    /// Ein Match kann Ereignisse ohne jeden Ballwechsel haben (Karte vor
    /// dem ersten Aufschlag) — dann gibt es kein Raster, aber ein
    /// Protokoll.
    #[test]
    fn ereignisse_ohne_punktverlauf_ergeben_ein_protokoll() {
        let mut karte = ev("dd", 1, 0, EventKind::CardYellow, 1, 0);
        karte.phase = Phase::PreMatch;
        let grid = sheet_grid(&tl(vec![]), &[karte], false);
        assert!(grid.blocks.is_empty());
        assert_eq!(grid.protokoll.len(), 1);
    }
}
