//! Court-Monitor: gemeinsame Helfer für die read-only TV-Anzeige am
//! Spielfeld.
//!
//! Die Anzeige-Seite selbst ist `assets/monitor.html`. Hier liegen die
//! serverseitigen Bausteine, die der LAN-Server, der Relay-Client und die
//! Werbebild-Verwaltung teilen: Werbebild-Verzeichnis, Dateinamen-
//! Validierung und der Bau des [`MonitorState`].

use std::collections::HashMap;
use std::path::Path;

use relay_proto::{
    device_code, MonitorConfig, MonitorMatch, MonitorPlayer, MonitorState, MonitorTarget, SetAb,
};

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
        show_match_clock: c.show_match_clock,
        show_ads: c.show_ads,
        layout: c.layout.clone(),
    }
}

/// Baut den vollständigen Anzeige-Zustand eines Feldes (LAN-Pfad).
/// `court_id` ist die Feld-Identität, `court_label` der Anzeigename.
pub fn build_monitor_state(
    court_id: i64,
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
        court_id,
        court_label,
        tournament_name: court.tournament_name,
        match_info,
        court_state: court.court_state,
        config: to_monitor_config(config),
        ads,
        command: None,
        device_code: String::new(),
        unassigned: false,
        redirect_to: None,
    }
}

fn player(p: &BtpPlayer) -> MonitorPlayer {
    MonitorPlayer {
        name: p.name.clone(),
        // Vor- und Nachname getrennt aus den BTP-Daten – damit der
        // Court-Monitor sie exakt im Broadcast-Stil anzeigt, statt `name`
        // zu zerlegen.
        given: p.first.clone(),
        family: p.last.clone(),
        nationality: p.nationality.clone(),
    }
}

// ─────────────────────────── Geräte-Verwaltung ────────────────────────────

/// Dateiname der Monitor-Geräte-Zuweisungen (im App-Config-Verzeichnis).
///
/// `…-v3`: der Wert-Typ ist seit dem Info-Monitor-Konzept ein
/// [`MonitorTarget`] (Feld ODER Info-Anzeige), vorher direkt eine `CourtID`
/// (v2) bzw. ein Feldname (v1). Eine vorhandene v2-Datei wird beim ersten
/// Lesen automatisch nach v3 migriert (jede CourtID → `Target::Court`),
/// die v1-Datei wird ignoriert.
pub const MONITOR_ASSIGN_FILE: &str = "monitor-assignments-v3.json";

/// Vorgänger-Dateiname (v2: nur CourtIDs). Wird beim Lesen als
/// Migrationsquelle benutzt, falls die v3-Datei fehlt.
const MONITOR_ASSIGN_FILE_V2: &str = "monitor-assignments-v2.json";

/// Liest die Geräte→Target-Zuweisungen aus der JSON-Datei.
/// Fehlt oder klemmt die Datei, ist die Zuweisung leer (kein Fehler).
/// **Migration:** Existiert noch keine v3-Datei, wird eine vorhandene
/// v2-Datei (nur CourtIDs) eingelesen und jeder Eintrag in
/// `Target::Court` übersetzt – einmalig, transparent.
pub fn read_assignments(path: &Path) -> HashMap<String, MonitorTarget> {
    // Schritt 1: v3 versuchen.
    if let Ok(j) = std::fs::read_to_string(path) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, MonitorTarget>>(&j) {
            return map;
        }
    }
    // Schritt 2: v2 als Migrationsquelle.
    let v2_path = path.with_file_name(MONITOR_ASSIGN_FILE_V2);
    if let Ok(j) = std::fs::read_to_string(&v2_path) {
        if let Ok(v2_map) = serde_json::from_str::<HashMap<String, i64>>(&j) {
            return v2_map
                .into_iter()
                .map(|(dev, court_id)| (dev, MonitorTarget::court(court_id)))
                .collect();
        }
    }
    HashMap::new()
}

/// Schreibt die Geräte→Target-Zuweisungen als JSON (v3-Format).
pub fn write_assignments(path: &Path, map: &HashMap<String, MonitorTarget>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, json)
}

/// Anzeige-Zustand für ein noch keinem Feld zugewiesenes Gerät – der
/// Monitor zeigt damit die Kopplungs-Seite mit seinem Code.
pub fn unassigned_monitor_state(device_id: &str) -> MonitorState {
    MonitorState {
        court_id: 0,
        court_label: String::new(),
        tournament_name: String::new(),
        match_info: None,
        court_state: None,
        config: MonitorConfig::default(),
        ads: Vec::new(),
        command: None,
        device_code: device_code(device_id),
        unassigned: true,
        redirect_to: None,
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

    #[test]
    fn read_write_assignments_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MONITOR_ASSIGN_FILE);
        assert!(read_assignments(&path).is_empty()); // fehlende Datei → leer
        let mut map = HashMap::new();
        map.insert("dev-1".to_string(), MonitorTarget::court(103));
        map.insert("dev-2".to_string(), MonitorTarget::InfoOverview);
        map.insert("dev-3".to_string(), MonitorTarget::InfoPreparation);
        write_assignments(&path, &map).unwrap();
        assert_eq!(read_assignments(&path), map);
    }

    #[test]
    fn read_assignments_migrates_v2_when_v3_absent() {
        // v2-Datei (Geräte-ID → CourtID als int) muss transparent in v3
        // (MonitorTarget::Court) übersetzt werden, wenn v3 noch nicht
        // existiert.
        let dir = tempfile::tempdir().unwrap();
        let v3_path = dir.path().join(MONITOR_ASSIGN_FILE);
        let v2_path = dir.path().join("monitor-assignments-v2.json");
        std::fs::write(&v2_path, r#"{"dev-1":103,"dev-2":205}"#).unwrap();
        let map = read_assignments(&v3_path);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("dev-1"), Some(&MonitorTarget::court(103)));
        assert_eq!(map.get("dev-2"), Some(&MonitorTarget::court(205)));
    }

    #[test]
    fn read_assignments_prefers_v3_over_v2() {
        // Existiert v3, wird v2 ignoriert (sonst würde manuelles Editieren
        // an v3 nicht halten).
        let dir = tempfile::tempdir().unwrap();
        let v3_path = dir.path().join(MONITOR_ASSIGN_FILE);
        let v2_path = dir.path().join("monitor-assignments-v2.json");
        std::fs::write(&v2_path, r#"{"dev-1":999}"#).unwrap();
        let mut v3 = HashMap::new();
        v3.insert("dev-1".to_string(), MonitorTarget::InfoOverview);
        write_assignments(&v3_path, &v3).unwrap();
        assert_eq!(read_assignments(&v3_path), v3);
    }

    #[test]
    fn monitor_target_serde_format_is_kind_tagged() {
        // Sanity-Check der JSON-Repräsentation – damit die TypeScript-
        // Seite (api.ts) verlässlich Bescheid weiß.
        let court = serde_json::to_string(&MonitorTarget::court(5)).unwrap();
        assert_eq!(court, r#"{"kind":"court","court_id":5}"#);
        let info = serde_json::to_string(&MonitorTarget::InfoOverview).unwrap();
        assert_eq!(info, r#"{"kind":"info_overview"}"#);
        let prep = serde_json::to_string(&MonitorTarget::InfoPreparation).unwrap();
        assert_eq!(prep, r#"{"kind":"info_preparation"}"#);
    }
}
