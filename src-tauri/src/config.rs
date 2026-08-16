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
    /// Optionale manuelle Sprach-Korrektur für den Azure-`<lang>`-Pfad, falls die
    /// automatische Erkennung daneben liegt. Leer = automatisch; `"de"` = erzwingt
    /// deutschen Default (kein `<lang>`); sonst ein `NameLang` ("cn","vn","es",…).
    #[serde(default)]
    pub lang: String,
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

/// Turnierweite Anzeige-Optionen für Spielernamen (TL-Web + Tablet).
///
/// **Zentral, nicht je Gerät** (Nutzer-Entscheidung 12.08.2026): einmal im
/// Tool gesetzt, gilt für alle Turnierleitungs-Bildschirme und Tablets. Die
/// TL-Web-Seite bekommt die Werte live über den Zustand; das Tablet erhält
/// sie beim Ausliefern getemplatet (greift dort nach dem nächsten Neuladen).
///
/// Datenschutz: Der **Verein** ist wie die Nationalität ein bewusst
/// zuschaltbares, standardmäßig ausgeschaltetes Anzeige-Feld (Entscheidung
/// analog zur Nation vom 09.08.2026 — der Sieger-Monitor zeigt ihn ohnehin
/// öffentlich). Kein Geburtsjahr, keine Lizenznummer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct DisplayConfig {
    /// Vereinsnamen an den Spielernamen anzeigen?
    pub show_club_names: bool,
    /// Vereinslogos anzeigen (über den badhub-Logo-Weg `/info/club-logo`)?
    pub show_club_logos: bool,
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
    /// Minuten nach dem 1. Aufruf, ab denen ein Spiel, in dem **noch kein
    /// Punkt gefallen ist**, als überfällig gilt. Die Turnierleitungs-Seite
    /// färbt solche Felder auffällig ein.
    ///
    /// Bewusst unabhängig vom `enabled`-Schalter oben: Die Einfärbung ist
    /// eine Anzeige, kein Aufruf-Automatismus — sie soll auch in Turnieren
    /// wirken, die ohne Aufruf-Timer arbeiten.
    pub not_started_minutes: f64,
}

impl Default for CallTimerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            second_call_minutes: 2.0,
            third_call_minutes: 4.0,
            not_started_minutes: 5.0,
        }
    }
}

/// Startzeit-Prognose in der Turnierleitungs-Oberfläche (Spec
/// `docs/features/spielzeiten-prognose.md`, E7). Standardmäßig **an** —
/// reine Anzeige ohne Schreibpfad; wer sie nicht will, schaltet sie im
/// Setup ab.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PredictionConfig {
    /// Prognostizierte Startzeiten in TL-Web anzeigen?
    pub enabled: bool,
    /// Angenommene Bruttodauer eines Spiels (Minuten), solange weder die
    /// Gruppe (Klasse × Disziplin) noch Klasse oder Turnier mindestens
    /// drei Messwerte haben.
    pub default_duration_mins: f64,
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_duration_mins: 25.0,
        }
    }
}

/// Automatische Hallen-Vorverteilung (Spec
/// `docs/features/hallen-vorverteilung.md`, ADR 0029/0030). Opt-in —
/// standardmäßig aus; nur bei Mehr-Hallen-Turnieren wirksam, und niemals
/// zusammen mit einer gesetzten aktiven Halle (E2).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HallPrefillConfig {
    /// Vorverteilung aktiv?
    pub enabled: bool,
    /// Fenstergröße x — wie viele Spiele im Voraus eine Halle bekommen.
    /// 0 = automatisch (Gesamtzahl der Spielfelder, B4). Host klemmt auf
    /// 1..=120 (Wartelisten-Limit).
    pub window: u32,
}

/// Zähltafelbediener-Verwaltung (ADR 0007, Phase 1). Opt-in — standardmäßig
/// aus, damit Turniere ohne dieses Konzept unverändert laufen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScorekeeperConfig {
    /// Warteschlange der Zähltafelbediener führen (Verlierer einreihen)?
    pub enabled: bool,
    /// Garantierte Mindestpause (Sekunden) nach einem Bedien-Einsatz, bevor
    /// jemand erneut gezogen wird. Wirkt erst mit der Zuweisung (Phase 1
    /// Scheibe 2/3); hier bereits konfigurierbar. Default 300 s (Tilo).
    pub break_seconds: u64,
}

impl Default for ScorekeeperConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            break_seconds: 300,
        }
    }
}

/// Schiedsrichtermanagement (Spec `docs/features/schiedsrichter-management.md`).
/// Opt-in — standardmäßig aus, damit Turniere ohne Schiedsrichter unverändert
/// laufen und nirgends SR/AR-Bedienelemente erscheinen.
///
/// Hier liegen nur die **geräteweiten** Schalter. Alles Turnier-Spezifische
/// (feldweise Schalter, Rotationsreihenfolge, Pausen, Sperrlisten,
/// Vereins-Overrides) liegt bewusst in einer turniergebundenen Datei außerhalb
/// der config.json (ADR 0022) — Sperrlisten sind Personendaten und dürfen
/// nicht ins Identitäts-Export-Bündel wandern.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OfficialsConfig {
    /// Mit Schiedsrichtern spielen? Aus ⇒ keine SR/AR-Elemente in Client,
    /// TL-Web, Tablet und Ansagen (wie heute).
    pub enabled: bool,
    /// Automatische Rotation für Schiedsrichter (Official1)?
    pub rotation_sr: bool,
    /// Automatische Rotation für Aufschlagrichter (Official2)?
    pub rotation_ar: bool,
}

/// Einstellungen des Hallen-Check-Ins (ADR 0009). Opt-in — standardmäßig aus,
/// damit Turniere ohne Check-In unverändert laufen.
///
/// Die Anfangszeiten, Anmeldeschlüsse und Check-In-Stände liegen bewusst
/// **nicht** hier, sondern bei badhub unter der Turnier-GUID: eine
/// Installation läuft über Jahre über viele Turniere, und `AppConfig` kennt
/// keine Turnier-Trennung. Lokal steht nur, welches Turnier gerade läuft.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CheckinConfig {
    /// Hallen-Check-In für dieses Turnier aktiv?
    pub enabled: bool,
    /// turnier.de-Turnier-GUID (36 Zeichen, aus der Turnier-URL
    /// `turnier.de/tournament/<GUID>/matches`). Leer = nicht eingerichtet;
    /// ohne sie wird nichts an badhub gesendet. BTP liefert diese ID **nicht**
    /// mit, sie muss einmalig eingetragen werden.
    pub tournament_uuid: String,
    /// Bis zu wie vielen fehlenden Spielern nennt die Ansage Namen? Darüber
    /// wird nur die Anzahl angesagt („In Herrendoppel B fehlen noch 23
    /// Anmeldungen") — sonst läuft die Ansage kurz nach Fensteröffnung
    /// minutenlang, wenn noch fast niemand eingecheckt ist.
    pub missing_names_max: u32,
}

impl Default for CheckinConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tournament_uuid: String::new(),
            missing_names_max: 8,
        }
    }
}

impl CheckinConfig {
    /// Ist der Check-In einsatzbereit — eingeschaltet **und** mit gültiger
    /// Turnier-GUID? Ohne beides wird nichts gesendet und nichts angezeigt.
    pub fn is_ready(&self) -> bool {
        self.enabled && is_tournament_uuid(&self.tournament_uuid)
    }
}

/// Prüft das Format einer turnier.de-Turnier-GUID:
/// `8-4-4-4-12` Hex-Zeichen, z. B. `0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA`.
///
/// Geschweifte Klammern (BTP schreibt GUIDs als `{…}`) und Groß-/Kleinschreibung
/// sind erlaubt; die Prüfung soll den Tippfehler abfangen, nicht den Nutzer
/// über Formalien belehren.
pub fn is_tournament_uuid(value: &str) -> bool {
    let trimmed = value.trim().trim_start_matches('{').trim_end_matches('}');
    let groups: Vec<&str> = trimmed.split('-').collect();
    if groups.len() != 5 {
        return false;
    }
    let expected = [8, 4, 4, 4, 12];
    groups
        .iter()
        .zip(expected)
        .all(|(g, len)| g.len() == len && g.chars().all(|c| c.is_ascii_hexdigit()))
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
    /// „de-DE-SeraphinaMultilingualNeural". Das ist die Standard-/Hauptstimme.
    pub voice: String,
    /// Optionale Stimme **je Disziplin** (Schlüssel = Disziplin-Kürzel wie
    /// „mens_singles", Wert = Azure-Stimmenname). Leer/fehlend für eine
    /// Disziplin → es gilt `voice`. Damit lässt sich z. B. Herreneinzel/
    /// -doppel von einer männlichen und Damen-Disziplinen von einer weiblichen
    /// Stimme ansagen — frei pro Disziplin wählbar, kein Zwang.
    #[serde(default)]
    pub discipline_voices: std::collections::HashMap<String, String>,
}

impl Default for AzureTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            region: String::new(),
            key: String::new(),
            voice: "de-DE-SeraphinaMultilingualNeural".to_string(),
            discipline_voices: std::collections::HashMap::new(),
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
    /// Cloud-Ansage-Slave (Mehr-Hallen über Cloud, B1a): Namespace/Kopplungs-Code
    /// des MASTERS. Ist er gesetzt UND `slave_mode` aktiv, holt diese Instanz die
    /// Hallen-Matches + Freitext NICHT aus BTP, sondern aus dem Cloud-Relay des
    /// Masters und sagt sie lokal an. Leer = klassischer LAN-Slave (liest BTP).
    /// `#[serde(default)]` hält ältere Konfigurationsdateien lesbar.
    #[serde(default)]
    pub master_namespace: String,
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
    /// Turnierweite Anzeige-Optionen (Vereinsnamen/-logos). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub display: DisplayConfig,
    /// Einstellungen des Aufruf-Timers (1./2./3. Aufruf). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub call_timer: CallTimerConfig,
    /// Startzeit-Prognose (Spec `spielzeiten-prognose`). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub prediction: PredictionConfig,
    /// Automatische Hallen-Vorverteilung (Spec `hallen-vorverteilung`).
    /// `#[serde(default)]` hält ältere Konfigurationsdateien lesbar;
    /// Default ist **aus**.
    #[serde(default)]
    pub hall_prefill: HallPrefillConfig,
    /// Zähltafelbediener-Verwaltung (ADR 0007). `#[serde(default)]` hält
    /// ältere Konfigurationsdateien lesbar.
    #[serde(default)]
    pub scorekeeper: ScorekeeperConfig,
    /// Einstellungen der automatischen Feldvergabe. `#[serde(default)]` hält
    /// ältere Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub auto_assign: AutoAssignConfig,
    /// Hallen-Check-In (ADR 0009). `#[serde(default)]` hält ältere
    /// Konfigurationsdateien lesbar.
    #[serde(default)]
    pub checkin: CheckinConfig,
    /// Schiedsrichtermanagement (globale Schalter; Turnierdaten liegen in
    /// einer eigenen Datei, ADR 0022). `#[serde(default)]` hält ältere
    /// Konfigurationsdateien lesbar.
    #[serde(default)]
    pub officials: OfficialsConfig,
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
    /// Turnierleitungs-Oberfläche im Browser (ADR 0012/0012). `#[serde(default)]`
    /// hält ältere Konfigurationsdateien lesbar; der Default ist **aus**.
    #[serde(default)]
    pub tl_web: TlWebConfig,
    /// Raster-Anordnung der Felder je Halle (Felderübersicht + TL-Web).
    /// `#[serde(default)]` hält ältere Konfigurationsdateien lesbar; Default
    /// leer = Fließ-Darstellung ohne festes Raster.
    #[serde(default)]
    pub hall_layouts: Vec<HallLayoutConfig>,
    /// Farb-Übersteuerungen je Halle (Spec hallen-farben, ADR 0031). Bewusst
    /// eine EIGENE namensbasierte Zuordnung neben `hall_layouts` — eine Halle
    /// kann eine Farbe ohne Raster haben (und umgekehrt). Leer = alle Hallen
    /// bekommen ihre Farbe aus der Auto-Palette (`hall_colors::HALL_PALETTE`,
    /// ADR 0032). `#[serde(default)]` hält ältere Konfigurationsdateien
    /// lesbar.
    #[serde(default)]
    pub hall_colors: Vec<HallColorConfig>,
    /// A2 / ADR 0017 (Reconnect-Wahrheit): Rückfall auf das alte
    /// rev-Zähler-Verhalten. `false` (Default) = NEUES Ownership-Verhalten
    /// aktiv — nach einem Tablet-Reconnect entscheidet der Slot-Halter, wessen
    /// Stand gilt (Server/Relay liefern die Autorität im `StateRestore`).
    /// `true` = Legacy: der Server setzt `authoritative` immer auf `true` und
    /// das Tablet entscheidet wie bisher selbst per rev-Zähler — der
    /// Laufzeit-Rollback im laufenden Turnier, falls das neue Verhalten
    /// Probleme macht. Server/Relay lesen das Flag bei JEDER
    /// Reconnect-Entscheidung frisch aus der `config.json`, damit der Schalter
    /// ohne App-Neustart greift. `#[serde(default)]` hält ältere
    /// Konfigurationsdateien ohne dieses Feld lesbar.
    #[serde(default)]
    pub reconnect_legacy_rev: bool,
}

/// Standard-PIN fürs Tablet-Einstellungsmenü (überschreibbar in der Config).
fn default_tablet_settings_pin() -> String {
    "0000".to_string()
}

/// Ecke, in der die Feld-Nummerierung beginnt — aus Sicht der
/// Turnierleitung auf die Halle geschaut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutOrigin {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

/// Anordnung der Felder einer Halle als Raster. Host-Einstellung: Alle
/// Geräte zeigen dasselbe Raster — sonst meinte „das Feld links unten"
/// auf jedem Tablet ein anderes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HallLayoutConfig {
    pub hall: String,
    pub columns: u8,
    pub origin: LayoutOrigin,
    /// Richtungswechsel je Reihe (Schlangen-Nummerierung), wie Hallen
    /// mit 1-2-3 / 6-5-4 zählen. Bei `vertical` gilt sie je Spalte.
    pub serpentine: bool,
    /// Spaltenweise statt reihenweise nummerieren (Feld 1 an der Start-Ecke,
    /// Feld 2 in derselben Spalte weiter weg von der Start-Reihe, bis die
    /// Spalte voll ist, dann die nächste Spalte). `#[serde(default)]` hält
    /// Konfigurationen aus v0.9.178 (vor dieser Option) lesbar — dort galt
    /// ausschließlich die reihenweise Zählung, also `false`.
    #[serde(default)]
    pub vertical: bool,
}

/// Farb-Übersteuerung einer Halle (Spec hallen-farben, ADR 0031/0033).
/// `color` ist immer ein Palettenton als lowercase `#rrggbb` — validiert
/// am einzigen Schreibpunkt `upsert_hall_color`, damit der Draht überall
/// den Hex-Wert selbst tragen kann.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HallColorConfig {
    pub hall: String,
    pub color: String,
}

/// Ein gekoppeltes Turnierleitungs-Gerät (ADR 0012).
///
/// Das `token` ist der Zugang — **vom Turnier-PC ausgestellt**, damit die
/// `install_id` (Relay-Namespace, Log-Kennung, Host-Slot, Azure-Erbe) den
/// Master nicht verlässt. Es liegt im Klartext in der `config.json`, wie die
/// bereits dort stehenden Passwörter: Wer Zugriff auf den Turnier-PC hat, hat
/// ohnehin alles.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TlDevice {
    /// Stabile Kennung des Geräts — taucht im Protokoll auf, damit
    /// nachvollziehbar bleibt, wer was ausgelöst hat.
    pub id: String,
    /// Der Zugang. Wird nie protokolliert und nie exportiert.
    pub token: String,
    /// Was die Turnierleitung in der Geräteliste liest („Tablet Meeting
    /// Point"). Bleibt am Host — der Relay bekommt nur die Tokens.
    pub label: String,
    pub created_at_ms: u64,
    /// Optionale Bindung an eine Halle. Leer = keine Einschränkung; in dieser
    /// Stufe noch nicht erzwungen, aber vorgesehen, damit ein Helfer der
    /// zweiten Halle später nicht versehentlich in der ersten vergibt.
    pub hall: String,
    /// Gewähltes Panel-Profil (Spec tl-web-panelsystem, ADR 0025). Leer =
    /// `TlWebConfig.default_profile_id` (bzw. das eingebaute Standardprofil,
    /// wenn auch das leer ist). Kein eigenes `#[serde(default)]` nötig — der
    /// Container trägt bereits `#[serde(default)]`, das deckt fehlende
    /// Felder in jeder Tiefe ab (siehe `tl_device_without_hall_loads_unrestricted`).
    /// Wird bewusst NICHT vom Identitäts-Export mitgenommen (bleibt lokal an
    /// diesem Gerät hängen, wie Token/Label).
    pub profile_id: String,
}

/// Turnierleitungs-Oberfläche im Browser (ADR 0012/0012). Opt-in —
/// standardmäßig **aus** (`enabled: false`, keine Geräte), damit Turniere
/// ohne sie unverändert laufen.
///
/// Der Schalter ist zugleich die Sicherung des schreibenden Cloud-Pfads: Der
/// Relay kennt Turnierleitungs-Geräte **ausschließlich** über die vom Host
/// gepushten Tokens. Bleibt das Feature aus, pusht der Host nichts, die
/// Token-Zuordnung im Relay bleibt leer, und jede Anfrage endet abgewiesen,
/// bevor neuer Code Zustand berührt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TlWebConfig {
    /// Oberfläche freigeschaltet?
    pub enabled: bool,
    /// Die gekoppelten Geräte. Entfernen = Zugang entziehen; mehr braucht
    /// der Widerruf nicht.
    pub devices: Vec<TlDevice>,
    /// Der Panel-Profil-Katalog (Spec tl-web-panelsystem, ADR 0024/0025) —
    /// installationsweit, nicht turniergebunden (Profile sind
    /// geräteklassen-, keine Turnierdaten). Angelegt/bearbeitet/gelöscht
    /// wird ausschließlich über `TlAction` aus `tl.html` (R1 greift hier
    /// nicht, siehe ADR 0024), nie über den Setup-Assistenten — deshalb
    /// schützt `keep_host_managed_fields` dieses Feld wie `devices`.
    ///
    /// Kein eigenes `#[serde(default)]`: Dieser Struct trägt bereits
    /// `#[serde(default)]` auf Container-Ebene (siehe oben), das deckt jedes
    /// fehlende Feld ab — anders als `HallLayoutConfig.vertical`, dessen
    /// Struct KEINEN Container-Default hat. Ein weiteres `#[serde(default)]`
    /// hier wäre redundant.
    pub profiles: Vec<TlPanelProfile>,
    /// Turnierweiter Standard, wenn ein Gerät kein eigenes Profil gewählt
    /// hat. Leer = eingebautes Standardprofil (`tl.html` kennt es, ohne dass
    /// es hier ein Element bräuchte). Selbe Begründung wie bei `profiles`:
    /// kein eigenes `#[serde(default)]` nötig.
    pub default_profile_id: String,
}

impl TlWebConfig {
    /// Nimmt ein neu gekoppeltes Gerät auf.
    ///
    /// Fehler statt stiller Ablehnung, wenn die Liste voll ist: Der Relay
    /// verwirft eine zu lange Liste vollständig, und das bliebe sonst
    /// unbemerkt, bis niemand mehr durchkommt. Die Grenze ist die geteilte
    /// aus `relay-proto` — beide Seiten müssen dieselbe Zahl meinen.
    pub fn add_device(&mut self, device: TlDevice) -> Result<(), String> {
        if device.token.trim().is_empty() || device.id.trim().is_empty() {
            return Err("Gerät ohne Kennung oder Zugang.".to_string());
        }
        if self.devices.iter().any(|d| d.id == device.id) {
            return Err("Dieses Gerät ist schon gekoppelt.".to_string());
        }
        if self.devices.len() >= relay_proto::MAX_TL_DEVICES_MIRRORED {
            return Err(format!(
                "Mehr als {} gekoppelte Geräte kann der Relay nicht führen — \
                 bitte alte Kopplungen entfernen.",
                relay_proto::MAX_TL_DEVICES_MIRRORED
            ));
        }
        self.devices.push(device);
        Ok(())
    }

    /// Entzieht einem Gerät den Zugang. `true`, wenn es eines gab.
    ///
    /// Mehr braucht der Widerruf nicht: Der nächste Push nennt das Gerät
    /// nicht mehr, und der Relay ersetzt seine Liste damit vollständig.
    pub fn remove_device(&mut self, id: &str) -> bool {
        let vorher = self.devices.len();
        self.devices.retain(|d| d.id != id);
        self.devices.len() != vorher
    }
}

/// Seite, auf der die Warteliste/Ergebnis-Spalte im Panel-System erscheint
/// (Spec tl-web-panelsystem). Muster [`LayoutOrigin`]: Rust-Enum statt
/// String, `rename_all = "snake_case"` liefert dieselbe Wire-Form wie
/// `relay_proto::TlListPositionWire`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlListPosition {
    Right,
    Bottom,
}

impl Default for TlListPosition {
    /// Reiner Serde-Nothilfe-Wert für ein einzelnes, unvollständig
    /// gespeichertes Profil (siehe `TlDisplaySettings`) — **nicht** die
    /// fachliche Vorgabe. Das eingebaute Standardprofil (Liste rechts) lebt
    /// in `tl.html`, nicht hier.
    fn default() -> Self {
        Self::Right
    }
}

/// Ein einzelnes Panel innerhalb eines [`TlPanelProfile`]: Sichtbarkeit +
/// relative Höhe. `key` benennt den Abschnitt (`"courts"`, `"walkovers"`,
/// `"scorekeepers"`, `"officials"`, `"queue_called"`, `"queue_ready"`,
/// `"queue_waiting"`, `"queue_no_hall"`, `"finished"`) — als String statt
/// Enum, damit künftige Panels ohne Protokolländerung dazukommen können.
///
/// `#[serde(default)]` auf Container-Ebene: Ein einzelnes Profil-Element,
/// dem (z. B. nach einer künftigen Erweiterung) ein Feld fehlt, soll das
/// Laden der GANZEN `config.json` nicht zum Scheitern bringen — Muster
/// [`TlDevice`], nicht [`HallLayoutConfig`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TlPanelSetting {
    pub key: String,
    pub visible: bool,
    /// Relative Höhen-Gewichtung; clientseitig gegen ein Mindestmaß
    /// geklammert (`tl.html`), hier unbeschränkt gespeichert.
    pub height_fr: f64,
    /// Zugeklappt? **Zweite, unabhängige Dimension neben `visible`**:
    /// ausgeblendet (`visible = false`) heißt „gar nicht da", zugeklappt
    /// heißt „Kopfzeile sichtbar, Inhalt eingeklappt". Beide Zustände
    /// liegen im Profil, überstehen also den Reload.
    ///
    /// Fehlt das Feld (Bestandsprofile), greift `false` — aufgeklappt,
    /// also das bisherige Verhalten.
    pub collapsed: bool,
    /// In welcher Spalte des Mehrspalten-Layouts das Panel steht —
    /// **1-basiert**, passend zu [`TlPanelProfile::columns`].
    ///
    /// `0`/fehlend heißt „Spalte 1". Die eine Ausnahme ist ein Profil, dem
    /// auch `columns` fehlt: Dort leitet `tl.html` die ganze Aufteilung aus
    /// `display.list_position` ab („rechts" = Felder links, Rest rechts).
    /// Diese Ableitung sitzt bewusst **nur dort** — der Host reicht die
    /// Zahlen durch und kennt die Panel-Fachlichkeit nicht.
    pub column: u8,
}

/// Turnierweite Anzeige-Optionen eines Panel-Profils — dieselben Schalter,
/// die vorher als lose `localStorage`-Werte in `tl.html` lebten (Spec
/// tl-web-panelsystem). Container-`#[serde(default)]` wie
/// [`TlPanelSetting`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TlDisplaySettings {
    pub show_numbers: bool,
    pub show_nations: bool,
    pub show_club_names: bool,
    pub show_club_logos: bool,
    pub show_discipline: bool,
    pub show_round: bool,
    pub show_group: bool,
    pub list_position: TlListPosition,
}

/// Ein benanntes Panel-Profil: Panel-Sichtbarkeit/-Reihenfolge/-Höhe +
/// Anzeige-Optionen, an einem einzigen Ort statt auf drei Bedienstellen
/// verstreut (Spec tl-web-panelsystem, ADR 0024/0025). `panels`-Reihenfolge
/// = Panel-Reihenfolge auf der Seite.
///
/// Enthält **keine Personendaten** — reine Layout-/Sichtbarkeits-Angaben,
/// deshalb ohne Bedenken auf einer aus dem Internet erreichbaren Seite
/// pflegbar (ADR 0024) und ohne Bedenken im Identitäts-Export enthalten
/// (`identity_bundle` strippt dieses Feld NICHT, anders als `TlDevice`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TlPanelProfile {
    pub id: String,
    pub name: String,
    pub panels: Vec<TlPanelSetting>,
    pub display: TlDisplaySettings,
    /// Spaltenzahl des Seitenlayouts (1…3, feste Presets — kein freies
    /// Dashboard). `0`/fehlend = aus `display.list_position` ableiten:
    /// „rechts" ⇒ zwei Spalten (Felder links, alles Übrige rechts),
    /// „darunter" ⇒ eine Spalte. Damit sehen Bestandsprofile unverändert
    /// aus, ohne dass `list_position` entfernt werden müsste.
    ///
    /// Die Ableitung selbst steht **ausschließlich** in `tl.html`
    /// (`normalizedLayout()`) — hier wird nur gespeichert, was ankommt.
    pub columns: u8,
    /// Relative Spaltenbreiten, wie [`TlPanelSetting::height_fr`] bei den
    /// Panel-Höhen: leer = gleichmäßig. Geklammert wird clientseitig
    /// (`tl.html`), hier unbeschränkt gespeichert.
    pub column_widths: Vec<f64>,
    /// Last-Write-Wins-Marker — **immer** vom Host beim Speichern
    /// gestempelt (`tablet::tl::profile_save`), nie vom Client übernommen.
    pub updated_at_ms: u64,
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

    /// Legt die Raster-Anordnung einer Halle fest (oder ersetzt sie).
    ///
    /// Der Hallenname wird getrimmt gespeichert und beim Abgleich mit
    /// vorhandenen Einträgen Groß-/Kleinschreibung-unabhängig verglichen
    /// (wie `hall_allows_match`) — sonst würde aus „Halle 1 " gegenüber
    /// „halle 1" ein Duplikat statt eines Ersatzes, und `remove_hall_layout`
    /// fände die Zeile hinterher nie wieder.
    pub fn upsert_hall_layout(&mut self, mut layout: HallLayoutConfig) -> Result<(), String> {
        if layout.columns == 0 || layout.columns > 12 {
            return Err("Spaltenzahl muss zwischen 1 und 12 liegen.".to_string());
        }
        layout.hall = layout.hall.trim().to_string();
        self.hall_layouts
            .retain(|l| !l.hall.trim().eq_ignore_ascii_case(&layout.hall));
        self.hall_layouts.push(layout);
        Ok(())
    }

    /// Entfernt die Anordnung einer Halle — zurück zur Fließ-Darstellung.
    /// `true`, wenn es eine gab (trimmt + vergleicht Groß-/Kleinschreibung-
    /// unabhängig, siehe `upsert_hall_layout`).
    pub fn remove_hall_layout(&mut self, hall: &str) -> bool {
        let hall = hall.trim();
        let vorher = self.hall_layouts.len();
        self.hall_layouts
            .retain(|l| !l.hall.trim().eq_ignore_ascii_case(hall));
        self.hall_layouts.len() != vorher
    }

    /// Übersteuert die Farbe einer Halle (oder ersetzt die Übersteuerung).
    /// Gleiche Matching-Regeln wie `upsert_hall_layout`: getrimmt gespeichert,
    /// case-insensitiv ersetzt. Nur Palettentöne sind zulässig — das ist der
    /// EINZIGE Punkt mit Palettenzwang (ADR 0033), der Draht trägt danach
    /// den Hex-Wert selbst.
    pub fn upsert_hall_color(&mut self, hall: &str, color: &str) -> Result<(), String> {
        let hall = hall.trim();
        if hall.is_empty() {
            return Err("Halle darf nicht leer sein.".to_string());
        }
        let color = color.trim().to_lowercase();
        if !crate::hall_colors::HALL_PALETTE.contains(&color.as_str()) {
            return Err("Die Farbe muss ein Ton aus der Palette sein.".to_string());
        }
        self.hall_colors
            .retain(|c| !c.hall.trim().eq_ignore_ascii_case(hall));
        self.hall_colors.push(HallColorConfig {
            hall: hall.to_string(),
            color,
        });
        Ok(())
    }

    /// Entfernt die Farb-Übersteuerung einer Halle — zurück zur Auto-Palette.
    /// `true`, wenn es eine gab (Matching wie `remove_hall_layout`).
    pub fn remove_hall_color(&mut self, hall: &str) -> bool {
        let hall = hall.trim();
        let vorher = self.hall_colors.len();
        self.hall_colors
            .retain(|c| !c.hall.trim().eq_ignore_ascii_case(hall));
        self.hall_colors.len() != vorher
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
            Ok(json) => {
                let mut cfg: AppConfig = serde_json::from_str(&json)?;
                // Turnierleitungs-Geräte ohne Zugang verwerfen. Ein leeres
                // Token ist kein Gerät, sondern ein Loch: Die Autorisierung
                // schlägt eingehende Tokens in dieser Liste nach, und eine
                // Anfrage OHNE Token träfe sonst auf einen leeren Eintrag —
                // und käme als vollwertiges Turnierleitungs-Gerät durch.
                // Solche Einträge entstehen durch handgeschriebene oder halb
                // geschriebene Dateien, nicht durch die Kopplung.
                let before = cfg.tl_web.devices.len();
                cfg.tl_web.devices.retain(|d| !d.token.trim().is_empty());
                let dropped = before - cfg.tl_web.devices.len();
                if dropped > 0 {
                    tracing::warn!(
                        "{dropped} Turnierleitungs-Gerät(e) ohne Zugang aus der Konfiguration verworfen"
                    );
                }
                Ok(cfg)
            }
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
        // **Erst daneben schreiben, dann umbenennen.** Ein direktes Schreiben
        // kürzt die Datei zuerst auf null: Wer sie in diesem Augenblick liest
        // — und die Turnierleitungs-Zugänge werden bei **jeder** Anfrage
        // gelesen —, bekommt eine halbe oder leere Datei und daraus die
        // Standardwerte. Das hieße für einen Wimpernschlag: kein Gerät
        // zugelassen. Das Umbenennen ist auf beiden Dateisystemen atomar.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(ConfigError::Write)?;
        std::fs::rename(&tmp, path).map_err(ConfigError::Write)
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

    #[test]
    fn prediction_config_defaults_and_roundtrip() {
        // Alte Configs ohne `prediction`-Block laden mit Defaults (Spec
        // `spielzeiten-prognose`, E7): Prognose an, 25 Minuten Startwert.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .expect("Minimal-Config lädt");
        assert!(cfg.prediction.enabled);
        assert_eq!(cfg.prediction.default_duration_mins, 25.0);
        // … und geänderte Werte überleben den Roundtrip.
        let mut cfg = cfg;
        cfg.prediction.enabled = false;
        cfg.prediction.default_duration_mins = 18.0;
        let json = serde_json::to_string(&cfg).unwrap();
        let wieder: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(wieder.prediction, cfg.prediction);
    }

    #[test]
    fn hall_prefill_defaults_and_roundtrip() {
        // Alte Configs ohne `hall_prefill` laden mit Default AUS (Spec
        // `hallen-vorverteilung`, B3); gesetzte Werte überleben.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .expect("Minimal-Config lädt");
        assert!(!cfg.hall_prefill.enabled);
        assert_eq!(cfg.hall_prefill.window, 0, "0 = automatisch");
        let mut cfg = cfg;
        cfg.hall_prefill.enabled = true;
        cfg.hall_prefill.window = 18;
        let json = serde_json::to_string(&cfg).unwrap();
        let wieder: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(wieder.hall_prefill, cfg.hall_prefill);
    }

    #[test]
    fn hall_layouts_survive_a_config_roundtrip_and_default_empty() {
        // Alte Configs ohne das Feld laden weiter (serde default) — `btp` und
        // `badhub` sind Pflichtfelder ohne Default, deshalb minimal statt "{}"
        // (Muster aus `config_without_announce_key_loads_with_defaults`).
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .expect("Minimal-Config lädt");
        assert!(cfg.hall_layouts.is_empty());
        // … und ein gesetztes Layout überlebt Speichern + Laden.
        let mut cfg = cfg;
        cfg.hall_layouts.push(HallLayoutConfig {
            hall: "Halle 1".into(),
            columns: 3,
            origin: LayoutOrigin::BottomRight,
            serpentine: true,
            vertical: true,
        });
        let json = serde_json::to_string(&cfg).expect("serialisiert");
        let zurueck: AppConfig = serde_json::from_str(&json).expect("lädt");
        assert_eq!(zurueck.hall_layouts, cfg.hall_layouts);
    }

    /// A2 / ADR 0017: Der Reconnect-Schalter ist standardmäßig AUS
    /// (`false` = neues Ownership-Verhalten aktiv), und eine ältere
    /// `config.json` ohne das Feld bleibt lesbar und fällt auf den Default.
    #[test]
    fn reconnect_legacy_rev_defaults_off_and_old_config_stays_readable() {
        // Default: neues Verhalten aktiv.
        assert!(!AppConfig::default().reconnect_legacy_rev);
        // Alte Config ohne das Feld → serde default (false), lädt weiter.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .expect("Minimal-Config ohne Reconnect-Feld lädt");
        assert!(!cfg.reconnect_legacy_rev);
        // Explizit gesetzter Legacy-Schalter überlebt Speichern + Laden.
        let mut cfg = cfg;
        cfg.reconnect_legacy_rev = true;
        let json = serde_json::to_string(&cfg).expect("serialisiert");
        let zurueck: AppConfig = serde_json::from_str(&json).expect("lädt");
        assert!(zurueck.reconnect_legacy_rev);
    }

    /// Schiedsrichtermanagement (Spec Schritt 3): Die globalen Schalter sind
    /// standardmäßig AUS — Bestandsinstallationen verhalten sich nach dem
    /// Auto-Update unverändert — und eine ältere config.json ohne das Feld
    /// bleibt lesbar. (Der Speichern+Laden-Roundtrip gesetzter Schalter läuft
    /// in `save_then_load_roundtrip` mit.)
    #[test]
    fn officials_default_off_and_old_config_stays_readable() {
        // Default: Feature aus, keine Rotation.
        let def = OfficialsConfig::default();
        assert!(!def.enabled);
        assert!(!def.rotation_sr);
        assert!(!def.rotation_ar);
        // Alte Config ohne das Feld → serde default, lädt weiter.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .expect("Minimal-Config ohne officials-Feld lädt");
        assert_eq!(cfg.officials, OfficialsConfig::default());
    }

    /// Teilbefüllter officials-Block (z. B. von Hand editiert oder aus einer
    /// künftigen Version mit weniger Feldern): fehlende Schalter fallen auf
    /// ihren Default (aus) statt das Laden scheitern zu lassen.
    #[test]
    fn officials_block_with_missing_keys_falls_back_to_defaults() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "officials":{"enabled":true}}"#,
        )
        .expect("Config mit teilbefülltem officials-Block lädt");
        assert!(cfg.officials.enabled);
        assert!(!cfg.officials.rotation_sr);
        assert!(!cfg.officials.rotation_ar);
    }

    #[test]
    fn hall_layout_without_vertical_key_loads_as_horizontal() {
        // Upgrade-Pfad v0.9.178 → danach: Ein Raster-Eintrag ohne das neue
        // `vertical`-Feld (ältere config.json) muss weiter laden — mit dem
        // bisherigen (einzigen) Verhalten, reihenweiser Nummerierung.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "hall_layouts":[{"hall":"Halle 1","columns":3,"origin":"bottom_left","serpentine":false}]}"#,
        )
        .expect("Config mit altem Raster-Eintrag lädt");
        assert_eq!(cfg.hall_layouts.len(), 1);
        assert!(
            !cfg.hall_layouts[0].vertical,
            "fehlendes vertical muss false sein (Bestandsverhalten reihenweise)"
        );
    }

    /// Spec tl-web-panelsystem: Ein Panel-Profil (inkl. verschachtelter
    /// Panel-Liste + Anzeige-Optionen) übersteht Serialisieren + Laden
    /// unverändert.
    #[test]
    fn tl_panel_profile_serde_roundtrip() {
        let profile = TlPanelProfile {
            id: "profil-1".to_string(),
            name: "Wandmonitor Halle 2".to_string(),
            panels: vec![
                TlPanelSetting {
                    key: "courts".to_string(),
                    visible: true,
                    height_fr: 3.0,
                    collapsed: false,
                    column: 1,
                },
                TlPanelSetting {
                    key: "officials".to_string(),
                    visible: true,
                    height_fr: 1.0,
                    collapsed: true,
                    column: 2,
                },
                TlPanelSetting {
                    key: "finished".to_string(),
                    visible: false,
                    height_fr: 1.0,
                    collapsed: false,
                    column: 3,
                },
            ],
            display: TlDisplaySettings {
                show_numbers: true,
                show_nations: true,
                show_club_names: false,
                show_club_logos: false,
                show_discipline: true,
                show_round: true,
                show_group: false,
                list_position: TlListPosition::Bottom,
            },
            columns: 3,
            column_widths: vec![2.0, 1.0, 1.5],
            updated_at_ms: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&profile).expect("serialisiert");
        let back: TlPanelProfile = serde_json::from_str(&json).expect("lädt");
        assert_eq!(profile, back);
    }

    /// Ein Profil aus einer `config.json` von vor dem Mehrspalten-Layout
    /// kennt weder `columns`/`column_widths` noch `column` am Panel. Es lädt
    /// trotzdem — und zwar mit den Nullwerten, die `tl.html` als „aus
    /// `list_position` ableiten" bzw. „Spalte 1" liest. Ohne das wäre die
    /// ganze `config.json` unlesbar.
    #[test]
    fn tl_panel_profile_columns_default_to_zero_on_old_config() {
        let profile: TlPanelProfile = serde_json::from_str(
            r#"{"id":"profil-1","name":"Alt",
                "panels":[{"key":"queue","visible":true,"height_fr":2.0,"collapsed":false}],
                "display":{"list_position":"right"},
                "updated_at_ms":1}"#,
        )
        .expect("altes Profil lädt");
        assert_eq!(profile.columns, 0, "0 = aus list_position ableiten");
        assert!(profile.column_widths.is_empty(), "leer = gleichmäßig");
        assert_eq!(profile.panels[0].column, 0, "0 = Spalte 1");
    }

    /// Ein Profil aus einer `config.json` von vor dem Auf-/Zuklappen kennt
    /// `collapsed` nicht — es lädt trotzdem, und zwar aufgeklappt (das
    /// bisherige Verhalten), nicht zugeklappt.
    #[test]
    fn tl_panel_setting_collapsed_defaults_to_open_on_old_config() {
        let profile: TlPanelProfile = serde_json::from_str(
            r#"{"id":"profil-1","name":"Alt",
                "panels":[{"key":"queue","visible":true,"height_fr":2.0}],
                "updated_at_ms":1}"#,
        )
        .expect("altes Profil lädt");
        assert_eq!(profile.panels.len(), 1);
        assert!(!profile.panels[0].collapsed);
    }

    /// Ein neu gekoppeltes Gerät (bzw. eines aus einer `config.json` ohne
    /// das Feld) hat keine Profilbindung — leer heißt „Standardprofil"
    /// (ADR 0025).
    #[test]
    fn tl_device_profile_id_defaults_empty_on_old_config() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"h","port":1,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "tl_web":{"enabled":true,"devices":[
                    {"id":"d","token":"t","label":"L","created_at_ms":1}]}}"#,
        )
        .expect("Config mit altem Geräte-Eintrag ohne profile_id lädt");
        assert_eq!(cfg.tl_web.devices.len(), 1);
        assert!(cfg.tl_web.devices[0].profile_id.is_empty());
    }

    /// Fehlt der ganze `profiles`/`default_profile_id`-Block (ältere
    /// `config.json`), bleibt `tl_web` trotzdem ladbar — Container-Default
    /// von `TlWebConfig` deckt beide neuen Felder ab.
    #[test]
    fn tl_web_config_profiles_default_empty_on_missing_field() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"h","port":1,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "tl_web":{"enabled":true,"devices":[]}}"#,
        )
        .expect("Config mit tl_web-Block ohne Profil-Felder lädt");
        assert!(cfg.tl_web.profiles.is_empty());
        assert!(cfg.tl_web.default_profile_id.is_empty());
    }

    /// Eine komplette `config.json` aus einer Version vor diesem Feature
    /// (kein `tl_web`-Block überhaupt) lädt unverändert — dieselbe Prüfung
    /// wie bei anderen Feature-Einführungen (`officials_default_off_and_old_config_stays_readable`),
    /// hier über den ganzen Rollout-Weg hinweg: von „gar kein tl_web" bis
    /// „tl_web ohne die neuen Felder".
    #[test]
    fn old_config_without_profiles_stays_readable() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .expect("Minimal-Config ganz ohne tl_web lädt");
        assert!(!cfg.tl_web.enabled);
        assert!(cfg.tl_web.devices.is_empty());
        assert!(cfg.tl_web.profiles.is_empty());
        assert!(cfg.tl_web.default_profile_id.is_empty());
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
            master_namespace: String::new(),
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
                    lang: "vn".to_string(),
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
                discipline_voices: std::collections::HashMap::from([(
                    "womens_singles".to_string(),
                    "de-DE-SeraphinaMultilingualNeural".to_string(),
                )]),
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
                not_started_minutes: 6.0,
            },
            prediction: PredictionConfig {
                enabled: false,
                default_duration_mins: 18.0,
            },
            hall_prefill: HallPrefillConfig {
                enabled: true,
                window: 12,
            },
            scorekeeper: ScorekeeperConfig {
                enabled: true,
                break_seconds: 300,
            },
            auto_assign: AutoAssignConfig {
                enabled: true,
                wait_minutes: 0.5,
                pause_minutes: 2.0,
                active_hall: "Halle A".to_string(),
            },
            checkin: CheckinConfig {
                enabled: true,
                tournament_uuid: "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string(),
                missing_names_max: 5,
            },
            officials: OfficialsConfig {
                enabled: true,
                rotation_sr: true,
                rotation_ar: false,
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
            tl_web: TlWebConfig {
                enabled: true,
                devices: vec![TlDevice {
                    id: "dev-1".to_string(),
                    token: "tok-1".to_string(),
                    label: "Tablet Meeting Point".to_string(),
                    created_at_ms: 1_700_000_000_000,
                    hall: "Halle A".to_string(),
                    profile_id: "profil-1".to_string(),
                }],
                profiles: vec![TlPanelProfile {
                    id: "profil-1".to_string(),
                    name: "Wandmonitor".to_string(),
                    panels: vec![TlPanelSetting {
                        key: "courts".to_string(),
                        visible: true,
                        height_fr: 2.0,
                        collapsed: true,
                        column: 1,
                    }],
                    display: TlDisplaySettings {
                        show_numbers: true,
                        list_position: TlListPosition::Bottom,
                        ..Default::default()
                    },
                    columns: 2,
                    column_widths: vec![1.5, 1.0],
                    updated_at_ms: 1_700_000_000_500,
                }],
                default_profile_id: "profil-1".to_string(),
            },
            hall_layouts: vec![HallLayoutConfig {
                hall: "Halle A".to_string(),
                columns: 4,
                origin: LayoutOrigin::TopLeft,
                serpentine: true,
                vertical: true,
            }],
            hall_colors: vec![HallColorConfig {
                hall: "Halle A".to_string(),
                color: crate::hall_colors::HALL_PALETTE[0].to_string(),
            }],
            display: DisplayConfig {
                show_club_names: true,
                show_club_logos: true,
            },
            reconnect_legacy_rev: true,
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
        // Ebenso der Hallen-Check-In (ADR 0009) → Defaults, insbesondere aus.
        // Eine Installation, die per Auto-Update auf diese Version kommt,
        // darf ohne Zutun nichts an badhub senden.
        assert_eq!(loaded.checkin, CheckinConfig::default());
        assert!(!loaded.checkin.enabled);
        assert!(loaded.checkin.tournament_uuid.is_empty());
        assert_eq!(loaded.checkin.missing_names_max, 8);
        assert!(!loaded.checkin.is_ready());
    }

    #[test]
    fn config_without_tl_web_loads_with_the_feature_switched_off() {
        // Eine Installation, die per Auto-Update auf diese Version kommt,
        // darf die Turnierleitungs-Oberfläche NICHT stillschweigend
        // mitbringen: ohne Schalter ist der schreibende Cloud-Pfad
        // unerreichbar, weil der Relay ohne gepushte Tokens niemanden
        // hereinlässt (ADR 0011).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.tl_web, TlWebConfig::default());
        assert!(!loaded.tl_web.enabled);
        assert!(loaded.tl_web.devices.is_empty());
    }

    #[test]
    fn tl_web_devices_survive_save_and_load() {
        // Die Gerätetokens liegen am Host und müssen einen App-Neustart
        // überleben — sonst müsste die Turnierleitung nach jedem Start alle
        // Geräte neu koppeln (ADR 0012).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = AppConfig::default();
        cfg.tl_web.enabled = true;
        cfg.tl_web.devices.push(TlDevice {
            id: "dev-1".to_string(),
            token: "tok-geheim".to_string(),
            label: "Tablet Turnierleitung".to_string(),
            created_at_ms: 1_700_000_000_000,
            hall: "Halle 2".to_string(),
            profile_id: String::new(),
        });
        cfg.save_to(&path).unwrap();

        let loaded = AppConfig::load_from(&path).unwrap();
        assert!(loaded.tl_web.enabled);
        assert_eq!(loaded.tl_web.devices.len(), 1);
        assert_eq!(loaded.tl_web.devices[0].token, "tok-geheim");
        assert_eq!(loaded.tl_web.devices[0].label, "Tablet Turnierleitung");
        assert_eq!(loaded.tl_web.devices[0].hall, "Halle 2");
    }

    #[test]
    fn tl_device_without_a_token_is_dropped_on_load() {
        // Ein Eintrag ohne Zugang ist kein Gerät, sondern ein Loch: Sobald
        // die Autorisierung eingehende Tokens in dieser Liste nachschlägt,
        // passte eine Anfrage *ohne* Token auf einen leeren Eintrag und
        // käme als vollwertiges Turnierleitungs-Gerät durch — genau der
        // Zugang, den ADR 0011 absichern soll. Solche Einträge entstehen
        // durch handgeschriebene oder halb geschriebene Dateien.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"h","port":1,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "tl_web":{"enabled":true,"devices":[
                    {"id":"kaputt","label":"ohne Token","created_at_ms":1},
                    {"id":"gut","token":"tok","label":"echt","created_at_ms":2}]}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.tl_web.devices.len(), 1);
        assert_eq!(loaded.tl_web.devices[0].id, "gut");
    }

    #[test]
    fn tl_device_without_hall_loads_unrestricted() {
        // Die Hallen-Bindung ist optional (Ein-Hallen-Turniere, und der
        // Scope wird in dieser Stufe noch nicht erzwungen). Ein Eintrag ohne
        // das Feld muss lesbar bleiben statt das Laden scheitern zu lassen.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"h","port":1,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "tl_web":{"enabled":true,"devices":[
                    {"id":"d","token":"t","label":"L","created_at_ms":1}]}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.tl_web.devices.len(), 1);
        assert!(loaded.tl_web.devices[0].hall.is_empty());
    }

    #[test]
    fn checkin_is_only_ready_with_switch_and_valid_guid() {
        // Eingeschaltet allein reicht nicht — ohne GUID weiß badhub nicht,
        // zu welchem Turnier die Meldeliste gehört.
        let mut cfg = CheckinConfig {
            enabled: true,
            ..CheckinConfig::default()
        };
        assert!(!cfg.is_ready(), "ohne GUID nicht bereit");

        cfg.tournament_uuid = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string();
        assert!(cfg.is_ready());

        // Ausgeschaltet bleibt ausgeschaltet, auch mit gültiger GUID.
        cfg.enabled = false;
        assert!(!cfg.is_ready());
    }

    #[test]
    fn tournament_uuid_accepts_real_turnier_de_guids() {
        // Echte Turnier-GUID aus einer turnier.de-URL.
        assert!(is_tournament_uuid("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"));
        // Kleinschreibung, umgebende Leerzeichen und die BTP-Schreibweise
        // mit geschweiften Klammern sind dasselbe Turnier.
        assert!(is_tournament_uuid("0ea5fd86-a64f-4445-a8de-bae3dbf762ba"));
        assert!(is_tournament_uuid(
            "  0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA  "
        ));
        assert!(is_tournament_uuid("{0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA}"));
    }

    #[test]
    fn tournament_uuid_rejects_typos_and_wrong_ids() {
        assert!(!is_tournament_uuid(""));
        // Zu kurz / zu lang in einer Gruppe (klassischer Tippfehler).
        assert!(!is_tournament_uuid("0EA5FD8-A64F-4445-A8DE-BAE3DBF762BA"));
        assert!(!is_tournament_uuid("0EA5FD866-A64F-4445-A8DE-BAE3DBF762BA"));
        // Nicht-Hex-Zeichen.
        assert!(!is_tournament_uuid("0EA5FD8G-A64F-4445-A8DE-BAE3DBF762BA"));
        // Zu wenige Gruppen.
        assert!(!is_tournament_uuid("0EA5FD86-A64F-4445-A8DE"));
        // Die numerische turnier.de-Turniernummer ist NICHT die GUID.
        assert!(!is_tournament_uuid("488544"));
        // Eine ganze URL ist es auch nicht.
        assert!(!is_tournament_uuid(
            "https://www.turnier.de/tournament/0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        ));
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

#[cfg(test)]
mod tl_device_tests {
    use super::*;

    fn geraet(id: &str) -> TlDevice {
        TlDevice {
            id: id.to_string(),
            token: format!("tok-{id}"),
            label: "Tablet".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        }
    }

    #[test]
    fn a_paired_device_can_be_revoked_again() {
        // Entfernen ist der ganze Widerruf: Der nächste Push nennt das Gerät
        // nicht mehr, und der Relay ersetzt seine Liste damit vollständig.
        let mut cfg = TlWebConfig::default();
        cfg.add_device(geraet("a")).unwrap();
        cfg.add_device(geraet("b")).unwrap();
        assert!(cfg.remove_device("a"));
        assert_eq!(cfg.devices.len(), 1);
        assert!(!cfg.remove_device("a"), "zweimal entziehen ändert nichts");
    }

    #[test]
    fn pairing_the_same_device_twice_is_refused() {
        let mut cfg = TlWebConfig::default();
        cfg.add_device(geraet("a")).unwrap();
        assert!(cfg.add_device(geraet("a")).is_err());
    }

    #[test]
    fn a_device_without_access_is_no_device() {
        let mut cfg = TlWebConfig::default();
        let mut ohne = geraet("a");
        ohne.token = String::new();
        assert!(cfg.add_device(ohne).is_err());
    }

    #[test]
    fn the_list_stops_where_the_relay_stops() {
        // Der Relay verwirft eine zu lange Liste **vollständig**. Ohne diese
        // Grenze hier bliebe das unbemerkt, bis kein Gerät mehr durchkommt —
        // und auch ein Widerruf käme nicht mehr an.
        let mut cfg = TlWebConfig::default();
        for i in 0..relay_proto::MAX_TL_DEVICES_MIRRORED {
            cfg.add_device(geraet(&format!("g{i}"))).unwrap();
        }
        let err = cfg.add_device(geraet("zuviel")).unwrap_err();
        assert!(err.contains("Relay"), "sagt, woran es liegt: {err}");
    }
}

#[cfg(test)]
mod hall_layout_tests {
    use super::*;

    fn layout(hall: &str, columns: u8) -> HallLayoutConfig {
        HallLayoutConfig {
            hall: hall.to_string(),
            columns,
            origin: LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: false,
        }
    }

    #[test]
    fn upsert_replaces_an_existing_entry_by_trimmed_case_insensitive_hall_name() {
        // "Halle 1 " (Leerzeichen) und "halle 1" (Groß/klein) müssen dieselbe
        // Zeile treffen wie "Halle 1" — sonst entstünde ein Duplikat statt
        // eines Ersatzes.
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_layout(layout("Halle 1", 2)).unwrap();
        cfg.upsert_hall_layout(layout("  halle 1  ", 5)).unwrap();
        assert_eq!(cfg.hall_layouts.len(), 1, "keine Dublette");
        assert_eq!(cfg.hall_layouts[0].columns, 5, "der neue Stand gewinnt");
        assert_eq!(
            cfg.hall_layouts[0].hall, "halle 1",
            "getrimmt gespeichert (nicht Ursprungsschreibweise erzwungen)"
        );
    }

    #[test]
    fn upsert_leaves_other_halls_untouched() {
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_layout(layout("Halle 1", 2)).unwrap();
        cfg.upsert_hall_layout(layout("Halle 2", 3)).unwrap();
        assert_eq!(cfg.hall_layouts.len(), 2);
    }

    #[test]
    fn columns_boundaries_are_enforced_with_a_german_error() {
        let mut cfg = AppConfig::default();
        let err = cfg.upsert_hall_layout(layout("H", 0)).unwrap_err();
        assert_eq!(err, "Spaltenzahl muss zwischen 1 und 12 liegen.");
        assert!(
            cfg.upsert_hall_layout(layout("H", 1)).is_ok(),
            "1 ist erlaubt"
        );
        assert!(
            cfg.upsert_hall_layout(layout("H", 12)).is_ok(),
            "12 ist erlaubt"
        );
        let err = cfg.upsert_hall_layout(layout("H", 13)).unwrap_err();
        assert_eq!(err, "Spaltenzahl muss zwischen 1 und 12 liegen.");
    }

    #[test]
    fn remove_finds_the_hall_regardless_of_case_and_whitespace() {
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_layout(layout("Halle 1", 2)).unwrap();
        assert!(cfg.remove_hall_layout("  HALLE 1 "));
        assert!(cfg.hall_layouts.is_empty());
    }

    #[test]
    fn remove_reports_false_for_an_unknown_hall() {
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_layout(layout("Halle 1", 2)).unwrap();
        assert!(!cfg.remove_hall_layout("Halle 2"));
        assert_eq!(cfg.hall_layouts.len(), 1, "unbekannte Halle ändert nichts");
    }
}

#[cfg(test)]
mod hall_color_tests {
    use super::*;
    use crate::hall_colors::HALL_PALETTE;

    #[test]
    fn hall_colors_survive_a_config_roundtrip_and_default_empty() {
        // Alte Configs ohne das Feld müssen lesbar bleiben (Auto-Update!) —
        // und gespeicherte Übersteuerungen den Neustart überleben.
        let mut json: serde_json::Value =
            serde_json::to_value(AppConfig::default()).expect("Default serialisiert");
        json.as_object_mut()
            .unwrap()
            .remove("hall_colors")
            .expect("Feld existiert im neuen Schema");
        let alt: AppConfig = serde_json::from_value(json).expect("alte Config lädt");
        assert!(
            alt.hall_colors.is_empty(),
            "Default ist leer = Auto-Palette"
        );

        let mut cfg = AppConfig::default();
        cfg.upsert_hall_color("Nord", HALL_PALETTE[3]).unwrap();
        let json = serde_json::to_string(&cfg).unwrap();
        let wieder: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(wieder.hall_colors, cfg.hall_colors);
    }

    #[test]
    fn upsert_hall_color_trims_and_replaces_case_insensitive() {
        // "Halle 1 " und "halle 1" müssen dieselbe Zeile treffen — sonst
        // entstünde ein Duplikat und remove fände die Zeile nie wieder
        // (dasselbe Muster wie upsert_hall_layout).
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_color("Halle 1", HALL_PALETTE[0]).unwrap();
        cfg.upsert_hall_color("  halle 1  ", HALL_PALETTE[4])
            .unwrap();
        assert_eq!(cfg.hall_colors.len(), 1, "keine Dublette");
        assert_eq!(
            cfg.hall_colors[0].color, HALL_PALETTE[4],
            "neuer Stand gewinnt"
        );
        assert_eq!(cfg.hall_colors[0].hall, "halle 1", "getrimmt gespeichert");
    }

    #[test]
    fn upsert_hall_color_rejects_a_tone_outside_the_palette() {
        // Der Palettenzwang gilt am einzigen Schreibpunkt (ADR 0033) — auch
        // ein gültiges Hex außerhalb der Palette wird abgelehnt.
        let mut cfg = AppConfig::default();
        let err = cfg.upsert_hall_color("Nord", "#123456").unwrap_err();
        assert!(err.contains("Palette"), "deutsche Fehlermeldung: {err}");
        assert!(cfg.hall_colors.is_empty(), "nichts gespeichert");
        let err = cfg.upsert_hall_color("Nord", "rot").unwrap_err();
        assert!(err.contains("Palette"), "auch kein freier Name: {err}");
    }

    #[test]
    fn upsert_hall_color_accepts_palette_tones_case_insensitive() {
        // tl.html/React reichen den Ton durch — eine Großschreibung aus
        // fremder Quelle darf nicht an der Validierung scheitern, gespeichert
        // wird normalisiert lowercase (ADR 0033).
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_color("Nord", &HALL_PALETTE[1].to_uppercase())
            .unwrap();
        assert_eq!(cfg.hall_colors[0].color, HALL_PALETTE[1]);
    }

    #[test]
    fn upsert_hall_color_rejects_an_empty_hall_name() {
        let mut cfg = AppConfig::default();
        let err = cfg.upsert_hall_color("   ", HALL_PALETTE[0]).unwrap_err();
        assert!(err.contains("Halle"), "deutsche Fehlermeldung: {err}");
    }

    #[test]
    fn remove_hall_color_matches_trimmed_case_insensitive() {
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_color("Halle 1", HALL_PALETTE[0]).unwrap();
        assert!(cfg.remove_hall_color("  HALLE 1 "));
        assert!(cfg.hall_colors.is_empty());
        assert!(
            !cfg.remove_hall_color("Halle 1"),
            "zweites Entfernen: false"
        );
    }
}
