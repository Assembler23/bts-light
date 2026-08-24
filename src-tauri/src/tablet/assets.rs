//! In die Binärdatei eingebettete statische Assets des Tablet-Servers.

use include_dir::{include_dir, Dir};

/// Die Tablet-Spielzettel-UI – wird unter `/court/{label}` ausgeliefert.
/// `__COURT_LABEL__` wird beim Ausliefern durch den Court-Namen ersetzt.
pub const TABLET_HTML: &str = include_str!("../../assets/tablet.html");

/// Fingerabdruck der ausgelieferten Tablet-Seite (Spec
/// `tablet-version-abgleich`) — **einmal** berechnet.
///
/// Sonst würden bei jedem Lebenszeichen jedes Tablets die vollen ~210 KB
/// `tablet.html` gehasht; bei zwanzig Geräten im Sekundentakt ist das
/// messbare Arbeit für ein Ergebnis, das sich zur Laufzeit nie ändert
/// (Review-Fund 24.08.2026).
///
/// Über das **unersetzte** `TABLET_HTML`: Sonst trüge jedes Feld eine eigene
/// Marke (der Platzhalter ist Teil des Textes) und keine passte zur anderen.
pub fn seiten_marke() -> &'static str {
    static MARKE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MARKE.get_or_init(|| relay_proto::seiten_marke(TABLET_HTML))
}

/// Die Turnierleitungs-Oberfläche – wird unter `/tl` ausgeliefert.
///
/// Ohne Platzhalter: Die Seite holt ihren Zugang aus dem Adress-Fragment und
/// leitet die Adresse ihrer Schnittstelle aus dem eigenen Pfad ab. So läuft
/// dieselbe Datei im Hallennetz und später hinter dem Cloud-Präfix.
pub const TL_HTML: &str = include_str!("../../assets/tl.html");

/// Die Court-Monitor-Anzeige (read-only TV am Spielfeld) – wird unter
/// `/court/{label}/display` ausgeliefert. `__COURT_LABEL__` wird beim
/// Ausliefern durch den Court-Namen ersetzt.
pub const MONITOR_HTML: &str = include_str!("../../assets/monitor.html");

/// TV-Launcher (`/` und `/tv`): Vollbild-Menü zum Auswählen einer Anzeige per
/// Fernbedienung (Pfeiltasten + OK) — man muss nur einmal die kurze Adresse
/// tippen, statt lange `?halle=`-URLs. Holt die Hallen aus `/health`.
pub const TV_HTML: &str = include_str!("../../assets/tv.html");

/// Court-Übersichts-Monitor (Info-Display): zeigt alle Felder mit Status
/// und aktuellem Spiel, nach Hallen gruppiert. Optional `?halle=`-Filter,
/// `?rotate=90|180|270` für Pivot-TVs. Pollt `/health` für Daten.
pub const OVERVIEW_HTML: &str = include_str!("../../assets/overview.html");

/// Vorbereitungs-Monitor (Info-Display): listet aufgerufene und
/// eingeplante Spiele. Optional `?halle=`-Filter, `?rotate=`. Pollt
/// `/info/preparation/state` für Daten.
pub const PREPARATION_HTML: &str = include_str!("../../assets/preparation.html");

/// Sieger-Monitor (Info-Display): Podien (1./2./3., zwei dritte Plätze möglich)
/// aller ausgespielten Disziplinen mit Verein. Pollt `/info/winners/state`.
pub const WINNERS_HTML: &str = include_str!("../../assets/winners.html");

/// Werbe-Anzeige (Info-Display ohne Spielbezug): Vollbild-Werbebild
/// oder rotierende Bildschleife aus den `/ads/{file}` ausgelieferten
/// Werbebildern. Steuerung via Query: `?mode=single&file=…` oder
/// `?mode=rotation`. Pollt `/monitor/state` zum Erkennen einer
/// Re-Zuweisung (analog overview/preparation).
pub const AD_HTML: &str = include_str!("../../assets/ad.html");

/// Kombi-Anzeige (mehrere Felder als horizontale Bänder auf einem
/// Bildschirm). CourtIDs via `?courts=1,2,3`, optional `?device=` +
/// `?rotate=`. Pollt `/combo/state` für die Live-Stände.
pub const COMBO_HTML: &str = include_str!("../../assets/combo.html");

/// Felder-Lobby (`/felder`): Start-Seite fürs Zähltablett. Kacheln aller
/// Felder mit Live-Belegung (Poll `/courts`), Tippen führt auf `/court/{id}`.
pub const LOBBY_HTML: &str = include_str!("../../assets/lobby.html");

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
