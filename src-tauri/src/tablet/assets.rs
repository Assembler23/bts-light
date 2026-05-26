//! In die Binärdatei eingebettete statische Assets des Tablet-Servers.

use include_dir::{include_dir, Dir};

/// Die Tablet-Spielzettel-UI – wird unter `/court/{label}` ausgeliefert.
/// `__COURT_LABEL__` wird beim Ausliefern durch den Court-Namen ersetzt.
pub const TABLET_HTML: &str = include_str!("../../assets/tablet.html");

/// Die Court-Monitor-Anzeige (read-only TV am Spielfeld) – wird unter
/// `/court/{label}/display` ausgeliefert. `__COURT_LABEL__` wird beim
/// Ausliefern durch den Court-Namen ersetzt.
pub const MONITOR_HTML: &str = include_str!("../../assets/monitor.html");

/// Court-Übersichts-Monitor (Info-Display): zeigt alle Felder mit Status
/// und aktuellem Spiel, nach Hallen gruppiert. Optional `?halle=`-Filter,
/// `?rotate=90|180|270` für Pivot-TVs. Pollt `/health` für Daten.
pub const OVERVIEW_HTML: &str = include_str!("../../assets/overview.html");

/// Vorbereitungs-Monitor (Info-Display): listet aufgerufene und
/// eingeplante Spiele. Optional `?halle=`-Filter, `?rotate=`. Pollt
/// `/info/preparation/state` für Daten.
pub const PREPARATION_HTML: &str = include_str!("../../assets/preparation.html");

/// Werbe-Anzeige (Info-Display ohne Spielbezug): Vollbild-Werbebild
/// oder rotierende Bildschleife aus den `/ads/{file}` ausgelieferten
/// Werbebildern. Steuerung via Query: `?mode=single&file=…` oder
/// `?mode=rotation`. Pollt `/monitor/state` zum Erkennen einer
/// Re-Zuweisung (analog overview/preparation).
pub const AD_HTML: &str = include_str!("../../assets/ad.html");

/// Badhub-Logo (512x512 PNG) – zeigt der Court-Monitor an, solange das
/// Gerät noch keinem Feld/Info-Target zugewiesen ist. Im "Identifizieren"-
/// Modus blendet der Monitor stattdessen den Device-Code groß ein.
pub const BADHUB_LOGO_PNG: &[u8] = include_bytes!("../../assets/badhub-logo.png");

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
