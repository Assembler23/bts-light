//! bts-relay – Cloud-Relay für den digitalen Tablet-Spielzettel.
//!
//! Auf IT-verwalteten Turnier-PCs blockiert die Windows-Firewall eingehende
//! Verbindungen; manche Hallen-WLANs isolieren die Geräte. Dann erreichen
//! die Tablets bts-light nicht direkt. Dieser Relay löst das: bts-light
//! **und** die Tablets verbinden sich nur noch *nach außen* zu badhub.de –
//! eine ausgehende Verbindung lässt jede Firma-IT durch.
//!
//! Der Relay ist ein reiner Broker ohne Persistenz. Jede bts-light-
//! Installation hat über ihre `install_id` einen eigenen **Namespace** –
//! Turniere kollidieren nicht. Pro Namespace gibt es genau einen „Host"
//! (bts-light) und beliebig viele Tablets, je an einen Court gebunden.
//!
//! Läuft als systemd-Dienst auf dem Hetzner-Server hinter nginx
//! (`https://badhub.de/bts-relay/` → `127.0.0.1:8090`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use include_dir::{include_dir, Dir};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, Mutex};

use relay_proto::{
    device_code, html_escape, path_encode, CourtBrief, HostFrame, MatchBrief, MonitorConfig,
    MonitorControl, MonitorDeviceInfo, MonitorMatch, MonitorPlayer, MonitorState, MonitorUpload,
    PlayerBrief, RelayFrame, ResultBody, ResultResponse, ServerMsg, SetAb, TabletMsg, TlAction,
    TlErrorCode, TlResponse,
};

/// Die Tablet-Spielzettel-UI – dieselbe Datei wie in der bts-light-App.
const TABLET_HTML: &str = include_str!("../../src-tauri/assets/tablet.html");

/// Die Turnierleitungs-Oberfläche – dieselbe Datei wie in der bts-light-App.
/// Eine Quelle für LAN und Cloud: Was im Hallennetz erprobt wurde, ist
/// unterwegs dieselbe Seite.
const TL_HTML: &str = include_str!("../../src-tauri/assets/tl.html");

/// Die Court-Monitor-Anzeige – dieselbe Datei wie in der bts-light-App.
const MONITOR_HTML: &str = include_str!("../../src-tauri/assets/monitor.html");

/// Der „In Vorbereitung"-Info-Monitor – dieselbe Datei wie im LAN. Leitet
/// seinen Basis-Pfad aus der eigenen URL ab, läuft also unter `/{ns}/…` ohne
/// Templating.
const PREPARATION_HTML: &str = include_str!("../../src-tauri/assets/preparation.html");

/// Die Vollbild-Werbe-Anzeige (Rotation) – dieselbe Datei wie im LAN. Leitet
/// ihren Basis-Pfad aus der eigenen URL ab, läuft also unter `/{ns}/info/ad`.
const AD_HTML: &str = include_str!("../../src-tauri/assets/ad.html");

/// Die Court-Übersicht (alle Felder × aktuelles Spiel) – dieselbe Datei wie im
/// LAN. Leitet ihren Basis-Pfad aus der eigenen URL ab und holt ihre Daten über
/// `<BASE>health`; läuft also unter `/{ns}/info/overview` gegen `/{ns}/health`.
const OVERVIEW_HTML: &str = include_str!("../../src-tauri/assets/overview.html");

/// Gebündelte SVG-Länderflaggen (IOC-Code → `<code>.svg`), ausgeliefert
/// unter `/{ns}/flags/{file}`.
static FLAGS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../src-tauri/assets/flags");

/// Obergrenze gleichzeitiger Werbebilder je Namespace.
const MAX_ADS: usize = 24;

/// Obergrenze der Gesamtgröße aller Werbebilder eines Namespace (12 MB).
const MAX_ADS_TOTAL: usize = 12 * 1024 * 1024;

/// Obergrenze fürs Turnierlogo (2 MB) – dasselbe Maß, das die bts-light-App
/// beim Setzen des Logos erzwingt. Ein eigener, knapper Cap statt des vollen
/// Ad-Budgets: das Logo ist naturgemäß klein und soll die Speicherobergrenze
/// je Namespace nicht verdoppeln.
const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;

/// Body-Limit der Werbe-Upload-Route – Base64 bläht die Rohdaten ~+33 % auf.
const MONITOR_UPLOAD_LIMIT: usize = 20 * 1024 * 1024;

/// Obergrenze gleichzeitiger Tablets je Namespace (einfacher Missbrauchs-
/// Schutz – ein reales Turnier hat höchstens ~30 Felder).
const MAX_TABLETS_PER_NS: usize = 64;

/// WebSocket-Ping-Intervall – hält Verbindungen über NAT/LB offen.
const HEARTBEAT: Duration = Duration::from_secs(30);

/// Ping-Intervall der HOST-Verbindung – bewusst enger als [`HEARTBEAT`],
/// damit `host_last_seen` (Pong-Stempel) höchstens ~5 s alt ist und die
/// Stale-Erkennung schnell und sicher entscheiden kann.
///
/// **Kopplung (nicht ohne Abstimmung erhöhen):** Der Host-Client
/// (`src-tauri/.../relay_client.rs`, `RELAY_READ_IDLE = 15 s`, Hebel D /
/// ADR 0020) verlässt sich darauf, dass dieser Ping ≤ 5 s bleibt — er
/// erkennt eine half-open Verbindung an ausbleibendem Empfang und pingt
/// selbst nicht. Wird `HOST_PING` größer, muss `RELAY_READ_IDLE` mitziehen.
const HOST_PING: Duration = Duration::from_secs(5);

/// Nach so viel Empfangs-Stille gilt eine Host-Verbindung als tot
/// (= 3 verpasste Pongs bei [`HOST_PING`]): Die Verbindung beendet sich
/// selbst, und ein neu verbindender Host darf den Slot übernehmen.
/// Ein LEBENDIGER Host antwortet binnen ~5 s auf Pings — die Übernahme
/// kann also nie einen gesunden Host verdrängen (R4 bleibt gewahrt).
const HOST_STALE: Duration = Duration::from_secs(15);

/// Ping-Intervall der TABLET-Verbindung — bewusst enger als [`HEARTBEAT`]
/// (die geteilte 30-s-Konstante bleibt `monitor_conn` vorbehalten). Der
/// Relay pingt jedes Tablet aktiv; der Browser auto-pongt auf **Protokoll-
/// Ebene**, immun gegen die JS-Timer-Drosselung backgroundeter mobiler
/// Seiten. So fällt ein totes Tablet schnell auf, ohne ein lebendes zu
/// verdrängen (Hebel D / ADR 0020).
const TABLET_PING: Duration = Duration::from_secs(5);

/// Nach so viel Empfangs-Stille gilt eine Tablet-Verbindung als tot
/// (= 3 verpasste Pongs bei [`TABLET_PING`]): Die Verbindung beendet sich
/// selbst und gibt den Relay-Slot frei (in ~15 s statt bei ~30 s+). Ein
/// LEBENDIGES Tablet pongt binnen ~5 s — ein gesundes Feld wird also nie
/// gedroppt; ein 5–10-s-WLAN-Hänger (< 15 s) ebenso wenig.
const TABLET_STALE: Duration = Duration::from_secs(15);

/// Reine Stale-Entscheidung: liegt der letzte Empfang mindestens `threshold`
/// zurück? Ausgelagert, damit die Grenz-Semantik (`>=`) ohne Laufzeit/Clock
/// prüfbar ist. `tokio::time::Instant`, damit `tokio::time::pause()` in Tests
/// greift (wie im ganzen Zombie-/Stale-Pfad).
fn is_stale(last: tokio::time::Instant, now: tokio::time::Instant, threshold: Duration) -> bool {
    now.duration_since(last) >= threshold
}

/// Wie lange der Relay auf die `ResultAck` von bts-light wartet. 8 s (nicht mehr
/// 20 s, Hebel B / ADR 0018): Der Client puffert das Ergebnis ohnehin und retryt
/// (idempotent seit dem `process_result`-Idempotenz-Zweig) — ein kürzeres
/// Warten gibt den `pending`-Slot (Cap `MAX_PENDING_PER_NS`) bei zäher Leitung
/// schneller frei, statt ihn 20 s je Versuch zu binden.
const RESULT_TIMEOUT: Duration = Duration::from_secs(8);

/// Obergrenze gleichzeitiger Namespaces (Speicher-Schutz – jede echte
/// Installation ist ein Namespace, real also höchstens ein paar Hundert).
const MAX_NAMESPACES: usize = 2000;

/// Obergrenze offener Ergebnis-Übermittlungen je Namespace.
const MAX_PENDING_PER_NS: usize = 16;

/// Maximale Größe eines gespiegelten Spielzustands (Schutz gegen Missbrauch).
const MAX_STATE_LEN: usize = 64 * 1024;

type Tx = mpsc::UnboundedSender<Message>;

/// Ein hochgeladenes Werbebild im Speicher (Content-Type + Rohbytes).
struct AdImage {
    content_type: String,
    bytes: Vec<u8>,
    /// `true`, wenn das Bild zusätzlich klein in der oberen Leiste erscheinen
    /// soll (Sponsor-Leiste). Bestimmt, welche Indizes `/{ns}/info/ad/state`
    /// als `barAds` ausweist.
    in_bar: bool,
}

/// Court-Monitor-Datensatz eines Namespace: Anzeige-Konfiguration und
/// Werbebilder, vom bts-light-Host hochgeladen.
struct MonitorBundle {
    config: MonitorConfig,
    tournament_name: String,
    ads: Vec<AdImage>,
    /// Aufruf-Timer-Schwellen (vom Host hochgeladen) für die Monitor-Anzeige.
    call_timer: relay_proto::CallTimerView,
    /// Turnierlogo für die Sponsor-Leiste (Content-Type + Rohbytes), falls der
    /// Host eins hochgeladen hat. `None` = kein Logo.
    logo: Option<AdImage>,
}

/// Ein Namespace: ein bts-light-Host und seine Tablets.
struct Namespace {
    /// Sende-Ende zur Host-WebSocket (bts-light), falls verbunden.
    host: Option<Tx>,
    /// Zeitpunkt (Unix-ms) des letzten Lebenszeichens der Host-Verbindung
    /// (Frame oder Pong). Grundlage der Zombie-Host-Ablösung: ein neuer
    /// Host darf einen seit [`HOST_STALE`] stummen alten ersetzen
    /// (Turnier-Befund 19.07.: tote TCP-Verbindung nach Netzwechsel hielt
    /// den Slot 17 Minuten — der Master war so lange ausgesperrt).
    host_last_seen: u64,
    /// CourtID → Sende-Ende zur Tablet-WebSocket.
    tablets: HashMap<i64, Tx>,
    /// CourtID → Geräte-Kennung des aktiven Tablets (leer bei alten
    /// Tablet-Seiten). Reconnect-Erkennung: dasselbe Gerät darf seine
    /// eigene, tote Session nahtlos ablösen (kein „Feld belegt").
    tablet_devices: HashMap<i64, String>,
    /// CourtID → zuletzt gespiegelter Spielzustand (JSON) des aktiven
    /// Tablets – wird einem übernehmenden Gerät übergeben.
    court_state: HashMap<i64, String>,
    /// CourtID → aktuelles Match (für die Court-Monitor-Anzeige).
    court_matches: HashMap<i64, MatchBrief>,
    /// CourtID → Satzstand in Team-Koordinaten (für die Monitor-Anzeige).
    court_scores: HashMap<i64, Vec<SetAb>>,
    /// CourtID → Zeitpunkt (Unix-ms), seit dem das aktuelle Spiel auf dem Feld
    /// steht (1. Aufruf). Wird beim ersten `MatchAssigned` eines neuen Matches
    /// gestempelt – Grundlage der Aufruf-Uhr am Cloud-Monitor.
    court_on_court_since: HashMap<i64, u64>,
    /// CourtID → Feldname (Anzeige) – vom Host mit jedem `MatchAssigned`/
    /// `MatchCleared`-Frame mitgeliefert, für die Monitor-Anzeige.
    court_labels: HashMap<i64, String>,
    /// CourtID → Hallenname (BTP-Location) – für die hallengefilterte
    /// Cloud-Ansage der fernen Halle (B1a).
    court_hall: HashMap<i64, String>,
    /// Freitext-Ansagen (Master → Slave), dedupliziert nach id, Cap 50.
    freetext: Vec<relay_proto::FreetextItem>,
    /// Cloud-Ansage-Slaves: id → (Halle, letzter Poll Unix-ms). Für die
    /// „ferne Halle online?"-Anzeige am Master. Rein informativ.
    slaves: HashMap<String, (String, u64)>,
    /// Vollständige Feld-Liste (vom Host via `HostFrame::Courts` gepusht) für
    /// das Cloud-Feldwechsel-Menü des Tablets (`/{ns}/courts`).
    courts: Vec<CourtBrief>,
    /// Aufgerufene (in Vorbereitung gerufene) Spiele der fernen Hallen – für
    /// die Slave-Spielübersicht + den Nachruf am Slave (Cluster C Stufe 2).
    /// Vom Host via `HostFrame::Prepared` gepusht; ersetzt jeweils die Liste.
    prepared: Vec<relay_proto::PreparedMatch>,
    /// Azure-TTS-Konfiguration des Masters für die Vererbung an Cloud-Ansage-
    /// Slaves (ADR 0003). Kommt mit jedem `HostFrame::Courts`-Push; `None`
    /// überschreibt bewusst — Azure am Master aus = Vererbung endet. Enthält
    /// den Subscription-Key → niemals loggen.
    azure_tts: Option<relay_proto::AzureTtsShare>,
    /// Court-Monitor-Konfiguration + Werbebilder, falls hochgeladen.
    monitor: Option<MonitorBundle>,
    /// Geräte-Steuerung (Feld-Zuweisungen + Fernbefehle), vom Host gepusht.
    monitor_control: MonitorControl,
    /// Geräte-ID → Zeitpunkt des letzten Monitor-Polls (Unix-ms) – für den
    /// Online-Status in der „Court-Monitore"-Seite des Tools.
    monitor_seen: HashMap<String, u64>,
    /// Offene Ergebnis-Übermittlungen: `req_id` → wartender HTTP-Handler.
    pending: HashMap<u64, oneshot::Sender<ResultResponse>>,
    /// Fortlaufende Request-ID für Ergebnis-Übermittlungen.
    next_req: u64,
    /// Zugänge der Turnierleitungs-Geräte, vom Turnier-PC gespiegelt
    /// (`HostFrame::TlAuth`): Zugang → Kennung des Geräts. Der Relay stellt
    /// **keine** aus und merkt sich nichts über das Turnier hinaus: Was der
    /// Host nicht mehr nennt, gilt nicht mehr. Das ist der Widerruf (ADR 0012).
    /// Die Kennung reist mit jedem Kommando zurück, damit das Protokoll des
    /// Turnier-PCs benennen kann, wer gehandelt hat.
    tl_tokens: HashMap<String, String>,
    /// Zuletzt gepushter Anzeige-Zustand: `(Revision, JSON)`. **Opak** — der
    /// Relay liest ihn nie, er legt ihn ab und liefert ihn aus. So bleibt
    /// jede Turnierlogik im Host (R5).
    tl_state: Option<(u64, String)>,
    /// Offene TL-Kommandos: `req_id` → wartender HTTP-Handler.
    tl_pending: HashMap<u64, oneshot::Sender<TlResponse>>,
    /// Offene Punktverlauf-Abrufe: `req_id` → wartender HTTP-Handler,
    /// Antwort `(found, json)` — der Relay hält keine Verläufe vor (AK-5),
    /// er reicht nur durch.
    timeline_pending: HashMap<u64, oneshot::Sender<(bool, String)>>,
    /// Belegte Geräteplätze: Zugang → letzter Zugriff (Unix-ms). Begrenzt,
    /// damit nicht Dutzende Browser denselben Turnier-PC abfragen.
    tl_devices: HashMap<String, u64>,
    /// Zählt jede neue Host-Verbindung. Teil des ETags, weil die Revision
    /// beim Neustart des Turnier-PCs wieder klein beginnt.
    tl_gen: u64,
    /// Court-Monitor-Nudge-Abonnenten (A1, ADR 0016): CourtID → Sende-Enden
    /// der Monitor-WS-Verbindungen, die **genau dieses Feld** beobachten (ein
    /// Court-Monitor). Bei einer Änderung des Felds bekommt jeder Eintrag ein
    /// winziges „Feld geändert, seq N"-Signal; die Anzeige holt daraufhin den
    /// Vollstand über ihre bestehende Poll-Route. Namespace-lokal — ein Nudge
    /// verlässt seinen Namespace nie.
    monitor_subs: HashMap<i64, Vec<Tx>>,
    /// Nudge-Abonnenten **ohne** Feld-Filter: die Feld-Übersicht
    /// (`overview.html`) will Signale ALLER Felder dieses Namespace.
    monitor_subs_all: Vec<Tx>,
    /// Pro-Court monoton steigende Nudge-Sequenz (Client verwirft Veraltetes).
    monitor_seq: HashMap<i64, u64>,
    /// A2 / ADR 0017 (Reconnect-Wahrheit): der Legacy-rev-Schalter des Hosts,
    /// vom Host über `HostFrame::Courts` durchgereicht. `false` (Default) =
    /// Ownership aktiv → der Relay meldet `ownership_active=true` im
    /// `StateRestore` und das Tablet folgt der Autorität. `true` = Legacy →
    /// `ownership_active=false`, das Tablet nutzt seine rev-Logik. So greift
    /// der Laufzeit-Rollback AUCH im Cloud-Modus.
    reconnect_legacy_rev: bool,
}

/// Der winzige Monitor-Nudge (A1, ADR 0016): „Feld `court` hat sich geändert,
/// Sequenz `seq`". Trägt bewusst KEINE Score-Daten — die Anzeige holt den
/// Vollstand über ihre bestehende Poll-Route (eine Datenquelle, kein
/// Flackern). Identisches Drahtformat wie am LAN-Server.
#[derive(Serialize)]
struct MonitorNudge {
    court: i64,
    seq: u64,
}

/// Obergrenze der Monitor-Nudge-Abonnenten je Namespace (Fan-out-/DoS-
/// Schutz, analog `MAX_MONITOR_DEVICES`). Ein reales Turnier hat je Feld
/// wenige TVs; darüber hinausgehende Verbindungen fallen still auf ihren
/// Poll-Fallback zurück (kein Regress).
const MAX_MONITOR_SUBS: usize = 256;

impl Namespace {
    fn new() -> Self {
        Self {
            host: None,
            host_last_seen: 0,
            tablets: HashMap::new(),
            tablet_devices: HashMap::new(),
            court_state: HashMap::new(),
            court_matches: HashMap::new(),
            court_scores: HashMap::new(),
            court_on_court_since: HashMap::new(),
            court_labels: HashMap::new(),
            court_hall: HashMap::new(),
            prepared: Vec::new(),
            freetext: Vec::new(),
            slaves: HashMap::new(),
            courts: Vec::new(),
            azure_tts: None,
            monitor: None,
            monitor_control: MonitorControl::default(),
            monitor_seen: HashMap::new(),
            pending: HashMap::new(),
            next_req: 1,
            tl_tokens: HashMap::new(),
            tl_state: None,
            tl_pending: HashMap::new(),
            timeline_pending: HashMap::new(),
            tl_devices: HashMap::new(),
            tl_gen: 0,
            monitor_subs: HashMap::new(),
            monitor_subs_all: Vec::new(),
            monitor_seq: HashMap::new(),
            reconnect_legacy_rev: false,
        }
    }

    /// Leer = kann aus der Namespace-Tabelle entfernt werden. Der
    /// Court-Monitor-Datensatz (`monitor`) zählt hier bewusst NICHT mit:
    /// ohne Host gibt es nichts anzuzeigen, und der Host lädt ihn nach
    /// einem Reconnect binnen 30 s erneut hoch. Ihn zu behalten würde nur
    /// Speicher belegen, falls ein Host endgültig weg ist.
    fn is_empty(&self) -> bool {
        self.host.is_none() && self.tablets.is_empty() && self.pending.is_empty()
    }
}

/// Geteilter Broker-Zustand aller Handler.
#[derive(Clone)]
struct Broker {
    namespaces: Arc<Mutex<HashMap<String, Namespace>>>,
    /// Telefon-Kopplungscodes (ADR 0004): Code → (Namespace, Ablauf Unix-ms).
    /// Nur im RAM; ein Relay-Neustart macht offene Codes ungültig.
    pairings: Arc<Mutex<HashMap<String, PairingEntry>>>,
    /// Fehlversuchs-Zähler fürs Einlösen (globales Sliding Window gegen
    /// Durchprobieren): (Fensterbeginn Unix-ms, Fehlversuche im Fenster).
    pair_fails: Arc<Mutex<(u64, u32)>>,
    /// Wegweiser der Turnierleitungs-Zugänge: Zugang → Namespace.
    ///
    /// Nötig, weil die TL-Adressen **keinen** Namespace tragen: Der ist die
    /// `install_id` und damit zugleich der Zugang der Zähltablets
    /// (`/{ns}/ws`). Stünde sie in der Adresse, die jeder Helfer auf dem
    /// Bildschirm hat, könnte sich damit jeder als Tablet ausgeben (ADR 0012).
    /// Der Zugang findet sein Turnier deshalb selbst.
    tl_index: Arc<Mutex<HashMap<String, String>>>,
    /// Öffentliche Basis-URL für QR-Codes, z. B. `https://badhub.de/bts-relay`.
    public_base: String,
}

/// Ein ausgestellter Telefon-Kopplungscode (ADR 0004).
struct PairingEntry {
    namespace: String,
    expires_ms: u64,
}

/// Gültigkeit eines Telefon-Kopplungscodes (Nutzerwunsch 19.07.2026:
/// 1 Stunde statt 15 Minuten — bequemer beim Turnier-Aufbau).
const PAIRING_TTL_MS: u64 = 60 * 60 * 1000;
/// Fehlversuchs-Fenster + -Limit fürs Einlösen (danach 429). Großzügig für
/// vertippte Menschen, viel zu knapp für 10⁸ Kombinationen.
const PAIR_FAIL_WINDOW_MS: u64 = 60_000;
const PAIR_FAIL_LIMIT: u32 = 100;

impl Broker {
    fn new(public_base: String) -> Self {
        Self {
            namespaces: Arc::new(Mutex::new(HashMap::new())),
            pairings: Arc::new(Mutex::new(HashMap::new())),
            pair_fails: Arc::new(Mutex::new((0, 0))),
            tl_index: Arc::new(Mutex::new(HashMap::new())),
            public_base,
        }
    }
}

/// Erzeugt einen 8-stelligen Zahlen-Code (führende Nullen möglich) aus
/// OS-Zufall. Modulo-Bias bei u64 → 10⁸ ist vernachlässigbar (~10⁻¹¹).
fn gen_pairing_code() -> Result<String, String> {
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).map_err(|e| e.to_string())?;
    Ok(format!("{:08}", u64::from_le_bytes(buf) % 100_000_000))
}

/// Sieht `code` wie ein Telefon-Kopplungscode aus (genau 8 Ziffern)?
fn valid_pairing_code(code: &str) -> bool {
    code.len() == 8 && code.bytes().all(|b| b.is_ascii_digit())
}

/// Serialisiert einen Wert zu einem WebSocket-Text-Frame.
fn text<T: Serialize>(value: &T) -> Message {
    Message::Text(Utf8Bytes::from(
        serde_json::to_string(value).unwrap_or_default(),
    ))
}

/// Prüft, ob `ns` wie eine kanonische UUID aussieht (Form `8-4-4-4-12`,
/// nur Hex und Bindestriche). Die `install_id` ist immer eine
/// `crypto.randomUUID()` – frei erfundene oder überlange Namespaces
/// werden so abgewiesen, bevor sie Speicher belegen.
fn valid_namespace(ns: &str) -> bool {
    let bytes = ns.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Richtet das Logging ein. Ist `RELAY_LOG_DIR` gesetzt, schreibt der Relay
/// ZUSÄTZLICH zu stdout (journald) in eine **täglich rotierende Datei**
/// `bts-relay.log.YYYY-MM-DD` in diesem Verzeichnis — auf dem Hetzner-Server
/// nach `storage/relay-logs/`, das der `badhub`-User direkt lesen darf (kein
/// journalctl-Recht nötig). Ohne die Env-Var bleibt es bei stdout-only (lokal/
/// `cargo run`). Der zurückgegebene Guard muss für die Programmlaufzeit leben
/// (sonst flusht der non-blocking Writer nicht).
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::prelude::*;
    // INFO-Default: unsere Diagnose-Zeilen (info!) bleiben, der TRACE-/DEBUG-
    // Verbindungsspam von axum/hyper wird gefiltert → lesbare, kompakte Datei.
    let level = LevelFilter::INFO;
    let stdout_layer = tracing_subscriber::fmt::layer().with_ansi(false);
    match std::env::var("RELAY_LOG_DIR") {
        Ok(dir) if !dir.is_empty() => {
            // Relay läuft als `badhub` und darf in storage/ schreiben. Scheitert
            // das (falsche Rechte/Quota), warnen wir nach stdout/journald — sonst
            // bliebe die erwartete Datei stumm leer (im Ernstfall der falsche
            // Moment, das zu merken). Der stdout-Fallback greift weiterhin.
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("WARN: RELAY_LOG_DIR '{dir}' nicht anlegbar: {e} — nur stdout");
            }
            let (non_blocking, guard) = tracing_appender::non_blocking(
                tracing_appender::rolling::daily(&dir, "bts-relay.log"),
            );
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking);
            tracing_subscriber::registry()
                .with(level)
                .with(stdout_layer)
                .with(file_layer)
                .init();
            Some(guard)
        }
        _ => {
            tracing_subscriber::registry()
                .with(level)
                .with(stdout_layer)
                .init();
            None
        }
    }
}

#[tokio::main]
async fn main() {
    // Guard bis Programmende halten → der Datei-Writer flusht zuverlässig.
    let _log_guard = init_tracing();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);
    let public_base =
        std::env::var("PUBLIC_BASE").unwrap_or_else(|_| "https://badhub.de/bts-relay".to_string());

    let broker = Broker::new(public_base.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route("/{ns}/court/{id}", get(court_page))
        .route("/{ns}/courts", get(courts_list))
        .route("/{ns}/court/{id}/display", get(monitor_page))
        .route("/{ns}/court/{id}/state", get(monitor_state))
        .route("/{ns}/monitor", get(monitor_device_page))
        .route("/{ns}/monitor/state", get(monitor_device_state))
        .route("/{ns}/monitor/control", post(monitor_control_upload))
        .route("/{ns}/monitor-devices", get(monitor_devices_list))
        .route("/{ns}/info/announce/state", get(announce_state))
        .route("/{ns}/info/ad/state", get(ad_bar_state))
        .route("/{ns}/info/logo", get(tournament_logo))
        .route("/{ns}/info/preparation", get(preparation_page))
        .route("/{ns}/info/preparation/state", get(preparation_state))
        .route("/{ns}/info/ad", get(ad_page))
        .route("/{ns}/info/overview", get(overview_page))
        .route("/{ns}/health", get(overview_health))
        .route("/{ns}/pairing-code", post(pairing_code_create))
        .route("/pair/{code}", get(pairing_resolve))
        .route("/{ns}/slaves", get(slaves_list))
        .route("/{ns}/qr/{id}", get(qr_svg))
        .route("/{ns}/flags/{file}", get(flag_route))
        .route("/{ns}/ads/{idx}", get(ad_image))
        .route(
            "/{ns}/monitor",
            post(monitor_upload).layer(DefaultBodyLimit::max(MONITOR_UPLOAD_LIMIT)),
        )
        .route("/{ns}/ws", get(tablet_ws))
        // Court-Monitor-Nudge (A1, ADR 0016): niedrig-latente Anzeige über die
        // Cloud. `court` gesetzt → nur dieses Feld, fehlt es → alle Felder.
        .route("/{ns}/monitor-ws", get(monitor_ws))
        .route("/{ns}/host-ws", get(host_ws))
        .route("/{ns}/result", post(result));

    // Not-Aus für die Turnierleitungs-Oberfläche: `BTS_RELAY_TL=off` lässt
    // die Routen gar nicht erst entstehen. Der Relay ist ein **globales**
    // Binary für alle Installationen; träte im neuen Schreibweg ein Fehler
    // auf, muss er sich ohne Rebuild und ohne Rückbau der übrigen Dienste
    // abschalten lassen — mitten im Turnierbetrieb anderer.
    let tl_an = tl_enabled(std::env::var("BTS_RELAY_TL").ok().as_deref());
    // **Ohne Namespace in der Adresse**: Der wäre die `install_id` und damit
    // zugleich der Zugang der Zähltablets. Der Zugang des Geräts findet sein
    // Turnier über den Wegweiser selbst (ADR 0012).
    let app = if tl_an {
        app.route("/tl", get(tl_page))
            .route("/tl/api/state", get(tl_state_route))
            .route("/tl/api/command", post(tl_command_route))
            // Punktverlauf on-demand (Spec punktverlauf-graph, AK-5) —
            // gleicher Pfad wie am LAN-Server, damit tl.html in beiden
            // Modi identisch abruft.
            .route("/tl/api/timeline/{match_id}", get(tl_timeline_route))
            // Flaggen für die TL-Seite: Sie hängt ohne Namespace unter
            // `/tl` und findet ihre Flaggen deshalb unter `/flags/…` —
            // Begründung am Handler.
            .route("/flags/{file}", get(flag_route_global))
    } else {
        tracing::warn!("Turnierleitungs-Oberfläche per BTS_RELAY_TL=off abgeschaltet");
        app
    };
    let app = app.with_state(broker);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bts-relay konnte den Port nicht binden");
    tracing::info!("bts-relay lauscht auf 127.0.0.1:{port} (öffentlich: {public_base})");
    axum::serve(listener, app)
        .await
        .expect("bts-relay-Server beendet");
}

// ─────────────────────────────── HTTP-Routen ──────────────────────────────

/// Status-Schnappschuss.
async fn health(State(broker): State<Broker>) -> Json<serde_json::Value> {
    let map = broker.namespaces.lock().await;
    Json(serde_json::json!({
        "ok": true,
        "namespaces": map.len(),
        "tablets": map.values().map(|n| n.tablets.len()).sum::<usize>(),
    }))
}

/// Liefert die Tablet-UI für ein Feld (per CourtID; kein Caching – immer
/// frisch). Der Feldname für die Anzeige stammt – falls bekannt – aus dem
/// Namespace; sonst bleibt er leer und wird vom ersten Server-Frame
/// nachgeliefert.
async fn court_page(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    tracing::info!("Tablet-Seite ausgeliefert für Feld {court_id}");
    let label = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.court_labels.get(&court_id).cloned())
            .unwrap_or_default()
    };
    let body = TABLET_HTML
        .replace("__COURT_ID__", &court_id.to_string())
        .replace("__COURT_LABEL__", &html_escape(&label))
        // Der Relay kennt den Host-PIN nicht → leer lassen; tablet.html fällt
        // dann defensiv auf „0000" zurück. Die Feldwechsel-Liste liefert
        // `/{ns}/courts` (vom Host gepusht).
        .replace("__TABLET_PIN__", "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// Feld-Liste fürs Feldwechsel-PIN-Menü des Tablets (Cloud-Modus). Liefert die
/// vom Host via `HostFrame::Courts` gepushte Liste; leer, solange kein Push kam.
async fn courts_list(State(broker): State<Broker>, Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let courts = {
        let map = broker.namespaces.lock().await;
        map.get(&ns).map(|n| n.courts.clone()).unwrap_or_default()
    };
    let items: Vec<serde_json::Value> = courts
        .into_iter()
        .map(|c| serde_json::json!({ "id": c.id, "label": c.label, "hall": c.hall }))
        .collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::Value::Array(items)),
    )
        .into_response()
}

/// QR-Code (SVG), der auf die öffentliche Tablet-URL des Felds (per
/// CourtID) zeigt.
async fn qr_svg(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let url = format!(
        "{}/{}/court/{}",
        broker.public_base,
        path_encode(&ns),
        court_id
    );
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(220, 220)
                .build();
            (
                [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
                svg,
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "QR-Erzeugung fehlgeschlagen",
        )
            .into_response(),
    }
}

// ─────────────────────────────── Court-Monitor ────────────────────────────

/// Obergrenze gepollter Monitor-Geräte je Namespace (Missbrauchs-Schutz).
const MAX_MONITOR_DEVICES: usize = 128;

/// Aktuelle Unix-Zeit in Millisekunden.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Baut den URL-Basis-Pfad für `monitor.html`: der **Pfad-Teil** der
/// öffentlichen Relay-Basis plus der Namespace, z. B. `/bts-relay/<ns>/`.
/// Wichtig: der Relay läuft hinter nginx unter `/bts-relay/` – ohne
/// diesen Präfix zeigen die absoluten Asset-/State-URLs ins Leere.
fn monitor_base(public_base: &str, ns: &str) -> String {
    let after = public_base
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(public_base);
    let path = after.find('/').map(|i| &after[i..]).unwrap_or("");
    format!("{}/{}/", path.trim_end_matches('/'), ns)
}

/// Rendert `monitor.html` mit den Platzhaltern. `base` ist der
/// absolute URL-Basis-Pfad ([`monitor_base`]) – so lösen sich Flaggen,
/// Werbung und State-Abfrage korrekt auf.
fn render_monitor_html(mode: &str, base: &str, court_label: &str) -> String {
    MONITOR_HTML
        .replace("__MODE__", mode)
        .replace("__BASE__", base)
        .replace("__COURT_LABEL__", &html_escape(court_label))
}

/// Liefert die Court-Monitor-Anzeige fest für ein Feld
/// (`/court/{id}/display`, per CourtID).
async fn monitor_page(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let label = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.court_labels.get(&court_id).cloned())
            .unwrap_or_default()
    };
    let body = render_monitor_html("fixed", &monitor_base(&broker.public_base, &ns), &label);
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// Liefert die Court-Monitor-Anzeige im Geräte-Modus (`/{ns}/monitor`).
async fn monitor_device_page(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let body = render_monitor_html("device", &monitor_base(&broker.public_base, &ns), "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

/// Anzeige-Zustand eines fest verdrahteten Feldes (per CourtID), im
/// Sekundentakt gepollt.
async fn monitor_state(
    State(broker): State<Broker>,
    Path((ns, court_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let map = broker.namespaces.lock().await;
    let state = match map.get(&ns) {
        Some(namespace) => build_monitor_state(namespace, court_id),
        // Kein Host verbunden: leerer Zustand → neutrale Leerlauf-Seite.
        None => empty_monitor_state(court_id, String::new()),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

/// Query-Parameter der Geräte-Modus-Abfrage: die Geräte-ID.
#[derive(serde::Deserialize)]
struct DeviceQuery {
    device: String,
}

/// Anzeige-Zustand für ein Monitor-Gerät: löst die Feld-Zuweisung auf,
/// registriert den Poll und hängt einen offenen Fernbefehl an.
async fn monitor_device_state(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Query(q): Query<DeviceQuery>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    if q.device.is_empty() || q.device.len() > 64 {
        return (StatusCode::BAD_REQUEST, "Ungültige Geräte-ID").into_response();
    }
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        // Host nicht verbunden – das Gerät zeigt die Leerlauf-Seite.
        let state = empty_monitor_state(0, String::new());
        return ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response();
    };
    // Poll registrieren. Bei erreichter Obergrenze das am längsten nicht
    // gesehene Gerät verdrängen – so sperrt der Missbrauchs-Schutz keine
    // echten Geräte nach Geräte-Wechseln dauerhaft aus.
    if !namespace.monitor_seen.contains_key(&q.device)
        && namespace.monitor_seen.len() >= MAX_MONITOR_DEVICES
    {
        if let Some(oldest) = namespace
            .monitor_seen
            .iter()
            .min_by_key(|(_, &ts)| ts)
            .map(|(id, _)| id.clone())
        {
            namespace.monitor_seen.remove(&oldest);
        }
    }
    namespace.monitor_seen.insert(q.device.clone(), now_ms());
    let command = namespace.monitor_control.commands.get(&q.device).copied();
    // Volles Ziel bevorzugen; ein noch nicht aktualisierter Host liefert nur
    // `assignments` (CourtID) — die als Court-Ziel nachbilden.
    let target = namespace
        .monitor_control
        .targets
        .get(&q.device)
        .cloned()
        .or_else(|| {
            namespace
                .monitor_control
                .assignments
                .get(&q.device)
                .map(|&id| relay_proto::MonitorTarget::court(id))
        });
    let mut state = match target {
        Some(relay_proto::MonitorTarget::Court { court_id }) => {
            build_monitor_state(namespace, court_id)
        }
        // Nur auf Ziele umleiten, die der Relay auch WIRKLICH ausliefert —
        // Court-Übersicht, „In Vorbereitung" und „Werbung" (Rotation). Würden
        // wir pauschal `redirect_path()` nehmen, landete ein Sieger-/Kombi-
        // Monitor (oder Werbe-Einzelbild, das dateinamenbasiert ist und der
        // Relay per Index nicht auflöst) im Cloud auf einer 404-Seite ohne JS
        // und damit ohne Selbstheilung (schlimmer als die Kopplungs-Seite, die
        // weiterpollt).
        Some(
            t @ (relay_proto::MonitorTarget::InfoPreparation
            | relay_proto::MonitorTarget::AdRotation
            | relay_proto::MonitorTarget::InfoOverview { .. }),
        ) => {
            let mut s = unassigned_state(&q.device);
            s.unassigned = false;
            s.redirect_to = t.redirect_path();
            s
        }
        Some(_) | None => unassigned_state(&q.device),
    };
    state.command = command;
    state.device_code = device_code(&q.device);
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

/// Nimmt die Geräte-Steuerdaten (Feld-Zuweisungen + Fernbefehle) vom
/// bts-light-Host entgegen. Nur erlaubt, solange der Host verbunden ist.
///
/// Wie alle Namespace-Routen bewusst ohne eigenes Auth-Token: der
/// 128-Bit-UUID-Namespace ist das Zugangsmerkmal. Worst Case ist ein
/// erzwungenes „Neu laden"/„Identifizieren" eines bekannten Turniers –
/// die Befehle sind ein geschlossenes Enum, kein Code.
async fn monitor_control_upload(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Json(control): Json<MonitorControl>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace");
    }
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        return (StatusCode::NOT_FOUND, "bts-light ist nicht verbunden.");
    };
    namespace.monitor_control = control;
    (StatusCode::OK, "ok")
}

/// Liefert dem bts-light-Host die Liste der gemeldeten Monitor-Geräte.
async fn monitor_devices_list(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let map = broker.namespaces.lock().await;
    let devices: Vec<MonitorDeviceInfo> = match map.get(&ns) {
        Some(n) => {
            // Volle Ziele bevorzugen (Court, Info, Werbung, Kombi), damit die
            // Geräteliste im Tool das echte Ziel zeigt. Ein alter Host ohne
            // `targets` liefert nur `assignments` (CourtID) → als Court-Ziel
            // nachbilden, für die übrigen Geräte greift der `targets`-Eintrag.
            let mut assignments: std::collections::HashMap<String, relay_proto::MonitorTarget> = n
                .monitor_control
                .assignments
                .iter()
                .map(|(k, &v)| (k.clone(), relay_proto::MonitorTarget::court(v)))
                .collect();
            for (k, t) in &n.monitor_control.targets {
                assignments.insert(k.clone(), t.clone());
            }
            relay_proto::build_device_list(&assignments, &n.court_labels, &n.monitor_seen, now_ms())
        }
        None => Vec::new(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(devices)).into_response()
}

/// Query für den Ansage-Status der fernen Halle.
#[derive(serde::Deserialize)]
struct AnnounceStateQuery {
    #[serde(default)]
    hall: String,
    #[serde(default)]
    since: u64,
    /// Optionale Slave-ID – wenn gesetzt, registriert der Poll die Präsenz des
    /// Slaves (für die „ferne Halle online?"-Anzeige am Master).
    #[serde(default)]
    slave: String,
}

/// Liefert dem Cloud-Ansage-Slave die hallengefilterten Court-Matches (für die
/// Auto-Feld-Ansage) + neue Freitext-Ansagen (`id > since`). Leerer `hall` =
/// keine Hallen-Einschränkung. Registriert nebenbei die Slave-Präsenz.
async fn announce_state(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Query(q): Query<AnnounceStateQuery>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let mut map = broker.namespaces.lock().await;
    let state = match map.get_mut(&ns) {
        Some(n) => {
            // Präsenz des Slaves merken (rein informativ; Cap gegen Wildwuchs).
            if !q.slave.is_empty() {
                let id: String = q.slave.chars().take(64).collect();
                let hall: String = q.hall.chars().take(128).collect();
                let now = now_ms();
                // Veraltete Slaves (> 60 s ungesehen) entfernen → Slots werden
                // frei, der Cap blockiert keine echten Slaves nach Altlasten.
                n.slaves
                    .retain(|_, (_, last)| now.saturating_sub(*last) < 60_000);
                if n.slaves.len() < 64 || n.slaves.contains_key(&id) {
                    n.slaves.insert(id, (hall, now));
                }
            }
            let courts: Vec<relay_proto::AnnounceCourt> = n
                .court_matches
                .iter()
                .filter(|(cid, _)| {
                    let h = n.court_hall.get(cid).map(String::as_str).unwrap_or("");
                    q.hall.is_empty() || h.is_empty() || h == q.hall
                })
                .map(|(cid, m)| relay_proto::AnnounceCourt {
                    court_id: *cid,
                    label: n.court_labels.get(cid).cloned().unwrap_or_default(),
                    match_brief: Some(m.clone()),
                })
                .collect();
            let freetext: Vec<relay_proto::FreetextItem> = n
                .freetext
                .iter()
                .filter(|f| {
                    f.id > q.since && (f.hall.is_empty() || q.hall.is_empty() || f.hall == q.hall)
                })
                .cloned()
                .collect();
            // Aufgerufene Spiele der Halle (Slave-Spielübersicht + Nachruf,
            // Cluster C Stufe 2) — gleiche Hallenfilter-Regel wie bei courts.
            let prepared: Vec<relay_proto::PreparedMatch> = n
                .prepared
                .iter()
                .filter(|p| q.hall.is_empty() || p.hall.is_empty() || p.hall == q.hall)
                .cloned()
                .collect();
            relay_proto::AnnounceState {
                courts,
                freetext,
                prepared,
                // Geerbte Azure-Config (ADR 0003) — gleiche Vertrauensstufe
                // wie der übrige Namespace-Inhalt (Bearer = install_id).
                azure_tts: n.azure_tts.clone(),
            }
        }
        None => relay_proto::AnnounceState::default(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(state)).into_response()
}

/// Stellt einen kurzlebigen Telefon-Kopplungscode für den Namespace aus
/// (ADR 0004). Nur für Namespaces mit **verbundenem Host** — sonst könnte
/// jeder beliebige (noch unbenutzte) Namespaces mit Codes belegen. Genau
/// ein aktiver Code je Namespace: ein neuer ersetzt den alten.
async fn pairing_code_create(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let host_connected = broker
        .namespaces
        .lock()
        .await
        .get(&ns)
        .is_some_and(|n| n.host.is_some());
    if !host_connected {
        return (
            StatusCode::CONFLICT,
            "Kein verbundener Host für diesen Namespace",
        )
            .into_response();
    }
    let now = now_ms();
    let mut pairings = broker.pairings.lock().await;
    // Abgelaufene Codes und den bisherigen Code dieses Namespace räumen.
    pairings.retain(|_, e| e.expires_ms > now && e.namespace != ns);
    let code = loop {
        match gen_pairing_code() {
            Ok(c) if !pairings.contains_key(&c) => break c,
            Ok(_) => continue, // Kollision (praktisch nie) → neu würfeln
            Err(e) => {
                tracing::warn!("Pairing-Code-Erzeugung fehlgeschlagen: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Zufall nicht verfügbar")
                    .into_response();
            }
        }
    };
    pairings.insert(
        code.clone(),
        PairingEntry {
            namespace: ns,
            expires_ms: now + PAIRING_TTL_MS,
        },
    );
    Json(relay_proto::PairingCode {
        code,
        expires_in_s: PAIRING_TTL_MS / 1000,
    })
    .into_response()
}

/// Löst einen Telefon-Kopplungscode zum vollen Namespace auf (ADR 0004).
/// Fehlversuchs-Limit VOR dem Lookup: Ist das Fenster ausgeschöpft, wird
/// auch ein zufällig richtiger Code nicht mehr beantwortet (429) — sonst
/// wäre das Limit fürs Durchprobieren wirkungslos.
async fn pairing_resolve(
    State(broker): State<Broker>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    if !valid_pairing_code(&code) {
        return (StatusCode::NOT_FOUND, "Ungültiger Code").into_response();
    }
    let now = now_ms();
    {
        // JEDEN Versuch atomar in EINEM Lock zählen (auch erfolgreiche):
        // Prüfen und Erhöhen getrennt wäre ein TOCTOU-Fenster, in dem
        // parallele Requests das Limit überschießen (Review-Befund).
        // Legitime Kopplungen liegen um Größenordnungen unter dem Limit.
        let mut fails = broker.pair_fails.lock().await;
        if now.saturating_sub(fails.0) > PAIR_FAIL_WINDOW_MS {
            *fails = (now, 0);
        }
        fails.1 += 1;
        if fails.1 > PAIR_FAIL_LIMIT {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Zu viele Fehlversuche – kurz warten",
            )
                .into_response();
        }
    }
    {
        let mut pairings = broker.pairings.lock().await;
        pairings.retain(|_, e| e.expires_ms > now);
        if let Some(e) = pairings.get(&code) {
            return Json(relay_proto::PairingResolved {
                namespace: e.namespace.clone(),
            })
            .into_response();
        }
    }
    (StatusCode::NOT_FOUND, "Code unbekannt oder abgelaufen").into_response()
}

/// Slaves gelten als online, wenn ihr letzter Poll < 12 s her ist (4 verpasste
/// 3-s-Polls Toleranz).
const SLAVE_ONLINE_MS: u64 = 12_000;

/// Liefert dem Master die bekannten Cloud-Ansage-Slaves seines Namespaces samt
/// Online-Status – für die „ferne Halle online?"-Anzeige in der Kopfzeile.
async fn slaves_list(State(broker): State<Broker>, Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let now = now_ms();
    let map = broker.namespaces.lock().await;
    let slaves: Vec<relay_proto::SlaveInfo> = match map.get(&ns) {
        Some(n) => n
            .slaves
            .iter()
            .map(|(id, (hall, last))| relay_proto::SlaveInfo {
                id: id.clone(),
                hall: hall.clone(),
                online: now.saturating_sub(*last) < SLAVE_ONLINE_MS,
                last_seen_ms: *last,
            })
            .collect(),
        None => Vec::new(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(slaves)).into_response()
}

/// Leerer Monitor-Zustand (kein Match, keine Werbung) – Leerlauf-Anzeige.
fn empty_monitor_state(court_id: i64, court_label: String) -> MonitorState {
    MonitorState {
        court_id,
        court_label,
        tournament_name: String::new(),
        match_info: None,
        court_state: None,
        config: MonitorConfig::default(),
        ads: Vec::new(),
        command: None,
        device_code: String::new(),
        unassigned: false,
        redirect_to: None,
        server_now_ms: now_ms(),
        // Aufruf-Timer am Monitor: im Cloud-Pfad noch nicht durchgereicht
        // (Host→Relay-Push fehlt) → Default (aus). LAN-Pfad zeigt ihn bereits.
        on_court_since_ms: None,
        call_timer: relay_proto::CallTimerView::default(),
    }
}

/// Zustand für ein noch keinem Feld zugewiesenes Gerät (Kopplungs-Seite).
fn unassigned_state(device_id: &str) -> MonitorState {
    MonitorState {
        unassigned: true,
        device_code: device_code(device_id),
        ..empty_monitor_state(0, String::new())
    }
}

/// Baut den Monitor-Anzeige-Zustand aus dem gespeicherten Namespace-Stand
/// (für ein Feld per CourtID).
fn build_monitor_state(namespace: &Namespace, court_id: i64) -> MonitorState {
    let monitor = namespace.monitor.as_ref();
    let match_info = namespace
        .court_matches
        .get(&court_id)
        .map(|mb| MonitorMatch {
            match_id: mb.match_id,
            discipline: mb.discipline.clone(),
            event_label: mb.event_label.clone(),
            match_number: mb.match_number,
            team1: mb.team_a.iter().map(monitor_player).collect(),
            team2: mb.team_b.iter().map(monitor_player).collect(),
            sets: namespace
                .court_scores
                .get(&court_id)
                .cloned()
                .unwrap_or_default(),
        });
    MonitorState {
        court_id,
        court_label: namespace
            .court_labels
            .get(&court_id)
            .cloned()
            .unwrap_or_default(),
        tournament_name: monitor
            .map(|m| m.tournament_name.clone())
            .unwrap_or_default(),
        match_info,
        court_state: namespace.court_state.get(&court_id).cloned(),
        config: monitor.map(|m| m.config.clone()).unwrap_or_default(),
        ads: monitor
            .map(|m| (0..m.ads.len()).map(|i| i.to_string()).collect())
            .unwrap_or_default(),
        command: None,
        device_code: String::new(),
        unassigned: false,
        redirect_to: None,
        server_now_ms: now_ms(),
        // 1.-Aufruf-Zeitpunkt (relay-seitig gestempelt) + Aufruf-Timer-Schwellen
        // aus dem Host-Upload → der Cloud-Monitor zeigt dieselbe Aufruf-Uhr.
        on_court_since_ms: namespace.court_on_court_since.get(&court_id).copied(),
        call_timer: monitor.map(|m| m.call_timer.clone()).unwrap_or_default(),
    }
}

fn monitor_player(p: &PlayerBrief) -> MonitorPlayer {
    MonitorPlayer {
        name: p.name.clone(),
        // `PlayerBrief` führt nur den kombinierten Namen – Vor-/Nachname
        // bleiben leer, der Court-Monitor zerlegt dann `name` selbst.
        given: String::new(),
        family: String::new(),
        nationality: p.nationality.clone(),
    }
}

/// Schlägt eine gebündelte Flaggen-Datei nach.
///
/// Pure Funktion, von **beiden** Flaggen-Routen genutzt (mit und ohne
/// Namespace). Der Dateiname kommt aus dem Anfrage-Pfad und darf das
/// Bündel nie verlassen — deshalb die Traversal-Abwehr hier, nicht in den
/// Routen.
fn flag_lookup(file: &str) -> Option<&'static [u8]> {
    if file.is_empty() || file.contains(['/', '\\']) || file.contains("..") {
        return None;
    }
    FLAGS.get_file(file).map(|f| f.contents())
}

/// Antwortform der Flaggen-Routen aus dem Nachschlage-Ergebnis.
fn flag_response(inhalt: Option<&'static [u8]>) -> axum::response::Response {
    match inhalt {
        Some(svg) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            svg,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Flagge nicht gefunden").into_response(),
    }
}

/// Liefert eine gebündelte SVG-Länderflagge (`/{ns}/flags/GER.svg`).
async fn flag_route(Path((ns, file)): Path<(String, String)>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    flag_response(flag_lookup(&file))
}

/// Liefert eine Länderflagge **ohne** Namespace (`/flags/GER.svg`).
///
/// Für die Turnierleitungs-Seite: Sie läuft bewusst ohne Namespace in der
/// Adresse (ADR 0012) und leitet die Flaggen-Basis aus ihrem eigenen Pfad
/// ab — `/bts-relay/tl` → `/bts-relay/flags/`. Ohne diese Route lief dort
/// jede Flagge in ein 404, die Seite zeigte nur Kürzel, und der
/// `onerror`-Tausch ließ die Listen bei jedem Poll-Neuaufbau sichtbar
/// springen. Die Flaggen sind statische Länder-SVGs ohne Turnierbezug —
/// ein Namespace hätte hier nichts abzusichern.
async fn flag_route_global(Path(file): Path<String>) -> impl IntoResponse {
    flag_response(flag_lookup(&file))
}

/// Liefert ein hochgeladenes Werbebild eines Namespace (per Index).
async fn ad_image(
    State(broker): State<Broker>,
    Path((ns, idx)): Path<(String, String)>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let Ok(i) = idx.parse::<usize>() else {
        return (StatusCode::NOT_FOUND, "Nicht gefunden").into_response();
    };
    // Bytes unter dem Lock herauskopieren, dann den Lock fallen lassen –
    // ein mehrere MB großes memcpy darf nicht den Namespace-Mutex halten.
    let ad = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.monitor.as_ref())
            .and_then(|m| m.ads.get(i))
            .map(|ad| (ad.content_type.clone(), ad.bytes.clone()))
    };
    match ad {
        Some((content_type, bytes)) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Werbebild nicht gefunden").into_response(),
    }
}

/// Liefert das hochgeladene **Turnierlogo** eines Namespace (Sponsor-Leiste im
/// Cloud-Modus). Kein Logo → 404, damit die Seite per `onerror` sauber
/// degradiert. Gegenstück zum LAN-`/info/logo`; die Cloud-Anzeigeseiten rufen
/// beide über denselben relativen Pfad ab.
async fn tournament_logo(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let logo = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .and_then(|n| n.monitor.as_ref())
            .and_then(|m| m.logo.as_ref())
            .map(|l| (l.content_type.clone(), l.bytes.clone()))
    };
    match logo {
        Some((content_type, bytes)) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "public, max-age=300".to_string()),
            ],
            bytes,
        )
            .into_response(),
        None => (
            [(header::CACHE_CONTROL, "public, max-age=60".to_string())],
            StatusCode::NOT_FOUND,
        )
            .into_response(),
    }
}

/// Zustand für die Sponsor-Leiste der Cloud-Anzeigeseiten: welche Werbebild-
/// **Indizes** in die Leiste gehören (`barAds`) und ob ein Logo vorliegt
/// (`hasLogo`). Gegenstück zum LAN-`/info/ad/state` (dort Dateinamen, hier
/// Indizes – genau wie `MonitorState.ads`). `intervalS` für die Vollständigkeit.
async fn ad_bar_state(State(broker): State<Broker>, Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let (ads, bar_ads, has_logo, interval_s) = {
        let map = broker.namespaces.lock().await;
        match map.get(&ns).and_then(|n| n.monitor.as_ref()) {
            Some(m) => {
                // Cloud adressiert die Werbebilder per Index (`/{ns}/ads/0`);
                // `ads` ist die volle Rotationsliste (für ad.html), `barAds`
                // nur die als „in Leiste" markierten (für die Sponsor-Leiste).
                let all: Vec<String> = (0..m.ads.len()).map(|i| i.to_string()).collect();
                let bar: Vec<String> = m
                    .ads
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.in_bar)
                    .map(|(i, _)| i.to_string())
                    .collect();
                (all, bar, m.logo.is_some(), m.config.ad_interval_s.max(1))
            }
            None => (Vec::new(), Vec::new(), false, 1),
        }
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({
            "ads": ads,
            "barAds": bar_ads,
            "hasLogo": has_logo,
            "intervalS": interval_s,
        })),
    )
        .into_response()
}

/// Die Vollbild-Werbe-Anzeige (Rotation) im Cloud-Modus (HTML). Dieselbe Datei
/// wie im LAN; sie leitet ihren Basis-Pfad aus der eigenen URL ab, läuft also
/// unter `/{ns}/info/ad`.
async fn ad_page(Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    ([(header::CACHE_CONTROL, "no-store")], Html(AD_HTML)).into_response()
}

/// Die Court-Übersicht im Cloud-Modus (HTML). Dieselbe Datei wie im LAN; sie
/// holt ihre Daten über `<BASE>health`, hier `/{ns}/health`.
async fn overview_page(Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    ([(header::CACHE_CONTROL, "no-store")], Html(OVERVIEW_HTML)).into_response()
}

/// Datenquelle der Cloud-Court-Übersicht (`overview.html` pollt `<BASE>health`).
/// Baut je Feld die Anzeige-Form aus dem, was der Host schon zum Relay pusht:
/// Feldliste (`courts`), aktuelles Match (`court_matches`), Satzstand
/// (`court_scores`), 1.-Aufruf-Zeit (`court_on_court_since`) und die
/// Aufruf-Timer-Schwellen. Aufschlag/Verletzung/Turnierleitungs-Ruf stehen im
/// Cloud (noch) nicht zur Verfügung — sie werden konservativ weggelassen; die
/// Seite degradiert sauber (kein Aufschlag-Highlight, keine Badges), der Kern
/// (Feld × Spiel × Satzstand × Aufruf-Uhr) ist vollständig.
async fn overview_health(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let (courts, call_timer) = {
        let map = broker.namespaces.lock().await;
        match map.get(&ns) {
            Some(n) => {
                let names = |team: &[relay_proto::PlayerBrief]| {
                    team.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
                };
                // Nationalitäten parallel zu den Namen (leerer String = unbekannt)
                // — die Länderflaggen der Übersicht. Der Host pusht `nationality`
                // nur, wenn das (default-aus) Anzeige-Feld eingeschaltet ist; ist
                // es leer, blendet overview.html die Flagge stumm aus.
                let nats = |team: &[relay_proto::PlayerBrief]| {
                    team.iter()
                        .map(|p| p.nationality.clone().unwrap_or_default())
                        .collect::<Vec<_>>()
                };
                let courts: Vec<serde_json::Value> = n
                    .courts
                    .iter()
                    .map(|c| {
                        let m = n.court_matches.get(&c.id);
                        // Satzstand als `Vec<SetAb>` → JSON `[{"a":…,"b":…}]`. Die
                        // LAN-`/health` liefert `[[a,b]]`; overview.html `setVal()`
                        // akzeptiert beide Formen, daher unkritisch.
                        let sets = n.court_scores.get(&c.id).cloned().unwrap_or_default();
                        serde_json::json!({
                            "court_id": c.id,
                            "court": c.label,
                            "location": c.hall,
                            "match_id": m.map(|m| m.match_id).unwrap_or(0),
                            "match_name": m.map(|m| m.event_label.clone()).unwrap_or_default(),
                            "team1": m.map(|m| names(&m.team_a)).unwrap_or_default(),
                            "team2": m.map(|m| names(&m.team_b)).unwrap_or_default(),
                            "team1_nationalities": m.map(|m| nats(&m.team_a)).unwrap_or_default(),
                            "team2_nationalities": m.map(|m| nats(&m.team_b)).unwrap_or_default(),
                            "sets": sets,
                            "on_court_since_ms": n.court_on_court_since.get(&c.id).copied(),
                            // Aufschlag/Verletzung/TL-Ruf hält der Relay nicht → im
                            // Cloud konservativ weggelassen (kein Highlight/Badge).
                            "serving_team": serde_json::Value::Null,
                            "injury": false,
                            "official_call": false,
                        })
                    })
                    .collect();
                let ct = n
                    .monitor
                    .as_ref()
                    .map(|mo| mo.call_timer.clone())
                    .unwrap_or_default();
                (courts, ct)
            }
            None => (Vec::new(), relay_proto::CallTimerView::default()),
        }
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({
            "courts": courts,
            "serverNowMs": now_ms(),
            "callTimer": call_timer,
        })),
    )
        .into_response()
}

/// Der „In Vorbereitung"-Info-Monitor im Cloud-Modus (HTML). Dieselbe Datei wie
/// im LAN; sie leitet ihren Basis-Pfad aus der eigenen URL ab, läuft also unter
/// `/{ns}/info/preparation` ohne Templating.
async fn preparation_page(Path(ns): Path<String>) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(PREPARATION_HTML),
    )
        .into_response()
}

/// Zustand des Vorbereitungs-Monitors: die vom Host gepushten aufgerufenen
/// Spiele (`prepared`) in der Kandidaten-Form, die `preparation.html` erwartet.
/// Alle Einträge sind aufgerufen (`call` gesetzt) — der Cloud-Monitor zeigt
/// damit genau die „In Vorbereitung"-Liste (ohne die reinen Zeitplan-Einträge,
/// die nur der LAN-Server aus dem vollen BTP-Snapshot kennt).
async fn preparation_state(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace").into_response();
    }
    let candidates: Vec<serde_json::Value> = {
        let map = broker.namespaces.lock().await;
        map.get(&ns)
            .map(|n| {
                n.prepared
                    .iter()
                    .map(|pm| {
                        // PreparedMatch trägt kein `draw_name` → Label aus Klasse
                        // + Runde (kosmetisch; die Namen sind der Kern).
                        let label = format!("{} {}", pm.class_label, pm.round_name)
                            .trim()
                            .to_string();
                        serde_json::json!({
                            "match_num": pm.match_number,
                            "label": label,
                            "team1": pm.team_a.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                            "team2": pm.team_b.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                            "call": { "hall": pm.hall, "called_at_ms": pm.called_at_ms },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "candidates": candidates })),
    )
        .into_response()
}

/// Nimmt den Court-Monitor-Datensatz (Konfiguration + Werbebilder) vom
/// bts-light-Host entgegen. Nur erlaubt, solange der Host verbunden ist –
/// das verhindert das Anlegen von Namespaces ohne Host.
///
/// Bewusst ohne eigenes Auth-Token: Wer den 128-Bit-UUID-Namespace kennt,
/// darf hochladen – dasselbe Vertrauensmodell wie für die übrigen
/// Namespace-Routen. Worst Case ist das Überschreiben der Werbebilder
/// eines bekannten Turniers; kein Code, keine Ergebnis-Schreibrechte.
async fn monitor_upload(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Json(upload): Json<MonitorUpload>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return (StatusCode::NOT_FOUND, "Unbekannter Namespace");
    }
    let mut ads = Vec::new();
    let mut total = 0usize;
    for ad in upload.ads.into_iter().take(MAX_ADS) {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(ad.data.as_bytes()) else {
            continue;
        };
        total += bytes.len();
        if total > MAX_ADS_TOTAL {
            break;
        }
        ads.push(AdImage {
            content_type: sanitize_content_type(&ad.content_type),
            bytes,
            in_bar: ad.in_bar,
        });
    }
    // Turnierlogo (falls mitgeschickt) – MIME wie bei den Ads gewhitelistet,
    // Größe gegen den eigenen knappen `MAX_LOGO_BYTES`-Cap; ein kaputtes Base64
    // oder ein zu großes Logo verwirft nur das Logo, nicht den Upload.
    let logo = upload.logo.and_then(|l| {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(l.data.as_bytes())
            .ok()?;
        if bytes.is_empty() || bytes.len() > MAX_LOGO_BYTES {
            return None;
        }
        Some(AdImage {
            content_type: sanitize_content_type(&l.content_type),
            bytes,
            in_bar: false,
        })
    });
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        return (StatusCode::NOT_FOUND, "bts-light ist nicht verbunden.");
    };
    namespace.monitor = Some(MonitorBundle {
        config: upload.config,
        tournament_name: upload.tournament_name,
        ads,
        call_timer: upload.call_timer,
        logo,
    });
    tracing::info!("Namespace '{ns}': Court-Monitor-Datensatz aktualisiert");
    (StatusCode::OK, "ok")
}

/// Lässt nur die erwarteten Bild-MIME-Typen durch (der Header kommt vom
/// Host und landet ungeprüft im Content-Type der Auslieferung).
fn sanitize_content_type(ct: &str) -> String {
    match ct {
        "image/png" | "image/webp" | "image/gif" | "image/jpeg" => ct.to_string(),
        _ => "image/jpeg".to_string(),
    }
}

/// Nimmt das Endergebnis vom Tablet entgegen, leitet es an den Host weiter
/// und wartet auf dessen `ResultAck` (BTP-Schreiben passiert lokal bei
/// bts-light).
async fn result(
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Json(body): Json<ResultBody>,
) -> Json<ResultResponse> {
    if !valid_namespace(&ns) {
        return Json(ResultResponse::err("Unbekannter Namespace."));
    }
    let (ack_tx, ack_rx) = oneshot::channel();
    let req_id;
    {
        let mut map = broker.namespaces.lock().await;
        let Some(namespace) = map.get_mut(&ns) else {
            return Json(ResultResponse::err(
                "bts-light ist nicht mit dem Relay verbunden.",
            ));
        };
        let Some(host) = namespace.host.clone() else {
            return Json(ResultResponse::err(
                "bts-light ist nicht mit dem Relay verbunden.",
            ));
        };
        // Schutz gegen geflutete Ergebnis-Übermittlungen: jede hält bis zu
        // RESULT_TIMEOUT lang einen pending-Eintrag offen.
        if namespace.pending.len() >= MAX_PENDING_PER_NS {
            return Json(ResultResponse::err(
                "Zu viele offene Übermittlungen – bitte kurz warten.",
            ));
        }
        req_id = namespace.next_req;
        namespace.next_req += 1;
        namespace.pending.insert(req_id, ack_tx);
        let frame = RelayFrame::Result {
            req_id,
            court_id: body.court_id,
            court_label: body.court_label.clone(),
            match_id: body.match_id,
            sets: body.sets.clone(),
            retired: body.retired,
            walkover: body.walkover,
            winner: body.winner,
            cascade_walkover: body.cascade_walkover,
        };
        if host.send(text(&frame)).is_err() {
            namespace.pending.remove(&req_id);
            return Json(ResultResponse::err("bts-light ist nicht erreichbar."));
        }
    }
    match tokio::time::timeout(RESULT_TIMEOUT, ack_rx).await {
        Ok(Ok(resp)) => Json(resp),
        _ => {
            let mut map = broker.namespaces.lock().await;
            if let Some(namespace) = map.get_mut(&ns) {
                namespace.pending.remove(&req_id);
            }
            Json(ResultResponse::err(
                "Zeitüberschreitung – bts-light hat nicht geantwortet.",
            ))
        }
    }
}

// ─────────────────────────────── Tablet-WS ────────────────────────────────

async fn tablet_ws(
    ws: WebSocketUpgrade,
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.on_upgrade(move |socket| tablet_conn(socket, broker, ns))
        .into_response()
}

/// Eine Tablet-Verbindung: meldet sich für ein Feld (per CourtID) an,
/// leitet Score-Updates an den Host weiter, empfängt Match-Zuweisungen.
async fn tablet_conn(mut socket: WebSocket, broker: Broker, ns: String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    // Feld-Identität dieser Verbindung: die CourtID, sobald `identify` kam.
    let mut court: Option<i64> = None;
    // Schiedst dieses Tablet das Feld aktiv? Passive Tablets warten auf
    // „Übernehmen"; ihre Score-/Alert-Frames werden nicht weitergeleitet.
    let mut active = false;
    // Persistente Geräte-Kennung (aus identify/take_over) — leer bei
    // alten Tablet-Seiten. Für die Reconnect-Erkennung je Feld.
    let mut my_device = String::new();
    // Enger Ping-Takt + Empfangs-Stale wie bei `host_conn`: Der Relay pingt
    // aktiv, der Browser auto-pongt auf Protokoll-Ebene. Bleibt jedes
    // Lebenszeichen (Frame/Pong) länger als TABLET_STALE aus, beendet sich
    // die Verbindung selbst und gibt den Court-Slot frei (Hebel D).
    let mut ping = tokio::time::interval(TABLET_PING);
    let mut last_incoming = tokio::time::Instant::now();

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                // VOR dem inneren `match` stempeln: gilt für Text/Pong/Close/
                // alle — ein `Message::Pong` (fällt in `_ => {}`) frischt den
                // Stempel damit ebenso auf, ganz ohne eigenen Pong-Arm.
                last_incoming = tokio::time::Instant::now();
                match msg {
                    Message::Text(t) => {
                        match serde_json::from_str::<TabletMsg>(t.as_str()) {
                            Ok(TabletMsg::Identify { court_id, device_id, .. }) => {
                                my_device = device_id;
                                match attach_tablet(&broker, &ns, court_id, &my_device, &tx).await {
                                    AttachResult::Active => {
                                        tracing::info!("Tablet verbunden: Namespace '{ns}', Feld {court_id}");
                                        active = true;
                                        court = Some(court_id);
                                    }
                                    AttachResult::Occupied => {
                                        tracing::info!("Feld {court_id} belegt – Tablet wartet auf Übernahme");
                                        let _ = tx.send(text(&ServerMsg::CourtOccupied));
                                        court = Some(court_id);
                                    }
                                    AttachResult::Rejected => {
                                        let _ = socket.send(Message::Close(None)).await;
                                        break;
                                    }
                                }
                            }
                            Ok(TabletMsg::TakeOver { device_id }) => {
                                if let (Some(c), false) = (court, active) {
                                    if !device_id.is_empty() {
                                        my_device = device_id;
                                    }
                                    take_over_court(&broker, &ns, c, &my_device, &tx).await;
                                    active = true;
                                    tracing::info!("Tablet übernimmt Feld {c} (Namespace '{ns}')");
                                }
                            }
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history, match_id }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_score(&broker, &ns, c, score_a, score_b, sets_history, match_id, &tx).await;
                                }
                            }
                            // Punktverlauf (ADR 0014): 1:1 an den Host
                            // durchreichen — Briefträger, nur Halter-,
                            // Stale- und Größen-Prüfung, keine Deutung.
                            Ok(TabletMsg::Rally { match_id, set, n, winner, score_a, score_b }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_rally(&broker, &ns, c, match_id, set, n, winner, score_a, score_b, &tx).await;
                                }
                            }
                            Ok(TabletMsg::RallySync { match_id, timeline }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_rally_sync(&broker, &ns, c, match_id, timeline, &tx).await;
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_battery(&broker, &ns, c, percent, charging).await;
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), true) = (court, active) {
                                    forward_alert(&broker, &ns, c, injury, official, &tx).await;
                                }
                            }
                            Ok(TabletMsg::StateSync { state }) => {
                                if let (Some(c), true) = (court, active) {
                                    store_court_state(&broker, &ns, c, state, &tx).await;
                                }
                            }
                            Ok(TabletMsg::Ping) => {
                                // Lebenszeichen des Tablets über die Cloud →
                                // sofort Pong zurück, ohne bts-light zu behelligen.
                                let _ = tx.send(text(&ServerMsg::Pong));
                            }
                            Err(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(m) => { if socket.send(m).await.is_err() { break } }
                    None => break,
                }
            }
            _ = ping.tick() => {
                if is_stale(last_incoming, tokio::time::Instant::now(), TABLET_STALE) {
                    tracing::warn!(
                        "Tablet (Namespace '{ns}') seit {}s stumm – Verbindung als tot verworfen",
                        last_incoming.elapsed().as_secs()
                    );
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }

    // Nur das aktive Tablet räumt seinen Court-Eintrag ab.
    if let (Some(c), true) = (court, active) {
        detach_tablet(&broker, &ns, c, &tx).await;
        tracing::info!("Tablet getrennt: Namespace '{ns}', Feld {c}");
    }
}

// ─────────────────────── Court-Monitor-Nudge (A1) ─────────────────────────

/// Query der Monitor-Nudge-WS: optionale CourtID. Fehlt sie, abonniert der
/// Client Nudges ALLER Felder (Feld-Übersicht `overview.html`); ist sie
/// gesetzt, nur die dieses Felds (Court-Monitor `monitor.html`).
#[derive(serde::Deserialize)]
struct MonitorWsQuery {
    court: Option<i64>,
}

/// Upgrade der Court-Monitor-Nudge-WS (A1, ADR 0016). `valid_namespace`-Guard
/// wie die übrigen Namespace-Routen; kein `identify`-Handshake nötig (die
/// Anzeige liest nur, der Court steht im Query).
async fn monitor_ws(
    ws: WebSocketUpgrade,
    State(broker): State<Broker>,
    Path(ns): Path<String>,
    Query(q): Query<MonitorWsQuery>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let court = q.court;
    ws.on_upgrade(move |socket| monitor_conn(socket, broker, ns, court))
        .into_response()
}

/// Eine Court-Monitor-Nudge-Verbindung (A1, ADR 0016). Analog zum LAN-Server:
/// nur winzige „Feld X geändert, seq N"-Signale; die Anzeige holt den
/// Vollstand über ihre bestehende Poll-Route (eine Datenquelle, kein
/// Flackern). Der Sender liegt ausschließlich im eigenen Namespace →
/// Namespace-Isolation strikt.
///
/// TODO(A1): Match-Zuweisung/-Räumung stößt der Relay noch nicht an — der
/// Cloud-Score-Weg (`forward_score`) ist der Muss; die ~250-ms-Poll deckt die
/// Zuweisungs-Latenz ab (Score-Cache-Räumung folgt dem Host-Frame, nicht
/// einem lokalen State-Aufruf).
async fn monitor_conn(mut socket: WebSocket, broker: Broker, ns: String, court: Option<i64>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    subscribe_monitor(&broker, &ns, court, &tx).await;
    let mut ping = tokio::time::interval(HEARTBEAT);
    loop {
        tokio::select! {
            outgoing = rx.recv() => {
                match outgoing {
                    Some(m) => { if socket.send(m).await.is_err() { break } }
                    None => break,
                }
            }
            incoming = socket.recv() => {
                // Die Anzeige sendet nichts Fachliches; wir lesen nur, um
                // Close/Fehler (tote Verbindung) zu bemerken.
                match incoming {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }
    unsubscribe_monitor(&broker, &ns, court, &tx).await;
}

/// Trägt eine Monitor-Verbindung als Nudge-Abonnent ein (A1). Legt den
/// Namespace **nicht** an, falls er fehlt: Ohne Host gibt es nichts zu
/// melden, und ein von Zuschauer-TVs erzeugter Namespace hätte keinen
/// Aufräum-Pfad (`is_empty` zählt Monitore bewusst nicht mit). Fehlt der
/// Namespace, bleibt die Verbindung still (Poll-Fallback) und der Client
/// verbindet sich neu, sobald der Host da ist.
async fn subscribe_monitor(broker: &Broker, ns: &str, court: Option<i64>, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    // Fan-out-Deckel: über der Grenze nicht eintragen (Verbindung degradiert
    // still auf Poll). Schützt Speicher + Broadcast-Kosten je Namespace.
    let total = namespace.monitor_subs.values().map(Vec::len).sum::<usize>()
        + namespace.monitor_subs_all.len();
    if total >= MAX_MONITOR_SUBS {
        return;
    }
    match court {
        Some(c) => namespace
            .monitor_subs
            .entry(c)
            .or_default()
            .push(tx.clone()),
        None => namespace.monitor_subs_all.push(tx.clone()),
    }
}

/// Trägt eine Monitor-Verbindung wieder aus (Verbindungsende). Vergleicht per
/// `same_channel`, damit nur der eigene Sender verschwindet.
async fn unsubscribe_monitor(broker: &Broker, ns: &str, court: Option<i64>, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    match court {
        Some(c) => {
            if let Some(list) = namespace.monitor_subs.get_mut(&c) {
                list.retain(|t| !t.same_channel(tx));
                if list.is_empty() {
                    namespace.monitor_subs.remove(&c);
                }
            }
        }
        None => namespace.monitor_subs_all.retain(|t| !t.same_channel(tx)),
    }
    if namespace.is_empty() {
        map.remove(ns);
    }
}

/// Weckt die Monitor-Abonnenten eines Felds (A1, ADR 0016): erhöht die
/// pro-Court-Sequenz und schickt den winzigen Nudge an die Abonnenten GENAU
/// dieses Felds UND an die „alle Felder"-Abonnenten (Feld-Übersicht). Tote
/// Sender (Anzeige weg) werden ausgesiebt. Namespace-lokal — der Aufrufer
/// hält bereits den Namespace, ein Nudge verlässt ihn nie.
fn notify_monitor(namespace: &mut Namespace, court_id: i64) {
    let seq = {
        let s = namespace.monitor_seq.entry(court_id).or_insert(0);
        *s += 1;
        *s
    };
    let nudge = text(&MonitorNudge {
        court: court_id,
        seq,
    });
    if let Some(list) = namespace.monitor_subs.get_mut(&court_id) {
        list.retain(|t| t.send(nudge.clone()).is_ok());
        if list.is_empty() {
            namespace.monitor_subs.remove(&court_id);
        }
    }
    namespace
        .monitor_subs_all
        .retain(|t| t.send(nudge.clone()).is_ok());
}

/// Ergebnis eines Tablet-Verbindungsversuchs an einem Feld.
enum AttachResult {
    /// Das Tablet schiedst dieses Feld nun aktiv.
    Active,
    /// Das Feld ist belegt – das Tablet bleibt passiv (Übernahme möglich).
    Occupied,
    /// Abgewiesen, weil ein Limit erreicht ist.
    Rejected,
}

/// Liefert den bekannten Feldnamen (Anzeige) eines Felds im Namespace –
/// leer, solange der Host noch kein Frame für dieses Feld geschickt hat.
fn label_of(namespace: &Namespace, court_id: i64) -> String {
    namespace
        .court_labels
        .get(&court_id)
        .cloned()
        .unwrap_or_default()
}

/// Cloud-Pendant zur reinen `reconnect_decision` (state.rs, A2 / ADR 0017):
/// bestimmt, ob ein (re)verbindendes Tablet der AUTORITÄTS-Halter ist und
/// seinen lokalen Stand durchsetzen darf (`true`) oder den mitgeschickten
/// `state` adoptiert (`false`). Der Relay kennt den Slot-Halter über
/// `tablet_devices`.
///
/// `owner_device` = aktuell eingetragener Halter VOR diesem (Re-)Attach
/// (`None` = Feld frei). `owner_scored` = hat dieser Halter seit seiner
/// Übernahme gezählt.
///
/// Regeln identisch zu `reconnect_decision`:
/// - `finalized` → `false` (Hand-Ergebnis nicht überbügeln).
/// - Feld frei ODER Halter == Rückkehrer → `true` (Reclaimer ist die Wahrheit).
/// - Fremder Halter UND `owner_scored` → `false` (legitimer Übernehmer gewinnt).
/// - Fremder Halter OHNE Score → `true` (Rückkehrer gewinnt deterministisch).
///
/// KONSERVATIV (Cloud-Besonderheit): Der Relay hat KEIN per-Claim-Flag wie der
/// Host. Er leitet `owner_scored` aus `court_scores` ab (liegt ein nicht-leerer
/// Live-Stand am Feld, gilt der Halter als „hat gezählt"). Im Zweifel wird
/// `owner_scored = true` gewählt — dann tritt der Rückkehrer zurück und ein
/// legitimer Übernehmer wird NIE überschrieben (das Ziel der Konfliktregel;
/// der bewusste stille Verlierer ist der Rückkehrer, nicht der Übernehmer).
fn relay_reconnect_authoritative(
    returning_device: &str,
    owner_device: Option<&str>,
    owner_scored: bool,
    finalized: bool,
) -> bool {
    // Regel b (A2 / ADR 0017): `finalized` reist vom Host im MatchBrief zum
    // Relay (BTP-Wahrheit, in `court_matches` gespeichert); ist das Match
    // finalisiert, überbügelt kein zurückkehrendes Tablet das Hand-Ergebnis.
    if finalized {
        return false;
    }
    match owner_device {
        None => true,
        Some(dev) if dev == returning_device => true,
        Some(_) if owner_scored => false,
        Some(_) => true,
    }
}

/// Versucht, ein Tablet als aktiv schiedsendes Gerät an einem Feld (per
/// CourtID) zu registrieren. Ist das Feld schon belegt, bleibt das Tablet
/// passiv.
async fn attach_tablet(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    device_id: &str,
    tx: &Tx,
) -> AttachResult {
    let mut map = broker.namespaces.lock().await;
    if !map.contains_key(ns) && map.len() >= MAX_NAMESPACES {
        tracing::warn!("Namespace-Limit erreicht – Tablet für '{ns}' abgewiesen");
        return AttachResult::Rejected;
    }
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if namespace.tablets.contains_key(&court_id) {
        // Reconnect-Erkennung: Meldet sich DASSELBE Gerät erneut (tote
        // Vorgänger-Session nach Netz-Abriss), löst es seine alte Session
        // nahtlos ab — kein „Feld belegt"-Overlay fürs eigene Gerät.
        // Leere Kennungen (alte Tablet-Seiten) zählen nie als „dasselbe".
        let same_device = !device_id.is_empty()
            && namespace.tablet_devices.get(&court_id).map(String::as_str) == Some(device_id);
        if !same_device {
            return AttachResult::Occupied;
        }
        if let Some(old) = namespace.tablets.remove(&court_id) {
            let _ = old.send(text(&ServerMsg::SessionSuperseded));
        }
        tracing::info!("Feld {court_id} (Namespace '{ns}'): Reconnect desselben Geräts");
    }
    if namespace.tablets.len() >= MAX_TABLETS_PER_NS {
        tracing::warn!("Namespace '{ns}' am Tablet-Limit – Feld {court_id} abgewiesen");
        return AttachResult::Rejected;
    }
    // A2 / ADR 0017: Den Slot-Halter VOR dem Überschreiben festhalten — die
    // Konfliktregel braucht den Zustand von vorher (nach dem `insert` sind wir
    // selbst der Halter). `owner_scored` konservativ aus `court_scores`.
    let prev_owner_device = namespace.tablet_devices.get(&court_id).cloned();
    let owner_scored = namespace
        .court_scores
        .get(&court_id)
        .is_some_and(|s| !s.is_empty());
    // A2 / ADR 0017, Regel b: Der Host reicht `finalized` im MatchBrief mit
    // (BTP-Wahrheit); der Relay hat es im zuletzt zugewiesenen Match des Felds
    // (`court_matches`). Ist das Match finalisiert, tritt der Rückkehrer
    // zurück → authoritative=false (Hand-Ergebnis nicht überbügeln).
    let finalized = namespace
        .court_matches
        .get(&court_id)
        .is_some_and(|m| m.finalized);
    // A2 / ADR 0017: Ownership-Modus? Der Host reicht den Legacy-Schalter über
    // `HostFrame::Courts` durch. Im Legacy-Modus meldet der Relay
    // `ownership_active=false`, worauf das Tablet seine rev-Logik nutzt
    // (Laufzeit-Rollback auch im Cloud-Modus).
    let ownership = !namespace.reconnect_legacy_rev;
    namespace.tablets.insert(court_id, tx.clone());
    namespace
        .tablet_devices
        .insert(court_id, device_id.to_string());
    // Laufenden Spielstand auch beim NORMALEN Verbinden wiederherstellen
    // (Crash/Ersatz-Tablet) – nicht nur bei Übernahme. Das Tablet behält ihn
    // nur, wenn die matchId zum gleich gepushten Match passt, sonst überschreibt
    // der Host das Feld (kein Wiederaufleben eines alten Stands).
    //
    // Diagnose (14.06.-Vorfall: Ersatz-Tablet sprang auf 0:0): explizit
    // protokollieren, ob beim (Neu-)Verbinden ein gespeicherter Stand
    // wiederhergestellt wurde oder das Feld ohne Stand startet.
    if let Some(state) = namespace.court_state.get(&court_id) {
        let len = state.len();
        // A2 / ADR 0017: Autorität nach dem Slot-Halter bestimmen. „Feld frei"
        // und „dasselbe Gerät reclaimt" ergeben authoritative=true. Erreichbar
        // ist hier aber AUCH der fremde Halter: ist dessen Tablet-Session tot
        // (`tablets`-Eintrag weg), `tablet_devices` nennt ihn aber noch, kommt
        // ein ANDERES Gerät bis hierher — dann liefert `relay_reconnect_
        // authoritative` korrekt authoritative=false (adoptieren, nicht
        // überschreiben). Genau das ist der wichtige Cloud-Reconnect-Pfad.
        // Der Relay führt keine Epoch → owner_epoch=0. `ownership_active`
        // spiegelt den durchgereichten Legacy-Schalter.
        let authoritative = relay_reconnect_authoritative(
            device_id,
            prev_owner_device.as_deref(),
            owner_scored,
            finalized,
        );
        let _ = tx.send(text(&ServerMsg::StateRestore {
            state: state.clone(),
            ownership_active: ownership,
            authoritative,
            owner_epoch: 0,
            owner_device: device_id.to_string(),
        }));
        tracing::info!("Feld {court_id} (Namespace '{ns}'): StateRestore gesendet ({len} Bytes)");
    } else {
        tracing::info!(
            "Feld {court_id} (Namespace '{ns}'): kein gespeicherter Stand – Tablet startet bei 0:0"
        );
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_id,
            court_label,
        }));
    }
    AttachResult::Active
}

/// Übernimmt ein belegtes Feld für ein bisher passives Tablet – das
/// zuvor aktive Tablet wird mit `SessionSuperseded` gesperrt.
async fn take_over_court(broker: &Broker, ns: &str, court_id: i64, device_id: &str, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let namespace = map.entry(ns.to_string()).or_insert_with(Namespace::new);
    if let Some(old) = namespace.tablets.insert(court_id, tx.clone()) {
        let _ = old.send(text(&ServerMsg::SessionSuperseded));
    }
    namespace
        .tablet_devices
        .insert(court_id, device_id.to_string());
    // A2 / ADR 0017: Ownership-Modus? (Legacy-Schalter des Hosts, durchgereicht
    // über `HostFrame::Courts`.) Im Legacy-Modus meldet der Relay
    // `ownership_active=false`, worauf das Tablet seine rev-Logik nutzt.
    let ownership = !namespace.reconnect_legacy_rev;
    // Laufenden Spielstand an das übernehmende Tablet übergeben.
    if let Some(state) = namespace.court_state.get(&court_id) {
        let len = state.len();
        // A2 / ADR 0017: Eine BEWUSSTE Übernahme adoptiert den laufenden Stand
        // des Felds — das übernehmende Gerät hat keine eigene „lokale Wahrheit".
        // Daher authoritative=false (adoptieren); konservativ, damit ein
        // frisch übernehmendes Tablet den Live-Stand nie überschreibt.
        let _ = tx.send(text(&ServerMsg::StateRestore {
            state: state.clone(),
            ownership_active: ownership,
            authoritative: false,
            owner_epoch: 0,
            owner_device: device_id.to_string(),
        }));
        tracing::info!(
            "Feld {court_id} (Namespace '{ns}'): Übernahme – StateRestore gesendet ({len} Bytes)"
        );
    } else {
        tracing::info!("Feld {court_id} (Namespace '{ns}'): Übernahme ohne gespeicherten Stand");
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::TabletConnected {
            court_id,
            court_label,
        }));
    }
}

/// Ist `tx` noch das am Feld eingetragene (aktive) Tablet? Nach einem
/// Reconnect-Reclaim lebt die abgelöste Session evtl. noch kurz weiter —
/// ihre nachlaufenden Frames dürfen Cache und Host nicht mehr mit dem
/// ALTEN Stand füttern (sonst kehrt genau der überbügelte Stand zurück,
/// den die Reconnect-Logik verhindert).
fn is_holder(namespace: &Namespace, court_id: i64, tx: &Tx) -> bool {
    namespace
        .tablets
        .get(&court_id)
        .map(|t| t.same_channel(tx))
        .unwrap_or(false)
}

/// Speichert den gespiegelten Spielzustand des aktiven Tablets am Feld.
async fn store_court_state(broker: &Broker, ns: &str, court_id: i64, state: String, tx: &Tx) {
    if state.len() > MAX_STATE_LEN {
        return;
    }
    let mut map = broker.namespaces.lock().await;
    if let Some(namespace) = map.get_mut(ns) {
        if !is_holder(namespace, court_id, tx) {
            return;
        }
        // Stale-Filter (A4): Ein State des ALTEN Matches darf den beim
        // Match-Wechsel geleerten Cache nicht wieder befüllen — sonst
        // bekäme ein übernehmendes Gerät das falsche Spiel.
        if let Some(state_match) = relay_proto::state_sync_match_id(&state) {
            if !match_id_matches_court(namespace, court_id, state_match) {
                tracing::info!(
                    "State von Feld {court_id} verworfen: Tablet-State trägt Match \
                     {state_match}, Feld hat ein anderes (Namespace '{ns}')"
                );
                return;
            }
        }
        namespace.court_state.insert(court_id, state);
    }
}

/// Entfernt das Tablet wieder – nur, wenn der eingetragene Sender noch
/// unserer ist (ein Reconnect auf dasselbe Feld darf nichts wegräumen).
async fn detach_tablet(broker: &Broker, ns: &str, court_id: i64, tx: &Tx) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    let still_ours = namespace
        .tablets
        .get(&court_id)
        .map(|t| t.same_channel(tx))
        .unwrap_or(false);
    if still_ours {
        namespace.tablets.remove(&court_id);
        namespace.tablet_devices.remove(&court_id);
        let court_label = label_of(namespace, court_id);
        if let Some(host) = &namespace.host {
            let _ = host.send(text(&RelayFrame::TabletDisconnected {
                court_id,
                court_label,
            }));
        }
    }
    if namespace.is_empty() {
        map.remove(ns);
    }
}

/// Passt die vom Tablet gemeldete Match-ID zum aktuellen Court-Match?
/// `match_id == 0` (alte Tablet-Seite ohne das Feld) → kein Filter,
/// Verhalten wie vor dem Feature. Nennt das Tablet ein Match, wird
/// verworfen, wenn der Relay fürs Feld ein ANDERES kennt — **oder gar
/// keins**: Nach `MatchCleared` (Feld frei) ist ein Frame mit Match-ID
/// per Definition ein Nachzügler des alten Spiels und darf den gerade
/// geleerten Cache nicht wieder befüllen (A4-Review-Befund). Gefahrlos,
/// weil `MatchAssigned` den Cache füllt, BEVOR das Tablet die Zuweisung
/// sieht — ein legitimes neues Match ist hier immer schon bekannt.
fn match_id_matches_court(namespace: &Namespace, court_id: i64, match_id: i64) -> bool {
    if match_id == 0 {
        return true;
    }
    match namespace.court_matches.get(&court_id) {
        Some(current) => current.match_id == match_id,
        None => false,
    }
}

/// Leitet einen Live-Score von einem Tablet an den Host weiter und merkt
/// ihn zugleich für die Court-Monitor-Anzeige.
#[allow(clippy::too_many_arguments)]
async fn forward_score(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    score_a: i64,
    score_b: i64,
    sets_history: Vec<SetAb>,
    match_id: i64,
    tx: &Tx,
) {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return;
    };
    if !is_holder(namespace, court_id, tx) {
        return;
    }
    // Stale-Filter (Turnier-Befund HM-03): Ein nach Doze/Reconnect noch
    // im ALTEN Spiel hängendes Tablet darf den beim Match-Wechsel frisch
    // geleerten Score-Cache nicht wieder mit dem alten Stand befüllen.
    if !match_id_matches_court(namespace, court_id, match_id) {
        tracing::info!(
            "Score von Feld {court_id} verworfen: Tablet zählt Match {match_id}, \
             Feld hat ein anderes (Namespace '{ns}')"
        );
        return;
    }
    // Vollständige Satzliste (abgeschlossene Sätze + laufender Satz) für
    // die Court-Monitor-Anzeige merken.
    let mut sets = sets_history.clone();
    sets.push(SetAb {
        a: score_a,
        b: score_b,
    });
    namespace.court_scores.insert(court_id, sets);
    // Niedrig-latente Anzeige (A1, ADR 0016): Court-Monitor + Feld-Übersicht
    // dieses Namespace sofort anstoßen, statt auf ihren nächsten Poll zu
    // warten. Muss NACH dem Cache-Insert stehen, damit der ausgelöste
    // Poll-`fetch` bereits den neuen Stand sieht.
    notify_monitor(namespace, court_id);
    let court_label = label_of(namespace, court_id);
    if let Some(host) = &namespace.host {
        let _ = host.send(text(&RelayFrame::ScoreUpdate {
            court_id,
            court_label,
            score_a,
            score_b,
            sets_history,
            match_id,
        }));
    }
}

/// Leitet den Akkustand eines Tablets an den Host weiter.
async fn forward_battery(broker: &Broker, ns: &str, court_id: i64, percent: i64, charging: bool) {
    let map = broker.namespaces.lock().await;
    if let Some(namespace) = map.get(ns) {
        let court_label = label_of(namespace, court_id);
        if let Some(host) = namespace.host.as_ref() {
            let _ = host.send(text(&RelayFrame::Battery {
                court_id,
                court_label,
                percent,
                charging,
            }));
        }
    }
}

/// Leitet den Meldungs-Zustand eines Felds an den Host weiter.
/// Einen Ballwechsel an den Host durchreichen (Punktverlauf, ADR 0014).
/// Halter- und Stale-Prüfung wie beim Score; interpretiert wird beim Host.
#[allow(clippy::too_many_arguments)]
async fn forward_rally(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    match_id: i64,
    set: i64,
    n: i64,
    winner: String,
    score_a: i64,
    score_b: i64,
    tx: &Tx,
) {
    // Der Gewinner ist ein einzelnes 'A'/'B' — alles Längere ist kein
    // legitimes Tablet und wird gar nicht erst transportiert.
    if winner.len() > 1 {
        return;
    }
    let map = broker.namespaces.lock().await;
    let Some(namespace) = map.get(ns) else {
        return;
    };
    if !is_holder(namespace, court_id, tx) {
        return;
    }
    if !match_id_matches_court(namespace, court_id, match_id) {
        return;
    }
    if let Some(host) = namespace.host.as_ref() {
        let _ = host.send(text(&RelayFrame::Rally {
            court_id,
            match_id,
            set,
            n,
            winner,
            score_a,
            score_b,
        }));
    }
}

/// Einen Verlaufs-Resync an den Host durchreichen. Der Relay prüft nur die
/// geteilten Deckel (`MatchTimeline::is_valid`, `MAX_TIMELINE_LEN`) — ein
/// überlanger Sync wird verworfen statt gespeichert (Cloud-DoS-Riegel).
async fn forward_rally_sync(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    match_id: i64,
    timeline: relay_proto::MatchTimeline,
    tx: &Tx,
) {
    if !timeline.is_valid() {
        return;
    }
    if serde_json::to_string(&timeline)
        .map(|json| json.len() > relay_proto::MAX_TIMELINE_LEN)
        .unwrap_or(true)
    {
        return;
    }
    let map = broker.namespaces.lock().await;
    let Some(namespace) = map.get(ns) else {
        return;
    };
    if !is_holder(namespace, court_id, tx) {
        return;
    }
    if !match_id_matches_court(namespace, court_id, match_id) {
        return;
    }
    if let Some(host) = namespace.host.as_ref() {
        let _ = host.send(text(&RelayFrame::RallySync {
            court_id,
            match_id,
            timeline,
        }));
    }
}

async fn forward_alert(
    broker: &Broker,
    ns: &str,
    court_id: i64,
    injury: bool,
    official: bool,
    tx: &Tx,
) {
    let map = broker.namespaces.lock().await;
    let Some(namespace) = map.get(ns) else {
        return;
    };
    if !is_holder(namespace, court_id, tx) {
        return;
    }
    let court_label = label_of(namespace, court_id);
    if let Some(host) = namespace.host.as_ref() {
        let _ = host.send(text(&RelayFrame::Alert {
            court_id,
            court_label,
            injury,
            official,
        }));
    }
}

// ─────────────────────────────── Host-WS ──────────────────────────────────

async fn host_ws(
    ws: WebSocketUpgrade,
    State(broker): State<Broker>,
    Path(ns): Path<String>,
) -> impl IntoResponse {
    if !valid_namespace(&ns) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.on_upgrade(move |socket| host_conn(socket, broker, ns))
        .into_response()
}

/// Ergebnis eines Host-Registrierungsversuchs ([`try_claim_host`]).
enum HostClaim {
    /// Slot übernommen; `true`, wenn dabei eine stumme alte Verbindung
    /// verdrängt wurde.
    Accepted { superseded: bool },
    /// Ein lebendiger Host hält den Slot — Verbindung abweisen.
    Refused,
}

/// Versucht, den Host-Slot eines Namespace zu übernehmen. Genau ein Host
/// ist erlaubt; ein LEBENDIGER Inhaber wird nie verdrängt (R4 — kein
/// fremder Host übernimmt die Kontrolle). Ist der Inhaber aber seit
/// [`HOST_STALE`] stumm (kein Frame, kein Pong), gilt er als tote
/// TCP-Leiche und der neue Host ersetzt ihn (Zombie-Host-Ablösung —
/// Turnier-Befund 19.07.: 333× „Zweiter Host abgewiesen" in 17 Minuten,
/// weil die tote alte Verbindung den Slot hielt).
fn try_claim_host(namespace: &mut Namespace, tx: &Tx, now: u64) -> HostClaim {
    let stale = namespace.host.is_some()
        && now.saturating_sub(namespace.host_last_seen) >= HOST_STALE.as_millis() as u64;
    match (&namespace.host, stale) {
        (Some(_), false) => HostClaim::Refused,
        (old, _) => {
            let superseded = old.is_some();
            namespace.host = Some(tx.clone());
            namespace.host_last_seen = now;
            // Neue Host-Verbindung = neue Generation. Sie steckt im ETag des
            // Anzeige-Zustands, weil die Revision beim Neustart des
            // Turnier-PCs wieder klein beginnt: Ein Gerät mit gemerkter
            // Fassung „1" bekäme sonst „unverändert" auf einen völlig
            // anderen Turnierstand und arbeitete auf einem Plan von vorhin.
            namespace.tl_gen = namespace.tl_gen.wrapping_add(1);
            HostClaim::Accepted { superseded }
        }
    }
}

/// Die Host-Verbindung (bts-light) eines Namespace. Genau eine ist erlaubt;
/// eine zweite wird abgewiesen — außer der bisherige Host ist nachweislich
/// stumm ([`try_claim_host`]), dann ersetzt ihn die neue Verbindung.
async fn host_conn(mut socket: WebSocket, broker: Broker, ns: String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    {
        let mut map = broker.namespaces.lock().await;
        if !map.contains_key(&ns) && map.len() >= MAX_NAMESPACES {
            tracing::warn!("Namespace-Limit erreicht – Host für '{ns}' abgewiesen");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        let namespace = map.entry(ns.clone()).or_insert_with(Namespace::new);
        match try_claim_host(namespace, &tx, now_ms()) {
            HostClaim::Refused => {
                tracing::warn!("Zweiter Host für Namespace '{ns}' abgewiesen");
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            HostClaim::Accepted { superseded } => {
                if superseded {
                    tracing::warn!(
                        "Stummen alten Host für Namespace '{ns}' ersetzt (Zombie-Ablösung)"
                    );
                }
            }
        }
        // Schon verbundene Tablets nachmelden, damit der Host ihre Matches
        // sofort pusht.
        let connected: Vec<i64> = namespace.tablets.keys().copied().collect();
        for court_id in connected {
            let court_label = label_of(namespace, court_id);
            let _ = tx.send(text(&RelayFrame::TabletConnected {
                court_id,
                court_label,
            }));
        }
    }
    tracing::info!("Host verbunden für Namespace '{ns}'");

    // Enger Ping-Takt + Stale-Abbruch: Eine tote TCP-Verbindung fällt beim
    // `send` u. U. minutenlang nicht auf (Kernel puffert) — deshalb zählt
    // hier die EMPFANGS-Seite: bleibt jedes Lebenszeichen (Frame/Pong)
    // länger als HOST_STALE aus, beendet sich die Verbindung selbst und
    // gibt den Slot frei.
    let mut ping = tokio::time::interval(HOST_PING);
    let mut last_incoming = tokio::time::Instant::now();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                last_incoming = tokio::time::Instant::now();
                match msg {
                    Message::Text(t) => {
                        if let Ok(frame) = serde_json::from_str::<HostFrame>(t.as_str()) {
                            if !handle_host_frame(&broker, &ns, frame, &tx).await {
                                // Wir sind nicht mehr der eingetragene Host
                                // (wiedererwachte Alt-Verbindung nach einer
                                // Ablösung) → sauber beenden; bts-light
                                // verbindet sich neu und sieht die Lage.
                                tracing::warn!(
                                    "Abgelöste Host-Verbindung für '{ns}' meldet sich zurück – getrennt"
                                );
                                break;
                            }
                        }
                    }
                    Message::Pong(_) => {
                        // Pong-Stempel für die Zombie-Erkennung festhalten —
                        // aber nur, solange wir der eingetragene Host sind
                        // (eine abgelöste Verbindung darf den Stempel des
                        // neuen Hosts nicht verfälschen).
                        let mut map = broker.namespaces.lock().await;
                        if let Some(namespace) = map.get_mut(&ns) {
                            if namespace.host.as_ref().is_some_and(|h| h.same_channel(&tx)) {
                                namespace.host_last_seen = now_ms();
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(m) => { if socket.send(m).await.is_err() { break } }
                    None => break,
                }
            }
            _ = ping.tick() => {
                if last_incoming.elapsed() >= HOST_STALE {
                    tracing::warn!(
                        "Host für Namespace '{ns}' seit {}s stumm – Verbindung als tot verworfen",
                        last_incoming.elapsed().as_secs()
                    );
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }

    // Aufräumen: Host-Slot freigeben (nur wenn noch unserer), offene
    // Ergebnis-Übermittlungen mit Fehler beantworten.
    {
        let mut map = broker.namespaces.lock().await;
        if let Some(namespace) = map.get_mut(&ns) {
            // Nur aufräumen, wenn WIR der eingetragene Host waren: Eine per
            // Zombie-Ablösung verdrängte Alt-Verbindung darf dem neuen Host
            // nicht die Zugänge unter den Füßen wegziehen.
            if release_host_slot(namespace, &tx) {
                forget_tl_access(namespace);
                // Den Wegweiser **hier** mitnehmen, unter derselben Sperre.
                // Aufgeschoben liefe es in ein Rennen: Ein bereits neu
                // verbundener Turnier-PC hätte seine frischen Zugänge schon
                // eingetragen, und das nachlaufende Aufräumen risse sie
                // wieder heraus. (Sperrreihenfolge: namespaces → tl_index.)
                broker.tl_index.lock().await.retain(|_, n| n != &ns);
            }
            for (_, pending) in namespace.pending.drain() {
                let _ = pending.send(ResultResponse::err("Verbindung zu bts-light verloren."));
            }
            if namespace.is_empty() {
                map.remove(&ns);
            }
        }
    }
    tracing::info!("Host getrennt für Namespace '{ns}'");
}

/// Gibt den Host-Slot frei — aber nur, wenn `tx` noch der eingetragene
/// Host ist. Eine per Zombie-Ablösung verdrängte Alt-Verbindung, die
/// später stirbt, darf den Slot des NEUEN Hosts nicht abräumen. Liefert
/// `true`, wenn der Slot tatsächlich freigegeben wurde.
fn release_host_slot(namespace: &mut Namespace, tx: &Tx) -> bool {
    if namespace
        .host
        .as_ref()
        .map(|h| h.same_channel(tx))
        .unwrap_or(false)
    {
        namespace.host = None;
        return true;
    }
    false
}

// ──────────────────── Turnierleitungs-Oberfläche (TL-Web) ─────────────────
//
// Der Relay ist hier **Briefträger, nicht Schiedsrichter**. Er kennt weder
// Spiele noch Felder; er prüft den Zugang, reicht das Kommando an den
// Turnier-PC durch und trägt dessen Antwort zurück. Jede fachliche
// Entscheidung — darf dieses Spiel auf dieses Feld? — fällt dort (R5). Das
// ist zugleich die Sicherheits-Mitigation: Ein Fehler hier kann keine
// Wertung erfinden, weil hier nichts entschieden wird.

/// Höchstzahl gleichzeitig bedienter Turnierleitungs-Geräte je Turnier.
///
/// Ein Turnier hat eine Handvoll Helfer. Die Grenze schützt den Turnier-PC
/// davor, von Dutzenden offenen Browsern abgefragt zu werden.
const MAX_TL_DEVICES: usize = relay_proto::MAX_TL_DEVICES_ONLINE;

/// Nach dieser Stille gilt ein Geräteplatz als frei. Wer nur den Tab
/// geschlossen hat, soll seinen Platz nicht bis zum Turnierende blockieren.
const TL_DEVICE_TTL_MS: u64 = 60_000;

/// Höchstzahl der Zugänge, die ein Turnier-PC spiegeln darf.
///
/// Großzügig gegenüber [`MAX_TL_DEVICES`] (Geräte kommen und gehen, alte
/// Kopplungen bleiben in der Liste), aber weit unter dem, was den Speicher
/// gefährdet. Wird sie überschritten, verwirft der Relay das **ganze** Frame:
/// Ein gekappter Widerruf wäre schlimmer als gar keiner.
///
/// **Geteilt mit dem Host** (`relay_proto`), damit er vorher weiß, wo Schluss
/// ist — sonst hielte er ein verworfenes Frame für zugestellt.
const MAX_TL_TOKENS: usize = relay_proto::MAX_TL_DEVICES_MIRRORED;

/// Wie lange eine Anfrage auf die Quittung des Turnier-PCs wartet.
const TL_TIMEOUT: Duration = Duration::from_secs(20);

// **Sperrreihenfolge:** Wird beides gebraucht, zuerst `namespaces`, dann
// `tl_index` — nie umgekehrt. Die Handler lesen den Wegweiser deshalb in
// einem eigenen Block, dessen Sperre fällt, bevor sie den Namespace greifen.

/// Was der Relay über einen Zugang sagen kann.
#[derive(Debug, PartialEq, Eq)]
enum TlAccess {
    /// Turnier gefunden, Turnier-PC verbunden, Zugang eingetragen.
    Ok { ns: String, device_id: String },
    /// Der Turnier-PC ist nicht verbunden. **Keine Aussage über den Zugang**
    /// — ohne ihn weiß der Relay nicht, wer zugelassen ist.
    HostOffline,
    /// Der Turnier-PC ist da und kennt diesen Zugang nicht.
    Unknown,
}

/// Was gilt für diesen Zugang gerade?
///
/// Die Unterscheidung ist der Kern: „Der Turnier-PC ist kurz weg" darf **nie**
/// wie „dein Zugang wurde entzogen" aussehen. Sonst würfe jedes Gerät bei
/// jedem Netzwackler seinen Zugang weg und müsste mitten im Turnier neu
/// gekoppelt werden — vom Turnier-PC aus, quer durch die Halle.
async fn tl_access_state(broker: &Broker, token: &str) -> TlAccess {
    if token.is_empty() {
        return TlAccess::HostOffline;
    }
    // Sperrreihenfolge: Der Wegweiser wird gelesen und wieder freigegeben,
    // bevor der Namespace gegriffen wird.
    let Some(ns) = broker.tl_index.lock().await.get(token).cloned() else {
        // Kein Wegweiser-Eintrag: Entweder ist der Turnier-PC weg (dann sind
        // seine Zugänge verfallen) oder der Zugang war nie einer. Der Relay
        // kann beides nicht unterscheiden — und im Zweifel ist die
        // zurückhaltende Auskunft auch die sicherere.
        return TlAccess::HostOffline;
    };
    let map = broker.namespaces.lock().await;
    let Some(namespace) = map.get(&ns) else {
        return TlAccess::HostOffline;
    };
    if namespace.host.is_none() {
        return TlAccess::HostOffline;
    }
    match tl_device_in(namespace, token) {
        Some(device_id) => TlAccess::Ok { ns, device_id },
        None => TlAccess::Unknown,
    }
}

/// Zu welchem Turnier gehört dieser Zugang? (nur für Tests)
#[cfg(test)]
async fn tl_namespace_of(broker: &Broker, token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    broker.tl_index.lock().await.get(token).cloned()
}

/// Entfernt alle Wegweiser-Einträge eines Turniers (nur für Tests — im
/// Betrieb geschieht das unter derselben Sperre wie das Aufräumen selbst).
#[cfg(test)]
async fn forget_tl_index(broker: &Broker, ns: &str) {
    broker.tl_index.lock().await.retain(|_, n| n != ns);
}

/// Gehört dieser Zugang zu diesem Turnier?
///
/// Die zweite Hürde: Auch wenn der Wegweiser stimmt, muss der Zugang im
/// Turnier selbst eingetragen sein. Ein Zugang, den der eine Turnier-PC
/// ausgestellt hat, ist im Turnier nebenan nichts wert.
#[cfg(test)]
async fn tl_lookup(broker: &Broker, ns: &str, token: &str) -> bool {
    tl_device_id(broker, ns, token).await.is_some()
}

#[cfg(test)]
async fn tl_device_id(broker: &Broker, ns: &str, token: &str) -> Option<String> {
    let map = broker.namespaces.lock().await;
    tl_device_in(map.get(ns)?, token)
}

/// Die Kennung des Geräts hinter diesem Zugang — `None`, wenn er in diesem
/// Turnier nicht eingetragen ist.
///
/// Arbeitet auf dem bereits gesperrten Namespace, damit die Handler und die
/// Tests **dieselbe** Prüfung benutzen: Eine zweite, inline abgeschriebene
/// Fassung im Handler wäre genau die, die niemand testet.
fn tl_device_in(namespace: &Namespace, token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    namespace.tl_tokens.get(token).cloned()
}

/// Vergisst alle Zugänge und den Anzeige-Zustand eines Turniers.
///
/// Beim Verschwinden des Turnier-PCs: Ohne ihn gibt es nichts zu bedienen,
/// und die Zugänge über das Turnier hinaus gültig zu halten wäre auf einem
/// Relay, der viele Turniere sieht, die falsche Vorgabe.
fn forget_tl_access(namespace: &mut Namespace) {
    namespace.tl_tokens.clear();
    namespace.tl_state = None;
    namespace.tl_devices.clear();
    for (_, pending) in namespace.tl_pending.drain() {
        let _ = pending.send(TlResponse::err(
            TlErrorCode::HostOffline,
            "Die Verbindung zum Turnier-PC ist abgerissen.",
        ));
    }
    // Wartende Punktverlauf-Abrufe ebenso auflösen — sonst hingen ihre
    // HTTP-Handler bis zum Timeout an einem toten Host. Die Sender werden
    // FALLENGELASSEN statt mit `found:false` beantwortet: `false` hieße
    // in der Route „kein Verlauf" (404) — ein Host-Abriss ist aber ein
    // 503, sonst kippte ein offenes Overlay beim Reconnect kurz auf
    // „Zu diesem Spiel liegt kein Punktverlauf vor" (Review 2026-08-11).
    namespace.timeline_pending.clear();
}

/// Der abgelegte Anzeige-Zustand (Revision + JSON), falls einer da ist.
#[cfg(test)]
async fn tl_stored_state(broker: &Broker, ns: &str) -> Option<(u64, String)> {
    let map = broker.namespaces.lock().await;
    map.get(ns).and_then(|n| n.tl_state.clone())
}

/// Beansprucht einen Geräteplatz. `false` = das Turnier ist voll.
///
/// Ein bereits belegter Platz wird nur aufgefrischt. Stille Plätze verfallen
/// nach [`TL_DEVICE_TTL_MS`].
fn claim_tl_slot(namespace: &mut Namespace, token: &str, now: u64) -> bool {
    namespace
        .tl_devices
        .retain(|_, seen| *seen + TL_DEVICE_TTL_MS > now);
    if !namespace.tl_devices.contains_key(token) && namespace.tl_devices.len() >= MAX_TL_DEVICES {
        return false;
    }
    namespace.tl_devices.insert(token.to_string(), now);
    true
}

/// Reicht ein Kommando an den Turnier-PC durch und wartet auf seine Antwort.
///
/// Dasselbe erprobte Muster wie bei der Ergebnismeldung vom Tablet: eine
/// laufende Nummer, ein wartender Kanal, ein Zeitablauf. Der Relay entscheidet
/// nichts — er trägt nur.
async fn tl_forward(
    broker: &Broker,
    ns: &str,
    device_id: String,
    op_id: String,
    view_rev: u64,
    action: TlAction,
) -> TlResponse {
    let (ack_tx, ack_rx) = oneshot::channel();
    let req_id;
    {
        let mut map = broker.namespaces.lock().await;
        let Some(namespace) = map.get_mut(ns) else {
            return TlResponse::err(
                TlErrorCode::HostOffline,
                "Der Turnier-PC ist nicht mit dem Relay verbunden.",
            );
        };
        let Some(host) = namespace.host.clone() else {
            return TlResponse::err(
                TlErrorCode::HostOffline,
                "Der Turnier-PC ist nicht mit dem Relay verbunden.",
            );
        };
        // Wie bei den Ergebnissen: Jede offene Anfrage hält bis zum Zeitablauf
        // einen Platz. Ohne Grenze könnte ein einzelnes Gerät den Namespace
        // mit wartenden Anfragen füllen.
        if namespace.tl_pending.len() >= MAX_PENDING_PER_NS {
            return TlResponse::err(
                TlErrorCode::HostOffline,
                "Zu viele offene Anfragen — bitte kurz warten.",
            );
        }
        req_id = namespace.next_req;
        namespace.next_req += 1;
        namespace.tl_pending.insert(req_id, ack_tx);
        let frame = RelayFrame::TlCommand {
            req_id,
            device_id,
            op_id,
            view_rev,
            action,
        };
        if host.send(text(&frame)).is_err() {
            namespace.tl_pending.remove(&req_id);
            return TlResponse::err(
                TlErrorCode::HostOffline,
                "Der Turnier-PC ist nicht erreichbar.",
            );
        }
    }
    match tokio::time::timeout(TL_TIMEOUT, ack_rx).await {
        Ok(Ok(resp)) => resp,
        _ => {
            let mut map = broker.namespaces.lock().await;
            if let Some(namespace) = map.get_mut(ns) {
                namespace.tl_pending.remove(&req_id);
            }
            TlResponse::err(
                TlErrorCode::HostOffline,
                "Der Turnier-PC hat nicht geantwortet — bitte den Stand prüfen.",
            )
        }
    }
}

/// Soll die Turnierleitungs-Oberfläche bedient werden?
///
/// Genau ein Wort schaltet ab: `off`. Der Relay ist ein globales Binary —
/// der Not-Aus muss im Notfall sicher greifen, aber ein Tippfehler in der
/// Umgebung darf nicht stillschweigend die halbe Turnierleitung lahmlegen.
/// Im Zweifel bleibt die Oberfläche an; abschalten ist eine bewusste Tat.
fn tl_enabled(env_value: Option<&str>) -> bool {
    !env_value.is_some_and(|v| v.trim().eq_ignore_ascii_case("off"))
}

/// Die Turnierleitungs-Seite. **Ohne Zugangsprüfung** — genau wie die
/// Tablet-Seite: Ausgeliefert wird nur eine leere Hülle, die ihren Zugang
/// erst aus dem Adress-Fragment liest. Alles Verwertbare kommt über
/// `/tl/api/state`, und das ist geschützt.
async fn tl_page() -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store")], Html(TL_HTML))
}

/// Liest den Zugang aus dem `Authorization: Bearer`-Kopf.
fn bearer(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim().chars().take(256).collect())
        .unwrap_or_default()
}

/// Der Anzeige-Zustand für ein Turnierleitungs-Gerät.
///
/// Der Relay liefert nur aus, was der Turnier-PC zuletzt gepusht hat — er
/// baut nichts und ergänzt nichts. Fehlt der Stand, ist das **kein** leeres
/// Turnier, sondern „nicht verbunden": Ein leerer Stand sähe aus wie „alle
/// Felder frei" und lüde dazu ein, alles neu zu vergeben.
async fn tl_state_route(
    State(broker): State<Broker>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let token = bearer(&headers);
    let now = now_ms();
    // Der Zugang findet sein Turnier selbst.
    let ns = match tl_access_state(&broker, &token).await {
        TlAccess::Ok { ns, .. } => ns,
        // **Nicht 401**: Ohne Turnier-PC weiß der Relay nichts über den
        // Zugang. Ein 401 hier hieße für die Seite „entzogen" — und ein
        // Netzwackler kostete jedes Gerät seine Kopplung.
        TlAccess::HostOffline => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CACHE_CONTROL, "no-store")],
                "Der Turnier-PC ist nicht mit dem Relay verbunden.",
            )
                .into_response()
        }
        TlAccess::Unknown => return (StatusCode::UNAUTHORIZED, "Kein Zugang").into_response(),
    };
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(&ns) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Der Turnier-PC ist nicht mit dem Relay verbunden.",
        )
            .into_response();
    };
    if !claim_tl_slot(namespace, &token, now) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Zu viele Turnierleitungs-Geräte in diesem Turnier.",
        )
            .into_response();
    }
    let Some((rev, json)) = namespace.tl_state.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CACHE_CONTROL, "no-store")],
            "Der Turnier-PC hat noch keinen Stand geliefert.",
        )
            .into_response();
    };
    // Generation **und** Revision als ETag: Ein Gerät, das denselben Stand
    // schon hat, bekommt 304 und spart die Übertragung — bei einer Seite, die
    // alle zwei Sekunden fragt, ist das der Unterschied zwischen sparsam und
    // lästig. Die Generation muss mit hinein, weil die Revision beim Neustart
    // des Turnier-PCs wieder klein beginnt.
    let etag = format!("\"{}-{rev}\"", namespace.tl_gen);
    let unveraendert = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag);
    if unveraendert {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, "no-store"),
            ],
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, etag.as_str()),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    )
        .into_response()
}

/// Punktverlauf eines Matches für ein Turnierleitungs-Gerät — **on-demand**
/// durchgereicht (Spec punktverlauf-graph, AK-5): Anfrage per
/// `TimelineRequest` an den Turnier-PC, Antwort (`TimelineData`) zurück an
/// den wartenden Abruf. Der Relay hält keine Verläufe vor — Briefträger,
/// nicht Speicher; genau deshalb bleibt das Mobilfunk-Budget unberührt.
/// Ohne Namespace in der Adresse, wie alle TL-Routen (ADR 0012).
async fn tl_timeline_route(
    State(broker): State<Broker>,
    headers: axum::http::HeaderMap,
    Path(match_id): Path<i64>,
) -> axum::response::Response {
    let token = bearer(&headers);
    let now = now_ms();
    let ns = match tl_access_state(&broker, &token).await {
        TlAccess::Ok { ns, .. } => ns,
        // Nicht 401 (siehe tl_state_route): ohne Turnier-PC weiß der Relay
        // nichts über den Zugang.
        TlAccess::HostOffline => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CACHE_CONTROL, "no-store")],
                "Der Turnier-PC ist nicht mit dem Relay verbunden.",
            )
                .into_response()
        }
        TlAccess::Unknown => return (StatusCode::UNAUTHORIZED, "Kein Zugang").into_response(),
    };
    let (ack_rx, req_id) = {
        let mut map = broker.namespaces.lock().await;
        let Some(namespace) = map.get_mut(&ns) else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Der Turnier-PC ist nicht mit dem Relay verbunden.",
            )
                .into_response();
        };
        if !claim_tl_slot(namespace, &token, now) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Zu viele Turnierleitungs-Geräte in diesem Turnier.",
            )
                .into_response();
        }
        let Some(host) = namespace.host.clone() else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Der Turnier-PC ist nicht mit dem Relay verbunden.",
            )
                .into_response();
        };
        // Platz-Limit wie bei den Kommandos: Ein einzelnes Gerät darf den
        // Namespace nicht mit wartenden Anfragen füllen.
        if namespace.timeline_pending.len() >= MAX_PENDING_PER_NS {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Zu viele offene Anfragen — bitte kurz warten.",
            )
                .into_response();
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        let req_id = namespace.next_req;
        namespace.next_req += 1;
        namespace.timeline_pending.insert(req_id, ack_tx);
        if host
            .send(text(&RelayFrame::TimelineRequest { req_id, match_id }))
            .is_err()
        {
            namespace.timeline_pending.remove(&req_id);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Der Turnier-PC ist nicht erreichbar.",
            )
                .into_response();
        }
        (ack_rx, req_id)
    };
    match tokio::time::timeout(TL_TIMEOUT, ack_rx).await {
        Ok(Ok((true, json))) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            json,
        )
            .into_response(),
        Ok(Ok((false, _))) => (
            StatusCode::NOT_FOUND,
            "Zu diesem Spiel liegt kein Punktverlauf vor.",
        )
            .into_response(),
        _ => {
            let mut map = broker.namespaces.lock().await;
            if let Some(namespace) = map.get_mut(&ns) {
                namespace.timeline_pending.remove(&req_id);
            }
            // Auch der Versions-Schiefstand landet hier: Ein älterer
            // Turnier-PC kennt den Frame nicht und antwortet nie.
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Der Turnier-PC hat nicht geantwortet — seine Version kennt \
                 den Punktverlauf möglicherweise noch nicht.",
            )
                .into_response()
        }
    }
}

/// Rumpf eines TL-Kommandos, wie ihn die Seite schickt.
#[derive(serde::Deserialize)]
struct TlCommandBody {
    #[serde(rename = "opId", default)]
    op_id: String,
    #[serde(rename = "viewRev", default)]
    view_rev: u64,
    action: TlAction,
}

/// Ein Kommando eines Turnierleitungs-Geräts.
///
/// Der Relay prüft den Zugang und reicht durch. Ob die Aktion zulässig ist,
/// entscheidet allein der Turnier-PC (R5) — er ist der Einzige, der den
/// Turnierstand kennt.
async fn tl_command_route(
    State(broker): State<Broker>,
    headers: axum::http::HeaderMap,
    Json(body): Json<TlCommandBody>,
) -> impl IntoResponse {
    let token = bearer(&headers);
    let now = now_ms();
    // Die Kennung des Geräts kommt vom Turnier-PC und reist zu ihm zurück —
    // sein Protokoll soll benennen können, wer gehandelt hat. Der Zugang
    // selbst bleibt hier; in Protokollen hat er nichts verloren.
    let (ns, device_id) = match tl_access_state(&broker, &token).await {
        TlAccess::Ok { ns, device_id } => (ns, device_id),
        TlAccess::HostOffline => {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-store")],
                Json(TlResponse::err(
                    TlErrorCode::HostOffline,
                    "Der Turnier-PC ist nicht mit dem Relay verbunden.",
                )),
            )
                .into_response()
        }
        TlAccess::Unknown => return (StatusCode::UNAUTHORIZED, "Kein Zugang").into_response(),
    };
    {
        let mut map = broker.namespaces.lock().await;
        let Some(namespace) = map.get_mut(&ns) else {
            return (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-store")],
                Json(TlResponse::err(
                    TlErrorCode::HostOffline,
                    "Der Turnier-PC ist nicht mit dem Relay verbunden.",
                )),
            )
                .into_response();
        };
        if !claim_tl_slot(namespace, &token, now) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "Zu viele Turnierleitungs-Geräte in diesem Turnier.",
            )
                .into_response();
        }
    }
    let op_id: String = body.op_id.chars().take(128).collect();
    let antwort = tl_forward(&broker, &ns, device_id, op_id, body.view_rev, body.action).await;
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(antwort),
    )
        .into_response()
}

/// Verarbeitet ein Frame vom Host: an das passende Tablet weiterleiten bzw.
/// eine wartende Ergebnis-Übermittlung abschließen.
///
/// `sender` ist das Sende-Ende der aufrufenden Host-Verbindung: Frames
/// werden nur verarbeitet, wenn sie vom AKTUELL eingetragenen Host
/// stammen — eine per Zombie-Ablösung verdrängte Alt-Verbindung, die
/// wieder erwacht, darf den Zustand nicht mehr verändern. Liefert
/// `false`, wenn der Sender nicht (mehr) der eingetragene Host ist.
async fn handle_host_frame(broker: &Broker, ns: &str, frame: HostFrame, sender: &Tx) -> bool {
    let mut map = broker.namespaces.lock().await;
    let Some(namespace) = map.get_mut(ns) else {
        return false;
    };
    if !namespace
        .host
        .as_ref()
        .is_some_and(|h| h.same_channel(sender))
    {
        return false;
    }
    // Jedes Host-Frame ist ein Lebenszeichen für die Zombie-Erkennung.
    namespace.host_last_seen = now_ms();
    match frame {
        HostFrame::MatchAssigned {
            court_id,
            court_label,
            hall,
            match_brief,
            on_court_since_ms,
        } => {
            // Feldname (Anzeige) merken – der Monitor liest ihn.
            if !court_label.is_empty() {
                namespace.court_labels.insert(court_id, court_label);
            }
            // Halle des Felds merken – für die hallengefilterte Cloud-Ansage.
            namespace.court_hall.insert(court_id, hall);
            // Satzstand/Spielzustand nur bei einem ECHTEN Match-Wechsel
            // zurücksetzen. Ein erneutes `MatchAssigned` fürs selbe Match
            // (z. B. nach einem kurzen Tablet-Reconnect) darf den Monitor
            // nicht auf 0:0 zurückwerfen.
            let same_match = namespace
                .court_matches
                .get(&court_id)
                .map(|m| m.match_id == match_brief.match_id)
                .unwrap_or(false);
            if !same_match {
                namespace.court_scores.remove(&court_id);
                namespace.court_state.remove(&court_id);
            }
            // 1.-Aufruf-Zeitpunkt: den autoritativen Host-Stempel übernehmen
            // (gleicher Wert auch bei Reconnect, frisch je Turnier → kein
            // veralteter Stand). Fehlt er (älterer Host), Eintrag entfernen.
            match on_court_since_ms {
                Some(ts) => {
                    namespace.court_on_court_since.insert(court_id, ts);
                }
                None => {
                    namespace.court_on_court_since.remove(&court_id);
                }
            }
            namespace
                .court_matches
                .insert(court_id, match_brief.clone());
            if let Some(t) = namespace.tablets.get(&court_id) {
                let _ = t.send(text(&ServerMsg::MatchAssigned { match_brief }));
            }
        }
        HostFrame::MatchCleared {
            court_id,
            court_label,
            hall,
        } => {
            if !court_label.is_empty() {
                namespace.court_labels.insert(court_id, court_label);
            }
            namespace.court_hall.insert(court_id, hall);
            namespace.court_matches.remove(&court_id);
            namespace.court_scores.remove(&court_id);
            namespace.court_state.remove(&court_id);
            namespace.court_on_court_since.remove(&court_id);
            if let Some(t) = namespace.tablets.get(&court_id) {
                let _ = t.send(text(&ServerMsg::MatchCleared));
            }
        }
        HostFrame::Freetext { id, hall, text } => {
            // Längen hart begrenzen (Schutz vor RAM-Aufblähung durch
            // pathologische Frames; char-genau, kein Byte-Slice-Panic).
            let text: String = text.chars().take(1000).collect();
            let hall: String = hall.chars().take(128).collect();
            // Neue Freitext-Ansage zwischenspeichern (dedup nach id, Cap 50) –
            // der Cloud-Ansage-Slave holt sie über /info/announce/state.
            if !namespace.freetext.iter().any(|f| f.id == id) {
                namespace
                    .freetext
                    .push(relay_proto::FreetextItem { id, hall, text });
                let len = namespace.freetext.len();
                if len > 50 {
                    namespace.freetext.drain(0..len - 50);
                }
            }
        }
        HostFrame::ResultAck { req_id, ok, error } => {
            if let Some(pending) = namespace.pending.remove(&req_id) {
                let _ = pending.send(ResultResponse { ok, error });
            }
        }
        HostFrame::Courts {
            courts,
            azure_tts,
            reconnect_legacy_rev,
        } => {
            // Vollständige Feld-Liste für das Cloud-Feldwechsel-Menü merken.
            // Leere Liste NICHT übernehmen: der Host schickt sie nur, um die
            // Azure-Vererbung zu transportieren, solange BTP noch kein
            // Turnier geladen hat — sie darf eine gültige Liste nicht wischen.
            if !courts.is_empty() {
                namespace.courts = courts;
            }
            // Azure-Vererbung: jeder Push ist autoritativ, auch `None`
            // (Azure am Master deaktiviert → geerbte Config verfällt).
            namespace.azure_tts = azure_tts;
            // A2 / ADR 0017: Legacy-rev-Schalter des Hosts übernehmen (jeder
            // Push autoritativ) — steuert `ownership_active` beim nächsten
            // Reconnect. Ältere Hosts senden `false` per `#[serde(default)]`
            // (Ownership aktiv, sicherer Default).
            namespace.reconnect_legacy_rev = reconnect_legacy_rev;
        }
        HostFrame::Prepared { mut prepared } => {
            // Aufgerufene Spiele der fernen Hallen für die Slave-Spielübersicht
            // + den Nachruf merken (Cluster C Stufe 2). Jeder Push ersetzt die
            // Liste vollständig — ein leerer Push (kein Aufruf offen) leert sie
            // bewusst. Cap gegen pathologische Frames.
            prepared.truncate(200);
            namespace.prepared = prepared;
        }
        // TL-Web (Turnierleitungs-Oberfläche): Die Wire-Typen stehen, der
        // Relay wertet sie aber noch nicht aus — Token-Map, Routen und
        // Weiterleitung kommen in einem eigenen Schritt
        // (docs/features/turnierleitung-web.md, ADR 0012/0012). Bis dahin
        // bewusst folgenlos verworfen, damit ein Host, der schon pusht, die
        // Verbindung nicht verliert. Absichtlich einzeln aufgeführt statt
        // per Auffang-Arm: So zwingt der Compiler beim Ausbau dazu, jede
        // Variante anzufassen.
        //
        // **Beim Ausbau zwingend:** `TlAck` muss die wartende Anfrage
        // auflösen (Muster `ResultAck`). Bleibt dieser Arm stehen, verpufft
        // auch die Absage, die der Host bereits sendet
        // (src-tauri/src/tablet/relay_client.rs) — jedes Gerät liefe dann
        // in den Zeitablauf statt eine Klartext-Meldung zu sehen.
        HostFrame::TlAuth { devices } => {
            // Zu viele: das ganze Frame verwerfen. Kappen wäre schlimmer —
            // dann gälte ein Teil der Geräte weiter und ein anderer nicht,
            // und der Widerruf wäre nur noch halb wirksam. Ohne diese Grenze
            // könnte ein fehlerhafter (oder feindlicher) Host mit einer
            // Millionenliste den Relay-Prozess in den Speichertod treiben —
            // und mit ihm die Tablets, Monitore und den Ergebnisweg **aller**
            // gleichzeitig laufenden Turniere.
            if devices.len() > MAX_TL_TOKENS {
                tracing::warn!(
                    "TlAuth für '{ns}' verworfen: {} Geräte (Grenze {MAX_TL_TOKENS})",
                    devices.len()
                );
                return true;
            }
            // **Ersetzen, nicht ergänzen**: Das ist der Widerruf. Ein
            // abhandengekommenes Tablet verliert seinen Zugang, sobald der
            // Turnier-PC ihn nicht mehr nennt — ergänzten wir hier, bliebe er
            // bis zum Turnierende gültig.
            namespace.tl_tokens = devices
                .into_iter()
                .filter(|d| !d.token.is_empty())
                .map(|d| (d.token, d.id))
                .collect();
            // Plätze von Geräten aufgeben, deren Zugang eben widerrufen wurde.
            namespace
                .tl_devices
                .retain(|t, _| namespace.tl_tokens.contains_key(t));
            // Den Wegweiser mitziehen — sonst zeigte ein widerrufener Zugang
            // weiter auf sein Turnier, und nur die zweite Prüfung hielte ihn
            // auf. (Sperrreihenfolge: namespaces ist gehalten, tl_index folgt.)
            let mut index = broker.tl_index.lock().await;
            index.retain(|_, n| n != ns);
            for token in namespace.tl_tokens.keys() {
                // Einen fremden Eintrag **nicht** überschreiben: Sonst könnte
                // ein zweiter Namespace, der denselben Zugang nennt, ein
                // fremdes Gerät zu sich umleiten — es bekäme den Stand eines
                // fremden Turniers samt Spielernamen. Zufällige Zugänge
                // kollidieren praktisch nie; ein Riegel ohne Riegel ist
                // trotzdem keiner.
                match index.get(token) {
                    Some(vorhanden) if vorhanden != ns => {
                        tracing::warn!(
                            "TL-Zugang von '{ns}' kollidiert mit '{vorhanden}' — ignoriert"
                        );
                    }
                    _ => {
                        index.insert(token.clone(), ns.to_string());
                    }
                }
            }
        }
        HostFrame::TlState { rev, json } => {
            if json.len() > MAX_STATE_LEN {
                // Zu groß: nicht ablegen — der Relay trägt viele Turniere,
                // und ein Zustand, der aus dem Ruder läuft, darf sie nicht
                // mitnehmen. **Und den alten mit wegwerfen:** Bliebe er
                // liegen, bekäme jedes Gerät weiter 304 auf einen längst
                // eingefrorenen Feldplan und läse dazu „aktuell". Eine
                // ehrliche Fehlanzeige ist besser als ein falscher Plan, auf
                // den jemand ein Spiel setzt.
                tracing::warn!(
                    "TL-Zustand für '{ns}' verworfen: {} Bytes (Grenze {MAX_STATE_LEN}) \
                     — die Oberfläche meldet sich als nicht verbunden",
                    json.len()
                );
                namespace.tl_state = None;
            } else {
                namespace.tl_state = Some((rev, json));
            }
        }
        HostFrame::TlAck { req_id, response } => {
            if let Some(pending) = namespace.tl_pending.remove(&req_id) {
                let _ = pending.send(response);
            }
        }
        // Punktverlauf-Antwort des Hosts → an den wartenden Abruf. Der
        // Größen-Deckel gilt auch hier: ein überlanger Verlauf wird zur
        // ehrlichen Fehlanzeige statt zum Speicherfresser.
        HostFrame::TimelineData {
            req_id,
            found,
            json,
        } => {
            if let Some(pending) = namespace.timeline_pending.remove(&req_id) {
                let zu_gross = json.len() > relay_proto::MAX_TIMELINE_LEN;
                let _ = pending.send(if zu_gross {
                    (false, String::new())
                } else {
                    (found, json)
                });
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::{CourtExpectation, MatchBrief, PlayerBrief};

    #[test]
    fn pairing_code_is_eight_digits_and_random() {
        // Format: genau 8 Ziffern, führende Nullen erlaubt.
        let a = gen_pairing_code().unwrap();
        assert!(
            valid_pairing_code(&a),
            "Code nicht 8-stellig numerisch: {a}"
        );
        // Zwei Züge kollidieren praktisch nie – schützt vor einem
        // versehentlich konstanten Generator (z. B. vergessener Zufall).
        let b = gen_pairing_code().unwrap();
        let c = gen_pairing_code().unwrap();
        assert!(a != b || b != c, "Generator liefert konstant {a}");
    }

    #[test]
    fn valid_pairing_code_rejects_non_digits_and_wrong_length() {
        assert!(valid_pairing_code("00000000"));
        assert!(valid_pairing_code("12345678"));
        assert!(!valid_pairing_code("1234567"));
        assert!(!valid_pairing_code("123456789"));
        assert!(!valid_pairing_code("12a45678"));
        assert!(!valid_pairing_code("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
        assert!(!valid_pairing_code(""));
    }

    #[test]
    fn flag_lookup_liefert_svg_und_wehrt_traversal_ab() {
        // Regulärer Abruf: die gebündelten Länder-SVGs sind auffindbar.
        assert!(flag_lookup("GER.svg").is_some());
        // Unbekanntes Kürzel: kein Treffer, kein Fehler.
        assert!(flag_lookup("XXX.svg").is_none());
        assert!(flag_lookup("").is_none());
        // Pfad-Spielereien laufen ins Leere — die Datei kommt aus dem
        // Anfrage-Pfad und darf das Bündel nie verlassen.
        assert!(flag_lookup("../Cargo.toml").is_none());
        assert!(flag_lookup("a/GER.svg").is_none());
        assert!(flag_lookup("a\\GER.svg").is_none());
    }

    #[test]
    fn valid_namespace_accepts_uuid_rejects_garbage() {
        assert!(valid_namespace("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
        assert!(!valid_namespace(""));
        assert!(!valid_namespace("not-a-uuid"));
        // 32 Hex ohne Bindestriche – falsche Form.
        assert!(!valid_namespace("a1b2c3d4e5f67890abcdef1234567890abcd"));
        assert!(!valid_namespace("../../../etc/passwd"));
    }

    #[test]
    fn monitor_base_keeps_the_mount_path() {
        // Der Relay läuft hinter nginx unter /bts-relay/ – der Präfix muss
        // im Basis-Pfad erhalten bleiben, sonst zeigen die State-/Asset-
        // URLs des Monitors ins Leere.
        assert_eq!(
            monitor_base("https://badhub.de/bts-relay", "ns1"),
            "/bts-relay/ns1/"
        );
        assert_eq!(
            monitor_base("https://badhub.de/bts-relay/", "ns1"),
            "/bts-relay/ns1/"
        );
        // Relay direkt auf der Domain-Wurzel.
        assert_eq!(monitor_base("https://relay.example.com", "ns1"), "/ns1/");
    }

    fn brief(id: i64) -> MatchBrief {
        MatchBrief {
            match_id: id,
            team_a: vec![PlayerBrief {
                id: 1,
                name: "Anna".into(),
                nationality: Some("GER".into()),
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
            interval_at: Some(11),
            discipline: "mens_singles".into(),
            class_label: String::new(),
            match_number: Some(14),
            scorekeeper: Vec::new(),
            scorekeeper_assigned: false,
            show_club_names: false,
            show_club_logos: false,
            finalized: false,
        }
    }

    /// Registriert `tx` als Host des Namespace (wie eine frisch
    /// angenommene Host-Verbindung).
    async fn register_host(broker: &Broker, ns: &str, tx: &Tx) {
        let mut map = broker.namespaces.lock().await;
        let namespace = map.entry(ns.into()).or_insert_with(Namespace::new);
        namespace.host = Some(tx.clone());
        namespace.host_last_seen = now_ms();
    }

    /// Legt einen Namespace mit registriertem Host und einem Tablet (an
    /// Feld `court_id`) an; liefert Tablet-Empfangsende + Host-Sender.
    async fn broker_with_tablet(court_id: i64) -> (Broker, mpsc::UnboundedReceiver<Message>, Tx) {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (tx, rx) = mpsc::unbounded_channel();
        let (host_tx, _host_rx) = mpsc::unbounded_channel();
        let mut map = broker.namespaces.lock().await;
        let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
        ns.tablets.insert(court_id, tx);
        ns.host = Some(host_tx.clone());
        ns.host_last_seen = now_ms();
        drop(map);
        (broker, rx, host_tx)
    }

    // ───────────── Court-Monitor-Nudge (A1, ADR 0016) ─────────────

    /// Liest `court`/`seq` aus einem Nudge-Text-Frame.
    fn nudge_of(m: Message) -> (i64, u64) {
        let Message::Text(t) = m else {
            panic!("kein Text-Frame");
        };
        let v: serde_json::Value = serde_json::from_str(t.as_str()).unwrap();
        (v["court"].as_i64().unwrap(), v["seq"].as_u64().unwrap())
    }

    #[test]
    fn monitor_nudge_routes_to_court_and_all_but_not_other_courts() {
        // Broker-Routing: `notify_monitor(5)` weckt GENAU die Court-5-Anzeige
        // und die „alle Felder"-Übersicht — Feld 3 bleibt still.
        let mut ns = Namespace::new();
        let (tx5, mut rx5) = mpsc::unbounded_channel();
        let (tx3, mut rx3) = mpsc::unbounded_channel();
        let (tx_all, mut rx_all) = mpsc::unbounded_channel();
        ns.monitor_subs.entry(5).or_default().push(tx5);
        ns.monitor_subs.entry(3).or_default().push(tx3);
        ns.monitor_subs_all.push(tx_all);

        notify_monitor(&mut ns, 5);

        assert_eq!(nudge_of(rx5.try_recv().unwrap()), (5, 1));
        assert_eq!(nudge_of(rx_all.try_recv().unwrap()).0, 5);
        assert!(rx3.try_recv().is_err(), "Feld 3 bleibt unberührt");
    }

    #[test]
    fn monitor_seq_is_monotonic_per_court() {
        // `seq` steigt je Feld streng monoton und zählt getrennt je Feld.
        let mut ns = Namespace::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        ns.monitor_subs.entry(1).or_default().push(tx1);
        ns.monitor_subs.entry(2).or_default().push(tx2);

        notify_monitor(&mut ns, 1);
        notify_monitor(&mut ns, 1);
        notify_monitor(&mut ns, 2);

        assert_eq!(nudge_of(rx1.try_recv().unwrap()).1, 1);
        assert_eq!(nudge_of(rx1.try_recv().unwrap()).1, 2);
        assert_eq!(nudge_of(rx2.try_recv().unwrap()).1, 1);
    }

    #[test]
    fn dead_monitor_subscriber_is_pruned_on_next_nudge() {
        // Fällt die Anzeige weg (Rx fallengelassen), siebt der nächste Nudge
        // den toten Sender aus und entfernt die leere Feld-Liste.
        let mut ns = Namespace::new();
        let (tx, rx) = mpsc::unbounded_channel();
        ns.monitor_subs.entry(4).or_default().push(tx);
        drop(rx);

        notify_monitor(&mut ns, 4);

        assert!(
            !ns.monitor_subs.contains_key(&4),
            "toter Abonnent ausgesiebt"
        );
    }

    #[tokio::test]
    async fn monitor_nudge_stays_within_its_namespace() {
        // Namespace-Isolation: Ein Nudge in ns-a erreicht KEINEN Abonnenten
        // in ns-b — die Trennung der Turniere ist strikt.
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (h_a, _ra) = mpsc::unbounded_channel();
        let (h_b, _rb) = mpsc::unbounded_channel();
        register_host(&broker, "ns-a", &h_a).await;
        register_host(&broker, "ns-b", &h_b).await;
        let (tx_a, mut rx_a) = mpsc::unbounded_channel();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel();
        subscribe_monitor(&broker, "ns-a", Some(5), &tx_a).await;
        subscribe_monitor(&broker, "ns-b", Some(5), &tx_b).await;

        {
            let mut map = broker.namespaces.lock().await;
            notify_monitor(map.get_mut("ns-a").unwrap(), 5);
        }

        assert!(rx_a.try_recv().is_ok(), "ns-a wird geweckt");
        assert!(
            rx_b.try_recv().is_err(),
            "ns-b bleibt still (Namespace-Isolation)"
        );
    }

    #[tokio::test]
    async fn monitor_subscribe_and_unsubscribe_lifecycle() {
        // Der Abonnent wird eingetragen und beim Verbindungsende wieder
        // ausgetragen; der Namespace bleibt, solange der Host verbunden ist.
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (h, _rh) = mpsc::unbounded_channel();
        register_host(&broker, "ns1", &h).await;
        let (tx, _rx) = mpsc::unbounded_channel();

        subscribe_monitor(&broker, "ns1", Some(9), &tx).await;
        {
            let map = broker.namespaces.lock().await;
            assert_eq!(
                map.get("ns1").unwrap().monitor_subs.get(&9).map(Vec::len),
                Some(1)
            );
        }

        unsubscribe_monitor(&broker, "ns1", Some(9), &tx).await;
        {
            let map = broker.namespaces.lock().await;
            assert!(!map.get("ns1").unwrap().monitor_subs.contains_key(&9));
        }
    }

    #[tokio::test]
    async fn monitor_fanout_cap_rejects_the_over_limit_subscription() {
        // Fan-out-Deckel (N5): Bis exakt `MAX_MONITOR_SUBS` werden Abos
        // eingetragen; das (N+1)-te wird abgelehnt (Zuschauer-DoS-Schutz).
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (h, _rh) = mpsc::unbounded_channel();
        register_host(&broker, "ns1", &h).await;
        for _ in 0..MAX_MONITOR_SUBS {
            let (tx, _rx) = mpsc::unbounded_channel();
            subscribe_monitor(&broker, "ns1", Some(1), &tx).await;
        }
        {
            let map = broker.namespaces.lock().await;
            let total: usize = map
                .get("ns1")
                .unwrap()
                .monitor_subs
                .values()
                .map(Vec::len)
                .sum();
            assert_eq!(total, MAX_MONITOR_SUBS, "genau der Deckel ist eingetragen");
        }
        // Das (N+1)-te Abo wird abgelehnt — der Gesamtstand bleibt am Deckel.
        let (tx_over, _rx_over) = mpsc::unbounded_channel();
        subscribe_monitor(&broker, "ns1", Some(1), &tx_over).await;
        {
            let map = broker.namespaces.lock().await;
            let total: usize = map
                .get("ns1")
                .unwrap()
                .monitor_subs
                .values()
                .map(Vec::len)
                .sum();
            assert_eq!(total, MAX_MONITOR_SUBS, "über dem Deckel kein weiteres Abo");
        }
    }

    #[tokio::test]
    async fn monitor_subscribe_without_host_does_not_create_a_namespace() {
        // Ohne Host wird KEIN Namespace angelegt (kein Zuschauer-TV soll durch
        // bloßes Verbinden Speicher belegen — es gäbe keinen Aufräum-Pfad).
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (tx, _rx) = mpsc::unbounded_channel();
        subscribe_monitor(&broker, "ns-ghost", None, &tx).await;
        assert!(broker.namespaces.lock().await.get("ns-ghost").is_none());
    }

    // ───────────────── Turnierleitungs-Oberfläche (TL-Web) ─────────────────
    //
    // Der Relay ist hier **Briefträger, nicht Schiedsrichter**: Er kennt
    // weder Spiele noch Felder, prüft nur den Zugang und reicht durch. Jede
    // fachliche Entscheidung fällt am Turnier-PC (R5). Getestet wird deshalb
    // genau das, was der Relay verspricht: Wer darf durch, wer nicht, und
    // kommt die Antwort zurück.

    /// Namespace mit Host und einem eingetragenen Zugang.
    async fn broker_with_tl_device(token: &str) -> (Broker, mpsc::UnboundedReceiver<Message>, Tx) {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host_tx, host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx.clone());
            ns.host_last_seen = now_ms();
            ns.tl_tokens
                .insert(token.to_string(), "tl-test".to_string());
        }
        // Wie im Betrieb: Der Wegweiser wird mit derselben Bewegung gepflegt.
        broker
            .tl_index
            .lock()
            .await
            .insert(token.to_string(), "ns1".to_string());
        (broker, host_rx, host_tx)
    }

    #[test]
    fn the_emergency_switch_takes_only_an_explicit_off() {
        // Der Relay ist ein globales Binary. Der Not-Aus muss im Notfall
        // sicher greifen — aber ein Tippfehler in der Umgebung darf die
        // Turnierleitung nicht stillschweigend abschalten. Deshalb zählt
        // genau ein Wort, und im Zweifel bleibt die Oberfläche an.
        assert!(!tl_enabled(Some("off")));
        assert!(!tl_enabled(Some("OFF")));
        assert!(tl_enabled(None), "ohne Angabe an");
        assert!(tl_enabled(Some("on")));
        assert!(tl_enabled(Some("")), "leer ist keine Abschaltung");
        assert!(tl_enabled(Some("0")), "kein Rätselraten über 0/1");
    }

    #[tokio::test]
    async fn a_short_host_outage_does_not_look_like_a_revoked_access() {
        // Der schwerste Fehler, den dieser Weg haben kann: Ein Wackler in
        // der Verbindung des Turnier-PCs sieht aus wie ein Widerruf, alle
        // Geräte werfen ihren Zugang weg und müssen mitten im Turnier neu
        // gekoppelt werden. Ohne Turnier-PC kann der Relay über einen Zugang
        // GAR NICHTS sagen — also sagt er „nicht verbunden", nicht „kein
        // Zugang".
        let (broker, _rx, host) = broker_with_tl_device("token").await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            release_host_slot(ns, &host);
            forget_tl_access(ns);
        }
        assert_eq!(
            tl_access_state(&broker, "token").await,
            TlAccess::HostOffline,
            "der Turnier-PC ist weg — mehr weiß der Relay nicht"
        );
        // Über einen Zugang, den er nicht kennt, sagt der Relay ebenfalls
        // nichts: Er kann „nie ausgestellt" nicht von „widerrufen" oder
        // „Turnier vorbei" unterscheiden — und wer Zugänge durchprobiert,
        // soll aus der Antwort nichts lernen.
        let (broker2, _rx2, _host2) = broker_with_tl_device("echt").await;
        assert_eq!(
            tl_access_state(&broker2, "erfunden").await,
            TlAccess::HostOffline
        );
        assert!(matches!(
            tl_access_state(&broker2, "echt").await,
            TlAccess::Ok { .. }
        ));
    }

    #[tokio::test]
    async fn a_token_finds_its_own_tournament_without_naming_it() {
        // Die Adresse der Turnierleitungs-Seite trägt **keinen** Namespace:
        // Der ist die `install_id`, und die ist zugleich der Zugang der
        // Zähltablets (`/{ns}/ws`). Stünde sie in der URL, die jeder Helfer
        // auf dem Bildschirm hat, könnte sich damit jeder als Tablet
        // ausgeben. Also findet der Zugang sein Turnier selbst.
        let (broker, _rx, host) = broker_with_tl_device("token-a").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAuth {
                devices: vec![relay_proto::TlAuthDevice {
                    id: "tl-1".to_string(),
                    token: "token-a".to_string(),
                }],
            },
            &host,
        )
        .await;
        assert_eq!(
            tl_namespace_of(&broker, "token-a").await.as_deref(),
            Some("ns1")
        );
        assert!(tl_namespace_of(&broker, "unbekannt").await.is_none());
        assert!(tl_namespace_of(&broker, "").await.is_none());
    }

    #[tokio::test]
    async fn a_revoked_token_no_longer_finds_any_tournament() {
        // Der Widerruf muss auch den Wegweiser mitnehmen — sonst zeigte der
        // Zugang weiter auf sein Turnier und nur die zweite Prüfung hielte
        // ihn auf.
        let (broker, _rx, host) = broker_with_tl_device("alt").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAuth {
                devices: vec![relay_proto::TlAuthDevice {
                    id: "tl-1".to_string(),
                    token: "alt".to_string(),
                }],
            },
            &host,
        )
        .await;
        assert!(tl_namespace_of(&broker, "alt").await.is_some());

        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAuth {
                devices: vec![relay_proto::TlAuthDevice {
                    id: "tl-2".to_string(),
                    token: "neu".to_string(),
                }],
            },
            &host,
        )
        .await;
        assert!(
            tl_namespace_of(&broker, "alt").await.is_none(),
            "widerrufen"
        );
        assert_eq!(
            tl_namespace_of(&broker, "neu").await.as_deref(),
            Some("ns1")
        );

        // Und mit dem Turnier-PC verschwindet auch der Wegweiser.
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            release_host_slot(ns, &host);
            forget_tl_access(ns);
        }
        forget_tl_index(&broker, "ns1").await;
        assert!(tl_namespace_of(&broker, "neu").await.is_none());
    }

    #[tokio::test]
    async fn only_a_registered_device_may_command_a_tournament() {
        // Der Zugang ist die einzige Hürde vor einem Schreibweg, der aus dem
        // Internet erreichbar ist. Ein unbekannter Zugang darf den Host nicht
        // einmal erreichen — sonst wäre der Turnier-PC dem offenen Netz
        // ausgesetzt und müsste jede Anfrage selbst abwehren.
        let (broker, mut host_rx, _host) = broker_with_tl_device("gutes-token").await;

        let abgewiesen = tl_lookup(&broker, "ns1", "falsches-token").await;
        assert!(!abgewiesen, "fremder Zugang");
        assert!(
            host_rx.try_recv().is_err(),
            "und der Turnier-PC hat davon nie erfahren"
        );
        assert!(tl_lookup(&broker, "ns1", "gutes-token").await);
    }

    #[tokio::test]
    async fn a_token_from_one_tournament_never_reaches_another() {
        // Zwei Turniere laufen gleichzeitig auf demselben Relay. Der Zugang
        // des einen darf im anderen nichts bewirken — sonst könnte eine
        // fremde Turnierleitung Felder umräumen.
        let (broker, _rx, _host) = broker_with_tl_device("token-a").await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns2 = map.entry("ns2".into()).or_insert_with(Namespace::new);
            let (tx, _rx) = mpsc::unbounded_channel();
            ns2.host = Some(tx);
            ns2.tl_tokens
                .insert("token-b".to_string(), "tl-b".to_string());
        }
        assert!(!tl_lookup(&broker, "ns1", "token-b").await);
        assert!(!tl_lookup(&broker, "ns2", "token-a").await);
        assert!(tl_lookup(&broker, "ns2", "token-b").await);
    }

    #[tokio::test]
    async fn revoking_a_device_takes_effect_with_the_next_push() {
        // Der Widerruf ist die einzige Handhabe, wenn ein Tablet abhanden
        // kommt. Deshalb **ersetzt** der Push die Menge, statt sie zu
        // ergänzen: Was der Turnier-PC nicht mehr nennt, gilt nicht mehr.
        let (broker, _rx, host) = broker_with_tl_device("altes-token").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAuth {
                devices: vec![relay_proto::TlAuthDevice {
                    id: "tl-neu".to_string(),
                    token: "neues-token".to_string(),
                }],
            },
            &host,
        )
        .await;
        assert!(
            !tl_lookup(&broker, "ns1", "altes-token").await,
            "widerrufen"
        );
        assert!(tl_lookup(&broker, "ns1", "neues-token").await);
    }

    #[tokio::test]
    async fn the_tournament_pc_going_away_takes_the_access_with_it() {
        // Ohne Turnier-PC gibt es nichts zu bedienen. Die Zugänge dann
        // liegenzulassen hieße, sie über das Turnier hinaus gültig zu halten
        // — auf einem Relay, der viele Turniere sieht.
        let (broker, _rx, host) = broker_with_tl_device("token").await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            release_host_slot(ns, &host);
            forget_tl_access(ns);
        }
        assert!(!tl_lookup(&broker, "ns1", "token").await);
    }

    #[tokio::test]
    async fn an_absurd_device_list_is_refused_whole() {
        // Der Relay trägt alle Turniere zugleich. Eine Millionenliste würde
        // ihn in den Speichertod treiben und Tablets, Monitore und den
        // Ergebnisweg aller anderen mitreißen. Gekappt wird nicht: Ein halb
        // wirksamer Widerruf ist schlimmer als ein verworfenes Frame.
        let (broker, _rx, host) = broker_with_tl_device("gut").await;
        let zu_viele: Vec<relay_proto::TlAuthDevice> = (0..MAX_TL_TOKENS + 1)
            .map(|i| relay_proto::TlAuthDevice {
                id: format!("tl-{i}"),
                token: format!("t-{i}"),
            })
            .collect();
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAuth { devices: zu_viele },
            &host,
        )
        .await;

        assert!(
            tl_lookup(&broker, "ns1", "gut").await,
            "der alte Stand bleibt"
        );
        assert!(!tl_lookup(&broker, "ns1", "t-0").await, "nichts übernommen");
    }

    #[tokio::test]
    async fn an_oversized_state_also_drops_the_one_before_it() {
        // Bliebe der alte Stand liegen, bekäme jedes Gerät weiter „unverändert"
        // auf einen eingefrorenen Feldplan — und läse dazu „aktuell". Dann
        // setzt jemand ein Spiel auf ein Feld, das seit zehn Minuten belegt
        // ist. Eine ehrliche Fehlanzeige ist besser als ein falscher Plan.
        let (broker, _rx, host) = broker_with_tl_device("token").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlState {
                rev: 5,
                json: r#"{"rev":5}"#.to_string(),
            },
            &host,
        )
        .await;
        assert!(tl_stored_state(&broker, "ns1").await.is_some());

        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlState {
                rev: 6,
                json: "x".repeat(MAX_STATE_LEN + 1),
            },
            &host,
        )
        .await;
        assert!(
            tl_stored_state(&broker, "ns1").await.is_none(),
            "auch der alte Stand ist weg"
        );
    }

    #[tokio::test]
    async fn the_state_is_served_unchanged_and_its_revision_stays_put() {
        // Der Relay legt den Anzeige-Zustand nur ab — er versteht ihn nicht.
        // Die Revision kommt vom Turnier-PC und ändert sich nur bei echter
        // Änderung; daran erkennt ein Gerät, ob es neu zeichnen muss.
        let (broker, _rx, host) = broker_with_tl_device("token").await;
        let json = r#"{"rev":7,"courts":[]}"#;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlState {
                rev: 7,
                json: json.to_string(),
            },
            &host,
        )
        .await;
        let (rev, gespeichert) = tl_stored_state(&broker, "ns1").await.expect("Zustand da");
        assert_eq!(rev, 7);
        assert_eq!(gespeichert, json, "unverändert durchgereicht");

        // Derselbe Stand erneut gepusht: Die Revision bleibt, was sie war.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlState {
                rev: 7,
                json: json.to_string(),
            },
            &host,
        )
        .await;
        assert_eq!(tl_stored_state(&broker, "ns1").await.unwrap().0, 7);
    }

    #[tokio::test]
    async fn an_oversized_state_is_dropped_instead_of_filling_the_relay() {
        // Der Relay trägt viele Turniere. Ein Zustand, der aus dem Ruder
        // läuft, darf ihn nicht mitnehmen — dann fiele auch der
        // Tablet-Spielzettel aller anderen aus.
        let (broker, _rx, host) = broker_with_tl_device("token").await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlState {
                rev: 1,
                json: "x".repeat(MAX_STATE_LEN + 1),
            },
            &host,
        )
        .await;
        assert!(
            tl_stored_state(&broker, "ns1").await.is_none(),
            "zu groß: gar nicht erst abgelegt"
        );
    }

    #[tokio::test]
    async fn the_command_carries_the_device_name_not_its_access() {
        // Der Turnier-PC protokolliert, wer was ausgelöst hat. Dafür braucht
        // er die Kennung des Geräts — der Zugang selbst darf nirgends
        // auftauchen, auch nicht in Teilen. Deshalb reist die Kennung mit dem
        // Zugang mit, statt aus ihm abgeleitet zu werden.
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx.clone());
            ns.host_last_seen = now_ms();
        }
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAuth {
                devices: vec![relay_proto::TlAuthDevice {
                    id: "tl-3f2a".to_string(),
                    token: "geheim".to_string(),
                }],
            },
            &host_tx,
        )
        .await;
        assert_eq!(
            tl_device_id(&broker, "ns1", "geheim").await.as_deref(),
            Some("tl-3f2a")
        );
        assert!(tl_device_id(&broker, "ns1", "falsch").await.is_none());

        // Und im weitergeleiteten Kommando steht genau diese Kennung.
        let broker2 = broker.clone();
        tokio::spawn(async move {
            tl_forward(
                &broker2,
                "ns1",
                "tl-3f2a".to_string(),
                "op".to_string(),
                1,
                TlAction::SetAutoAssign { enabled: false },
            )
            .await
        });
        let msg = tokio::time::timeout(Duration::from_secs(2), host_rx.recv())
            .await
            .expect("kein Kommando")
            .expect("Kanal zu");
        let Message::Text(t) = msg else {
            panic!("Text erwartet")
        };
        assert!(
            t.as_str().contains("tl-3f2a"),
            "die Kennung fehlt: {}",
            t.as_str()
        );
        assert!(
            !t.as_str().contains("geheim"),
            "der Zugang darf nie mitreisen: {}",
            t.as_str()
        );
    }

    #[tokio::test]
    async fn a_command_reaches_the_host_and_its_answer_comes_back() {
        let (broker, mut host_rx, host) = broker_with_tl_device("token").await;
        let broker2 = broker.clone();

        // Die Anfrage wartet auf die Quittung — wie bei der Ergebnismeldung.
        let warten = tokio::spawn(async move {
            tl_forward(
                &broker2,
                "ns1",
                "dev-1".to_string(),
                "op-1".to_string(),
                12,
                TlAction::FreeCourt {
                    court_id: 3,
                    expect: CourtExpectation::Any,
                },
            )
            .await
        });

        // Der Host bekommt das Kommando mitsamt Kennungen.
        let msg = tokio::time::timeout(Duration::from_secs(2), host_rx.recv())
            .await
            .expect("kein Kommando beim Host")
            .expect("Kanal zu");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let frame: RelayFrame = serde_json::from_str(t.as_str()).unwrap();
        let RelayFrame::TlCommand {
            req_id,
            device_id,
            op_id,
            view_rev,
            ..
        } = frame
        else {
            panic!("TlCommand erwartet")
        };
        assert_eq!(device_id, "dev-1");
        assert_eq!(op_id, "op-1", "der Doppelschutz muss mitreisen");
        assert_eq!(view_rev, 12);

        // Und seine Antwort löst die wartende Anfrage.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TlAck {
                req_id,
                response: TlResponse::ok(13),
            },
            &host,
        )
        .await;
        let antwort = warten.await.unwrap();
        assert!(antwort.ok);
        assert_eq!(antwort.state_rev, 13);
    }

    #[tokio::test]
    async fn timeline_request_reaches_host_and_answer_reaches_caller() {
        // Punktverlauf on-demand (AK-5): gleiche Mechanik wie das
        // TL-Kommando — Anfrage zum Host, Antwort zurück, nichts im Relay.
        let (broker, mut host_rx, host) = broker_with_tl_device("token").await;
        let broker2 = broker.clone();

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer token".parse().unwrap());
        let warten =
            tokio::spawn(async move { tl_timeline_route(State(broker2), headers, Path(42)).await });

        let msg = tokio::time::timeout(Duration::from_secs(2), host_rx.recv())
            .await
            .expect("keine Anfrage beim Host")
            .expect("Kanal zu");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let frame: RelayFrame = serde_json::from_str(t.as_str()).unwrap();
        let RelayFrame::TimelineRequest { req_id, match_id } = frame else {
            panic!("TimelineRequest erwartet, war: {frame:?}")
        };
        assert_eq!(match_id, 42);

        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TimelineData {
                req_id,
                found: true,
                json: r#"{"sets":[]}"#.to_string(),
            },
            &host,
        )
        .await;
        let antwort = warten.await.unwrap();
        assert_eq!(antwort.status(), StatusCode::OK);
        let body = axum::body::to_bytes(antwort.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert_eq!(&body[..], br#"{"sets":[]}"#);
    }

    #[tokio::test]
    async fn monitor_upload_exposes_bar_ads_and_logo() {
        use base64::Engine;
        const NS: &str = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        const NS2: &str = "b1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let b64 = |s: &[u8]| base64::engine::general_purpose::STANDARD.encode(s);
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host, _hrx) = mpsc::unbounded_channel();
        register_host(&broker, NS, &host).await;

        let upload = relay_proto::MonitorUpload {
            config: relay_proto::MonitorConfig::default(),
            tournament_name: "Test-Cup".into(),
            ads: vec![
                // Index 0 in der Leiste, Index 1 nur Vollbild-Rotation.
                relay_proto::AdUpload {
                    content_type: "image/png".into(),
                    data: b64(b"bar-bild"),
                    in_bar: true,
                },
                relay_proto::AdUpload {
                    content_type: "image/jpeg".into(),
                    data: b64(b"voll-bild"),
                    in_bar: false,
                },
            ],
            call_timer: relay_proto::CallTimerView::default(),
            logo: Some(relay_proto::LogoUpload {
                content_type: "image/png".into(),
                data: b64(b"logo-bytes"),
            }),
        };
        let resp = monitor_upload(State(broker.clone()), Path(NS.into()), axum::Json(upload))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        // /{ns}/info/ad/state: nur der in_bar-Index, hasLogo true.
        let state = ad_bar_state(State(broker.clone()), Path(NS.into()))
            .await
            .into_response();
        assert_eq!(state.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(state.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["barAds"], serde_json::json!(["0"]));
        // `ads` = volle Rotationsliste (beide Indizes), für ad.html im Cloud.
        assert_eq!(json["ads"], serde_json::json!(["0", "1"]));
        assert_eq!(json["hasLogo"], serde_json::json!(true));

        // /{ns}/info/logo: liefert die Logo-Bytes.
        let logo = tournament_logo(State(broker.clone()), Path(NS.into()))
            .await
            .into_response();
        assert_eq!(logo.status(), StatusCode::OK);
        let logo_bytes = axum::body::to_bytes(logo.into_body(), 4096).await.unwrap();
        assert_eq!(&logo_bytes[..], b"logo-bytes");

        // Unbekannter Namespace ohne Logo → 404 (sauberer onerror-Rückfall).
        let miss = tournament_logo(State(broker.clone()), Path(NS2.into()))
            .await
            .into_response();
        assert_eq!(miss.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cloud_monitor_redirects_non_court_targets() {
        // Regression: Cloud-Monitore mit einem Info-/Werbe-Ziel bekamen früher
        // keinen Redirect (nur CourtID reiste) → blieben „unzugewiesen" (Logo).
        const NS: &str = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host, _hrx) = mpsc::unbounded_channel();
        register_host(&broker, NS, &host).await;

        let mut targets = std::collections::HashMap::new();
        targets.insert(
            "pi-prep".to_string(),
            relay_proto::MonitorTarget::InfoPreparation,
        );
        targets.insert("pi-court".to_string(), relay_proto::MonitorTarget::court(5));
        // Werbung (Rotation) wird ebenfalls serviert → leitet um.
        targets.insert("pi-ad".to_string(), relay_proto::MonitorTarget::AdRotation);
        // Court-Übersicht wird jetzt ebenfalls serviert → leitet um.
        targets.insert(
            "pi-overview".to_string(),
            relay_proto::MonitorTarget::InfoOverview { hall: None },
        );
        // Siegerehrung: (noch) nicht servierte Sicht → darf NICHT umleiten (sonst
        // 404-Sackgasse) → bleibt „unzugewiesen", pollt weiter, heilt sich.
        targets.insert(
            "pi-winners".to_string(),
            relay_proto::MonitorTarget::InfoWinners { rank: None },
        );
        let control = relay_proto::MonitorControl {
            assignments: std::collections::HashMap::new(),
            targets,
            commands: std::collections::HashMap::new(),
        };
        let up =
            monitor_control_upload(State(broker.clone()), Path(NS.into()), axum::Json(control))
                .await
                .into_response();
        assert_eq!(up.status(), StatusCode::OK);

        // Info-Ziel → Redirect gesetzt, nicht mehr „unassigned".
        let read_state = |dev: &str| {
            let b = broker.clone();
            let d = dev.to_string();
            async move {
                let r = monitor_device_state(
                    State(b),
                    Path(NS.into()),
                    axum::extract::Query(DeviceQuery { device: d }),
                )
                .await
                .into_response();
                let bytes = axum::body::to_bytes(r.into_body(), 8192).await.unwrap();
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
            }
        };
        let prep = read_state("pi-prep").await;
        assert_eq!(prep["redirectTo"], serde_json::json!("/info/preparation"));
        assert_eq!(prep["unassigned"], serde_json::json!(false));

        // Werbe-Rotation → Redirect auf die Werbe-Seite.
        let ad = read_state("pi-ad").await;
        assert_eq!(
            ad["redirectTo"],
            serde_json::json!("/info/ad?mode=rotation")
        );
        assert_eq!(ad["unassigned"], serde_json::json!(false));

        // Court-Ziel → kein Redirect, courtId gesetzt.
        let court = read_state("pi-court").await;
        assert_eq!(court["courtId"], serde_json::json!(5));
        assert!(court["redirectTo"].is_null());

        // Court-Übersicht → Redirect auf die Übersichts-Seite.
        let ov = read_state("pi-overview").await;
        assert_eq!(ov["redirectTo"], serde_json::json!("/info/overview"));
        assert_eq!(ov["unassigned"], serde_json::json!(false));

        // Noch nicht serviertes Ziel (Siegerehrung) → KEIN Redirect, unassigned
        // (Selbstheilung), statt 404-Sackgasse.
        let win = read_state("pi-winners").await;
        assert!(win["redirectTo"].is_null());
        assert_eq!(win["unassigned"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn cloud_overview_health_lists_courts_with_match_and_score() {
        // Die Cloud-Court-Übersicht (`overview.html`) pollt `/{ns}/health`; der
        // Relay baut daraus je Feld die Anzeige-Form aus dem, was der Host schon
        // pusht (Feldliste, Match, Satzstand, 1.-Aufruf-Zeit).
        const NS: &str = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let broker = Broker::new("x".into());
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry(NS.into()).or_insert_with(Namespace::new);
            ns.courts = vec![
                relay_proto::CourtBrief {
                    id: 101,
                    label: "1".into(),
                    hall: "Halle 1".into(),
                },
                // Feld ohne Match → leere Anzeige, aber gelistet.
                relay_proto::CourtBrief {
                    id: 102,
                    label: "2".into(),
                    hall: "Halle 1".into(),
                },
            ];
            ns.court_matches.insert(101, brief(7));
            ns.court_scores.insert(101, vec![SetAb { a: 21, b: 15 }]);
            ns.court_on_court_since.insert(101, 1000);
            ns.monitor = Some(MonitorBundle {
                config: MonitorConfig::default(),
                tournament_name: String::new(),
                ads: Vec::new(),
                call_timer: relay_proto::CallTimerView {
                    enabled: true,
                    second_call_minutes: 2.0,
                    third_call_minutes: 5.0,
                },
                logo: None,
            });
        }
        register_host(&broker, NS, &host).await;

        let resp = overview_health(State(broker.clone()), Path(NS.into()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let courts = v["courts"].as_array().unwrap();
        assert_eq!(courts.len(), 2);
        // Feld mit Match: Kernfelder vollständig.
        let c0 = &courts[0];
        assert_eq!(c0["court_id"], serde_json::json!(101));
        assert_eq!(c0["court"], serde_json::json!("1"));
        assert_eq!(c0["location"], serde_json::json!("Halle 1"));
        assert_eq!(c0["match_id"], serde_json::json!(7));
        assert_eq!(c0["team1"], serde_json::json!(["Anna"]));
        // Länderflaggen: Nationalitäten parallel zu den Namen (aus PlayerBrief).
        assert_eq!(c0["team1_nationalities"], serde_json::json!(["GER"]));
        assert_eq!(c0["sets"][0]["a"], serde_json::json!(21));
        assert_eq!(c0["on_court_since_ms"], serde_json::json!(1000));
        // Im Cloud (noch) nicht verfügbar → konservativ weggelassen.
        assert!(c0["serving_team"].is_null());
        assert_eq!(c0["injury"], serde_json::json!(false));
        // Feld ohne Match: gelistet, aber leer.
        let c1 = &courts[1];
        assert_eq!(c1["court_id"], serde_json::json!(102));
        assert_eq!(c1["match_id"], serde_json::json!(0));
        assert!(c1["on_court_since_ms"].is_null());
        // Aufruf-Timer in camelCase, wie die LAN-`/health` ihn liefert.
        assert_eq!(v["callTimer"]["enabled"], serde_json::json!(true));
        assert_eq!(v["callTimer"]["secondCallMinutes"], serde_json::json!(2.0));

        // Unbekannter Namespace → 404, kein Datenleck.
        let miss = overview_health(State(broker.clone()), Path("nope".into()))
            .await
            .into_response();
        assert_eq!(miss.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn timeline_without_recording_yields_404_and_foreign_token_is_rejected() {
        let (broker, mut host_rx, host) = broker_with_tl_device("token").await;

        // Fremder Zugang → zurückhaltende 503 (wie tl_state_route: ein
        // unbekannter Zugang ist vom kurz abwesenden Turnier-PC nicht zu
        // unterscheiden und darf nie wie „entzogen" aussehen), und der
        // Host wird gar nicht erst behelligt.
        let mut falsch = axum::http::HeaderMap::new();
        falsch.insert(header::AUTHORIZATION, "Bearer falsch".parse().unwrap());
        let antwort = tl_timeline_route(State(broker.clone()), falsch, Path(42)).await;
        assert_eq!(antwort.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(host_rx.try_recv().is_err(), "kein Frame beim Host");

        // Papier-Spiel: der Host meldet found:false → ehrliches 404.
        let broker2 = broker.clone();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer token".parse().unwrap());
        let warten =
            tokio::spawn(async move { tl_timeline_route(State(broker2), headers, Path(43)).await });
        let msg = tokio::time::timeout(Duration::from_secs(2), host_rx.recv())
            .await
            .expect("keine Anfrage beim Host")
            .expect("Kanal zu");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let RelayFrame::TimelineRequest { req_id, .. } =
            serde_json::from_str::<RelayFrame>(t.as_str()).unwrap()
        else {
            panic!("TimelineRequest erwartet")
        };
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::TimelineData {
                req_id,
                found: false,
                json: String::new(),
            },
            &host,
        )
        .await;
        assert_eq!(warten.await.unwrap().status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn without_a_tournament_pc_the_page_gets_a_clear_answer() {
        // Kein Warten ins Leere: Die Seite soll sagen können, woran es liegt.
        let broker = Broker::new("https://example.test/bts-relay".into());
        let antwort = tl_forward(
            &broker,
            "ns1",
            "dev-1".to_string(),
            "op-1".to_string(),
            0,
            TlAction::SetAutoAssign { enabled: true },
        )
        .await;
        assert!(!antwort.ok);
        assert_eq!(antwort.code, Some(TlErrorCode::HostOffline));
    }

    #[tokio::test]
    async fn the_ninth_device_is_turned_away_and_a_stale_slot_frees_up() {
        // Ein Turnier hat eine Handvoll Helfer. Die Grenze schützt den
        // Turnier-PC davor, von Dutzenden Browsern abgefragt zu werden —
        // aber ein Gerät, das nur den Tab geschlossen hat, darf seinen Platz
        // nicht auf Dauer blockieren.
        let broker = Broker::new("https://example.test/bts-relay".into());
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            let (tx, _rx) = mpsc::unbounded_channel();
            ns.host = Some(tx);
            for i in 0..9 {
                ns.tl_tokens.insert(format!("token-{i}"), format!("tl-{i}"));
            }
            for i in 0..MAX_TL_DEVICES {
                assert!(
                    claim_tl_slot(ns, &format!("token-{i}"), 100_000),
                    "Gerät {i} passt noch"
                );
            }
            assert!(
                !claim_tl_slot(ns, "token-8", 100_000),
                "das neunte wird abgewiesen"
            );
            // Dasselbe Gerät noch einmal: kein neuer Platz nötig.
            assert!(claim_tl_slot(ns, "token-0", 100_001));
            // Eine Minute später sind die stummen Plätze frei.
            assert!(claim_tl_slot(ns, "token-8", 100_000 + TL_DEVICE_TTL_MS + 1));
        }
    }

    #[tokio::test]
    async fn host_match_assigned_reaches_the_courts_tablet() {
        let (broker, mut rx, host) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
                match_brief: brief(7),
                on_court_since_ms: None,
            },
            &host,
        )
        .await;
        let msg = rx.try_recv().expect("Tablet bekommt das Frame");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: ServerMsg = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            parsed,
            ServerMsg::MatchAssigned {
                match_brief: brief(7)
            }
        );
    }

    #[tokio::test]
    async fn host_frame_for_unknown_court_is_dropped() {
        let (broker, mut rx, host) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchCleared {
                court_id: 999,
                court_label: "Feld 99".into(),
                hall: String::new(),
            },
            &host,
        )
        .await;
        assert!(rx.try_recv().is_err(), "fremdes Feld bekommt nichts");
    }

    /// Mehr-Hallen-Regression: zwei Felder heißen beide „1", haben aber
    /// verschiedene CourtIDs. Ein `MatchAssigned` für das eine Feld darf
    /// nur dessen Tablet erreichen, nicht das des gleichnamigen Felds.
    #[tokio::test]
    async fn host_frame_routes_by_court_id_not_name() {
        let broker = Broker::new("x".into());
        let (tx_a, mut rx_a) = mpsc::unbounded_channel();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel();
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tx_a); // Halle 1 · Feld „1"
            ns.tablets.insert(401, tx_b); // Halle 2 · Feld „1"
        }
        register_host(&broker, "ns1", &host).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 401,
                court_label: "1".into(),
                hall: String::new(),
                match_brief: brief(7),
                on_court_since_ms: None,
            },
            &host,
        )
        .await;
        // Nur das Tablet von Feld 401 bekommt das Match.
        assert!(rx_b.try_recv().is_ok(), "Feld 401 bekommt das Match");
        assert!(rx_a.try_recv().is_err(), "Feld 101 bleibt unberührt");
    }

    #[tokio::test]
    async fn reassign_same_match_keeps_the_score() {
        let broker = Broker::new("x".into());
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.court_matches.insert(101, brief(7));
            ns.court_scores.insert(101, vec![SetAb { a: 21, b: 15 }]);
        }
        register_host(&broker, "ns1", &host).await;
        // Erneutes MatchAssigned fürs SELBE Match (Tablet-Reconnect) →
        // der gemerkte Satzstand bleibt erhalten.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
                match_brief: brief(7),
                on_court_since_ms: Some(1000),
            },
            &host,
        )
        .await;
        assert_eq!(
            broker.namespaces.lock().await["ns1"].court_scores.get(&101),
            Some(&vec![SetAb { a: 21, b: 15 }])
        );
        // Aufruf-Timer: der Host-Stempel wird übernommen (auch bei Reconnect).
        assert_eq!(
            broker.namespaces.lock().await["ns1"]
                .court_on_court_since
                .get(&101),
            Some(&1000)
        );
        // Echter Match-Wechsel → Satzstand zurückgesetzt, neuer Aufruf-Stempel.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchAssigned {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
                match_brief: brief(9),
                on_court_since_ms: Some(2000),
            },
            &host,
        )
        .await;
        let ns = broker.namespaces.lock().await;
        assert!(!ns["ns1"].court_scores.contains_key(&101));
        assert_eq!(ns["ns1"].court_on_court_since.get(&101), Some(&2000));
    }

    #[tokio::test]
    async fn result_ack_resolves_the_pending_request() {
        let broker = Broker::new("x".into());
        let (ack_tx, ack_rx) = oneshot::channel();
        let (host, _host_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.pending.insert(5, ack_tx);
        }
        register_host(&broker, "ns1", &host).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::ResultAck {
                req_id: 5,
                ok: true,
                error: None,
            },
            &host,
        )
        .await;
        assert_eq!(ack_rx.await.unwrap(), ResultResponse::ok());
    }

    #[tokio::test]
    async fn score_from_tablet_is_forwarded_to_the_host() {
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        // Der Sender muss als aktives Tablet des Felds eingetragen sein —
        // nur der aktuelle Halter darf Scores liefern (is_holder).
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, tablet_tx.clone());
        }
        forward_score(&broker, "ns1", 101, 11, 9, vec![], 0, &tablet_tx).await;
        let msg = host_rx.try_recv().expect("Host bekommt den Score");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: RelayFrame = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            parsed,
            RelayFrame::ScoreUpdate {
                court_id: 101,
                court_label: String::new(),
                score_a: 11,
                score_b: 9,
                sets_history: vec![],
                match_id: 0,
            }
        );
    }

    #[tokio::test]
    async fn rally_is_forwarded_verbatim_and_oversized_sync_dropped() {
        // Punktverlauf (ADR 0014): Der Relay ist Briefträger — ein Rally
        // des aktiven Halters mit passender matchId geht 1:1 an den Host;
        // ein Sync jenseits der geteilten Deckel wird verworfen.
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, tablet_tx.clone());
            ns.court_matches.insert(101, brief(9));
        }
        forward_rally(&broker, "ns1", 101, 9, 1, 1, "A".into(), 1, 0, &tablet_tx).await;
        let Message::Text(t) = host_rx.try_recv().expect("Rally erreicht den Host") else {
            panic!("Text-Frame erwartet")
        };
        let parsed: RelayFrame = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            parsed,
            RelayFrame::Rally {
                court_id: 101,
                match_id: 9,
                set: 1,
                n: 1,
                winner: "A".into(),
                score_a: 1,
                score_b: 0,
            }
        );
        // Fremdes Match (Stale, HM-03) → verworfen.
        forward_rally(&broker, "ns1", 101, 7, 1, 2, "A".into(), 2, 0, &tablet_tx).await;
        assert!(host_rx.try_recv().is_err(), "Stale-Rally verworfen");
        // Gültiger Sync fließt …
        let timeline = relay_proto::MatchTimeline {
            sets: vec![relay_proto::TimelineSet {
                start_a: 0,
                start_b: 0,
                points: "AB".into(),
            }],
            ..Default::default()
        };
        forward_rally_sync(&broker, "ns1", 101, 9, timeline, &tablet_tx).await;
        assert!(
            host_rx.try_recv().is_ok(),
            "gültiger Sync erreicht den Host"
        );
        // … ein überlanger nicht (Deckel MAX_RALLIES_PER_SET greift über
        // is_valid, bevor irgendetwas transportiert wird).
        let zu_lang = relay_proto::MatchTimeline {
            sets: vec![relay_proto::TimelineSet {
                start_a: 0,
                start_b: 0,
                points: "A".repeat(relay_proto::MAX_RALLIES_PER_SET + 1),
            }],
            ..Default::default()
        };
        forward_rally_sync(&broker, "ns1", 101, 9, zu_lang, &tablet_tx).await;
        assert!(host_rx.try_recv().is_err(), "überlanger Sync verworfen");
    }

    #[tokio::test]
    async fn score_for_foreign_match_is_dropped() {
        // Stale-Filter (A4, Turnier-Befund HM-03): Das Feld hat Match 9,
        // ein hängengebliebenes Tablet meldet noch Match 7 → Score wird
        // weder gecacht noch an den Host weitergereicht. Mit passender
        // (oder ohne) matchId fließt er normal.
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, tablet_tx.clone());
            ns.court_matches.insert(101, brief(9));
        }
        forward_score(&broker, "ns1", 101, 14, 16, vec![], 7, &tablet_tx).await;
        assert!(host_rx.try_recv().is_err(), "fremder Match-Score verworfen");
        assert!(
            !broker.namespaces.lock().await["ns1"]
                .court_scores
                .contains_key(&101),
            "Cache bleibt leer"
        );
        // Passende matchId → normal verarbeitet.
        forward_score(&broker, "ns1", 101, 1, 0, vec![], 9, &tablet_tx).await;
        assert!(host_rx.try_recv().is_ok(), "passender Score fließt");
        assert!(broker.namespaces.lock().await["ns1"]
            .court_scores
            .contains_key(&101));
    }

    #[tokio::test]
    async fn state_sync_for_foreign_match_is_dropped() {
        // Stale-Filter (A4): Ein state_sync des ALTEN Matches darf den
        // beim Match-Wechsel geleerten court_state nicht wieder befüllen.
        let broker = Broker::new("x".into());
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tablet_tx.clone());
            ns.court_matches.insert(101, brief(9));
        }
        let stale = r#"{"match":{"matchId":7},"finished":false}"#.to_string();
        store_court_state(&broker, "ns1", 101, stale, &tablet_tx).await;
        assert!(
            !broker.namespaces.lock().await["ns1"]
                .court_state
                .contains_key(&101),
            "alter Match-State verworfen"
        );
        let current = r#"{"match":{"matchId":9},"finished":false}"#.to_string();
        store_court_state(&broker, "ns1", 101, current, &tablet_tx).await;
        assert!(broker.namespaces.lock().await["ns1"]
            .court_state
            .contains_key(&101));
    }

    #[tokio::test]
    async fn score_after_match_cleared_is_dropped() {
        // A4-Review-Befund: Nach MatchCleared (Feld frei, kein Eintrag in
        // court_matches) ist ein Frame MIT Match-ID ein Nachzügler des
        // alten Spiels — er darf den geleerten Cache nicht neu befüllen.
        // Nur matchId 0 (alte Tablet-Seite) läuft weiter durch.
        let broker = Broker::new("x".into());
        let (tablet_tx, _tablet_rx) = mpsc::unbounded_channel();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.tablets.insert(101, tablet_tx.clone());
            // KEIN court_matches-Eintrag — wie nach MatchCleared.
        }
        forward_score(&broker, "ns1", 101, 21, 15, vec![], 7, &tablet_tx).await;
        let stale = r#"{"match":{"matchId":7}}"#.to_string();
        store_court_state(&broker, "ns1", 101, stale, &tablet_tx).await;
        let map = broker.namespaces.lock().await;
        assert!(!map["ns1"].court_scores.contains_key(&101));
        assert!(!map["ns1"].court_state.contains_key(&101));
    }

    #[test]
    fn second_host_for_a_namespace_is_refused_while_first_is_live() {
        // R4: Ein LEBENDIGER Host wird nie verdrängt — der zweite
        // Verbindungsversuch (z. B. versehentlich zweiter Master mit
        // derselben install_id) wird abgewiesen.
        let mut ns = Namespace::new();
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();
        assert!(matches!(
            try_claim_host(&mut ns, &tx1, 1_000_000),
            HostClaim::Accepted { superseded: false }
        ));
        // 5 s später (Host hat gerade gepongt): Abweisung.
        ns.host_last_seen = 1_000_000;
        assert!(matches!(
            try_claim_host(&mut ns, &tx2, 1_005_000),
            HostClaim::Refused
        ));
        assert!(ns.host.as_ref().unwrap().same_channel(&tx1));
    }

    #[test]
    fn silent_host_is_superseded_after_stale_timeout() {
        // Zombie-Ablösung (Turnier-Befund 19.07.: tote TCP-Verbindung
        // hielt den Slot 17 Minuten): Ist der Inhaber ≥ HOST_STALE stumm,
        // übernimmt die neue Verbindung.
        let mut ns = Namespace::new();
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();
        try_claim_host(&mut ns, &tx1, 1_000_000);
        let stale_ms = HOST_STALE.as_millis() as u64;
        // 1 ms UNTER der Schwelle: noch abgewiesen (Grenze ist `>=`).
        assert!(matches!(
            try_claim_host(&mut ns, &tx2, 1_000_000 + stale_ms - 1),
            HostClaim::Refused
        ));
        // Genau an der Schwelle: Übernahme.
        assert!(matches!(
            try_claim_host(&mut ns, &tx2, 1_000_000 + stale_ms),
            HostClaim::Accepted { superseded: true }
        ));
        assert!(
            ns.host.as_ref().unwrap().same_channel(&tx2),
            "neuer Host hält den Slot"
        );
    }

    #[test]
    fn superseded_connection_does_not_release_the_new_hosts_slot() {
        // Der wichtigste Korrektheits-Baustein der Ablösung: Stirbt die
        // verdrängte Alt-Verbindung SPÄTER, darf ihr Aufräumen den Slot
        // des neuen Hosts nicht leeren.
        let mut ns = Namespace::new();
        let (old_tx, _old_rx) = mpsc::unbounded_channel();
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        try_claim_host(&mut ns, &old_tx, 0);
        let stale_ms = HOST_STALE.as_millis() as u64;
        try_claim_host(&mut ns, &new_tx, stale_ms);
        // Alt-Verbindung stirbt und räumt auf → Slot bleibt beim neuen Host.
        assert!(!release_host_slot(&mut ns, &old_tx));
        assert!(ns.host.as_ref().unwrap().same_channel(&new_tx));
        // Der echte Inhaber gibt den Slot dagegen frei.
        assert!(release_host_slot(&mut ns, &new_tx));
        assert!(ns.host.is_none());
    }

    #[tokio::test]
    async fn frames_from_superseded_host_are_ignored() {
        // Eine verdrängte Alt-Verbindung, die wieder erwacht, darf den
        // Namespace-Zustand nicht mehr verändern (Sender-Guard).
        let (broker, _rx, host) = broker_with_tablet(101).await;
        let (old_host, _old_rx) = mpsc::unbounded_channel();
        let accepted = handle_host_frame(
            &broker,
            "ns1",
            HostFrame::MatchCleared {
                court_id: 101,
                court_label: "Feld 1".into(),
                hall: String::new(),
            },
            &old_host,
        )
        .await;
        assert!(!accepted, "fremder/abgelöster Sender wird abgewiesen");
        // Der eingetragene Host bleibt unangetastet.
        assert!(broker.namespaces.lock().await["ns1"]
            .host
            .as_ref()
            .unwrap()
            .same_channel(&host));
    }

    #[test]
    fn host_frame_stamps_liveness() {
        // Konstanten-Beziehung der Stale-Erkennung: Ein gesunder Host
        // pongt alle HOST_PING — die Übernahme-Schwelle muss deutlich
        // darüber liegen, sonst würde ein lebendiger Host verdrängt.
        assert!(HOST_STALE >= HOST_PING * 3);
    }

    #[test]
    fn tablet_stale_invariant() {
        // Analog zum Host: Ein gesundes Tablet pongt alle TABLET_PING —
        // die Stale-Schwelle muss ≥ 3× darüber liegen (3 verpasste Pongs),
        // sonst würde ein lebendes (auch backgroundetes) Feld fälschlich
        // gedroppt.
        assert!(TABLET_STALE >= TABLET_PING * 3);
    }

    #[test]
    fn is_stale_grenzfaelle() {
        use tokio::time::{Duration, Instant};
        let t0 = Instant::now();
        let threshold = Duration::from_secs(15);
        // Knapp unter der Schwelle → noch lebendig (Grenze ist `>=`).
        let almost = t0 + threshold - Duration::from_millis(1);
        assert!(!is_stale(t0, almost, threshold));
        // Exakt an der Schwelle → tot.
        let exact = t0 + threshold;
        assert!(is_stale(t0, exact, threshold));
        // Deutlich darüber → tot.
        assert!(is_stale(
            t0,
            t0 + threshold + Duration::from_secs(5),
            threshold
        ));
    }

    #[test]
    fn gesundes_tablet_nicht_stale() {
        use tokio::time::{Duration, Instant};
        // Ein Tablet, dessen Stempel je Ping/Pong frisch bleibt (hier: erst
        // 4 s alt bei TABLET_STALE = 15 s), wird nie gedroppt.
        let now = Instant::now();
        let last = now - Duration::from_secs(4);
        assert!(!is_stale(last, now, TABLET_STALE));
    }

    /// Reconnect-Erkennung: Meldet sich DASSELBE Gerät erneut an einem
    /// belegten Feld, löst es seine tote Vorgänger-Session nahtlos ab —
    /// kein „Feld belegt" fürs eigene Gerät (Turnier-Feedback 18.07.2026:
    /// Tablet verlor nach Netz-Aussetzer den Stand an sein „fremdes" Ich).
    #[tokio::test]
    async fn same_device_reconnect_replaces_old_session() {
        let (broker, mut old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.tablet_devices.insert(101, "dev-x".into());
        }
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "dev-x", &new_tx).await;
        assert!(
            matches!(res, AttachResult::Active),
            "eigenes Gerät kommt sofort rein"
        );
        // Die alte Session wird gesperrt, damit sie nicht weiterzählt.
        let msg = old_rx
            .try_recv()
            .expect("alte Session bekommt SessionSuperseded");
        let Message::Text(t) = msg else {
            panic!("Text-Frame erwartet")
        };
        let parsed: ServerMsg = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(parsed, ServerMsg::SessionSuperseded);
    }

    /// Ein FREMDES Gerät sieht weiterhin „belegt" (Übernehmen-Dialog).
    #[tokio::test]
    async fn foreign_device_still_sees_occupied() {
        let (broker, mut old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            map.get_mut("ns1")
                .unwrap()
                .tablet_devices
                .insert(101, "dev-x".into());
        }
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "dev-anders", &new_tx).await;
        assert!(
            matches!(res, AttachResult::Occupied),
            "fremdes Gerät bleibt draußen"
        );
        assert!(old_rx.try_recv().is_err(), "alte Session bleibt aktiv");
    }

    /// A2 / ADR 0017: Wahrheitstabelle des Cloud-Pendants zu
    /// `reconnect_decision`. Autorität nach Slot-Halter (`tablet_devices`):
    /// gleicher Eintrag → Rückkehrer ist autoritativ; fremder Eintrag hängt am
    /// „hat gezählt".
    #[test]
    fn relay_reconnect_authoritative_truth_table() {
        // Feld frei → Rückkehrer ist die Wahrheit.
        assert!(relay_reconnect_authoritative("dev-a", None, false, false));
        // Gleicher tablet_devices-Eintrag (eigener Reclaim) → autoritativ,
        // auch wenn schon gezählt wurde.
        assert!(relay_reconnect_authoritative(
            "dev-a",
            Some("dev-a"),
            true,
            false
        ));
        // Fremder Halter, hat gezählt → Rückkehrer tritt zurück.
        assert!(!relay_reconnect_authoritative(
            "dev-a",
            Some("dev-b"),
            true,
            false
        ));
        // Fremder Halter, hat NICHT gezählt → Rückkehrer gewinnt.
        assert!(relay_reconnect_authoritative(
            "dev-a",
            Some("dev-b"),
            false,
            false
        ));
        // Finalisiert → nie überbügeln (StandDown), egal wer hält.
        assert!(!relay_reconnect_authoritative(
            "dev-a",
            Some("dev-a"),
            false,
            true
        ));
    }

    /// A2 / ADR 0017 (Wire-Ebene): Beim Reconnect DESSELBEN Geräts liefert der
    /// Relay ein `StateRestore` mit `authoritative=true` — der Reclaimer setzt
    /// seinen lokalen Stand durch.
    #[tokio::test]
    async fn same_device_reconnect_state_restore_is_authoritative() {
        let (broker, _old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.tablet_devices.insert(101, "dev-x".into());
            ns.court_state.insert(101, "{\"score\":\"7:5\"}".into());
        }
        let (new_tx, mut new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "dev-x", &new_tx).await;
        assert!(matches!(res, AttachResult::Active));
        // Unter den gesendeten Frames muss das StateRestore mit
        // authoritative=true sein.
        let mut saw_authoritative = false;
        while let Ok(Message::Text(t)) = new_rx.try_recv() {
            if let Ok(ServerMsg::StateRestore { authoritative, .. }) =
                serde_json::from_str::<ServerMsg>(t.as_str())
            {
                assert!(authoritative, "eigener Reclaim ist autoritativ");
                saw_authoritative = true;
            }
        }
        assert!(saw_authoritative, "StateRestore wurde gesendet");
    }

    /// A2 / ADR 0017 (Wire-Ebene): `ownership_active` im `StateRestore`
    /// spiegelt den durchgereichten Legacy-Schalter. Default (kein Legacy) →
    /// `ownership_active=true` (Tablet folgt der Autorität); Legacy an →
    /// `ownership_active=false` (Tablet nutzt seine rev-Logik).
    #[tokio::test]
    async fn state_restore_ownership_active_reflects_legacy_flag() {
        // Default: Ownership aktiv.
        let (broker, _old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.tablet_devices.insert(101, "dev-x".into());
            ns.court_state.insert(101, "{\"score\":\"7:5\"}".into());
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        attach_tablet(&broker, "ns1", 101, "dev-x", &tx).await;
        let mut saw = false;
        while let Ok(Message::Text(t)) = rx.try_recv() {
            if let Ok(ServerMsg::StateRestore {
                ownership_active, ..
            }) = serde_json::from_str::<ServerMsg>(t.as_str())
            {
                assert!(ownership_active, "Default → Ownership aktiv");
                saw = true;
            }
        }
        assert!(saw, "StateRestore wurde gesendet");

        // Legacy an: der Relay meldet ownership_active=false.
        let (broker2, _old_rx2, _host2) = broker_with_tablet(101).await;
        {
            let mut map = broker2.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.reconnect_legacy_rev = true;
            ns.tablet_devices.insert(101, "dev-x".into());
            ns.court_state.insert(101, "{\"score\":\"7:5\"}".into());
        }
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        attach_tablet(&broker2, "ns1", 101, "dev-x", &tx2).await;
        let mut saw2 = false;
        while let Ok(Message::Text(t)) = rx2.try_recv() {
            if let Ok(ServerMsg::StateRestore {
                ownership_active, ..
            }) = serde_json::from_str::<ServerMsg>(t.as_str())
            {
                assert!(!ownership_active, "Legacy → Ownership inaktiv (rev)");
                saw2 = true;
            }
        }
        assert!(saw2, "StateRestore wurde gesendet");
    }

    /// A2 / ADR 0017 (Wire-Ebene): Ein `HostFrame::Courts` mit
    /// `reconnect_legacy_rev=true` schaltet den Namespace auf Legacy — der
    /// nächste Reconnect meldet dann `ownership_active=false`.
    #[tokio::test]
    async fn courts_frame_sets_namespace_legacy_flag() {
        let (broker, _old_rx, host) = broker_with_tablet(101).await;
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Courts {
                courts: vec![],
                azure_tts: None,
                reconnect_legacy_rev: true,
            },
            &host,
        )
        .await;
        let map = broker.namespaces.lock().await;
        assert!(
            map.get("ns1").unwrap().reconnect_legacy_rev,
            "Courts-Frame hat den Legacy-Schalter übernommen"
        );
    }

    /// A2 / ADR 0017, Regel b (Wire-Ebene): Trägt das zuletzt zugewiesene Match
    /// des Felds `finalized:true` (vom Host im MatchBrief hereingereicht), tritt
    /// selbst der eigene Reclaimer beim Reconnect zurück — `StateRestore` mit
    /// `authoritative=false`, damit das Hand-Ergebnis nicht überbügelt wird.
    #[tokio::test]
    async fn reconnect_stands_down_when_court_match_finalized() {
        let (broker, _old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.tablet_devices.insert(101, "dev-x".into());
            ns.court_state.insert(101, "{\"score\":\"21:19\"}".into());
            // Zuletzt zugewiesenes Match ist in BTP finalisiert.
            let mut finalized = brief(7);
            finalized.finalized = true;
            ns.court_matches.insert(101, finalized);
        }
        let (new_tx, mut new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "dev-x", &new_tx).await;
        assert!(matches!(res, AttachResult::Active));
        let mut saw_restore = false;
        while let Ok(Message::Text(t)) = new_rx.try_recv() {
            if let Ok(ServerMsg::StateRestore { authoritative, .. }) =
                serde_json::from_str::<ServerMsg>(t.as_str())
            {
                assert!(
                    !authoritative,
                    "finalisiertes Match → Rückkehrer tritt zurück"
                );
                saw_restore = true;
            }
        }
        assert!(saw_restore, "StateRestore wurde gesendet");
    }

    /// A2 / ADR 0017 (Wire-Ebene): Eine bewusste Übernahme (`take_over_court`)
    /// liefert `authoritative=false` — das übernehmende Tablet adoptiert den
    /// laufenden Stand statt ihn zu überschreiben.
    #[tokio::test]
    async fn take_over_state_restore_is_not_authoritative() {
        let (broker, _old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.get_mut("ns1").unwrap();
            ns.court_state.insert(101, "{\"score\":\"9:9\"}".into());
        }
        let (new_tx, mut new_rx) = mpsc::unbounded_channel();
        take_over_court(&broker, "ns1", 101, "dev-neu", &new_tx).await;
        let mut saw_restore = false;
        while let Ok(Message::Text(t)) = new_rx.try_recv() {
            if let Ok(ServerMsg::StateRestore { authoritative, .. }) =
                serde_json::from_str::<ServerMsg>(t.as_str())
            {
                assert!(!authoritative, "Übernahme adoptiert, ist nicht autoritativ");
                saw_restore = true;
            }
        }
        assert!(saw_restore, "StateRestore wurde gesendet");
    }

    /// Nachlaufende Frames einer ABGELÖSTEN Session (Reconnect-Reclaim/
    /// Übernahme) dürfen Cache und Host nicht mehr erreichen — sonst kehrt
    /// genau der alte Stand zurück, den die Reconnect-Logik verhindert.
    #[tokio::test]
    async fn superseded_session_frames_are_dropped() {
        let broker = Broker::new("x".into());
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        let (holder_tx, _holder_rx) = mpsc::unbounded_channel();
        let (old_tx, _old_rx) = mpsc::unbounded_channel::<Message>();
        {
            let mut map = broker.namespaces.lock().await;
            let ns = map.entry("ns1".into()).or_insert_with(Namespace::new);
            ns.host = Some(host_tx);
            ns.tablets.insert(101, holder_tx); // aktueller Halter ist ein ANDERER
        }
        forward_score(&broker, "ns1", 101, 3, 1, vec![], 0, &old_tx).await;
        store_court_state(&broker, "ns1", 101, "{\"alt\":true}".into(), &old_tx).await;
        assert!(
            host_rx.try_recv().is_err(),
            "Score der alten Session wird verworfen"
        );
        let map = broker.namespaces.lock().await;
        assert!(
            !map.get("ns1").unwrap().court_state.contains_key(&101),
            "alter Stand landet nicht im Cache"
        );
    }

    fn prepared(match_id: i64, hall: &str) -> relay_proto::PreparedMatch {
        relay_proto::PreparedMatch {
            match_id,
            hall: hall.into(),
            discipline: "mens_singles".into(),
            class_label: "A".into(),
            round_name: "G1".into(),
            team_a: vec![PlayerBrief {
                id: 1,
                name: "Anna Weber".into(),
                nationality: Some("GER".into()),
                club: None,
            }],
            team_b: vec![PlayerBrief {
                id: 2,
                name: "Bea Schulz".into(),
                nationality: None,
                club: None,
            }],
            match_number: Some(101),
            called_at_ms: 1_700_000_000_000,
        }
    }

    /// `HostFrame::Prepared` ersetzt die Liste vollständig; ein leerer Push
    /// leert sie (kein Aufruf mehr offen). Grundlage der Slave-Spielübersicht.
    #[tokio::test]
    async fn prepared_frame_replaces_and_clears_list() {
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host, _hrx) = mpsc::unbounded_channel();
        register_host(&broker, "ns1", &host).await;

        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Prepared {
                prepared: vec![prepared(42, "Halle 1"), prepared(43, "Halle 2")],
            },
            &host,
        )
        .await;
        {
            let map = broker.namespaces.lock().await;
            assert_eq!(map.get("ns1").unwrap().prepared.len(), 2);
        }

        // Zweiter Push mit nur einem Spiel ersetzt die Liste vollständig.
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Prepared {
                prepared: vec![prepared(43, "Halle 2")],
            },
            &host,
        )
        .await;
        {
            let map = broker.namespaces.lock().await;
            let p = &map.get("ns1").unwrap().prepared;
            assert_eq!(p.len(), 1);
            assert_eq!(p[0].match_id, 43);
        }

        // Leerer Push leert die Liste (alle Aufrufe zurückgenommen/aufs Feld).
        handle_host_frame(
            &broker,
            "ns1",
            HostFrame::Prepared { prepared: vec![] },
            &host,
        )
        .await;
        let map = broker.namespaces.lock().await;
        assert!(map.get("ns1").unwrap().prepared.is_empty());
    }

    #[tokio::test]
    async fn cloud_preparation_state_lists_prepared_matches() {
        // Cloud-Info-Monitor „In Vorbereitung": die gepushten aufgerufenen
        // Spiele erscheinen als Kandidaten (mit `call`), sodass preparation.html
        // sie im Cloud-Modus rendern kann.
        const NS: &str = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let broker = Broker::new("https://example.test/bts-relay".into());
        let (host, _hrx) = mpsc::unbounded_channel();
        register_host(&broker, NS, &host).await;
        handle_host_frame(
            &broker,
            NS,
            HostFrame::Prepared {
                prepared: vec![prepared(42, "Halle 1"), prepared(43, "Halle 2")],
            },
            &host,
        )
        .await;

        let resp = preparation_state(State(broker.clone()), Path(NS.into()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let cands = json["candidates"].as_array().unwrap();
        assert_eq!(cands.len(), 2);
        // Jeder Kandidat ist „aufgerufen" (call gesetzt) und trägt die Halle.
        assert_eq!(cands[0]["call"]["hall"], serde_json::json!("Halle 1"));
        assert!(!cands[0]["team1"].as_array().unwrap().is_empty());

        // Unbekannter Namespace → 404 (die Seite zeigt dann „keine Verbindung").
        let miss = preparation_state(State(broker.clone()), Path("nope".into()))
            .await
            .into_response();
        assert_eq!(miss.status(), StatusCode::NOT_FOUND);
    }

    /// Der Hallenfilter der Ansage-Antwort zeigt jeder Halle nur ihre eigenen
    /// aufgerufenen Spiele (leere Halle am Match = überall sichtbar).
    #[test]
    fn prepared_hall_filter_matches_court_rule() {
        let all = [prepared(42, "Halle 1"), prepared(43, "Halle 2"), {
            let mut p = prepared(44, "");
            p.hall = String::new();
            p
        }];
        let for_hall = |hall: &str| -> Vec<i64> {
            all.iter()
                .filter(|p| hall.is_empty() || p.hall.is_empty() || p.hall == hall)
                .map(|p| p.match_id)
                .collect()
        };
        assert_eq!(for_hall("Halle 1"), vec![42, 44]);
        assert_eq!(for_hall("Halle 2"), vec![43, 44]);
        assert_eq!(for_hall(""), vec![42, 43, 44]);
    }

    /// Alte Tablet-Seiten (ohne deviceId) zählen nie als „dasselbe Gerät" —
    /// leere Kennungen dürfen einander nicht matchen.
    #[tokio::test]
    async fn empty_device_id_never_matches() {
        let (broker, _old_rx, _host) = broker_with_tablet(101).await;
        {
            let mut map = broker.namespaces.lock().await;
            map.get_mut("ns1")
                .unwrap()
                .tablet_devices
                .insert(101, String::new());
        }
        let (new_tx, _new_rx) = mpsc::unbounded_channel();
        let res = attach_tablet(&broker, "ns1", 101, "", &new_tx).await;
        assert!(
            matches!(res, AttachResult::Occupied),
            "leer = wie bisher belegt"
        );
    }
}

// ─────────────────────────── Last-/Soak-Harness (Hebel C) ──────────────────
//
// In-Process-Concurrency-Test des Brokers (ADR 0019,
// docs/features/last-soak-test.md). Viele `tokio::spawn`-Tasks treiben die
// Broker-Eintrittspunkte gegen EINEN geteilten `Broker` (Clone) unter echter
// Multi-Thread-Contention am globalen `namespaces`-Mutex.
//
// **BEWIESEN** (nur das, nichts mehr): Cap-Einhaltung, Ownership-End-Invariante
// (genau ein Halter je Court, `T-1` Superseded), Routing/Nudge-Zählung,
// Namespace-Isolation, `is_empty`-Cleanup, „kein Panic + terminiert unter
// Timeout". **NICHT bewiesen** (bewusst, ADR 0019): Socket-`send().await`-
// Backpressure, unbegrenztes Wachstum der `UnboundedSender`-Queue, HTTP-Layer,
// echte Scheduling-Reihenfolge; „Deadlock-Freiheit" wird NICHT behauptet.
//
// Assertions-Doktrin (gegen Flakiness): JEDE End-Assertion läuft NACH dem Join
// ALLER Tasks und prüft NUR reihenfolge-unabhängige Invarianten (Zählung/
// Existenz) — nie „welches Gerät gewann", nie `seq`-Reihenfolge. Die
// Empfangs-Enden (`rx`) bleiben im Test-Hauptscope; nur `tx`-Klone wandern in
// die Tasks/den Broker — sonst siebte `retain(send.is_ok())` einen Sender aus
// und die Zählung würde falsch. Ein umschließender `tokio::time::timeout` gilt
// als Testfehler (Hang). Leichte Variante = CI-Regressionswache; schwere
// `#[ignore]`-Soak-Variante ≥ reales Setup, manuell.
#[cfg(test)]
mod load {
    use super::*;
    use tokio::sync::mpsc::UnboundedReceiver;
    use tokio::task::JoinSet;

    /// Größe eines Lastszenarios. Leichte (CI) und schwere (Soak) Variante
    /// rufen denselben `run_*`-Kern mit unterschiedlichen Werten — so gibt es
    /// keine doppelten Assertionen.
    struct LoadParams {
        /// Anzahl gleichzeitiger Namespaces (Turniere/Installationen).
        namespaces: usize,
        /// Felder je Namespace.
        courts_per_ns: usize,
        /// Gleichzeitig um EIN Feld ringende Tablets (Massen-Connect + der
        /// Reconnect-Sturm nutzen dies als `T`).
        tablets_per_court: usize,
        /// Monitor-Abonnenten je Namespace (Massen-Connect + Cleanup).
        subs_per_ns: usize,
        /// Nudges je Feld im Fan-out-Szenario (`N_c`).
        notifies_per_court: usize,
        /// Ergebnis-`POST`s je Namespace im Ergebnis-Schwall.
        results: usize,
        /// Großzügiges Gesamt-Timeout je Lauf; Ablauf = Hang = Testfehler.
        timeout: Duration,
    }

    /// Kleine, in Sekunden laufende Werte — genug Contention für die
    /// CI-Wache, ohne die Suite zu bremsen. Muss REPRODUZIERBAR grün sein.
    fn light() -> LoadParams {
        LoadParams {
            namespaces: 3,
            courts_per_ns: 4,
            tablets_per_court: 4,
            subs_per_ns: 4,
            notifies_per_court: 6,
            results: 20,
            timeout: Duration::from_secs(10),
        }
    }

    /// Schwere Werte ≥ reales Setup (z. B. eine 18-Feld-Halle) für den
    /// manuellen Soak-Lauf vor dem Turnier.
    fn soak() -> LoadParams {
        LoadParams {
            namespaces: 3,
            courts_per_ns: 18,
            tablets_per_court: 8,
            subs_per_ns: 32,
            notifies_per_court: 40,
            results: 200,
            timeout: Duration::from_secs(60),
        }
    }

    /// Baut eine gültige, im `valid_namespace`-Format (`8-4-4-4-12`) liegende
    /// UUID aus dem Index — deterministisch und kollisionsfrei je `i`.
    fn fresh_ns(i: usize) -> String {
        let ns = format!("{:08x}-0000-4000-8000-{:012x}", i as u32, i as u64);
        debug_assert!(
            valid_namespace(&ns),
            "fresh_ns muss valid_namespace erfüllen"
        );
        ns
    }

    /// Registriert `tx` als Host des Namespace (wie eine frisch angenommene
    /// Host-Verbindung). Eigener schmaler Helfer — das Pendant in `mod tests`
    /// ist modul-privat.
    async fn register_host(broker: &Broker, ns: &str, tx: &Tx) {
        let mut map = broker.namespaces.lock().await;
        let namespace = map.entry(ns.into()).or_insert_with(Namespace::new);
        namespace.host = Some(tx.clone());
        namespace.host_last_seen = now_ms();
    }

    /// Liest `court`/`seq` aus einem Nudge-Text-Frame (wie in `mod tests`).
    fn nudge_of(m: Message) -> (i64, u64) {
        let Message::Text(t) = m else {
            panic!("kein Text-Frame");
        };
        let v: serde_json::Value = serde_json::from_str(t.as_str()).unwrap();
        (v["court"].as_i64().unwrap(), v["seq"].as_u64().unwrap())
    }

    /// Ist der Frame ein `session_superseded` (abgelöstes Tablet)?
    fn is_superseded(m: &Message) -> bool {
        let Message::Text(t) = m else { return false };
        serde_json::from_str::<serde_json::Value>(t.as_str())
            .ok()
            .and_then(|v| v["type"].as_str().map(|s| s == "session_superseded"))
            .unwrap_or(false)
    }

    /// Bekannte, erlaubte Fehlermeldungen des Ergebnis-Wegs (Cap/Timeout/
    /// Verbindungsverlust). Alles andere gilt als unerwartet.
    fn known_result_error(msg: Option<&str>) -> bool {
        matches!(
            msg,
            Some("Zu viele offene Übermittlungen – bitte kurz warten.")
                | Some("Zeitüberschreitung – bts-light hat nicht geantwortet.")
                | Some("bts-light ist nicht erreichbar.")
                | Some("bts-light ist nicht mit dem Relay verbunden.")
                | Some("Verbindung zu bts-light verloren.")
        )
    }

    // ─────────────────────────── Szenarien ─────────────────────────────────

    /// **Massen-Connect:** M NS × Host × F Felder × je T ringende Tablets + S
    /// Monitor-Subs, alle gleichzeitig. Endcheck: je NS `tablets.len() ==
    /// min(F, MAX_TABLETS_PER_NS)` (genau ein Halter je Feld), Subs-Summe
    /// `== min(S, MAX_MONITOR_SUBS)`, `namespaces.len() <= MAX_NAMESPACES` und
    /// KEIN Cap überschritten.
    async fn run_mass_connect(broker: Broker, p: &LoadParams) {
        let courts = p.courts_per_ns;
        let tablets = p.tablets_per_court;
        let subs = p.subs_per_ns;

        // rx im Hauptscope halten — nur tx-Klone wandern in die Tasks.
        let mut rxs: Vec<UnboundedReceiver<Message>> = Vec::new();
        let mut host_rxs: Vec<UnboundedReceiver<Message>> = Vec::new();
        let mut ns_list: Vec<String> = Vec::new();
        let mut set = JoinSet::new();

        for i in 0..p.namespaces {
            let ns = fresh_ns(i);
            let (host_tx, host_rx) = mpsc::unbounded_channel::<Message>();
            register_host(&broker, &ns, &host_tx).await;
            host_rxs.push(host_rx);

            // T Tablets je Feld, distinct device_id → genau eines wird Halter,
            // die übrigen bleiben passiv (Occupied), nichts wird abgelehnt,
            // solange F <= MAX_TABLETS_PER_NS.
            for c in 0..courts {
                for t in 0..tablets {
                    let (tx, rx) = mpsc::unbounded_channel::<Message>();
                    rxs.push(rx);
                    let b = broker.clone();
                    let ns_c = ns.clone();
                    let dev = format!("dev-{i}-{c}-{t}");
                    let court = c as i64;
                    set.spawn(async move {
                        attach_tablet(&b, &ns_c, court, &dev, &tx).await;
                    });
                }
            }
            // S Subs, gemischt court-spezifisch/„alle" — Deckel greift auf die
            // Summe.
            for s in 0..subs {
                let (tx, rx) = mpsc::unbounded_channel::<Message>();
                rxs.push(rx);
                let b = broker.clone();
                let ns_c = ns.clone();
                let court = if s % 2 == 0 {
                    Some((s % courts.max(1)) as i64)
                } else {
                    None
                };
                set.spawn(async move {
                    subscribe_monitor(&b, &ns_c, court, &tx).await;
                });
            }
            ns_list.push(ns);
        }

        while let Some(r) = set.join_next().await {
            r.expect("Task-Panik im Massen-Connect");
        }

        let map = broker.namespaces.lock().await;
        assert!(map.len() <= MAX_NAMESPACES, "Namespace-Cap eingehalten");
        for ns in &ns_list {
            let n = map.get(ns).expect("Namespace existiert");
            let expect_tablets = courts.min(MAX_TABLETS_PER_NS);
            assert_eq!(
                n.tablets.len(),
                expect_tablets,
                "genau ein Halter je Feld, Cap eingehalten"
            );
            assert!(
                n.tablets.len() <= MAX_TABLETS_PER_NS,
                "Tablet-Cap nie überschritten"
            );
            let sub_total: usize =
                n.monitor_subs.values().map(Vec::len).sum::<usize>() + n.monitor_subs_all.len();
            assert_eq!(
                sub_total,
                subs.min(MAX_MONITOR_SUBS),
                "Subs-Summe == min(S, Deckel)"
            );
            assert!(
                sub_total <= MAX_MONITOR_SUBS,
                "Monitor-Cap nie überschritten"
            );
        }
        // rx/host_rx bewusst bis hier halten (Sender-Klone sonst ausgesiebt).
        drop(rxs);
        drop(host_rxs);
    }

    /// **Reconnect-Sturm:** je Feld ringen T Tasks per `take_over_court`
    /// (distinct device_id) um den Slot. Der Mutex serialisiert die `insert`s:
    /// der erste trifft den leeren Slot (kein Supersede), die restlichen `T-1`
    /// verdrängen den Vorgänger. Endcheck je Feld: GENAU ein Halter (Identität
    /// offen), Superseded-Summe `== T-1`, kein Panic.
    async fn run_reconnect_storm(broker: Broker, p: &LoadParams) {
        let t = p.tablets_per_court;
        // Je (NS, Feld) alle rx sammeln, um die Superseded-Frames zu zählen.
        let mut groups: Vec<(String, i64, Vec<UnboundedReceiver<Message>>)> = Vec::new();
        let mut set = JoinSet::new();

        for i in 0..p.namespaces {
            let ns = fresh_ns(i);
            for c in 0..p.courts_per_ns {
                let court = c as i64;
                let mut court_rx = Vec::with_capacity(t);
                for k in 0..t {
                    let (tx, rx) = mpsc::unbounded_channel::<Message>();
                    court_rx.push(rx);
                    let b = broker.clone();
                    let ns_c = ns.clone();
                    let dev = format!("dev-{i}-{c}-{k}");
                    set.spawn(async move {
                        take_over_court(&b, &ns_c, court, &dev, &tx).await;
                    });
                }
                groups.push((ns.clone(), court, court_rx));
            }
        }

        while let Some(r) = set.join_next().await {
            r.expect("Task-Panik im Reconnect-Sturm");
        }

        let map = broker.namespaces.lock().await;
        for (ns, court, mut court_rx) in groups {
            let n = map.get(&ns).expect("Namespace existiert");
            assert!(
                n.tablets.contains_key(&court),
                "Feld {court} hat genau einen Halter"
            );
            let mut superseded = 0usize;
            for rx in court_rx.iter_mut() {
                while let Ok(m) = rx.try_recv() {
                    assert!(is_superseded(&m), "nur Ablöse-Frames erwartet");
                    superseded += 1;
                }
            }
            assert_eq!(superseded, t - 1, "Feld {court}: genau T-1 Ablösungen");
        }
    }

    /// **Nudge-Fan-out:** ein Namespace A mit je Feld einem court-spezifischen
    /// Sub + einem „alle"-Sub; dazu ein Fremd-Namespace B mit Sub. Viele
    /// `notify_monitor`-Tasks feuern `N_c` Nudges je Feld. Endcheck: jeder
    /// court-c-Sub sah GENAU `N_c` Nudges (alle `court==c`), der „alle"-Sub sah
    /// `Σ N_c`, der Fremd-NS-Sub 0 (Isolation). `seq`-Reihenfolge NICHT geprüft.
    async fn run_nudge_fanout(broker: Broker, p: &LoadParams) {
        let courts = p.courts_per_ns;
        let notifies = p.notifies_per_court;
        let nsa = fresh_ns(0);
        let nsb = fresh_ns(1);

        // Hosts, damit `subscribe_monitor` die Namespaces bespielt. Die Host-rx
        // bleiben bis Funktionsende gebunden (nicht `let _ = …`, das dropt).
        let (ha, _rha) = mpsc::unbounded_channel::<Message>();
        let (hb, _rhb) = mpsc::unbounded_channel::<Message>();
        register_host(&broker, &nsa, &ha).await;
        register_host(&broker, &nsb, &hb).await;

        // court-spezifische Subs (Index == Feld), „alle"-Sub, Fremd-NS-Sub.
        let mut court_rx: Vec<UnboundedReceiver<Message>> = Vec::with_capacity(courts);
        for c in 0..courts {
            let (tx, rx) = mpsc::unbounded_channel::<Message>();
            subscribe_monitor(&broker, &nsa, Some(c as i64), &tx).await;
            court_rx.push(rx);
        }
        let (tx_all, mut all_rx) = mpsc::unbounded_channel::<Message>();
        subscribe_monitor(&broker, &nsa, None, &tx_all).await;
        let (tx_f, mut foreign_rx) = mpsc::unbounded_channel::<Message>();
        subscribe_monitor(&broker, &nsb, Some(0), &tx_f).await;

        let mut set = JoinSet::new();
        for c in 0..courts {
            for _ in 0..notifies {
                let b = broker.clone();
                let ns = nsa.clone();
                let court = c as i64;
                set.spawn(async move {
                    let mut m = b.namespaces.lock().await;
                    if let Some(n) = m.get_mut(&ns) {
                        notify_monitor(n, court);
                    }
                });
            }
        }
        while let Some(r) = set.join_next().await {
            r.expect("Task-Panik im Nudge-Fan-out");
        }

        for (c, rx) in court_rx.iter_mut().enumerate() {
            let mut cnt = 0usize;
            while let Ok(m) = rx.try_recv() {
                let (court, _seq) = nudge_of(m);
                assert_eq!(court, c as i64, "court-Sub sah nur sein Feld");
                cnt += 1;
            }
            assert_eq!(cnt, notifies, "Feld {c}: genau N_c Nudges");
        }
        let mut all_cnt = 0usize;
        while all_rx.try_recv().is_ok() {
            all_cnt += 1;
        }
        assert_eq!(all_cnt, courts * notifies, "„alle\"-Sub sah Σ N_c");
        assert!(
            foreign_rx.try_recv().is_err(),
            "Fremd-NS-Sub blieb still (Isolation)"
        );
    }

    /// **Ergebnis-Schwall:** je NS ein Host + eine schlanke Acker-Task, die die
    /// weitergeleiteten `RelayFrame::Result` sofort mit `HostFrame::ResultAck`
    /// beantwortet. Viele parallele `result`-`POST`s (inkl. idempotenter Retries
    /// mit geteilter `match_id`). Endcheck: je NS `pending.is_empty()`; jede
    /// Antwort `ok()` ODER bekannter Fehlerstring; `MAX_PENDING_PER_NS` nie
    /// überschritten (Nachweis = End-`pending` leer); kein Panic.
    async fn run_result_storm(broker: Broker, p: &LoadParams) {
        let results = p.results;
        let courts = p.courts_per_ns.max(1);
        let mut ns_list: Vec<String> = Vec::new();
        let mut ackers: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        for i in 0..p.namespaces {
            let ns = fresh_ns(i);
            let (host_tx, mut host_rx) = mpsc::unbounded_channel::<Message>();
            register_host(&broker, &ns, &host_tx).await;
            // Schlanke Acker-Task: Frame lesen → `reqId` ziehen → sofort acken.
            // So schlägt das 8-s-`RESULT_TIMEOUT` nie zu. `host_tx` wandert mit
            // (jeder Klon erfüllt den `same_channel`-Check); die Task wird nach
            // dem Join der POSTs abgebrochen (sie hält selbst einen Sender).
            let b = broker.clone();
            let ns_c = ns.clone();
            let acker = tokio::spawn(async move {
                while let Some(msg) = host_rx.recv().await {
                    let Message::Text(txt) = msg else { continue };
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(txt.as_str()) else {
                        continue;
                    };
                    if let Some(req_id) = v.get("reqId").and_then(|x| x.as_u64()) {
                        handle_host_frame(
                            &b,
                            &ns_c,
                            HostFrame::ResultAck {
                                req_id,
                                ok: true,
                                error: None,
                            },
                            &host_tx,
                        )
                        .await;
                    }
                }
            });
            ackers.push(acker);
            ns_list.push(ns);
        }

        let mut set: JoinSet<ResultResponse> = JoinSet::new();
        for ns in &ns_list {
            for k in 0..results {
                let b = broker.clone();
                let ns_c = ns.clone();
                // Geteilte `match_id` für ~die Hälfte = idempotente Retries;
                // der Relay vergibt je POST dennoch eine eigene `req_id`.
                let match_id = (k % (results / 2 + 1)) as i64;
                let court_id = (k % courts) as i64;
                set.spawn(async move {
                    let body = ResultBody {
                        match_id,
                        court_id,
                        court_label: String::new(),
                        sets: Vec::new(),
                        retired: false,
                        walkover: false,
                        winner: None,
                        cascade_walkover: false,
                    };
                    result(State(b), Path(ns_c), Json(body)).await.0
                });
            }
        }
        while let Some(r) = set.join_next().await {
            let resp = r.expect("Task-Panik im Ergebnis-Schwall");
            assert!(
                resp.ok || known_result_error(resp.error.as_deref()),
                "Antwort ok() oder bekannter Fehler, war: {resp:?}"
            );
        }

        // Alle POSTs sind zurück → jeder weitergeleitete Frame wurde entweder
        // geackt (pending entfernt) oder lief in den Timeout (Handler entfernt
        // pending selbst). Die Acker werden jetzt abgebrochen.
        for acker in &ackers {
            acker.abort();
        }
        let map = broker.namespaces.lock().await;
        for ns in &ns_list {
            let empty = map.get(ns).map(|n| n.pending.is_empty()).unwrap_or(true);
            assert!(
                empty,
                "je NS keine offenen Übermittlungen (Cap nie gerissen)"
            );
        }
    }

    /// **Cleanup:** ein voll bespielter Namespace (Host + je Feld ein Tablet +
    /// S Subs) wird komplett getrennt. Endcheck: nach dem Austragen aller Subs
    /// sind die Sub-Listen `is_empty`; nach Host-Freigabe + Tablet-Detach ist
    /// der Namespace via `Namespace::is_empty()` aus `namespaces` entfernt
    /// (kein unbegrenztes Wachsen).
    async fn run_cleanup(broker: Broker, p: &LoadParams) {
        let courts = p.courts_per_ns;
        let subs = p.subs_per_ns;
        let mut ns_list: Vec<String> = Vec::new();
        // Pro NS die Sende-Enden + rx aufbewahren (detach/unsubscribe brauchen
        // den EIGENEN Sender via `same_channel`).
        let mut per_ns_tablets: Vec<Vec<(i64, Tx)>> = Vec::new();
        let mut per_ns_subs: Vec<Vec<(Option<i64>, Tx)>> = Vec::new();
        let mut host_txs: Vec<Tx> = Vec::new();
        let mut keep_rx: Vec<UnboundedReceiver<Message>> = Vec::new();

        for i in 0..p.namespaces {
            let ns = fresh_ns(i);
            let (host_tx, host_rx) = mpsc::unbounded_channel::<Message>();
            register_host(&broker, &ns, &host_tx).await;
            keep_rx.push(host_rx);

            let mut tablets = Vec::with_capacity(courts);
            for c in 0..courts {
                let (tx, rx) = mpsc::unbounded_channel::<Message>();
                keep_rx.push(rx);
                let dev = format!("dev-{i}-{c}");
                attach_tablet(&broker, &ns, c as i64, &dev, &tx).await;
                tablets.push((c as i64, tx));
            }
            let mut sub_list = Vec::with_capacity(subs);
            for s in 0..subs {
                let (tx, rx) = mpsc::unbounded_channel::<Message>();
                keep_rx.push(rx);
                let court = if s % 2 == 0 {
                    Some((s % courts.max(1)) as i64)
                } else {
                    None
                };
                subscribe_monitor(&broker, &ns, court, &tx).await;
                sub_list.push((court, tx));
            }

            per_ns_tablets.push(tablets);
            per_ns_subs.push(sub_list);
            host_txs.push(host_tx);
            ns_list.push(ns);
        }

        // Phase A (nebenläufig): alle Subs austragen. Der Namespace lebt noch
        // (Host + Tablets), also lässt sich die leere Sub-Liste danach prüfen.
        let mut set = JoinSet::new();
        for (ns, sub_list) in ns_list.iter().zip(per_ns_subs) {
            for (court, tx) in sub_list {
                let b = broker.clone();
                let ns_c = ns.clone();
                set.spawn(async move {
                    unsubscribe_monitor(&b, &ns_c, court, &tx).await;
                });
            }
        }
        while let Some(r) = set.join_next().await {
            r.expect("Task-Panik beim Sub-Austragen");
        }
        {
            let map = broker.namespaces.lock().await;
            for ns in &ns_list {
                let n = map.get(ns).expect("Namespace lebt noch (Host+Tablets)");
                assert!(n.monitor_subs.is_empty(), "court-Sub-Listen leer");
                assert!(n.monitor_subs_all.is_empty(), "„alle\"-Sub-Liste leer");
            }
        }

        // Host-Slot je NS freigeben (setzt nur `host = None`, entfernt nicht) —
        // danach räumt der LETZTE Tablet-Detach den nun leeren Namespace ab.
        {
            let mut map = broker.namespaces.lock().await;
            for (ns, host_tx) in ns_list.iter().zip(&host_txs) {
                if let Some(n) = map.get_mut(ns) {
                    release_host_slot(n, host_tx);
                }
            }
        }

        // Phase B (nebenläufig): alle Tablets trennen. Der letzte Detach je NS
        // sieht `is_empty()` und entfernt den Namespace.
        let mut set = JoinSet::new();
        for (ns, tablets) in ns_list.iter().zip(per_ns_tablets) {
            for (court, tx) in tablets {
                let b = broker.clone();
                let ns_c = ns.clone();
                set.spawn(async move {
                    detach_tablet(&b, &ns_c, court, &tx).await;
                });
            }
        }
        while let Some(r) = set.join_next().await {
            r.expect("Task-Panik beim Tablet-Detach");
        }

        let map = broker.namespaces.lock().await;
        for ns in &ns_list {
            assert!(
                map.get(ns).is_none(),
                "Namespace nach vollständiger Trennung entfernt"
            );
        }
        drop(keep_rx);
    }

    /// **Cap-Boundary:** treibt die Deckel ABSICHTLICH ÜBER ihre Grenze und
    /// prüft, dass GENAU am Deckel abgeschnitten wird (Überschuss abgewiesen).
    /// Anders als die übrigen Szenarien (die unter den Caps bleiben und deren
    /// `== min(N, Cap)` sich auf `== N` reduziert) macht DAS die ADR-0019-Aussage
    /// „Cap-Einhaltung bewiesen" wahr: entfernte man einen Cap-Check im Broker,
    /// würde `== Cap` rot (Review-Befund MEDIUM). `over` = Überschuss über den
    /// Deckel. Alle Abos/Attaches laufen nebenläufig gegen den Namespace-Mutex.
    async fn run_cap_boundary(broker: Broker, over: usize) {
        let ns = fresh_ns(0);
        let (host_tx, host_rx) = mpsc::unbounded_channel::<Message>();
        register_host(&broker, &ns, &host_tx).await;

        let mut rxs: Vec<UnboundedReceiver<Message>> = Vec::new();
        let mut set = JoinSet::new();

        // Monitor-Subs: MAX_MONITOR_SUBS + over Abos (alle in die „alle"-Liste).
        for _ in 0..(MAX_MONITOR_SUBS + over) {
            let (tx, rx) = mpsc::unbounded_channel::<Message>();
            rxs.push(rx);
            let b = broker.clone();
            let ns_c = ns.clone();
            set.spawn(async move {
                subscribe_monitor(&b, &ns_c, None, &tx).await;
            });
        }
        // Tablets: MAX_TABLETS_PER_NS + over DISTINKTE Felder (je ein Tablet) —
        // jenseits des Deckels wird ein neues Feld abgewiesen.
        for c in 0..(MAX_TABLETS_PER_NS + over) {
            let (tx, rx) = mpsc::unbounded_channel::<Message>();
            rxs.push(rx);
            let b = broker.clone();
            let ns_c = ns.clone();
            let dev = format!("cap-dev-{c}");
            let court = c as i64;
            set.spawn(async move {
                attach_tablet(&b, &ns_c, court, &dev, &tx).await;
            });
        }
        while let Some(r) = set.join_next().await {
            r.expect("Task-Panik im Cap-Boundary");
        }

        let map = broker.namespaces.lock().await;
        let n = map.get(&ns).expect("Namespace existiert");
        let sub_total: usize =
            n.monitor_subs.values().map(Vec::len).sum::<usize>() + n.monitor_subs_all.len();
        assert_eq!(
            sub_total, MAX_MONITOR_SUBS,
            "Monitor-Sub-Cap schneidet GENAU am Deckel ab (Überschuss abgewiesen)"
        );
        assert_eq!(
            n.tablets.len(),
            MAX_TABLETS_PER_NS,
            "Tablet-Cap schneidet GENAU am Deckel ab (Überschuss abgewiesen)"
        );
        drop(rxs);
        drop(host_rx);
    }

    // ─────────────────── Leichte CI-Wache (Sekunden, grün) ──────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn light_cap_boundary() {
        // over=4: knapp über MAX_MONITOR_SUBS/MAX_TABLETS_PER_NS, schnell +
        // deterministisch (der Deckel ist eine harte Grenze unter dem Mutex).
        tokio::time::timeout(
            Duration::from_secs(10),
            run_cap_boundary(Broker::new("t".into()), 4),
        )
        .await
        .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn light_mass_connect() {
        let p = light();
        tokio::time::timeout(p.timeout, run_mass_connect(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn light_reconnect_storm() {
        let p = light();
        tokio::time::timeout(p.timeout, run_reconnect_storm(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn light_nudge_fanout() {
        let p = light();
        tokio::time::timeout(p.timeout, run_nudge_fanout(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn light_result_storm() {
        let p = light();
        tokio::time::timeout(p.timeout, run_result_storm(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn light_cleanup() {
        let p = light();
        tokio::time::timeout(p.timeout, run_cleanup(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    // ────────────── Schwere Soak-Variante (manuell, `--ignored`) ────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Soak: manuell vor dem Turnier (cargo test -p bts-relay -- --ignored)"]
    async fn soak_cap_boundary() {
        // Deutlich über dem Deckel — die harte Grenze muss auch unter starkem
        // gleichzeitigem Andrang exakt greifen.
        tokio::time::timeout(
            Duration::from_secs(60),
            run_cap_boundary(Broker::new("t".into()), 128),
        )
        .await
        .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Soak: manuell vor dem Turnier (cargo test -p bts-relay -- --ignored)"]
    async fn soak_mass_connect() {
        let p = soak();
        tokio::time::timeout(p.timeout, run_mass_connect(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Soak: manuell vor dem Turnier (cargo test -p bts-relay -- --ignored)"]
    async fn soak_reconnect_storm() {
        let p = soak();
        tokio::time::timeout(p.timeout, run_reconnect_storm(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Soak: manuell vor dem Turnier (cargo test -p bts-relay -- --ignored)"]
    async fn soak_nudge_fanout() {
        let p = soak();
        tokio::time::timeout(p.timeout, run_nudge_fanout(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Soak: manuell vor dem Turnier (cargo test -p bts-relay -- --ignored)"]
    async fn soak_result_storm() {
        let p = soak();
        tokio::time::timeout(p.timeout, run_result_storm(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Soak: manuell vor dem Turnier (cargo test -p bts-relay -- --ignored)"]
    async fn soak_cleanup() {
        let p = soak();
        tokio::time::timeout(p.timeout, run_cleanup(Broker::new("t".into()), &p))
            .await
            .expect("Timeout = Hang → Testfehler");
    }
}
