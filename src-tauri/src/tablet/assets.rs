//! In die Binärdatei eingebettete statische Assets des Tablet-Servers.

use include_dir::{include_dir, Dir};

/// Die Tablet-Spielzettel-UI – wird unter `/court/{label}` ausgeliefert.
/// `__COURT_LABEL__` wird beim Ausliefern durch den Court-Namen ersetzt.
pub const TABLET_HTML: &str = include_str!("../../assets/tablet.html");

/// Die Court-Monitor-Anzeige (read-only TV am Spielfeld) – wird unter
/// `/court/{label}/display` ausgeliefert. `__COURT_LABEL__` wird beim
/// Ausliefern durch den Court-Namen ersetzt.
pub const MONITOR_HTML: &str = include_str!("../../assets/monitor.html");

/// Gebündelte SVG-Länderflaggen, je Datei nach IOC-3-Buchstaben-Code
/// (`GER.svg`, `POL.svg`, …) – ausgeliefert unter `/flags/{file}`.
/// Herkunft/Lizenz siehe `NOTICE.md`.
pub static FLAGS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/flags");

/// Liefert den Inhalt einer gebündelten Flaggen-Datei, falls vorhanden.
/// `name` darf nur ein Dateiname sein (keine Pfad-Trennzeichen).
pub fn flag_svg(name: &str) -> Option<&'static [u8]> {
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return None;
    }
    FLAGS.get_file(name).map(|f| f.contents())
}
