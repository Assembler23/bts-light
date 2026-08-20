//! Das Blatt des Schiedsrichterzettels — Layout nach dem DBV-Bogen
//! (ADR 0043) als **Elementliste in Millimetern** (ADR 0042).
//!
//! Hier wird gerechnet, nicht gezeichnet: [`blatt`] liefert Seiten aus
//! Linien, Flächen, Rahmen und Textkästen. Zwei Treiber geben dieselbe
//! Liste aus — HTML für Bildschirm und Handdruck
//! ([`super::scoresheet::render_html`]) und später GDI für den stillen
//! Druck. **Die Seitenaufteilung gehört hierher**, nicht in die Treiber;
//! sonst zählten beide unterschiedlich.
//!
//! Der Gewinn: Das Blatt ist ein reiner Wert und damit prüfbar, **ohne zu
//! drucken**. Genau daran ist v0.9.246 gescheitert, als das Raster 16 mm
//! über den Blattrand lief und das erst am Papier auffiel.

use super::scoresheet::{Grid, SheetDoc};

// ── Blattmaße ───────────────────────────────────────────────────────────
//
// Alle Werte sind aus dem DBV-Blankobogen vermessen (PDF, A4 quer). Sie
// stehen bewusst als Konstanten und **nicht** im CSS: Sie bilden zwei
// Budgets, die aufgehen müssen — in der Breite und, neu gegenüber dem
// alten Zettel, auch in der Höhe.

/// A4 quer.
pub const SEITE_BREITE_MM: f32 = 297.0;
pub const SEITE_HOEHE_MM: f32 = 210.0;
/// Rand ringsum. Das Vorbild nutzt schmale Ränder; darunter passt das
/// Raster nicht in die Höhe.
pub const RAND_MM: f32 = 5.0;

/// Namensspalte links. Fest — ein langer Doppelname wird gekürzt, statt
/// das Raster vom Blatt zu schieben.
pub const NAMENSSPALTE_MM: f32 = 40.0;
/// Schmale Spalte für die Marken „A" (Aufschläger) und „R" (Rückschläger).
pub const AR_SPALTE_MM: f32 = 9.5;
/// Breite einer Rasterzelle.
pub const ZELLE_BREITE_MM: f32 = 6.41;
/// Spalten je Block — **inklusive** der Startstand-Spalte ganz links.
pub const SPALTEN_JE_BLOCK: usize = 33;
/// Ballwechsel je Block: eine Spalte geht für den Startstand ab.
pub const BALLWECHSEL_JE_BLOCK: usize = SPALTEN_JE_BLOCK - 1;
/// Breitere Schlussspalte rechts für den Satz-Endstand.
pub const ENDSPALTE_MM: f32 = 14.0;
/// Höhe einer Rasterzeile.
pub const ZEILE_HOEHE_MM: f32 = 5.27;
/// Zeilen je Block — vier, auch im Einzel (dann bleiben zwei leer).
pub const ZEILEN_JE_BLOCK: usize = 4;
/// Luft zwischen zwei Blöcken.
pub const BLOCK_ABSTAND_MM: f32 = 2.12;
/// Blöcke je Seite.
pub const BLOECKE_JE_SEITE: usize = 6;
/// Kopfhöhe der ersten Seite.
pub const KOPF_HOEHE_MM: f32 = 50.0;
/// Verkürzter Kopf ab der zweiten Seite.
pub const KOPF_FOLGE_MM: f32 = 12.0;
/// Fußhöhe (Unterschriften, Erzeugungsvermerk).
pub const FUSS_HOEHE_MM: f32 = 8.0;

/// Gesamtbreite des Rasters.
pub const RASTER_BREITE_MM: f32 =
    NAMENSSPALTE_MM + AR_SPALTE_MM + SPALTEN_JE_BLOCK as f32 * ZELLE_BREITE_MM + ENDSPALTE_MM;
/// Höhe eines Blocks samt Abstand zum nächsten.
pub const BLOCK_RASTER_MM: f32 = ZEILEN_JE_BLOCK as f32 * ZEILE_HOEHE_MM + BLOCK_ABSTAND_MM;
/// Nutzbare Breite und Höhe zwischen den Rändern.
pub const NUTZBAR_BREITE_MM: f32 = SEITE_BREITE_MM - 2.0 * RAND_MM;
pub const NUTZBAR_HOEHE_MM: f32 = SEITE_HOEHE_MM - 2.0 * RAND_MM;
/// Linker Rasterrand — das Raster sitzt mittig auf dem Blatt.
pub const RASTER_X_MM: f32 = (SEITE_BREITE_MM - RASTER_BREITE_MM) / 2.0;

// Die beiden Budgets sind **Kompilierbedingungen**, nicht bloß Tests: Wer
// eine Maßzahl anfasst, ohne dass die Summe aufgeht, bekommt keinen Build
// statt eines Zettels, der über den Blattrand läuft (v0.9.246).
const _: () = assert!(
    RASTER_BREITE_MM <= NUTZBAR_BREITE_MM,
    "Raster ist breiter als das Blatt"
);
const _: () = assert!(
    KOPF_HOEHE_MM + BLOECKE_JE_SEITE as f32 * BLOCK_RASTER_MM + FUSS_HOEHE_MM <= NUTZBAR_HOEHE_MM,
    "Kopf + sechs Blöcke + Fuß sind höher als das Blatt"
);

// Schriftgrade.
const TITEL_PT: f32 = 15.0;
const NAME_PT: f32 = 8.5;
const ZUSATZ_PT: f32 = 6.5;
const LABEL_PT: f32 = 7.0;
const WERT_PT: f32 = 8.5;
const ZELLE_PT: f32 = 7.0;
const MARKER_PT: f32 = 5.0;
const KLEIN_PT: f32 = 6.5;

// Strichstärken.
const STRICH_FEIN_MM: f32 = 0.2;
const STRICH_MM: f32 = 0.35;
const STRICH_STARK_MM: f32 = 0.6;

/// Grauwert der Team-B-Zeilen (0 = schwarz, 255 = weiß).
const GRAU_TEAM_B: u8 = 0xE6;

// ── Elemente ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ausrichtung {
    Links,
    Mitte,
    Rechts,
}

/// Ein Textkasten. Der Treiber setzt den Text **in** den Kasten: waagerecht
/// nach [`Ausrichtung`], senkrecht mittig. Damit braucht keiner der beiden
/// Treiber eine eigene Grundlinien-Rechnung.
#[derive(Debug, Clone, PartialEq)]
pub struct TextKasten {
    pub x: f32,
    pub y: f32,
    pub breite: f32,
    pub hoehe: f32,
    pub text: String,
    pub groesse_pt: f32,
    pub fett: bool,
    pub ausrichtung: Ausrichtung,
    /// Zu langer Text wird gekürzt (Auslassungspunkte), nie überlaufen.
    pub kuerzen: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Linie {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        staerke_mm: f32,
    },
    /// Gefüllte Fläche ohne Rand (graue Zeilen).
    Flaeche {
        x: f32,
        y: f32,
        breite: f32,
        hoehe: f32,
        grau: u8,
    },
    Rahmen {
        x: f32,
        y: f32,
        breite: f32,
        hoehe: f32,
        staerke_mm: f32,
    },
    Text(TextKasten),
    /// Turnierlogo. **Optional** — fehlt es oder kann der Treiber es nicht
    /// laden, bleibt der Platz leer und das Blatt gilt unverändert.
    Logo {
        x: f32,
        y: f32,
        breite: f32,
        hoehe: f32,
    },
}

impl Element {
    /// Umschließendes Rechteck — die Grundlage des Wächter-Tests, dass
    /// nichts über das Blatt hinausläuft.
    pub fn rechteck(&self) -> (f32, f32, f32, f32) {
        match self {
            Element::Linie { x1, y1, x2, y2, .. } => {
                (x1.min(*x2), y1.min(*y2), x1.max(*x2), y1.max(*y2))
            }
            Element::Flaeche {
                x,
                y,
                breite,
                hoehe,
                ..
            }
            | Element::Rahmen {
                x,
                y,
                breite,
                hoehe,
                ..
            }
            | Element::Logo {
                x,
                y,
                breite,
                hoehe,
            } => (*x, *y, x + breite, y + hoehe),
            Element::Text(t) => (t.x, t.y, t.x + t.breite, t.y + t.hoehe),
        }
    }
}

/// Eine Druckseite.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seite {
    pub elemente: Vec<Element>,
}

impl Seite {
    fn linie_h(&mut self, x: f32, y: f32, breite: f32, staerke_mm: f32) {
        self.elemente.push(Element::Linie {
            x1: x,
            y1: y,
            x2: x + breite,
            y2: y,
            staerke_mm,
        });
    }

    fn linie_v(&mut self, x: f32, y: f32, hoehe: f32, staerke_mm: f32) {
        self.elemente.push(Element::Linie {
            x1: x,
            y1: y,
            x2: x,
            y2: y + hoehe,
            staerke_mm,
        });
    }

    fn rahmen(&mut self, x: f32, y: f32, breite: f32, hoehe: f32) {
        self.elemente.push(Element::Rahmen {
            x,
            y,
            breite,
            hoehe,
            staerke_mm: STRICH_MM,
        });
    }

    fn text(&mut self, t: TextKasten) {
        if t.text.is_empty() {
            return;
        }
        self.elemente.push(Element::Text(t));
    }
}

/// Kurzform für einen Textkasten mit den üblichen Vorgaben.
fn tk(x: f32, y: f32, breite: f32, hoehe: f32, text: impl Into<String>) -> TextKasten {
    TextKasten {
        x,
        y,
        breite,
        hoehe,
        text: text.into(),
        groesse_pt: WERT_PT,
        fett: false,
        ausrichtung: Ausrichtung::Links,
        kuerzen: true,
    }
}

impl TextKasten {
    fn pt(mut self, pt: f32) -> Self {
        self.groesse_pt = pt;
        self
    }
    fn fett(mut self) -> Self {
        self.fett = true;
        self
    }
    fn mitte(mut self) -> Self {
        self.ausrichtung = Ausrichtung::Mitte;
        self
    }
    fn rechts(mut self) -> Self {
        self.ausrichtung = Ausrichtung::Rechts;
        self
    }
}

// ── Spaltenachsen des Rasters ───────────────────────────────────────────

/// Linke Kante der A/R-Spalte.
pub const AR_X_MM: f32 = RASTER_X_MM + NAMENSSPALTE_MM;
/// Linke Kante der ersten Rasterzelle (Startstand-Spalte).
pub const ZELLEN_X_MM: f32 = AR_X_MM + AR_SPALTE_MM;
/// Linke Kante der Schlussspalte.
pub const ENDSPALTE_X_MM: f32 = ZELLEN_X_MM + SPALTEN_JE_BLOCK as f32 * ZELLE_BREITE_MM;

/// Linke Kante der Spalte `i` (0 = Startstand, 1..=32 = Ballwechsel).
fn spalte_x(i: usize) -> f32 {
    ZELLEN_X_MM + i as f32 * ZELLE_BREITE_MM
}

// ── Ein Block auf dem Blatt ─────────────────────────────────────────────

/// Ein Rasterblock: ein Ausschnitt von höchstens
/// [`BALLWECHSEL_JE_BLOCK`] Ballwechseln **eines** Satzes.
///
/// Ein Satz beginnt immer in einem neuen Block; ist er länger, läuft er im
/// nächsten weiter. Genau dafür hat das Vorbild sechs Blöcke.
#[derive(Debug, Clone, PartialEq)]
struct BlockPlan {
    /// Index in `grid.blocks` (= Satz).
    satz_index: usize,
    /// Wievielter Block **innerhalb** des Satzes (0-basiert).
    lauf: usize,
    /// Erster Ballwechsel dieses Blocks, 1-basiert.
    von: usize,
    /// Letzter möglicher Ballwechsel dieses Blocks, 1-basiert.
    bis: usize,
}

/// Blockfolge eines Zettels planen.
fn blockfolge(grid: &Grid) -> Vec<BlockPlan> {
    let mut plan = Vec::new();
    for (i, block) in grid.blocks.iter().enumerate() {
        let ballwechsel = block.zellen.iter().map(|z| z.col).max().unwrap_or(0);
        // Auch ein Satz ohne Ballwechsel bekommt seinen Block — sonst
        // fehlte dem Vorabzettel jede Zeile zum Schreiben.
        let bloecke = ballwechsel.div_ceil(BALLWECHSEL_JE_BLOCK).max(1);
        for lauf in 0..bloecke {
            plan.push(BlockPlan {
                satz_index: i,
                lauf,
                von: lauf * BALLWECHSEL_JE_BLOCK + 1,
                bis: (lauf + 1) * BALLWECHSEL_JE_BLOCK,
            });
        }
    }
    plan
}

// ── Das Blatt bauen ─────────────────────────────────────────────────────

/// Die Seiten eines Zettels rechnen.
///
/// Seite 1 trägt den vollen Kopf, Folgeseiten einen verkürzten. Gibt es
/// Vorkommnisse (Karten, Behandlungen, Rücknahmen), folgt eine
/// **Anhangseite** mit dem Protokoll — das Vorbild kennt keine
/// Protokolltabelle, und auf den Rasterseiten ist dafür kein Millimeter
/// frei. Ein Vorabzettel hat nie Vorkommnisse und damit nie eine
/// Anhangseite.
pub fn blatt(doc: &SheetDoc) -> Vec<Seite> {
    let plan = blockfolge(&doc.grid);
    let mut seiten: Vec<Seite> = Vec::new();

    for (seiten_nr, teil) in plan.chunks(BLOECKE_JE_SEITE).enumerate() {
        let mut seite = Seite::default();
        let erste = seiten_nr == 0;
        let kopf_hoehe = if erste { KOPF_HOEHE_MM } else { KOPF_FOLGE_MM };
        if erste {
            kopf_voll(&mut seite, doc);
        } else {
            kopf_kurz(&mut seite, doc, seiten_nr + 1);
        }
        let raster_y = RAND_MM + kopf_hoehe;
        for (i, bp) in teil.iter().enumerate() {
            block_zeichnen(&mut seite, doc, bp, raster_y + i as f32 * BLOCK_RASTER_MM);
        }
        seiten.push(seite);
    }

    if seiten.is_empty() {
        // Ein Zettel ohne jeden Satz-Block — theoretisch möglich, wenn die
        // Aufzeichnung leer ist. Der Kopf allein ist immer noch ein Blatt.
        let mut seite = Seite::default();
        kopf_voll(&mut seite, doc);
        seiten.push(seite);
    }

    if let Some(letzte) = seiten.last_mut() {
        fuss(letzte, doc);
    }

    if !doc.grid.protokoll.is_empty() {
        seiten.push(anhang(doc));
    }
    seiten
}

/// Voller Kopf: Logo, Turniername, Spielangaben, beide Mannschaftskästen,
/// Satzergebnis und die Besetzung.
fn kopf_voll(seite: &mut Seite, doc: &SheetDoc) {
    let x0 = RASTER_X_MM;
    let y0 = RAND_MM;

    // Kopfzeile: Logo links, Turniername mittig. **Nie** Wort- oder
    // Bildmarke eines Verbands (ADR 0043) — das Turnier zeichnet für sein
    // eigenes Blatt.
    if doc.logo_vorhanden {
        seite.elemente.push(Element::Logo {
            x: x0,
            y: y0,
            breite: 26.0,
            hoehe: 16.0,
        });
    }
    seite.text(
        tk(
            x0 + 30.0,
            y0 + 2.0,
            RASTER_BREITE_MM - 60.0,
            10.0,
            doc.turnier.clone(),
        )
        .pt(TITEL_PT)
        .fett()
        .mitte(),
    );

    let y = y0 + 22.0;
    let zeile = 6.5_f32;

    // Die fünf Kopfspalten bilden ein Budget wie das Raster darunter:
    // Spielangaben · Kasten A (mit Marke L links) · Satzergebnis ·
    // Kasten B (mit Marke R rechts) · Besetzung. Die Marken brauchen je
    // 4,5 mm Luft neben ihrem Kasten.
    const KASTEN_BREITE: f32 = 66.0;
    const TEAM_A_X: f32 = 44.0;
    const SATZ_KASTEN_X: f32 = 114.0;
    const TEAM_B_X: f32 = 150.0;
    const BESETZUNG_X: f32 = 224.0;

    // Spalte 1 — Spielangaben.
    // Die Wertspalte endet bei 39 mm, nicht bei 40: Bei 40 stieße sie mit
    // der Marke „L" des Mannschaftskastens zusammen, die links daneben
    // hängt.
    let mut angabe = |i: usize, label: &str, wert: &str| {
        let yy = y + i as f32 * zeile;
        seite.text(tk(x0, yy, 15.0, zeile, label).pt(LABEL_PT));
        seite.text(tk(x0 + 15.0, yy, 24.0, zeile, wert).fett());
    };
    angabe(
        0,
        "Spiel Nr",
        &doc.spielnummer.map(|n| n.to_string()).unwrap_or_default(),
    );
    angabe(1, "Disziplin", &doc.disziplin);
    angabe(2, "Feld Nr", &doc.feld);
    angabe(3, "Datum", &doc.datum);

    // Mannschaftskästen mit den Marken „L" und „R" wie im Vorbild: L
    // links vom linken Kasten, R rechts vom rechten.
    mannschaft(
        seite,
        x0 + TEAM_A_X,
        y,
        &doc.team_a,
        "L",
        false,
        KASTEN_BREITE,
    );
    mannschaft(
        seite,
        x0 + TEAM_B_X,
        y,
        &doc.team_b,
        "R",
        true,
        KASTEN_BREITE,
    );

    // Satzergebnis.
    let sx = x0 + SATZ_KASTEN_X;
    seite.text(
        tk(sx, y - 5.0, 32.0, 5.0, "Satzergebnis")
            .pt(LABEL_PT)
            .mitte(),
    );
    seite.rahmen(sx, y, 32.0, 3.0 * 7.0);
    for i in 0..3 {
        let yy = y + i as f32 * 7.0;
        if i > 0 {
            seite.linie_h(sx, yy, 32.0, STRICH_FEIN_MM);
        }
        seite.text(tk(sx + 1.0, yy, 5.0, 7.0, (i + 1).to_string()).pt(LABEL_PT));
        if let Some((a, b)) = doc.saetze.get(i) {
            seite.text(tk(sx + 6.0, yy, 10.0, 7.0, a.to_string()).rechts().fett());
            seite.text(tk(sx + 16.0, yy, 3.0, 7.0, ":").mitte().pt(LABEL_PT));
            seite.text(tk(sx + 19.0, yy, 10.0, 7.0, b.to_string()).fett());
        }
    }

    // Rechte Spalte — Besetzung und Zeiten.
    let rx = x0 + BESETZUNG_X;
    let rw = RASTER_BREITE_MM - BESETZUNG_X;
    // Vier Zeilen, nicht fünf: Beginn und Ende teilen sich eine Zeile wie
    // im Vorbild. Mit fünf ragte die Spalte 4,5 mm in den ersten
    // Rasterblock hinein.
    // `spalte`: 0 = volle Breite, 1 = linke Hälfte, 2 = rechte Hälfte.
    let mut rechts = |i: usize, label: &str, wert: &str, spalte: u8| {
        let yy = y + i as f32 * zeile;
        let (x, lb, wb) = match spalte {
            1 => (rx, 12.0, rw / 2.0 - 13.0),
            2 => (rx + rw / 2.0, 10.0, rw / 2.0 - 11.0),
            _ => (rx, 24.0, rw - 24.0),
        };
        seite.text(tk(x, yy, lb, zeile, label).pt(LABEL_PT));
        seite.text(tk(x + lb, yy, wb, zeile, wert).fett());
        seite.linie_h(x + lb, yy + zeile - 0.8, wb, STRICH_FEIN_MM);
    };
    rechts(0, "Schiedsrichter", &doc.schiedsrichter.join(", "), 0);
    rechts(1, "Aufschlagrichter", &doc.service_richter.join(", "), 0);
    rechts(2, "Beginn", &doc.beginn, 1);
    rechts(2, "Ende", &doc.ende, 2);
    rechts(
        3,
        "Dauer",
        &doc.dauer_min
            .map(|m| format!("{m} Min."))
            .unwrap_or_default(),
        0,
    );
}

/// Ein Mannschaftskasten: zwei Namenszeilen und eine Vereinszeile.
///
/// Die Vereinszeile steht **immer** — auch leer. Sie ist im Vorbild
/// vorgedruckt und auf einem handgeführten Blatt beschreibbar.
fn mannschaft(
    seite: &mut Seite,
    x: f32,
    y: f32,
    team: &[super::scoresheet::SpielerZeile],
    marke: &str,
    marke_rechts: bool,
    breite: f32,
) {
    let zeile = 7.0;
    seite.rahmen(x, y, breite, 3.0 * zeile);
    let marke_x = if marke_rechts {
        x + breite + 0.5
    } else {
        x - 4.5
    };
    seite.text(
        tk(marke_x, y, 4.0, zeile, marke)
            .pt(LABEL_PT)
            .fett()
            .mitte(),
    );
    for i in 0..2 {
        let yy = y + i as f32 * zeile;
        if i > 0 {
            seite.linie_h(x, yy, breite, STRICH_FEIN_MM);
        }
        if let Some(s) = team.get(i) {
            seite.text(tk(x + 1.5, yy, breite - 3.0, zeile, s.name.clone()).pt(NAME_PT + 1.0));
        }
    }
    let yy = y + 2.0 * zeile;
    seite.linie_h(x, yy, breite, STRICH_FEIN_MM);
    let verein = team
        .iter()
        .map(|s| s.zusatz.clone())
        .filter(|z| !z.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    let text = if verein.is_empty() {
        "(Verein)".to_string()
    } else {
        verein
    };
    seite.text(tk(x + 1.5, yy, breite - 3.0, zeile, text).pt(ZUSATZ_PT));
}

/// Verkürzter Kopf der Folgeseiten.
fn kopf_kurz(seite: &mut Seite, doc: &SheetDoc, nr: usize) {
    let x0 = RASTER_X_MM;
    let y0 = RAND_MM;
    let namen = |team: &[super::scoresheet::SpielerZeile]| {
        team.iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
            .join(" / ")
    };
    seite.text(
        tk(
            x0,
            y0,
            RASTER_BREITE_MM * 0.7,
            6.0,
            format!(
                "{} — {} / {}",
                doc.turnier,
                namen(&doc.team_a),
                namen(&doc.team_b)
            ),
        )
        .pt(NAME_PT)
        .fett(),
    );
    let mut rechts_text = Vec::new();
    if let Some(n) = doc.spielnummer {
        rechts_text.push(format!("Spiel {n}"));
    }
    rechts_text.push(format!("Seite {nr}"));
    seite.text(
        tk(
            x0 + RASTER_BREITE_MM * 0.7,
            y0,
            RASTER_BREITE_MM * 0.3,
            6.0,
            rechts_text.join(" · "),
        )
        .pt(LABEL_PT)
        .rechts(),
    );
    seite.linie_h(x0, y0 + 7.0, RASTER_BREITE_MM, STRICH_MM);
}

/// Einen Rasterblock zeichnen.
fn block_zeichnen(seite: &mut Seite, doc: &SheetDoc, bp: &BlockPlan, y: f32) {
    let x0 = RASTER_X_MM;
    let hoehe = ZEILEN_JE_BLOCK as f32 * ZEILE_HOEHE_MM;
    let Some(satz) = doc.grid.blocks.get(bp.satz_index) else {
        return;
    };

    // Team B liegt grau hinterlegt — wie im Vorbild, damit die Seiten auch
    // in Schwarz-Weiß auseinanderzuhalten sind.
    seite.elemente.push(Element::Flaeche {
        x: x0,
        y: y + 2.0 * ZEILE_HOEHE_MM,
        breite: RASTER_BREITE_MM,
        hoehe: 2.0 * ZEILE_HOEHE_MM,
        grau: GRAU_TEAM_B,
    });

    seite.elemente.push(Element::Rahmen {
        x: x0,
        y,
        breite: RASTER_BREITE_MM,
        hoehe,
        staerke_mm: STRICH_STARK_MM,
    });
    for i in 1..ZEILEN_JE_BLOCK {
        seite.linie_h(
            x0,
            y + i as f32 * ZEILE_HOEHE_MM,
            RASTER_BREITE_MM,
            STRICH_FEIN_MM,
        );
    }
    seite.linie_v(AR_X_MM, y, hoehe, STRICH_MM);
    seite.linie_v(ZELLEN_X_MM, y, hoehe, STRICH_MM);
    for i in 1..SPALTEN_JE_BLOCK {
        seite.linie_v(spalte_x(i), y, hoehe, STRICH_FEIN_MM);
    }
    seite.linie_v(ENDSPALTE_X_MM, y, hoehe, STRICH_MM);

    // Namen — im Einzel stehen sie in Zeile 0 und 2, damit die Zuordnung
    // `2 * team + player` auf dem Papier dieselbe bleibt wie im Raster.
    for (idx, s) in doc.team_a.iter().chain(doc.team_b.iter()).enumerate() {
        let row = zeile_fuer(&doc.grid, idx, doc.team_a.len());
        if row >= ZEILEN_JE_BLOCK {
            continue;
        }
        seite.text(
            tk(
                x0 + 1.2,
                y + row as f32 * ZEILE_HOEHE_MM,
                NAMENSSPALTE_MM - 2.4,
                ZEILE_HOEHE_MM,
                s.name.clone(),
            )
            .pt(NAME_PT),
        );
    }

    // Nur der erste Block eines Satzes trägt A/R-Marken und Startstände.
    if bp.lauf == 0 {
        if let Some(row) = satz.aufschlag_row {
            marke_und_start(seite, y, row, "A", satz.start_a, satz.start_b, true);
        }
        if let Some(row) = satz.rueckschlag_row {
            marke_und_start(seite, y, row, "R", satz.start_a, satz.start_b, false);
        }
    }

    // Zellwerte.
    for z in satz
        .zellen
        .iter()
        .filter(|z| z.col >= bp.von && z.col <= bp.bis)
    {
        let spalte = z.col - bp.von + 1;
        let row = z.row.min(ZEILEN_JE_BLOCK - 1);
        let x = spalte_x(spalte);
        let yy = y + row as f32 * ZEILE_HOEHE_MM;
        seite.text(
            tk(x, yy, ZELLE_BREITE_MM, ZEILE_HOEHE_MM, z.wert.to_string())
                .pt(ZELLE_PT)
                .mitte(),
        );
        if let Some(m) = z.marker {
            seite.text(
                tk(
                    x,
                    yy,
                    ZELLE_BREITE_MM - 0.3,
                    ZELLE_BREITE_MM * 0.5,
                    m.to_string(),
                )
                .pt(MARKER_PT)
                .fett()
                .rechts(),
            );
        }
    }

    // Endstand in der Schlussspalte des letzten Blocks eines Satzes.
    let letzter = satz
        .zellen
        .iter()
        .map(|z| z.col)
        .max()
        .unwrap_or(0)
        .div_ceil(BALLWECHSEL_JE_BLOCK)
        .max(1)
        - 1;
    if bp.lauf == letzter && !satz.zellen.is_empty() {
        seite.text(
            tk(
                ENDSPALTE_X_MM,
                y,
                ENDSPALTE_MM,
                ZEILE_HOEHE_MM,
                satz.end_a.to_string(),
            )
            .pt(ZELLE_PT)
            .fett()
            .mitte(),
        );
        seite.text(
            tk(
                ENDSPALTE_X_MM,
                y + 2.0 * ZEILE_HOEHE_MM,
                ENDSPALTE_MM,
                ZEILE_HOEHE_MM,
                satz.end_b.to_string(),
            )
            .pt(ZELLE_PT)
            .fett()
            .mitte(),
        );
    }
}

/// A/R-Marke samt Startstand in der Startspalte.
fn marke_und_start(
    seite: &mut Seite,
    y: f32,
    row: usize,
    marke: &str,
    start_a: i64,
    start_b: i64,
    aufschlag: bool,
) {
    if row >= ZEILEN_JE_BLOCK {
        return;
    }
    let yy = y + row as f32 * ZEILE_HOEHE_MM;
    seite.text(
        tk(AR_X_MM, yy, AR_SPALTE_MM, ZEILE_HOEHE_MM, marke)
            .pt(ZELLE_PT)
            .fett()
            .mitte(),
    );
    // Der Startstand steht in der Zeile dessen, dem er gehört: Zeile 0/1
    // ist Mannschaft A, Zeile 2/3 Mannschaft B.
    let wert = if row < 2 { start_a } else { start_b };
    let _ = aufschlag;
    seite.text(
        tk(
            spalte_x(0),
            yy,
            ZELLE_BREITE_MM,
            ZEILE_HOEHE_MM,
            wert.to_string(),
        )
        .pt(ZELLE_PT)
        .mitte(),
    );
}

/// Zeile eines Spielers im Block: `2 * team + player`, im Einzel `2 * team`.
fn zeile_fuer(grid: &Grid, index: usize, team_a_len: usize) -> usize {
    if grid.zeilen == 4 {
        if index < team_a_len {
            index
        } else {
            2 + (index - team_a_len)
        }
    } else if index < team_a_len {
        0
    } else {
        2
    }
}

/// Fußzeile: Unterschriften und Erzeugungsvermerk.
fn fuss(seite: &mut Seite, doc: &SheetDoc) {
    let x0 = RASTER_X_MM;
    let y = SEITE_HOEHE_MM - RAND_MM - FUSS_HOEHE_MM;
    // Zwei Unterschriftslinien und drei Lücken à 20 mm teilen sich die
    // Rasterbreite — sonst schiebt die rechte Linie das Blatt auf.
    let luecke = 20.0;
    let breite = (RASTER_BREITE_MM - 3.0 * luecke) / 2.0;
    for (i, label) in ["Schiedsrichter", "Referee"].iter().enumerate() {
        let x = x0 + luecke + i as f32 * (breite + luecke);
        seite.linie_h(x, y + 4.0, breite, STRICH_FEIN_MM);
        seite.text(tk(x, y + 4.2, breite, 3.5, *label).pt(KLEIN_PT).mitte());
    }
    let mut vermerk = format!("bts-light {}", env!("CARGO_PKG_VERSION"));
    if !doc.ergebnisart.is_empty() && doc.ergebnisart != "regulär" {
        vermerk = format!("{} · {}", doc.ergebnisart, vermerk);
    }
    // Der Vermerk steht **über** den Unterschriftszeilen, nicht auf ihrer
    // Höhe: Er läuft über die volle Breite und würde ihre Beschriftung
    // sonst überschreiben, sobald er länger wird.
    seite.text(
        tk(x0, y + 0.2, RASTER_BREITE_MM, 3.5, vermerk)
            .pt(KLEIN_PT)
            .rechts(),
    );
}

/// Anhangseite mit dem Protokoll der Vorkommnisse.
fn anhang(doc: &SheetDoc) -> Seite {
    let mut seite = Seite::default();
    kopf_kurz(&mut seite, doc, 0);
    let x0 = RASTER_X_MM;
    let mut y = RAND_MM + 12.0;
    seite.text(
        tk(x0, y, RASTER_BREITE_MM, 6.0, "Vorkommnisse")
            .pt(NAME_PT + 1.5)
            .fett(),
    );
    y += 7.0;

    let spalten: [(f32, &str); 6] = [
        (10.0, "Nr."),
        (18.0, "Uhrzeit"),
        (12.0, "Satz"),
        (18.0, "Stand"),
        (90.0, "Art"),
        (20.0, "Spieler"),
    ];
    let mut x = x0;
    for (breite, titel) in spalten {
        seite.text(tk(x, y, breite, 5.0, titel).pt(LABEL_PT).fett());
        x += breite;
    }
    seite.linie_h(x0, y + 5.0, x - x0, STRICH_MM);
    y += 5.5;

    for z in &doc.grid.protokoll {
        let mut x = x0;
        let mannschaft = if z.team == 0 { "A" } else { "B" };
        let spieler = match doc.grid.zeilen {
            4 => format!("{mannschaft}{}", z.player + 1),
            _ => mannschaft.to_string(),
        };
        let art = format!(
            "{}{}{}",
            super::scoresheet::art_klartext(z.art),
            super::scoresheet::phase_klartext(z.phase),
            if z.zurueckgenommen {
                " — zurückgenommen"
            } else {
                ""
            }
        );
        let werte = [
            z.nr.to_string(),
            super::scoresheet::uhrzeit(z.ts_ms),
            z.satz.to_string(),
            format!("{}:{}", z.score_a, z.score_b),
            art,
            spieler,
        ];
        for (i, (breite, _)) in spalten.iter().enumerate() {
            seite.text(tk(x, y, *breite, 5.0, werte[i].clone()).pt(KLEIN_PT));
            x += breite;
        }
        seite.linie_h(x0, y + 5.0, x - x0, STRICH_FEIN_MM);
        y += 5.2;
        if y > SEITE_HOEHE_MM - RAND_MM - 10.0 {
            break;
        }
    }

    // Ereignisse ohne Trägerballwechsel (Karte in der Satzpause) stehen
    // ausschließlich hier — im Raster hätten sie keine eigene Zelle.
    let rand: Vec<String> = doc
        .grid
        .blocks
        .iter()
        .flat_map(|b| {
            b.rand_marker.iter().map(move |m| {
                format!(
                    "Satz {}: {} nach Ballwechsel {}",
                    b.satz, m.marker, m.nach_ballwechsel
                )
            })
        })
        .collect();
    if !rand.is_empty() {
        seite.text(
            tk(
                x0,
                y + 3.0,
                RASTER_BREITE_MM,
                5.0,
                format!("Ohne Ballwechsel: {}", rand.join(" · ")),
            )
            .pt(KLEIN_PT),
        );
    }
    seite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tablet::scoresheet::{Grid, SatzBlock, SheetDoc, SpielerZeile, Zelle};

    fn spieler(name: &str) -> SpielerZeile {
        SpielerZeile {
            name: name.to_string(),
            zusatz: String::new(),
        }
    }

    fn zellen(anzahl: usize) -> Vec<Zelle> {
        (1..=anzahl)
            .map(|col| Zelle {
                row: (col % 4),
                col,
                gruppe: (col - 1) / BALLWECHSEL_JE_BLOCK,
                wert: col as i64,
                marker: None,
            })
            .collect()
    }

    fn satz(nr: i64, ballwechsel: usize) -> SatzBlock {
        SatzBlock {
            satz: nr,
            start_a: 0,
            start_b: 0,
            end_a: 21,
            end_b: 19,
            zellen: zellen(ballwechsel),
            rand_marker: Vec::new(),
            aufschlag_row: Some(0),
            rueckschlag_row: Some(2),
        }
    }

    fn doc(saetze: Vec<SatzBlock>) -> SheetDoc {
        SheetDoc {
            turnier: "Jux-Turnier".into(),
            disziplin: "HD".into(),
            spielnummer: Some(111),
            feld: "1".into(),
            team_a: vec![spieler("Becker, Heinz"), spieler("Meier, Kurt")],
            team_b: vec![spieler("Krause, Dieter"), spieler("Müller, Herbert")],
            grid: Grid {
                zeilen: 4,
                blocks: saetze,
                aufschlagfolge_fehlt: false,
                protokoll: Vec::new(),
            },
            ..Default::default()
        }
    }

    /// Ein Zettel, dessen erste Seite alle sechs Blöcke trägt.
    fn volles_blatt() -> Vec<Seite> {
        blatt(&doc(vec![satz(1, 40), satz(2, 40), satz(3, 40)]))
    }

    /// Größte Ausdehnung aller Elemente einer Seite (rechts, unten).
    fn ausdehnung(seite: &Seite) -> (f32, f32) {
        seite
            .elemente
            .iter()
            .map(|e| e.rechteck())
            .fold((0.0_f32, 0.0_f32), |(bx, by), (_, _, x2, y2)| {
                (bx.max(x2), by.max(y2))
            })
    }

    fn texte(seite: &Seite) -> Vec<String> {
        seite
            .elemente
            .iter()
            .filter_map(|e| match e {
                Element::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Das Breitenbudget des Vorbilds: Namensspalte, A/R-Spalte,
    /// 33 Zellen und Schlussspalte müssen nebeneinander aufs Blatt.
    #[test]
    fn breitenbudget_geht_auf() {
        let (breit, _) = ausdehnung(&volles_blatt()[0]);
        assert!(
            breit <= SEITE_BREITE_MM - RAND_MM,
            "Blatt ist bis {breit} mm bedruckt, erlaubt sind {} mm",
            SEITE_BREITE_MM - RAND_MM
        );
    }

    /// **Die Achse, an der v0.9.246 gescheitert ist — diesmal die Höhe.**
    /// Sechs Blöcke plus Kopf und Fuß füllen das Blatt fast aus. Gemessen
    /// wird am **erzeugten** Blatt; die reine Maßrechnung hält schon der
    /// `const`-Assert oben fest.
    #[test]
    fn blatt_passt_in_die_hoehe() {
        let seiten = volles_blatt();
        assert_eq!(
            blockfolge(&doc(vec![satz(1, 40), satz(2, 40), satz(3, 40)]).grid).len(),
            BLOECKE_JE_SEITE,
            "der Test muss eine wirklich volle Seite messen"
        );
        let (_, hoch) = ausdehnung(&seiten[0]);
        assert!(
            hoch <= SEITE_HOEHE_MM - RAND_MM,
            "Blatt ist bis {hoch} mm bedruckt, erlaubt sind {} mm",
            SEITE_HOEHE_MM - RAND_MM
        );
    }

    /// **Kein Textkasten darf einen anderen überdecken.** Der Kopf des
    /// Bogens ist ein Puzzle aus fünf Spalten, zwei Kästen und einer
    /// Marke; beim Bauen dieses Layouts kollidierten die Spielangaben mit
    /// der Marke „L", die Marke „R" mit dem Satzergebnis-Kasten und die
    /// Besetzungsspalte mit dem ersten Rasterblock. Nichts davon wäre in
    /// einer Maßrechnung aufgefallen — auf Papier aber sofort.
    #[test]
    fn textkaesten_ueberdecken_sich_nicht() {
        let seiten = volles_blatt();
        let kaesten: Vec<&TextKasten> = seiten[0]
            .elemente
            .iter()
            .filter_map(|e| match e {
                Element::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        for (i, a) in kaesten.iter().enumerate() {
            for b in kaesten.iter().skip(i + 1) {
                let quer = a.x < b.x + b.breite - 0.05 && b.x < a.x + a.breite - 0.05;
                let hoch = a.y < b.y + b.hoehe - 0.05 && b.y < a.y + a.hoehe - 0.05;
                assert!(
                    !(quer && hoch),
                    "überdeckt: {:?} ({},{} {}×{}) und {:?} ({},{} {}×{})",
                    a.text,
                    a.x,
                    a.y,
                    a.breite,
                    a.hoehe,
                    b.text,
                    b.x,
                    b.y,
                    b.breite,
                    b.hoehe
                );
            }
        }
    }

    /// Der Kopf muss über dem Raster bleiben — sonst schreibt er in den
    /// ersten Block hinein.
    #[test]
    fn der_kopf_bleibt_ueber_dem_raster() {
        let seiten = volles_blatt();
        let raster_y = RAND_MM + KOPF_HOEHE_MM;
        for el in &seiten[0].elemente {
            let (_, y1, _, y2) = el.rechteck();
            // Kopfelemente erkennt man daran, dass sie oberhalb beginnen.
            if y1 < raster_y {
                assert!(
                    y2 <= raster_y + 0.01,
                    "{el:?} ragt vom Kopf ins Raster (bis {y2} mm, Raster ab {raster_y} mm)"
                );
            }
        }
    }

    /// Ein Name muss in seine Rasterzeile passen — sonst überläuft er sie
    /// und die Zuordnung Name ↔ Zeile stimmt optisch nicht mehr. Die
    /// Rasterzeile des DBV-Bogens ist mit 5,27 mm deutlich flacher als die
    /// 7 mm des alten Zettels, deshalb bleibt der Test wichtig.
    #[test]
    fn namen_bleiben_in_ihrer_zeilenhoehe() {
        const PT_IN_MM: f32 = 0.352_777_8;
        let gebraucht = NAME_PT * 1.05 * PT_IN_MM;
        assert!(
            gebraucht <= ZEILE_HOEHE_MM,
            "Name braucht {gebraucht} mm, die Zeile ist nur {ZEILE_HOEHE_MM} mm hoch"
        );
    }

    /// Der schärfere Wächter: kein einziges gezeichnetes Element darf über
    /// den Satzspiegel hinauslaufen — Budgetrechnung hin oder her.
    #[test]
    fn kein_element_verlaesst_das_blatt() {
        let seiten = blatt(&doc(vec![satz(1, 40), satz(2, 40), satz(3, 30)]));
        for (nr, seite) in seiten.iter().enumerate() {
            for el in &seite.elemente {
                let (x1, y1, x2, y2) = el.rechteck();
                assert!(
                    x1 >= RAND_MM - 0.01 && x2 <= SEITE_BREITE_MM - RAND_MM + 0.01,
                    "Seite {nr}: {el:?} liegt waagerecht außerhalb ({x1}..{x2})"
                );
                assert!(
                    y1 >= RAND_MM - 0.01 && y2 <= SEITE_HOEHE_MM - RAND_MM + 0.01,
                    "Seite {nr}: {el:?} liegt senkrecht außerhalb ({y1}..{y2})"
                );
            }
        }
    }

    #[test]
    fn ein_satz_bis_32_ballwechsel_belegt_einen_block() {
        let plan = blockfolge(&doc(vec![satz(1, 32)]).grid);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].von, 1);
        assert_eq!(plan[0].bis, 32);
    }

    /// Ein Satz mit 40 Ballwechseln läuft im nächsten Block weiter, und
    /// der beginnt bei Ballwechsel 33.
    #[test]
    fn satz_ueber_32_laeuft_im_naechsten_block_weiter() {
        let plan = blockfolge(&doc(vec![satz(1, 40)]).grid);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[1].lauf, 1);
        assert_eq!(plan[1].von, 33);
    }

    /// Ein Satz beginnt nie in einem Block, den ein anderer angefangen hat.
    #[test]
    fn jeder_satz_beginnt_in_einem_neuen_block() {
        let plan = blockfolge(&doc(vec![satz(1, 5), satz(2, 5)]).grid);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].satz_index, 0);
        assert_eq!(plan[1].satz_index, 1);
        assert!(plan.iter().all(|p| p.lauf == 0));
    }

    #[test]
    fn drei_lange_saetze_passen_auf_eine_seite() {
        let seiten = blatt(&doc(vec![satz(1, 40), satz(2, 40), satz(3, 40)]));
        assert_eq!(seiten.len(), 1, "sechs Blöcke sind genau eine Seite");
    }

    #[test]
    fn ein_siebter_block_erzwingt_eine_zweite_seite() {
        let seiten = blatt(&doc(vec![satz(1, 40), satz(2, 40), satz(3, 70)]));
        assert_eq!(seiten.len(), 2);
    }

    /// Ein leerer Satz bekommt trotzdem seinen Block — sonst hätte der
    /// Vorabzettel keine Zeile zum Schreiben.
    #[test]
    fn ein_satz_ohne_ballwechsel_bekommt_seinen_block() {
        let plan = blockfolge(&doc(vec![satz(1, 0)]).grid);
        assert_eq!(plan.len(), 1);
    }

    /// Lange Namen werden gekürzt statt das Raster zu verschieben: Der
    /// Textkasten hat feste Breite und ist als kürzbar markiert.
    #[test]
    fn lange_namen_werden_gekuerzt() {
        let mut d = doc(vec![satz(1, 5)]);
        d.team_a[0].name = "Mustermann-Nachtigaller, Wolfgang-Sebastian".into();
        let seiten = blatt(&d);
        let kaesten: Vec<&TextKasten> = seiten[0]
            .elemente
            .iter()
            .filter_map(|e| match e {
                Element::Text(t) if t.text.starts_with("Mustermann") => Some(t),
                _ => None,
            })
            .collect();
        assert!(!kaesten.is_empty(), "Name kommt gar nicht vor");
        assert!(
            kaesten.iter().all(|k| k.kuerzen),
            "jeder Namenskasten muss kürzen dürfen"
        );
        // Der Kasten in der Namensspalte des Rasters ist der kritische:
        // Er darf die Spalte nicht verbreitern, sonst wandert das Raster.
        let im_raster = kaesten
            .iter()
            .find(|k| k.x < AR_X_MM)
            .expect("kein Name in der Namensspalte");
        assert!(im_raster.breite <= NAMENSSPALTE_MM);
        assert!(im_raster.x + im_raster.breite <= AR_X_MM);
    }

    /// Weder Wort- noch Bildmarke eines Verbands (ADR 0043).
    #[test]
    fn kein_verbandslogo_im_dokument() {
        let seiten = blatt(&doc(vec![satz(1, 5)]));
        for seite in &seiten {
            for t in texte(seite) {
                let klein = t.to_lowercase();
                assert!(!klein.contains("verband"), "Verbandsbezug im Text: {t}");
                assert!(!klein.contains("dbv"), "Verbandsbezug im Text: {t}");
            }
        }
    }

    /// Der Vermerk „kein amtlicher Beleg" ist zurückgenommen (ADR 0043).
    #[test]
    fn kein_archiv_vermerk_mehr() {
        let seiten = blatt(&doc(vec![satz(1, 5)]));
        for seite in &seiten {
            for t in texte(seite) {
                assert!(!t.contains("amtlich"), "Archiv-Vermerk noch da: {t}");
            }
        }
    }

    /// Ohne hinterlegtes Logo entsteht dasselbe Blatt, nur ohne Bild — die
    /// Kopfhöhe bleibt gleich.
    #[test]
    fn ohne_logo_bleibt_die_kopfhoehe_gleich() {
        let ohne = blatt(&doc(vec![satz(1, 5)]));
        let mut mit_doc = doc(vec![satz(1, 5)]);
        mit_doc.logo_vorhanden = true;
        let mit = blatt(&mit_doc);
        let hat_logo = |s: &Seite| s.elemente.iter().any(|e| matches!(e, Element::Logo { .. }));
        assert!(!hat_logo(&ohne[0]));
        assert!(hat_logo(&mit[0]));
        assert_eq!(texte(&ohne[0]), texte(&mit[0]));
    }

    /// Vorkommnisse stehen auf einer eigenen Anhangseite — das Vorbild
    /// kennt keine Protokolltabelle im Raster.
    #[test]
    fn ereignisse_kommen_auf_eine_anhangseite() {
        use relay_proto::{EventKind, Phase};
        let mut d = doc(vec![satz(1, 5)]);
        d.grid
            .protokoll
            .push(crate::tablet::scoresheet::ProtokollZeile {
                nr: 1,
                ts_ms: 1_755_000_000_000,
                satz: 1,
                score_a: 3,
                score_b: 2,
                art: EventKind::CardYellow,
                phase: Phase::Play,
                team: 0,
                player: 1,
                zurueckgenommen: false,
            });
        let seiten = blatt(&d);
        assert_eq!(seiten.len(), 2);
        assert!(texte(&seiten[1]).iter().any(|t| t == "Vorkommnisse"));
    }

    /// Ein Vorabzettel hat keine Vorkommnisse und damit keine Anhangseite.
    #[test]
    fn ohne_ereignisse_keine_anhangseite() {
        let seiten = blatt(&doc(vec![satz(1, 0), satz(2, 0), satz(3, 0)]));
        assert_eq!(seiten.len(), 1);
    }

    /// Die Vereinszeile steht auch dann, wenn kein Verein bekannt ist —
    /// auf einem handgeführten Blatt ist sie beschreibbar.
    #[test]
    fn vereinszeile_steht_immer() {
        let seiten = blatt(&doc(vec![satz(1, 5)]));
        assert_eq!(
            texte(&seiten[0])
                .iter()
                .filter(|t| *t == "(Verein)")
                .count(),
            2,
            "je Mannschaft eine Vereinszeile"
        );
    }

    /// Im Einzel bleiben die Zeilen 1 und 3 leer — die Zuordnung
    /// `2 * team` hält das Raster mit der Projektion deckungsgleich.
    #[test]
    fn im_einzel_stehen_die_namen_in_zeile_0_und_2() {
        let mut d = doc(vec![satz(1, 5)]);
        d.grid.zeilen = 2;
        d.team_a.truncate(1);
        d.team_b.truncate(1);
        assert_eq!(zeile_fuer(&d.grid, 0, 1), 0);
        assert_eq!(zeile_fuer(&d.grid, 1, 1), 2);
    }
}
