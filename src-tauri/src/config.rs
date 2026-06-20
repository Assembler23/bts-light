//! Verbindungs-Konfiguration der App: BTP-Quelle und Badhub-Ziel.

use serde::{Deserialize, Serialize};

/// Verbindungsdaten für das lokale BTP (TP-Network).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BtpConfig {
    pub host: String,
    pub port: u16,
    /// TP-Network-Passwort, falls in BTP gesetzt.
    pub password: Option<String>,
}

impl Default for BtpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9901,
            password: None,
        }
    }
}

/// Verbindungsdaten für den Badhub-Liveticker-Endpunkt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadhubConfig {
    /// Push-Endpunkt (`live_update.php`).
    pub url: String,
    /// Bearer-Token aus dem Badhub-Liveticker-Admin.
    pub password: String,
    /// Öffentliche Live-Seite, z. B. `https://badhub.de/live?t=bvbb`.
    /// Leer, wenn nicht hinterlegt. `#[serde(default)]` hält ältere
    /// Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub live_url: String,
}

impl Default for BadhubConfig {
    fn default() -> Self {
        Self {
            url: "https://badhub.de/api/live_update.php".to_string(),
            password: String::new(),
            live_url: String::new(),
        }
    }
}

/// Verbindungsart für die Schiedsrichter-Tablets.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    /// Eingebetteter Server im Hallen-LAN (schnell, offline – braucht aber
    /// einen offenen eingehenden Port 8088).
    #[default]
    Lan,
    /// Über den Cloud-Relay auf badhub.de – funktioniert auch hinter
    /// gesperrten Firmen-Firewalls (nur ausgehende Verbindungen).
    Cloud,
    /// LAN **und** Cloud gleichzeitig – z. B. ein Zwei-Hallen-Turnier, bei
    /// dem die Haupthalle die Tablets per LAN anbindet und die zweite Halle
    /// über den Cloud-Relay. Beide Wege laufen für dieselbe Turnierinstanz.
    /// Eigener `rename`, damit die Wire-Form `"lan+cloud"` ist – `"lan"`
    /// und `"cloud"` bleiben unverändert.
    #[serde(rename = "lan+cloud")]
    LanAndCloud,
}

impl ConnectionMode {
    /// Ist der LAN-Pfad aktiv (eingebetteter Server + mDNS)?
    pub fn lan_enabled(self) -> bool {
        matches!(self, ConnectionMode::Lan | ConnectionMode::LanAndCloud)
    }

    /// Ist der Cloud-Pfad aktiv (Relay-Client)?
    pub fn cloud_enabled(self) -> bool {
        matches!(self, ConnectionMode::Cloud | ConnectionMode::LanAndCloud)
    }
}

/// Sprachmodus der gesprochenen Feld-Ansagen.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnounceLanguageMode {
    /// Immer Deutsch ansagen.
    De,
    /// Immer Englisch ansagen.
    En,
    /// Automatisch: Englisch, wenn mindestens die Hälfte der Spieler auf
    /// dem Feld international ist (Nationalität gesetzt und ≠ `GER`).
    #[default]
    Auto,
}

/// Einstellungen für die gesprochene Ansage neu aufs Feld gezogener Spiele.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AnnounceConfig {
    /// Sind Ansagen aktiv?
    pub enabled: bool,
    /// Sprachmodus (Deutsch / Englisch / Automatisch).
    pub language_mode: AnnounceLanguageMode,
    /// Bevorzugte deutsche Stimme (`voiceURI`); leer = Browser-Standard.
    pub voice_de: String,
    /// Bevorzugte englische Stimme (`voiceURI`); leer = Browser-Standard.
    pub voice_en: String,
    /// Sprech-Geschwindigkeit (sinnvoll 0,5–1,5).
    pub rate: f64,
    /// Gong vor der Ansage abspielen?
    pub gong: bool,
    /// Phonetische Aussprache-Korrekturen: Name oder Namensteil → gesprochene
    /// Schreibweise. Behebt z. B. asiatische Namen, die die deutsche/englische
    /// TTS-Stimme falsch ausspricht. Offline, kein externer Dienst.
    pub name_overrides: Vec<NameOverride>,
    /// Aussprache-Korrekturen (Basis-Wörterbuch + obige Nutzer-Einträge)
    /// überhaupt anwenden? Default an; aus = Namen werden 1:1 vorgelesen.
    pub name_overrides_enabled: bool,
    /// Mehr-Hallen-Turnier: Diese Instanz sagt NUR Spiele dieser Halle an
    /// (BTP-Location-Name). Leer = alle Hallen (Standard, Einzelhalle unberührt).
    /// So hört in einem 2-Hallen-Setup jede Halle nur ihre eigenen Ansagen.
    pub announce_hall: String,
    /// Gespeicherte Ansage-Blöcke für wiederkehrende Freitext-Ansagen
    /// (z. B. „Siegerehrung in 10 Minuten"). Werden auf der Ansagen-Seite
    /// per Knopfdruck abgespielt (wie Freitext, Halle wählbar).
    pub saved_announcements: Vec<String>,
    /// Opt-in: Eigene Aussprache-Korrekturen mit der Community-DB teilen
    /// (POST an badhub). Default aus. Das geteilte Wörterbuch wird unabhängig
    /// davon immer geladen.
    pub share_corrections: bool,
}

/// Eine Aussprache-Korrektur für die Ansage. `name` ist der ganze Name ODER ein
/// einzelner Namensteil (z. B. ein Nachname), `say` die phonetische Ersatz-
/// Schreibweise, die die TTS besser trifft (z. B. „Nguyen" → „Nwujen").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NameOverride {
    pub name: String,
    pub say: String,
}

impl Default for AnnounceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            language_mode: AnnounceLanguageMode::Auto,
            voice_de: String::new(),
            voice_en: String::new(),
            rate: 0.8,
            gong: true,
            name_overrides: Vec::new(),
            name_overrides_enabled: true,
            announce_hall: String::new(),
            saved_announcements: Vec::new(),
            share_corrections: false,
        }
    }
}

/// Einstellungen der Court-Monitor-Anzeige (TV am Spielfeld, Raspberry Pi).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CourtMonitorConfig {
    /// Ist die Court-Monitor-Anzeige eingerichtet/aktiv? Steuert nur die
    /// Sichtbarkeit der Monitor-Adressen in der Oberfläche – die
    /// Anzeige-Seite selbst ist immer erreichbar.
    pub enabled: bool,
    /// Wechsel-Intervall der Werbebilder im Leerlauf (Sekunden).
    pub ad_interval_s: i64,
    /// Disziplin in der Kopfzeile anzeigen?
    pub show_discipline: bool,
    /// Runde in der Fußzeile anzeigen?
    pub show_round: bool,
    /// Spielnummer in der Fußzeile anzeigen?
    pub show_match_number: bool,
    /// Pausen-Countdown (Retro-Klappanzeige) anzeigen?
    pub show_timer: bool,
    /// Spieldauer (Minuten, mit Stoppuhr-Symbol) in der Kopfzeile anzeigen?
    pub show_match_clock: bool,
    /// Werbung im Leerlauf anzeigen? Aus → leeres Feld zeigt die neutrale
    /// Leerlauf-Seite statt der Werbebilder.
    pub show_ads: bool,
    /// Anzeige-Layout des Monitors (`split` = „A — Geteilt"). Vorbereitet
    /// für weitere Layouts.
    pub layout: String,
    /// Kombi-Anzeige: Felder NEBENEINANDER (Hochformat je Feld) statt
    /// übereinander. Sinnvoll, wenn ein TV zwischen zwei Feldern steht.
    /// Hängt `&dir=v` an die Kombi-URL.
    pub combo_vertical: bool,
}

impl Default for CourtMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ad_interval_s: 10,
            show_discipline: true,
            show_round: true,
            show_match_number: true,
            show_timer: true,
            show_match_clock: true,
            show_ads: true,
            layout: "split".to_string(),
            combo_vertical: false,
        }
    }
}

/// Einstellungen des Aufruf-Timers (1./2./3. Aufruf). Der 1. Aufruf ist das
/// Aufrufen aufs Feld; danach zeigt bts-light je belegtem Feld eine
/// hochzählende Uhr und ab den Schwellen den 2. bzw. 3./letzten Aufruf als
/// fällig an. Die Anzeige/Logik läuft im Frontend; hier stehen nur die
/// Schwellen, damit sie über die Geräte hinweg einheitlich sind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CallTimerConfig {
    /// Aufruf-Timer aktiv?
    pub enabled: bool,
    /// Minuten nach dem 1. Aufruf, ab denen der 2. Aufruf fällig ist.
    pub second_call_minutes: f64,
    /// Minuten nach dem 1. Aufruf, ab denen der 3./letzte Aufruf fällig ist.
    pub third_call_minutes: f64,
}

impl Default for CallTimerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            second_call_minutes: 2.0,
            third_call_minutes: 4.0,
        }
    }
}

/// Einstellungen der automatischen Feldvergabe. Ist sie aktiv, weist bts-light
/// ein spielbereites Match automatisch einem freien, nicht gesperrten Feld zu,
/// sobald dieses lange genug frei ist – schreibt das wie die manuelle Vergabe
/// nach BTP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AutoAssignConfig {
    /// Automatische Feldvergabe aktiv?
    pub enabled: bool,
    /// Wartezeit in Minuten, die ein Feld frei sein muss, bevor automatisch
    /// belegt wird (verhindert Zuweisung in der kurzen Lücke zwischen Spielen).
    pub wait_minutes: f64,
    /// Mindest-Pause eines Spielers nach seinem letzten Spiel, bevor er
    /// automatisch wieder aufgerufen wird (Minuten). `0.0` = Wert aus BTP
    /// (Setting 1303) übernehmen; >0 überschreibt den BTP-Wert. Unabhängig
    /// davon wird ein Spieler nie aufgerufen, solange er gerade spielt.
    pub pause_minutes: f64,
    /// Aktive Halle (BTP-`Location`-Name) für Mehr-Hallen-Turniere, bei denen
    /// an einem Tag nur in EINER Halle gespielt wird (z. B. 2-Tage-1-Datei).
    /// Ist sie gesetzt, vergibt die Auto-Feldvergabe nur auf die Felder DIESER
    /// Halle und braucht KEINEN manuellen „in Vorbereitung"-Aufruf je Halle.
    /// Leer = alle Hallen (bei Mehr-Hallen dann wie bisher: Aufruf nötig).
    pub active_hall: String,
}

impl Default for AutoAssignConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wait_minutes: 1.0,
            pause_minutes: 0.0,
            active_hall: String::new(),
        }
    }
}

/// Eine Disziplin/Klasse→Halle-Regel (Mehr-Hallen-Turniere). Schränkt die
/// Feldvergabe ein: Spiele dieser Disziplin (bzw. genau dieser Auslosung) dürfen
/// NUR auf Felder der angegebenen Halle — manuell wie automatisch.
///
/// `draw_name` leer = **Kategorie-Default** (gilt für alle Auslosungen der
/// `discipline`); `draw_name` gesetzt = **Override** für genau diese Auslosung
/// (z. B. „HE A"), schlägt den Kategorie-Default. `discipline` ist der
/// snake_case-Schlüssel (`Discipline::as_str()`, z. B. „mens_singles").
/// `hall` = BTP-`Location`-Name; leer = Regel ohne Wirkung.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisciplineHallRule {
    pub discipline: String,
    #[serde(default)]
    pub draw_name: String,
    pub hall: String,
}

/// Turnierlogo für den badhub-Liveticker. BTP liefert kein Logo (verifiziert),
/// deshalb lädt es der Operator in den Einstellungen hoch; bts-light schickt es
/// im `tset`-Event mit, wo badhubs `#live-logo`-Element es anzeigt — genau wie
/// das Original-BTS. Leere `data` ⇒ kein Logo (Felder werden dann nicht gesendet).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LogoConfig {
    /// Base64-kodierte Bilddaten OHNE `data:`-Präfix.
    pub data: String,
    /// MIME-Typ, z. B. `image/png`.
    pub mime: String,
    /// CSS-Hintergrundfarbe hinter dem Logo (viele Logos sind transparent).
    /// Leer ⇒ badhub fällt auf sein Standard-Weiß zurück.
    pub background_color: String,
}

/// Hochwertige Cloud-Ansage über Azure Cognitive Services Speech (Neural TTS).
/// Opt-in; ist sie aus oder schlägt der Aufruf fehl, greift die lokale
/// Web-Speech-Ansage als Fallback. Schlüssel/Region aus dem Azure-Portal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AzureTtsConfig {
    /// Azure-TTS für die Ansage verwenden?
    pub enabled: bool,
    /// Azure-Region der Speech-Ressource, z. B. „westeurope".
    pub region: String,
    /// Subscription-Key (KEY 1) der Speech-Ressource.
    pub key: String,
    /// Stimme (mehrsprachig, für `<lang>`-Spans), z. B.
    /// „de-DE-SeraphinaMultilingualNeural".
    pub voice: String,
}

impl Default for AzureTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            region: String::new(),
            key: String::new(),
            voice: "de-DE-SeraphinaMultilingualNeural".to_string(),
        }
    }
}

/// Gesamte App-Konfiguration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub btp: BtpConfig,
    pub badhub: BadhubConfig,
    /// Opt-in: Diagnose-Logs automatisch an badhub.de hochladen, damit
    /// Fehler über alle Installationen hinweg auswertbar sind.
    #[serde(default)]
    pub upload_logs: bool,
    /// Zufällige, dauerhafte Installations-ID (vom Frontend erzeugt) –
    /// ordnet hochgeladene Logs einer Installation zu und ist zugleich der
    /// Namespace im Cloud-Relay.
    #[serde(default)]
    pub install_id: String,
    /// Verbindungsart für die Tablets (LAN oder Cloud). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub connection_mode: ConnectionMode,
    /// Ansage-Slave-Modus (Mehr-Hallen): diese Instanz liest nur BTP und sagt
    /// ihre Halle (`announce.announce_hall`) an — KEIN Liveticker-Push, KEINE
    /// Auto-Feldvergabe, KEIN Tablet-Server/mDNS/Relay. Für einen zweiten
    /// Rechner in der anderen Halle, der nur Ansagen macht (Master steuert).
    /// `#[serde(default)]` hält ältere Konfigurationsdateien lesbar.
    #[serde(default)]
    pub slave_mode: bool,
    /// Einstellungen der gesprochenen Feld-Ansagen. `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub announce: AnnounceConfig,
    /// Hochwertige Cloud-Ansage über Azure Neural TTS (opt-in). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub azure_tts: AzureTtsConfig,
    /// Einstellungen der Court-Monitor-Anzeige. `#[serde(default)]` hält
    /// ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub court_monitor: CourtMonitorConfig,
    /// Einstellungen des Aufruf-Timers (1./2./3. Aufruf). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub call_timer: CallTimerConfig,
    /// Einstellungen der automatischen Feldvergabe. `#[serde(default)]` hält
    /// ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub auto_assign: AutoAssignConfig,
    /// Disziplin/Klasse→Halle-Regeln (Mehr-Hallen): schränken die Feldvergabe
    /// ein (manuell + automatisch). Leer = keine Einschränkung. `#[serde(default)]`
    /// hält ältere Konfigurationsdateien lesbar.
    #[serde(default)]
    pub discipline_hall_rules: Vec<DisciplineHallRule>,
    /// Turnierlogo für den badhub-Liveticker (Upload in den Einstellungen).
    /// `#[serde(default)]` hält ältere Konfigurationsdateien lesbar.
    #[serde(default)]
    pub tournament_logo: LogoConfig,
    /// Vom Operator gesperrte Felder (CourtIDs) – werden nicht automatisch
    /// belegt. bts-light-seitig, persistiert über Neustarts. `#[serde(default)]`
    /// hält ältere Konfigurationsdateien lesbar.
    #[serde(default)]
    pub locked_courts: Vec<i64>,
    /// PIN für das Einstellungs-Menü am Zähltablett (Feldwechsel ohne QR).
    /// Reiner Bedien-Schutz gegen versehentliche Änderungen durch Helfer –
    /// KEINE Sicherheitsgrenze (der echte Kiosk-Lock liegt im Kiosk-Browser).
    /// Default „0000"; pro Verleih-Set änderbar. `#[serde(default = …)]` hält
    /// ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default = "default_tablet_settings_pin")]
    pub tablet_settings_pin: String,
}

/// Standard-PIN fürs Tablet-Einstellungsmenü (überschreibbar in der Config).
fn default_tablet_settings_pin() -> String {
    "0000".to_string()
}

impl AppConfig {
    /// Erlaubte Halle (BTP-`Location`-Name) für ein Match anhand seiner
    /// Disziplin (`Discipline::as_str()`) und Auslosung (`draw_name`).
    /// `None` = keine Einschränkung (alle Hallen erlaubt). Ein Klassen-Override
    /// (exakte `draw_name`-Regel) schlägt den Kategorie-Default.
    pub fn allowed_hall_for(&self, discipline: &str, draw_name: &str) -> Option<&str> {
        let dn = draw_name.trim();
        // 1) Klassen-Override: exakte Auslosung (draw_name) DERSELBEN Disziplin
        //    gewinnt (gleicher draw_name in zwei Disziplinen wäre sonst mehrdeutig).
        if !dn.is_empty() {
            if let Some(r) = self.discipline_hall_rules.iter().find(|r| {
                r.discipline == discipline
                    && !r.draw_name.trim().is_empty()
                    && r.draw_name.trim().eq_ignore_ascii_case(dn)
                    && !r.hall.trim().is_empty()
            }) {
                return Some(r.hall.trim());
            }
        }
        // 2) Kategorie-Default: Regel ohne draw_name für diese Disziplin.
        self.discipline_hall_rules
            .iter()
            .find(|r| {
                r.draw_name.trim().is_empty()
                    && r.discipline == discipline
                    && !r.hall.trim().is_empty()
            })
            .map(|r| r.hall.trim())
    }

    /// Darf ein Match (Disziplin + Auslosung) auf ein Feld in `court_hall`
    /// (BTP-`Location`-Name, leer = keine Halle) vergeben werden? Ohne passende
    /// Regel: immer erlaubt.
    pub fn hall_allows_match(&self, discipline: &str, draw_name: &str, court_hall: &str) -> bool {
        // Sicherung: ohne ermittelbare Hallenzuordnung (Ein-Hallen-Turnier oder
        // Feld ohne Location) NICHT blocken — sonst würde eine versehentlich
        // mitgeschleppte Regel die Vergabe lahmlegen.
        if court_hall.trim().is_empty() {
            return true;
        }
        match self.allowed_hall_for(discipline, draw_name) {
            None => true,
            Some(allowed) => court_hall.trim().eq_ignore_ascii_case(allowed),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Konfiguration konnte nicht gelesen werden: {0}")]
    Read(std::io::Error),
    #[error("Konfiguration konnte nicht geschrieben werden: {0}")]
    Write(std::io::Error),
    #[error("Konfiguration ist beschädigt: {0}")]
    Parse(#[from] serde_json::Error),
}

impl AppConfig {
    /// Lädt die Konfiguration aus einer JSON-Datei. Fehlt die Datei, wird
    /// die Default-Konfiguration zurückgegeben (erster Start).
    pub fn load_from(path: &std::path::Path) -> Result<AppConfig, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(json) => Ok(serde_json::from_str(&json)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
            Err(e) => Err(ConfigError::Read(e)),
        }
    }

    /// Schreibt die Konfiguration als JSON. Fehlende Verzeichnisse werden
    /// angelegt.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(ConfigError::Write)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).map_err(ConfigError::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_default_config() {
        let path = std::env::temp_dir().join("bts-light-does-not-exist-xyz.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(AppConfig::load_from(&path).unwrap(), AppConfig::default());
    }

    fn rule(disc: &str, draw: &str, hall: &str) -> DisciplineHallRule {
        DisciplineHallRule {
            discipline: disc.to_string(),
            draw_name: draw.to_string(),
            hall: hall.to_string(),
        }
    }

    #[test]
    fn no_rules_means_no_restriction() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.allowed_hall_for("mens_singles", "HE A"), None);
        assert!(cfg.hall_allows_match("mens_singles", "HE A", "Halle 2"));
    }

    #[test]
    fn category_default_restricts_all_draws_of_discipline() {
        let cfg = AppConfig {
            discipline_hall_rules: vec![rule("mens_singles", "", "Halle 1")],
            ..AppConfig::default()
        };
        assert_eq!(
            cfg.allowed_hall_for("mens_singles", "HE A"),
            Some("Halle 1")
        );
        assert!(cfg.hall_allows_match("mens_singles", "HE A", "Halle 1"));
        assert!(!cfg.hall_allows_match("mens_singles", "HE A", "Halle 2"));
        // Andere Disziplin bleibt unbeschränkt.
        assert!(cfg.hall_allows_match("womens_singles", "DE A", "Halle 2"));
    }

    #[test]
    fn class_override_beats_category_default() {
        // HE-Default Halle 1, aber HE C ausdrücklich Halle 2.
        let cfg = AppConfig {
            discipline_hall_rules: vec![
                rule("mens_singles", "", "Halle 1"),
                rule("mens_singles", "HE C", "Halle 2"),
            ],
            ..AppConfig::default()
        };
        assert!(cfg.hall_allows_match("mens_singles", "HE A", "Halle 1"));
        assert!(!cfg.hall_allows_match("mens_singles", "HE A", "Halle 2"));
        assert!(cfg.hall_allows_match("mens_singles", "HE C", "Halle 2"));
        assert!(!cfg.hall_allows_match("mens_singles", "HE C", "Halle 1"));
    }

    #[test]
    fn hall_match_is_case_and_space_insensitive() {
        let cfg = AppConfig {
            discipline_hall_rules: vec![rule("mixed", "", "  Halle B ")],
            ..AppConfig::default()
        };
        assert!(cfg.hall_allows_match("mixed", "MX A", "halle b"));
    }

    #[test]
    fn draw_override_is_scoped_to_its_discipline() {
        // Gleicher draw_name „A" in zwei Disziplinen, verschiedene Hallen.
        let cfg = AppConfig {
            discipline_hall_rules: vec![
                rule("mens_singles", "A", "Halle 1"),
                rule("womens_singles", "A", "Halle 2"),
            ],
            ..AppConfig::default()
        };
        assert_eq!(cfg.allowed_hall_for("mens_singles", "A"), Some("Halle 1"));
        assert_eq!(cfg.allowed_hall_for("womens_singles", "A"), Some("Halle 2"));
    }

    #[test]
    fn empty_court_hall_never_blocks() {
        // Ein-Hallen-Turnier (court_hall leer) + versehentliche Regel → nicht blocken.
        let cfg = AppConfig {
            discipline_hall_rules: vec![rule("mens_singles", "", "Halle 1")],
            ..AppConfig::default()
        };
        assert!(cfg.hall_allows_match("mens_singles", "HE A", ""));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");

        let config = AppConfig {
            btp: BtpConfig {
                host: "192.168.1.50".to_string(),
                port: 9901,
                password: Some("geheim".to_string()),
            },
            badhub: BadhubConfig {
                url: "https://badhub.de/api/live_update.php".to_string(),
                password: "token123".to_string(),
                live_url: "https://badhub.de/live?t=test".to_string(),
            },
            upload_logs: true,
            install_id: "inst-abc123".to_string(),
            connection_mode: ConnectionMode::Cloud,
            slave_mode: false,
            announce: AnnounceConfig {
                enabled: true,
                language_mode: AnnounceLanguageMode::En,
                voice_de: "voice-de-1".to_string(),
                voice_en: "voice-en-1".to_string(),
                rate: 1.1,
                gong: false,
                name_overrides: vec![NameOverride {
                    name: "Nguyen".to_string(),
                    say: "Nujen".to_string(),
                }],
                name_overrides_enabled: false,
                announce_hall: "Halle A".to_string(),
                saved_announcements: vec!["Siegerehrung in 10 Minuten".to_string()],
                share_corrections: true,
            },
            azure_tts: AzureTtsConfig {
                enabled: true,
                region: "westeurope".to_string(),
                key: "secret-key".to_string(),
                voice: "de-DE-FlorianMultilingualNeural".to_string(),
            },
            court_monitor: CourtMonitorConfig {
                enabled: true,
                ad_interval_s: 8,
                show_discipline: false,
                show_round: true,
                show_match_number: false,
                show_timer: true,
                show_match_clock: false,
                show_ads: false,
                layout: "split".to_string(),
                combo_vertical: true,
            },
            call_timer: CallTimerConfig {
                enabled: true,
                second_call_minutes: 1.5,
                third_call_minutes: 3.0,
            },
            auto_assign: AutoAssignConfig {
                enabled: true,
                wait_minutes: 0.5,
                pause_minutes: 2.0,
                active_hall: "Halle A".to_string(),
            },
            discipline_hall_rules: vec![DisciplineHallRule {
                discipline: "mens_singles".to_string(),
                draw_name: String::new(),
                hall: "Halle A".to_string(),
            }],
            locked_courts: vec![3, 7],
            tablet_settings_pin: "1234".to_string(),
            tournament_logo: LogoConfig {
                data: "aGVsbG8=".to_string(),
                mime: "image/png".to_string(),
                background_color: "#112233".to_string(),
            },
        };
        config.save_to(&path).unwrap();
        assert_eq!(AppConfig::load_from(&path).unwrap(), config);
    }

    #[test]
    fn announce_block_without_name_overrides_enabled_defaults_to_true() {
        // Upgrade-Pfad v0.9.107 → v0.9.108: announce-Block vorhanden, aber das
        // neue Feld name_overrides_enabled fehlt. #[serde(default)] am Struct
        // muss den Default aus AnnounceConfig::default() (= true) ziehen, NICHT
        // bool::default() (= false) — sonst verlören Bestandsnutzer das Feature.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "announce":{"enabled":true,"rate":0.9,"gong":true,"name_overrides":[]}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert!(
            loaded.announce.name_overrides_enabled,
            "fehlendes name_overrides_enabled muss true sein (Default-Impl)"
        );
    }

    #[test]
    fn config_without_announce_key_loads_with_defaults() {
        // Ältere config.json kennt den announce-Block nicht – er muss mit
        // den Default-Werten geladen werden, statt das Laden scheitern zu
        // lassen.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.announce, AnnounceConfig::default());
        assert!(!loaded.announce.enabled);
        assert_eq!(loaded.announce.rate, 0.8);
        assert!(loaded.announce.gong);
        // Ebenso der court_monitor-Block – ältere config.json kennt ihn nicht.
        assert_eq!(loaded.court_monitor, CourtMonitorConfig::default());
        assert!(!loaded.court_monitor.enabled);
        assert_eq!(loaded.court_monitor.ad_interval_s, 10);
        assert!(loaded.court_monitor.show_timer);
        // Ebenso der call_timer-Block – ältere config.json (vor v0.9.52) kennt
        // ihn nicht; er muss mit den Defaults laden (Auto-Update-Pfad).
        assert_eq!(loaded.call_timer, CallTimerConfig::default());
        assert!(!loaded.call_timer.enabled);
        assert_eq!(loaded.call_timer.second_call_minutes, 2.0);
        assert_eq!(loaded.call_timer.third_call_minutes, 4.0);
        // Ebenso die Auto-Feldvergabe (vor v0.9.56 unbekannt) → Defaults.
        assert_eq!(loaded.auto_assign, AutoAssignConfig::default());
        assert!(!loaded.auto_assign.enabled);
        assert_eq!(loaded.auto_assign.wait_minutes, 1.0);
        // Tablet-Einstellungs-PIN (vor diesem Feature unbekannt) → Default „0000".
        assert_eq!(loaded.tablet_settings_pin, "0000");
    }

    #[test]
    fn lan_and_cloud_mode_save_then_load_roundtrip() {
        // Der neue Doppelmodus muss verlustfrei gespeichert und geladen
        // werden – die Wire-Form ist `"lan+cloud"`.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = AppConfig {
            connection_mode: ConnectionMode::LanAndCloud,
            ..AppConfig::default()
        };
        config.save_to(&path).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(r#""connection_mode": "lan+cloud""#));
        assert_eq!(AppConfig::load_from(&path).unwrap(), config);
    }

    #[test]
    fn legacy_cloud_mode_string_still_loads() {
        // Regression: eine bestehende config.json mit "connection_mode":
        // "cloud" muss unverändert als Cloud geladen werden.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "connection_mode":"cloud"}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.connection_mode, ConnectionMode::Cloud);
        // Und ebenso "lan".
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "connection_mode":"lan"}"#,
        )
        .unwrap();
        assert_eq!(
            AppConfig::load_from(&path).unwrap().connection_mode,
            ConnectionMode::Lan
        );
    }

    #[test]
    fn connection_mode_enable_flags_truth_table() {
        // lan_enabled()/cloud_enabled() für alle drei Varianten.
        assert!(ConnectionMode::Lan.lan_enabled());
        assert!(!ConnectionMode::Lan.cloud_enabled());
        assert!(!ConnectionMode::Cloud.lan_enabled());
        assert!(ConnectionMode::Cloud.cloud_enabled());
        assert!(ConnectionMode::LanAndCloud.lan_enabled());
        assert!(ConnectionMode::LanAndCloud.cloud_enabled());
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ kaputt").unwrap();
        assert!(matches!(
            AppConfig::load_from(&path),
            Err(ConfigError::Parse(_))
        ));
    }
}
