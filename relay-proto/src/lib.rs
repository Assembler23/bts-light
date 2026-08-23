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
    /// Vereinsname (BTP), Grundlage für die optionale Vereinsanzeige auf dem
    /// Tablet-Spielzettel. Wie die Nationalität turnierweit zuschaltbar und
    /// standardmäßig aus. `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(default)]
    pub club: Option<String>,
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
    /// Zähltafelbediener: bei aktiver Verwaltung der zugewiesene Bediener,
    /// sonst der pro-Feld-Hinweis (Verlierer des Vorspiels). Wird dem Tablet
    /// bei der Seitenwahl angezeigt. Leer, wenn keiner bekannt.
    /// `#[serde(default)]` hält ältere Frames lesbar.
    #[serde(default)]
    pub scorekeeper: Vec<String>,
    /// `true`, wenn `scorekeeper` aus einer echten Zuweisung stammt (Verwaltung
    /// aktiv), sonst ist es der pro-Feld-Hinweis. Seit ADR 0040 entscheidet
    /// über die **Ansage** der Schalter `announce.announce_scorekeeper`, nicht
    /// mehr die Herkunft des Namens; dieses Feld bleibt für die Anzeige.
    #[serde(rename = "scorekeeperAssigned", default)]
    pub scorekeeper_assigned: bool,
    /// Turnierweite Anzeige-Schalter: ob das Tablet Vereinsname bzw. -logo
    /// zeigen darf. Kommen **in-band** mit der Paarung, damit LAN und Cloud
    /// dieselbe zentrale Einstellung ohne Seiten-Neuladen übernehmen.
    /// `#[serde(default)]` hält ältere Frames lesbar (aus).
    #[serde(rename = "showClubNames", default)]
    pub show_club_names: bool,
    #[serde(rename = "showClubLogos", default)]
    pub show_club_logos: bool,
    /// `true`, wenn das zugewiesene Match in BTP finalisiert ist (Sieger steht,
    /// per Hand fertig eingegeben — A2 / ADR 0017). Dann tritt das Tablet
    /// zurück: kein Score-Push, kein state_sync, keine Ergebnis-Absendung
    /// (`tablet.html` gated diese Pfade über `STATE.finalized`), damit das
    /// Hand-Ergebnis nicht überbügelt wird. `#[serde(default)]` hält ältere
    /// Frames lesbar (false → altes Verhalten).
    #[serde(default)]
    pub finalized: bool,
    /// Schiedsrichter und Aufschlagrichter dieses Spiels — als **Namen**,
    /// nicht als IDs: Das Tablet müsste sie sonst auflösen und dazu die
    /// Officials-Liste kennen. Leer, wenn keiner zugewiesen ist oder ohne
    /// Schiedsrichter gespielt wird. `#[serde(default)]` hält ältere Frames
    /// lesbar. Gilt für LAN und Cloud gleichermaßen, ferne Halle
    /// eingeschlossen (der Brief reist mit `MatchAssigned`).
    #[serde(rename = "srNames", default)]
    pub sr_names: Vec<String>,
    #[serde(rename = "arNames", default)]
    pub ar_names: Vec<String>,
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
    /// Darf die Anzeige ihren Sicherheits-Poll auf vier Sekunden
    /// verlangsamen, solange ihr Push-Kanal gesund ist (Spec
    /// monitor-livestand-push, S6)?
    ///
    /// `#[serde(default)]` → `false` bei einem älteren Absender, und dann
    /// pollt die Seite wie vorher im 250-ms-Takt. Der Schalter kann also nur
    /// entlasten, nie etwas kaputt machen, das vorher lief.
    #[serde(rename = "pushFallbackSlow", default)]
    pub push_fallback_slow: bool,
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
            // Aus: Der langsame Sicherheits-Poll ist ein bewusst zu
            // setzender Schalter (Spec monitor-livestand-push, S6).
            push_fallback_slow: false,
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
    /// `true`, wenn das Bild zusätzlich klein in der oberen Leiste erscheinen
    /// soll (Sponsor-Leiste). `#[serde(default)]` (= aus) hält ältere Uploads
    /// lesbar.
    #[serde(rename = "inBar", default)]
    pub in_bar: bool,
    /// Anzeige-Stil im Leerlauf-Vollbild (Spec `werbung-hintergrund-und-feld`,
    /// ADR 0041). **Positionell**: Der Stil gehört zu dem Bild, an dem er
    /// hängt, und wandert mit dessen Index. Leere Farbstrings bedeuten „nicht
    /// gesetzt" — dann gilt die Vorgabe der Anzeigeseite (Schwarz). So bleibt
    /// ein Upload einer älteren App ohne Sonderfall lesbar.
    #[serde(default, skip_serializing_if = "AdStyleWire::ist_leer")]
    pub style: AdStyleWire,
}

/// Anzeige-Stil eines Werbebilds auf dem Draht.
///
/// `fg` rechnet der **Host** aus `bg` (relative Luminanz, ADR 0041) — der
/// Relay reicht nur durch. So gibt es genau eine Stelle, die den Kontrast
/// bestimmt, und keine zweite Rechnung im Browser, die davon abweichen könnte.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdStyleWire {
    /// Hintergrundfarbe `#rrggbb`; leer = nicht gesetzt.
    #[serde(default)]
    pub bg: String,
    /// Dazu kontrastierende Schriftfarbe `#rrggbb`; leer = nicht gesetzt.
    #[serde(default)]
    pub fg: String,
    /// Feldbezeichnung über der Werbung zeigen?
    #[serde(rename = "showCourt", default)]
    pub show_court: bool,
}

impl AdStyleWire {
    /// Nichts gesetzt — dann muss der Stil gar nicht erst über den Draht.
    pub fn ist_leer(&self) -> bool {
        self.bg.is_empty() && self.fg.is_empty() && !self.show_court
    }
}

/// Das Turnierlogo für die Sponsor-Leiste der Cloud-Anzeigeseiten – Base64,
/// damit es in den Monitor-Upload passt. Wird bewusst nur mitgeschickt, wenn
/// gesetzt (Option = None → kein Logo), und der Upload ist änderungs-gegated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogoUpload {
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
    /// Turnierlogo für die Sponsor-Leiste (`None` = keins). `#[serde(default)]`
    /// hält ältere Host-Uploads ohne dieses Feld lesbar.
    #[serde(default)]
    pub logo: Option<LogoUpload>,
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
    /// Effektive Hallen-Farbe des Felds (Hex, Spec hallen-farben) — der
    /// Monitor zeigt sie als Marke neben dem Feld-Label. `None` bei
    /// Ein-Hallen-Turnieren und von alten Hosts/Relays.
    #[serde(rename = "hallColor", default, skip_serializing_if = "Option::is_none")]
    pub hall_color: Option<String>,
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
    /// Anzeige-Stil je Werbebild, **index-parallel zu `ads`** (ADR 0041).
    /// Kürzer als `ads` oder ganz leer = für die übrigen Bilder gilt die
    /// Vorgabe; so bleibt ein Frame eines älteren Hosts lesbar.
    #[serde(rename = "adStyles", default, skip_serializing_if = "Vec::is_empty")]
    pub ad_styles: Vec<AdStyleWire>,
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
    /// Ordnungszahl des Feld-Stands (Spec monitor-livestand-push, S4) —
    /// dieselbe Zahl, die der Nudge auf der Monitor-WS trägt.
    ///
    /// Damit kann die Anzeige Push und Voll-Abruf zueinander ordnen, statt
    /// beide blind anzuwenden: Ein Push gilt bei `seq > gezeigt`, eine
    /// Voll-Antwort bei `seq >= gezeigt`. Das Gleichheitszeichen ist
    /// Absicht — eine Voll-Antwort trägt denselben Stand, den der Nudge
    /// angekündigt hat, und kann ihn auch dann noch berichtigen (etwa wenn
    /// BTP einen Satzstand zurücknimmt).
    ///
    /// **Prozesslokal** (ADR 0035): Host und Relay zählen getrennt, die Zahl
    /// ist nur innerhalb einer Verbindung zu derselben Gegenstelle
    /// vergleichbar. `#[serde(default)]` → `0` bei einem älteren Absender,
    /// und dann verhält sich die Seite wie vor dieser Etappe.
    #[serde(default)]
    pub seq: u64,
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
    /// Geräte-ID → CourtID des zugewiesenen Felds. **Nur Court-Ziele** — bleibt
    /// für alte Relays lesbar, die `targets` noch nicht kennen.
    #[serde(default)]
    pub assignments: HashMap<String, i64>,
    /// Geräte-ID → **vollständiges** Anzeige-Ziel (Court, Info-Übersicht/
    /// Vorbereitung/Sieger, Werbung, Kombi). `#[serde(default)]` hält ältere
    /// Host-Uploads ohne dieses Feld lesbar. Ein neues Relay bevorzugt `targets`
    /// und kann so auch Info-/Werbe-Monitore im Cloud-Modus umleiten — vorher
    /// gingen nur Court-Ziele durch (der Rest blieb „unzugewiesen", zeigte also
    /// nur das bts-light-Logo).
    #[serde(default)]
    pub targets: HashMap<String, MonitorTarget>,
    /// Geräte-ID → offener Fernbefehl.
    pub commands: HashMap<String, MonitorCommand>,
}

/// Ein Gerät gilt als „online", wenn sein letzter Poll höchstens so lange
/// her ist (der Monitor pollt im Sekundentakt). Großzügig (20 s), damit ein
/// kurzer WLAN-Zucker den Online-Status NICHT flackern lässt – im flakigen
/// Hallen-/Verleih-WLAN sind einzelne >6-s-Aussetzer normal. Ein wirklich
/// totes Gerät fällt weiterhin nach 20 s raus.
pub const MONITOR_ONLINE_WINDOW_MS: u64 = 20_000;

/// Abstand des **sichtbaren** Herzschlags auf der Monitor-Nudge-WS (Spec
/// monitor-livestand-push, S6). Der daneben laufende WS-Ping ist für
/// JavaScript unsichtbar — eine Anzeige kann daran nicht erkennen, ob ihr
/// Kanal noch lebt.
///
/// Zehn Sekunden, damit die Anzeige die 25-Sekunden-Grenze
/// ([`MONITOR_HEARTBEAT_STALE_MS`]) auch dann sicher hält, wenn zwei
/// Herzschläge hintereinander im Netz hängenbleiben.
pub const MONITOR_HEARTBEAT_MS: u64 = 10_000;

/// Ab wann gilt der Nudge-Kanal einer Anzeige als tot (Spec
/// monitor-livestand-push, S6)? Zweieinhalb Herzschläge — ein einzelner
/// verlorener darf noch keinen Reconnect auslösen.
pub const MONITOR_HEARTBEAT_STALE_MS: u64 = 25_000;

/// Der sichtbare Herzschlag als fertiges Wire-Frame.
///
/// **Ohne `court`-Feld, und das ist die ganze Verträglichkeitszusage:** Eine
/// Anzeige aus einem älteren Stand prüft `typeof msg.court === "number"` und
/// verwirft alles andere folgenlos. So braucht der Kanal keine
/// Protokollversion (ADR 0035 c) — fest verdrahtete Monitore haben keinen
/// Reload-Kanal, über den man sie umstellen könnte.
pub fn monitor_heartbeat_frame(now_ms: u64) -> String {
    format!("{{\"hb\":{now_ms}}}")
}

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
/// Extrahiert die Match-ID aus einem `state_sync`-JSON. tablet.html
/// persistiert seinen Spielzustand als `{ "match": { "matchId": … }, … }`
/// — Server (LAN) und Relay (Cloud) verwerfen einen State, dessen Match
/// nicht mehr zum aktuellen Court-Match passt (Stale-Filter, Cluster A4).
/// `None`, wenn das JSON kein Match trägt oder nicht parsebar ist —
/// dann greift bewusst KEIN Filter (Verhalten wie vor dem Feature).
pub fn state_sync_match_id(state: &str) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(state).ok()?;
    v.get("match")?.get("matchId")?.as_i64()
}

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
        /// Persistente Geräte-Kennung des Tablets (zufällig, localStorage).
        /// Meldet sich DASSELBE Gerät nach einem Verbindungsabriss neu,
        /// erkennt der Server das als Reconnect (nahtlos weiter) statt als
        /// fremde Übernahme („Feld belegt"). Leer bei alten Tablet-Seiten
        /// (`#[serde(default)]`) → Verhalten wie bisher.
        #[serde(rename = "deviceId", default)]
        device_id: String,
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
        /// Match, das dieses Tablet gerade zählt. Server/Relay verwerfen
        /// den Stand, wenn er nicht zum aktuellen Court-Match passt —
        /// ein nach Doze/Reconnect im ALTEN Spiel hängendes Tablet darf
        /// den frisch geleerten Score-Cache des Felds nicht wieder mit
        /// dem alten Stand befüllen (Turnier-Befund HM-03, 19.07.2026;
        /// Tilos BTS verwirft solche Frames als „stale panel state").
        /// 0 bei alten Tablet-Seiten (`#[serde(default)]`) → wie bisher.
        #[serde(rename = "matchId", default)]
        match_id: i64,
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
    TakeOver {
        /// Geräte-Kennung des übernehmenden Tablets (wie bei `Identify`).
        #[serde(rename = "deviceId", default)]
        device_id: String,
    },
    /// Voller Spielzustand des Tablets als JSON-String – der Server hält
    /// ihn vor, damit ein übernehmendes Gerät das laufende Spiel bekommt.
    #[serde(rename = "state_sync")]
    StateSync { state: String },
    /// Lebenszeichen des Tablets. Der Server antwortet mit [`ServerMsg::Pong`].
    /// So erkennt das Tablet eine tote (stale) Verbindung, auch wenn der
    /// Browser kein `onclose` liefert (Router weg → nur Stille).
    #[serde(rename = "ping")]
    Ping,
    /// Ein gezählter Ballwechsel (Punktverlauf-Graph, ADR 0014). Der Host
    /// hängt ihn an den Verlauf des Matches an; passt die laufende Nummer
    /// nicht (Lücke), wartet er auf den nächsten [`TabletMsg::RallySync`].
    #[serde(rename = "rally")]
    Rally {
        /// Match, zu dem der Ballwechsel gehört — gefiltert wie beim
        /// `ScoreUpdate` (HM-03): passt es nicht zum Court-Match, wird
        /// der Frame verworfen. 0 (Default) wird immer verworfen.
        #[serde(rename = "matchId", default)]
        match_id: i64,
        /// Satz-Nummer, 1-basiert.
        #[serde(default)]
        set: i64,
        /// Laufende Nummer des Ballwechsels im Satz, 1-basiert.
        #[serde(default)]
        n: i64,
        /// Wer den Ballwechsel gewann: `"A"` oder `"B"`.
        #[serde(default)]
        winner: String,
        /// Stand NACH dem Ballwechsel — Plausibilitätsanker für den Host.
        #[serde(rename = "scoreA", default)]
        score_a: i64,
        #[serde(rename = "scoreB", default)]
        score_b: i64,
    },
    /// Kompletter Verlaufs-Resync (ADR 0014): **ersetzt** den Host-Stand
    /// des Matches vollständig. Gesendet nach Undo, Satz-Wiedereröffnung,
    /// Reconnect, Seiten-Reload und Geräte-Übernahme — damit heilt sich
    /// der Verlauf selbst, wo einzelne `rally`-Frames verloren gingen.
    #[serde(rename = "rally_sync")]
    RallySync {
        #[serde(rename = "matchId", default)]
        match_id: i64,
        #[serde(default)]
        timeline: MatchTimeline,
    },
    /// Ein Zettel-Ereignis (Schiedsrichterzettel, ADR 0037) — eigener
    /// Strom **neben** dem Punktverlauf, weil Karten personenbezogene
    /// Sanktionsdaten sind und [`MatchTimeline`] personenbezugsfrei
    /// bleiben muss (ADR 0015).
    ///
    /// Gefiltert wie `rally`: nur vom aktiven Halter des Feldes und nur
    /// für dessen Match.
    #[serde(rename = "match_event")]
    MatchEvent {
        #[serde(rename = "matchId", default)]
        match_id: i64,
        event: MatchEvent,
    },
    /// Kompletter Ereignis-Abgleich (ADR 0038): **vereinigt**, ersetzt
    /// nicht — anders als [`TabletMsg::RallySync`].
    ///
    /// Für die Ereignisse hält der Host die Wahrheit. Ein übernehmendes
    /// Ersatz-Tablet kennt die Karten seines Vorgängers nicht; ein
    /// ersetzender Sync löschte sie beim ersten Abgleich. Die Vereinigung
    /// ist idempotent und kommutativ, deshalb bringt ein Tablet, das
    /// offline war, seine Ereignisse beim Reconnect einfach nach.
    #[serde(rename = "match_event_sync")]
    MatchEventSync {
        #[serde(rename = "matchId", default)]
        match_id: i64,
        #[serde(default)]
        events: Vec<MatchEvent>,
    },
}

// ─────────────────────────── Punktverlauf ──────────────────────────────

/// Höchstzahl Ballwechsel je Satz, die Host/Relay annehmen.
///
/// Ein 21-Punkte-Satz endet spätestens bei 30:29 (59 Ballwechsel); der
/// Deckel lässt Luft für alte Zählweisen und Zwischenstands-Korrekturen,
/// bleibt aber ein harter Cloud-DoS-Riegel: mehr wird verworfen, nie
/// gespeichert.
pub const MAX_RALLIES_PER_SET: usize = 120;

/// Höchstzahl Sätze je Verlauf (Badminton spielt höchstens best-of-5).
pub const MAX_TIMELINE_SETS: usize = 5;

/// Obergrenze des Startstands eines Satzes (Zwischenstand-Einstieg).
///
/// Legitime Zwischenstände liegen bei ≤ ~30 — der Deckel ist großzügig,
/// aber hart: Ohne ihn könnte ein bösartiges Tablet `startA` auf i64-Max
/// setzen, und die Gitter-Schleifen der SVG-Renderer liefen sich auf
/// jedem anzeigenden Gerät tot (Security-Review 2026-08-11, Medium).
pub const MAX_START_SCORE: i64 = 120;

/// Höchstgröße eines serialisierten Verlaufs in Bytes (Sync + Abruf).
/// Geteilt, damit Host und Relay dieselbe Grenze durchsetzen.
pub const MAX_TIMELINE_LEN: usize = 8 * 1024;

/// Punktverlauf eines Matches: je Satz die Ballwechsel-Folge.
///
/// **Bewusst ohne Namen** (Datenschutz, ADR 0015): nur Kennungen und
/// Punktfolgen — die Anzeige holt Namen zur Laufzeit aus dem Turnierstand,
/// und genau in dieser Form wandert die Datei später zu badhub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatchTimeline {
    #[serde(default)]
    pub sets: Vec<TimelineSet>,
    /// Aufzeichnung begann mit eingetipptem Zwischenstand (`midGameSetup`)
    /// — der Graph startet dann nicht bei 0:0 und sagt das dazu.
    #[serde(rename = "midGame", default)]
    pub mid_game: bool,
    /// Match endete mit Aufgabe/Disqualifikation — der letzte Satz ist
    /// dann bewusst unvollständig.
    #[serde(default)]
    pub retired: bool,
    /// Match ist abgeschlossen (Ergebnis abgegeben) — es kommen keine
    /// Ballwechsel mehr.
    #[serde(default)]
    pub finished: bool,
}

/// Ein Satz im Punktverlauf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TimelineSet {
    /// Startstand des Satzes — 0:0, außer bei Zwischenstand-Einstieg.
    #[serde(rename = "startA", default)]
    pub start_a: i64,
    #[serde(rename = "startB", default)]
    pub start_b: i64,
    /// Ballwechsel-Gewinner in gespielter Reihenfolge, nur `'A'`/`'B'`.
    /// Als String statt Liste: kompakt auf dem Draht und in der Datei,
    /// trivial zu kürzen (Undo) und zu verlängern.
    #[serde(default)]
    pub points: String,
}

impl TimelineSet {
    /// Nur `'A'`/`'B'`, gedeckelt, Startstand in `0..=MAX_START_SCORE` —
    /// die Folge kommt übers Netz und landet in Persistenz und
    /// SVG-Renderern.
    pub fn is_valid(&self) -> bool {
        (0..=MAX_START_SCORE).contains(&self.start_a)
            && (0..=MAX_START_SCORE).contains(&self.start_b)
            && self.points.len() <= MAX_RALLIES_PER_SET
            && self.points.bytes().all(|b| b == b'A' || b == b'B')
    }
}

impl MatchTimeline {
    /// Gesamt-Gültigkeit — Host UND Relay verwerfen Ungültiges komplett,
    /// statt es zu kürzen (ein halber Verlauf wäre eine stille Lüge).
    pub fn is_valid(&self) -> bool {
        self.sets.len() <= MAX_TIMELINE_SETS && self.sets.iter().all(TimelineSet::is_valid)
    }
}

// ─────────────────────── Schiedsrichterzettel ──────────────────────────
//
// Eigener Strom **neben** dem Punktverlauf (ADR 0037). Karten sind
// personenbezogene Sanktionsdaten; sie in [`MatchTimeline`] zu legen würde
// deren Begründung kippen, personenbezugsfrei und badhub-tauglich zu sein
// (ADR 0015). Eigener Typ, eigene Datei, eigene Route, eigene Deckel.

/// Höchstzahl Ereignisse je Match, die Host und Relay annehmen.
///
/// Realistisch sind ≈ 20 je Spiel (fünf `serve_start`, einige Karten, zwei
/// bis vier Verletzungseinträge, Rücknahmen). Der Deckel zählt **auch die
/// Rücknahmen** mit — der Bestand wächst monoton (ADR 0038) — und ist wie
/// beim Punktverlauf ein harter Cloud-DoS-Riegel: Weiteres wird verworfen,
/// nie gespeichert.
pub const MAX_EVENTS_PER_MATCH: usize = 64;

/// Höchstgröße eines serialisierten Ereignis-Bestands in Bytes
/// (Sync + Abruf). Geteilt, damit Host und Relay dieselbe Grenze
/// durchsetzen.
///
/// Bewusst **neben** [`MAX_TIMELINE_LEN`] statt darin (ADR 0037): Ein
/// Ereignis-Schwall kann den Punktverlauf strukturell nicht mehr
/// verdrängen, weil beide Ströme getrennt gedeckelt sind.
pub const MAX_SHEET_LEN: usize = 16 * 1024;

/// Höchstlänge einer Ereignis-Kennung.
///
/// Die Kennung kommt vom Tablet und ist der Dedupe-Schlüssel des Bestands
/// — deshalb kurz und auf Hex-Ziffern beschränkt (siehe
/// [`MatchEvent::is_valid`]): sie darf niemals in einen Dateinamen oder
/// eine Ausgabe geraten können.
pub const MAX_EVENT_ID_LEN: usize = 32;

/// Höchstzahl Zettel in einem Druckauftrag (Stapeldruck einer Runde).
pub const MAX_SHEETS_PER_DOC: usize = 40;

/// Art eines Zettel-Ereignisses.
///
/// **Kein Freitextfeld** (Spec): Texte und Symbole entstehen erst beim
/// Rendern aus dieser Art. Das nimmt Größen-Explosion, HTML-Injection und
/// unkontrollierten Personenbezug in einem Zug weg.
///
/// Bewusst **ohne** Catch-All-Variante: Eine unbekannte Art ist ein
/// Deserialisierungs-Fehler, der den ganzen Frame verwirft — verwerfen
/// statt raten (ADR 0014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Aufschlagfolge zu Satz- oder Spielbeginn: `team`/`player` ist der
    /// Aufschläger, `receiver_team`/`receiver_player` der Empfänger.
    ///
    /// Die Aufschlagfolge ist ein **Ereignis, kein Feld** (ADR 0037) —
    /// sonst müsste [`TimelineSet`] wachsen und die Graph-Sicht wäre
    /// nicht mehr byte-gleich zu älteren Seiten.
    ServeStart,
    /// Verwarnung — kein Punkt.
    CardYellow,
    /// Fehler wegen unsportlichen Verhaltens — der Gegner bekommt einen
    /// regulären Punkt. Der Punkt entsteht am Tablet über den normalen
    /// Zählweg, nicht aus diesem Ereignis.
    CardRed,
    /// Disqualifikation. Bleibt **Protokollnotiz ohne Ergebnisweg**: die
    /// Wertung läuft weiter über `disqualify_match`.
    CardBlack,
    InjuryStart,
    InjuryEnd,
    Suspension,
    Overrule,
    RefereeCall,
    Retired,
    Disqualified,
    /// Rücknahme eines früheren Ereignisses (`retracts` nennt dessen
    /// Kennung). Der Bestand ist append-only — nichts wird gelöscht
    /// (ADR 0038); der Zettel druckt Zurückgenommenes durchgestrichen in
    /// der Protokollzeile, aber nicht im Raster.
    Retract,
}

impl EventKind {
    /// Sanktionsdaten im Sinne des Datenschutz-Abschnitts der Spec —
    /// diese Arten dürfen ausschließlich auf dem Zettel erscheinen, nie im
    /// Anzeige-Zustand, nie im Punktverlauf, nie im badhub-Push.
    pub fn is_sanction(self) -> bool {
        matches!(
            self,
            EventKind::CardYellow
                | EventKind::CardRed
                | EventKind::CardBlack
                | EventKind::Disqualified
        )
    }
}

/// Spielabschnitt, in dem ein Ereignis erfasst wurde.
///
/// Ereignisse mit `phase != Play` haben keinen Trägerballwechsel (eine
/// Karte in der Satzpause) — sie erscheinen auf dem Zettel als
/// Marker-Spalte am Blockrand statt in einer Rasterzelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Play,
    BreakEleven,
    BreakGame,
    BreakInjury,
    PreMatch,
    PostMatch,
}

/// Ein Ereignis für den Schiedsrichterzettel.
///
/// **Bewusst ohne Namen** (Datenschutz, wie [`MatchTimeline`] nach
/// ADR 0015): nur `team`/`player` als Koordinaten. Die Namen kommen zur
/// Laufzeit aus dem BTP-Snapshot und stehen nur auf dem gedruckten Zettel.
///
/// Der Anker ist eine **Schnittposition, keine Kennung** (ADR 0038):
/// `(set, after_n)` sagt „nach so vielen aufgezeichneten Ballwechseln
/// dieses Satzes", `score_a`/`score_b` sind Plausibilitätsanker. Nach
/// einem Undo existiert die Position semantisch nicht mehr — die
/// betroffenen Ereignisse werden ausdrücklich zurückgenommen, statt sich
/// stillschweigend zu verschieben.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchEvent {
    /// Kennung, am Tablet erzeugt (12 Hex) — **Dedupe-Schlüssel** der
    /// Vereinigung: dasselbe Ereignis zweimal empfangen ändert den
    /// Bestand nicht.
    #[serde(default)]
    pub id: String,
    /// Erfassungs-Reihenfolge am Tablet. Nur Sortier-Kriterium dritter
    /// Ordnung hinter `(set, after_n)` — nie eine Identität.
    #[serde(default)]
    pub seq: i64,
    /// Satz-Nummer, 1-basiert.
    #[serde(default)]
    pub set: i64,
    /// Zahl der aufgezeichneten Ballwechsel des Satzes zum Zeitpunkt der
    /// Erfassung; `0` = vor dem ersten Ballwechsel.
    #[serde(rename = "afterN", default)]
    pub after_n: i64,
    /// Stand zum Zeitpunkt der Erfassung — Plausibilitätsanker, damit ein
    /// verschobenes Ereignis auffällt statt still falsch zu stehen.
    #[serde(rename = "scoreA", default)]
    pub score_a: i64,
    #[serde(rename = "scoreB", default)]
    pub score_b: i64,
    /// Erfassungszeitpunkt in Millisekunden seit Epoch (Uhrzeit in der
    /// Protokollzeile des Zettels).
    #[serde(rename = "tsMs", default)]
    pub ts_ms: u64,
    /// Art des Ereignisses. **Pflichtfeld**: ohne sie ist der Frame
    /// bedeutungslos, und ein Default wäre geraten statt gelesen.
    pub kind: EventKind,
    /// Betroffene Mannschaft (`0` = A, `1` = B). Ohne Personenbezug
    /// (`suspension`, `overrule`, `referee_call`) bedeutungslos — der
    /// Renderer entscheidet nach `kind`, ob er sie zeigt.
    #[serde(default)]
    pub team: i64,
    /// Betroffener Spieler innerhalb der Mannschaft (`0`/`1`; im Einzel
    /// immer `0`).
    #[serde(default)]
    pub player: i64,
    /// Nur bei [`EventKind::ServeStart`]: Empfänger. Im Doppel ist er der
    /// diagonal gegenüberstehende Gegner und lässt sich aus dem Aufschläger
    /// allein nicht ableiten — deshalb ein eigenes Feld.
    #[serde(rename = "receiverTeam", default)]
    pub receiver_team: i64,
    #[serde(rename = "receiverPlayer", default)]
    pub receiver_player: i64,
    /// Spielabschnitt der Erfassung.
    #[serde(default)]
    pub phase: Phase,
    /// Nur bei [`EventKind::Retract`]: Kennung des zurückgenommenen
    /// Ereignisses; sonst leer.
    ///
    /// `skip_serializing_if` — anders als im Punktverlauf-Block, wo kein
    /// Feld es benutzt: Das Feld ist bei fast jedem Ereignis leer, und bei
    /// [`MAX_EVENTS_PER_MATCH`] Ereignissen kostete es rund ein Kilobyte
    /// gegen [`MAX_SHEET_LEN`], ohne etwas zu sagen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub retracts: String,
}

impl MatchEvent {
    /// Gültigkeit eines einzelnen Ereignisses.
    ///
    /// Host **und** Relay verwerfen Ungültiges komplett, statt es zu
    /// beschneiden — ein halb übernommenes Ereignis wäre eine stille Lüge
    /// auf einem Archivbeleg.
    pub fn is_valid(&self) -> bool {
        // Kennungen sind nicht-leere Hex-Ziffernfolgen im Deckel: Sie
        // werden verglichen, sortiert und protokolliert — alles andere
        // hätte hier nichts zu suchen.
        fn kennung(s: &str) -> bool {
            !s.is_empty() && s.len() <= MAX_EVENT_ID_LEN && s.bytes().all(|b| b.is_ascii_hexdigit())
        }
        // `team`/`player` sind Koordinaten in einem 2×2-Raster.
        fn seite(v: i64) -> bool {
            v == 0 || v == 1
        }

        kennung(&self.id)
            // Eine Rücknahme MUSS ihr Ziel nennen, alles andere darf es
            // nicht — sonst gäbe es Rücknahmen ins Leere und stille
            // Verweise, die der Renderer verschieden deuten könnte.
            && match self.kind {
                // Eine Rücknahme, die sich selbst zurücknimmt, ist ein
                // Zirkel: Die Projektion in E4 müsste entscheiden, ob sie
                // im Raster steht oder nicht, und beide Antworten wären
                // falsch.
                EventKind::Retract => kennung(&self.retracts) && self.retracts != self.id,
                _ => self.retracts.is_empty(),
            }
            && self.seq >= 0
            && (1..=MAX_TIMELINE_SETS as i64).contains(&self.set)
            && (0..=MAX_RALLIES_PER_SET as i64).contains(&self.after_n)
            && (0..=MAX_START_SCORE).contains(&self.score_a)
            && (0..=MAX_START_SCORE).contains(&self.score_b)
            && seite(self.team)
            && seite(self.player)
            && seite(self.receiver_team)
            && seite(self.receiver_player)
    }
}

/// Gültigkeit eines ganzen Ereignis-Bestands: Zahl **und** Inhalt.
///
/// Freie Funktion statt Methode, weil der Bestand auf dem Draht ein
/// schlichtes `Vec` ist — Host, Relay und Store prüfen damit dieselbe
/// Grenze.
pub fn match_events_valid(events: &[MatchEvent]) -> bool {
    events.len() <= MAX_EVENTS_PER_MATCH && events.iter().all(MatchEvent::is_valid)
}

/// Nachrichten vom Server an das Tablet.
// Wie bei [`HostFrame`]: `MatchAssigned` trägt ein volles `MatchBrief` und
// ist damit deutlich größer als die schlanken Varianten — bewusst
// akzeptiert. Diese Frames gehen serialisiert über die Leitung, sie liegen
// nicht in großer Zahl auf dem Stack; Boxing bliebe an jeder
// Konstruktions- und Match-Stelle hängen, ohne realen Gewinn.
#[allow(clippy::large_enum_variant)]
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
    StateRestore {
        state: String,
        /// A2 / ADR 0017 (Reconnect-Wahrheit): Ist der Server bzw. Relay im
        /// OWNERSHIP-Modus? `true` → der Server hat die Autorität berechnet
        /// (Slot-Halter gewinnt) und das Tablet FOLGT `authoritative`.
        /// `false` → Legacy-Modus (Config `reconnect_legacy_rev`) ODER ein
        /// ALTES Server-Frame ganz ohne dieses Feld: dann greift auf dem
        /// Tablet die bestehende rev-Logik. Dieses Gate hält „Legacy" sauber —
        /// ohne es würde ein Tablet `authoritative` auch im Legacy-Fall blind
        /// befolgen und der Reconnect-Bug bliebe offen. `#[serde(default)]` →
        /// altes Frame = `false` = rev-Fallback (Auto-Update-sicher).
        #[serde(default)]
        ownership_active: bool,
        /// A2 / ADR 0017 (Reconnect-Wahrheit): vom Server bzw. Relay
        /// berechnete Autorität — NUR wirksam, wenn `ownership_active`. `true`
        /// → das Tablet setzt seinen LOKALEN Stand durch (es ist der
        /// Slot-Halter/Reclaimer und damit die Wahrheit); `false` → es
        /// ADOPTIERT den mitgeschickten `state` (bewusste Übernahme eines
        /// fremden Felds oder finalisiertes Match). `#[serde(default)]` hält
        /// ältere Nachrichten OHNE das Feld lesbar.
        #[serde(default)]
        authoritative: bool,
        /// Diagnose / Tablet-Selbsttest: `epoch` (der `claim_court`-Token) und
        /// Geräte-ID des aktuellen Slot-Halters. Erlauben dem Tablet zu prüfen
        /// „bin ich der Halter?". `#[serde(default)]` = abwärtskompatibel.
        #[serde(default)]
        owner_epoch: u64,
        #[serde(default)]
        owner_device: String,
    },
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

/// Serde-Hilfe: `false` ist der Normalfall und muss nicht über die Leitung
/// (siehe [`ResultResponse::permanent`] und [`HostFrame::ResultAck`]).
fn is_false(b: &bool) -> bool {
    !*b
}

/// Antwort auf eine Ergebnis-Übermittlung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    /// Ist die Ablehnung **dauerhaft**? Dann wird derselbe Payload auch beim
    /// hundertsten Versuch abgelehnt, und das Tablet soll aufhören zu
    /// wiederholen und stattdessen den Grund zeigen.
    ///
    /// Die Trennlinie ist bewusst hart und liegt nicht beim Gefühl, sondern
    /// bei der Frage: **Hängt die Ablehnung allein am gesendeten Payload?**
    ///
    /// - Ja (Satzliste unstimmig, Satz passt nicht zur Zählweise) ⇒
    ///   `permanent`. Wiederholen kann das Ergebnis nie retten.
    /// - Nein (kein Match auf dem Feld, Feld inzwischen anders belegt) ⇒
    ///   **nicht** dauerhaft. Diese Gründe hängen am Serverzustand, und der
    ///   ändert sich mit dem nächsten BTP-Poll — ein noch nicht geladener
    ///   Snapshot darf kein Ergebnis wegwerfen.
    ///
    /// `#[serde(default)]` = `false`: Ältere Hosts kennen das Feld nicht,
    /// deren Ablehnungen gelten wie bisher als wiederholbar. Und weil `false`
    /// der Normalfall ist, bleibt es auch beim Senden weg — die Erfolgs-
    /// antwort ist damit Byte für Byte dieselbe wie vorher.
    #[serde(default, skip_serializing_if = "is_false")]
    pub permanent: bool,
}

impl ResultResponse {
    /// Erfolgsantwort.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            permanent: false,
        }
    }

    /// Fehlerantwort mit Meldung — **wiederholbar**: Der Grund liegt am
    /// Zustand des Turnier-PCs, nicht am Ergebnis selbst.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            permanent: false,
        }
    }

    /// Fehlerantwort für eine **dauerhafte** Ablehnung: Genau dieser Payload
    /// wird nie angenommen. Das Tablet stellt das Wiederholen ein und zeigt
    /// die Meldung.
    pub fn dauerhaft(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            permanent: true,
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
    /// Effektive Hallen-Farbe (Hex `#rrggbb`, Spec hallen-farben, ADR 0033).
    /// `None` bei Ein-Hallen-Turnieren und von alten Hosts — Relay und
    /// Seiten fallen dann auf farblos zurück.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hall_color: Option<String>,
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

// ─────────────────── TL-Web (Turnierleitungs-Oberfläche) ──────────────────
//
// Turnierleitungs-Geräte sind eine **dritte Client-Klasse** neben Tablets und
// Monitoren: viele je Namespace, nicht feldgebunden, ausschließlich schreibend
// **über den Host**. Sie tauchen nie in der Tablet-Liste auf und übernehmen nie
// eine Court-Session — R4 („ein aktives Tablet je Court") bleibt unberührt.
// Grundlage: docs/features/turnierleitung-web.md, ADR 0012 + 0012.

/// Was ein Turnierleitungs-Gerät auf einem Feld **vorgefunden** hat, als es die
/// Aktion auslöste. Der Host lehnt ab, wenn die Erwartung nicht mehr stimmt —
/// so überschreiben zwei gleichzeitig arbeitende Geräte einander nicht
/// stillschweigend.
///
/// Bewusst ein Enum statt `Option<i64>`: „Feld war leer" und „Feld hatte Spiel
/// X" müssen unterscheidbar bleiben. Mit `Option<Option<i64>>` wäre das über
/// Serde nicht sauber abbildbar (fehlend und `null` fielen zusammen).
///
/// **Ohne `#[serde(default)]`, bewusst.** Diese Typen sind neu — es gibt keine
/// ältere Gegenstelle, die ein Default schonen müsste. Ein fehlendes `expect`
/// würde den Konfliktschutz stillschweigend abschalten, und genau darauf ruht
/// die Mehrbenutzer-Fähigkeit. Wer keine Erwartung hat, sendet `any`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CourtExpectation {
    /// Keine Erwartung — der Host prüft nichts zusätzlich. Muss ausdrücklich
    /// gesendet werden.
    Any,
    /// Das Feld war leer und soll es noch sein.
    Free,
    /// Auf dem Feld stand genau dieses Spiel.
    Match {
        #[serde(rename = "matchId")]
        match_id: i64,
    },
}

/// Welche Partei ein erneuter Aufruf meint. Bewusst ein Enum statt einer Zahl:
/// Der gezielte Nachruf **einer** fehlenden Partei ist der Zweck der Aktion,
/// und ein Zahlenfeld ließe Werte zu, die niemand behandelt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepCallSide {
    /// Beide Parteien erneut aufrufen.
    Both,
    Team1,
    Team2,
}

impl PrepCallSide {
    /// Alle Varianten — Grundlage des Wire-Roundtrip-Tests.
    pub const ALL: [PrepCallSide; 3] =
        [PrepCallSide::Both, PrepCallSide::Team1, PrepCallSide::Team2];
}

/// Dienst eines Officials an einem Spiel. BTP: `Official1ID` = Schiedsrichter,
/// `Official2ID` = Aufschlagrichter (an der BTP-Maske verifiziert).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlOfficialRole {
    /// Schiedsrichter.
    Sr,
    /// Aufschlagrichter.
    Ar,
}

/// Ein einzelnes Panel innerhalb eines [`TlPanelProfileWire`]: Sichtbarkeit +
/// relative Höhe (Spec tl-web-panelsystem). `key` benennt den Abschnitt
/// (`"courts"`, `"walkovers"`, `"scorekeepers"`, `"officials"`, `"queue"`,
/// `"finished"`) — als String statt Enum, weil künftige Panels ohne
/// Protokolländerung dazukommen können sollen; die Panel-**Liste** ist
/// Konfiguration, kein geschlossener Aktions-Satz wie [`TlAction`] selbst.
/// (Bis 15.08.2026 gab es statt `"queue"` vier getrennte Schlüssel
/// `queue_called`/`queue_ready`/`queue_waiting`/`queue_no_hall` — seit ADR
/// 0026 ein einziges Panel, Status ist ein Zeilen-Abzeichen.)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TlPanelSettingWire {
    pub key: String,
    pub visible: bool,
    #[serde(rename = "heightFr")]
    pub height_fr: f64,
    /// Zugeklappt? Zweite, von `visible` unabhängige Dimension:
    /// ausgeblendet = gar nicht da, zugeklappt = Kopfzeile sichtbar,
    /// Inhalt eingeklappt.
    ///
    /// `#[serde(default)]`, weil ein Profil aus einem älteren Browser das
    /// Feld nicht mitschickt — `false` (aufgeklappt) ist dann das
    /// bisherige Verhalten. Der Host reicht den Wert nur durch; die
    /// Anzeige-Logik liegt in `tl.html`.
    #[serde(default)]
    pub collapsed: bool,
    /// Spalte des Mehrspalten-Layouts, **1-basiert** (passend zu
    /// [`TlPanelProfileWire::columns`]). `0`/fehlend = Spalte 1 — außer in
    /// einem Profil ganz ohne `columns`, wo `tl.html` die Aufteilung aus
    /// `display.listPosition` ableitet.
    ///
    /// `#[serde(default)]` aus demselben Grund wie `collapsed`: Ein Profil
    /// aus einem älteren Browser schickt das Feld nicht mit, und „Spalte 1"
    /// ist die harmlose Lesart. Der Host reicht nur durch.
    #[serde(default)]
    pub column: u8,
}

/// Seite, auf der die Warteliste/Ergebnis-Spalte im Panel-System erscheint
/// (Spec tl-web-panelsystem).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlListPositionWire {
    #[default]
    Right,
    Bottom,
}

/// Achse des Panels „Spielzeiten" (Spec `tl-sicht-feinschliff`, Punkt 1).
///
/// `Group` ist der Vorgabewert und zugleich die fachliche Voreinstellung:
/// Ein Profil aus einem älteren Browser-Stand kennt das Feld nicht und
/// landet damit auf der bisherigen Ansicht (Klasse × Disziplin).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlTimeStatsAxisWire {
    #[default]
    Group,
    Class,
    Discipline,
    Hall,
}

/// Turnierweite Anzeige-Optionen eines Panel-Profils (Spec
/// tl-web-panelsystem) — dieselben Schalter, die vorher als lose
/// `localStorage`-Werte in `tl.html` lebten.
///
/// Container-weites `#[serde(default)]` wie beim Config-Zwilling
/// `TlDisplaySettings`: Ein Profil aus einem älteren Browser-Stand darf
/// nie als Ganzes an einem fehlenden Häkchen-Feld scheitern — jedes
/// fehlende Feld liest sich als „aus" (Review 17.08.2026).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TlDisplaySettingsWire {
    #[serde(rename = "showNumbers")]
    pub show_numbers: bool,
    #[serde(rename = "showNations")]
    pub show_nations: bool,
    #[serde(rename = "showClubNames")]
    pub show_club_names: bool,
    #[serde(rename = "showClubLogos")]
    pub show_club_logos: bool,
    #[serde(rename = "showDiscipline")]
    pub show_discipline: bool,
    #[serde(rename = "showRound")]
    pub show_round: bool,
    #[serde(rename = "showGroup")]
    pub show_group: bool,
    /// Geschätzte Restzeit laufender Spiele am Feld (Spec
    /// `spielzeiten-prognose`, Etappe D). `#[serde(default)]`, weil alte
    /// Browser-Stände Profile ohne dieses Feld speichern/senden.
    #[serde(rename = "showCourtRemaining", default)]
    pub show_court_remaining: bool,
    /// Aufrufe am Feld beliebig oft und auch bei laufendem Spiel anbieten
    /// (Feldtest 17.08.2026). `#[serde(default)]` wie `showCourtRemaining`:
    /// Alte Browser-Stände kennen das Feld nicht — dann bleibt der Deckel
    /// bei drei Aufrufen, das bisherige Verhalten.
    #[serde(rename = "unlimitedCourtCalls", default)]
    pub unlimited_court_calls: bool,
    #[serde(rename = "listPosition")]
    pub list_position: TlListPositionWire,
    /// Achse des Panels „Spielzeiten" (Spec `tl-sicht-feinschliff`).
    /// `#[serde(default)]` wie die beiden Schalter darüber: Ein Profil aus
    /// einem älteren Browser-Stand kennt das Feld nicht und bleibt damit
    /// auf der bisherigen Ansicht.
    #[serde(rename = "timeStatsAxis", default)]
    pub time_stats_axis: TlTimeStatsAxisWire,
}

/// Ein benanntes Panel-Profil, wie es über die Wire-Grenze reist — sowohl als
/// Payload von [`TlAction::ProfileSave`] (Browser → Host) als auch,
/// unverändert wiederverwendet, als Element von `TlState.profiles` (Host →
/// Browser; Spec tl-web-panelsystem, ADR 0025).
///
/// **Bewusst kein rohes `config::TlPanelProfile` über die Wire-Grenze**
/// (dieselbe Trennung wie bei `TlDevice`/[`TlAuthDevice`]): `relay-proto`
/// bleibt unabhängig von `src-tauri::config` — dort liegt die
/// Host-interne, um Persistenz-Belange erweiterte Fassung. Der Host
/// übersetzt in beide Richtungen (`tablet::tl::profile_to_wire`/
/// `profile_from_wire`-Äquivalente).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TlPanelProfileWire {
    /// Leer bei [`TlAction::ProfileSave`] = „neu anlegen" — der Host vergibt
    /// dann eine Kennung.
    pub id: String,
    pub name: String,
    /// Reihenfolge = Panel-Reihenfolge auf der Seite.
    pub panels: Vec<TlPanelSettingWire>,
    pub display: TlDisplaySettingsWire,
    /// Spaltenzahl des Seitenlayouts (1…3). `0`/fehlend = aus
    /// `display.listPosition` ableiten (Bestandsprofile sehen dadurch
    /// unverändert aus) — die Ableitung sitzt in `tl.html`, nicht hier.
    #[serde(default)]
    pub columns: u8,
    /// Relative Spaltenbreiten wie `heightFr` bei den Panel-Höhen; leer =
    /// gleichmäßig. Der Host reicht sie nur durch.
    #[serde(rename = "columnWidths", default)]
    pub column_widths: Vec<f64>,
    /// Vom Client mitgeschickt, aber beim Speichern vom Host **verworfen**
    /// — der Host stempelt immer seine eigene Zeit (Last-Write-Wins-Marker;
    /// verhindert, dass eine falsch gehende Client-Uhr eine neuere Änderung
    /// verdrängt).
    #[serde(rename = "updatedAtMs")]
    pub updated_at_ms: u64,
}

/// Höchstzahl der Panel-Profile im Katalog (Spec tl-web-panelsystem).
///
/// Geteilt wie [`MAX_TL_DEVICES_MIRRORED`]: Der Katalog reist vollständig in
/// jedem `TlState`-Push mit (R4 — `MAX_TL_STATE_LEN`), eine unbegrenzte
/// Liste könnte den Zustand über das Limit treiben und die ganze
/// Turnierleitungs-Oberfläche stumm schalten. 32 ist großzügig für die
/// tatsächlichen Anwendungsfälle (Tablet/Wandmonitor-Varianten je Halle),
/// ohne dass eine versehentliche Endlos-Anlage den Zustand sprengen kann.
pub const MAX_TL_PROFILES: usize = 32;

/// Die Aktionen, die ein Turnierleitungs-Gerät auslösen darf — ein **bewusst
/// geschlossener** Satz (ADR 0011). Was hier nicht steht, ist nicht
/// darstellbar; der Relay leitet nur weiter, entschieden und validiert wird
/// ausschließlich im Host (R5).
///
/// **Kein Feld trägt `#[serde(default)]`.** Anders als bei den Tablet-Typen
/// gibt es hier keine ältere Gegenstelle zu schonen; ein stillschweigend
/// ergänzter Wert würde stattdessen die jeweils weitreichendere oder
/// ungeprüfte Variante auslösen.
///
/// **Kein `Eq`** (nur `PartialEq`): `ProfileSave` trägt über
/// [`TlPanelProfileWire`]/[`TlPanelSettingWire`] ein `f64` (`height_fr`),
/// und `f64` selbst ist nicht `Eq`. Für Tests/Vergleiche reicht
/// `PartialEq` — `assert_eq!` verlangt kein `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TlAction {
    /// Spiel auf ein freies Feld legen.
    AssignCourt {
        #[serde(rename = "courtId")]
        court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
        expect: CourtExpectation,
    },
    /// Feld räumen.
    FreeCourt {
        #[serde(rename = "courtId")]
        court_id: i64,
        expect: CourtExpectation,
    },
    /// Spiel von einem Feld auf ein anderes umhängen. Bewusst **eine** Aktion
    /// statt „freigeben + zuweisen": nur so wird das Umhängen in einem
    /// einzigen BTP-Request geschrieben — sonst wäre dazwischen ein Zustand
    /// sichtbar, in dem das Spiel auf keinem Feld steht, und die automatische
    /// Vergabe könnte das Zielfeld wegschnappen.
    MoveMatch {
        #[serde(rename = "fromCourtId")]
        from_court_id: i64,
        #[serde(rename = "toCourtId")]
        to_court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
        #[serde(rename = "expectFrom")]
        expect_from: CourtExpectation,
        #[serde(rename = "expectTo")]
        expect_to: CourtExpectation,
    },
    /// Spiele in die Vorbereitung rufen (optional in eine bestimmte Halle).
    CallPreparation {
        #[serde(rename = "matchIds")]
        match_ids: Vec<i64>,
        #[serde(
            rename = "locationId",
            skip_serializing_if = "Option::is_none",
            default
        )]
        location_id: Option<i64>,
    },
    /// Einen Vorbereitungs-Aufruf zurücknehmen.
    RetractPreparation {
        #[serde(rename = "matchId")]
        match_id: i64,
    },
    /// Einem noch nicht vergebenen Spiel eine **Halle** geben, ohne es aufs
    /// Feld zu legen. Leerer Name nimmt die Zuweisung zurück.
    ///
    /// Bewusst host-lokal (bts-light + Liveticker, nie zurück nach BTP): BTP
    /// kann an angesetzten Spielen durchaus einen Spielort tragen, wenn das
    /// Turnier die Spalte pflegt (gemessen 09.08.2026) — aber ein
    /// `SENDUPDATE` mit `LocationID` beantwortet BTP zwar mit `Result=1`,
    /// verwirft den Wert dabei jedoch (gemessen 10.08.2026, siehe
    /// `btp_location_probe.rs`). Rückschreiben ist also nicht möglich; für
    /// Turniere ohne gepflegte Spalte bleibt dies der einzige Weg, einem
    /// wartenden Spiel überhaupt eine Halle zu geben, wenn keine
    /// Disziplin-Regel greift.
    SetHall {
        #[serde(rename = "matchId")]
        match_id: i64,
        hall: String,
    },
    /// Ein Spiel von der automatischen Feldvergabe ausnehmen oder die
    /// Ausnahme zurücknehmen (Spec `feldvergabe-ausnahme`). Betrifft
    /// ausschließlich `sync.rs::auto_assign` — manuelles Zuweisen
    /// (`AssignCourt`/`MoveMatch`) bleibt für ein ausgenommenes Spiel
    /// jederzeit möglich, bewusst ohne BTP-Rückschreibung (rein
    /// host-lokaler Bedienzustand, wie `SetHall`).
    ExcludeFromAutoAssign {
        #[serde(rename = "matchId")]
        match_id: i64,
        excluded: bool,
    },
    /// Ein noch nicht gerufenes Spiel in der manuellen Präfix-Reihenfolge
    /// vor ein anderes ziehen (Spec `spielliste-manuelle-reihenfolge`,
    /// ADR 0026); `before_match_id = None` heißt „ans Ende des aktuell
    /// sichtbaren Präfix-Blocks", nicht ans Ende der Gesamtliste. Die
    /// Reihenfolge gilt turnierweit; ein Hallen-Parameter war hier nie
    /// nötig und ist es seit ADR 0026 auch begrifflich nicht mehr — das
    /// Frontend zieht nur zwei Match-IDs relativ zueinander.
    QueueReorder {
        #[serde(rename = "matchId")]
        match_id: i64,
        #[serde(
            rename = "beforeMatchId",
            skip_serializing_if = "Option::is_none",
            default
        )]
        before_match_id: Option<i64>,
    },
    /// Die komplette manuelle Spielreihenfolge auf einmal verwerfen —
    /// bewusst ohne Teil-Reset, weder je Halle noch je Spiel (Nicht-Ziel
    /// der Spec).
    QueueOrderReset,
    /// Automatische Hallen-Vorverteilung schalten (Spec
    /// `hallen-vorverteilung`, ADR 0029/0030). Schalter und Fenstergröße
    /// reisen **atomar** — die Seite kennt beim Umschalten das aktuelle x,
    /// ein Zwischenzustand „an mit altem x" kann nicht entstehen. Der Host
    /// validiert (klemmt `window`, lehnt Einschalten bei gesetzter aktiver
    /// Halle ab) und persistiert in der Config.
    SetHallPrefill {
        enabled: bool,
        /// Fenstergröße x; 0 = automatisch (Gesamtzahl der Spielfelder).
        window: u32,
    },
    /// Alle **automatisch** verteilten Hallen räumen (E10) — bewusst eine
    /// eigene, destruktive Aktion und nie Nebeneffekt eines Set. Hand-,
    /// Regel- und Aufruf-Hallen bleiben unberührt.
    ClearAutoHalls,
    /// Ein Feld sperren oder freigeben (Spec `tl-web-felder-sperren`).
    ///
    /// Ein gesperrtes Feld bekommt von der automatischen Vergabe kein Spiel
    /// mehr; ein bereits laufendes bleibt unangetastet und zählt zu Ende.
    /// BTP kennt die Sperre nicht (R2) — sie ist bts-light-eigen und wird
    /// nie geschrieben.
    ///
    /// `locked` trägt den **Zielzustand**, nicht „umschalten": Bei zwei
    /// Turnierleitungs-Geräten wäre ein Umschalten nicht eindeutig — wer
    /// zuletzt tippt, bekäme womöglich das Gegenteil dessen, was auf seinem
    /// Schirm stand.
    LockCourt {
        #[serde(rename = "courtId")]
        court_id: i64,
        locked: bool,
    },
    /// Erneuter Aufruf eines Spiels, das bereits auf dem Feld steht (2./3.
    /// Aufruf). Die **Stufe zählt der Host** — sie darf nicht im Browser
    /// leben, sonst zählt bei mehreren Geräten jedes für sich.
    ///
    /// `side` grenzt den Aufruf auf **eine Partei** ein (Vorbild
    /// [`TlAction::AnnouncePrepCall`]) — für den häufigen Fall, dass nur
    /// eine Seite fehlt. `None`/fehlend = beide Parteien.
    AnnounceCourtCall {
        #[serde(rename = "courtId")]
        court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
        /// **Ausnahme von der „kein `#[serde(default)]`"-Regel dieses
        /// Enums** — dieselbe Abwägung wie bei `winner` in
        /// [`TlAction::EnterResult`] und `location_id` in
        /// [`TlAction::CallPreparation`]: `None` ist hier die
        /// **neutralere**, nicht die weitreichendere Variante. Ein
        /// fehlendes Feld löst genau das bisherige Verhalten aus (beide
        /// Parteien rufen), niemals einen gezielteren oder ungeprüften
        /// Eingriff. Genau deshalb darf ein älterer Browser, der `side`
        /// noch nicht kennt, weiter senden.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        side: Option<PrepCallSide>,
    },
    /// Erneuter Aufruf eines in Vorbereitung gerufenen Spiels; `side` grenzt
    /// auf eine Partei ein.
    AnnouncePrepCall {
        #[serde(rename = "matchId")]
        match_id: i64,
        side: PrepCallSide,
    },
    /// Ergebnis eintragen.
    ///
    /// `retired` deckt die **Aufgabe mitten im Spiel** ab — dann sind die
    /// Sätze unvollständig und `winner` benennt die Partei, die weiterkommt.
    /// Das ist dieselbe Fähigkeit, die ein Tablet über [`ResultBody`] schon
    /// hat. Eine **kampflose** Wertung läuft dagegen nicht hierüber, sondern
    /// über [`TlAction::ConfirmWalkover`].
    ///
    /// `overwrite` verlangt ausdrücklich das Überschreiben einer bereits
    /// gewerteten Begegnung — der Host prüft zusätzlich, ob das folgenlos
    /// möglich ist.
    EnterResult {
        #[serde(rename = "matchId")]
        match_id: i64,
        sets: Vec<SetAb>,
        retired: bool,
        /// 1 oder 2; bei regulärem Ende aus den Sätzen ableitbar und dann
        /// `None`, bei Aufgabe zwingend. Hier ist ein Default vertretbar:
        /// „nicht angegeben" heißt „aus den Sätzen ableiten" und ist damit
        /// die neutrale, nicht die weitreichendere Lesart.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        winner: Option<i64>,
        overwrite: bool,
    },
    /// Einen Walkover-Vorschlag für die gewählten Spiele werten.
    ConfirmWalkover {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "matchIds")]
        match_ids: Vec<i64>,
    },
    /// Einen Walkover-Vorschlag verwerfen.
    DismissWalkover {
        #[serde(rename = "proposalId")]
        proposal_id: String,
    },
    /// Einen Wartenden an den Anfang der Zähltafelbediener-Schlange ziehen.
    ScorekeeperAdvance { key: String },
    /// Einen Wartenden aus der Schlange entfernen.
    ScorekeeperRemove { key: String },
    /// Manuell einen Zähltafelbediener einreihen.
    ScorekeeperAdd { names: Vec<String> },
    /// Automatische Feldvergabe an-/abschalten.
    SetAutoAssign { enabled: bool },

    // ── Schiedsrichter (Spec schiedsrichter-management) ──────────────
    /// Einem Spiel einen Schiedsrichter oder Aufschlagrichter zuweisen.
    ///
    /// Die Zuweisung hängt am **Spiel**, nicht am Feld — nach Spielende
    /// bleibt sie ihm zugeordnet (Grundlage der Einsatz-Ableitung). Das
    /// Feld reist trotzdem mit: Es ordnet die Aktion demselben Feld zu wie
    /// die übrigen Feld-Aktionen und schützt so vor zwei gleichzeitigen
    /// Zugriffen auf dasselbe Feld.
    OfficialAssign {
        #[serde(rename = "courtId")]
        court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
        #[serde(rename = "officialId")]
        official_id: i64,
        role: TlOfficialRole,
    },
    /// Eine Zuweisung lösen.
    OfficialClear {
        #[serde(rename = "courtId")]
        court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
        role: TlOfficialRole,
    },
    /// Einen Schiedsrichter pausieren oder wieder einteilen.
    OfficialPause {
        #[serde(rename = "officialId")]
        official_id: i64,
        paused: bool,
    },
    /// Einen Schiedsrichter in der Reihenfolge vor einen anderen ziehen;
    /// ohne Ziel ans Ende.
    OfficialReorder {
        #[serde(rename = "officialId")]
        official_id: i64,
        #[serde(
            rename = "beforeOfficialId",
            skip_serializing_if = "Option::is_none",
            default
        )]
        before_official_id: Option<i64>,
    },
    /// Stammverein pflegen (BTP überträgt am Official keinen).
    OfficialSetClub {
        #[serde(rename = "officialId")]
        official_id: i64,
        club: String,
    },
    /// Sperrlisten setzen (ersetzt beide Listen).
    ///
    /// Diese Angaben sind Personendaten: Sie reisen **nur** in dieser
    /// Aktion und in der Antwort der gezielten Leseroute — nie im
    /// Broadcast-Zustand, den alle Geräte bekommen.
    OfficialBlocklistSet {
        #[serde(rename = "officialId")]
        official_id: i64,
        clubs: Vec<String>,
        players: Vec<i64>,
    },
    /// Die drei Schalter eines Felds setzen (SR-Rotation, AR-Rotation,
    /// Zähltafelbediener-Vergabe).
    OfficialsCourtToggle {
        #[serde(rename = "courtId")]
        court_id: i64,
        sr: bool,
        ar: bool,
        operator: bool,
    },
    /// Schiedsrichter und Aufschlagrichter eines Felds ansagen (manueller
    /// Knopf — eine nachträgliche Zuweisung sagt nie von selbst an).
    AnnounceOfficials {
        #[serde(rename = "courtId")]
        court_id: i64,
    },
    /// Den Zähltafelbediener eines Felds nachrufen („… bitte als
    /// Tabletbedienung melden", ADR 0007 / Spec `tl-sicht-feinschliff`
    /// Punkt 2).
    ///
    /// **Kein Spieler-Aufruf.** Der Host führt dafür einen eigenen Zähler;
    /// `call_stages` und `prep_call_stages` bleiben unberührt, sonst zöge
    /// ein Nachruf an die Bedienung die angezeigte Aufruf-Zahl der Spieler
    /// hoch.
    AnnounceScorekeeper {
        #[serde(rename = "courtId")]
        court_id: i64,
    },
    /// „Feld X. Bitte mit dem Spielen beginnen." — die Aufforderung an ein
    /// besetztes Feld, auf dem noch kein Punkt gefallen ist (Spec
    /// `tl-sicht-feinschliff`, Punkt 3).
    ///
    /// **Ausdrücklich kein Aufruf.** Der Host lässt `call_stages` dabei
    /// unberührt: kein Stufenwort in der Ansage, kein Abzeichen an der
    /// Kachel, keine Änderung an der Fälligkeit. Die Spieler stehen ja
    /// längst am Feld — gerufen wurde vorher.
    ///
    /// `match_id` reist mit, damit die Ansage nicht mehr erklingt, wenn
    /// inzwischen ein anderes Spiel auf dem Feld steht.
    AnnounceStartPlay {
        #[serde(rename = "courtId")]
        court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
    },

    // ── Panel-Profile (Spec tl-web-panelsystem, ADR 0024/0025) ──────────
    /// Ein Profil anlegen oder überschreiben (Upsert nach `id`; leere `id`
    /// = neu anlegen, der Host vergibt dann eine Kennung). Last-Write-Wins
    /// — bewusst KEINE Konfliktprüfung gegen ein zuvor gesehenes
    /// `updated_at_ms`: Die Spec verlangt ausdrücklich keine Fehlermeldung
    /// bei gleichzeitiger Bearbeitung durch zwei Geräte.
    ProfileSave { profile: TlPanelProfileWire },
    /// Ein Profil löschen. Geräte, die es trugen, fallen beim nächsten Poll
    /// auf das Standardprofil zurück — kein Fehlerzustand (Spec, Grill-Punkt
    /// 7), Löschen eines bereits verschwundenen Profils ist ein No-Op.
    ProfileDelete {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
    /// Für das AUFRUFENDE Gerät ein Profil wählen. Bewusst **ohne**
    /// Geräte-Feld: Das aufrufende Gerät ist aus der Bearer-Token-Auth
    /// bekannt — ein Client-Feld könnte sonst ein fremdes Gerät umbiegen
    /// (Sicherheitsgrenze, von `tablet::tl::execute` durchgesetzt). Leere
    /// `profile_id` ("Standard") ist immer gültig.
    ProfileSelect {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
    /// Das turnierweite Standardprofil setzen (leer = eingebautes
    /// Standardprofil, gilt für jedes Gerät ohne eigene Wahl).
    ProfileSetDefault {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
}

/// Grund einer Ablehnung — **maschinenlesbar**, damit die Seite gezielt
/// reagieren kann (Feld hervorheben, Auswahl zurücksetzen, neu laden), statt
/// einen Fehlertext zu zerlegen. Der Klartext daneben ist für den Menschen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlErrorCode {
    /// Das Feld ist inzwischen belegt.
    CourtTaken,
    /// Das Feld ist inzwischen leer (erwartetes Spiel steht nicht mehr dort).
    CourtFree,
    /// Das Feld ist gesperrt.
    CourtLocked,
    /// Das Spiel steht bereits auf einem anderen Feld.
    MatchElsewhere,
    /// Die Disziplin/Klasse darf in dieser Halle nicht gespielt werden.
    HallNotAllowed,
    /// Die Begegnung ist in BTP bereits gewertet (ohne `overwrite`).
    AlreadyScored,
    /// Überschreiben abgelehnt, weil der Sieger schon in ein Folgespiel wirkt.
    CorrectionBlocked,
    /// Bereits erledigt (z. B. Walkover-Vorschlag von einem anderen Gerät).
    AlreadyHandled,
    /// Die Ansicht, auf der die Aktion beruht, war zu alt.
    StaleView,
    /// In dieser Rolle oder Betriebsart nicht erlaubt (z. B. Slave-Modus).
    NotAllowed,
    /// Diese bts-light-Version kennt die Aktion noch nicht — ein
    /// Versions-, kein Rechteproblem. Bewusst von `NotAllowed` getrennt:
    /// Die Seite soll „bitte aktualisieren" sagen können statt den Nutzer
    /// nach Rollen oder Betriebsart suchen zu lassen.
    Unsupported,
    /// In der Zielhalle ist kein Ansage-Gerät verbunden.
    NoAnnouncer,
    /// BTP hat den Schreibvorgang abgelehnt oder war nicht erreichbar.
    BtpError,
    /// Der Turnier-PC ist nicht mit dem Relay verbunden oder hat nicht
    /// geantwortet. **Nur der Relay** vergibt diesen Code — er ist die
    /// Grundlage dafür, dass die Seite „bts-light ist nicht verbunden" sagen
    /// kann, statt einen leeren Stand als „alle Felder frei" zu zeigen.
    HostOffline,
}

impl TlErrorCode {
    /// Alle Codes — Grundlage des Wire-Roundtrip-Tests. Wächst das Enum,
    /// muss diese Liste mitwachsen (der Test erzwingt es nicht, aber der
    /// fehlende Eintrag fiele bei der nächsten Durchsicht auf).
    pub const ALL: [TlErrorCode; 14] = [
        TlErrorCode::CourtTaken,
        TlErrorCode::CourtFree,
        TlErrorCode::CourtLocked,
        TlErrorCode::MatchElsewhere,
        TlErrorCode::HallNotAllowed,
        TlErrorCode::AlreadyScored,
        TlErrorCode::CorrectionBlocked,
        TlErrorCode::AlreadyHandled,
        TlErrorCode::StaleView,
        TlErrorCode::NotAllowed,
        TlErrorCode::Unsupported,
        TlErrorCode::NoAnnouncer,
        TlErrorCode::BtpError,
        TlErrorCode::HostOffline,
    ];
}

/// Antwort des Hosts auf eine TL-Aktion. `state_rev` ist die Revision des
/// Zustands **nach** der Aktion — die Seite erkennt daran, ob ihr nächster
/// Abruf schon das Ergebnis enthält.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub code: Option<TlErrorCode>,
    /// Ausgeführt, aber mit Einschränkung — etwa „in dieser Halle ist kein
    /// Ansage-Gerät verbunden". Ausdrücklich **kein** Fehler.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub warning: Option<String>,
    #[serde(rename = "stateRev", default)]
    pub state_rev: u64,
}

impl TlResponse {
    /// Erfolg ohne Einschränkung.
    pub fn ok(state_rev: u64) -> Self {
        Self {
            ok: true,
            error: None,
            code: None,
            warning: None,
            state_rev,
        }
    }

    /// Ablehnung mit Grund im Klartext und als Code.
    pub fn err(code: TlErrorCode, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(error.into()),
            code: Some(code),
            warning: None,
            state_rev: 0,
        }
    }

    /// Hinweis an eine Erfolgsantwort hängen.
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warning = Some(warning.into());
        self
    }

    /// Die Revision nachtragen, auf die sich die Seite neu ausrichten soll.
    /// Gerade **Ablehnungen** brauchen sie: Nach „Feld wurde gerade von
    /// jemand anderem belegt" soll die Ansicht auf den echten Stand springen
    /// — ohne Revision wüsste sie nicht, ob ihr nächster Abruf den Stand nach
    /// dem fremden Zugriff schon enthält.
    pub fn with_state_rev(mut self, state_rev: u64) -> Self {
        self.state_rev = state_rev;
        self
    }
}

/// Frames von bts-light (dem „Host" eines Namespace) an den Relay.
// `MatchAssigned` trägt ein volles `MatchBrief` und ist damit deutlich
// größer als die schlanken Varianten (`MatchCleared` etc.) — bewusst
// akzeptiert: Diese Frames werden serialisiert übertragen, nicht in großer
// Zahl auf dem Stack gehalten; Boxing würde ~20 Konstruktions-/Match-
// Stellen aufblähen ohne realen Gewinn.
#[allow(clippy::large_enum_variant)]
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
    /// Satzstand-Spiegel des Hosts (autoritativ). Im LAN(+Cloud)-Betrieb
    /// zählen die Tablets am Relay vorbei direkt gegen den Host — ohne diesen
    /// Spiegel bleiben Cloud-Monitor und Cloud-Übersicht auf 0:0 stehen
    /// (Turnier-Befund 13.08.2026). Der Relay übernimmt `sets` (und, falls
    /// vorhanden, den **opaken** Tablet-`court_state` mit Aufschlag/Pause) in
    /// seinen Anzeige-Cache und weckt die Monitor-Abonnenten — derselbe Pfad,
    /// den ein Cloud-Tablet über `TabletMsg::ScoreUpdate` nimmt.
    ScoreUpdate {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        /// Match, zu dem der Stand gehört — der Relay verwirft den Spiegel,
        /// wenn das Feld inzwischen ein anderes Match trägt (Stale-Schutz,
        /// gleiche Regel wie beim Tablet-Weg / HM-03).
        #[serde(rename = "matchId", default)]
        match_id: i64,
        /// Vollständige Satzliste (abgeschlossene Sätze + laufender Satz).
        #[serde(default)]
        sets: Vec<SetAb>,
        /// Roher Tablet-Spielzustand (Aufschlag, Pause) als JSON-String —
        /// opak wie beim Tablet-`state_sync`; `None` = kein Tablet-Zustand
        /// bekannt (der bisherige Relay-Stand bleibt dann stehen).
        #[serde(skip_serializing_if = "Option::is_none", default)]
        state: Option<String>,
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
    /// Aufgerufene (in Vorbereitung gerufene) Spiele der fernen Hallen –
    /// Grundlage der Slave-Spielübersicht und des Zweit-/Drittaufrufs am
    /// Slave-PC (Cluster C Stufe 2). Periodisch vom Host gepusht (selten
    /// veränderlich); ersetzt jeweils die komplette Liste im Relay.
    Prepared {
        #[serde(default)]
        prepared: Vec<PreparedMatch>,
    },
    /// Antwort auf eine zuvor weitergeleitete Ergebnis-Übermittlung.
    ResultAck {
        #[serde(rename = "reqId")]
        req_id: u64,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
        /// Ist die Ablehnung dauerhaft? Muss über den Cloud-Weg mitreisen —
        /// sonst wüsste ein Tablet am Relay nie, dass Wiederholen zwecklos
        /// ist, und der Fix wirkte ausgerechnet im Cloud-Betrieb nicht.
        /// Siehe [`ResultResponse::permanent`]. `#[serde(default)]` hält
        /// ältere Hosts lesbar (deren Absagen gelten als wiederholbar); der
        /// Normalfall `false` bleibt von der Leitung.
        #[serde(default, skip_serializing_if = "is_false")]
        permanent: bool,
    },
    /// Vollständige Feld-Liste des Turniers – Grundlage des Feldwechsels im
    /// PIN-Menü des Tablets im Cloud-Modus. Periodisch vom Host gepusht.
    Courts {
        #[serde(default)]
        courts: Vec<CourtBrief>,
        /// Azure-TTS-Konfiguration des Masters für die Vererbung an
        /// Cloud-Ansage-Slaves (ADR 0003). `None` = Azure am Master aus →
        /// ein zuvor geerbter Wert wird beim Slave verworfen. Optionales
        /// Feld statt neuem Frame-Typ, damit alte Relays den Frame weiter
        /// parsen (Serde ignoriert unbekannte Felder).
        #[serde(rename = "azureTts", skip_serializing_if = "Option::is_none", default)]
        azure_tts: Option<AzureTtsShare>,
        /// A2 / ADR 0017 (Reconnect-Wahrheit): der Legacy-rev-Schalter des
        /// Hosts. Der Relay kennt die App-Config nicht direkt; der Host reicht
        /// sie über diesen periodischen Frame durch, damit der
        /// Laufzeit-Rollback (Legacy = altes rev-Verhalten) AUCH im Cloud-Modus
        /// greift: der Relay setzt dann `ownership_active = !reconnect_legacy_rev`
        /// im `StateRestore`. `#[serde(rename)]` für camelCase auf der Wire,
        /// `#[serde(default)]` = ältere Hosts → `false` = Ownership aktiv
        /// (sicherer Default).
        #[serde(rename = "reconnectLegacyRev", default)]
        reconnect_legacy_rev: bool,
    },
    /// Die aktuell zugelassenen Turnierleitungs-Geräte. Der Host stellt sie
    /// aus, der Relay spiegelt sie nur (ADR 0012) — die `install_id`
    /// verlässt den Master nicht. Die Liste **ersetzt** die bisherige: ein
    /// entferntes Gerät ist damit sofort ausgesperrt, und das ist der
    /// gesamte Widerrufsmechanismus.
    ///
    /// Bewusst **ohne Gerätenamen** (Datensparsamkeit): Der Relay bekommt die
    /// zufällige Kennung, nicht das Etikett „Tablet Meeting Point". Die
    /// Kennung braucht er, um sie mit dem Kommando zurückzuschicken — sonst
    /// stünde im Protokoll des Turnier-PCs nicht, welches Gerät gehandelt
    /// hat, oder er müsste sie aus dem Zugang ableiten, und der hat in
    /// Protokollen nichts verloren.
    ///
    /// **Ohne `#[serde(default)]`:** Weil die Liste die bisherige ersetzt,
    /// hieße ein fehlendes Feld „alle Geräte aussperren". Ein verstümmeltes
    /// Frame darf nicht mitten im Turnier die gesamte Turnierleitung
    /// abmelden — es wird verworfen, der bisherige Stand bleibt stehen. Die
    /// **leere** Liste ist dagegen zulässig und heißt ausdrücklich „kein
    /// Gerät mehr zugelassen".
    TlAuth { devices: Vec<TlAuthDevice> },
    /// Der Anzeige-Zustand für die Turnierleitungs-Oberfläche, **opak**: Der
    /// Relay legt ihn nur ab und liefert ihn unverändert aus, wie schon beim
    /// Court-Zustand. So bleibt die Turnierlogik vollständig im Host (R5).
    /// `rev` steigt nur bei echter Änderung — daran erkennt ein abrufendes
    /// Gerät, ob sich etwas getan hat.
    TlState {
        #[serde(default)]
        rev: u64,
        #[serde(default)]
        json: String,
    },
    /// Antwort auf ein zuvor weitergeleitetes TL-Kommando.
    TlAck {
        #[serde(rename = "reqId")]
        req_id: u64,
        response: TlResponse,
    },
    /// Antwort auf einen [`RelayFrame::TimelineRequest`]: der Verlauf als
    /// **opaker** JSON-String (Muster `TlState`) — der Relay liefert ihn
    /// unverändert aus, die Form bestimmt allein der Host. `found: false`
    /// = zu diesem Match liegt kein Verlauf vor (Papier-Ergebnis).
    TimelineData {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(default)]
        found: bool,
        #[serde(default)]
        json: String,
    },
    /// Antwort auf einen [`RelayFrame::OfficialDetailRequest`]: Sperrlisten,
    /// Stammverein und Einsatz-Liste eines Schiedsrichters als **opaker**
    /// JSON-String (Muster [`HostFrame::TimelineData`]).
    ///
    /// Kein `found`-Flag: Ein unbekannter Schiedsrichter liefert leere
    /// Listen, damit die Pflege-Ansicht sich trotzdem öffnen lässt.
    OfficialDetail {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(default)]
        json: String,
    },
    /// Antwort auf einen [`RelayFrame::ScoresheetRequest`]: der fertige
    /// Schiedsrichterzettel als **opakes HTML-Dokument** (Muster
    /// [`HostFrame::TimelineData`]) — der Relay liefert es unverändert
    /// aus, die Form bestimmt allein der Host.
    ///
    /// `found: false` = zu keinem der angefragten Spiele liegt eine
    /// Aufzeichnung vor (Papier-Ergebnis).
    ScoresheetData {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(default)]
        found: bool,
        #[serde(default)]
        html: String,
    },
}

/// Höchstzahl der Turnierleitungs-Geräte, die der Relay spiegelt.
///
/// **Geteilt, weil beide Seiten dieselbe Zahl meinen müssen:** Der Relay
/// verwirft eine längere Liste vollständig (ein halbierter Widerruf wäre
/// schlimmer als keiner), und der Host muss das vorher wissen — sonst hielte
/// er ein nie angekommenes Frame für zugestellt und die Cloud-Oberfläche
/// bliebe stumm gesperrt.
pub const MAX_TL_DEVICES_MIRRORED: usize = 64;

/// Höchstzahl **gleichzeitig bedienter** Turnierleitungs-Geräte je Turnier.
///
/// Die spürbare Grenze: Das neunte Gerät, das die Seite offen hat, wird
/// abgewiesen (ein Platz wird nach 60 s Stille wieder frei). Sie ist viel
/// kleiner als [`MAX_TL_DEVICES_MIRRORED`], das nur die Länge der
/// gespiegelten Liste begrenzt — alte Kopplungen zählen dort mit, blockieren
/// aber keinen Platz.
///
/// Geteilt, damit die Geräteverwaltung im Desktop dieselbe Zahl nennt, die
/// der Relay durchsetzt.
pub const MAX_TL_DEVICES_ONLINE: usize = 8;

/// Höchstgröße des Anzeige-Zustands in Bytes.
///
/// Ebenfalls geteilt: Der Relay legt größere Stände nicht ab, und der Host
/// muss seinen Zustand vorher kürzen. Ohne dieses gemeinsame Maß liefe ein
/// großes Turnier in eine dauerhaft tote Cloud-Oberfläche — je größer das
/// Turnier, desto sicherer.
pub const MAX_TL_STATE_LEN: usize = 64 * 1024;

/// Ein zugelassenes Turnierleitungs-Gerät, wie der Host es dem Relay
/// spiegelt: zufällige Kennung + Zugang, **kein** Name.
///
/// Die Kennung reist mit jedem Kommando zurück zum Host, damit sein
/// Protokoll benennen kann, wer gehandelt hat. Der Zugang bleibt beim Relay
/// und taucht nirgends sonst auf.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlAuthDevice {
    pub id: String,
    pub token: String,
    /// Gewähltes Panel-Profil dieses Geräts (Spec tl-web-panelsystem, ADR
    /// 0025); leer = Standardprofil. Grundlage des
    /// `X-Tl-Active-Profile`-Antwort-Headers (`relay::tl_state_route` /
    /// `tablet::server::tl_state`).
    ///
    /// **Bewusste Ausnahme** von der oben dokumentierten Regel „kein
    /// `#[serde(default)]` auf TL-Feldern": Diese Regel schützt davor, dass
    /// ein still ergänzter Wert die weitreichendere/ungeprüfte Variante
    /// auslöst (siehe [`TlAction`]-Dokkommentar). Hier ist „leer" aber die
    /// NEUTRALSTE Lesart — sie bedeutet „Standardprofil", keine erweiterten
    /// Rechte oder größere Sichtbarkeit. Ein alter Host, der dieses Feld
    /// noch nicht kennt, sendet es schlicht nicht mit; `#[serde(default)]`
    /// hält den Relay davon unabhängig lauffähig. Begründung: ADR 0025.
    #[serde(default)]
    pub profile_id: String,
}

/// Vom Master an Cloud-Slaves vererbte Azure-TTS-Konfiguration (ADR 0003).
/// Enthält bewusst kein `enabled`: gesendet wird sie nur, wenn Azure am
/// Master aktiv und vollständig konfiguriert ist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AzureTtsShare {
    /// Azure-Region der Speech-Ressource, z. B. „westeurope".
    #[serde(default)]
    pub region: String,
    /// Subscription-Key der Speech-Ressource. Achtung: Secret — nie loggen.
    #[serde(default)]
    pub key: String,
    /// Stimme, z. B. „de-DE-SeraphinaMultilingualNeural" (Standard-/Hauptstimme).
    #[serde(default)]
    pub voice: String,
    /// Optionale Stimme je Disziplin (Disziplin-Kürzel → Azure-Stimme). Wird
    /// vom Master mitvererbt, damit die ferne Halle dieselbe Zuordnung nutzt.
    /// Leer/fehlend → Standard-Stimme (abwärtskompatibel per Default).
    #[serde(default)]
    pub discipline_voices: std::collections::HashMap<String, String>,
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

/// Ein in Vorbereitung gerufenes Spiel einer Halle – für die Slave-
/// Spielübersicht (Plan 7) und den gezielten Zweit-/Drittaufruf einer
/// fehlenden Partei am Slave-PC (Cluster C Stufe 2). Der Slave sagt den
/// Nachruf lokal in seiner Halle an; er braucht dafür nur die Paarung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedMatch {
    #[serde(rename = "matchId")]
    pub match_id: i64,
    /// Hallenname (BTP-Location), für den gerufen wurde – Grundlage der
    /// Hallenfilterung am Relay (Slave sieht nur seine Halle).
    #[serde(default)]
    pub hall: String,
    /// Effektive Hallen-Farbe des Aufrufs (Hex, Spec hallen-farben) —
    /// `None` ohne Halle, bei Ein-Hallen-Turnieren und von alten Hosts.
    #[serde(rename = "hallColor", default, skip_serializing_if = "Option::is_none")]
    pub hall_color: Option<String>,
    /// Disziplin als snake_case-Schlüssel (`mens_singles`, …; leer = unbekannt)
    /// – der Slave lokalisiert die Ansage selbst.
    #[serde(default)]
    pub discipline: String,
    /// Klassen-Kürzel („A", „B", …) für „Herreneinzel A" (leer = keins).
    #[serde(
        rename = "classLabel",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub class_label: String,
    /// Runden-/Spielbezeichnung (z. B. „G1", „Finale") für die Übersicht.
    #[serde(
        rename = "roundName",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub round_name: String,
    #[serde(rename = "teamA", default)]
    pub team_a: Vec<PlayerBrief>,
    #[serde(rename = "teamB", default)]
    pub team_b: Vec<PlayerBrief>,
    /// Spielnummer (BTP `MatchNr`), falls vergeben – für die Übersicht.
    #[serde(
        rename = "matchNumber",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub match_number: Option<i64>,
    /// Zeitpunkt des Aufrufs (Unix-ms) – für „vor X Min." in der Übersicht.
    #[serde(rename = "calledAtMs", default)]
    pub called_at_ms: u64,
}

/// Antwort von `GET /{ns}/info/announce/state?hall=&since=` — hallengefilterte
/// Court-Matches (Auto-Ansage) + neue Freitext-Ansagen für den Cloud-Slave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AnnounceState {
    #[serde(default)]
    pub courts: Vec<AnnounceCourt>,
    #[serde(default)]
    pub freetext: Vec<FreetextItem>,
    /// Aufgerufene Spiele der Halle (Slave-Spielübersicht + Nachruf am Slave,
    /// Cluster C Stufe 2). Leer bei altem Relay/Master oder ohne Aufrufe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prepared: Vec<PreparedMatch>,
    /// Vom Master geerbte Azure-TTS-Konfiguration (ADR 0003). `None` bei
    /// altem Relay oder ausgeschaltetem Azure am Master — der Slave nutzt
    /// dann seine lokale Config bzw. die Web-Speech-Standardstimme.
    #[serde(rename = "azureTts", skip_serializing_if = "Option::is_none", default)]
    pub azure_tts: Option<AzureTtsShare>,
}

/// Antwort von `POST /{ns}/pairing-code` — kurzlebiger 8-stelliger
/// Telefon-Kopplungscode (ADR 0004). Der Relay hält ihn nur im RAM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode {
    /// Genau 8 Ziffern (führende Nullen möglich) — telefonisch diktierbar.
    pub code: String,
    /// Restgültigkeit in Sekunden ab Ausstellung.
    #[serde(rename = "expiresInS", default)]
    pub expires_in_s: u64,
}

/// Antwort von `GET /pair/{code}` — der aufgelöste Master-Namespace
/// (= `install_id`), den der Slave als `master_namespace` speichert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingResolved {
    pub namespace: String,
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
///
/// **Kein `Eq`** (nur `PartialEq`): `TlCommand` trägt ein [`TlAction`], und
/// das ist seit den Panel-Profilen (`f64`-Feld `height_fr`) selbst nicht
/// mehr `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        /// Match, das das Tablet gerade zählt (durchgereicht aus
        /// [`TabletMsg::ScoreUpdate`]) — der Host filtert damit auch bei
        /// einem ALTEN Relay ohne eigenen Stale-Filter. 0 = unbekannt
        /// (alte Tablet-Seite/alter Relay) → Verhalten wie bisher.
        #[serde(rename = "matchId", default)]
        match_id: i64,
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
    /// Ein Kommando eines Turnierleitungs-Geräts. `req_id` korreliert die
    /// [`HostFrame::TlAck`] — der Absender bekommt eine echte Quittung nach
    /// dem BTP-Schreiben, kein Fire-and-forget.
    ///
    /// `op_id` ist der **Idempotenzschlüssel**: Dieselbe Aktion nach einem
    /// Netzwackler erneut geschickt darf nur einmal nach BTP schreiben; der
    /// Host antwortet auf eine Wiederholung mit dem gespeicherten Ergebnis.
    ///
    /// `view_rev` ist die Revision des Zustands, auf dem die Aktion beruhte.
    /// Sie ist die Grundlage der Altersprüfung: Ohne sie wäre
    /// [`TlErrorCode::StaleView`] für alles außer Feldaktionen unerreichbar —
    /// ein Gerät, das lange im Ruhezustand lag, könnte beim Aufwachen einen
    /// längst überholten Vorschlag bestätigen.
    ///
    /// Keines dieser Felder hat einen Default: Ohne Geräte- und
    /// Vorgangskennung sind weder Zuordnung noch Doppelschutz möglich, und
    /// ein stillschweigend leerer Schlüssel würde alle Vorgänge eines Geräts
    /// auf denselben Wert werfen.
    TlCommand {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(rename = "deviceId")]
        device_id: String,
        #[serde(rename = "opId")]
        op_id: String,
        #[serde(rename = "viewRev")]
        view_rev: u64,
        action: TlAction,
    },
    /// Ein Ballwechsel von einem Tablet (Punktverlauf, ADR 0014) — vom
    /// Relay 1:1 durchgereicht, Interpretation allein beim Host.
    Rally {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "matchId", default)]
        match_id: i64,
        #[serde(default)]
        set: i64,
        #[serde(default)]
        n: i64,
        #[serde(default)]
        winner: String,
        #[serde(rename = "scoreA", default)]
        score_a: i64,
        #[serde(rename = "scoreB", default)]
        score_b: i64,
    },
    /// Kompletter Verlaufs-Resync eines Tablets (siehe
    /// [`TabletMsg::RallySync`]) — der Relay prüft nur die Größe.
    RallySync {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "matchId", default)]
        match_id: i64,
        #[serde(default)]
        timeline: MatchTimeline,
    },
    /// Ein TL-Gerät möchte den Punktverlauf eines Matches sehen —
    /// Request/Response wie beim TL-Kommando (`req_id` korreliert die
    /// [`HostFrame::TimelineData`]). Der Relay bleibt Briefträger: er
    /// hält keine Verläufe vor (Mobilfunk-Budget, Spec AK-5).
    TimelineRequest {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(rename = "matchId")]
        match_id: i64,
    },
    /// Ein TL-Gerät möchte Sperrlisten und Einsätze eines Schiedsrichters
    /// sehen (Spec schiedsrichter-management). Wie beim Punktverlauf
    /// Request/Response über `req_id`; der Relay bleibt Briefträger und
    /// hält diese Personendaten **nie** vor.
    OfficialDetailRequest {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(rename = "officialId")]
        official_id: i64,
    },
    /// Ein Zettel-Ereignis von einem Tablet (ADR 0037) — vom Relay 1:1
    /// durchgereicht, Interpretation allein beim Host.
    MatchEvent {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "matchId", default)]
        match_id: i64,
        event: MatchEvent,
    },
    /// Kompletter Ereignis-Abgleich eines Tablets (siehe
    /// [`TabletMsg::MatchEventSync`]) — der Relay prüft nur Gültigkeit
    /// und Größe.
    MatchEventSync {
        #[serde(rename = "courtId", default)]
        court_id: i64,
        #[serde(rename = "matchId", default)]
        match_id: i64,
        #[serde(default)]
        events: Vec<MatchEvent>,
    },
    /// Ein TL-Gerät möchte Schiedsrichterzettel drucken — Request/Response
    /// über `req_id` wie beim Punktverlauf (die Antwort ist
    /// [`HostFrame::ScoresheetData`]).
    ///
    /// Mehrere Kennungen ergeben einen Stapeldruck (eine ganze Runde in
    /// einem Auftrag), gedeckelt auf [`MAX_SHEETS_PER_DOC`]. Der Relay
    /// bleibt Briefträger: Er hält **keine** Zettel vor — sie tragen
    /// Sanktionsdaten.
    ScoresheetRequest {
        #[serde(rename = "reqId")]
        req_id: u64,
        #[serde(rename = "matchIds")]
        match_ids: Vec<i64>,
        /// **Vorabzettel**: ein Blatt auch für Spiele, zu denen es noch
        /// keine Aufzeichnung gibt — Kopf gefüllt, Raster leer, von Hand
        /// zu führen (Spec `schiedsrichterzettel-autodruck`).
        ///
        /// `#[serde(default)]` hält die Erweiterung additiv: Ein Host
        /// älterer Version liest den Frame weiter und antwortet mit dem
        /// gewohnten Verhalten — ohne Aufzeichnung also mit `found:
        /// false`.
        #[serde(default)]
        vorab: bool,
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
                court_label: "Feld 1".to_string(),
                device_id: String::new()
            }
        );
    }

    #[test]
    fn tablet_msg_identify_with_device_id() {
        // Neue Tablet-Seiten schicken ihre persistente Geräte-Kennung mit —
        // Grundlage der Reconnect-Erkennung (Reconnect ≠ Übernahme).
        let json = r#"{"type":"identify","role":"tablet","courtId":7,"courtLabel":"Feld 1","deviceId":"dev-abc"}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            TabletMsg::Identify {
                court_id: 7,
                court_label: "Feld 1".to_string(),
                device_id: "dev-abc".to_string()
            }
        );
    }

    #[test]
    fn tablet_msg_take_over_with_and_without_device_id() {
        // Alte Tablet-Seiten schicken take_over ohne deviceId — muss
        // weiterhin parsen (leere Kennung).
        let old: TabletMsg = serde_json::from_str(r#"{"type":"take_over"}"#).unwrap();
        assert_eq!(
            old,
            TabletMsg::TakeOver {
                device_id: String::new()
            }
        );
        let new: TabletMsg =
            serde_json::from_str(r#"{"type":"take_over","deviceId":"dev-abc"}"#).unwrap();
        assert_eq!(
            new,
            TabletMsg::TakeOver {
                device_id: "dev-abc".to_string()
            }
        );
    }

    /// A2 / ADR 0017: `StateRestore` mit den neuen Autoritäts-Feldern
    /// (inkl. `ownership_active`) serialisiert und deserialisiert verlustfrei.
    #[test]
    fn state_restore_roundtrip_with_authority_fields() {
        roundtrip(&ServerMsg::StateRestore {
            state: "{\"score\":\"3:5\"}".to_string(),
            ownership_active: true,
            authoritative: true,
            owner_epoch: 42,
            owner_device: "dev-abc".to_string(),
        });
        // Ownership-Modus mit „adoptieren" (authoritative=false).
        roundtrip(&ServerMsg::StateRestore {
            state: "{}".to_string(),
            ownership_active: true,
            authoritative: false,
            owner_epoch: 7,
            owner_device: "dev-x".to_string(),
        });
    }

    /// Abwärtskompatibilität: eine ältere `state_restore`-Nachricht OHNE die
    /// neuen Felder bleibt lesbar — `ownership_active`/`authoritative`/`owner_*`
    /// fallen per `#[serde(default)]` auf ihre Defaults (false/false/0/leer).
    /// `ownership_active=false` ist genau der Auto-Update-sichere rev-Fallback:
    /// das Tablet ignoriert `authoritative` und nutzt seine rev-Logik.
    #[test]
    fn state_restore_backward_compatible_without_authority_fields() {
        let msg: ServerMsg =
            serde_json::from_str(r#"{"type":"state_restore","state":"{}"}"#).unwrap();
        assert_eq!(
            msg,
            ServerMsg::StateRestore {
                state: "{}".to_string(),
                ownership_active: false,
                authoritative: false,
                owner_epoch: 0,
                owner_device: String::new(),
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
                court_label: "Feld 1".to_string(),
                device_id: String::new()
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
                match_id: 0,
            }
        );
    }

    #[test]
    fn score_update_carries_match_id_and_roundtrips() {
        // Stale-Filter (A4): neue Tablet-Seiten senden die matchId mit;
        // alte Seiten ohne das Feld parsen weiter (Default 0 = kein Filter).
        let msg = TabletMsg::ScoreUpdate {
            score_a: 5,
            score_b: 3,
            sets_history: vec![],
            match_id: 4711,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"matchId\":4711"));
        assert_eq!(serde_json::from_str::<TabletMsg>(&json).unwrap(), msg);
    }

    #[test]
    fn state_sync_match_id_extracts_or_declines() {
        // tablet.html persistiert { match: { matchId: … }, … }.
        assert_eq!(
            state_sync_match_id(r#"{"match":{"matchId":42,"teamA":[]},"finished":false}"#),
            Some(42)
        );
        // Kein Match (Leerlauf-State) / kaputtes JSON → kein Filter.
        assert_eq!(state_sync_match_id(r#"{"match":null}"#), None);
        assert_eq!(state_sync_match_id("kein json"), None);
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
                    club: Some("SC Musterstadt".into()),
                }],
                team_b: vec![PlayerBrief {
                    id: 11,
                    name: "Ben".into(),
                    nationality: None,
                    club: None,
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
                scorekeeper_assigned: false,
                show_club_names: true,
                show_club_logos: false,
                finalized: false,
                sr_names: vec!["Sabine Schiedsmann".into()],
                ar_names: Vec::new(),
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"match_assigned""#));
        assert!(json.contains(r#""match":{"#));
        roundtrip(&msg);
    }

    /// Serde-Roundtrip des A2-Finalisiert-Flags (ADR 0017): ein `MatchBrief`
    /// MIT `finalized:true` übersteht Hin-/Rückwandlung, und ein ALTES Frame
    /// OHNE das Feld liest sich per `#[serde(default)]` als `finalized:false`
    /// (Auto-Update-sicher — ältere Relays/Clients bleiben kompatibel).
    #[test]
    fn match_brief_finalized_roundtrip_and_default() {
        let brief = MatchBrief {
            match_id: 7,
            team_a: vec![PlayerBrief {
                id: 1,
                name: "Anna".into(),
                nationality: None,
                club: None,
            }],
            team_b: vec![PlayerBrief {
                id: 11,
                name: "Ben".into(),
                nationality: None,
                club: None,
            }],
            event_label: "HE G1".into(),
            best_of_sets: 3,
            target_score: 21,
            cap_score: 30,
            interval_at: None,
            discipline: "mens_singles".into(),
            class_label: String::new(),
            match_number: None,
            scorekeeper: Vec::new(),
            scorekeeper_assigned: false,
            show_club_names: false,
            show_club_logos: false,
            finalized: true,
            sr_names: vec!["Sabine Schiedsmann".into()],
            ar_names: vec!["Alex Aufschlag".into()],
        };
        let json = serde_json::to_string(&brief).unwrap();
        assert!(json.contains(r#""finalized":true"#));
        // Die Namen reisen in camelCase mit — das ist der Vertrag mit
        // `tablet.html` (Spec schiedsrichter-management Nr. 7).
        assert!(json.contains(r#""srNames":["Sabine Schiedsmann"]"#));
        assert!(json.contains(r#""arNames":["Alex Aufschlag"]"#));
        roundtrip(&brief);

        // Altes Frame ohne das Feld → Default false bzw. leere Listen: Ein
        // älterer Relay/Client bleibt lesbar (Auto-Update-Sicherheit).
        let legacy = r#"{"matchId":7,"teamA":[],"teamB":[],"eventLabel":"HE G1","bestOfSets":3,"targetScore":21}"#;
        let parsed: MatchBrief = serde_json::from_str(legacy).unwrap();
        assert!(!parsed.finalized, "fehlendes Feld → finalized=false");
        assert!(parsed.sr_names.is_empty());
        assert!(parsed.ar_names.is_empty());
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
            permanent: true,
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

    /// Host→Relay-Score-Spiegel: Im LAN(+Cloud)-Betrieb zählen die Tablets am
    /// Host vorbei am Relay — dieses Frame trägt den Satzstand (und optional
    /// den opaken `court_state`) zum Relay, damit Cloud-Monitor und
    /// Cloud-Übersicht nicht auf 0:0 stehen bleiben.
    #[test]
    fn host_score_update_roundtrips_with_camel_case_wire() {
        let frame = HostFrame::ScoreUpdate {
            court_id: 5,
            match_id: 77,
            sets: vec![SetAb { a: 21, b: 19 }, SetAb { a: 3, b: 1 }],
            state: Some(r#"{"score":"3:1"}"#.into()),
        };
        roundtrip(&frame);
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains(r#""type":"score_update""#));
        assert!(json.contains(r#""courtId":5"#));
        assert!(json.contains(r#""matchId":77"#));

        // Ohne `state` (kein Tablet-Zustand vorhanden) → Feld fehlt auf der
        // Wire, parst als None; `sets` leer ist zulässig (Match ohne Punkte).
        let json = serde_json::to_string(&HostFrame::ScoreUpdate {
            court_id: 5,
            match_id: 77,
            sets: vec![],
            state: None,
        })
        .unwrap();
        assert!(!json.contains("state"), "None-state bleibt weg: {json}");
        let parsed: HostFrame =
            serde_json::from_str(r#"{"type":"score_update","courtId":5,"matchId":77,"sets":[]}"#)
                .unwrap();
        assert_eq!(
            parsed,
            HostFrame::ScoreUpdate {
                court_id: 5,
                match_id: 77,
                sets: vec![],
                state: None,
            }
        );
    }

    #[test]
    fn prepared_frame_and_state_roundtrip() {
        // Prepared-Frame (Cluster C Stufe 2) hält den Roundtrip …
        roundtrip(&HostFrame::Prepared {
            prepared: vec![PreparedMatch {
                match_id: 42,
                hall: "Halle 2".into(),
                hall_color: Some("#0ea5e9".into()),
                discipline: "mens_singles".into(),
                class_label: "A".into(),
                round_name: "G1".into(),
                team_a: vec![PlayerBrief {
                    id: 1,
                    name: "Anna Weber".into(),
                    nationality: Some("GER".into()),
                    club: Some("TV Beispiel".into()),
                }],
                team_b: vec![PlayerBrief {
                    id: 2,
                    name: "Bea Schulz".into(),
                    nationality: None,
                    club: None,
                }],
                match_number: Some(101),
                called_at_ms: 1_700_000_000_000,
            }],
        });

        // … und ein alter Relay ohne prepared-Feld bleibt lesbar (Default leer).
        let st: AnnounceState = serde_json::from_str(r#"{"courts":[],"freetext":[]}"#).unwrap();
        assert!(st.prepared.is_empty());
        // Leere prepared-Liste wird gar nicht erst serialisiert (altes Format).
        let json = serde_json::to_string(&AnnounceState::default()).unwrap();
        assert!(!json.contains("prepared"));
    }

    #[test]
    fn azure_tts_share_roundtrips_and_defaults() {
        // Courts-Frame mit Azure-Vererbung (ADR 0003) hält den Roundtrip …
        roundtrip(&HostFrame::Courts {
            courts: vec![],
            azure_tts: Some(AzureTtsShare {
                region: "westeurope".into(),
                key: "geheim".into(),
                voice: "de-DE-SeraphinaMultilingualNeural".into(),
                discipline_voices: std::collections::HashMap::from([(
                    "mens_singles".to_string(),
                    "de-DE-FlorianMultilingualNeural".to_string(),
                )]),
            }),
            reconnect_legacy_rev: false,
        });
        // … und ohne Azure wird das Feld gar nicht erst serialisiert
        // (alte Relays sehen exakt das bisherige Frame-Format).
        let json = serde_json::to_string(&HostFrame::Courts {
            courts: vec![],
            azure_tts: None,
            reconnect_legacy_rev: false,
        })
        .unwrap();
        assert!(!json.contains("azureTts"));

        // Älterer Host ohne azureTts-Feld bleibt für den neuen Relay lesbar.
        let frame: HostFrame = serde_json::from_str(r#"{"type":"courts","courts":[]}"#).unwrap();
        assert_eq!(
            frame,
            HostFrame::Courts {
                courts: vec![],
                azure_tts: None,
                reconnect_legacy_rev: false,
            }
        );

        // AnnounceState: alter Relay ohne azureTts → None; mit → Wert kommt an.
        let st: AnnounceState = serde_json::from_str(r#"{"courts":[],"freetext":[]}"#).unwrap();
        assert_eq!(st.azure_tts, None);
        let st: AnnounceState = serde_json::from_str(
            r#"{"courts":[],"freetext":[],"azureTts":{"region":"westeurope","key":"k","voice":"v"}}"#,
        )
        .unwrap();
        // Alter Master ohne discipline_voices → Default leer, bleibt lesbar.
        assert_eq!(
            st.azure_tts,
            Some(AzureTtsShare {
                region: "westeurope".into(),
                key: "k".into(),
                voice: "v".into(),
                discipline_voices: std::collections::HashMap::new(),
            })
        );
        // Neuer Master MIT discipline_voices → kommt beim Slave an.
        let st: AnnounceState = serde_json::from_str(
            r#"{"courts":[],"freetext":[],"azureTts":{"region":"we","key":"k","voice":"v","discipline_voices":{"mens_singles":"de-DE-FlorianMultilingualNeural"}}}"#,
        )
        .unwrap();
        assert_eq!(
            st.azure_tts
                .unwrap()
                .discipline_voices
                .get("mens_singles")
                .map(String::as_str),
            Some("de-DE-FlorianMultilingualNeural")
        );
    }

    /// A2 / ADR 0017: Der `reconnect_legacy_rev`-Schalter reist im
    /// `Courts`-Frame vom Host zum Relay (Cloud-Rollback). Roundtrip mit
    /// `true`; ein älterer Host ohne das Feld liest sich als `false`
    /// (= Ownership aktiv, sicherer Default).
    #[test]
    fn courts_frame_carries_reconnect_legacy_rev() {
        roundtrip(&HostFrame::Courts {
            courts: vec![],
            azure_tts: None,
            reconnect_legacy_rev: true,
        });
        let json = serde_json::to_string(&HostFrame::Courts {
            courts: vec![],
            azure_tts: None,
            reconnect_legacy_rev: true,
        })
        .unwrap();
        assert!(json.contains(r#""reconnectLegacyRev":true"#));
        // Älterer Host ohne das Feld → Default false.
        let frame: HostFrame = serde_json::from_str(r#"{"type":"courts","courts":[]}"#).unwrap();
        assert_eq!(
            frame,
            HostFrame::Courts {
                courts: vec![],
                azure_tts: None,
                reconnect_legacy_rev: false,
            }
        );
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
    fn player_brief_club_roundtrips_and_defaults() {
        // Neues Feld hält den Roundtrip (mit und ohne Verein).
        roundtrip(&PlayerBrief {
            id: 3,
            name: "Cara Lang".into(),
            nationality: Some("AUT".into()),
            club: Some("BC Beispiel".into()),
        });
        // Älterer Host/Relay ohne `club` bleibt lesbar (Default = None).
        let old = r#"{"id":9,"name":"Dora Kurz","nationality":"GER"}"#;
        let brief: PlayerBrief = serde_json::from_str(old).unwrap();
        assert_eq!(
            brief,
            PlayerBrief {
                id: 9,
                name: "Dora Kurz".into(),
                nationality: Some("GER".into()),
                club: None,
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
            hall_color: None,
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
                hall_color: None,
            }
        );
    }

    #[test]
    fn court_brief_hall_color_roundtrips_and_defaults_none() {
        // Spec hallen-farben: neues optionales Feld hält den Roundtrip …
        roundtrip(&CourtBrief {
            id: 401,
            label: "Halle 2 · 1".into(),
            hall: "Halle 2".into(),
            hall_color: Some("#f59e0b".into()),
        });
        // … und ein alter Host ohne das Feld bleibt lesbar (farblos), ohne
        // dass `None` überhaupt auf den Draht geht (skip_serializing_if).
        let old = r#"{"id":7,"label":"Feld 3","hall":"Halle 1"}"#;
        let brief: CourtBrief = serde_json::from_str(old).unwrap();
        assert_eq!(brief.hall_color, None);
        let json = serde_json::to_string(&brief).unwrap();
        assert!(!json.contains("hall_color"), "None reist nicht mit: {json}");
    }

    #[test]
    fn prepared_match_hall_color_defaults_none() {
        let old = r#"{"matchId":42,"hall":"Halle 2","teamA":[],"teamB":[],"calledAtMs":0}"#;
        let p: PreparedMatch = serde_json::from_str(old).unwrap();
        assert_eq!(p.hall_color, None, "alter Host → farblos");
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("hallColor"), "None reist nicht mit: {json}");
    }

    #[test]
    fn der_herzschlag_traegt_kein_court_feld() {
        // Spec monitor-livestand-push, S6. Daran hängt die ganze
        // Verträglichkeit: Eine Anzeige aus einem älteren Stand prüft auf ein
        // `court`-Feld und verwirft alles andere folgenlos. Käme hier eines
        // vor, hielte sie den Herzschlag für einen Anstoß und holte bei jedem
        // den vollen Stand — auf allen Anzeigen, alle zehn Sekunden.
        let frame = monitor_heartbeat_frame(1_787_000_000_042);
        let v: serde_json::Value = serde_json::from_str(&frame).expect("gültiges JSON");
        let obj = v.as_object().expect("Objekt");
        assert!(!obj.contains_key("court"), "kein court-Feld: {frame}");
        assert!(!obj.contains_key("seq"), "auch keine Sequenz: {frame}");
        assert_eq!(
            obj.get("hb").and_then(|h| h.as_u64()),
            Some(1_787_000_000_042)
        );
        assert_eq!(obj.len(), 1, "nur der Zeitstempel: {frame}");
    }

    #[test]
    fn der_herzschlag_takt_haelt_die_stale_grenze() {
        // Zwei Herzschläge müssen in die Grenze passen — ein einzelner
        // verlorener darf noch keinen Reconnect auslösen. Über Variablen,
        // damit clippy den Vergleich nicht wegoptimiert (`assertions_on_constants`).
        let takt = MONITOR_HEARTBEAT_MS;
        let grenze = MONITOR_HEARTBEAT_STALE_MS;
        assert!(takt * 2 < grenze, "{takt} · 2 muss unter {grenze} liegen");
    }

    #[test]
    fn ein_monitor_state_ohne_seq_bleibt_lesbar() {
        // Spec monitor-livestand-push, S4: `seq` ordnet Push und Voll-Abruf.
        // Ein **alter** Relay (oder Host) schickt das Feld nicht — die Seite
        // muss den Stand trotzdem verarbeiten und `seq = 0` sehen, damit sie
        // sich wie vor der Etappe verhält.
        // Frame eines alten Absenders simulieren: aktueller Zustand, aber
        // ohne das neue Feld (Muster `monitor_state_hall_color_defaults_none`).
        let mut json = serde_json::to_value(MonitorState {
            court_id: 3,
            court_label: "Feld 3".into(),
            hall_color: None,
            ad_styles: Vec::new(),
            tournament_name: "T".into(),
            match_info: None,
            court_state: None,
            config: MonitorConfig::default(),
            ads: vec![],
            command: None,
            device_code: String::new(),
            unassigned: false,
            redirect_to: None,
            server_now_ms: 0,
            on_court_since_ms: None,
            call_timer: CallTimerView::default(),
            seq: 4711,
        })
        .expect("serialisierbar");
        json.as_object_mut().expect("Objekt").remove("seq");
        let state: MonitorState = serde_json::from_value(json).expect("altes Frame bleibt lesbar");
        assert_eq!(
            state.seq, 0,
            "fehlendes Feld = 0 = „keine Ordnung bekannt\""
        );

        // Und mit Feld kommt der Wert an — hin und zurück.
        let mut mit = state.clone();
        mit.seq = 1_787_000_000_042;
        let json = serde_json::to_string(&mit).expect("serialisierbar");
        assert!(json.contains("\"seq\":1787000000042"), "{json}");
        let zurueck: MonitorState = serde_json::from_str(&json).expect("lesbar");
        assert_eq!(zurueck.seq, mit.seq);
    }

    #[test]
    fn monitor_state_hall_color_defaults_none() {
        // Frame eines alten Hosts/Relays simulieren: aktueller Zustand,
        // aber ohne das neue Feld.
        let mut json = serde_json::to_value(MonitorState {
            court_id: 3,
            court_label: "Feld 3".into(),
            hall_color: Some("#f59e0b".into()),
            tournament_name: String::new(),
            match_info: None,
            court_state: None,
            config: MonitorConfig::default(),
            ads: Vec::new(),
            ad_styles: Vec::new(),
            command: None,
            device_code: String::new(),
            unassigned: false,
            redirect_to: None,
            server_now_ms: 0,
            on_court_since_ms: None,
            call_timer: CallTimerView::default(),
            seq: 0,
        })
        .unwrap();
        json.as_object_mut().unwrap().remove("hallColor").unwrap();
        let s: MonitorState = serde_json::from_value(json).unwrap();
        assert_eq!(s.hall_color, None, "alter Host/Relay → farblos");
        let wieder = serde_json::to_string(&s).unwrap();
        assert!(
            !wieder.contains("hallColor"),
            "None reist nicht mit: {wieder}"
        );
    }

    #[test]
    fn distinct_halls_dedups_sorts_and_drops_empty() {
        let courts = vec![
            CourtBrief {
                id: 101,
                label: "Halle 1 · 1".into(),
                hall: "Halle 1".into(),
                hall_color: None,
            },
            CourtBrief {
                id: 401,
                label: "Halle 2 · 1".into(),
                hall: "Halle 2".into(),
                hall_color: None,
            },
            CourtBrief {
                id: 102,
                label: "Halle 1 · 2".into(),
                hall: "Halle 1".into(),
                hall_color: None,
            },
            // Leere Halle (unbekannt) wird ausgelassen.
            CourtBrief {
                id: 9,
                label: "Feld 9".into(),
                hall: String::new(),
                hall_color: None,
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
            hall_color: Some("#14b8a6".into()),
            ad_styles: Vec::new(),
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
            seq: 1_787_000_000_007,
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
                in_bar: true,
                style: AdStyleWire::default(),
            }],
            call_timer: CallTimerView {
                enabled: true,
                second_call_minutes: 2.0,
                third_call_minutes: 4.0,
            },
            logo: Some(LogoUpload {
                content_type: "image/png".into(),
                data: "BBBB".into(),
            }),
        });
    }

    #[test]
    fn ad_upload_in_bar_and_logo_default_to_off() {
        // Älterer Host-Upload ohne `inBar`/`logo` bleibt lesbar (Default aus).
        let old = r#"{"config":{"adIntervalS":8,"showDiscipline":true,"showRound":true,"showMatchNumber":true,"showTimer":true},"tournamentName":"T","ads":[{"contentType":"image/png","data":"AAAA"}]}"#;
        let up: MonitorUpload = serde_json::from_str(old).unwrap();
        assert!(!up.ads[0].in_bar);
        assert!(up.logo.is_none());
    }

    #[test]
    fn ad_style_defaults_to_black_and_off() {
        // Upload einer älteren App: kein `style` am Bild, kein `adStyles` im
        // Zustand. Beides muss lesbar bleiben und auf „nichts eingestellt"
        // fallen — dann zeigt die Anzeige Schwarz ohne Feldbezeichnung, also
        // genau das Bild von vor dem Feature (ADR 0041).
        let old = r#"{"config":{"adIntervalS":8,"showDiscipline":true,"showRound":true,"showMatchNumber":true,"showTimer":true},"tournamentName":"T","ads":[{"contentType":"image/png","data":"AAAA"}]}"#;
        let up: MonitorUpload = serde_json::from_str(old).unwrap();
        assert!(up.ads[0].style.ist_leer(), "kein Stil = Vorgabe");
        assert_eq!(up.ads[0].style.bg, "");
        assert!(!up.ads[0].style.show_court);

        // Und der Rückweg: Ein Stil, der nichts sagt, belegt keinen Platz auf
        // dem Draht.
        let ohne = AdUpload {
            content_type: "image/png".into(),
            data: "AAAA".into(),
            in_bar: false,
            style: AdStyleWire::default(),
        };
        let j = serde_json::to_string(&ohne).unwrap();
        assert!(!j.contains("style"), "leerer Stil reist nicht mit: {j}");

        // Ein gesetzter Stil überlebt den Roundtrip vollständig.
        let mit = AdUpload {
            content_type: "image/png".into(),
            data: "AAAA".into(),
            in_bar: false,
            style: AdStyleWire {
                bg: "#ffffff".into(),
                fg: "#111111".into(),
                show_court: true,
            },
        };
        let zurueck: AdUpload =
            serde_json::from_str(&serde_json::to_string(&mit).unwrap()).unwrap();
        assert_eq!(zurueck, mit);
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
    fn nur_die_dauerhafte_ablehnung_traegt_permanent() {
        // Der Normalfall bleibt von der Leitung: eine wiederholbare Absage
        // sieht auf der Wire aus wie vor v0.9.254, und ein altes Tablet liest
        // sie unverändert.
        let json = serde_json::to_string(&ResultResponse::err("später nochmal")).unwrap();
        assert!(
            !json.contains("permanent"),
            "wiederholbare Absage bleibt kompakt: {json}"
        );
        // Der Ausnahmefall dagegen muss ankommen — sonst wiederholt das
        // Tablet ein Ergebnis, das nie angenommen wird.
        let json =
            serde_json::to_string(&ResultResponse::dauerhaft("Satz 13:8 passt nicht")).unwrap();
        assert!(json.contains(r#""permanent":true"#), "{json}");
        roundtrip(&ResultResponse::dauerhaft("Satz 13:8 passt nicht"));
    }

    #[test]
    fn das_urteil_ueberlebt_den_weg_ueber_den_relay() {
        // Cloud-Weg: Der Host urteilt (R5), der Relay trägt es weiter. Ginge
        // `permanent` im ResultAck verloren, wirkte der Fix ausgerechnet dort
        // nicht, wo die meisten Tablets hängen.
        roundtrip(&HostFrame::ResultAck {
            req_id: 7,
            ok: false,
            error: Some("Satz 13:8 ist nicht regulär zu Ende gespielt.".into()),
            permanent: true,
        });
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

    // ─────────────────── TL-Web (Turnierleitungs-Oberfläche) ───────────────

    #[test]
    fn court_expectation_roundtrips_all_variants() {
        roundtrip(&CourtExpectation::Any);
        roundtrip(&CourtExpectation::Free);
        roundtrip(&CourtExpectation::Match { match_id: 4711 });
    }

    #[test]
    fn missing_expectation_is_rejected_rather_than_assumed() {
        // Kein Default: Diese Typen sind neu, es gibt keine alte
        // Gegenstelle, die geschont werden müsste. Ein fehlendes `expect`
        // würde den Konfliktschutz stillschweigend abschalten – genau die
        // Invariante, auf der die Mehrbenutzer-Fähigkeit ruht. Wer keine
        // Erwartung hat, muss `any` ausdrücklich senden.
        let missing: Result<TlAction, _> =
            serde_json::from_str(r#"{"action":"free_court","courtId":5}"#);
        assert!(missing.is_err());

        let explicit: TlAction =
            serde_json::from_str(r#"{"action":"free_court","courtId":5,"expect":{"kind":"any"}}"#)
                .unwrap();
        assert_eq!(
            explicit,
            TlAction::FreeCourt {
                court_id: 5,
                expect: CourtExpectation::Any,
            }
        );
    }

    #[test]
    fn court_expectation_distinguishes_free_from_a_named_match() {
        // „Feld war leer" und „Feld hatte Spiel X" müssen unterscheidbar
        // bleiben – mit Option<i64> wären beide `None` gewesen.
        let free: CourtExpectation = serde_json::from_str(r#"{"kind":"free"}"#).unwrap();
        let named: CourtExpectation =
            serde_json::from_str(r#"{"kind":"match","matchId":42}"#).unwrap();
        assert_eq!(free, CourtExpectation::Free);
        assert_eq!(named, CourtExpectation::Match { match_id: 42 });
        assert_ne!(free, named);
    }

    /// Ein Vertreter je `TlAction`-Variante. Wächst der Aktionssatz, muss
    /// diese Liste mitwachsen – das ist beabsichtigt: der Satz ist bewusst
    /// geschlossen (ADR 0011) und jede Erweiterung eine Entscheidung.
    fn every_tl_action() -> Vec<TlAction> {
        vec![
            TlAction::AssignCourt {
                court_id: 5,
                match_id: 4711,
                expect: CourtExpectation::Free,
            },
            TlAction::FreeCourt {
                court_id: 5,
                expect: CourtExpectation::Match { match_id: 4711 },
            },
            TlAction::MoveMatch {
                from_court_id: 5,
                to_court_id: 6,
                match_id: 4711,
                expect_from: CourtExpectation::Match { match_id: 4711 },
                expect_to: CourtExpectation::Free,
            },
            TlAction::CallPreparation {
                match_ids: vec![1, 2, 3],
                location_id: Some(7),
            },
            TlAction::RetractPreparation { match_id: 1 },
            TlAction::SetHall {
                match_id: 4711,
                hall: "Halle B".to_string(),
            },
            TlAction::ExcludeFromAutoAssign {
                match_id: 4711,
                excluded: true,
            },
            TlAction::QueueReorder {
                match_id: 4711,
                before_match_id: Some(4712),
            },
            TlAction::QueueOrderReset,
            TlAction::SetHallPrefill {
                enabled: true,
                window: 18,
            },
            TlAction::ClearAutoHalls,
            TlAction::LockCourt {
                court_id: 5,
                locked: true,
            },
            TlAction::LockCourt {
                court_id: 5,
                locked: false,
            },
            TlAction::AnnounceCourtCall {
                court_id: 5,
                match_id: 4711,
                side: None,
            },
            TlAction::AnnounceCourtCall {
                court_id: 5,
                match_id: 4711,
                side: Some(PrepCallSide::Team2),
            },
            TlAction::AnnouncePrepCall {
                match_id: 4711,
                side: PrepCallSide::Team2,
            },
            TlAction::EnterResult {
                match_id: 4711,
                sets: vec![SetAb { a: 21, b: 15 }, SetAb { a: 21, b: 19 }],
                retired: false,
                winner: None,
                overwrite: false,
            },
            TlAction::ConfirmWalkover {
                proposal_id: "p-1".to_string(),
                match_ids: vec![9],
            },
            TlAction::DismissWalkover {
                proposal_id: "p-1".to_string(),
            },
            TlAction::ScorekeeperAdvance {
                key: "k-1".to_string(),
            },
            TlAction::ScorekeeperRemove {
                key: "k-1".to_string(),
            },
            TlAction::ScorekeeperAdd {
                names: vec!["Müller".to_string(), "Schmidt".to_string()],
            },
            TlAction::SetAutoAssign { enabled: true },
            // Schiedsrichter (Spec schiedsrichter-management, Schritt 8)
            TlAction::OfficialAssign {
                court_id: 5,
                match_id: 4711,
                official_id: 3,
                role: TlOfficialRole::Sr,
            },
            TlAction::OfficialClear {
                court_id: 5,
                match_id: 4711,
                role: TlOfficialRole::Ar,
            },
            TlAction::OfficialPause {
                official_id: 3,
                paused: true,
            },
            TlAction::OfficialReorder {
                official_id: 3,
                before_official_id: Some(7),
            },
            TlAction::OfficialSetClub {
                official_id: 3,
                club: "TSV Musterstadt".to_string(),
            },
            TlAction::OfficialBlocklistSet {
                official_id: 3,
                clubs: vec!["SC Nachbar".to_string()],
                players: vec![42, 43],
            },
            TlAction::OfficialsCourtToggle {
                court_id: 5,
                sr: true,
                ar: false,
                operator: true,
            },
            TlAction::AnnounceOfficials { court_id: 5 },
            TlAction::AnnounceScorekeeper { court_id: 5 },
            TlAction::AnnounceStartPlay {
                court_id: 5,
                match_id: 77,
            },
            // Panel-Profile (Spec tl-web-panelsystem)
            TlAction::ProfileSave {
                profile: TlPanelProfileWire {
                    id: "profil-1".to_string(),
                    name: "Wandmonitor Halle 2".to_string(),
                    panels: vec![TlPanelSettingWire {
                        key: "courts".to_string(),
                        visible: true,
                        height_fr: 2.0,
                        collapsed: false,
                        column: 1,
                    }],
                    display: TlDisplaySettingsWire {
                        show_numbers: true,
                        list_position: TlListPositionWire::Bottom,
                        time_stats_axis: TlTimeStatsAxisWire::Group,
                        ..Default::default()
                    },
                    columns: 2,
                    column_widths: vec![2.0, 1.0],
                    updated_at_ms: 1_700_000_000_000,
                },
            },
            TlAction::ProfileDelete {
                profile_id: "profil-1".to_string(),
            },
            TlAction::ProfileSelect {
                profile_id: "profil-1".to_string(),
            },
            TlAction::ProfileSetDefault {
                profile_id: "profil-1".to_string(),
            },
        ]
    }

    #[test]
    fn tl_official_assign_wire_form() {
        // Wire-Vertrag mit tl.html — festgenagelt wie bei `assign_court`.
        let json = serde_json::to_string(&TlAction::OfficialAssign {
            court_id: 5,
            match_id: 4711,
            official_id: 3,
            role: TlOfficialRole::Sr,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"action":"official_assign","courtId":5,"matchId":4711,"officialId":3,"role":"sr"}"#
        );
    }

    #[test]
    fn tl_action_lock_court_wire_form() {
        // Die Oberfläche schickt genau diese Form (Spec
        // `tl-web-felder-sperren`); ein Tippfehler im Feldnamen wäre am Host
        // ein stilles Verwerfen, nicht ein Fehler.
        let json = serde_json::to_string(&TlAction::LockCourt {
            court_id: 7,
            locked: true,
        })
        .unwrap();
        assert_eq!(json, r#"{"action":"lock_court","courtId":7,"locked":true}"#);
        // Und zurück — beide Richtungen, beide Zustände.
        roundtrip(&TlAction::LockCourt {
            court_id: 7,
            locked: false,
        });
    }

    #[test]
    fn every_tl_action_variant_roundtrips() {
        for action in every_tl_action() {
            roundtrip(&action);
        }
    }

    #[test]
    fn tl_action_assign_court_wire_form() {
        // Die Wire-Form ist der Vertrag mit `tl.html` – sie wird hier
        // festgenagelt, damit ein Umbenennen nicht unbemerkt durchgeht.
        let json = serde_json::to_string(&TlAction::AssignCourt {
            court_id: 5,
            match_id: 4711,
            expect: CourtExpectation::Free,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"action":"assign_court","courtId":5,"matchId":4711,"expect":{"kind":"free"}}"#
        );
    }

    #[test]
    fn unknown_tl_action_is_rejected() {
        // Der Aktionssatz ist geschlossen (ADR 0011): Was nicht in der
        // Whitelist steht, ist nicht darstellbar und wird abgewiesen.
        let parsed: Result<TlAction, _> = serde_json::from_str(r#"{"action":"delete_tournament"}"#);
        assert!(parsed.is_err());
    }

    #[test]
    fn tl_command_frame_roundtrips_and_carries_device_and_op_id() {
        // op_id ist der Idempotenzschlüssel: dieselbe Aktion zweimal
        // geschickt (Netzwackler) darf nur einmal nach BTP schreiben.
        let frame = RelayFrame::TlCommand {
            req_id: 17,
            device_id: "dev-tl-1".to_string(),
            op_id: "op-abc".to_string(),
            view_rev: 12,
            action: TlAction::FreeCourt {
                court_id: 3,
                expect: CourtExpectation::Any,
            },
        };
        roundtrip(&frame);
    }

    #[test]
    fn tl_command_requires_identity_and_idempotency_key() {
        // Ohne Geräte- und Vorgangskennung ist weder Zuordnung noch
        // Doppelschutz möglich. Beides muss der Absender liefern – ein
        // stillschweigend leerer Wert würde alle Vorgänge eines Geräts
        // auf denselben Schlüssel werfen.
        let without_op: Result<RelayFrame, _> = serde_json::from_str(
            r#"{"type":"tl_command","reqId":1,"deviceId":"d","viewRev":1,
                "action":{"action":"set_auto_assign","enabled":true}}"#,
        );
        assert!(without_op.is_err());

        let without_device: Result<RelayFrame, _> = serde_json::from_str(
            r#"{"type":"tl_command","reqId":1,"opId":"o","viewRev":1,
                "action":{"action":"set_auto_assign","enabled":true}}"#,
        );
        assert!(without_device.is_err());
    }

    #[test]
    fn tl_command_carries_the_revision_it_was_based_on() {
        // Grundlage der Altersprüfung: Der Host lehnt Aktionen ab, die auf
        // einer veralteten Ansicht beruhen. Ohne diese Angabe wäre
        // `StaleView` für alles außer Feldaktionen unerreichbar.
        let frame: RelayFrame = serde_json::from_str(
            r#"{"type":"tl_command","reqId":1,"deviceId":"d","opId":"o","viewRev":77,
                "action":{"action":"scorekeeper_advance","key":"k"}}"#,
        )
        .unwrap();
        match frame {
            RelayFrame::TlCommand { view_rev, .. } => assert_eq!(view_rev, 77),
            other => panic!("falsche Variante: {other:?}"),
        }
    }

    #[test]
    fn tl_auth_frame_roundtrips() {
        // Der Host pusht die vollständige Token-Menge; sie ersetzt die
        // bisherige im Relay – das ist der Widerrufsmechanismus (ADR 0012).
        roundtrip(&HostFrame::TlAuth {
            devices: vec![
                TlAuthDevice {
                    id: "tl-1".to_string(),
                    token: "tok-a".to_string(),
                    ..Default::default()
                },
                TlAuthDevice {
                    id: "tl-2".to_string(),
                    token: "tok-b".to_string(),
                    ..Default::default()
                },
            ],
        });
        // Die leere Menge ist zulässig und bedeutet ausdrücklich „kein
        // Gerät zugelassen" – etwa nach dem Entfernen des letzten.
        roundtrip(&HostFrame::TlAuth {
            devices: Vec::new(),
        });
    }

    #[test]
    fn tl_auth_device_profile_id_roundtrips_with_and_without_value() {
        // ADR 0025: leer = Standardprofil, gesetzt = konkrete Wahl — beide
        // Formen müssen die Wire-Grenze unverändert überstehen.
        roundtrip(&TlAuthDevice {
            id: "tl-1".to_string(),
            token: "tok-a".to_string(),
            profile_id: String::new(),
        });
        roundtrip(&TlAuthDevice {
            id: "tl-1".to_string(),
            token: "tok-a".to_string(),
            profile_id: "profil-wand".to_string(),
        });
        // Ein alter Host, der `profile_id` noch nicht kennt, sendet das
        // Feld schlicht nicht mit — `#[serde(default)]` hält das lesbar.
        let alt: TlAuthDevice = serde_json::from_str(r#"{"id":"tl-1","token":"tok-a"}"#).unwrap();
        assert!(alt.profile_id.is_empty());
    }

    #[test]
    fn tl_panel_profile_wire_serde_roundtrip() {
        // Spec tl-web-panelsystem: Der eigens für die Wire-Grenze
        // definierte Profil-Typ (kein rohes `config::TlPanelProfile`)
        // übersteht Serialisieren + Laden unverändert, inklusive
        // verschachtelter Panel-Liste und Anzeige-Optionen.
        roundtrip(&TlPanelProfileWire {
            id: "profil-1".to_string(),
            name: "Wandmonitor Halle 2".to_string(),
            panels: vec![
                TlPanelSettingWire {
                    key: "courts".to_string(),
                    visible: true,
                    height_fr: 3.0,
                    collapsed: false,
                    column: 1,
                },
                TlPanelSettingWire {
                    key: "officials".to_string(),
                    visible: true,
                    height_fr: 1.0,
                    collapsed: true,
                    column: 2,
                },
                TlPanelSettingWire {
                    key: "finished".to_string(),
                    visible: false,
                    height_fr: 1.0,
                    collapsed: false,
                    column: 3,
                },
            ],
            display: TlDisplaySettingsWire {
                show_numbers: true,
                show_nations: true,
                show_club_names: false,
                show_club_logos: false,
                show_discipline: true,
                show_round: true,
                show_group: false,
                show_court_remaining: true,
                unlimited_court_calls: false,
                list_position: TlListPositionWire::Bottom,
                time_stats_axis: TlTimeStatsAxisWire::Group,
            },
            columns: 3,
            column_widths: vec![2.0, 1.0, 1.5],
            updated_at_ms: 1_700_000_000_000,
        });
    }

    /// Etappe D (`spielzeiten-prognose`): `showCourtRemaining` reist im
    /// Profil mit; ein altes Profil ohne das Feld liest sich als `false`
    /// (Auto-Update-Sicherheit — Browser-Stände altern langsam).
    #[test]
    fn display_settings_court_remaining_roundtrip_and_default() {
        let d = TlDisplaySettingsWire {
            show_court_remaining: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains(r#""showCourtRemaining":true"#), "{json}");
        let zurueck: TlDisplaySettingsWire = serde_json::from_str(&json).unwrap();
        assert_eq!(zurueck, d);

        let legacy = r#"{"showNumbers":true,"showNations":false,"showClubNames":false,"showClubLogos":false,"showDiscipline":false,"showRound":false,"showGroup":false,"listPosition":"right"}"#;
        let alt: TlDisplaySettingsWire = serde_json::from_str(legacy).unwrap();
        assert!(!alt.show_court_remaining);
        assert!(alt.show_numbers);
    }

    /// Option „Aufrufe unbegrenzt" (Feldtest 17.08.2026): reist als
    /// `unlimitedCourtCalls` im Profil; ein alter Browser-Stand ohne das
    /// Feld liest sich als `false` — das bisherige Verhalten (Deckel bei
    /// drei Aufrufen) bleibt dann unverändert.
    #[test]
    fn display_settings_unlimited_court_calls_roundtrip_and_default() {
        let d = TlDisplaySettingsWire {
            unlimited_court_calls: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains(r#""unlimitedCourtCalls":true"#), "{json}");
        let zurueck: TlDisplaySettingsWire = serde_json::from_str(&json).unwrap();
        assert_eq!(zurueck, d);

        let legacy = r#"{"showNumbers":true,"showNations":false,"showClubNames":false,"showClubLogos":false,"showDiscipline":false,"showRound":false,"showGroup":false,"listPosition":"right"}"#;
        let alt: TlDisplaySettingsWire = serde_json::from_str(legacy).unwrap();
        assert!(!alt.unlimited_court_calls);
    }

    #[test]
    fn tl_panel_profile_column_layout_wire_names_and_defaults() {
        // Mehrspalten-Layout (Plan tl-liste-vereinfachen F): Die Feldnamen
        // auf der Leitung sind camelCase (`columns`, `columnWidths`,
        // `column`) …
        let json = serde_json::to_string(&TlPanelProfileWire {
            id: "profil-1".to_string(),
            name: "Wandmonitor".to_string(),
            panels: vec![TlPanelSettingWire {
                key: "queue".to_string(),
                visible: true,
                height_fr: 1.0,
                collapsed: false,
                column: 3,
            }],
            display: TlDisplaySettingsWire::default(),
            columns: 3,
            column_widths: vec![2.0, 1.0, 1.0],
            updated_at_ms: 7,
        })
        .unwrap();
        assert!(json.contains(r#""columns":3"#), "{json}");
        assert!(json.contains(r#""columnWidths":[2.0,1.0,1.0]"#), "{json}");
        assert!(json.contains(r#""column":3"#), "{json}");

        // … und ein älterer Browser, der keines der drei Felder kennt,
        // bleibt lesbar: `0`/leer heißt „aus `listPosition` ableiten" bzw.
        // „Spalte 1" — beides entscheidet `tl.html`, nicht der Host.
        let alt: TlPanelProfileWire = serde_json::from_str(
            r#"{"id":"p","name":"Alt","panels":[{"key":"queue","visible":true,"heightFr":1.0}],
                "display":{"showNumbers":true,"showNations":false,"showClubNames":false,
                           "showClubLogos":false,"showDiscipline":true,"showRound":true,
                           "showGroup":true,"listPosition":"right"},
                "updatedAtMs":1}"#,
        )
        .unwrap();
        assert_eq!(alt.columns, 0);
        assert!(alt.column_widths.is_empty());
        assert_eq!(alt.panels[0].column, 0);
    }

    #[test]
    fn tl_panel_setting_collapsed_wire_name_and_default() {
        // Feldname auf der Leitung ist camelCase-neutral („collapsed"),
        // und ein älterer Browser, der das Feld nicht kennt, bleibt
        // lesbar: fehlendes `collapsed` ⇒ aufgeklappt (bisheriges
        // Verhalten), nicht zugeklappt.
        let json = serde_json::to_string(&TlPanelSettingWire {
            key: "queue".to_string(),
            visible: true,
            height_fr: 2.0,
            collapsed: true,
            column: 1,
        })
        .unwrap();
        assert!(json.contains(r#""collapsed":true"#), "{json}");

        let alt: TlPanelSettingWire =
            serde_json::from_str(r#"{"key":"queue","visible":true,"heightFr":2.0}"#).unwrap();
        assert!(!alt.collapsed, "fehlendes Feld heißt aufgeklappt");
    }

    #[test]
    fn tl_auth_without_token_field_is_rejected() {
        // Weil die Liste die bisherige ersetzt, hieße ein fehlendes Feld
        // „alle Geräte aussperren". Ein verstümmeltes Frame darf niemals
        // mitten im Turnier die gesamte Turnierleitung abmelden – es wird
        // verworfen, der bisherige Stand bleibt.
        let parsed: Result<HostFrame, _> = serde_json::from_str(r#"{"type":"tl_auth"}"#);
        assert!(parsed.is_err());
    }

    #[test]
    fn prep_call_side_must_be_explicit() {
        // Der gezielte Nachruf EINER fehlenden Partei ist der Zweck dieser
        // Aktion. Ein fehlendes Feld dürfte nicht stillschweigend die
        // weitreichendere Variante („beide") auslösen und die bereits
        // wartende Partei erneut ausrufen.
        let missing: Result<TlAction, _> =
            serde_json::from_str(r#"{"action":"announce_prep_call","matchId":7}"#);
        assert!(missing.is_err());

        for side in PrepCallSide::ALL {
            roundtrip(&side);
        }
    }

    #[test]
    fn court_call_side_is_optional_and_defaults_to_both() {
        // Anders als beim Vorbereitungs-Nachruf ist `side` hier optional:
        // Der Aufruf am Feld gilt seit jeher beiden Parteien, und genau das
        // ist die NEUTRALERE Variante — ein älterer Browser, der das Feld
        // nicht kennt, löst damit exakt das bisherige Verhalten aus.
        let alt: TlAction =
            serde_json::from_str(r#"{"action":"announce_court_call","courtId":3,"matchId":7}"#)
                .expect("ein alter Client ohne `side` bleibt lesbar");
        assert_eq!(
            alt,
            TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
                side: None,
            }
        );

        // Ohne Partei taucht das Feld auch nicht auf der Leitung auf.
        let json = serde_json::to_string(&alt).unwrap();
        assert!(!json.contains("side"), "{json}");

        for side in PrepCallSide::ALL {
            roundtrip(&TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
                side: Some(side),
            });
        }
    }

    #[test]
    fn enter_result_can_express_a_retirement() {
        // Gibt jemand mitten im Spiel auf und niemand zählte am Tablet,
        // muss die Turnierleitung das eintragen können – sonst bleibt nur
        // der Weg zum Turnier-PC (genau der Engpass) oder ein erfundener
        // Endstand in BTP und im Liveticker.
        let action = TlAction::EnterResult {
            match_id: 4711,
            sets: vec![
                SetAb { a: 21, b: 15 },
                SetAb { a: 11, b: 21 },
                SetAb { a: 5, b: 3 },
            ],
            retired: true,
            winner: Some(1),
            overwrite: false,
        };
        roundtrip(&action);
    }

    #[test]
    fn tl_state_frame_carries_opaque_json() {
        // Der Relay versteht den Inhalt nicht und soll es auch nicht –
        // er reicht ihn unverändert durch (wie court_state heute).
        let frame = HostFrame::TlState {
            rev: 42,
            json: r#"{"courts":[],"queue":[]}"#.to_string(),
        };
        roundtrip(&frame);
    }

    #[test]
    fn tl_ack_frame_roundtrips_with_and_without_error() {
        roundtrip(&HostFrame::TlAck {
            req_id: 17,
            response: TlResponse::ok(9),
        });
        roundtrip(&HostFrame::TlAck {
            req_id: 18,
            response: TlResponse::err(
                TlErrorCode::CourtTaken,
                "Feld 5 wurde gerade von jemand anderem belegt.",
            ),
        });
    }

    #[test]
    fn tl_response_omits_empty_fields_on_success() {
        // Erfolgsantworten sind der Normalfall und sollen schlank bleiben.
        let json = serde_json::to_string(&TlResponse::ok(9)).unwrap();
        assert_eq!(json, r#"{"ok":true,"stateRev":9}"#);
    }

    #[test]
    fn tl_response_error_carries_machine_readable_code() {
        // Der Code ist wichtiger als der Text: nur damit kann `tl.html`
        // gezielt reagieren, statt einen Fehlerstring zu zerlegen.
        let resp = TlResponse::err(TlErrorCode::MatchElsewhere, "Spiel steht auf Feld 3.");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""code":"match_elsewhere""#));
        assert!(!json.contains(r#""warning""#));
        roundtrip(&resp);
    }

    #[test]
    fn tl_response_error_can_name_the_revision_to_resync_to() {
        // Nach einer Ablehnung soll die Seite „auf den echten Stand
        // springen". Ohne Revision wüsste sie nicht, ob ihr nächster
        // Abruf den Stand nach dem fremden Zugriff schon enthält.
        let resp = TlResponse::err(TlErrorCode::CourtTaken, "Feld belegt.").with_state_rev(12);
        assert_eq!(resp.state_rev, 12);
        assert!(!resp.ok);
        roundtrip(&resp);
    }

    #[test]
    fn tl_response_can_warn_while_succeeding() {
        // „Ausgeführt, aber niemand konnte die Ansage sprechen" ist
        // ausdrücklich ein Erfolg mit Hinweis – kein Fehler.
        let resp = TlResponse::ok(3).with_warning("In Halle 2 ist kein Ansage-Gerät verbunden.");
        assert!(resp.ok);
        assert_eq!(resp.code, None);
        roundtrip(&resp);
    }

    #[test]
    fn every_tl_error_code_roundtrips() {
        for code in TlErrorCode::ALL {
            roundtrip(&code);
        }
    }

    #[test]
    fn unknown_host_frame_variant_yields_error_not_panic() {
        // Ein alter Relay bekommt von einem neuen Host unbekannte Frames.
        // Er verwirft sie still (`if let Ok(..)`) – das setzt voraus, dass
        // das Parsen einen Fehler liefert statt zu panicken.
        let parsed: Result<HostFrame, _> = serde_json::from_str(r#"{"type":"tl_auth_v99"}"#);
        assert!(parsed.is_err());
    }

    // ── Punktverlauf (ADR 0014) ────────────────────────────────────────

    fn beispiel_timeline() -> MatchTimeline {
        MatchTimeline {
            sets: vec![
                TimelineSet {
                    start_a: 0,
                    start_b: 0,
                    points: "AABBA".to_string(),
                },
                TimelineSet {
                    start_a: 7,
                    start_b: 5,
                    points: "BA".to_string(),
                },
            ],
            mid_game: true,
            retired: false,
            finished: false,
        }
    }

    #[test]
    fn rally_frames_roundtrip() {
        roundtrip(&TabletMsg::Rally {
            match_id: 42,
            set: 1,
            n: 3,
            winner: "A".to_string(),
            score_a: 2,
            score_b: 1,
        });
        roundtrip(&TabletMsg::RallySync {
            match_id: 42,
            timeline: beispiel_timeline(),
        });
        roundtrip(&RelayFrame::Rally {
            court_id: 7,
            match_id: 42,
            set: 2,
            n: 10,
            winner: "B".to_string(),
            score_a: 4,
            score_b: 6,
        });
        roundtrip(&RelayFrame::RallySync {
            court_id: 7,
            match_id: 42,
            timeline: beispiel_timeline(),
        });
        roundtrip(&RelayFrame::OfficialDetailRequest {
            req_id: 8,
            official_id: 3,
        });
        roundtrip(&HostFrame::OfficialDetail {
            req_id: 8,
            json: r#"{"blocked_clubs":[]}"#.to_string(),
        });
        roundtrip(&RelayFrame::TimelineRequest {
            req_id: 9,
            match_id: 42,
        });
        roundtrip(&HostFrame::TimelineData {
            req_id: 9,
            found: true,
            json: "{}".to_string(),
        });
    }

    #[test]
    fn rally_without_new_fields_parses_with_defaults() {
        // Verstümmelte/ältere Frames dürfen das Parsen nicht brechen —
        // sie werden dann inhaltlich verworfen (match_id 0), nie panicken.
        let msg: TabletMsg = serde_json::from_str(r#"{"type":"rally"}"#).unwrap();
        assert_eq!(
            msg,
            TabletMsg::Rally {
                match_id: 0,
                set: 0,
                n: 0,
                winner: String::new(),
                score_a: 0,
                score_b: 0,
            }
        );
        let sync: TabletMsg = serde_json::from_str(r#"{"type":"rally_sync"}"#).unwrap();
        assert_eq!(
            sync,
            TabletMsg::RallySync {
                match_id: 0,
                timeline: MatchTimeline::default(),
            }
        );
    }

    #[test]
    fn old_score_update_still_parses_next_to_rally_frames() {
        // Bestehende Frames bleiben unangetastet lesbar (Auto-Update ist
        // nicht atomar über Tablet-Cache/Relay/Host).
        let json = r#"{"type":"score_update","scoreA":11,"scoreB":9}"#;
        let msg: TabletMsg = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, TabletMsg::ScoreUpdate { .. }));
    }

    #[test]
    fn timeline_validation_accepts_only_ab_and_caps() {
        assert!(beispiel_timeline().is_valid());
        // Fremdzeichen in der Punktfolge: ungültig (die Folge kommt übers
        // Netz und landet in Persistenz + SVG-Renderern).
        let mut kaputt = beispiel_timeline();
        kaputt.sets[0].points = "AXB".to_string();
        assert!(!kaputt.is_valid());
        // Überlange Punktfolge: ungültig (Cloud-DoS-Deckel).
        let mut lang = beispiel_timeline();
        lang.sets[0].points = "A".repeat(MAX_RALLIES_PER_SET + 1);
        assert!(!lang.is_valid());
        // Zu viele Sätze: ungültig.
        let mut viele = beispiel_timeline();
        viele.sets = vec![TimelineSet::default(); MAX_TIMELINE_SETS + 1];
        assert!(!viele.is_valid());
        // Negativer Startstand: ungültig.
        let mut negativ = beispiel_timeline();
        negativ.sets[0].start_a = -1;
        assert!(!negativ.is_valid());
        // Absurder Startstand: ungültig — ohne den Deckel liefen sich die
        // Gitter-Schleifen der Renderer auf jedem anzeigenden Gerät tot.
        let mut riesig = beispiel_timeline();
        riesig.sets[0].start_b = i64::MAX - 1;
        assert!(!riesig.is_valid());
        assert!(MatchTimeline::default().is_valid());
        // Leere Timeline ist gültig (Match ohne gezählten Ballwechsel).
        assert!(MatchTimeline::default().is_valid());
    }

    // ── Schiedsrichterzettel (ADR 0037/0038/0039) ──────────────────────

    fn beispiel_ereignis() -> MatchEvent {
        MatchEvent {
            id: "a1b2c3d4e5f6".to_string(),
            seq: 3,
            set: 2,
            after_n: 17,
            score_a: 11,
            score_b: 6,
            ts_ms: 1_755_600_000_000,
            kind: EventKind::CardYellow,
            team: 1,
            player: 0,
            receiver_team: 0,
            receiver_player: 0,
            phase: Phase::Play,
            retracts: String::new(),
        }
    }

    /// Der Punktverlauf darf sich durch den zweiten Strom **nicht** ändern
    /// (ADR 0037). Ein Golden-String friert die Graph-Antwort ein: Wer
    /// `MatchTimeline` oder `TimelineSet` anfasst, bricht hier, nicht erst
    /// auf einer älteren `tl.html` im Turnier.
    #[test]
    fn graph_dto_bleibt_byte_gleich() {
        let json = serde_json::to_string(&beispiel_timeline()).unwrap();
        assert_eq!(
            json,
            r#"{"sets":[{"startA":0,"startB":0,"points":"AABBA"},{"startA":7,"startB":5,"points":"BA"}],"midGame":true,"retired":false,"finished":false}"#
        );
        // Und die leere Fassung, die `tl_timeline` bei einem Match ohne
        // Aufzeichnung ausliefert.
        assert_eq!(
            serde_json::to_string(&MatchTimeline::default()).unwrap(),
            r#"{"sets":[],"midGame":false,"retired":false,"finished":false}"#
        );
    }

    #[test]
    fn zettel_frames_roundtrip() {
        roundtrip(&TabletMsg::MatchEvent {
            match_id: 42,
            event: beispiel_ereignis(),
        });
        roundtrip(&TabletMsg::MatchEventSync {
            match_id: 42,
            events: vec![beispiel_ereignis()],
        });
        roundtrip(&RelayFrame::MatchEvent {
            court_id: 3,
            match_id: 42,
            event: beispiel_ereignis(),
        });
        roundtrip(&RelayFrame::MatchEventSync {
            court_id: 3,
            match_id: 42,
            events: vec![beispiel_ereignis()],
        });
        roundtrip(&RelayFrame::ScoresheetRequest {
            req_id: 7,
            match_ids: vec![41, 42, 43],
            vorab: false,
        });
        roundtrip(&RelayFrame::ScoresheetRequest {
            req_id: 8,
            match_ids: vec![41],
            vorab: true,
        });
        roundtrip(&HostFrame::ScoresheetData {
            req_id: 7,
            found: true,
            html: "<!doctype html><html></html>".to_string(),
        });
    }

    /// Alle Ereignis-Felder außer `kind` tragen `default` — ein Frame von
    /// einem älteren Tablet bleibt lesbar. `kind` ist Pflicht: Ohne sie
    /// wäre das Ereignis bedeutungslos, und ein Default wäre geraten
    /// statt gelesen.
    #[test]
    fn ereignis_ohne_optionale_felder_bleibt_lesbar() {
        let mager: MatchEvent = serde_json::from_str(r#"{"kind":"overrule"}"#).unwrap();
        assert_eq!(mager.kind, EventKind::Overrule);
        assert_eq!(mager.phase, Phase::Play);
        assert!(mager.retracts.is_empty());
        assert_eq!(mager.set, 0);

        let ohne_kind: Result<MatchEvent, _> = serde_json::from_str(r#"{"id":"ab"}"#);
        assert!(ohne_kind.is_err());
    }

    /// Ein Zettel-Abruf **ohne** das neue `vorab` bleibt lesbar und
    /// bedeutet „wie bisher" — sonst könnte ein Relay älterer Version
    /// keinen Zettel mehr anfragen.
    #[test]
    fn zettel_abruf_ohne_vorab_bleibt_lesbar() {
        let alt: RelayFrame =
            serde_json::from_str(r#"{"type":"scoresheet_request","reqId":3,"matchIds":[7]}"#)
                .unwrap();
        match alt {
            RelayFrame::ScoresheetRequest {
                req_id,
                match_ids,
                vorab,
            } => {
                assert_eq!(req_id, 3);
                assert_eq!(match_ids, vec![7]);
                assert!(!vorab, "ohne Angabe gilt das gewohnte Verhalten");
            }
            other => panic!("falscher Frame: {other:?}"),
        }
    }

    /// Eine unbekannte Art ist ein Fehler, kein Panic und keine Annahme —
    /// der Empfänger verwirft den Frame still (`if let Ok(..)`), das setzt
    /// ein `Err` voraus.
    #[test]
    fn unbekannte_ereignisart_liefert_fehler_statt_panik() {
        let parsed: Result<MatchEvent, _> =
            serde_json::from_str(r#"{"kind":"card_purple","id":"ab"}"#);
        assert!(parsed.is_err());
        let phase: Result<MatchEvent, _> =
            serde_json::from_str(r#"{"kind":"overrule","phase":"break_lunch"}"#);
        assert!(phase.is_err());
        let frame: Result<TabletMsg, _> = serde_json::from_str(r#"{"type":"match_event_v99"}"#);
        assert!(frame.is_err());
    }

    /// Das leere `retracts` wird nicht mitgeschrieben: Bei
    /// [`MAX_EVENTS_PER_MATCH`] Ereignissen kostete es rund ein Kilobyte
    /// gegen [`MAX_SHEET_LEN`], ohne etwas zu sagen.
    #[test]
    fn leeres_retracts_steht_nicht_auf_dem_draht() {
        let json = serde_json::to_string(&beispiel_ereignis()).unwrap();
        assert!(!json.contains("retracts"), "{json}");

        let zurueck = MatchEvent {
            id: "ff00ff00".to_string(),
            kind: EventKind::Retract,
            retracts: "a1b2c3d4e5f6".to_string(),
            ..beispiel_ereignis()
        };
        let json = serde_json::to_string(&zurueck).unwrap();
        assert!(json.contains(r#""retracts":"a1b2c3d4e5f6""#), "{json}");
    }

    #[test]
    fn ereignis_validierung_weist_unfug_ab() {
        assert!(beispiel_ereignis().is_valid());

        // Kennung: nicht leer, Hex-Whitelist, gedeckelt. Sie wird
        // verglichen, sortiert und protokolliert.
        let leer = MatchEvent {
            id: String::new(),
            ..beispiel_ereignis()
        };
        assert!(!leer.is_valid());
        let fremd = MatchEvent {
            id: "../../etc".to_string(),
            ..beispiel_ereignis()
        };
        assert!(!fremd.is_valid());
        let lang = MatchEvent {
            id: "a".repeat(MAX_EVENT_ID_LEN + 1),
            ..beispiel_ereignis()
        };
        assert!(!lang.is_valid());

        // team/player sind Koordinaten in einem 2×2-Raster.
        for kaputt in [
            MatchEvent {
                team: 2,
                ..beispiel_ereignis()
            },
            MatchEvent {
                player: -1,
                ..beispiel_ereignis()
            },
            MatchEvent {
                receiver_team: 7,
                ..beispiel_ereignis()
            },
            MatchEvent {
                receiver_player: i64::MAX,
                ..beispiel_ereignis()
            },
        ] {
            assert!(!kaputt.is_valid(), "{kaputt:?}");
        }

        // Anker und Stand bleiben in den Deckeln des Punktverlaufs.
        let zu_spaet = MatchEvent {
            after_n: MAX_RALLIES_PER_SET as i64 + 1,
            ..beispiel_ereignis()
        };
        assert!(!zu_spaet.is_valid());
        let zu_viele_saetze = MatchEvent {
            set: MAX_TIMELINE_SETS as i64 + 1,
            ..beispiel_ereignis()
        };
        assert!(!zu_viele_saetze.is_valid());
        let ohne_satz = MatchEvent {
            set: 0,
            ..beispiel_ereignis()
        };
        assert!(!ohne_satz.is_valid());
        let absurd = MatchEvent {
            score_b: i64::MAX - 1,
            ..beispiel_ereignis()
        };
        assert!(!absurd.is_valid());
    }

    /// Rücknahme und Ziel gehören zusammen (ADR 0038): Ohne die Kopplung
    /// gäbe es Rücknahmen ins Leere und stille Verweise an Ereignissen,
    /// die gar keine Rücknahme sind.
    #[test]
    fn ruecknahme_ohne_ziel_und_ziel_ohne_ruecknahme_sind_ungueltig() {
        let ohne_ziel = MatchEvent {
            kind: EventKind::Retract,
            retracts: String::new(),
            ..beispiel_ereignis()
        };
        assert!(!ohne_ziel.is_valid());

        let ziel_ohne_ruecknahme = MatchEvent {
            kind: EventKind::CardRed,
            retracts: "a1b2c3d4e5f6".to_string(),
            ..beispiel_ereignis()
        };
        assert!(!ziel_ohne_ruecknahme.is_valid());

        let ziel_kein_hex = MatchEvent {
            kind: EventKind::Retract,
            retracts: "zzz".to_string(),
            ..beispiel_ereignis()
        };
        assert!(!ziel_kein_hex.is_valid());

        // Selbstbezug: ein Zirkel, den die Projektion nicht auflösen
        // könnte (steht das Ereignis nun im Raster oder nicht?).
        let sich_selbst = MatchEvent {
            id: "ff00".to_string(),
            kind: EventKind::Retract,
            retracts: "ff00".to_string(),
            ..beispiel_ereignis()
        };
        assert!(!sich_selbst.is_valid());

        let gut = MatchEvent {
            id: "ff00".to_string(),
            kind: EventKind::Retract,
            retracts: "a1b2c3d4e5f6".to_string(),
            ..beispiel_ereignis()
        };
        assert!(gut.is_valid());
    }

    /// Der Bestands-Deckel zählt **auch die Rücknahmen** mit — der Bestand
    /// wächst monoton (ADR 0038).
    #[test]
    fn bestandsdeckel_gilt_fuer_zahl_und_inhalt() {
        let voll = vec![beispiel_ereignis(); MAX_EVENTS_PER_MATCH];
        assert!(match_events_valid(&voll));

        let zu_viele = vec![beispiel_ereignis(); MAX_EVENTS_PER_MATCH + 1];
        assert!(!match_events_valid(&zu_viele));

        let mit_unfug = vec![
            beispiel_ereignis(),
            MatchEvent {
                team: 9,
                ..beispiel_ereignis()
            },
        ];
        assert!(!match_events_valid(&mit_unfug));

        assert!(match_events_valid(&[]));
    }

    /// Die Sanktions-Arten sind der Grund für den eigenen Strom, die
    /// eigene Datei und die eigene Route — sie stehen namentlich fest,
    /// damit der Wächter-Test in `tl.rs` etwas Strukturelles zu prüfen hat
    /// statt einer Textregel.
    #[test]
    fn sanktionsarten_sind_benannt() {
        for art in [
            EventKind::CardYellow,
            EventKind::CardRed,
            EventKind::CardBlack,
            EventKind::Disqualified,
        ] {
            assert!(art.is_sanction(), "{art:?}");
        }
        for art in [
            EventKind::ServeStart,
            EventKind::InjuryStart,
            EventKind::InjuryEnd,
            EventKind::Suspension,
            EventKind::Overrule,
            EventKind::RefereeCall,
            EventKind::Retired,
            EventKind::Retract,
        ] {
            assert!(!art.is_sanction(), "{art:?}");
        }
    }
}
