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
///
/// **Reihenfolge:**
/// 1. v3-Datei lesen, wenn vorhanden — Erfolg → Map zurückgeben; **Fehler**
///    (Datei da, JSON kaputt) → leere Map. Eine vorhandene aber defekte
///    v3-Datei darf **nicht** auf v2 zurückfallen, sonst überschriebe
///    eine ältere v2 die jüngeren Info-Monitor-Zuweisungen (Code-Review
///    HIGH-Finding v0.9.19).
/// 2. Nur wenn v3-Datei **fehlt**: v2 als Migrationsquelle nutzen. Die
///    migrierte Map wird **sofort als v3 geschrieben**, damit die
///    Migration persistiert und Folge-Lesezugriffe direkt v3 finden.
pub fn read_assignments(path: &Path) -> HashMap<String, MonitorTarget> {
    // Schritt 1: v3 — Datei existiert?
    match std::fs::read_to_string(path) {
        Ok(j) => {
            // v3 da. JSON-Erfolg → fertig; JSON-Fehler → leere Map
            // (keinerlei v2-Fallback bei vorhandener, aber kaputter v3).
            return serde_json::from_str::<HashMap<String, MonitorTarget>>(&j).unwrap_or_default();
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            // Lese-Fehler ungleich NotFound (Berechtigungen etc.):
            // konservativ leer, nicht implizit auf v2 wechseln.
            return HashMap::new();
        }
        Err(_) => {
            // NotFound → fällt durch zu Schritt 2.
        }
    }
    // Schritt 2: v3 fehlt → v2 als einmalige Migrationsquelle.
    let v2_path = path.with_file_name(MONITOR_ASSIGN_FILE_V2);
    let Ok(j) = std::fs::read_to_string(&v2_path) else {
        return HashMap::new();
    };
    let Ok(v2_map) = serde_json::from_str::<HashMap<String, i64>>(&j) else {
        return HashMap::new();
    };
    let migrated: HashMap<String, MonitorTarget> = v2_map
        .into_iter()
        .map(|(dev, court_id)| (dev, MonitorTarget::court(court_id)))
        .collect();
    // Persistenz: v3 sofort schreiben, damit die Migration einmalig bleibt.
    // Best-effort; Fehler werden bewusst ignoriert (Aufrufer sieht trotzdem
    // die migrierte Map; nächster Aufruf migriert eben nochmal).
    let _ = write_assignments(path, &migrated);
    migrated
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
        // v0.9.19: Migration muss persistieren – beim ersten read_assignments
        // wurde v3 sofort geschrieben, ein zweiter Aufruf ohne v2-Datei
        // muss die gleiche Map zurückgeben.
        assert!(v3_path.exists(), "v3-Datei muss nach Migration existieren");
        std::fs::remove_file(&v2_path).unwrap();
        assert_eq!(read_assignments(&v3_path), map);
    }

    #[test]
    fn read_assignments_corrupt_v3_returns_empty_without_v2_fallback() {
        // v0.9.19 (Code-Review HIGH): Wenn die v3-Datei existiert aber
        // beschädigt ist (z.B. abgebrochener Schreibvorgang), darf
        // read_assignments NICHT auf v2 zurückfallen — sonst überschriebe
        // eine ältere v2 die jüngeren Info-Monitor-Zuweisungen. Erwartet:
        // leere Map.
        let dir = tempfile::tempdir().unwrap();
        let v3_path = dir.path().join(MONITOR_ASSIGN_FILE);
        let v2_path = dir.path().join("monitor-assignments-v2.json");
        std::fs::write(&v3_path, "{ not valid json").unwrap();
        std::fs::write(&v2_path, r#"{"dev-1":999}"#).unwrap();
        assert!(read_assignments(&v3_path).is_empty());
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
        // v0.9.20: Ad-Targets.
        let rot = serde_json::to_string(&MonitorTarget::AdRotation).unwrap();
        assert_eq!(rot, r#"{"kind":"ad_rotation"}"#);
        let sng = serde_json::to_string(&MonitorTarget::ad_single("foo.png")).unwrap();
        assert_eq!(sng, r#"{"kind":"ad_single","file":"foo.png"}"#);
    }

    #[test]
    fn monitor_target_ad_redirect_paths() {
        // Ad-Targets liefern Pfad+Query fuer ad.html.
        assert_eq!(
            MonitorTarget::AdRotation.redirect_path().as_deref(),
            Some("/info/ad?mode=rotation"),
        );
        assert_eq!(
            MonitorTarget::ad_single("sommerfest.png")
                .redirect_path()
                .as_deref(),
            Some("/info/ad?mode=single&file=sommerfest.png"),
        );
        // Sonderzeichen muessten URL-escaped werden — unsere Werbebild-
        // Namen sind aber per is_safe_image_name auf [A-Za-z0-9.-_]
        // beschraenkt, daher real eigentlich nie noetig. Sanity-Check
        // trotzdem:
        assert_eq!(
            MonitorTarget::ad_single("hat space.png")
                .redirect_path()
                .as_deref(),
            Some("/info/ad?mode=single&file=hat%20space.png"),
        );
    }
}
