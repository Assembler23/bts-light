//! Stiller Druck des Schiedsrichterzettels (ADR 0042).
//!
//! Der zweite Treiber der Blatt-Elementliste: Was
//! [`crate::tablet::blatt::blatt`] in Millimetern gerechnet hat, geht hier
//! ohne Dialog an einen benannten Windows-Drucker. Der erste Treiber
//! (HTML) bleibt für Bildschirm und Handdruck.
//!
//! **Was hier testbar ist und was nicht:** Die Umrechnung Millimeter →
//! Gerätepunkte und die Wahl des Druckers sind reine Funktionen und haben
//! Tests. Das Zeichnen selbst spricht Win32 an; es ist bewusst dünn
//! gehalten und fährt die Elementliste nur ab.

use crate::tablet::blatt::Seite;

#[cfg(windows)]
mod windows;

/// Was beim Drucken schiefgehen kann. Jede Ursache trägt einen Text, den
/// die Turnierleitung ohne Windows-Kenntnisse einordnen kann — die
/// Meldung landet im Dashboard.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DruckFehler {
    #[error("Kein Drucker eingerichtet — bitte in den Einstellungen einen auswählen.")]
    KeinDrucker,
    #[error("Der Drucker „{0}“ ist nicht erreichbar.")]
    NichtErreichbar(String),
    #[error("Der Druckauftrag wurde abgewiesen: {0}")]
    Abgewiesen(String),
    #[error("Drucken wird auf diesem Betriebssystem nicht unterstützt.")]
    NichtUnterstuetzt,
}

/// Umrechnung Blattmaß → Gerätepunkte.
///
/// **Der Nullpunkt eines Drucker-Gerätekontexts liegt in der linken oberen
/// Ecke des _bedruckbaren_ Bereichs, nicht des Blatts.** Wer das übersieht,
/// druckt alles um den nicht bedruckbaren Rand verschoben — bei 4 mm
/// Randversatz genug, um die letzte Rasterspalte zu verlieren. Deshalb
/// gehen die physischen Versätze hier ab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Umrechnung {
    /// Gerätepunkte je Zoll, waagerecht und senkrecht.
    pub dpi_x: f32,
    pub dpi_y: f32,
    /// Nicht bedruckbarer Rand in Gerätepunkten (linke obere Blattecke).
    pub versatz_x: f32,
    pub versatz_y: f32,
}

impl Umrechnung {
    const MM_JE_ZOLL: f32 = 25.4;

    pub fn x(&self, mm: f32) -> i32 {
        (mm / Self::MM_JE_ZOLL * self.dpi_x - self.versatz_x).round() as i32
    }

    pub fn y(&self, mm: f32) -> i32 {
        (mm / Self::MM_JE_ZOLL * self.dpi_y - self.versatz_y).round() as i32
    }

    /// Eine Länge ohne Versatz — für Breiten, Höhen und Strichstärken.
    pub fn laenge_x(&self, mm: f32) -> i32 {
        (mm / Self::MM_JE_ZOLL * self.dpi_x).round() as i32
    }

    pub fn laenge_y(&self, mm: f32) -> i32 {
        (mm / Self::MM_JE_ZOLL * self.dpi_y).round() as i32
    }

    /// Schriftgrad in Gerätepunkten: 1 pt = 1/72 Zoll.
    pub fn schrift(&self, pt: f32) -> i32 {
        let px = (pt / 72.0 * self.dpi_y).round() as i32;
        px.max(1)
    }
}

/// Die eingerichteten Drucker des Systems.
///
/// Der leere Name steht in der Konfiguration für „Windows-Standarddrucker"
/// und taucht hier deshalb **nicht** als Eintrag auf — die Oberfläche
/// bietet ihn als eigene Zeile an.
pub fn drucker_liste() -> Vec<String> {
    #[cfg(windows)]
    {
        windows::drucker_liste()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Seiten still an einen Drucker geben. Leerer Name = Standarddrucker.
///
/// Blockiert bis der Auftrag beim Spooler liegt — der Aufrufer gehört
/// deshalb in eine eigene Aufgabe, nie in den Sync-Lauf.
pub fn drucke(seiten: &[Seite], titel: &str, drucker: &str) -> Result<(), DruckFehler> {
    if seiten.is_empty() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        windows::drucke(seiten, titel, drucker)
    }
    #[cfg(not(windows))]
    {
        let _ = (seiten, titel, drucker);
        Err(DruckFehler::NichtUnterstuetzt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 600 dpi, 4 mm nicht bedruckbarer Rand ringsum — ein gewöhnlicher
    /// Laserdrucker.
    fn laser() -> Umrechnung {
        Umrechnung {
            dpi_x: 600.0,
            dpi_y: 600.0,
            versatz_x: 4.0 / 25.4 * 600.0,
            versatz_y: 4.0 / 25.4 * 600.0,
        }
    }

    #[test]
    fn millimeter_werden_zu_geraetepunkten() {
        let u = laser();
        // 25,4 mm = 1 Zoll = 600 Punkte, abzüglich des Versatzes.
        assert_eq!(u.laenge_x(25.4), 600);
        assert_eq!(u.laenge_y(25.4), 600);
        assert_eq!(u.schrift(72.0), 600);
    }

    /// **Der Fehler, der sonst erst auf Papier auffällt:** Der Nullpunkt
    /// des Gerätekontexts ist die bedruckbare Ecke. Ein Element bei 5 mm
    /// Blattrand liegt bei 4 mm Geräterand nur 1 mm vom Nullpunkt weg.
    #[test]
    fn der_nicht_bedruckbare_rand_geht_ab() {
        let u = laser();
        assert_eq!(u.x(4.0), 0, "genau am bedruckbaren Rand");
        assert_eq!(u.x(5.0), u.laenge_x(1.0));
        assert_eq!(u.y(5.0), u.laenge_y(1.0));
        // Ohne die Korrektur läge dasselbe Element um 4 mm verschoben.
        let ohne = Umrechnung {
            versatz_x: 0.0,
            versatz_y: 0.0,
            ..u
        };
        assert_eq!(ohne.x(5.0) - u.x(5.0), u.laenge_x(4.0));
    }

    /// Ein Schriftgrad rundet nie auf null — sonst zeichnete GDI mit der
    /// Systemvorgabe statt mit unserer Größe.
    #[test]
    fn winzige_schrift_bleibt_sichtbar() {
        let u = Umrechnung {
            dpi_x: 72.0,
            dpi_y: 72.0,
            versatz_x: 0.0,
            versatz_y: 0.0,
        };
        assert_eq!(u.schrift(0.1), 1);
    }

    /// Ohne Seiten gibt es nichts zu tun — und keinen Fehler. Das trifft
    /// jeden Aufrufer, bevor er den Drucker überhaupt anspricht.
    #[test]
    fn ohne_seiten_passiert_nichts() {
        assert_eq!(drucke(&[], "leer", ""), Ok(()));
    }

    /// Werkzeug statt Prüfung: zeigt, welche Drucker Windows meldet.
    /// Hängt am Rechner, auf dem er läuft, und ist deshalb nicht Teil des
    /// normalen Laufs — aber der einzige Weg, das Puffer-Handling von
    /// `EnumPrintersW` gegen ein echtes System zu halten.
    ///
    /// `cargo test --lib drucker_zeigen -- --ignored --nocapture`
    #[test]
    #[ignore = "fragt das echte System ab, prüft nichts"]
    fn drucker_zeigen() {
        let liste = drucker_liste();
        println!("{} Drucker: {liste:?}", liste.len());
    }

    /// **Der ganze Druckpfad, ohne Papier:** gegen „Microsoft Print to
    /// PDF" mit Zieldatei — Querformat, Seitenfolge, Zeichnen und
    /// Spooler-Übergabe laufen echt, heraus kommt eine PDF statt eines
    /// Blatts. Braucht diesen Drucker (auf Windows 11 vorinstalliert) und
    /// läuft deshalb nur auf Zuruf.
    ///
    /// `cargo test --lib druck_nach_pdf -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore = "druckt gegen einen echten Windows-Druckertreiber"]
    fn druck_nach_pdf() {
        use crate::tablet::blatt::{blatt, Element, Seite as _Seite};
        let _ = std::mem::size_of::<_Seite>();
        let doc = crate::tablet::scoresheet::SheetDoc {
            turnier: "Druckprobe".into(),
            disziplin: "Herrendoppel A".into(),
            spielnummer: Some(111),
            feld: "1".into(),
            team_a: vec![
                crate::tablet::scoresheet::SpielerZeile {
                    name: "Becker, Heinz".into(),
                    zusatz: "SC Musterstadt".into(),
                },
                crate::tablet::scoresheet::SpielerZeile {
                    name: "Meier, Kurt".into(),
                    zusatz: "SC Musterstadt".into(),
                },
            ],
            team_b: vec![
                crate::tablet::scoresheet::SpielerZeile {
                    name: "Krause, Dieter".into(),
                    zusatz: "TV Beispielheim".into(),
                },
                crate::tablet::scoresheet::SpielerZeile {
                    name: "Müller, Herbert".into(),
                    zusatz: "TV Beispielheim".into(),
                },
            ],
            ..Default::default()
        };
        let seiten = blatt(&doc);
        assert!(seiten[0]
            .elemente
            .iter()
            .any(|e| matches!(e, Element::Rahmen { .. })));

        let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("target")
            .join("druckprobe.pdf");
        let _ = std::fs::remove_file(&pfad);
        let ergebnis = super::windows::drucke_nach(
            &seiten,
            "Druckprobe",
            "Microsoft Print to PDF",
            pfad.to_str(),
        );
        println!("Druck: {ergebnis:?} → {}", pfad.display());
        assert_eq!(ergebnis, Ok(()));
        let bytes = std::fs::read(&pfad).expect("keine Ausgabe entstanden");
        println!("PDF: {} Bytes", bytes.len());
        assert!(bytes.len() > 1000, "die Ausgabe ist verdächtig klein");

        // **A4 quer, nicht Letter.** Ohne die ausdrückliche Papiergröße im
        // DEVMODE lieferte derselbe Lauf 792 × 612 pt (US-Letter quer) —
        // 279 mm breit, wo das Raster 275 mm plus Rand braucht. Die letzte
        // Spalte fiele ab, und zwar lautlos.
        let text = String::from_utf8_lossy(&bytes);
        let box_zeile = text
            .split("/MediaBox")
            .nth(1)
            .map(|s| s.chars().take(40).collect::<String>())
            .unwrap_or_default();
        println!("MediaBox: {box_zeile}");
        assert!(
            box_zeile.contains("841") && box_zeile.contains("595"),
            "A4 quer erwartet (841 × 595 pt), war: {box_zeile}"
        );
    }
}
