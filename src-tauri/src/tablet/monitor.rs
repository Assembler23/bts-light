//! Court-Monitor: gemeinsame Helfer für die read-only TV-Anzeige am
//! Spielfeld.
//!
//! Die Anzeige-Seite selbst ist `assets/monitor.html`. Hier liegen die
//! serverseitigen Bausteine, die der LAN-Server, der Relay-Client und die
//! Werbebild-Verwaltung teilen: Werbebild-Verzeichnis, Dateinamen-
//! Validierung und der Bau des [`MonitorState`].

use std::path::Path;

use relay_proto::{MonitorConfig, MonitorMatch, MonitorPlayer, MonitorState, SetAb};

use crate::btp::model::BtpPlayer;
use crate::config::CourtMonitorConfig;
use crate::tablet::state::MonitorCourt;

/// Unterverzeichnis im App-Datenverzeichnis für die Werbebilder.
pub const AD_DIR_NAME: &str = "court-ads";

/// Erlaubte Bild-Endungen für Werbebilder.
const IMAGE_EXTS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

/// Ist `name` ein zulässiger Werbebild-Dateiname? Erlaubt nur einen reinen
/// Dateinamen (kein Pfad, keine `..`) mit Bild-Endung – schützt die
/// `/ads/{file}`-Route gegen Pfad-Traversal.
pub fn is_safe_image_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    if name.contains(['/', '\\']) || name.contains("..") {
        return false;
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return false;
    }
    image_ext(name).is_some()
}

/// Liefert die (kleingeschriebene) Bild-Endung, falls `name` eine trägt.
pub fn image_ext(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    IMAGE_EXTS
        .into_iter()
        .find(|ext| lower.ends_with(&format!(".{ext}")))
}

/// MIME-Typ einer Bilddatei anhand ihrer Endung.
pub fn image_mime(name: &str) -> &'static str {
    match image_ext(name) {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

/// Listet die Werbebild-Dateinamen im Verzeichnis, alphabetisch sortiert.
/// Ein fehlendes Verzeichnis ergibt eine leere Liste.
pub fn list_ads(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_safe_image_name(n))
        .collect();
    names.sort();
    names
}

/// Übersetzt die persistierte [`CourtMonitorConfig`] in die Wire-Form.
pub fn to_monitor_config(c: &CourtMonitorConfig) -> MonitorConfig {
    MonitorConfig {
        ad_interval_s: c.ad_interval_s,
        show_discipline: c.show_discipline,
        show_round: c.show_round,
        show_match_number: c.show_match_number,
        show_timer: c.show_timer,
    }
}

/// Baut den vollständigen Anzeige-Zustand eines Feldes (LAN-Pfad).
pub fn build_monitor_state(
    court_label: String,
    court: MonitorCourt,
    config: &CourtMonitorConfig,
    ads: Vec<String>,
) -> MonitorState {
    let sets: Vec<SetAb> = court.sets.iter().map(|&(a, b)| SetAb { a, b }).collect();
    let match_info = court.current_match.map(|m| MonitorMatch {
        match_id: m.id,
        discipline: m.discipline.as_str().to_string(),
        event_label: format!("{} {}", m.draw_name, m.round_name)
            .trim()
            .to_string(),
        match_number: m.match_num,
        team1: m.team1.iter().map(player).collect(),
        team2: m.team2.iter().map(player).collect(),
        sets,
    });
    MonitorState {
        court_label,
        tournament_name: court.tournament_name,
        match_info,
        court_state: court.court_state,
        config: to_monitor_config(config),
        ads,
    }
}

fn player(p: &BtpPlayer) -> MonitorPlayer {
    MonitorPlayer {
        name: p.name.clone(),
        nationality: p.nationality.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_image_name_accepts_plain_images_rejects_traversal() {
        assert!(is_safe_image_name("ad-1.jpg"));
        assert!(is_safe_image_name("Sommerfest_2026.PNG"));
        assert!(!is_safe_image_name("../../etc/passwd"));
        assert!(!is_safe_image_name("ad/1.jpg"));
        assert!(!is_safe_image_name("ad-1.svg")); // keine Bild-Endung der Liste
        assert!(!is_safe_image_name("ad-1"));
        assert!(!is_safe_image_name(""));
    }

    #[test]
    fn image_mime_maps_by_extension() {
        assert_eq!(image_mime("x.png"), "image/png");
        assert_eq!(image_mime("x.JPG"), "image/jpeg");
        assert_eq!(image_mime("x.webp"), "image/webp");
    }
}
