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

// ── Blattmaße ────────────────────────────────────────────────────────────
//
// Diese fünf Werte sind Konstanten und **nicht** im CSS versteckt, weil sie
// zusammen ein Budget bilden, das aufgehen muss: 60 Rasterzellen plus
// Namensspalte plus Abstand dürfen die bedruckbare Breite nicht
// überschreiten. Vorher standen sie nur im Stylesheet und ergaben in Summe
// 297 mm bei 281 mm Platz — der Zettel lief um 16 mm über das Blatt hinaus,
// was sich als „die Schrift ist zu groß" bemerkbar machte. `raster_passt_auf
// _die_seite` hält das Budget jetzt fest.

/// Bedruckbare Breite: A4 quer (297 mm) minus zweimal `@page`-Rand (8 mm).
pub const SEITE_NUTZBAR_MM: f32 = 281.0;
/// Breite einer Rasterzelle.
pub const ZELLE_BREITE_MM: f32 = 4.0;
/// Feste Breite der Namensspalte. Fest, damit ein langer Doppelname die
/// Spalte nicht aufbläht und das Raster vom Blatt schiebt.
pub const NAMENSSPALTE_MM: f32 = 34.0;
/// Abstand zwischen Namensspalte und Raster (`.raster { gap }`).
pub const RASTER_ABSTAND_MM: f32 = 3.0;
/// Höhe einer Namens- wie Rasterzeile. Beide müssen gleich sein, sonst
/// laufen Namen und Rasterzeilen auseinander.
pub const ZEILE_HOEHE_MM: f32 = 7.0;

/// Schriftgrad des Spielernamens in der Namensspalte.
pub const NAME_PT: f32 = 8.0;
/// Schriftgrad des Zusatzes (Verein/Nation) darunter.
pub const ZUSATZ_PT: f32 = 6.5;
/// Zeilenabstand des Dokuments (`body { font: …/1.25 }`).
pub const ZEILENABSTAND: f32 = 1.25;
/// Ein typografischer Punkt in Millimetern.
pub const PT_IN_MM: f32 = 0.352_777_8;

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

// ─────────────────── Dokumente aus den Quellen bauen ──────────────────

/// Zettel für die genannten Matches zusammenstellen.
///
/// Join zur Laufzeit (ADR 0037): `TimelineStore` (Ballwechsel) +
/// `SheetStore` (Ereignisse) + BTP-Snapshot (Namen) + `match_times`
/// (Zeiten) + `court_officials` (Schiedsrichter).
///
/// **Datenschutz:** Namen und optional Verein/Nation wandern in den
/// Zettel — das ist sein Zweck. Die Lizenznummer (`BtpPlayer::member_id`)
/// und jedes Geburtsjahr bleiben draußen; sie werden hier nicht einmal
/// gelesen.
///
/// Matches außerhalb des aktuellen Snapshots und solche ohne jede
/// Aufzeichnung liefern **keinen** Zettel — der Abruf endet dann ehrlich
/// mit 404 statt mit einem leeren Blatt.
pub fn dokumente(
    state: &super::state::TabletState,
    display: &crate::config::DisplayConfig,
    match_ids: &[i64],
) -> Vec<SheetDoc> {
    let Some(snap) = state.snapshot_clone() else {
        return Vec::new();
    };
    // Der Verein steht nur auf dem Zettel, wenn er turnierweit
    // zugeschaltet ist — dieselbe Schranke wie auf dem Tablet-Spielzettel.
    // Die Nation bleibt außen vor: Sie hängt an der Monitor-Anzeige, und
    // ein zweiter Schalter für dieselbe Zeile brächte nur Verwirrung.
    let zeige_verein = display.show_club_names;

    match_ids
        .iter()
        .take(relay_proto::MAX_SHEETS_PER_DOC)
        .filter_map(|&id| {
            let m = snap.matches.iter().find(|m| m.id == id)?;
            let timeline = state.timeline_store().timeline(id).unwrap_or_default();
            let sheet = state.sheet_store().sheet(id).unwrap_or_default();
            // Ohne jede Aufzeichnung gibt es nichts zu drucken. Ein
            // Papier-Ergebnis bekommt bewusst keinen Zettel (Nicht-Ziel
            // der Spec) — ein halb ausgefüllter Bogen wäre irreführender
            // als keiner.
            if timeline.sets.iter().all(|s| s.points.is_empty()) && sheet.events.is_empty() {
                return None;
            }

            let doppel = m.team1.len() > 1 || m.team2.len() > 1;
            let zeile = |p: &crate::btp::model::BtpPlayer| SpielerZeile {
                name: p.name.clone(),
                zusatz: if zeige_verein {
                    p.club.clone().unwrap_or_default()
                } else {
                    String::new()
                },
            };

            let zeiten = state.match_times_store().entry(id);
            let (schiedsrichter, service_richter, _) = state.court_officials(Some(m), &snap);
            let halle = m
                .location_id
                .and_then(|lid| snap.locations.iter().find(|l| l.id == lid))
                .map(|l| l.name.clone())
                .unwrap_or_default();

            let start_ms = zeiten
                .as_ref()
                .and_then(|z| z.first_point_ms.or(z.first_assigned_ms));
            let (beginn, ende, dauer) = match &zeiten {
                Some(z) => {
                    let start = z.first_point_ms.or(z.first_assigned_ms);
                    (
                        start.map(uhrzeit).unwrap_or_default(),
                        z.finished_ms.map(uhrzeit).unwrap_or_default(),
                        start
                            .zip(z.finished_ms)
                            .and_then(|(a, b)| super::match_times::plausible_duration_mins(a, b)),
                    )
                }
                None => (String::new(), String::new(), None),
            };

            Some(SheetDoc {
                turnier: snap.tournament_name.clone(),
                disziplin: if m.class_label.is_empty() {
                    m.draw_name.clone()
                } else {
                    format!("{} {}", m.draw_name, m.class_label)
                },
                runde: m.round_name.clone(),
                spielnummer: m.match_num,
                feld: m.court.clone().unwrap_or_default(),
                halle,
                // Das Datum kommt aus dem SPIEL, nicht aus der Uhr beim
                // Drucken: Ein Zettel, der am Folgetag oder nach dem
                // Turnier nachgedruckt wird, trüge sonst ein falsches
                // Datum — auf einem Archivbeleg ein echter Fehler
                // (Review-Befund E5). Ohne Zeitstempel bleibt das Feld
                // leer statt zu raten.
                datum: start_ms.map(datum).unwrap_or_default(),
                beginn,
                ende,
                dauer_min: dauer,
                schiedsrichter,
                service_richter,
                team_a: m.team1.iter().map(zeile).collect(),
                team_b: m.team2.iter().map(zeile).collect(),
                grid: sheet_grid(&timeline, &sheet.events, doppel),
                saetze: m.sets.clone(),
                sieger: match m.winner {
                    Some(1) => m
                        .team1
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    Some(2) => m
                        .team2
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    _ => String::new(),
                },
                ergebnisart: match m.result {
                    crate::btp::model::MatchResult::Normal => "regulär".into(),
                    crate::btp::model::MatchResult::Walkover => "kampflos".into(),
                    crate::btp::model::MatchResult::Retired => "Aufgabe".into(),
                    crate::btp::model::MatchResult::Disqualified => "Disqualifikation".into(),
                },
            })
        })
        .collect()
}

/// Fertiges HTML für die genannten Matches — oder `None`, wenn zu keinem
/// eine Aufzeichnung vorliegt.
///
/// **Der Deckel greift vor der Arbeit:** Mehr als
/// [`relay_proto::MAX_SHEETS_PER_DOC`] Kennungen werden abgewiesen, bevor
/// je Kennung irgendetwas zusammengesucht wird.
pub fn html_fuer(
    state: &super::state::TabletState,
    display: &crate::config::DisplayConfig,
    match_ids: &[i64],
) -> Option<String> {
    if match_ids.is_empty() || match_ids.len() > relay_proto::MAX_SHEETS_PER_DOC {
        return None;
    }
    let docs = dokumente(state, display, match_ids);
    if docs.is_empty() {
        return None;
    }
    Some(render_html(&docs))
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

    /// Doppel **und** Zwischenstand-Einstieg zusammen: Die Startparität
    /// kommt dann aus `start_a`/`start_b` statt aus 0:0 — steht der
    /// Aufschläger bei ungeradem Stand links, verschiebt sich die ganze
    /// Rotation. (Lücke aus dem E4-Review.)
    #[test]
    fn doppel_mit_zwischenstand_startet_mit_richtiger_paritaet() {
        // A steht bei 7 (ungerade) → der Aufschläger steht LINKS, sein
        // Partner rechts; der Empfänger steht diagonal, also ebenfalls
        // links.
        let grid = sheet_grid(
            &tl(vec![satz(7, 5, "AB")]),
            &[aufschlag(1, 0, 0, 1, 0)],
            true,
        );
        assert_eq!(grid.zeilen, 4);
        let z = &grid.blocks[0].zellen;
        // A punktet: derselbe Spieler schlägt weiter auf, Stand 8.
        assert_eq!((z[0].row, z[0].wert), (zeile(0, 0), 8));
        // B punktet → Aufschlagwechsel. B steht danach bei 6 (gerade),
        // es schlägt auf, wer rechts steht. B0 begann links (Diagonale
        // zum linksstehenden Aufschläger), also steht B1 rechts.
        assert_eq!((z[1].row, z[1].wert), (zeile(1, 1), 6));
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

    // ── Renderer ──────────────────────────────────────────────────

    fn doc_mit(name_a: &str, name_b: &str) -> SheetDoc {
        SheetDoc {
            turnier: "Test-Cup".into(),
            disziplin: "Herreneinzel A".into(),
            runde: "Viertelfinale".into(),
            spielnummer: Some(42),
            feld: "3".into(),
            halle: "Halle Süd".into(),
            datum: "19.08.2026".into(),
            beginn: "10:15".into(),
            ende: "10:47".into(),
            dauer_min: Some(32),
            schiedsrichter: vec!["S. Richter".into()],
            service_richter: vec![],
            team_a: vec![SpielerZeile {
                name: name_a.into(),
                zusatz: "SC Musterstadt".into(),
            }],
            team_b: vec![SpielerZeile {
                name: name_b.into(),
                zusatz: String::new(),
            }],
            grid: sheet_grid(&tl(vec![satz(0, 0, "AABBA")]), &[], false),
            saetze: vec![(21, 19), (21, 15)],
            sieger: name_a.into(),
            ergebnisart: "regulär".into(),
        }
    }

    /// ADR 0039: `@page`, kein Skript, keine externe URL. Ohne Skript
    /// bleibt das Dokument auch außerhalb des WebViews harmlos.
    #[test]
    fn dokument_ist_druckfertig_und_selbstgenuegsam() {
        let html = render_html(&[doc_mit("A. Spieler", "B. Gegner")]);
        assert!(html.contains("@page"), "kein @page");
        assert!(html.contains("A4 landscape"), "kein A4 quer");
        assert!(!html.contains("<script"), "Skript im Dokument");
        assert!(!html.contains("http://"), "externe URL");
        assert!(!html.contains("https://"), "externe URL");
        assert!(!html.contains("//cdn"), "externe Ressource");
        assert!(html.contains("Internes Turnier-Archiv"), "Vermerk fehlt");
        assert!(html.contains("page-break-after"), "kein Seitenumbruch");
    }

    /// Das Blatt ist die harte Grenze: Raster, Namensspalte und Abstand
    /// müssen zusammen in die bedruckbare Breite passen. Vorher ergaben
    /// 60 × 4,2 mm plus 42 mm plus 3 mm zusammen 297 mm bei 281 mm Platz —
    /// der Zettel lief über das Blatt hinaus und wirkte dadurch „zu groß".
    #[test]
    fn raster_passt_auf_die_seite() {
        let breite =
            SPALTEN_JE_GRUPPE as f32 * ZELLE_BREITE_MM + NAMENSSPALTE_MM + RASTER_ABSTAND_MM;
        assert!(
            breite <= SEITE_NUTZBAR_MM,
            "Zettel ist {breite} mm breit, das Blatt gibt nur {SEITE_NUTZBAR_MM} mm her"
        );
    }

    /// Name und Zusatz stehen übereinander in einer Zeile fester Höhe.
    /// Passen sie nicht hinein, überlaufen sie ihre Rasterzeile und die
    /// Zuordnung Name ↔ Zeile stimmt optisch nicht mehr.
    #[test]
    fn namen_bleiben_in_ihrer_zeilenhoehe() {
        let gebraucht = (NAME_PT + ZUSATZ_PT) * ZEILENABSTAND * PT_IN_MM;
        assert!(
            gebraucht <= ZEILE_HOEHE_MM,
            "Name + Zusatz brauchen {gebraucht} mm, die Zeile ist nur {ZEILE_HOEHE_MM} mm hoch"
        );
    }

    /// Ein langer Doppelname darf die Namensspalte nicht verbreitern —
    /// sonst schiebt er das Raster vom Blatt. Deshalb feste Breite und
    /// Kürzung statt Umbruch.
    #[test]
    fn lange_namen_sprengen_die_spalte_nicht() {
        let html = render_html(&[doc_mit(
            "Maximilian Hieronymus-Wagner / Jan-Ole Petersen",
            "Christoph Brandenburger / Bo Li",
        )]);
        assert!(
            html.contains(&format!("flex: 0 0 {NAMENSSPALTE_MM}mm")),
            "Namensspalte ist nicht auf feste Breite gelegt"
        );
        assert!(
            html.contains("text-overflow: ellipsis"),
            "kein Kürzen langer Namen"
        );
        assert!(
            html.contains("white-space: nowrap"),
            "Namen dürfen nicht umbrechen"
        );
    }

    /// Erstmals im Projekt entsteht HTML aus BTP-Fremdeingaben. Ein Name
    /// mit Markup darf nie als Markup ankommen.
    #[test]
    fn fremdeingaben_werden_escaped() {
        let mut doc = doc_mit("<script>alert(1)</script>", "O'Brien & \"Sohn\"");
        doc.turnier = "<img src=x onerror=alert(1)>".into();
        doc.halle = "Halle <b>Süd</b>".into();
        let html = render_html(&[doc]);
        assert!(!html.contains("<script>alert"), "Skript durchgerutscht");
        assert!(!html.contains("<img src=x"), "Bild-Tag durchgerutscht");
        assert!(!html.contains("Halle <b>"), "Markup durchgerutscht");
        assert!(html.contains("&lt;script&gt;alert"), "nicht escaped");
        assert!(html.contains("&amp;"), "Ampersand nicht escaped");
    }

    /// Namen erscheinen (das ist der Zweck des Zettels), ein Geburtsjahr
    /// nirgends.
    #[test]
    fn namen_ja_geburtsjahr_nein() {
        let html = render_html(&[doc_mit("A. Spieler", "B. Gegner")]);
        assert!(html.contains("A. Spieler"), "Name fehlt");
        assert!(html.contains("SC Musterstadt"), "Verein fehlt");
        for verboten in ["birth", "Geburt", "geboren", "licence", "Lizenz"] {
            assert!(!html.contains(verboten), "'{verboten}' im Zettel");
        }
    }

    /// Stapeldruck: drei Kennungen ergeben drei Abschnitte; über dem
    /// Deckel wird nicht gedruckt, sondern abgewiesen.
    #[test]
    fn stapel_erzeugt_abschnitte_und_haelt_den_deckel() {
        let drei = vec![doc_mit("A", "B"), doc_mit("C", "D"), doc_mit("E", "F")];
        let html = render_html(&drei);
        assert_eq!(html.matches("<section class=\"zettel\"").count(), 3);

        let zu_viele: Vec<SheetDoc> = (0..relay_proto::MAX_SHEETS_PER_DOC + 5)
            .map(|_| doc_mit("A", "B"))
            .collect();
        let html = render_html(&zu_viele);
        assert_eq!(
            html.matches("<section class=\"zettel\"").count(),
            relay_proto::MAX_SHEETS_PER_DOC,
            "Deckel greift auch im Renderer"
        );
    }

    /// Ein unplausibler Zeitstempel wird zu „—" statt zu Unsinn — `ts_ms`
    /// ist auf dem Draht bewusst ungedeckelt (E1-Review).
    #[test]
    fn unplausible_uhrzeit_wird_zum_gedankenstrich() {
        assert_eq!(uhrzeit(u64::MAX), "—");
        assert_ne!(uhrzeit(1_755_600_000_000), "—");
    }

    /// Das Datum kommt aus dem Spiel, nicht aus der Uhr beim Drucken —
    /// sonst trüge ein Nachdruck nach dem Turnier ein falsches Datum
    /// (Review-Befund E5).
    #[test]
    fn datum_kommt_aus_dem_spiel_nicht_vom_druckzeitpunkt() {
        // 19.08.2026, 10:15 Ortszeit als Zeitstempel — der Test rechnet
        // in derselben Zeitzone wie der Renderer, deshalb Hin- und
        // Rückweg über chrono statt einer festen Zahl.
        use chrono::TimeZone;
        let dann = chrono::Local
            .with_ymd_and_hms(2026, 8, 19, 10, 15, 0)
            .unwrap();
        let ts = dann.timestamp() as u64 * 1000;
        assert_eq!(datum(ts), "19.08.2026");
        assert_eq!(uhrzeit(ts), "10:15");
        // Unplausibel → leer statt Unsinn (das Feld verschwindet dann
        // aus dem Kopf, statt eine Lüge zu drucken).
        assert_eq!(datum(u64::MAX), "");
    }

    /// Zurückgenommenes ist im Protokoll sichtbar durchgestrichen.
    #[test]
    fn zurueckgenommenes_ist_im_dokument_durchgestrichen() {
        let karte = ev("cc", 1, 3, EventKind::CardRed, 0, 0);
        let mut ruecknahme = ev("dd", 1, 3, EventKind::Retract, 0, 0);
        ruecknahme.retracts = "cc".to_string();
        let mut doc = doc_mit("A. Spieler", "B. Gegner");
        doc.grid = sheet_grid(&tl(vec![satz(0, 0, "AABBA")]), &[karte, ruecknahme], false);

        let html = render_html(&[doc]);
        assert!(html.contains("zurueckgenommen"), "keine Streichung");
        assert!(html.contains("Fehler (rot)"), "Art fehlt im Protokoll");
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

// ─────────────────────────── Renderer ─────────────────────────────────

/// Eine Spielerzeile in der Teamspalte links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpielerZeile {
    pub name: String,
    /// Verein oder Nation — nur wenn turnierweit zugeschaltet.
    pub zusatz: String,
}

/// Alles, was ein Zettel braucht. Der Join der Quellen ist zum Zeitpunkt
/// der Erzeugung schon passiert; der Renderer rechnet nichts mehr.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SheetDoc {
    pub turnier: String,
    pub disziplin: String,
    pub runde: String,
    pub spielnummer: Option<i64>,
    pub feld: String,
    pub halle: String,
    pub datum: String,
    pub beginn: String,
    pub ende: String,
    pub dauer_min: Option<i64>,
    pub schiedsrichter: Vec<String>,
    pub service_richter: Vec<String>,
    pub team_a: Vec<SpielerZeile>,
    pub team_b: Vec<SpielerZeile>,
    pub grid: Grid,
    /// Gewertete Satzstände (aus BTP), für den Fuß.
    pub saetze: Vec<(i64, i64)>,
    pub sieger: String,
    pub ergebnisart: String,
}

/// Art im Klartext für die Protokollzeile.
fn art_klartext(art: EventKind) -> &'static str {
    match art {
        EventKind::ServeStart => "Aufschlagfolge",
        EventKind::CardYellow => "Verwarnung (gelb)",
        EventKind::CardRed => "Fehler (rot)",
        EventKind::CardBlack => "Disqualifikation (schwarz)",
        EventKind::InjuryStart => "Behandlung Beginn",
        EventKind::InjuryEnd => "Behandlung Ende",
        EventKind::Suspension => "Unterbrechung",
        EventKind::Overrule => "Überstimmung",
        EventKind::RefereeCall => "Oberschiedsrichter gerufen",
        EventKind::Retired => "Aufgabe",
        EventKind::Disqualified => "Disqualifikation",
        EventKind::Retract => "Rücknahme",
    }
}

fn phase_klartext(phase: Phase) -> &'static str {
    match phase {
        Phase::Play => "",
        Phase::BreakEleven => " (Intervall)",
        Phase::BreakGame => " (Satzpause)",
        Phase::BreakInjury => " (Behandlungspause)",
        Phase::PreMatch => " (vor dem Spiel)",
        Phase::PostMatch => " (nach dem Spiel)",
    }
}

/// Uhrzeit `HH:MM` aus einem Zeitstempel.
///
/// Ein unplausibler Wert wird zu „—" statt zu Unsinn: `ts_ms` ist auf dem
/// Draht bewusst ungedeckelt (ein zu enger Deckel verwürfe ein legitimes
/// spätes Ereignis mitten im Turnier), also fängt der Renderer ihn ab.
fn uhrzeit(ts_ms: u64) -> String {
    zeitstempel(ts_ms, "%H:%M")
}

/// Datum `TT.MM.JJJJ` aus einem Zeitstempel — leer, wenn unplausibel.
fn datum(ts_ms: u64) -> String {
    match zeitstempel(ts_ms, "%d.%m.%Y") {
        s if s == "—" => String::new(),
        s => s,
    }
}

fn zeitstempel(ts_ms: u64, muster: &str) -> String {
    use chrono::TimeZone;
    let sekunden = (ts_ms / 1000) as i64;
    match chrono::Local.timestamp_opt(sekunden, 0) {
        chrono::LocalResult::Single(t) => t.format(muster).to_string(),
        _ => "—".to_string(),
    }
}

/// Höchstzahl Zettel je Dokument, defensiv auch hier — die Route weist
/// zu viele Kennungen schon vor der Arbeit ab.
fn deckel(docs: &[SheetDoc]) -> usize {
    docs.len().min(relay_proto::MAX_SHEETS_PER_DOC)
}

fn e(s: &str) -> String {
    relay_proto::html_escape(s)
}

/// Ein vollständiges, selbstgenügsames HTML-Dokument (ADR 0039).
///
/// **Genau ein Renderer, zwei Aufrufer, null Kopie** — das bewusste
/// Gegenteil von `timelineSetSvg`, das dreifach existiert. Inline-CSS mit
/// `@page`, **kein `<script>`**, keine externe URL: So bleibt das
/// Dokument auch außerhalb des WebViews harmlos und unverändert
/// darstellbar.
///
/// Jeder Fremdtext (Turnier-, Spieler-, Vereinsname aus BTP) läuft durch
/// [`relay_proto::html_escape`]. Das ist die erste Stelle im Projekt, an
/// der aus BTP-Fremdeingaben HTML entsteht.
pub fn render_html(docs: &[SheetDoc]) -> String {
    let anzahl = deckel(docs);
    let mut out = String::with_capacity(8 * 1024);
    out.push_str(
        r#"<!doctype html>
<html lang="de"><head><meta charset="utf-8">
<title>Schiedsrichterzettel</title>
<style>
@page { size: A4 landscape; margin: 8mm; }
* { box-sizing: border-box; }
body { font: 10pt/1.25 "Helvetica Neue", Arial, sans-serif; color: #000; background: #fff; margin: 0; }
section.zettel { page-break-after: always; }
section.zettel:last-child { page-break-after: auto; }
.kopf { display: flex; justify-content: space-between; align-items: flex-start; border-bottom: 1.5pt solid #000; padding-bottom: 2mm; margin-bottom: 3mm; }
.kopf h1 { font-size: 13pt; margin: 0 0 1mm; }
.kopf dl { display: grid; grid-template-columns: auto auto; gap: 0 3mm; margin: 0; font-size: 9pt; }
.kopf dt { font-weight: 600; }
.kopf dd { margin: 0; }
.vermerk { border: 1pt solid #000; padding: 1mm 2mm; font-size: 8pt; text-transform: uppercase; letter-spacing: .04em; white-space: nowrap; }
table.gitter { border-collapse: collapse; table-layout: fixed; }
table.gitter th { background: #eee; font-weight: 600; font-size: 6.5pt; }
td.marker { font-weight: 700; }
.satzkopf { font-weight: 600; font-size: 9pt; margin: 2mm 0 1mm; }
.randmarker { font-size: 8pt; margin-top: 1mm; }
table.protokoll { width: 100%; border-collapse: collapse; font-size: 8.5pt; margin-top: 2mm; }
table.protokoll th, table.protokoll td { border-bottom: .4pt solid #999; padding: .8mm 1mm; text-align: left; }
tr.zurueckgenommen td { text-decoration: line-through; color: #555; }
.fuss { display: flex; justify-content: space-between; align-items: flex-end; margin-top: 4mm; border-top: 1pt solid #000; padding-top: 2mm; font-size: 9pt; }
.unterschriften { display: flex; gap: 8mm; }
.unterschrift { width: 55mm; border-top: .5pt solid #000; padding-top: 1mm; font-size: 8pt; }
.hinweis { font-size: 8pt; font-style: italic; margin: 1mm 0; }
.erzeugt { font-size: 7.5pt; color: #444; }
"#,
    );

    // Maßabhängige Regeln aus den Blattmaß-Konstanten oben — sie bilden
    // zusammen das Breitenbudget, das `raster_passt_auf_die_seite` prüft.
    // Die Namensspalte ist bewusst **fest** (`flex: 0 0`) statt `min-width`:
    // Ein langer Doppelname darf sich nicht auf Kosten des Rasters breit
    // machen. Was dann nicht hineinpasst, wird gekürzt — sichtbar durch
    // Auslassungspunkte statt still über den Blattrand geschoben.
    out.push_str(&format!(
        r#".raster {{ display: flex; gap: {RASTER_ABSTAND_MM}mm; margin-bottom: 3mm; page-break-inside: avoid; }}
.teamspalte {{ flex: 0 0 {NAMENSSPALTE_MM}mm; max-width: {NAMENSSPALTE_MM}mm; overflow: hidden; }}
.teamspalte .zeile {{ height: {ZEILE_HOEHE_MM}mm; display: flex; flex-direction: column; justify-content: center; border-bottom: .5pt solid #999; overflow: hidden; }}
.teamspalte .name {{ font-weight: 600; font-size: {NAME_PT}pt; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
.teamspalte .zusatz {{ font-size: {ZUSATZ_PT}pt; color: #333; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
table.gitter td, table.gitter th {{ border: .4pt solid #666; width: {ZELLE_BREITE_MM}mm; height: {ZEILE_HOEHE_MM}mm; text-align: center; font-size: 7.5pt; padding: 0; }}
</style></head><body>
"#
    ));

    for doc in docs.iter().take(anzahl) {
        out.push_str("<section class=\"zettel\">\n");

        // ── Kopf ──
        out.push_str("<header class=\"kopf\"><div>");
        out.push_str(&format!("<h1>{}</h1>", e(&doc.turnier)));
        out.push_str("<dl>");
        let mut kopfzeile = |k: &str, v: &str| {
            if !v.is_empty() {
                out.push_str(&format!("<dt>{}</dt><dd>{}</dd>", e(k), e(v)));
            }
        };
        kopfzeile("Disziplin", &doc.disziplin);
        kopfzeile("Runde", &doc.runde);
        kopfzeile("Feld", &doc.feld);
        kopfzeile("Halle", &doc.halle);
        kopfzeile("Datum", &doc.datum);
        kopfzeile("Beginn", &doc.beginn);
        kopfzeile("Ende", &doc.ende);
        kopfzeile("Schiedsrichter", &doc.schiedsrichter.join(", "));
        kopfzeile("Service-Richter", &doc.service_richter.join(", "));
        if let Some(nr) = doc.spielnummer {
            out.push_str(&format!("<dt>Spiel-Nr.</dt><dd>{nr}</dd>"));
        }
        if let Some(min) = doc.dauer_min {
            out.push_str(&format!("<dt>Dauer</dt><dd>{min} min</dd>"));
        }
        out.push_str("</dl></div>");
        out.push_str(
            "<div class=\"vermerk\">Internes Turnier-Archiv — kein amtlicher Beleg</div></header>\n",
        );

        if doc.grid.aufschlagfolge_fehlt {
            out.push_str("<p class=\"hinweis\">Aufschlagfolge nicht aufgezeichnet — das Raster zeigt je Mannschaft eine Zeile.</p>\n");
        }

        // ── Raster je Satz ──
        for block in &doc.grid.blocks {
            out.push_str(&format!(
                "<div class=\"satzkopf\">Satz {} — Endstand {}:{}</div>\n",
                block.satz, block.end_a, block.end_b
            ));
            out.push_str("<div class=\"raster\"><div class=\"teamspalte\">");
            let zeilen: Vec<SpielerZeile> = if doc.grid.zeilen == 4 {
                doc.team_a
                    .iter()
                    .chain(doc.team_b.iter())
                    .cloned()
                    .collect()
            } else {
                // Zwei Zeilen (Einzel oder Degradation): je Mannschaft eine
                // Zeile. Der Zusatz (Verein/Nation) darf dabei NICHT
                // wegfallen — im Einzel wäre er sonst nie zu sehen.
                [&doc.team_a, &doc.team_b]
                    .into_iter()
                    .map(|team| SpielerZeile {
                        name: team
                            .iter()
                            .map(|s| s.name.clone())
                            .collect::<Vec<_>>()
                            .join(" / "),
                        zusatz: team
                            .iter()
                            .map(|s| s.zusatz.clone())
                            .filter(|z| !z.is_empty())
                            .collect::<Vec<_>>()
                            .join(" / "),
                    })
                    .collect()
            };
            for z in &zeilen {
                out.push_str(&format!(
                    "<div class=\"zeile\"><span class=\"name\">{}</span>",
                    e(&z.name)
                ));
                if !z.zusatz.is_empty() {
                    out.push_str(&format!("<span class=\"zusatz\">{}</span>", e(&z.zusatz)));
                }
                out.push_str("</div>");
            }
            out.push_str("</div><div>");

            // Zeilengruppen: 1–60, dann 61–120 („Fortsetzung").
            let gruppen = block
                .zellen
                .iter()
                .map(|z| z.gruppe)
                .max()
                .map(|g| g + 1)
                .unwrap_or(1);
            for gruppe in 0..gruppen {
                out.push_str("<table class=\"gitter\"><tr>");
                for i in 0..SPALTEN_JE_GRUPPE {
                    out.push_str(&format!("<th>{}</th>", gruppe * SPALTEN_JE_GRUPPE + i + 1));
                }
                out.push_str("</tr>");
                for row in 0..doc.grid.zeilen {
                    out.push_str("<tr>");
                    for i in 0..SPALTEN_JE_GRUPPE {
                        let col = gruppe * SPALTEN_JE_GRUPPE + i + 1;
                        match block
                            .zellen
                            .iter()
                            .find(|z| z.row == row && z.col == col && z.gruppe == gruppe)
                        {
                            Some(z) => match z.marker {
                                Some(m) => out.push_str(&format!(
                                    "<td class=\"marker\">{}<sup>{}</sup></td>",
                                    z.wert, m
                                )),
                                None => out.push_str(&format!("<td>{}</td>", z.wert)),
                            },
                            None => out.push_str("<td></td>"),
                        }
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
            out.push_str("</div></div>");

            if !block.rand_marker.is_empty() {
                out.push_str("<div class=\"randmarker\">Ohne Ballwechsel: ");
                let teile: Vec<String> = block
                    .rand_marker
                    .iter()
                    .map(|m| format!("{} nach Ballwechsel {}", m.marker, m.nach_ballwechsel))
                    .collect();
                out.push_str(&e(&teile.join(" · ")));
                out.push_str("</div>");
            }
        }

        // ── Protokoll ──
        if !doc.grid.protokoll.is_empty() {
            out.push_str("<table class=\"protokoll\"><tr><th>Nr.</th><th>Uhrzeit</th><th>Satz</th><th>Stand</th><th>Art</th><th>Spieler</th></tr>");
            for z in &doc.grid.protokoll {
                let klasse = if z.zurueckgenommen {
                    " class=\"zurueckgenommen\""
                } else {
                    ""
                };
                let mannschaft = if z.team == 0 { "A" } else { "B" };
                let spieler = match doc.grid.zeilen {
                    4 => format!("{mannschaft}{}", z.player + 1),
                    _ => mannschaft.to_string(),
                };
                out.push_str(&format!(
                    "<tr{klasse}><td>{}</td><td>{}</td><td>{}</td><td>{}:{}</td><td>{}{}</td><td>{}</td></tr>",
                    z.nr,
                    e(&uhrzeit(z.ts_ms)),
                    z.satz,
                    z.score_a,
                    z.score_b,
                    e(art_klartext(z.art)),
                    e(phase_klartext(z.phase)),
                    e(&spieler),
                ));
            }
            out.push_str("</table>");
        }

        // ── Fuß ──
        out.push_str("<footer class=\"fuss\"><div>");
        if !doc.saetze.is_empty() {
            let staende: Vec<String> = doc.saetze.iter().map(|(a, b)| format!("{a}:{b}")).collect();
            out.push_str(&format!("<div>Endstand: {}</div>", e(&staende.join(" · "))));
        }
        if !doc.sieger.is_empty() {
            out.push_str(&format!("<div>Sieger: {}</div>", e(&doc.sieger)));
        }
        if !doc.ergebnisart.is_empty() {
            out.push_str(&format!("<div>Ergebnisart: {}</div>", e(&doc.ergebnisart)));
        }
        out.push_str("</div><div class=\"unterschriften\">");
        out.push_str(
            "<div class=\"unterschrift\">Schiedsrichter (ohne rechtliche Bedeutung)</div>",
        );
        out.push_str(
            "<div class=\"unterschrift\">Turnierleitung (ohne rechtliche Bedeutung)</div>",
        );
        out.push_str("</div></footer>\n");
        out.push_str("</section>\n");
    }

    out.push_str("</body></html>");
    out
}
