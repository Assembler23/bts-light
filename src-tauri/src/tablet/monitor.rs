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
    device_code, AdStyleWire, MonitorConfig, MonitorMatch, MonitorPlayer, MonitorState,
    MonitorTarget, SetAb,
};

use crate::btp::model::BtpPlayer;
use crate::config::CourtMonitorConfig;
use crate::tablet::state::MonitorCourt;

/// Server-Zeit in Millisekunden seit Epoch. Wird in den `MonitorState`
/// gelegt (`server_now_ms`), damit der TV den Pausen-Countdown relativ
/// zur Server-Uhr rechnet statt zur eigenen (oft driftenden) Pi-Uhr.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

/// Dateiname der Werbebild-Label-Persistenz (im selben App-Config-
/// Verzeichnis wie [`MONITOR_ASSIGN_FILE`]). Mapping: Dateiname →
/// Anzeigename. Fehlende Eintraege bedeuten "kein Label gesetzt" und
/// werden in der UI als Dateiname dargestellt.
pub const AD_LABELS_FILE: &str = "court-ad-labels.json";

/// Liest die Werbebild-Labels aus der JSON-Datei. Fehlende oder
/// kaputte Datei → leere Map (kein Fehler – Labels sind rein optional).
pub fn read_ad_labels(path: &Path) -> HashMap<String, String> {
    let Ok(j) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str(&j).unwrap_or_default()
}

/// Schreibt eine Datei **atomar**: erst in eine temporäre Datei im selben
/// Verzeichnis, dann per `rename` über die Zieldatei. So sieht ein gleichzeitig
/// lesender Poll (z. B. `/monitor/state` jede Sekunde je Monitor) NIE eine halb
/// geschriebene Datei — sonst lieferte `serde_json::from_str` einen Fehler und
/// die Lese-Funktionen fielen auf eine leere Map zurück (Ursache für „Kombi
/// zeigt keine Daten", Turnier 2026-06-14).
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Schreibt die Werbebild-Labels in die JSON-Datei. Leere Werte werden
/// nicht persistiert (kein Punkt, "" zu speichern).
pub fn write_ad_labels(path: &Path, labels: &HashMap<String, String>) -> std::io::Result<()> {
    let cleaned: HashMap<&String, &String> = labels.iter().filter(|(_, v)| !v.is_empty()).collect();
    let json = serde_json::to_string_pretty(&cleaned).unwrap_or_else(|_| "{}".to_string());
    write_atomic(path, &json)
}

/// Datei mit den Werbebildern, die **zusätzlich klein in der oberen Leiste**
/// diverser Anzeigeseiten erscheinen sollen (neben dem Turnierlogo). Bewusst
/// getrennt von den Labels und ein reines String-Array — so bleibt der
/// Labels-Store unberührt und die Datei ist abwärtskompatibel (fehlt sie,
/// steht kein Bild in der Leiste). Liegt im `court-ads/`-Verzeichnis; ihre
/// `.json`-Endung fällt bei [`list_ads`] durchs Bild-Filter, stört den
/// Bildpool also nicht.
pub const AD_BAR_FILE: &str = "court-ad-bar.json";

/// Liest die Menge der als „Leisten-Sponsor" markierten Dateinamen. Fehlende
/// oder kaputte Datei → leere Menge (kein Fehler — die Markierung ist optional).
pub fn read_ad_bar(path: &Path) -> std::collections::HashSet<String> {
    let Ok(j) = std::fs::read_to_string(path) else {
        return std::collections::HashSet::new();
    };
    serde_json::from_str::<Vec<String>>(&j)
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

/// Schreibt die „Leisten-Sponsor"-Markierungen (atomar, sortiert für stabile
/// Diffs).
pub fn write_ad_bar(
    path: &Path,
    marked: &std::collections::HashSet<String>,
) -> std::io::Result<()> {
    let mut list: Vec<&String> = marked.iter().collect();
    list.sort();
    let json = serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".to_string());
    write_atomic(path, &json)
}

/// Datei mit dem **Anzeige-Stil je Werbebild** für das Leerlauf-Vollbild:
/// Hintergrundfarbe und ob die Feldbezeichnung mit erscheint (Spec
/// `werbung-hintergrund-und-feld`, ADR 0041).
///
/// **Dritter** Store neben Labels und Leisten-Markierung, und das mit Absicht:
/// [`read_ad_labels`] deserialisiert strikt nach `HashMap<String,String>` und
/// schluckt Fehler mit `unwrap_or_default()`. Würde man das Format dort
/// aufbohren, verlöre jede ältere Installation beim Auto-Update **still alle
/// Anzeigenamen** — und beim Rollback noch einmal. Eine eigene Datei ist in
/// beide Richtungen folgenlos: Wer sie nicht kennt, ignoriert sie.
/// Liegt im `court-ads/`-Verzeichnis; die `.json`-Endung fällt bei
/// [`list_ads`] durchs Bild-Filter.
pub const AD_STYLE_FILE: &str = "court-ad-style.json";

/// Vorgabe-Hintergrund des Leerlauf-Vollbilds — der Zustand vor dem Feature.
pub const AD_BG_DEFAULT: &str = "#000000";

/// Anzeige-Stil eines Werbebilds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdStyle {
    /// Hintergrundfarbe `#rrggbb` (Kleinbuchstaben) hinter dem Bild.
    #[serde(default = "bg_default")]
    pub bg: String,
    /// Feldbezeichnung über der Werbung zeigen?
    #[serde(default)]
    pub show_court: bool,
}

fn bg_default() -> String {
    AD_BG_DEFAULT.to_string()
}

impl Default for AdStyle {
    fn default() -> Self {
        Self {
            bg: bg_default(),
            show_court: false,
        }
    }
}

impl AdStyle {
    /// Ein Stil, der nichts vom Vorzustand abweicht, muss nicht gespeichert
    /// werden — so bleibt die Datei leer, solange niemand etwas einstellt.
    pub fn ist_vorgabe(&self) -> bool {
        self.bg == AD_BG_DEFAULT && !self.show_court
    }
}

/// Liest den Stil je Werbebild. Fehlende oder kaputte Datei → leere Map
/// (kein Fehler — der Stil ist optional, ohne ihn gilt die Vorgabe).
pub fn read_ad_style(path: &Path) -> HashMap<String, AdStyle> {
    let Ok(j) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut map: HashMap<String, AdStyle> = serde_json::from_str(&j).unwrap_or_default();
    // Ein von Hand verfälschter Farbwert darf nicht bis ins `style`-Attribut
    // einer Anzeigeseite durchreisen — dort wäre er eine Einschleusstelle.
    // Dieselbe Wächter-Haltung wie beim Hallen-Farb-Store.
    map.retain(|_, s| crate::hall_colors::ist_hex_farbe(&s.bg));
    map
}

/// Schreibt den Stil je Werbebild (atomar). Einträge, die nichts von der
/// Vorgabe abweichen, fallen raus.
pub fn write_ad_style(path: &Path, styles: &HashMap<String, AdStyle>) -> std::io::Result<()> {
    let cleaned: HashMap<&String, &AdStyle> =
        styles.iter().filter(|(_, s)| !s.ist_vorgabe()).collect();
    let json = serde_json::to_string_pretty(&cleaned).unwrap_or_else(|_| "{}".to_string());
    write_atomic(path, &json)
}

/// Schriftfarbe, die auf `bg` lesbar ist — relative Luminanz nach WCAG 2.1.
///
/// Bewusst **nicht** einstellbar: Es soll keinen Weg geben, die
/// Feldbezeichnung unlesbar zu konfigurieren (ADR 0041). Die Schwelle 0,179
/// ist der Punkt, an dem Weiß und Schwarz denselben Kontrastwert erreichen —
/// darüber gewinnt Schwarz, darunter Weiß.
pub fn schriftfarbe(bg: &str) -> &'static str {
    const HELL: &str = "#ffffff";
    const DUNKEL: &str = "#111111";
    if !crate::hall_colors::ist_hex_farbe(bg) {
        // Unbekannte Form → Vorgabe ist Schwarz, also helle Schrift.
        return HELL;
    }
    let kanal = |i: usize| -> f64 {
        let roh = u8::from_str_radix(&bg[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        // sRGB linearisieren, sonst wirkt jedes Mittelgrau zu hell.
        if roh <= 0.040_45 {
            roh / 12.92
        } else {
            ((roh + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminanz = 0.2126 * kanal(1) + 0.7152 * kanal(3) + 0.0722 * kanal(5);
    if luminanz > 0.179 {
        DUNKEL
    } else {
        HELL
    }
}

/// Werbebilder samt ihrem Anzeige-Stil — zwei index-parallele Listen, die
/// nur gemeinsam Sinn ergeben und deshalb auch gemeinsam gereicht werden.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdAnzeige {
    /// Kennungen der Bilder (LAN: Dateiname, Cloud: Index).
    pub ids: Vec<String>,
    /// Stil je Bild, gleiche Reihenfolge; leer = überall Vorgabe.
    pub stile: Vec<AdStyleWire>,
}

/// Baut die Wire-Stile **index-parallel** zu `files` (ADR 0041) und rechnet
/// dabei die Kontrastschrift — hier und nur hier.
///
/// Hat kein einziges Bild einen abweichenden Stil, kommt eine leere Liste
/// zurück: Dann muss gar nichts über den Draht, und eine Anzeige, die das
/// Feld nicht kennt, sieht denselben leeren Fall wie vor dem Feature.
pub fn ad_styles_fuer(files: &[String], styles: &HashMap<String, AdStyle>) -> Vec<AdStyleWire> {
    if styles.is_empty() {
        return Vec::new();
    }
    files
        .iter()
        .map(|f| match styles.get(f) {
            Some(s) => AdStyleWire {
                bg: s.bg.clone(),
                fg: schriftfarbe(&s.bg).to_string(),
                show_court: s.show_court,
            },
            None => AdStyleWire::default(),
        })
        .collect()
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
        push_fallback_slow: c.push_fallback_slow,
    }
}

/// Baut den vollständigen Anzeige-Zustand eines Feldes (LAN-Pfad).
/// `court_id` ist die Feld-Identität, `court_label` der Anzeigename.
pub fn build_monitor_state(
    court_id: i64,
    court_label: String,
    hall_color: Option<String>,
    court: MonitorCourt,
    config: &CourtMonitorConfig,
    call_timer: &crate::config::CallTimerConfig,
    ads: AdAnzeige,
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
        hall_color,
        // Ordnung für die Anzeige (Spec monitor-livestand-push, S4) —
        // dieselbe Zahl, die der Nudge dieses Felds trägt.
        seq: court.seq,
        tournament_name: court.tournament_name,
        match_info,
        court_state: court.court_state,
        config: to_monitor_config(config),
        ads: ads.ids,
        ad_styles: ads.stile,
        command: None,
        device_code: String::new(),
        unassigned: false,
        redirect_to: None,
        server_now_ms: now_ms(),
        on_court_since_ms: court.on_court_since_ms,
        call_timer: relay_proto::CallTimerView {
            enabled: call_timer.enabled,
            second_call_minutes: call_timer.second_call_minutes,
            third_call_minutes: call_timer.third_call_minutes,
        },
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
            // v3 da. JSON pro-Eintrag entserialisieren statt das ganze
            // Map auf einmal: bei einem **Downgrade** (z. B. zurück auf
            // v0.9.18/v0.9.19, die `ad_*`-Tags nicht kennen) würde sonst
            // ein einziger unbekannter Eintrag das gesamte File-Parse
            // zerstören → alle anderen Zuweisungen wären weg.
            // Mit `Value`-Zwischenstufe ignorieren wir nur die Einträge,
            // die wir nicht kennen, und bewahren die bekannten.
            // (Code-Review HIGH-Finding v0.9.21.)
            let raw: HashMap<String, serde_json::Value> =
                serde_json::from_str(&j).unwrap_or_default();
            return raw
                .into_iter()
                .filter_map(|(k, v)| serde_json::from_value(v).ok().map(|t| (k, t)))
                .collect();
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
    let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());
    write_atomic(path, &json)
}

/// Dateiname der expliziten Hallen-Zuordnung je Monitor-Gerät (Geräte-ID →
/// Hallenname). Getrennt von den Feld-Zuweisungen, damit eine Halle auch für
/// nicht-feldgebundene Geräte (Info/Werbung/Kombi/unzugewiesen) gilt.
pub const MONITOR_HALLS_FILE: &str = "monitor-halls.json";

/// Liest die Geräte→Hallenname-Zuordnung. Fehlt/klemmt die Datei: leere Map.
pub fn read_halls(path: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Schreibt die Geräte→Hallenname-Zuordnung.
pub fn write_halls(path: &Path, map: &HashMap<String, String>) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());
    write_atomic(path, &json)
}

/// Anzeige-Zustand für ein noch keinem Feld zugewiesenes Gerät – der
/// Monitor zeigt damit die Kopplungs-Seite mit seinem Code.
pub fn unassigned_monitor_state(device_id: &str) -> MonitorState {
    MonitorState {
        court_id: 0,
        court_label: String::new(),
        // Kopplungs-Seite: kein Feld, keine Halle.
        hall_color: None,
        // Kein Feld, also keine Ordnung — die Seite zeigt nur ihren Code.
        seq: 0,
        tournament_name: String::new(),
        match_info: None,
        court_state: None,
        config: MonitorConfig::default(),
        ads: Vec::new(),
        ad_styles: Vec::new(),
        command: None,
        device_code: device_code(device_id),
        unassigned: true,
        redirect_to: None,
        server_now_ms: now_ms(),
        on_court_since_ms: None,
        call_timer: relay_proto::CallTimerView::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_langsame_fallback_schalter_erreicht_die_anzeige() {
        // Spec monitor-livestand-push, S6. Der Schalter nützt nur, wenn er
        // die ganze Strecke überlebt: config.json → MonitorConfig →
        // JSON-Feld `pushFallbackSlow`, auf das die Seite schaut. Fällt er
        // unterwegs weg, pollt die Anzeige stillschweigend weiter im
        // 250-ms-Takt — die Entlastung bliebe unbemerkt aus.
        let mut cfg = crate::config::CourtMonitorConfig::default();
        assert!(!cfg.push_fallback_slow, "Standard ist aus");
        cfg.push_fallback_slow = true;

        let wire = to_monitor_config(&cfg);
        assert!(wire.push_fallback_slow);

        let json = serde_json::to_string(&wire).expect("serialisierbar");
        assert!(
            json.contains("\"pushFallbackSlow\":true"),
            "Feldname wie im Asset erwartet: {json}"
        );
    }

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
    fn read_halls_missing_file_is_empty_then_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(MONITOR_HALLS_FILE);
        // Fehlt die Datei → leere Map (kein Fehler).
        assert!(read_halls(&path).is_empty());
        // Schreiben + Lesen erhält die Zuordnung.
        let mut map = HashMap::new();
        map.insert("dev-1".to_string(), "Halle 2".to_string());
        write_halls(&path, &map).unwrap();
        assert_eq!(
            read_halls(&path).get("dev-1").map(String::as_str),
            Some("Halle 2")
        );
    }

    #[test]
    fn read_halls_corrupt_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MONITOR_HALLS_FILE);
        std::fs::write(&path, "{ kaputt").unwrap();
        assert!(read_halls(&path).is_empty());
    }

    #[test]
    fn read_write_assignments_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MONITOR_ASSIGN_FILE);
        assert!(read_assignments(&path).is_empty()); // fehlende Datei → leer
        let mut map = HashMap::new();
        map.insert("dev-1".to_string(), MonitorTarget::court(103));
        map.insert(
            "dev-2".to_string(),
            MonitorTarget::InfoOverview { hall: None },
        );
        map.insert("dev-3".to_string(), MonitorTarget::InfoPreparation);
        map.insert(
            "dev-4".to_string(),
            MonitorTarget::InfoWinners { rank: None },
        );
        map.insert(
            "dev-5".to_string(),
            MonitorTarget::InfoWinners { rank: Some(1) },
        );
        write_assignments(&path, &map).unwrap();
        assert_eq!(read_assignments(&path), map);
    }

    #[test]
    fn read_write_ad_bar_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(AD_BAR_FILE);
        // Fehlende Datei → leere Menge.
        assert!(read_ad_bar(&path).is_empty());
        let mut set = std::collections::HashSet::new();
        set.insert("ad-1.png".to_string());
        set.insert("ad-2.jpg".to_string());
        write_ad_bar(&path, &set).unwrap();
        assert_eq!(read_ad_bar(&path), set);
        // Kaputte Datei → leere Menge (kein Fehler, Markierung ist optional).
        std::fs::write(&path, "{ kaputt").unwrap();
        assert!(read_ad_bar(&path).is_empty());
        // Leere Menge schreibt ein leeres Array, das wieder leer liest.
        write_ad_bar(&path, &std::collections::HashSet::new()).unwrap();
        assert!(read_ad_bar(&path).is_empty());
    }

    #[test]
    fn read_write_ad_style_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(AD_STYLE_FILE);
        // Fehlende Datei → leere Map, jedes Bild bekommt die Vorgabe.
        assert!(read_ad_style(&path).is_empty());

        let mut map = HashMap::new();
        map.insert(
            "ad-1.png".to_string(),
            AdStyle {
                bg: "#ffffff".to_string(),
                show_court: true,
            },
        );
        // Ein Eintrag, der nichts von der Vorgabe abweicht, wird nicht
        // gespeichert — sonst wüchse die Datei mit jedem Anfassen.
        map.insert("ad-2.jpg".to_string(), AdStyle::default());
        write_ad_style(&path, &map).unwrap();

        let gelesen = read_ad_style(&path);
        assert_eq!(gelesen.len(), 1, "nur der abweichende Eintrag bleibt");
        assert_eq!(gelesen["ad-1.png"].bg, "#ffffff");
        assert!(gelesen["ad-1.png"].show_court);

        // Kaputte Datei → leere Map (kein Fehler, der Stil ist optional).
        std::fs::write(&path, "{ kaputt").unwrap();
        assert!(read_ad_style(&path).is_empty());

        // Von Hand verfälschte Farbe fliegt beim Lesen raus, statt bis in ein
        // `style`-Attribut durchzureisen.
        std::fs::write(
            &path,
            r#"{"ad-3.png":{"bg":"red; background:url(x)","show_court":true}}"#,
        )
        .unwrap();
        assert!(read_ad_style(&path).is_empty(), "krumme Farbe muss raus");
    }

    #[test]
    fn schriftfarbe_kontrastiert_zum_grund() {
        // Die beiden Ecken.
        assert_eq!(schriftfarbe("#000000"), "#ffffff");
        assert_eq!(schriftfarbe("#ffffff"), "#111111");
        // Beidseits der Schwelle: kräftiges Rot ist dunkel genug für helle
        // Schrift, ein Pastellgelb verlangt dunkle.
        assert_eq!(schriftfarbe("#c81432"), "#ffffff");
        assert_eq!(schriftfarbe("#ffe680"), "#111111");
        // Grün wiegt in der Luminanz am schwersten — mittleres Grün ist hell.
        assert_eq!(schriftfarbe("#00b050"), "#111111");
        // Unbekannte Form → wie auf der schwarzen Vorgabe.
        assert_eq!(schriftfarbe("rot"), "#ffffff");
        assert_eq!(schriftfarbe("#FFF"), "#ffffff");
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
    fn read_assignments_skips_unknown_variants_but_keeps_known() {
        // v0.9.21 (Code-Review HIGH): Eine v3-Datei mit einer unbekannten
        // Variante (z.B. nach Downgrade von v0.9.20 zurück auf v0.9.19)
        // darf NICHT die ganze Map verwerfen. Bekannte Einträge bleiben,
        // unbekannte werden still ignoriert.
        let dir = tempfile::tempdir().unwrap();
        let v3_path = dir.path().join(MONITOR_ASSIGN_FILE);
        // Mix: 2 bekannte + 1 unbekannte Variante.
        std::fs::write(
            &v3_path,
            r#"{
                "dev-1": {"kind":"court","court_id":42},
                "dev-2": {"kind":"future_thing","payload":"ignored"},
                "dev-3": {"kind":"info_overview"}
            }"#,
        )
        .unwrap();
        let map = read_assignments(&v3_path);
        assert_eq!(
            map.len(),
            2,
            "unbekannte Variante muss still ignoriert werden, bekannte bleiben"
        );
        assert_eq!(map.get("dev-1"), Some(&MonitorTarget::court(42)));
        assert_eq!(
            map.get("dev-3"),
            Some(&MonitorTarget::InfoOverview { hall: None })
        );
        assert!(!map.contains_key("dev-2"));
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
        v3.insert(
            "dev-1".to_string(),
            MonitorTarget::InfoOverview { hall: None },
        );
        write_assignments(&v3_path, &v3).unwrap();
        assert_eq!(read_assignments(&v3_path), v3);
    }

    #[test]
    fn monitor_target_serde_format_is_kind_tagged() {
        // Sanity-Check der JSON-Repräsentation – damit die TypeScript-
        // Seite (api.ts) verlässlich Bescheid weiß.
        let court = serde_json::to_string(&MonitorTarget::court(5)).unwrap();
        assert_eq!(court, r#"{"kind":"court","court_id":5}"#);
        let info = serde_json::to_string(&MonitorTarget::InfoOverview { hall: None }).unwrap();
        assert_eq!(info, r#"{"kind":"info_overview"}"#);
        let prep = serde_json::to_string(&MonitorTarget::InfoPreparation).unwrap();
        assert_eq!(prep, r#"{"kind":"info_preparation"}"#);
        let win = serde_json::to_string(&MonitorTarget::InfoWinners { rank: None }).unwrap();
        assert_eq!(win, r#"{"kind":"info_winners"}"#);
        let win1 = serde_json::to_string(&MonitorTarget::InfoWinners { rank: Some(3) }).unwrap();
        assert_eq!(win1, r#"{"kind":"info_winners","rank":3}"#);
        // v0.9.20: Ad-Targets.
        let rot = serde_json::to_string(&MonitorTarget::AdRotation).unwrap();
        assert_eq!(rot, r#"{"kind":"ad_rotation"}"#);
        let sng = serde_json::to_string(&MonitorTarget::ad_single("foo.png")).unwrap();
        assert_eq!(sng, r#"{"kind":"ad_single","file":"foo.png"}"#);
        // v0.9.27: Kombi-Target.
        let combo = serde_json::to_string(&MonitorTarget::court_combo(vec![1, 2, 3])).unwrap();
        assert_eq!(combo, r#"{"kind":"court_combo","court_ids":[1,2,3]}"#);
        // Roundtrip (auch fuer die read_assignments-Persistenz wichtig).
        let back: MonitorTarget = serde_json::from_str(&combo).unwrap();
        assert_eq!(back, MonitorTarget::court_combo(vec![1, 2, 3]));
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

    #[test]
    fn monitor_target_combo_redirect_path() {
        // Kombi-Target leitet auf /combo?courts=1,2,3 um (v0.9.27).
        assert_eq!(
            MonitorTarget::court_combo(vec![1, 2, 3])
                .redirect_path()
                .as_deref(),
            Some("/combo?courts=1,2,3"),
        );
        assert_eq!(
            MonitorTarget::court_combo(vec![7])
                .redirect_path()
                .as_deref(),
            Some("/combo?courts=7"),
        );
        // court_id() ist None fuer Kombi → wird im Cloud-Filter (nur
        // Court-Targets) korrekt ausgeschlossen, LAN-only wie Info/Ad.
        assert_eq!(MonitorTarget::court_combo(vec![1, 2]).court_id(), None);
    }
}
