//! In die Binärdatei eingebettete statische Assets des Tablet-Servers.

/// Die Tablet-Spielzettel-UI – wird unter `/court/{label}` ausgeliefert.
/// `__COURT_LABEL__` wird beim Ausliefern durch den Court-Namen ersetzt.
pub const TABLET_HTML: &str = include_str!("../../assets/tablet.html");
