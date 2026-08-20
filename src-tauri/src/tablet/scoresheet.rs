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

// ── Blattmaße ────────────────────────────────────────────────────────────
//
// Sie liegen seit dem Umbau auf den DBV-Bogen (ADR 0043) in
// [`super::blatt`] — zusammen mit den beiden Budget-Tests, die sie
// zusammenhalten. Hier bleibt nur, was die **Projektion** braucht: wie
// viele Ballwechsel in einen Block passen.
pub use super::blatt::BALLWECHSEL_JE_BLOCK;

/// Eine gefüllte Rasterzelle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zelle {
    /// `2 * team + player` im Doppel; `team` im degradierten Fall.
    pub row: usize,
    /// Ballwechsel-Nummer im Satz, **1-basiert**.
    pub col: usize,
    /// Wievielter Block **innerhalb** des Satzes, 0-basiert. Ein Satz
    /// beginnt in einem neuen Block und läuft nach
    /// [`BALLWECHSEL_JE_BLOCK`] Ballwechseln im nächsten weiter.
    pub gruppe: usize,
    /// Neuer Punktstand der Seite, die gleich aufschlägt.
    pub wert: i64,
    /// Druckbar in Schwarz-Weiß, Zeichen des DBV-Bogens (ADR 0043):
    /// `W` Warnung, `F` Fault (rote Karte), `R` Referee gerufen,
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
    /// Zeile des Aufschlägers zu Satzbeginn — trägt auf dem Blatt die
    /// Marke „A" und den Startstand. `None`, wenn die Aufschlagfolge nicht
    /// aufgezeichnet wurde.
    pub aufschlag_row: Option<usize>,
    /// Zeile des Rückschlägers, Marke „R".
    pub rueckschlag_row: Option<usize>,
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

/// Zeichen für die Zelle, in der Konvention des DBV-Bogens (ADR 0043).
/// `None` = erscheint nur im Protokoll.
///
/// `W` Warnung (gelbe Karte) · `F` Fault (rote Karte) · `R` Referee
/// gerufen · `D` Disqualifikation. Vorher standen dort die hausgemachten
/// `V`/`F`/`D`; Schiedsrichter erkennen jetzt ihre gewohnten Zeichen.
fn marker_fuer(art: EventKind) -> Option<char> {
    match art {
        EventKind::CardYellow => Some('W'),
        EventKind::CardRed => Some('F'),
        EventKind::RefereeCall => Some('R'),
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
        let start = wirksam
            .iter()
            .find(|e| e.kind == EventKind::ServeStart && e.set == nr)
            .filter(|_| !aufschlagfolge_fehlt);
        // Marken „A" und „R" des Bogens: Wer eröffnet, wer nimmt an. Ohne
        // aufgezeichnete Aufschlagfolge bleiben beide leer — geraten wird
        // nicht.
        let zeile_von = |team: i64, spieler: i64| {
            (2 * team.clamp(0, 1)
                + if volles_raster {
                    spieler.clamp(0, 1)
                } else {
                    0
                }) as usize
        };
        let (aufschlag_row, rueckschlag_row) = match start {
            Some(e) => (
                Some(zeile_von(e.team, e.player)),
                Some(zeile_von(e.receiver_team, e.receiver_player)),
            ),
            None => (None, None),
        };
        let mut geo = start
            .map(|e| Aufschlag::neu(e, punkte, volles_raster))
            .unwrap_or_else(Aufschlag::degradiert);

        // Die Zeile ist **immer** die Zeile des Blatts (0–3): `2 * team +
        // player` im vollen Raster, `2 * team` bei fehlender
        // Aufschlagfolge. So brauchen Raster, Namen und Marken keine
        // zweite Umrechnung — im Einzel bleiben die Zeilen 1 und 3 leer,
        // genau wie auf dem Papierbogen.
        let mut zellen = Vec::new();
        for (n, c) in satz.points.chars().enumerate() {
            let sieger: i64 = if c == 'A' { 0 } else { 1 };
            punkte[sieger as usize] += 1;
            let (team, spieler) = geo.nach_ballwechsel(sieger, punkte, volles_raster);
            let col = n + 1;
            zellen.push(Zelle {
                row: (2 * team + if volles_raster { spieler } else { 0 }) as usize,
                col,
                gruppe: (col - 1) / BALLWECHSEL_JE_BLOCK,
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
            let row = (2 * e.team.clamp(0, 1)
                + if volles_raster {
                    e.player.clamp(0, 1)
                } else {
                    0
                }) as usize;
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
            aufschlag_row,
            rueckschlag_row,
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
/// **Datenschutz:** Namen und Verein wandern in den Zettel — das ist sein
/// Zweck. Die Lizenznummer (`BtpPlayer::member_id`) und jedes Geburtsjahr
/// bleiben draußen; sie werden hier nicht einmal gelesen.
///
/// Matches außerhalb des aktuellen Snapshots und solche ohne jede
/// Aufzeichnung liefern **keinen** Zettel — der Abruf endet dann ehrlich
/// mit 404 statt mit einem leeren Blatt.
pub fn dokumente(
    state: &super::state::TabletState,
    logo_uri: Option<&str>,
    match_ids: &[i64],
) -> Vec<SheetDoc> {
    let Some(snap) = state.snapshot_clone() else {
        return Vec::new();
    };
    // Der Verein steht **immer** auf dem Zettel, wenn BTP ihn kennt —
    // bewusste Ausnahme vom Schalter `show_club_names` (ADR 0043): Der
    // Bogen hat eine vorgedruckte Vereinszeile, und der Verein steht
    // ohnehin auf Aushang und Meldeliste. Die Nation bleibt außen vor;
    // sie hängt an der Monitor-Anzeige.

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
                zusatz: p.club.clone().unwrap_or_default(),
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
                logo_vorhanden: logo_uri.is_some(),
                logo_uri: logo_uri.unwrap_or_default().to_string(),
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
    logo_uri: Option<&str>,
    match_ids: &[i64],
) -> Option<String> {
    if match_ids.is_empty() || match_ids.len() > relay_proto::MAX_SHEETS_PER_DOC {
        return None;
    }
    let docs = dokumente(state, logo_uri, match_ids);
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
        //
        // **Die Zeile ist die Zeile des Blatts** (ADR 0043): Der Bogen hat
        // immer vier, im Einzel stehen die Spieler in Zeile 0 und 2 und
        // die Zeilen 1 und 3 bleiben leer. Vorher zählte hier `team`
        // (0/1) — das hätte auf dem Papier zwei Spieler untereinander
        // gequetscht.
        let werte: Vec<(usize, i64)> = b.zellen.iter().map(|z| (z.row, z.wert)).collect();
        assert_eq!(werte, vec![(0, 1), (0, 2), (2, 1), (2, 2), (0, 3)]);
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
        // Auch degradiert sind es die Blattzeilen 0 und 2 — je Mannschaft
        // eine, mit einer Leerzeile darunter.
        let werte: Vec<(usize, i64)> = grid.blocks[0]
            .zellen
            .iter()
            .map(|z| (z.row, z.wert))
            .collect();
        assert_eq!(werte, vec![(0, 1), (0, 2), (2, 1), (2, 2), (0, 3)]);
        assert!(
            grid.blocks[0].aufschlag_row.is_none(),
            "ohne Aufschlagfolge wird keine Marke geraten"
        );
    }

    /// Die Zeichen des Bogens statt der hausgemachten (ADR 0043):
    /// W Warnung, F Fault, R Referee gerufen, D Disqualifikation.
    #[test]
    fn marker_folgen_der_dbv_konvention() {
        assert_eq!(marker_fuer(EventKind::CardYellow), Some('W'));
        assert_eq!(marker_fuer(EventKind::CardRed), Some('F'));
        assert_eq!(marker_fuer(EventKind::RefereeCall), Some('R'));
        assert_eq!(marker_fuer(EventKind::CardBlack), Some('D'));
        assert_eq!(marker_fuer(EventKind::Disqualified), Some('D'));
        // Was keine Zelle bekommt, steht nur im Protokoll.
        assert_eq!(marker_fuer(EventKind::InjuryStart), None);
        assert_eq!(marker_fuer(EventKind::ServeStart), None);
    }

    /// Der Bogen markiert den Satzbeginn mit „A" und „R" — die Zeilen
    /// dafür kommen aus dem `serve_start`, nicht aus einer Vermutung.
    #[test]
    fn aufschlag_und_rueckschlag_marken_stehen_fest() {
        // Doppel: A1 schlägt auf, B0 nimmt an.
        let grid = sheet_grid(
            &tl(vec![satz(0, 0, "AAB")]),
            &[aufschlag(1, 0, 1, 1, 0)],
            true,
        );
        assert_eq!(grid.blocks[0].aufschlag_row, Some(zeile(0, 1)));
        assert_eq!(grid.blocks[0].rueckschlag_row, Some(zeile(1, 0)));
    }

    /// Das Logo landet in einem `src`-Attribut. Nur bekannte Rasterformate
    /// und sauberes Base64 kommen durch — `image/svg+xml` kann Skript
    /// tragen und bleibt deshalb draußen.
    #[test]
    fn logo_nimmt_nur_saubere_bilder_an() {
        let logo = |mime: &str, data: &str| crate::config::LogoConfig {
            mime: mime.into(),
            data: data.into(),
            background_color: String::new(),
        };
        assert_eq!(
            logo_data_uri(&logo("image/png", "aGVsbG8=")).as_deref(),
            Some("data:image/png;base64,aGVsbG8=")
        );
        assert!(logo_data_uri(&logo("image/svg+xml", "aGVsbG8=")).is_none());
        assert!(logo_data_uri(&logo("image/png\" onerror=x", "aGVsbG8=")).is_none());
        assert!(logo_data_uri(&logo("image/png", "\"><script>")).is_none());
        assert!(logo_data_uri(&logo("image/png", "")).is_none());
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

    /// Ein Satz läuft nach 32 Ballwechseln im nächsten Block weiter — der
    /// Bogen hat 33 Spalten, von denen die erste den Startstand trägt.
    #[test]
    fn umbruch_ab_ballwechsel_33() {
        let punkte: String = std::iter::repeat_n("AB", 35).collect(); // 70
        let grid = sheet_grid(&tl(vec![satz(0, 0, &punkte)]), &[], false);
        let z = &grid.blocks[0].zellen;
        assert_eq!(z[31].col, 32);
        assert_eq!(z[31].gruppe, 0);
        assert_eq!(z[32].col, 33);
        assert_eq!(z[32].gruppe, 1, "ab 33 der zweite Block");
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
            logo_vorhanden: false,
            logo_uri: String::new(),
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
        assert!(html.contains("page-break-after"), "kein Seitenumbruch");
        // Der Archiv-Vermerk ist mit dem DBV-Blatt zurückgenommen
        // (ADR 0043) — er passt nicht auf einen Bogen, der während des
        // Spiels geführt wird.
        assert!(
            !html.contains("Internes Turnier-Archiv"),
            "Archiv-Vermerk steht noch im Dokument"
        );
    }

    /// Ein langer Doppelname darf die Namensspalte nicht verbreitern —
    /// sonst schiebt er das Raster vom Blatt. Die Breite selbst prüft
    /// `blatt::lange_namen_werden_gekuerzt`; hier geht es darum, dass der
    /// HTML-Treiber die Kürzung auch tatsächlich anweist.
    #[test]
    fn lange_namen_sprengen_die_spalte_nicht() {
        let html = render_html(&[doc_mit(
            "Maximilian Hieronymus-Wagner / Jan-Ole Petersen",
            "Christoph Brandenburger / Bo Li",
        )]);
        assert!(
            html.contains("text-overflow: ellipsis"),
            "kein Kürzen langer Namen"
        );
        assert!(
            html.contains("white-space: nowrap"),
            "Namen dürfen nicht umbrechen"
        );
        assert!(
            html.contains("class=\"t k"),
            "Namenskasten nicht als kürzbar"
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

    /// Stapeldruck: drei Kennungen ergeben drei Seiten; über dem Deckel
    /// wird nicht gedruckt, sondern abgewiesen. Jeder dieser Zettel hat
    /// genau eine Seite — Vorkommnisse gibt es hier keine.
    #[test]
    fn stapel_erzeugt_abschnitte_und_haelt_den_deckel() {
        let drei = vec![doc_mit("A", "B"), doc_mit("C", "D"), doc_mit("E", "F")];
        let html = render_html(&drei);
        assert_eq!(html.matches("<section class=\"blatt").count(), 3);

        let zu_viele: Vec<SheetDoc> = (0..relay_proto::MAX_SHEETS_PER_DOC + 5)
            .map(|_| doc_mit("A", "B"))
            .collect();
        let html = render_html(&zu_viele);
        assert_eq!(
            html.matches("<section class=\"blatt").count(),
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

    /// Zurückgenommenes verschwindet nicht spurlos (ADR 0038): Es steht im
    /// Protokoll auf der Anhangseite, ausdrücklich als zurückgenommen
    /// bezeichnet — und **nicht** im Raster.
    #[test]
    fn zurueckgenommenes_ist_im_dokument_vermerkt() {
        let karte = ev("cc", 1, 3, EventKind::CardRed, 0, 0);
        let mut ruecknahme = ev("dd", 1, 3, EventKind::Retract, 0, 0);
        ruecknahme.retracts = "cc".to_string();
        let mut doc = doc_mit("A. Spieler", "B. Gegner");
        doc.grid = sheet_grid(&tl(vec![satz(0, 0, "AABBA")]), &[karte, ruecknahme], false);

        let html = render_html(&[doc]);
        assert!(html.contains("zurückgenommen"), "keine Streichung");
        assert!(html.contains("Fehler (rot)"), "Art fehlt im Protokoll");
        assert!(html.contains("Vorkommnisse"), "keine Anhangseite");
    }

    /// Werkzeug statt Prüfung: schreibt ein Musterblatt nach
    /// `target/musterblatt.html`, damit sich das Seitenbild im Browser
    /// begutachten und probedrucken lässt.
    ///
    /// `cargo test --lib musterblatt -- --ignored --nocapture`
    #[test]
    #[ignore = "erzeugt ein Musterblatt zum Ansehen, prüft nichts"]
    fn musterblatt_schreiben() {
        // Beispieldaten in der Anmutung des Blankobogens — erfundene
        // Namen, keine echten Personen.
        let mut karte = ev("k1", 1, 12, EventKind::CardYellow, 1, 1);
        karte.ts_ms = 1_755_600_000_000;
        let mut ruf = ev("k2", 2, 5, EventKind::RefereeCall, 0, 0);
        ruf.ts_ms = 1_755_601_000_000;

        let punkte_1: String = std::iter::repeat_n("AABB", 9).collect(); // 36
        let punkte_2: String = std::iter::repeat_n("ABBA", 8).collect(); // 32
        let doc = SheetDoc {
            turnier: "Jux-Turnier 2026".into(),
            disziplin: "Herrendoppel A".into(),
            runde: "Viertelfinale".into(),
            spielnummer: Some(111),
            feld: "1".into(),
            halle: "Halle Süd".into(),
            datum: "20.08.2026".into(),
            beginn: "13:00".into(),
            ende: "13:44".into(),
            dauer_min: Some(44),
            schiedsrichter: vec!["Tom Schiedsrichter".into()],
            service_richter: vec!["Jerry Aufschlagrichter".into()],
            team_a: vec![
                SpielerZeile {
                    name: "Becker, Heinz".into(),
                    zusatz: "SC Musterstadt".into(),
                },
                SpielerZeile {
                    name: "Meier, Kurt".into(),
                    zusatz: "SC Musterstadt".into(),
                },
            ],
            team_b: vec![
                SpielerZeile {
                    name: "Krause, Dieter".into(),
                    zusatz: "TV Beispielheim".into(),
                },
                SpielerZeile {
                    name: "Müller, Herbert".into(),
                    zusatz: "TV Beispielheim".into(),
                },
            ],
            grid: sheet_grid(
                &tl(vec![
                    satz(0, 0, &punkte_1),
                    satz(0, 0, &punkte_2),
                    satz(0, 0, "AABBA"),
                ]),
                &[
                    aufschlag(1, 0, 0, 1, 0),
                    aufschlag(2, 1, 0, 0, 1),
                    aufschlag(3, 0, 1, 1, 1),
                    karte,
                    ruf,
                ],
                true,
            ),
            saetze: vec![(21, 15), (18, 21), (21, 19)],
            sieger: "Becker, Heinz / Meier, Kurt".into(),
            ergebnisart: "regulär".into(),
            logo_vorhanden: false,
            logo_uri: String::new(),
        };
        let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("target")
            .join("musterblatt.html");
        std::fs::write(&pfad, render_html(&[doc])).expect("Musterblatt schreiben");
        println!("Musterblatt: {}", pfad.display());
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
    /// Ist ein Turnierlogo hinterlegt? Der Kopf hält den Platz dafür frei;
    /// ohne Logo bleibt er leer, die Kopfhöhe ändert sich nicht (ADR 0043).
    pub logo_vorhanden: bool,
    /// Das Logo als `data:`-URI — nur der HTML-Treiber nutzt es.
    pub logo_uri: String,
}

/// Art im Klartext für die Protokollzeile.
pub(crate) fn art_klartext(art: EventKind) -> &'static str {
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

pub(crate) fn phase_klartext(phase: Phase) -> &'static str {
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
pub(crate) fn uhrzeit(ts_ms: u64) -> String {
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

/// Das hinterlegte Turnierlogo als `data:`-URI — oder `None`.
///
/// **Riegel gegen Fremdeingaben:** Der MIME-Typ kommt aus der
/// Konfiguration (Upload in den Einstellungen) und landet in einem
/// `src`-Attribut. Erlaubt sind deshalb nur vier bekannte Rasterformate;
/// alles andere — insbesondere `image/svg+xml`, das Skript tragen kann —
/// wird abgewiesen, ebenso alles, was nicht dem Base64-Alphabet folgt.
pub fn logo_data_uri(logo: &crate::config::LogoConfig) -> Option<String> {
    const ERLAUBT: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];
    let mime = logo.mime.trim();
    if !ERLAUBT.contains(&mime) || logo.data.is_empty() {
        return None;
    }
    let sauber = logo
        .data
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=');
    if !sauber {
        return None;
    }
    Some(format!("data:{};base64,{}", mime, logo.data))
}

/// Ein vollständiges, selbstgenügsames HTML-Dokument (ADR 0039/0042).
///
/// **Genau ein Layout, zwei Treiber, null Kopie:** Gerechnet wird das
/// Blatt in [`super::blatt`]; hier wird die Elementliste nur noch in
/// absolut positioniertes HTML übersetzt. Der Druck-Treiber (GDI) fährt
/// dieselbe Liste ab — deshalb kann das Seitenbild zwischen Bildschirm und
/// Papier nicht auseinanderlaufen.
///
/// Inline-CSS mit `@page`, **kein `<script>`**, keine externe URL: So
/// bleibt das Dokument auch außerhalb des WebViews harmlos.
///
/// Jeder Fremdtext (Turnier-, Spieler-, Vereinsname aus BTP) läuft durch
/// [`relay_proto::html_escape`].
pub fn render_html(docs: &[SheetDoc]) -> String {
    use super::blatt::{Ausrichtung, Element, Seite, SEITE_BREITE_MM, SEITE_HOEHE_MM};

    let anzahl = deckel(docs);
    let mut out = String::with_capacity(32 * 1024);
    out.push_str(
        r#"<!doctype html>
<html lang="de"><head><meta charset="utf-8">
<title>Schiedsrichterzettel</title>
<style>
@page { size: A4 landscape; margin: 0; }
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; background: #fff; color: #000; }
body { font-family: "Helvetica Neue", Arial, sans-serif; }
section.blatt { position: relative; overflow: hidden; page-break-after: always; break-after: page; }
section.blatt.letzte { page-break-after: auto; break-after: auto; }
.li { position: absolute; background: #000; }
.fl { position: absolute; }
.ra { position: absolute; border-style: solid; border-color: #000; }
.t { position: absolute; display: flex; align-items: center; overflow: hidden; line-height: 1.05; }
.t > span { display: block; width: 100%; overflow: hidden; white-space: nowrap; }
.t.k > span { text-overflow: ellipsis; }
.t.mitte > span { text-align: center; }
.t.rechts > span { text-align: right; }
.t.fett { font-weight: 700; }
img.logo { position: absolute; object-fit: contain; }
</style></head><body>
"#,
    );

    // Alle Seiten aller Zettel hintereinander — ein Druckauftrag.
    let mut seiten: Vec<(&SheetDoc, Seite)> = Vec::new();
    for doc in docs.iter().take(anzahl) {
        for seite in super::blatt::blatt(doc) {
            seiten.push((doc, seite));
        }
    }
    let letzte = seiten.len().saturating_sub(1);

    for (i, (doc, seite)) in seiten.iter().enumerate() {
        out.push_str(&format!(
            "<section class=\"blatt{}\" style=\"width:{SEITE_BREITE_MM}mm;height:{SEITE_HOEHE_MM}mm\">",
            if i == letzte { " letzte" } else { "" }
        ));
        for el in &seite.elemente {
            match el {
                Element::Linie {
                    x1,
                    y1,
                    x2,
                    y2,
                    staerke_mm,
                } => {
                    let breite = (x2 - x1).abs().max(*staerke_mm);
                    let hoehe = (y2 - y1).abs().max(*staerke_mm);
                    out.push_str(&format!(
                        "<div class=\"li\" style=\"left:{:.2}mm;top:{:.2}mm;width:{breite:.2}mm;height:{hoehe:.2}mm\"></div>",
                        x1.min(*x2),
                        y1.min(*y2)
                    ));
                }
                Element::Flaeche {
                    x,
                    y,
                    breite,
                    hoehe,
                    grau,
                } => out.push_str(&format!(
                    "<div class=\"fl\" style=\"left:{x:.2}mm;top:{y:.2}mm;width:{breite:.2}mm;height:{hoehe:.2}mm;background:rgb({grau},{grau},{grau})\"></div>"
                )),
                Element::Rahmen {
                    x,
                    y,
                    breite,
                    hoehe,
                    staerke_mm,
                } => out.push_str(&format!(
                    "<div class=\"ra\" style=\"left:{x:.2}mm;top:{y:.2}mm;width:{breite:.2}mm;height:{hoehe:.2}mm;border-width:{staerke_mm:.2}mm\"></div>"
                )),
                Element::Text(t) => {
                    let mut klassen = String::from("t");
                    if t.kuerzen {
                        klassen.push_str(" k");
                    }
                    match t.ausrichtung {
                        Ausrichtung::Mitte => klassen.push_str(" mitte"),
                        Ausrichtung::Rechts => klassen.push_str(" rechts"),
                        Ausrichtung::Links => {}
                    }
                    if t.fett {
                        klassen.push_str(" fett");
                    }
                    out.push_str(&format!(
                        "<div class=\"{klassen}\" style=\"left:{:.2}mm;top:{:.2}mm;width:{:.2}mm;height:{:.2}mm;font-size:{:.2}pt\"><span>{}</span></div>",
                        t.x,
                        t.y,
                        t.breite,
                        t.hoehe,
                        t.groesse_pt,
                        e(&t.text)
                    ));
                }
                Element::Logo {
                    x,
                    y,
                    breite,
                    hoehe,
                } => {
                    if !doc.logo_uri.is_empty() {
                        out.push_str(&format!(
                            "<img class=\"logo\" alt=\"\" src=\"{}\" style=\"left:{x:.2}mm;top:{y:.2}mm;width:{breite:.2}mm;height:{hoehe:.2}mm\">",
                            e(&doc.logo_uri)
                        ));
                    }
                }
            }
        }
        out.push_str("</section>\n");
    }

    out.push_str("</body></html>");
    out
}
