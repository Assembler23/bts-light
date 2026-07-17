//! Geteilte JSON-Wire-Typen für den digitalen Tablet-Spielzettel.
//!
//! Zwei Verbindungs-Ebenen nutzen diese Typen:
//!
//! 1. **Tablet ↔ Server** ([`TabletMsg`], [`ServerMsg`], [`ResultBody`],
//!    [`ResultResponse`]). „Server" ist im LAN-Modus der eingebettete
//!    Server in bts-light, im Cloud-Modus der Relay. Die Wire-Form ist in
//!    beiden Fällen identisch – das Tablet (`tablet.html`) merkt keinen
//!    Unterschied.
//! 2. **bts-light-Host ↔ Relay** ([`HostFrame`], [`RelayFrame`]). Der
//!    Relay multiplext mehrere Tablets über eine einzige Host-Verbindung,
//!    deshalb trägt hier jedes Frame eine Feld-Identität.
//!
//! **Feld-Identität:** Jedes court-bezogene Frame trägt `courtId` (die
//! stabile BTP-CourtID, `i64`) als Identität und `courtLabel` (den
//! Feldnamen) nur noch für die Anzeige. Feldnamen wiederholen sich bei
//! Mehr-Hallen-Turnieren – die CourtID nicht. Alle `courtId`-Felder tragen
//! `#[serde(default)]`, damit ältere Relays/Clients ohne dieses Feld noch
//! deserialisieren (sie fallen dann auf CourtID 0 zurück).
//!
//! Beim Verändern der Renames aufpassen: `tablet.html` und der
//! verifizierte LAN-Pfad hängen exakt an dieser Wire-Form.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ─────────────────────────── Gemeinsame Bausteine ─────────────────────────

/// Ein Satz-Ergebnis als Punkte (Team A, Team B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAb {
    pub a: i64,
    pub b: i64,
}

/// Ein Spieler einer Paarung, wie ihn das Tablet anzeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerBrief {
    pub id: i64,
    pub name: String,
    /// Nationalität als ISO-/IOC-Code (z. B. "GER") – Grundlage der
    /// Landesflagge auf dem Court-Monitor. `#[serde(default)]` hält
    /// ältere Frames ohne dieses Feld lesbar.
    #[serde(default)]
    pub nationality: Option<String>,
}

/// Match-Kurzinfo fürs Tablet (Schema wie bei badhub-tournament).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchBrief {
    #[serde(rename = "matchId")]
    pub match_id: i64,
    #[serde(rename = "teamA")]
    pub team_a: Vec<PlayerBrief>,
    #[serde(rename = "teamB")]
    pub team_b: Vec<PlayerBrief>,
    #[serde(rename = "eventLabel")]
    pub event_label: String,
    #[serde(rename = "bestOfSets")]
    pub best_of_sets: i64,
    #[serde(rename = "targetScore")]
    pub target_score: i64,
    /// Maximalpunktzahl/Cap des Satzes (z. B. 30 bei 21, 21 bei 15). Bei
    /// Gleichstand wird bis dahin gespielt, dann gewinnt der Führende.
    /// `#[serde(default)]` hält ältere Frames lesbar (0 → Tablet-Fallback).
    #[serde(rename = "capScore", default)]
    pub cap_score: i64,
    /// Punktestand, bei dem die Intervall-Pause (60 s) ausgelöst wird; `None`
    /// = keine reguläre Intervall-Pause je Satz. `#[serde(default)]` hält
    /// ältere Frames lesbar.
    #[serde(rename = "intervalAt", default)]
    pub interval_at: Option<i64>,
    /// Disziplin als snake_case-Schlüssel (`mens_singles`, `mixed`, …;
    /// leer = unbekannt) – der Court-Monitor lokalisiert ihn selbst.
    /// `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(default)]
    pub discipline: String,
    /// Klassen-Kürzel („A", „B", …) für die Ansage „Herreneinzel A" am
    /// Cloud-Slave. Leer = keine Klasse erkennbar. `#[serde(default)]` +
    /// `skip_serializing_if` halten alte Relays/Clients kompatibel.
    #[serde(
        rename = "classLabel",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub class_label: String,
    /// Spielnummer (BTP `MatchNr`), falls vergeben – für die Monitor-Fußzeile.
    #[serde(rename = "matchNumber", default)]
    pub match_number: Option<i64>,
    /// Voraussichtlicher Zähltafelbediener fürs nächste Spiel: die Namen des
    /// Verlierer-Teams des zuletzt auf diesem Feld beendeten Spiels. Wird dem
    /// Tablet bei der Seitenwahl als Hinweis angezeigt. Leer, wenn es kein
    /// Vorspiel auf dem Feld gab. `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(default)]
    pub scorekeeper: Vec<String>,
}

// ─────────────────────────── Court-Monitor ────────────────────────────────
//
// Die read-only TV-Anzeige am Spielfeld (`monitor.html`) pollt `…/state`
// und bekommt diesen [`MonitorState`]. LAN-Server und Relay erzeugen ihn
// identisch, damit der Monitor in beiden Modi dieselbe Seite ist.

/// Ein Spieler in der Monitor-Anzeige: Name + Nationalität (für die Flagge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorPlayer {
    /// Kombinierter Anzeigename ("Vorname Nachname").
    pub name: String,
    /// Vorname(n) – getrennt geführt, damit der Court-Monitor Vor- und
    /// Nachnamen exakt im Broadcast-Stil darstellen kann. `#[serde(default)]`
    /// hält ältere Relays/Clients ohne dieses Feld lesbar; der Monitor fällt
    /// dann auf eine Aufteilung von `name` zurück.
    #[serde(default)]
    pub given: String,
    /// Nachname – getrennt geführt, siehe `given`.
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub nationality: Option<String>,
}

/// Das aktuelle Match eines Feldes für die Monitor-Anzeige.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorMatch {
    #[serde(rename = "matchId")]
    pub match_id: i64,
    /// Disziplin als snake_case-Schlüssel; der Monitor lokalisiert selbst.
    pub discipline: String,
    /// Auslosung + Runde, z. B. "HE G1" – für die Fußzeile.
    #[serde(rename = "eventLabel")]
    pub event_label: String,
    #[serde(rename = "matchNumber", default)]
    pub match_number: Option<i64>,
    pub team1: Vec<MonitorPlayer>,
    pub team2: Vec<MonitorPlayer>,
    /// Satzstand in Team-Koordinaten (abgeschlossene Sätze + laufender Satz).
    pub sets: Vec<SetAb>,
}

/// Anzeige-Optionen des Court-Monitors (vom Tool gesetzt).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(rename = "adIntervalS")]
    pub ad_interval_s: i64,
    #[serde(rename = "showDiscipline")]
    pub show_discipline: bool,
    #[serde(rename = "showRound")]
    pub show_round: bool,
    #[serde(rename = "showMatchNumber")]
    pub show_match_number: bool,
    #[serde(rename = "showTimer")]
    pub show_timer: bool,
    /// Spieldauer (Minuten) in der Kopfzeile anzeigen?
    #[serde(rename = "showMatchClock", default = "default_true")]
    pub show_match_clock: bool,
    /// Werbung im Leerlauf anzeigen? Aus → leeres Feld zeigt die neutrale
    /// Leerlauf-Seite statt der Werbebilder.
    #[serde(rename = "showAds", default = "default_true")]
    pub show_ads: bool,
    /// Anzeige-Layout (`split` = „A — Geteilt").
    #[serde(default = "default_layout")]
    pub layout: String,
}

fn default_true() -> bool {
    true
}

fn default_layout() -> String {
    "split".to_string()
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            ad_interval_s: 10,
            show_discipline: true,
            show_round: true,
            show_match_number: true,
            show_timer: true,
            show_match_clock: true,
            show_ads: true,
            layout: default_layout(),
        }
    }
}

/// Ein hochgeladenes Werbebild – Base64-kodiert, damit es in ein
/// JSON-Frame passt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdUpload {
    #[serde(rename = "contentType")]
    pub content_type: String,
    /// Bilddaten, Base64 (Standard-Alphabet).
    pub data: String,
}

/// Court-Monitor-Datensatz, den der bts-light-Host zum Relay hochlädt –
/// damit Cloud-Monitore Werbung und Anzeige-Konfiguration bekommen.
// Kein `Eq`: enthält über `call_timer` f64-Felder (CallTimerView).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorUpload {
    pub config: MonitorConfig,
    #[serde(rename = "tournamentName", default)]
    pub tournament_name: String,
    pub ads: Vec<AdUpload>,
    /// Aufruf-Timer-Schwellen (1./2./3. Aufruf) – damit der Relay sie beim
    /// Bauen des MonitorState mitschickt. `#[serde(default)]` (= aus) hält
    /// ältere Host-Uploads lesbar.
    #[serde(rename = "callTimer", default)]
    pub call_timer: CallTimerView,
}

/// Was ein Court-Monitor-Gerät anzeigen soll – per Gerät zugewiesen.
/// Zuweisungs-Ziel eines Court-Monitor-Geräts. Drei große Familien:
/// 1. **Court** – klassisch ein bestimmtes Feld
/// 2. **Info** – Hallen-weites Info-Display (Übersicht / In Vorbereitung)
/// 3. **Ad** – dedizierte Werbe-Anzeige (rotierend oder Einzelbild)
///
/// JSON-Form (`#[serde(tag = "kind")]`):
/// - `{"kind":"court","court_id":5}`
/// - `{"kind":"info_overview"}`
/// - `{"kind":"info_preparation"}`
/// - `{"kind":"ad_rotation"}`
/// - `{"kind":"ad_single","file":"sommerfest.jpg"}`
///
/// `Copy` ist seit dem Ad-Single-Variant (String) nicht mehr ableitbar;
/// wo bisher `.copied()` reichte, ist es jetzt `.cloned()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MonitorTarget {
    /// Klassischer Court-Monitor für ein bestimmtes Feld.
    Court {
        #[serde(rename = "court_id")]
        court_id: i64,
    },
    /// Hallen-Übersicht (`/info/overview`). `hall = Some(name)` bindet den
    /// Monitor fest an eine Halle (`?halle=…`, ein Pi je Halle); `None` =
    /// alle Hallen (rotiert bei mehreren). `skip_serializing_if` hält die
    /// JSON-Form bei `None` exakt wie früher (`{"kind":"info_overview"}`) →
    /// alte gespeicherte Zuweisungen bleiben lesbar.
    InfoOverview {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hall: Option<String>,
    },
    /// Spiele-in-Vorbereitung-Liste (`/info/preparation`).
    InfoPreparation,
    /// Sieger-/Podium-Anzeige ausgespielter Disziplinen (`/info/winners`).
    /// `rank = None` → ganzes Podium auf einem Monitor; `Some(1|2|3)` → nur
    /// dieser Rang (drei TVs vor dem physischen Podest, je ein Platz).
    /// `skip_serializing_if` hält die JSON-Form bei `None` exakt wie früher
    /// (`{"kind":"info_winners"}`) → alte Zuweisungen bleiben lesbar.
    InfoWinners {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rank: Option<u8>,
    },
    /// Werbung: alle hinterlegten Werbebilder rotierend.
    AdRotation,
    /// Werbung: ein bestimmtes Werbebild, dauerhaft.
    AdSingle { file: String },
    /// Kombi-Anzeige: Spielstände mehrerer Felder (1-3) gleichzeitig auf
    /// einem Bildschirm, als horizontale Bänder.
    CourtCombo {
        #[serde(rename = "court_ids")]
        court_ids: Vec<i64>,
    },
}

impl MonitorTarget {
    /// Court-Konstruktor zur Bequemlichkeit.
    pub fn court(court_id: i64) -> Self {
        Self::Court { court_id }
    }

    /// Ad-Single-Konstruktor zur Bequemlichkeit.
    pub fn ad_single(file: impl Into<String>) -> Self {
        Self::AdSingle { file: file.into() }
    }

    /// Kombi-Konstruktor zur Bequemlichkeit.
    pub fn court_combo(court_ids: Vec<i64>) -> Self {
        Self::CourtCombo { court_ids }
    }

    /// CourtID, falls dieses Target ein Feld ist; sonst `None`.
    pub fn court_id(&self) -> Option<i64> {
        match self {
            Self::Court { court_id } => Some(*court_id),
            _ => None,
        }
    }

    /// Pfad+Query, zu dem ein Nicht-Court-Target umleitet (für
    /// `MonitorState.redirect_to`). Bei `Court` `None` (keine Umleitung,
    /// normale Monitor-Seite). Ad-Targets kommen mit Query, damit die
    /// Anzeige-Seite weiß, welches Bild bzw. Rotation gemeint ist.
    pub fn redirect_path(&self) -> Option<String> {
        match self {
            Self::Court { .. } => None,
            Self::InfoOverview { hall } => Some(match hall {
                Some(h) => format!("/info/overview?halle={}", url_encode(h)),
                None => "/info/overview".to_string(),
            }),
            Self::InfoPreparation => Some("/info/preparation".to_string()),
            Self::InfoWinners { rank } => Some(match rank {
                Some(r) => format!("/info/winners?only={r}"),
                None => "/info/winners".to_string(),
            }),
            Self::AdRotation => Some("/info/ad?mode=rotation".to_string()),
            Self::AdSingle { file } => {
                // Dateiname URL-escapen (Punkte/Bindestriche/Unterstriche
                // bleiben unverändert, alles andere ist eh nicht erlaubt
                // dank `is_safe_image_name`).
                Some(format!("/info/ad?mode=single&file={}", url_encode(file)))
            }
            Self::CourtCombo { court_ids } => {
                // CourtIDs als kommaseparierte Query (?courts=1,2,3).
                let csv = court_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                Some(format!("/combo?courts={csv}"))
            }
        }
    }

    /// Kurz-Schlüssel – gleich dem serde-Tag. Für UI-Logik und Debug.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Court { .. } => "court",
            Self::InfoOverview { .. } => "info_overview",
            Self::InfoPreparation => "info_preparation",
            Self::InfoWinners { .. } => "info_winners",
            Self::AdRotation => "ad_rotation",
            Self::AdSingle { .. } => "ad_single",
            Self::CourtCombo { .. } => "court_combo",
        }
    }
}

/// Minimaler URL-Encoder fürs Werbebild-Query. Akzeptiert ASCII-
/// alphanumerisch + `.`, `-`, `_` 1:1 (das deckt alle nach
/// `is_safe_image_name` erlaubten Zeichen ab); alles andere als `%HH`.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.bytes() {
        match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => out.push(c as char),
            _ => out.push_str(&format!("%{c:02X}")),
        }
    }
    out
}

/// Vollständiger Anzeige-Zustand eines Feldes, den `monitor.html` pollt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorState {
    /// Stabile BTP-CourtID des angezeigten Felds (Identität).
    #[serde(rename = "courtId", default)]
    pub court_id: i64,
    /// Feldname (Anzeige), z. B. „1" oder „Feld 3".
    #[serde(rename = "courtLabel")]
    pub court_label: String,
    #[serde(rename = "tournamentName", default)]
    pub tournament_name: String,
    /// Aktuelles Match, oder `null` wenn das Feld frei ist (→ Werbemodus).
    #[serde(rename = "match", skip_serializing_if = "Option::is_none", default)]
    pub match_info: Option<MonitorMatch>,
    /// Roher Tablet-Spielzustand (`court_state`) als JSON-String, falls ein
    /// Tablet das Feld zählt – liefert Aufschlag-Seite und Pause/Timer.
    /// `monitor.html` parst ihn selbst.
    #[serde(
        rename = "courtState",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub court_state: Option<String>,
    pub config: MonitorConfig,
    /// Kennungen der Werbebilder; der Monitor lädt sie über `../../ads/<id>`.
    pub ads: Vec<String>,
    /// Auszuführender Fernbefehl (Neu laden / Identifizieren) – nur im
    /// Geräte-Modus gesetzt. `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(rename = "command", skip_serializing_if = "Option::is_none", default)]
    pub command: Option<MonitorCommand>,
    /// Kurz-Code des Geräts (für die Kopplungs-Anzeige). Nur Geräte-Modus.
    #[serde(rename = "deviceCode", default)]
    pub device_code: String,
    /// `true`, wenn das Gerät noch keinem Feld zugewiesen ist → der Monitor
    /// zeigt die Kopplungs-Seite statt einer Match-/Werbe-Ansicht.
    #[serde(default)]
    pub unassigned: bool,
    /// Pfad, zu dem die Monitor-Seite navigieren soll. Wird gesetzt, wenn
    /// das Gerät neuerdings als Info-Monitor (`/info/overview` oder
    /// `/info/preparation`) zugewiesen wurde, der TV aber noch die
    /// Feld-Seite (`monitor.html`) zeigt. `monitor.html` macht dann ein
    /// `location.href = redirect_to` und lädt die richtige Info-HTML.
    /// Bei Feld-Zuweisung leer/none. `#[serde(default)]` hält ältere
    /// Frames lesbar.
    #[serde(
        rename = "redirectTo",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub redirect_to: Option<String>,
    /// Server-Zeit (ms seit Epoch) zum Zeitpunkt des Polls. Der Pausen-
    /// Countdown im `courtState` trägt ein absolutes `endsAt` (mit der
    /// Uhr des zählenden Tablets gesetzt). Der TV (Pi) hat aber oft keine
    /// synchrone Uhr (kein RTC, evtl. kein NTP im Turnier-WLAN) → `endsAt
    /// - Date.now()` weicht um den Uhren-Drift ab (z. B. +1 min). Mit
    /// `server_now_ms` rechnet `monitor.html` die Restzeit relativ zur
    /// Server-Uhr statt zur eigenen → Pi-Drift eliminiert. `default` (0)
    /// = altes Frame, dann fällt der TV auf `Date.now()` zurück.
    #[serde(rename = "serverNowMs", default)]
    pub server_now_ms: u64,
    /// Zeitpunkt (Unix-ms) des 1. Aufrufs = seit wann das Spiel auf dem Feld
    /// steht; `None` = kein Spiel. Grundlage der Aufruf-Uhr am Monitor.
    /// `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(
        rename = "onCourtSinceMs",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub on_court_since_ms: Option<u64>,
    /// Aufruf-Timer (1./2./3. Aufruf) – Schwellen für die Monitor-Anzeige.
    /// `#[serde(default)]` (= aus) hält ältere Frames lesbar.
    #[serde(rename = "callTimer", default)]
    pub call_timer: CallTimerView,
}

/// Aufruf-Timer-Einstellungen für die Monitor-Anzeige (gespiegelt aus der
/// App-Config). Der Monitor rechnet die hochzählende Uhr und den fälligen
/// Aufruf selbst aus `on_court_since_ms` + `server_now_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CallTimerView {
    #[serde(default)]
    pub enabled: bool,
    #[serde(rename = "secondCallMinutes", default)]
    pub second_call_minutes: f64,
    #[serde(rename = "thirdCallMinutes", default)]
    pub third_call_minutes: f64,
}

/// Art eines Fernbefehls an einen Court-Monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorCommandKind {
    /// Seite neu laden.
    Reload,
    /// Feldnummer + Code groß einblenden (zum Zuordnen Gerät ↔ TV).
    Identify,
}

/// Ein Fernbefehl an einen Monitor. `id` zählt je Gerät hoch; der Monitor
/// führt einen Befehl genau einmal aus (er merkt sich die zuletzt
/// ausgeführte `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorCommand {
    pub id: u64,
    pub kind: MonitorCommandKind,
}

/// Ein Monitor-Gerät, wie es die „Court-Monitore"-Seite im Tool zeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorDeviceInfo {
    /// Stabile Geräte-ID (UUID, vom Monitor selbst erzeugt).
    pub id: String,
    /// Kurz-Code (erste Zeichen der ID), wie ihn der TV anzeigt.
    pub code: String,
    /// CourtID des zugewiesenen Felds (Identität), falls eines gesetzt ist.
    /// `None` bei nicht zugewiesenen Geräten **oder** wenn das Gerät einem
    /// Info-Display zugewiesen ist (dann steht der Typ in `target`).
    #[serde(rename = "courtId", default)]
    pub court_id: Option<i64>,
    /// Feldname (Anzeige) des zugewiesenen Felds, falls eines gesetzt ist.
    #[serde(default)]
    pub court: Option<String>,
    /// Vollständige Geräte-Zuweisung (Feld ODER Info-Display). `None` =
    /// nicht zugewiesen. `#[serde(default)]` hält ältere Frames lesbar
    /// (Cloud-Relay-Versionen ohne Info-Monitor-Konzept liefern hier
    /// nichts → das Frontend behandelt sie als reine Feld-Zuweisungen
    /// auf Basis von `court_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<MonitorTarget>,
    /// Hat sich das Gerät zuletzt gemeldet (kürzlich gepollt)?
    pub online: bool,
    /// Vom Operator explizit gewählte Halle (Hallenname) für dieses Gerät –
    /// überschreibt die aus dem zugewiesenen Feld abgeleitete Halle. Nötig für
    /// Geräte ohne Feld (unzugewiesen, Info-/Werbe-/Kombi-Monitore), damit sie
    /// bei Mehr-Hallen-Turnieren einer Halle zugeordnet werden können. Wird
    /// host-seitig angehängt (`monitor-halls.json`); `None` = keine explizite
    /// Wahl → Halle folgt dem Feld. `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hall: Option<String>,
}

/// Steuerdaten, die der bts-light-Host zum Relay schickt: Feld-Zuweisungen
/// und offene Fernbefehle. Klein und ohne Bilddaten – darf häufig gepusht
/// werden (anders als [`MonitorUpload`] mit den Werbebildern).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorControl {
    /// Geräte-ID → CourtID des zugewiesenen Felds.
    #[serde(default)]
    pub assignments: HashMap<String, i64>,
    /// Geräte-ID → offener Fernbefehl.
    pub commands: HashMap<String, MonitorCommand>,
}

/// Ein Gerät gilt als „online", wenn sein letzter Poll höchstens so lange
/// her ist (der Monitor pollt im Sekundentakt). Großzügig (20 s), damit ein
/// kurzer WLAN-Zucker den Online-Status NICHT flackern lässt – im flakigen
/// Hallen-/Verleih-WLAN sind einzelne >6-s-Aussetzer normal. Ein wirklich
/// totes Gerät fällt weiterhin nach 20 s raus.
pub const MONITOR_ONLINE_WINDOW_MS: u64 = 20_000;

/// Kurz-Code eines Geräts: die **letzten** vier alphanumerischen Zeichen der
/// ID, groß – so wie der Monitor ihn auf dem TV anzeigt.
///
/// Bewusst das Ende, nicht der Anfang: Pi-Monitore melden sich als
/// `pi-<CPU-Serial>`, und alle Raspberry-Pi-Serials beginnen mit demselben
/// Präfix (`00000000…`/`10000000…`). Die ersten vier Zeichen wären deshalb für
/// jeden Pi identisch („PI00") – die unterscheidende Entropie der Serial steht
/// am Ende. Der Code ist reine Anzeige + Sortier-Tiebreak (kein Identitäts-
/// Schlüssel – der ist die volle `device_id`).
pub fn device_code(device_id: &str) -> String {
    let alnum: Vec<char> = device_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let start = alnum.len().saturating_sub(4);
    alnum[start..]
        .iter()
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Baut die Monitor-Geräteliste für die „Court-Monitore"-Seite aus den
/// Geräte-Zuweisungen (Geräte-ID → [`MonitorTarget`]) und dem Live-Zustand
/// (`seen`: Geräte-ID → Zeitpunkt des letzten Polls in ms). `court_names`
/// löst die CourtID einer Feld-Zuweisung auf einen Anzeigenamen auf –
/// fehlt eine ID darin, bleibt der Anzeigename leer (das Gerät bleibt
/// trotzdem in der Liste). Sortiert nach Feldname, dann Code – noch nicht
/// zugewiesene Geräte (`court = None`) stehen damit zuerst, weil `None`
/// vor `Some(_)` sortiert.
pub fn build_device_list(
    assignments: &HashMap<String, MonitorTarget>,
    court_names: &HashMap<i64, String>,
    seen: &HashMap<String, u64>,
    now_ms: u64,
) -> Vec<MonitorDeviceInfo> {
    let mut ids: Vec<&String> = assignments.keys().collect();
    for id in seen.keys() {
        if !assignments.contains_key(id) {
            ids.push(id);
        }
    }
    let mut out: Vec<MonitorDeviceInfo> = ids
        .into_iter()
        .map(|id| {
            let last_seen = seen.get(id).copied().unwrap_or(0);
            let target = assignments.get(id).cloned();
            let court_id = target.as_ref().and_then(|t| t.court_id());
            MonitorDeviceInfo {
                id: id.clone(),
                code: device_code(id),
                court_id,
                court: court_id.and_then(|cid| court_names.get(&cid).cloned()),
                target,
                online: last_seen > 0
                    && now_ms.saturating_sub(last_seen) <= MONITOR_ONLINE_WINDOW_MS,
                // Explizite Halle hängt der Host nachträglich an (monitor_devices).
                hall: None,
            }
        })
        .collect();
    out.sort_by(|a, b| a.court.cmp(&b.court).then(a.code.cmp(&b.code)));
    out
}

/// Vereint zwei Monitor-Gerätelisten zu einer – für den Doppelmodus
/// (`LanAndCloud`), in dem die „Court-Monitore"-Seite die lokal gebaute
/// LAN-Liste und die vom Relay gemeldete Cloud-Liste zusammenführt. Geräte
/// werden über [`MonitorDeviceInfo::id`] dedupliziert; taucht ein Gerät in
/// beiden Listen auf, gilt es als online, sobald **eine** der beiden
/// Quellen es online meldet (`online`-Flag wird ge-ODER-t). Die übrigen
/// Felder stammen aus dem ersten Vorkommen (LAN zuerst). Die Ausgabe ist
/// sortiert wie [`build_device_list`] (nach Feldname, dann Code – noch nicht
/// zugewiesene Geräte zuerst, weil `None` vor `Some(_)` sortiert).
pub fn merge_device_lists(
    lan: &[MonitorDeviceInfo],
    cloud: &[MonitorDeviceInfo],
) -> Vec<MonitorDeviceInfo> {
    let mut out: Vec<MonitorDeviceInfo> = Vec::new();
    for dev in lan.iter().chain(cloud.iter()) {
        if let Some(existing) = out.iter_mut().find(|d| d.id == dev.id) {
            // Gerät schon bekannt → Online-Status der Quellen vereinen,
            // explizite Halle übernehmen, falls eine Quelle sie kennt.
            existing.online = existing.online || dev.online;
            existing.hall = existing.hall.clone().or_else(|| dev.hall.clone());
        } else {
            out.push(dev.clone());
        }
    }
    out.sort_by(|a, b| a.court.cmp(&b.court).then(a.code.cmp(&b.code)));
    out
}

// ─────────────────────────── Tablet ↔ Server ──────────────────────────────

/// Nachrichten vom Tablet an den Server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TabletMsg {
    /// Erstes Frame: das Tablet bindet sich an seinen Court (per CourtID).
    #[serde(rename = "identify")]
    Identify {
        /// CourtID des Felds (Identität).
        #[serde(rename = "courtId", default)]
        court_id: i64,
        /// Feldname (Anzeige) – nur informativ, die Routing-Identität ist
        /// `court_id`.
        #[serde(rename = "courtLabel", default)]
        court_label: String,
    },
    /// Laufender Punktestand des aktuellen Satzes plus die schon
    /// abgeschlossenen Sätze.
    #[serde(rename = "score_update")]
    ScoreUpdate {
        #[serde(rename = "scoreA")]
        score_a: i64,
        #[serde(rename = "scoreB")]
        score_b: i64,
        #[serde(rename = "setsHistory", default)]
        sets_history: Vec<SetAb>,
    },
    /// Akkustand des Tablets (nur Android/Chrome – iPads liefern ihn nicht).
    #[serde(rename = "battery")]
    Battery { percent: i64, charging: bool },
    /// Aktueller Meldungs-Zustand des Courts (vollständig, nicht inkrementell):
    /// Verletzung/Behandlung und/oder Turnierleitung gerufen.
    #[serde(rename = "alert")]
    Alert { injury: bool, official: bool },
    /// Das Tablet möchte einen bereits belegten Court übernehmen.
    #[serde(rename = "take_over")]
    TakeOver,
    /// Voller Spielzustand des Tablets als JSON-String – der Server hält
    /// ihn vor, damit ein übernehmendes Gerät das laufende Spiel bekommt.
    #[serde(rename = "state_sync")]
    StateSync { state: String },
    /// Lebenszeichen des Tablets. Der Server antwortet mit [`ServerMsg::Pong`].
    /// So erkennt das Tablet eine tote (stale) Verbindung, auch wenn der
    /// Browser kein `onclose` liefert (Router weg → nur Stille).
    #[serde(rename = "ping")]
    Ping,
}

/// Nachrichten vom Server an das Tablet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    /// BTP hat dem Court ein Match zugewiesen.
    #[serde(rename = "match_assigned")]
    MatchAssigned {
        #[serde(rename = "match")]
        match_brief: MatchBrief,
    },
    /// Der Court hat aktuell kein Match.
    #[serde(rename = "match_cleared")]
    MatchCleared,
    /// Der Court wird bereits von einem anderen Gerät geschiedst – dieses
    /// Tablet bleibt passiv, bis der Nutzer „Übernehmen" tippt.
    #[serde(rename = "court_occupied")]
    CourtOccupied,
    /// Dieses Tablet wurde von einem anderen Gerät übernommen und ist nun
    /// gesperrt – kein Zählen mehr möglich.
    #[serde(rename = "session_superseded")]
    SessionSuperseded,
    /// Spielzustand für ein Tablet, das einen Court übernimmt – damit es
    /// das laufende Spiel fortsetzt statt bei 0:0 zu beginnen.
    #[serde(rename = "state_restore")]
    StateRestore { state: String },
    /// Antwort auf [`TabletMsg::Ping`] – bestätigt dem Tablet die lebende
    /// Verbindung.
    #[serde(rename = "pong")]
    Pong,
}

/// Endergebnis-Body, den das Tablet per `POST …/result` schickt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultBody {
    #[serde(rename = "matchId")]
    pub match_id: i64,
    /// CourtID des Felds (Identität).
    #[serde(rename = "courtId", default)]
    pub court_id: i64,
    /// Feldname (Anzeige) – die Routing-Identität ist `court_id`.
    #[serde(rename = "courtLabel", default)]
    pub court_label: String,
    pub sets: Vec<SetAb>,
    /// Aufgabe (Retired): das Match wurde abgebrochen. Der Sieger ist dann
    /// nicht aus den Sätzen ableitbar, sondern steht in `winner`.
    #[serde(default)]
    pub retired: bool,
    /// Kampflos (Walkover): das Match wurde nicht ausgespielt. `sets` ist leer,
    /// der Sieger steht in `winner` (BTP-ScoreStatus 1). `#[serde(default)]`
    /// hält ältere Tablets kompatibel (Feld fehlt → false).
    #[serde(default)]
    pub walkover: bool,
    /// Sieger-Team (1 oder 2) bei Aufgabe/Kampflos; sonst aus den Sätzen bestimmt.
    #[serde(default)]
    pub winner: Option<i64>,
    /// Nur bei Aufgabe relevant: soll die aufgebende Mannschaft auch in den
    /// **restlichen** Spielen dieser Disziplin kampflos gewertet werden (echte
    /// Verletzung → Walkover-Vorschlag für die Folgespiele)? `false` (Default)
    /// = nur dieses Spiel zählt als Aufgabe. `#[serde(default)]` hält ältere
    /// Tablets kompatibel.
    #[serde(rename = "cascadeWalkover", default)]
    pub cascade_walkover: bool,
}

/// Antwort auf eine Ergebnis-Übermittlung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

impl ResultResponse {
    /// Erfolgsantwort.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    /// Fehlerantwort mit Meldung.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
    }
}

// ─────────────────────────── Host ↔ Relay ─────────────────────────────────

/// Ein Feld (CourtID + Anzeige-Label) für die Cloud-Feldliste. Der Host pusht
/// die vollständige Liste, der Relay liefert sie unter `/{ns}/courts` an das
/// Feldwechsel-Menü des Tablets (PIN). Im LAN-Modus baut der Server `/courts`
/// direkt aus seinen BTP-Daten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourtBrief {
    pub id: i64,
    pub label: String,
    /// Hallenname (BTP-Location) des Felds – damit der Cloud-Ansage-Slave die
    /// Felder **seiner** Halle herausfiltern und ihre Tablet-QR-/Monitor-Links
    /// anzeigen kann (ferne Halle: Geräte direkt am Cloud-Relay). Leer =
    /// Ein-Hallen-Turnier / unbekannt. `#[serde(default)]` hält ältere
    /// Hosts/Relays ohne dieses Feld lesbar.
    #[serde(default)]
    pub hall: String,
}

/// Die eindeutigen, nicht-leeren Hallennamen einer Feldliste – alphabetisch
/// sortiert. Grundlage der Hallen-Auswahl auf dem **Cloud-Slave**, der kein BTP
/// hat und die Hallennamen deshalb aus der Relay-Feldliste ziehen muss (statt
/// wie der Master aus dem lokalen BTP-Snapshot).
pub fn distinct_halls(courts: &[CourtBrief]) -> Vec<String> {
    let mut halls: Vec<String> = Vec::new();
    for c in courts {
        if !c.hall.is_empty() && !halls.contains(&c.hall) {
            halls.push(c.hall.clone());
        }
    }
    // Case-insensitiv sortieren – deckungsgleich mit der Master-Hallenliste
    // (`tournamentStats`), damit dasselbe `announce_hall`-Dropdown in beiden
    // Rollen gleich sortiert erscheint.
    halls.sort_by_key(|h| h.to_lowercase());
    halls
}

/// Frames von bts-light (dem „Host" eines Namespace) an den Relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostFrame {
    /// Court hat ein Match bekommen – an das zugehörige Tablet weiterleiten.
    MatchAssigned {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
        /// Hallenname (BTP-Location) des Felds – für die hallengefilterte
        /// Cloud-Ansage der fernen Halle (B1a). `#[serde(default)]` = ältere
        /// Hosts (leer → keine Hallen-Einschränkung).
        #[serde(default)]
        hall: String,
        #[serde(rename = "match")]
        match_brief: MatchBrief,
        /// Zeitpunkt (Unix-ms) des 1. Aufrufs = seit wann das Spiel auf dem
        /// Feld steht. Vom Host autoritativ gestempelt (überlebt Reconnects,
        /// frisch je Turnier) → der Relay übernimmt ihn 1:1 für die
        /// Aufruf-Uhr am Cloud-Monitor. `#[serde(default)]` = ältere Hosts.
        #[serde(
            rename = "onCourtSinceMs",
            skip_serializing_if = "Option::is_none",
            default
        )]
        on_court_since_ms: Option<u64>,
    },
    /// Court-Match aufgehoben.
    MatchCleared {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
        /// Hallenname des Felds (wie bei `MatchAssigned`). `#[serde(default)]`.
        #[serde(default)]
        hall: String,
    },
    /// Freitext-Ansage (Master → Relay → ferne Halle). Der Cloud-Ansage-Slave
    /// holt sie über `GET /{ns}/info/announce/freetext` und spricht sie lokal.
    Freetext {
        id: u64,
        #[serde(default)]
        hall: String,
        #[serde(default)]
        text: String,
    },
    /// Antwort auf eine zuvor weitergeleitete Ergebnis-Übermittlung.
    ResultAck {
        #[serde(rename = "reqId")]
        req_id: u64,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
    },
    /// Vollständige Feld-Liste des Turniers – Grundlage des Feldwechsels im
    /// PIN-Menü des Tablets im Cloud-Modus. Periodisch vom Host gepusht.
    Courts {
        #[serde(default)]
        courts: Vec<CourtBrief>,
    },
}

/// Eine Freitext-Ansage (Relay-Zwischenspeicher; Quelle = Master). `id`
/// monoton zum Entduplizieren beim Slave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreetextItem {
    pub id: u64,
    #[serde(default)]
    pub hall: String,
    #[serde(default)]
    pub text: String,
}

/// Ein Feld im Ansage-Status (für den Cloud-Ansage-Slave): aktuelles Match
/// (oder `None`) mit Anzeige-Label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnounceCourt {
    #[serde(rename = "courtId")]
    pub court_id: i64,
    #[serde(default)]
    pub label: String,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none", default)]
    pub match_brief: Option<MatchBrief>,
}

/// Antwort von `GET /{ns}/info/announce/state?hall=&since=` — hallengefilterte
/// Court-Matches (Auto-Ansage) + neue Freitext-Ansagen für den Cloud-Slave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AnnounceState {
    #[serde(default)]
    pub courts: Vec<AnnounceCourt>,
    #[serde(default)]
    pub freetext: Vec<FreetextItem>,
}

/// Präsenz-Info eines Cloud-Ansage-Slaves (für die „ferne Halle online?"-Anzeige
/// am Master). `online` = zuletzt innerhalb des Timeouts gesehen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlaveInfo {
    pub id: String,
    #[serde(default)]
    pub hall: String,
    pub online: bool,
    #[serde(rename = "lastSeenMs", default)]
    pub last_seen_ms: u64,
}

/// Frames vom Relay an den bts-light-Host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayFrame {
    /// Ein Tablet hat sich für diesen Court verbunden.
    TabletConnected {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
    },
    /// Das Tablet dieses Courts ist getrennt.
    TabletDisconnected {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
    },
    /// Live-Punktestand von einem Tablet.
    ScoreUpdate {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
        #[serde(rename = "scoreA")]
        score_a: i64,
        #[serde(rename = "scoreB")]
        score_b: i64,
        #[serde(rename = "setsHistory", default)]
        sets_history: Vec<SetAb>,
    },
    /// Endergebnis von einem Tablet – `req_id` korreliert die `ResultAck`.
    Result {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
        #[serde(rename = "matchId")]
        match_id: i64,
        sets: Vec<SetAb>,
        #[serde(default)]
        retired: bool,
        /// Kampflos (Walkover) – siehe [`ResultBody::walkover`].
        #[serde(default)]
        walkover: bool,
        #[serde(default)]
        winner: Option<i64>,
        /// Verletzung → Folgespiele der Disziplin kampflos – siehe
        /// [`ResultBody::cascade_walkover`].
        #[serde(rename = "cascadeWalkover", default)]
        cascade_walkover: bool,
    },
    /// Akkustand eines Tablets.
    Battery {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
        percent: i64,
        charging: bool,
    },
    /// Meldungs-Zustand eines Courts (Verletzung / Turnierleitung gerufen).
    Alert {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "courtLabel", default)]
        court_label: String,
        injury: bool,
        official: bool,
    },
}

// ─────────────────────────── Encoding-Helfer ──────────────────────────────

/// Minimaler Prozent-Encoder für einen URL-Pfad-Abschnitt (Court-Namen).
pub fn path_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Escapt HTML-Sonderzeichen inklusive `'`, weil der Court-Name in
/// `tablet.html` sowohl in HTML-Text als auch in einem JS-String-Literal
/// landet – ohne `'`-Escape könnte ein Apostroph das Literal aufbrechen.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serde-Roundtrip: deserialisieren, was wir serialisiert haben.
    fn roundtrip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    #[test]
    fn tablet_msg_identify_wire_form() {
        let json = r#"{"type":"identify","role":"tablet","courtId":7,"courtLabel":"Feld 1"}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            TabletMsg::Identify {
                court_id: 7,
                court_label: "Feld 1".to_string()
            }
        );
    }

    #[test]
    fn tablet_msg_identify_without_court_id_defaults_to_zero() {
        // Älteres Tablet ohne courtId-Feld bleibt deserialisierbar.
        let json = r#"{"type":"identify","role":"tablet","courtLabel":"Feld 1"}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            TabletMsg::Identify {
                court_id: 0,
                court_label: "Feld 1".to_string()
            }
        );
    }

    #[test]
    fn tablet_msg_score_update_ignores_extra_fields() {
        // tablet.html schickt zusätzlich currentSet/setsA/servingTeam – die
        // dürfen den Parser nicht stören.
        let json = r#"{"type":"score_update","courtId":3,"courtLabel":"x","scoreA":21,"scoreB":19,
            "currentSet":2,"setsA":1,"setsB":0,"setsHistory":[{"a":21,"b":15}],"servingTeam":"a"}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            TabletMsg::ScoreUpdate {
                score_a: 21,
                score_b: 19,
                sets_history: vec![SetAb { a: 21, b: 15 }],
            }
        );
    }

    #[test]
    fn server_msg_match_assigned_uses_match_key() {
        let msg = ServerMsg::MatchAssigned {
            match_brief: MatchBrief {
                match_id: 7,
                team_a: vec![PlayerBrief {
                    id: 1,
                    name: "Anna".into(),
                    nationality: Some("GER".into()),
                }],
                team_b: vec![PlayerBrief {
                    id: 11,
                    name: "Ben".into(),
                    nationality: None,
                }],
                event_label: "HE G1".into(),
                best_of_sets: 3,
                target_score: 21,
                cap_score: 30,
                interval_at: Some(11),
                discipline: "mens_singles".into(),
                class_label: String::new(),
                match_number: Some(14),
                scorekeeper: vec!["Cara / Dora".into()],
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"match_assigned""#));
        assert!(json.contains(r#""match":{"#));
        roundtrip(&msg);
    }

    #[test]
    fn host_and_relay_frames_roundtrip() {
        roundtrip(&HostFrame::MatchCleared {
            court_id: 2,
            court_label: "Feld 2".into(),
            hall: String::new(),
        });
        roundtrip(&HostFrame::ResultAck {
            req_id: 42,
            ok: false,
            error: Some("BTP abgelehnt".into()),
        });
        roundtrip(&RelayFrame::TabletConnected {
            court_id: 3,
            court_label: "Feld 3".into(),
        });
        roundtrip(&RelayFrame::Result {
            req_id: 9,
            court_id: 1,
            court_label: "Feld 1".into(),
            match_id: 18,
            sets: vec![SetAb { a: 21, b: 0 }, SetAb { a: 0, b: 21 }],
            retired: false,
            walkover: false,
            winner: None,
            cascade_walkover: false,
        });
        roundtrip(&RelayFrame::Result {
            req_id: 10,
            court_id: 2,
            court_label: "Feld 2".into(),
            match_id: 19,
            sets: vec![SetAb { a: 21, b: 10 }, SetAb { a: 5, b: 5 }],
            retired: true,
            walkover: false,
            winner: Some(1),
            cascade_walkover: true,
        });
        roundtrip(&RelayFrame::Result {
            req_id: 11,
            court_id: 3,
            court_label: "Feld 3".into(),
            match_id: 20,
            sets: vec![],
            retired: false,
            walkover: true,
            winner: Some(2),
            cascade_walkover: false,
        });
    }

    #[test]
    fn host_frame_without_court_id_defaults_to_zero() {
        // Älterer Relay schickt ein Frame ohne courtId – bleibt lesbar.
        let json = r#"{"type":"match_cleared","courtLabel":"Feld 2"}"#;
        let frame: HostFrame = serde_json::from_str(json).unwrap();
        assert_eq!(
            frame,
            HostFrame::MatchCleared {
                court_id: 0,
                court_label: "Feld 2".into(),
                hall: String::new(),
            }
        );
    }

    #[test]
    fn court_brief_hall_roundtrips_and_defaults() {
        // Neues Feld hält den Roundtrip.
        roundtrip(&CourtBrief {
            id: 401,
            label: "Halle 2 · 1".into(),
            hall: "Halle 2".into(),
        });
        // Älterer Host/Relay ohne `hall` bleibt lesbar (Default = leer).
        let old = r#"{"id":7,"label":"Feld 3"}"#;
        let brief: CourtBrief = serde_json::from_str(old).unwrap();
        assert_eq!(
            brief,
            CourtBrief {
                id: 7,
                label: "Feld 3".into(),
                hall: String::new(),
            }
        );
    }

    #[test]
    fn distinct_halls_dedups_sorts_and_drops_empty() {
        let courts = vec![
            CourtBrief {
                id: 101,
                label: "Halle 1 · 1".into(),
                hall: "Halle 1".into(),
            },
            CourtBrief {
                id: 401,
                label: "Halle 2 · 1".into(),
                hall: "Halle 2".into(),
            },
            CourtBrief {
                id: 102,
                label: "Halle 1 · 2".into(),
                hall: "Halle 1".into(),
            },
            // Leere Halle (unbekannt) wird ausgelassen.
            CourtBrief {
                id: 9,
                label: "Feld 9".into(),
                hall: String::new(),
            },
        ];
        assert_eq!(
            distinct_halls(&courts),
            vec!["Halle 1".to_string(), "Halle 2".to_string()]
        );
        assert!(distinct_halls(&[]).is_empty());
    }

    #[test]
    fn monitor_state_and_upload_roundtrip() {
        let state = MonitorState {
            court_id: 3,
            court_label: "Feld 3".into(),
            tournament_name: "Test-Cup".into(),
            match_info: Some(MonitorMatch {
                match_id: 14,
                discipline: "mens_singles".into(),
                event_label: "HE G2".into(),
                match_number: Some(14),
                team1: vec![MonitorPlayer {
                    name: "Anna Berg".into(),
                    given: "Anna".into(),
                    family: "Berg".into(),
                    nationality: Some("GER".into()),
                }],
                team2: vec![MonitorPlayer {
                    name: "Hilde".into(),
                    given: String::new(),
                    family: "Hilde".into(),
                    nationality: None,
                }],
                sets: vec![SetAb { a: 21, b: 18 }, SetAb { a: 11, b: 7 }],
            }),
            court_state: Some(r#"{"servingSide":"left"}"#.into()),
            config: MonitorConfig::default(),
            ads: vec!["0".into(), "1".into()],
            command: Some(MonitorCommand {
                id: 3,
                kind: MonitorCommandKind::Identify,
            }),
            device_code: "4F2A".into(),
            unassigned: false,
            redirect_to: None,
            server_now_ms: 0,
            on_court_since_ms: Some(1_700_000_000_000),
            call_timer: CallTimerView {
                enabled: true,
                second_call_minutes: 2.0,
                third_call_minutes: 4.0,
            },
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains(r#""match":{"#));
        roundtrip(&state);
        // Leeres Feld: `match` wird weggelassen (→ Werbemodus).
        let idle = MonitorState {
            match_info: None,
            ..state
        };
        let json = serde_json::to_string(&idle).unwrap();
        assert!(!json.contains(r#""match""#));
        roundtrip(&idle);
        roundtrip(&MonitorUpload {
            config: MonitorConfig::default(),
            tournament_name: "Test-Cup".into(),
            ads: vec![AdUpload {
                content_type: "image/png".into(),
                data: "AAAA".into(),
            }],
            call_timer: CallTimerView {
                enabled: true,
                second_call_minutes: 2.0,
                third_call_minutes: 4.0,
            },
        });
    }

    #[test]
    fn info_overview_redirect_carries_hall_filter() {
        // Ohne Halle: alte Form, unveränderter Pfad.
        assert_eq!(
            MonitorTarget::InfoOverview { hall: None }.redirect_path(),
            Some("/info/overview".to_string())
        );
        // Mit Halle: ?halle= mit URL-kodiertem Namen (Leerzeichen → %20).
        assert_eq!(
            MonitorTarget::InfoOverview {
                hall: Some("Halle 1".to_string())
            }
            .redirect_path(),
            Some("/info/overview?halle=Halle%201".to_string())
        );
    }

    #[test]
    fn info_overview_without_hall_serializes_like_before() {
        // Abwärtskompatibilität: hall=None darf KEIN hall-Feld schreiben, damit
        // alte gespeicherte Zuweisungen ({"kind":"info_overview"}) gleich bleiben.
        let json = serde_json::to_string(&MonitorTarget::InfoOverview { hall: None }).unwrap();
        assert_eq!(json, r#"{"kind":"info_overview"}"#);
        // Und eine alte gespeicherte Zuweisung lädt weiterhin (hall = None).
        let back: MonitorTarget = serde_json::from_str(r#"{"kind":"info_overview"}"#).unwrap();
        assert_eq!(back, MonitorTarget::InfoOverview { hall: None });
    }

    #[test]
    fn info_winners_redirect_carries_rank_filter() {
        // Ohne Rang: ganzes Podium, unveränderter Pfad.
        assert_eq!(
            MonitorTarget::InfoWinners { rank: None }.redirect_path(),
            Some("/info/winners".to_string())
        );
        // Mit Rang: ?only=N (ein Monitor je Podest-Platz).
        assert_eq!(
            MonitorTarget::InfoWinners { rank: Some(2) }.redirect_path(),
            Some("/info/winners?only=2".to_string())
        );
    }

    #[test]
    fn info_winners_without_rank_serializes_like_before() {
        // Abwärtskompatibilität: rank=None darf KEIN rank-Feld schreiben, damit
        // alte gespeicherte Zuweisungen ({"kind":"info_winners"}) gleich bleiben.
        let json = serde_json::to_string(&MonitorTarget::InfoWinners { rank: None }).unwrap();
        assert_eq!(json, r#"{"kind":"info_winners"}"#);
        let back: MonitorTarget = serde_json::from_str(r#"{"kind":"info_winners"}"#).unwrap();
        assert_eq!(back, MonitorTarget::InfoWinners { rank: None });
    }

    #[test]
    fn device_code_takes_last_four_uppercase() {
        assert_eq!(device_code("a1b2c3d4-e5f6-7890-abcd-ef1234567890"), "7890");
        assert_eq!(device_code("xy"), "XY");
    }

    #[test]
    fn device_code_distinguishes_pi_serials_with_shared_prefix() {
        // Pi-Monitore melden sich als pi-<CPU-Serial>; alle Serials beginnen
        // mit demselben Präfix (00000000…). Der Code muss sie am ENDE
        // unterscheiden – sonst zeigen alle Pis denselben Code ("PI00").
        assert_eq!(device_code("pi-00000000a3a5a3f8"), "A3F8");
        assert_eq!(device_code("pi-00000000a3a5b1c2"), "B1C2");
        assert_ne!(
            device_code("pi-00000000a3a5a3f8"),
            device_code("pi-00000000a3a5b1c2")
        );
    }

    #[test]
    fn build_device_list_merges_assignments_and_seen() {
        // Zuweisungen sind jetzt MonitorTarget; court_names löst die
        // CourtID-Variante auf.
        let mut assign = HashMap::new();
        assign.insert("dev-online".to_string(), MonitorTarget::court(101));
        assign.insert("dev-offline".to_string(), MonitorTarget::court(102));
        let mut court_names = HashMap::new();
        court_names.insert(101i64, "Feld 1".to_string());
        court_names.insert(102i64, "Feld 2".to_string());
        let mut seen = HashMap::new();
        seen.insert("dev-online".to_string(), 10_000u64);
        // Gesehen, aber noch keinem Feld zugewiesen.
        seen.insert("dev-new".to_string(), 9_500u64);
        let list = build_device_list(&assign, &court_names, &seen, 12_000);
        assert_eq!(list.len(), 3);
        let online = list.iter().find(|d| d.id == "dev-online").unwrap();
        assert!(online.online);
        assert_eq!(online.court_id, Some(101));
        assert_eq!(online.court.as_deref(), Some("Feld 1"));
        // Zugewiesen, aber nie gepollt → offline.
        assert!(!list.iter().find(|d| d.id == "dev-offline").unwrap().online);
        let fresh = list.iter().find(|d| d.id == "dev-new").unwrap();
        assert!(fresh.online);
        assert_eq!(fresh.court_id, None);
        assert_eq!(fresh.court, None);
    }

    #[test]
    fn merge_device_lists_dedups_by_id_and_ors_online() {
        // Hilfskonstruktor für ein knappes Gerät.
        let dev = |id: &str, court: Option<&str>, online: bool| MonitorDeviceInfo {
            id: id.to_string(),
            code: device_code(id),
            court_id: court.map(|_| 1),
            court: court.map(|c| c.to_string()),
            target: court.map(|_| MonitorTarget::court(1)),
            online,
            hall: None,
        };
        // LAN: Feld-1-Gerät online, gemeinsames Gerät offline.
        let lan = vec![
            dev("dev-lan-1", Some("Feld 1"), true),
            dev("dev-both", Some("Feld 2"), false),
        ];
        // Cloud: gemeinsames Gerät online, eigenes Gerät offline.
        let cloud = vec![
            dev("dev-both", Some("Feld 2"), true),
            dev("dev-cloud-1", Some("Feld 3"), false),
        ];
        let merged = merge_device_lists(&lan, &cloud);
        // Drei distinkte Geräte – das gemeinsame nur einmal.
        assert_eq!(merged.len(), 3);
        // Das in beiden Listen geführte Gerät ist online (OR der Quellen).
        let both = merged.iter().find(|d| d.id == "dev-both").unwrap();
        assert!(both.online);
        // Reine LAN-/Cloud-Geräte bleiben mit ihrem Status erhalten.
        assert!(merged.iter().find(|d| d.id == "dev-lan-1").unwrap().online);
        assert!(
            !merged
                .iter()
                .find(|d| d.id == "dev-cloud-1")
                .unwrap()
                .online
        );
        // Stabil nach Feldname sortiert.
        assert_eq!(
            merged.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["dev-lan-1", "dev-both", "dev-cloud-1"]
        );
    }

    #[test]
    fn merge_device_lists_preserves_hall_from_either_source() {
        // Vertrag: kennt eine Quelle die explizite Halle, bleibt sie im Merge
        // erhalten (auch wenn der Host sie i. d. R. nachträglich überschreibt).
        let mk = |id: &str, hall: Option<&str>| MonitorDeviceInfo {
            id: id.to_string(),
            code: device_code(id),
            court_id: None,
            court: None,
            target: None,
            online: true,
            hall: hall.map(|h| h.to_string()),
        };
        let lan = vec![mk("dev-1", None)];
        let cloud = vec![mk("dev-1", Some("Halle 2"))];
        let merged = merge_device_lists(&lan, &cloud);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].hall.as_deref(), Some("Halle 2"));
    }

    #[test]
    fn merge_device_lists_sorts_unassigned_devices_first() {
        // Vertrag: ein noch nicht zugewiesenes Gerät (court = None) sortiert
        // VOR zugewiesenen, weil `None` vor `Some(_)` ordnet. Pinnt die im
        // Docstring zugesicherte Reihenfolge fest.
        let dev = |id: &str, court: Option<&str>| MonitorDeviceInfo {
            id: id.to_string(),
            code: device_code(id),
            court_id: court.map(|_| 1),
            court: court.map(|c| c.to_string()),
            target: court.map(|_| MonitorTarget::court(1)),
            online: false,
            hall: None,
        };
        let lan = vec![dev("dev-assigned", Some("Feld 1"))];
        let cloud = vec![dev("dev-free", None)];
        let merged = merge_device_lists(&lan, &cloud);
        assert_eq!(
            merged.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["dev-free", "dev-assigned"]
        );
    }

    #[test]
    fn merge_device_lists_handles_empty_inputs() {
        // Einzelmodus: eine der beiden Listen ist leer – die andere geht
        // unverändert (nur stabil sortiert) durch.
        let lan = vec![MonitorDeviceInfo {
            id: "dev-x".into(),
            code: device_code("dev-x"),
            court_id: None,
            court: None,
            target: None,
            online: true,
            hall: None,
        }];
        assert_eq!(merge_device_lists(&lan, &[]), lan);
        assert_eq!(merge_device_lists(&[], &lan), lan);
        assert!(merge_device_lists(&[], &[]).is_empty());
    }

    #[test]
    fn result_response_omits_error_on_success() {
        let json = serde_json::to_string(&ResultResponse::ok()).unwrap();
        assert_eq!(json, r#"{"ok":true}"#);
        roundtrip(&ResultResponse::err("Zeitüberschreitung"));
    }

    #[test]
    fn path_encode_escapes_spaces_and_keeps_safe_chars() {
        assert_eq!(path_encode("Feld 1"), "Feld%201");
        assert_eq!(path_encode("Court-3"), "Court-3");
    }

    #[test]
    fn html_escape_neutralizes_markup_and_quotes() {
        assert_eq!(html_escape("a<b>&\"'c"), "a&lt;b&gt;&amp;&quot;&#39;c");
    }
}
