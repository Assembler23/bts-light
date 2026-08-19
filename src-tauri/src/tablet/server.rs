//! Eingebetteter HTTP+WebSocket-Server für die Schiedsrichter-Tablets
//! (LAN-Modus).
//!
//! bts-light ist damit der zentrale Hub: Tablets laden die Spielzettel-UI,
//! binden sich an einen Court, bekommen das von BTP zugewiesene Match,
//! zählen Punkte (Live-Score → Liveticker) und schreiben am Spielende das
//! Ergebnis via `SENDUPDATE` zurück nach BTP.
//!
//! Im Cloud-Modus läuft dieser Server nicht – stattdessen verbindet sich
//! [`crate::tablet::relay_client`] ausgehend zum Relay. Die Kernlogik
//! ([`ServerCtx`], [`process_result`], [`handle_score`], [`match_brief`])
//! ist `pub(crate)` und wird von beiden Modi geteilt.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};

use relay_proto::{
    device_code, html_escape, path_encode, MatchBrief, PlayerBrief, ResultBody, ResultResponse,
    ServerMsg, SetAb, TabletMsg,
};

use crate::badhub::diff::Update;
use crate::badhub::payload::build_tupdate;
use crate::badhub::push;
use crate::btp::model::{BtpMatch, MatchStatus};
use crate::btp::{client, proto};
use crate::config::{AppConfig, CourtMonitorConfig};
use crate::tablet::assets::{self, TABLET_HTML};
use crate::tablet::monitor;
use crate::tablet::perf;
use crate::tablet::state::{reconnect_decision, ReconnectDecision, TabletState};

/// Fester Port des Tablet-Servers im Hallen-LAN.
pub const TABLET_PORT: u16 = 8088;

/// Geteilter Kontext der Tablet-Logik – im LAN-Modus von den HTTP-/WS-
/// Handlern genutzt, im Cloud-Modus vom Relay-Client.
pub struct ServerCtx {
    pub tablet: Arc<TabletState>,
    config: AppConfig,
    pub(crate) http: reqwest::Client,
    /// Request-IDs für Liveticker-Pushes. Eigener Zähler – Badhub spiegelt
    /// `rid` nur zurück, dedupliziert nicht; eine Kollision mit dem
    /// Sync-Loop wäre folgenlos.
    rid: AtomicU64,
    /// Verzeichnis der hochgeladenen Court-Monitor-Werbebilder (`court-ads`).
    pub monitor_dir: PathBuf,
    /// Pfad zur `config.json` – der Court-Monitor lädt seine Konfiguration
    /// frisch von dort, damit Änderungen im Tool ohne Neustart greifen.
    config_path: PathBuf,
    /// Pfad zur Monitor-Zuweisungsdatei (Gerät → CourtID). Wird frisch
    /// gelesen, damit Zuweisungen aus dem Tool sofort greifen.
    pub assignments_path: PathBuf,
    /// App-Log-Verzeichnis (wie „Logs öffnen"). Hierhin schreibt der Server die
    /// von den Tablets hochgeladenen Diagnoselogs (Unterordner `tablet-logs`).
    pub log_dir: PathBuf,
    /// Derselbe In-Memory-Konfigurationsstand wie `AppState.config`
    /// (`commands.rs`) — ein einziges, geteiltes `Arc<Mutex<_>>` statt zweier
    /// getrennter Kopien. `ServerCtx::mutate_app_config` (TlAction-
    /// Ausführungen, Panel-Profile, Spec tl-web-panelsystem, ADR 0025) und
    /// `commands::mutate_config` (Tauri-Commands wie `save_config`,
    /// `tl_device_add`) sperren so exakt dasselbe Schloss: Egal welcher der
    /// beiden Wege zuerst dran ist, der zweite sieht garantiert den Stand
    /// des ersten als Ausgangspunkt seines eigenen Lesen-Ändern-Schreiben-
    /// Zyklus — kein Lost-Update mehr zwischen den beiden Schreibpfaden.
    /// (Vorher schrieb `mutate_app_config` direkt an der Platte vorbei am
    /// In-Memory-Stand; ein `save_config` danach überschrieb die Datei mit
    /// dem veralteten In-Memory-Stand und löschte den Platten-Schreibvorgang
    /// wieder kommentarlos.)
    shared_config: Arc<std::sync::Mutex<AppConfig>>,
    /// Warteschlange der Live-Score-Pushes an badhub, je Feld serialisiert
    /// und gebündelt — siehe [`ScorePushQueue`].
    score_push: Arc<ScorePushQueue>,
    /// Zwischenstand des Config-Lesens: `(Änderungszeit, Größe, geparst)`.
    ///
    /// `app_config()` läuft auf den heißesten Pfaden — jede TL-Anfrage
    /// prüft damit ihren Zugang, jeder Monitor-Abruf seine Einstellungen —
    /// und las bisher **jedes Mal** `config.json` von der Platte und
    /// parste sie neu. Jetzt entscheidet ein Blick auf Änderungszeit und
    /// Größe: Ist die Datei unverändert, gibt es die gemerkte Fassung.
    /// Die Semantik bleibt exakt dieselbe (jede geschriebene Änderung
    /// wird erkannt, ein unlesbarer Stand meldet weiterhin einen Fehler) —
    /// nur ohne das Lesen und Parsen (Review-Vorschlag 18.08.2026).
    config_cache: std::sync::Mutex<Option<(std::time::SystemTime, u64, Arc<AppConfig>)>>,
    /// Zwischenstand der Werbebild-Liste: `(Änderungszeit des Ordners,
    /// Dateinamen)`. `list_ads` macht ein `read_dir` samt Sortieren —
    /// auf den Monitor-Pfaden lief das bei **jedem** Abruf, bei zwanzig
    /// Anzeigen also achtzigmal je Sekunde. Der Ordner ändert sich nur,
    /// wenn jemand ein Bild hochlädt oder löscht (Review 18.08.2026).
    ads_cache: std::sync::Mutex<Option<(std::time::SystemTime, Vec<String>)>>,
    /// Das dekodierte Turnierlogo, gemerkt an `(Länge, MIME)` der
    /// Base64-Daten: Sonst dekodierte jeder Abruf der Logo-Route die bis
    /// zu 2,7 MB Base64 neu — bei zwanzig Anzeigen regelmäßig.
    logo_cache: std::sync::Mutex<LogoCache>,
    /// Zwischenstand der „Leisten-Sponsor"-Markierungen, gemerkt an
    /// `(Änderungszeit, Größe)` der Datei. Sie wird nur beim Setzen eines
    /// Häkchens im Setup geschrieben, aber von jedem `/info/ad/state`
    /// gelesen und geparst — das ist die Werbe-Seite im 5-Sekunden-Takt
    /// plus die Sponsor-Leiste jeder Anzeige im Minuten-Takt.
    bar_cache: std::sync::Mutex<BarCache>,
}

/// Das dekodierte Turnierlogo, geschlüsselt nach der Marke seines Inhalts.
type LogoCache = Option<(String, Arc<Vec<u8>>)>;

/// Die „Leisten-Sponsor"-Markierungen, geschlüsselt nach
/// `(Änderungszeit, Größe)` ihrer Datei.
type BarCache = Option<((std::time::SystemTime, u64), Arc<HashSet<String>>)>;

/// Die Live-Score-Pushes an badhub, **je Feld serialisiert und
/// gebündelt** — und vor allem: **außerhalb** der Tablet-Verbindung.
///
/// Vorher lief der Push mitten in der WebSocket-Schleife des Tablets, mit
/// 15 s Timeout. Der Server erwartet aber spätestens nach 10 s ein
/// Lebenszeichen desselben Sockets (`STALE_AFTER`) — ein hängendes badhub
/// ließ die Verbindung also auflaufen, der Server schloss sie **und gab
/// das Feld frei**. Bei zwanzig Feldern hätte das alle zugleich getroffen
/// (Analyse 18.08.2026). Der Push gehört deshalb hinter die Verbindung,
/// nicht hinein.
///
/// Trotzdem kein blindes `spawn` je Punkt: Zwei Pushes desselben Felds
/// dürfen sich nicht überholen (badhub zeigte sonst kurzzeitig den
/// älteren Stand). Deshalb ist je Feld immer nur einer unterwegs, und
/// während er läuft, sammelt sich nur der **neueste** Stand an — ein
/// Punkteregen wird so zusätzlich zusammengefasst statt in eine
/// Anfragen-Lawine übersetzt.
#[derive(Default)]
struct ScorePushQueue {
    /// Beides unter EINEM Schloss, damit „nichts mehr da, ich höre auf"
    /// und „ich stelle etwas ein" nicht ineinander rutschen können — sonst
    /// bliebe ein zuletzt eingestellter Stand ungesendet liegen.
    inner: std::sync::Mutex<ScorePushState>,
}

#[derive(Default)]
struct ScorePushState {
    /// Feld → neuester noch nicht gesendeter Stand, mit dem Spiel, zu dem
    /// er gehört.
    pending: HashMap<i64, (i64, crate::badhub::diff::Update)>,
    /// Felder, für die gerade ein Push unterwegs ist.
    busy: std::collections::HashSet<i64>,
}

impl ScorePushQueue {
    /// Stellt einen Stand ein. `true` = für dieses Feld läuft noch kein
    /// Arbeiter, der Aufrufer muss einen starten.
    fn einstellen(
        &self,
        court_id: i64,
        match_id: i64,
        update: crate::badhub::diff::Update,
    ) -> bool {
        let mut g = self.inner.lock().expect("Score-Push-Mutex nicht vergiftet");
        g.pending.insert(court_id, (match_id, update));
        g.busy.insert(court_id)
    }

    /// Der nächste zu sendende Stand eines Felds — oder `None`, wenn
    /// nichts mehr ansteht. Im `None`-Fall meldet sich der Arbeiter
    /// **unter demselben Schloss** ab: Ein Stand, der genau jetzt
    /// eingestellt wird, sieht uns dann nicht mehr als beschäftigt und
    /// startet seinen eigenen Arbeiter — so bleibt kein Stand liegen.
    fn naechster(&self, court_id: i64) -> Option<(i64, crate::badhub::diff::Update)> {
        let mut g = self.inner.lock().expect("Score-Push-Mutex nicht vergiftet");
        match g.pending.remove(&court_id) {
            Some(eintrag) => Some(eintrag),
            None => {
                g.busy.remove(&court_id);
                None
            }
        }
    }
}

impl ServerCtx {
    /// Stellt einen Live-Score zum Senden ein und kehrt **sofort** zurück.
    fn queue_score_push(&self, court_id: i64, match_id: i64, update: crate::badhub::diff::Update) {
        if !self.score_push.einstellen(court_id, match_id, update) {
            // Für dieses Feld läuft schon einer — er nimmt den neuen
            // Stand mit, sobald er fertig ist.
            return;
        }
        let queue = self.score_push.clone();
        let tablet = self.tablet.clone();
        let http = self.http.clone();
        let url = self.config.badhub.url.clone();
        let password = self.config.badhub.password.clone();
        tokio::spawn(async move {
            while let Some((match_id, update)) = queue.naechster(court_id) {
                // Veraltet-Prüfung DIREKT vor dem Senden (Review-Fund
                // 18.08.2026): Zwischen Einstellen und Senden kann ein
                // hängendes badhub Minuten liegen lassen. In der Zeit kann
                // das Spiel enden und die Turnierleitung das Ergebnis
                // **korrigiert** haben — ein nachträglich eintreffender
                // Live-Stand überschriebe die Korrektur im Liveticker, und
                // der Diff des Sync-Zyklus schickt ein beendetes Match nie
                // wieder mit. Steht das Spiel nicht mehr auf dem Feld oder
                // ist es in BTP finalisiert, wird der Stand verworfen.
                if match_id != 0 {
                    let noch_aktuell =
                        tablet.match_for_court(court_id).map(|m| m.id) == Some(match_id);
                    if !noch_aktuell || tablet.is_match_finalized(court_id, match_id) {
                        tracing::info!(
                            "Live-Score für Match {match_id} verworfen: nicht mehr aktuell \
                             (Spiel beendet oder Feld neu belegt)"
                        );
                        continue;
                    }
                }
                if let Err(e) = push::push_update(&http, &url, &password, &update).await {
                    tracing::warn!("Live-Score-Push fehlgeschlagen: {e}");
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tablet: Arc<TabletState>,
        config: AppConfig,
        http: reqwest::Client,
        monitor_dir: PathBuf,
        config_path: PathBuf,
        assignments_path: PathBuf,
        log_dir: PathBuf,
        shared_config: Arc<std::sync::Mutex<AppConfig>>,
    ) -> Self {
        Self {
            tablet,
            config,
            http,
            rid: AtomicU64::new(1),
            monitor_dir,
            config_path,
            assignments_path,
            log_dir,
            shared_config,
            config_cache: std::sync::Mutex::new(None),
            ads_cache: std::sync::Mutex::new(None),
            logo_cache: std::sync::Mutex::new(None),
            bar_cache: std::sync::Mutex::new(None),
            score_push: Arc::new(ScorePushQueue::default()),
        }
    }

    fn next_rid(&self) -> u64 {
        self.rid.fetch_add(1, Ordering::Relaxed)
    }

    /// Lädt die aktuelle Court-Monitor-Konfiguration frisch von der Platte.
    /// Schlägt das Lesen fehl, gelten die Default-Werte.
    pub fn monitor_config(&self) -> CourtMonitorConfig {
        self.app_config_result()
            .map(|c| c.court_monitor)
            .unwrap_or_default()
    }

    /// Gesamte App-Config frisch von der Platte (Default bei Fehler) – für
    /// Aufrufer, die mehrere Felder daraus brauchen, ohne doppelt zu lesen.
    pub fn app_config(&self) -> AppConfig {
        self.app_config_result().unwrap_or_default()
    }

    /// Wie [`Self::app_config`], aber **ohne Kopie** — für die heißen
    /// Pfade.
    ///
    /// `AppConfig` trägt unter anderem das Turnierlogo als Base64-Text
    /// (bis 2,7 MB). Auf den Monitor-Routen wurde es bei jedem Abruf
    /// mitkopiert, teils zweimal — bei zwanzig Anzeigen mit ihrem
    /// 250-ms-Takt ergab das hunderte Megabyte reines Kopieren pro
    /// Sekunde (Analyse 18.08.2026). Wer nur lesen will, nimmt das
    /// `Arc`.
    pub fn app_config_arc(&self) -> Arc<AppConfig> {
        self.app_config_arc_result().unwrap_or_default()
    }

    fn app_config_arc_result(&self) -> Result<Arc<AppConfig>, String> {
        let meta = std::fs::metadata(&self.config_path).map_err(|e| e.to_string())?;
        let stempel = (meta.modified().map_err(|e| e.to_string())?, meta.len());
        {
            let cache = self
                .config_cache
                .lock()
                .expect("Config-Cache nicht vergiftet");
            if let Some((zeit, groesse, config)) = cache.as_ref() {
                if (*zeit, *groesse) == stempel {
                    return Ok(config.clone());
                }
            }
        }
        let config = Arc::new(AppConfig::load_from(&self.config_path).map_err(|e| e.to_string())?);
        *self
            .config_cache
            .lock()
            .expect("Config-Cache nicht vergiftet") = Some((stempel.0, stempel.1, config.clone()));
        Ok(config)
    }

    /// Das dekodierte Turnierlogo — aus dem Zwischenstand, solange die
    /// Marke des Inhalts gleich bleibt (siehe `logo_cache`). `marke` ist
    /// dieselbe, die die Route als `ETag` ausgibt.
    fn logo_bytes(&self, logo: &crate::config::LogoConfig, marke: &str) -> Option<Arc<Vec<u8>>> {
        use base64::Engine;
        let schluessel = marke.to_string();
        {
            let cache = self.logo_cache.lock().expect("Logo-Cache nicht vergiftet");
            if let Some((gemerkt, bytes)) = cache.as_ref() {
                if *gemerkt == schluessel {
                    return Some(bytes.clone());
                }
            }
        }
        let bytes = Arc::new(
            base64::engine::general_purpose::STANDARD
                .decode(logo.data.as_bytes())
                .ok()?,
        );
        *self.logo_cache.lock().expect("Logo-Cache nicht vergiftet") =
            Some((schluessel, bytes.clone()));
        Some(bytes)
    }

    /// Werbebilder des Court-Monitors — aus dem Zwischenstand, solange
    /// sich der Ordner nicht geändert hat (siehe `ads_cache`).
    fn ads(&self) -> Vec<String> {
        let zeit = std::fs::metadata(&self.monitor_dir)
            .and_then(|m| m.modified())
            .ok();
        if let Some(zeit) = zeit {
            {
                let cache = self.ads_cache.lock().expect("Ads-Cache nicht vergiftet");
                if let Some((gemerkt, namen)) = cache.as_ref() {
                    if *gemerkt == zeit {
                        return namen.clone();
                    }
                }
            }
            let namen = monitor::list_ads(&self.monitor_dir);
            // Eine LEERE Liste nicht merken: `list_ads` liefert sie auch
            // dann, wenn das Verzeichnis-Lesen fehlschlug (Virenscanner,
            // kurze Sperre). Gemerkt würde daraus „dieses Turnier hat
            // keine Werbung", bis jemand ein Bild anfasst oder die App neu
            // startet — vorher versuchte es jeder Abruf erneut. Dieselbe
            // Abwägung wie beim Config-Zwischenstand: Fehler merkt man
            // sich nicht (Review-Fund 18.08.2026).
            if !namen.is_empty() {
                *self.ads_cache.lock().expect("Ads-Cache nicht vergiftet") =
                    Some((zeit, namen.clone()));
            }
            return namen;
        }
        // Kein Zeitstempel lesbar (Ordner fehlt): wie bisher direkt lesen.
        monitor::list_ads(&self.monitor_dir)
    }

    /// Die als „Leisten-Sponsor" markierten Dateinamen — aus dem
    /// Zwischenstand, solange die Datei unverändert ist (siehe `bar_cache`).
    /// Fehlt die Datei (Normalfall ohne Sponsoren), bleibt es beim leeren
    /// Ergebnis ohne Zwischenstand.
    fn ad_bar(&self) -> Arc<HashSet<String>> {
        let pfad = self.monitor_dir.join(monitor::AD_BAR_FILE);
        let stempel = std::fs::metadata(&pfad)
            .and_then(|m| Ok((m.modified()?, m.len())))
            .ok();
        let Some(stempel) = stempel else {
            return Arc::new(HashSet::new());
        };
        {
            let cache = self.bar_cache.lock().expect("Bar-Cache nicht vergiftet");
            if let Some((gemerkt, namen)) = cache.as_ref() {
                if *gemerkt == stempel {
                    return namen.clone();
                }
            }
        }
        let namen = Arc::new(monitor::read_ad_bar(&pfad));
        *self.bar_cache.lock().expect("Bar-Cache nicht vergiftet") = Some((stempel, namen.clone()));
        namen
    }

    /// Wie [`Self::app_config`], aber **mit** dem Lesefehler.
    ///
    /// Für Entscheidungen, bei denen „Datei gerade nicht lesbar" etwas ganz
    /// anderes bedeutet als „so ist es eingestellt": Der Widerruf der
    /// Turnierleitungs-Zugänge etwa. Die Datei wird beim Speichern
    /// abgeschnitten und neu geschrieben — trifft der Abgleich genau dieses
    /// Fenster, ergäbe die Standard-Konfiguration „keine Geräte
    /// zugelassen", und alle Turnierleitungs-Geräte flögen für einen Takt
    /// hinaus.
    pub fn app_config_result(&self) -> Result<AppConfig, String> {
        // Datei-Stempel statt Datei-Inhalt: Ändert sich weder Zeitpunkt
        // noch Größe, ist es dieselbe Datei — dann genügt die gemerkte
        // Fassung. Beides gemeinsam, weil eine Änderung innerhalb
        // derselben Zeitstempel-Auflösung sonst durchrutschen könnte.
        // Der Datei-Stempel entscheidet (siehe `app_config_arc_result`);
        // hier nur die Kopie für Aufrufer, die eine eigene Fassung
        // brauchen.
        self.app_config_arc_result().map(|c| (*c).clone())
    }

    /// Lässt `f` den **geteilten** In-Memory-Stand ändern (`shared_config`,
    /// dasselbe `Arc<Mutex<_>>` wie `AppState.config`) und schreibt das
    /// Ergebnis nach `config.json` — für TlAction-Ausführungen, die
    /// `AppConfig`-Zustand pflegen (Panel-Profile, Spec tl-web-panelsystem,
    /// ADR 0025). `f` darf die Änderung selbst ablehnen (`Err`) — dann
    /// bleiben weder Datei noch In-Memory-Stand angetastet.
    ///
    /// **Kein Lost-Update mehr zwischen den zwei Schreibpfaden:** Der Guard
    /// auf `shared_config` wird über den ganzen Lesen-Ändern-Schreiben-
    /// Zyklus gehalten (nicht nur ums Schreiben) — dasselbe Schloss, das
    /// auch `commands::mutate_config` (Tauri-Commands: `save_config`,
    /// `tl_device_add`, …) hält. Zwei praktisch gleichzeitige Aufrufe (z. B.
    /// zwei Geräte, die im selben Moment ein Profil speichern, ODER ein
    /// Profil-Speichern gleichzeitig mit einer Einstellungsänderung im
    /// Setup-Assistenten) laufen so strikt nacheinander, und jeder sieht als
    /// Ausgangspunkt garantiert den Stand des jeweils anderen — „der letzte
    /// gewinnt" (von der Spec ausdrücklich erlaubt), nie ein stiller
    /// Datenverlust.
    pub(crate) fn mutate_app_config<T>(
        &self,
        f: impl FnOnce(&mut AppConfig) -> Result<T, relay_proto::TlResponse>,
    ) -> Result<T, relay_proto::TlResponse> {
        let mut guard = self
            .shared_config
            .lock()
            .expect("Config-Mutex nicht vergiftet");
        let mut config = guard.clone();
        let result = f(&mut config)?;
        config.save_to(&self.config_path).map_err(|e| {
            relay_proto::TlResponse::err(
                relay_proto::TlErrorCode::NotAllowed,
                format!("Konfiguration nicht schreibbar: {e}"),
            )
        })?;
        // Der Antwortcache der Übersicht trägt Werte aus der Konfiguration
        // (Hallen-Farben, Aufruf-Timer) — nach einem Schreibvorgang ist er
        // überholt (Spec monitor-livestand-push, S1).
        self.tablet.bump_overview_rev();
        // Gemerkte Fassung verwerfen: Der Datei-Stempel ist zwar frisch,
        // aber eine Änderung innerhalb derselben Zeitstempel-Auflösung
        // (gleiche Größe, gleiche Zeit) würde sonst übersehen. Bei einem
        // Schreibvorgang, den wir selbst auslösen, ist das billig zu
        // vermeiden.
        *self
            .config_cache
            .lock()
            .expect("Config-Cache nicht vergiftet") = None;
        *guard = config;
        Ok(result)
    }

    /// Lädt die Geräte→Target-Zuweisungen frisch von der Platte. Ein
    /// Target ist entweder eine CourtID (klassischer Court-Monitor) oder
    /// ein Info-Display (`InfoOverview` / `InfoPreparation`).
    pub fn monitor_assignments(&self) -> HashMap<String, relay_proto::MonitorTarget> {
        monitor::read_assignments(&self.assignments_path)
    }
}

/// Startet den Server auf `0.0.0.0:8088` und bedient ihn, bis der Task
/// abgebrochen wird.
pub async fn run(ctx: Arc<ServerCtx>) -> std::io::Result<()> {
    let ctx_fuer_takt = ctx.clone();
    let app = Router::new()
        // TV-Launcher: kurze Root-Adresse landet auf einem Auswahl-Menü
        // (Fernbedienung statt langer ?halle=-URLs). Kurz-Pfade leiten direkt.
        .route("/", get(tv_page))
        .route("/tv", get(tv_page))
        .route("/status", get(index))
        .route("/alle", get(|| async { Redirect::to("/info/overview") }))
        .route("/next", get(|| async { Redirect::to("/info/preparation") }))
        .route("/h/{n}", get(hall_short))
        .route("/court/{id}", get(court_page))
        .route("/courts", get(courts_list))
        .route("/felder", get(lobby_page))
        .route("/court/{id}/display", get(monitor_page))
        .route("/court/{id}/state", get(monitor_state))
        .route("/monitor", get(monitor_device_page))
        .route("/monitor/state", get(monitor_device_state))
        .route("/qr/{id}", get(qr_svg))
        .route("/flags/{file}", get(flag_route))
        .route("/ads/{file}", get(ad_image))
        .route("/health", get(health))
        // Ablesestand der Perf-Zähler (Spec monitor-livestand-push, S0) —
        // nur hier, nicht am Relay (siehe `debug_perf`).
        .route("/debug/perf", get(debug_perf))
        .route("/info/overview", get(info_overview_page))
        .route("/info/preparation", get(info_preparation_page))
        .route("/info/preparation/state", get(info_preparation_state))
        .route("/info/winners", get(info_winners_page))
        .route("/info/winners/state", get(info_winners_state))
        .route("/info/club-logo", get(info_club_logo))
        .route("/info/logo", get(info_tournament_logo))
        .route("/info/announce/freetext", get(info_announce_freetext))
        .route("/info/announce/jobs", get(info_announce_jobs))
        .route("/info/ad", get(info_ad_page))
        .route("/info/ad/state", get(info_ad_state))
        .route("/combo", get(combo_page))
        .route("/combo/state", get(combo_state))
        // Turnierleitungs-Oberfläche. Ohne freigeschaltetes Feature und
        // gültigen Zugang antworten beide Routen abweisend — die Prüfung
        // sitzt in `tl::authorize` und liest die Konfiguration frisch, damit
        // ein Widerruf ohne Neustart greift.
        .route("/tl", get(tl_page))
        .route("/tl/api/state", get(tl_state))
        .route("/tl/api/command", post(tl_command))
        // Punktverlauf on-demand (AK-5): gleicher Pfad wie über den Relay,
        // damit tl.html in beiden Modi identisch abruft.
        .route("/tl/api/timeline/{match_id}", get(tl_timeline))
        .route("/tl/api/officials/{official_id}", get(tl_official_detail))
        // TL-Push (Spec tl-web-push): Anstoß-Kanal `{"rev":n}` — nie Daten,
        // die holt die Seite über `/tl/api/state`. Auth im ersten Frame
        // (Browser-WebSockets können keine Header setzen, und der Zugang
        // gehört nie in eine URL). Siehe `tl_ws_upgrade`.
        .route("/tl-ws", get(tl_ws_upgrade))
        .route("/result", post(result))
        .route("/tablet-log", post(tablet_log))
        .route("/pi-log", post(pi_log))
        .route("/ws", get(ws_upgrade))
        // Court-Monitor-Nudge (A1, ADR 0016): niedrig-latente Anzeige. `court`
        // gesetzt → nur dieses Feld (Court-Monitor), fehlt es → alle Felder
        // (Feld-Übersicht). Siehe `monitor_ws_upgrade`.
        .route("/monitor-ws", get(monitor_ws_upgrade))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", TABLET_PORT)).await?;
    tracing::info!("Tablet-Server lauscht auf http://{}", lan_host());
    // TL-Erkennungstakt (Spec tl-web-push): lebt und stirbt mit dem
    // LAN-Server — er versorgt dessen Antwort-Cache und `/tl-ws`-Nudges.
    // Im reinen Cloud-Modus läuft weder Server noch Takt; dort erkennt der
    // Relay-Client Änderungen selbst (TICK) und der Relay nudgt.
    //
    // Der Wächter ist Absicht: Das Stoppen der Übertragung bricht diese
    // Funktion mitten in `axum::serve` ab (`handle.abort()` in
    // `commands.rs`) — ein bloßes `takt.abort()` DANACH liefe nie, und ein
    // fallengelassenes `JoinHandle` beendet in Tokio gar nichts, es löst
    // die Aufgabe nur ab. Jedes Stoppen/Starten hinterließe damit einen
    // weiteren Takt, der bis zum Programmende sekündlich weiterrechnet
    // (Review-Fund 18.08.2026). `Drop` läuft auch beim Abbruch.
    let _takt = TaktWaechter(tokio::spawn(tl_push_takt(ctx_fuer_takt)));
    axum::serve(listener, app).await
}

/// Beendet den TL-Erkennungstakt, sobald der Server-Task endet — egal ob
/// regulär oder per Abbruch.
struct TaktWaechter(tokio::task::JoinHandle<()>);

impl Drop for TaktWaechter {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Zentraler TL-Erkennungstakt (Spec tl-web-push): baut den Zustand EINMAL
/// pro Sekunde, legt die fertige Antwort in den Cache und nudgt die
/// `/tl-ws`-Zuhörer bei jeder neuen Revision. Ersetzt die frühere
/// Je-Gerät-und-Anfrage-Rechnung (Snapshot-Clone + zwei Serialisierungen
/// pro Poll) durch genau eine pro Takt — und macht Änderungen in unter
/// einer Sekunde sichtbar statt erst beim nächsten 2-s-Poll.
async fn tl_push_takt(ctx: Arc<ServerCtx>) {
    let mut takt = tokio::time::interval(Duration::from_secs(1));
    takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut letzte_rev = 0u64;
    loop {
        takt.tick().await;
        let now = now_ms();
        // Nur arbeiten, wenn jemand zusieht: kein offener Push-Kanal und
        // seit einer Minute kein Abruf → gar nichts tun. Sonst läse ein
        // Turnier ohne ein einziges TL-Gerät sekündlich die Config von
        // Platte, klonte den BTP-Schnappschuss und serialisierte den
        // vollen Zustand (Review-Fund 18.08.2026). Diese Prüfung ist ein
        // Atomar-Lesen plus ein Blick in eine Liste.
        if !ctx.tablet.tl_interest(now) {
            continue;
        }
        // Config je Takt frisch (wie der Relay-TICK): Ein- und Ausschalten
        // von TL-Web greift ohne Neustart.
        let config = ctx.app_config();
        if !config.tl_web.enabled {
            continue;
        }
        let state = crate::tablet::tl::build_state_with_rev(&ctx.tablet, &config, now);
        let etag = format!("\"{}-{}\"", ctx.tablet.process_tag(), state.rev);
        let rev = state.rev;
        let json = serde_json::to_string(&state).unwrap_or_default();
        ctx.tablet.set_tl_state_cache(rev, etag, json, now);
        if rev != letzte_rev {
            letzte_rev = rev;
            ctx.tablet.notify_tl(rev);
        }
    }
}

/// LAN-Adresse `<ip>:<port>` für Tablet-URLs und QR-Codes.
pub fn lan_host() -> String {
    match local_ip_address::local_ip() {
        Ok(ip) => format!("{ip}:{TABLET_PORT}"),
        Err(_) => format!("localhost:{TABLET_PORT}"),
    }
}

// ─────────────────────────────── HTTP-Routen ──────────────────────────────

/// TV-Launcher (`/` und `/tv`): Vollbild-Auswahlmenü, per Fernbedienung
/// bedienbar — so muss am Smart-TV nur einmal die kurze Adresse getippt werden
/// statt langer `?halle=`-URLs.
async fn tv_page(State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    // Konfigurierten badhub-Liveticker einsetzen, damit der Launcher auch die
    // öffentlichen Online-Anzeigen je Halle anbieten kann. Defensiv: nur eine
    // saubere http(s)-URL ohne Zeichen, die das JS-String-Literal aufbrechen
    // könnten (Anführungszeichen/Backslash/Spitzklammern/Whitespace) – sonst
    // leer (keine Online-Kacheln).
    let live = ctx.app_config().badhub.live_url;
    let safe = (live.starts_with("https://") || live.starts_with("http://"))
        && !live
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\' | '<' | '>' | '`'));
    let body = assets::TV_HTML.replace("__LIVE_URL__", if safe { &live } else { "" });
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Kurz-Pfad `/h/{n}` → leitet auf die Court-Übersicht der n-ten Halle
/// (1-basiert, Hallen alphabetisch sortiert). Unbekannte Nummer → alle Hallen.
/// Spart das Tippen langer `?halle=`-URLs an der TV-Fernbedienung.
async fn hall_short(State(ctx): State<Arc<ServerCtx>>, Path(n): Path<usize>) -> Redirect {
    let mut halls: Vec<String> = ctx
        .tablet
        .overview()
        .into_iter()
        .map(|c| c.location)
        .filter(|l| !l.is_empty())
        .collect();
    halls.sort();
    halls.dedup();
    match n.checked_sub(1).and_then(|i| halls.get(i)) {
        Some(h) => Redirect::to(&format!("/info/overview?halle={}", path_encode(h))),
        None => Redirect::to("/info/overview"),
    }
}

/// Landing-Page (Debug, `/status`): zeigt die Tablet-Adressen je Court. Die URL
/// trägt die stabile CourtID, der angezeigte Text den Feldnamen.
async fn index(State(ctx): State<Arc<ServerCtx>>) -> Html<String> {
    let host = lan_host();
    let courts = ctx.tablet.courts();
    let mut rows = String::new();
    for c in &courts {
        // Anzeigename inkl. Halle bei Mehr-Hallen-Turnieren ("Halle 2 · 6").
        let label = ctx.tablet.court_display_label(c.id);
        rows.push_str(&format!(
            "<li><b>{}</b> &mdash; <a href=\"/court/{id}\">/court/{id}</a> \
             &middot; <a href=\"/qr/{id}\">QR</a></li>",
            html_escape(&label),
            id = c.id,
        ));
    }
    if courts.is_empty() {
        rows.push_str(
            "<li><i>Noch keine Courts geladen – bts-light muss zuerst mit BTP \
             verbunden sein.</i></li>",
        );
    }
    Html(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>bts-light Tablet-Server</title>\
         <style>body{{font-family:system-ui;max-width:40rem;margin:2rem auto;padding:0 1rem}}\
         code{{background:#f1f5f9;padding:.1em .4em;border-radius:.25rem}}\
         li{{margin:.3rem 0}}</style>\
         <h1>&#127992; bts-light Tablet-Server</h1>\
         <p>LAN-Adresse <code>http://{host}</code></p>\
         <h2>Spielfelder</h2><ul>{rows}</ul>"
    ))
}

/// Liefert die Tablet-UI für ein Feld (per CourtID; kein Caching – immer
/// frisch). Der Platzhalter `__COURT_ID__` trägt die Identität,
/// `__COURT_LABEL__` den Feldnamen für die Anzeige.
async fn court_page(
    State(ctx): State<Arc<ServerCtx>>,
    Path(court_id): Path<i64>,
) -> impl IntoResponse {
    let label = court_label_for(&ctx, court_id);
    // PIN fürs Einstellungs-Menü (Feldwechsel) – Live-Config. NUR Ziffern
    // (Bedien-PIN; leer → Default „0000"). Ziffern sind in einem JS-String-
    // Literal unkritisch → kein Escape nötig (Code-Review-Hinweis: html_escape
    // wäre für einen JS-Kontext der falsche Escaper).
    let pin: String = ctx
        .app_config()
        .tablet_settings_pin
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(8)
        .collect();
    let pin = if pin.is_empty() {
        "0000".to_string()
    } else {
        pin
    };
    tracing::info!("Tablet-Seite ausgeliefert für Feld {court_id} ('{label}')");
    let body = TABLET_HTML
        .replace("__COURT_ID__", &court_id.to_string())
        .replace("__COURT_LABEL__", &html_escape(&label))
        .replace("__TABLET_PIN__", &pin);
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Feldliste (CourtID + Anzeige-Label) für den Feldwechsel im PIN-Menü des
/// Tablets – so kann das Tablet ohne QR-Scan auf ein anderes Feld umschalten,
/// und die Felder-Lobby (`/felder`) baut daraus ihre Kacheln.
/// Bewusst ohne Auth (wie die anderen Anzeige-Routen): Nutzung nur im Hallen-LAN.
/// Enthält die Spielernamen der laufenden Partie (`pairing`) – dieselbe Exposition
/// wie Zähltablett und Court-Monitor, die die Namen im LAN ohnehin anzeigen.
async fn courts_list(State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    // Spielernamen eines Teams kompakt verbinden ("Müller / Schmidt").
    let names = |players: &[crate::btp::model::BtpPlayer]| {
        players
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(" / ")
    };
    let items: Vec<serde_json::Value> = ctx
        .tablet
        .courts()
        .into_iter()
        .map(|c| {
            // Belegt = ein Tablet zählt das Feld bereits (Doppelbelegung-Schutz).
            // Paarung/Untertitel für die Felder-Lobby, damit man sieht, was auf
            // dem Feld läuft, bevor man es antippt.
            let m = ctx.tablet.match_for_court(c.id);
            let (pairing, sub) = match &m {
                Some(m) => {
                    let a = names(&m.team1);
                    let b = names(&m.team2);
                    let pairing = if a.is_empty() && b.is_empty() {
                        String::new()
                    } else {
                        format!("{a} — {b}")
                    };
                    let sub = format!("{} {}", m.draw_name, m.round_name)
                        .trim()
                        .to_string();
                    (pairing, sub)
                }
                None => (String::new(), String::new()),
            };
            serde_json::json!({
                "id": c.id,
                "label": ctx.tablet.court_display_label(c.id),
                "occupied": ctx.tablet.court_occupied(c.id),
                "pairing": pairing,
                "sub": sub,
            })
        })
        .collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::Value::Array(items)),
    )
}

/// Felder-Lobby (`/felder`): Start-Seite fürs Zähltablett. Listet alle Felder
/// (Live-Belegung via `/courts`-Poll), Tippen auf ein Feld führt auf
/// `/court/{id}` (zählen bzw. – bei Belegung – die bestehende Übernahme-Abfrage).
async fn lobby_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::LOBBY_HTML),
    )
}

/// Fester (verbandsweiter) Token zum Weiterleiten der Tablet-Logs an badhub –
/// derselbe wie Diagnose-/Pi-Log. Nicht geheim (Bedien-Token, nicht-PII-Daten).
const TABLET_LOG_TOKEN: &str = "d896d5c45f1dfe72d324be2da0dcc8031e447809f9a3c1ce";

#[derive(serde::Deserialize)]
struct TabletLogQuery {
    #[serde(default)]
    court: i64,
}

/// Nimmt das Diagnoselog eines Zähltablets entgegen (LAN, ohne Auth wie die
/// anderen Hallen-Routen): legt es lokal unter `<log_dir>/tablet-logs/court-<id>.log`
/// ab (über „Logs öffnen" greifbar) UND leitet es – sofern Internet da ist – an
/// die badhub-Cloud weiter (fire-and-forget, scheitert still ohne Uplink).
async fn tablet_log(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<TabletLogQuery>,
    body: String,
) -> impl IntoResponse {
    if body.len() > 2 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }
    let court_id = q.court;
    // 1) Lokal beim Turnier-PC ablegen (auch offline verfügbar).
    let dir = ctx.log_dir.join("tablet-logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("court-{court_id}.log")), &body);
    // 2) An die Cloud weiterleiten – Geräte-ID inkl. install_id, damit sich
    //    verschiedene PCs/Turniere nicht gegenseitig überschreiben.
    let install = ctx.app_config().install_id;
    let device_id = if install.is_empty() {
        format!("court-{court_id}")
    } else {
        format!("{install}-court-{court_id}")
    };
    let http = ctx.http.clone();
    tokio::spawn(async move {
        let _ = http
            .post("https://badhub.de/api/tablet_log.php")
            .bearer_auth(TABLET_LOG_TOKEN)
            .header("X-Device-Id", device_id)
            .header(header::CONTENT_TYPE, "text/plain")
            .timeout(std::time::Duration::from_secs(8))
            .body(body)
            .send()
            .await;
    });
    StatusCode::OK
}

#[derive(serde::Deserialize)]
struct PiLogQuery {
    /// Geräte-ID des Pi-Monitors (= `pi-<CPU-Serial>`), vom Pi-Startskript
    /// mitgeschickt. Bestimmt den Dateinamen + die Cloud-Geräte-ID.
    #[serde(default)]
    device: String,
}

/// Nimmt das Verbindungslog eines Pi-Court-Monitors entgegen (LAN, ohne Auth
/// wie die anderen Hallen-Routen). Einheitlich mit den Tablets: der Pi postet
/// im LAN an den PC (plain HTTP – kein TLS/keine Pi-Uhr nötig), der PC legt es
/// lokal unter `<log_dir>/pi-logs/<device>.log` ab UND leitet es – sofern
/// Internet da ist – an die badhub-Cloud weiter (fire-and-forget).
async fn pi_log(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<PiLogQuery>,
    body: String,
) -> impl IntoResponse {
    if body.len() > 2 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE;
    }
    // Geräte-ID auf dateinamen-/header-sichere Zeichen reduzieren.
    let id: String = q
        .device
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    let id = if id.is_empty() {
        "pi-unbekannt".to_string()
    } else {
        id
    };
    // 1) Lokal beim Turnier-PC ablegen (auch offline verfügbar, „Logs öffnen").
    let dir = ctx.log_dir.join("pi-logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{id}.log")), &body);
    // 2) An die Cloud weiterleiten (gleicher Token + Endpoint wie der frühere
    //    Direkt-Upload der Pis). Bewusst OHNE install_id-Präfix (anders als bei
    //    Tablets): die Pi-Serial ist global eindeutig → ein Cloud-Log je
    //    physischem Pi über alle Turniere (gut für Ferndiagnose desselben Geräts).
    let http = ctx.http.clone();
    tokio::spawn(async move {
        let _ = http
            .post("https://badhub.de/api/pi_log.php")
            .bearer_auth(TABLET_LOG_TOKEN)
            .header("X-Device-Id", id)
            .header(header::CONTENT_TYPE, "text/plain")
            .timeout(std::time::Duration::from_secs(8))
            .body(body)
            .send()
            .await;
    });
    StatusCode::OK
}

/// Löst die CourtID auf ihre Anzeige-Bezeichnung auf. Bei Mehr-Hallen-
/// Turnieren `"{Halle} · {Feld}"` (z. B. „Halle 2 · 6"), sonst nur der
/// Feldname. Leer, wenn die ID kein bekanntes Feld ist (z. B. nach einem
/// Turnierwechsel).
fn court_label_for(ctx: &ServerCtx, court_id: i64) -> String {
    ctx.tablet.court_display_label(court_id)
}

/// QR-Code (SVG), der auf die Tablet-URL des Felds (per CourtID) zeigt.
async fn qr_svg(Path(court_id): Path<i64>) -> impl IntoResponse {
    let url = format!("http://{}/court/{}", lan_host(), court_id);
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

/// Optionaler `device`-Query-Param. Wird von den Info-Pages
/// (`overview.html`, `preparation.html`, `ad.html`) mitgegeben, damit
/// der State-Poll als „Lebenszeichen" gezaehlt wird – sonst gilt der
/// Pi auf einer Info-Page als offline, weil `record_monitor_poll`
/// nur in `/monitor/state` aufgerufen wird (Code-Review v0.9.22).
#[derive(serde::Deserialize, Default)]
struct DeviceHeartbeat {
    #[serde(default)]
    device: Option<String>,
    /// Was den Abruf ausgelöst hat: `push` (ein Nudge kam) oder `poll`
    /// (Fallback-Takt). **Additiv** angehängt (Spec monitor-livestand-push,
    /// S0) — eine Seite aus einem älteren Stand sendet ihn nicht, und ihr
    /// Abruf zählt dann als `poll`, was er auch ist.
    #[serde(default)]
    src: Option<String>,
}

/// Markiert das Geraet als „gesehen", falls eine Device-ID im Query
/// kam. Geteilte Hilfsfunktion fuer alle Info-State-Endpoints und
/// `/health`.
fn note_heartbeat(ctx: &ServerCtx, q: &DeviceHeartbeat) {
    if let Some(d) = q.device.as_deref() {
        if !d.is_empty() && d.len() <= 64 {
            // Rueckgabewert (Fernbefehl) ignorieren — Info-Pages
            // verarbeiten Commands ueber den separaten /monitor/state-Poll.
            let _ = ctx.tablet.record_monitor_poll(d);
        }
    }
}

/// Höchstalter des `/health`-Antwortcaches (Spec monitor-livestand-push,
/// S1). Kurz genug, dass eine übersehene Änderungsquelle nur eine
/// Viertelsekunde durchschlägt — und lang genug, dass bei zwanzig Anzeigen
/// im 250-ms-Takt aus rund siebzig Bauten je Sekunde vier werden.
const OVERVIEW_CACHE_TTL_MS: u64 = 250;

/// Status-Schnappschuss für die bts-light-Oberfläche. Optional
/// `?device=<id>` als Lebenszeichen-Markierung von der Info-Page.
async fn health(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    // Ohne Kopie: Diese Route ist der Zustands-Abruf der Feld-Übersicht
    // und der Kombi-Anzeigen — sie kommt im 250-ms-Takt je Gerät und war
    // damit die heißeste verbliebene Stelle, an der die ganze
    // Konfiguration samt Turnierlogo kopiert wurde (Review-Fund
    // 18.08.2026).
    let cfg = ctx.app_config_arc();
    let ct = &cfg.call_timer;
    let jetzt = monitor::now_ms();
    let (courts_json, etag) = uebersicht_json(&ctx, &cfg, jetzt);

    // Unveränderter Stand → Bestätigung statt Inhalt. Spart dem
    // Fallback-Poll die ganzen Nutzdaten (Spec monitor-livestand-push, S1);
    // die Anzeige zeigt einfach weiter, was sie hat.
    if marke_bekannt(&headers, &etag) {
        ctx.tablet
            .perf()
            .note_health(perf::Quelle::aus_query(q.src.as_deref()), 0);
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag.as_str())],
            String::new(),
        )
            .into_response();
    }

    // Umschlag je Abruf: `serverNowMs` ist bei jedem Abruf ein anderer und
    // gehört deshalb nicht in den Cache. Von Hand zusammengesetzt statt über
    // `serde_json::json!`, damit die zwischengespeicherte Feld-Liste nicht
    // erst wieder geparst und neu serialisiert werden muss.
    let json = format!(
        // Server-Zeit für den Uhr-Offset des Tablets (Pausen-`endsAt` in
        // Server-Zeit, sonst Drift durch abweichende Geräteuhren, v0.9.32);
        // `callTimer` (camelCase wie im MonitorState), damit die
        // Multifeld-Übersicht „Zeit seit Aufruf" gleich gaten kann (Plan 4).
        "{{\"ok\":true,\"courts\":{courts_json},\"serverNowMs\":{jetzt},\
         \"callTimer\":{{\"enabled\":{},\"secondCallMinutes\":{},\"thirdCallMinutes\":{}}}}}",
        ct.enabled, ct.second_call_minutes, ct.third_call_minutes,
    );
    ctx.tablet
        .perf()
        .note_health(perf::Quelle::aus_query(q.src.as_deref()), json.len() as u64);
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, etag.as_str()),
        ],
        json,
    )
        .into_response()
}

/// Die Feld-Liste als fertiges JSON samt ihrer Marke — aus dem Antwortcache,
/// wenn er zur aktuellen Revision passt und jung genug ist, sonst frisch
/// gebaut (Spec monitor-livestand-push, S1).
///
/// Der Cache ist **Beschleuniger, nicht Wahrheit**: Jeder Zweifel führt zum
/// Direktbau. Zwei Bedingungen müssen stimmen — die Revision (steigt bei
/// Nudge, neuem BTP-Stand und Config-Schreibvorgang) **und** die Hart-TTL.
/// Die TTL ist das Sicherheitsnetz gegen eine Quelle, an die niemand
/// gedacht hat: Schlimmstenfalls ist die Anzeige eine Viertelsekunde alt,
/// statt bis zum nächsten Ereignis falsch zu bleiben.
fn uebersicht_json(ctx: &ServerCtx, cfg: &AppConfig, jetzt: u64) -> (String, String) {
    let rev = ctx.tablet.overview_rev();
    if let Some(c) = ctx.tablet.overview_cache() {
        if c.rev == rev && jetzt.saturating_sub(c.gebaut_ms) < OVERVIEW_CACHE_TTL_MS {
            return (c.courts_json, c.etag);
        }
    }
    // Hallen-Farben (Spec hallen-farben) für die Multifeld-Übersicht —
    // gleiche kanonische Hallenliste wie Desktop und TL-Web.
    let mut courts = ctx.tablet.overview();
    crate::hall_colors::paint(&mut courts, cfg, &ctx.tablet.hall_names());
    let courts_json = serde_json::to_string(&courts).unwrap_or_else(|_| "[]".to_string());
    // Prozess-Kennung in der Marke: Nach einem Neustart beginnt die Revision
    // wieder bei null, und eine Anzeige mit gemerkter Marke bekäme sonst ein
    // 304 auf einen ganz anderen Zustand (Muster `tl_state`).
    let etag = format!("\"ov-{}-{}\"", ctx.tablet.process_tag(), rev);
    ctx.tablet
        .set_overview_cache(rev, etag.clone(), courts_json.clone(), jetzt);
    (courts_json, etag)
}

/// `GET /debug/perf` — der Ablesestand der Perf-Zähler (Spec
/// monitor-livestand-push, S0).
///
/// **Bewusst nur am LAN-Server.** Der Relay steht im Internet; dort hätte
/// eine parameterlose, unauthentifizierte Route, die Lastdaten aller
/// Namespaces zusammenfasst, nichts zu suchen. Im LAN ist sie das
/// Ablesegerät für den Messlauf — und trägt ausschließlich Zahlen
/// (Wächter-Test `debug_perf_enthaelt_keine_personendaten` in `perf.rs`).
async fn debug_perf(State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    let s = ctx.tablet.perf().snapshot();
    let mut v = serde_json::to_value(s).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        // Ohne Uhrzeit ist aus zwei Abrufen keine Rate zu bilden.
        obj.insert("serverNowMs".into(), monitor::now_ms().into());
    }
    ([(header::CACHE_CONTROL, "no-store")], Json(v))
}

// ─────────────────────────────── Court-Monitor ────────────────────────────

/// Rendert `monitor.html` mit den Platzhaltern. `base` ist der URL-Präfix
/// für Unter-Ressourcen (`/` im LAN), `mode` ist `fixed` oder `device`.
fn render_monitor_html(mode: &str, base: &str, court_label: &str) -> String {
    assets::MONITOR_HTML
        .replace("__MODE__", mode)
        .replace("__BASE__", base)
        .replace("__COURT_LABEL__", &html_escape(court_label))
}

/// Liefert die Court-Monitor-Anzeige fest für ein Feld
/// (`/court/{id}/display`, per CourtID).
async fn monitor_page(
    State(ctx): State<Arc<ServerCtx>>,
    Path(court_id): Path<i64>,
) -> impl IntoResponse {
    let label = court_label_for(&ctx, court_id);
    tracing::info!("Court-Monitor-Seite (fest) ausgeliefert für Feld {court_id} ('{label}')");
    let body = render_monitor_html("fixed", "/", &label);
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Liefert die Court-Monitor-Anzeige im Geräte-Modus (`/monitor`) – das
/// Gerät bekommt sein Feld erst über die Zuweisung im Tool.
async fn monitor_device_page() -> impl IntoResponse {
    let body = render_monitor_html("device", "/", "");
    ([(header::CACHE_CONTROL, "no-store")], Html(body))
}

/// Anzeige-Zustand eines fest verdrahteten Feldes (per CourtID), im
/// Sekundentakt gepollt.
async fn monitor_state(
    State(ctx): State<Arc<ServerCtx>>,
    Path(court_id): Path<i64>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    let label = court_label_for(&ctx, court_id);
    let court = ctx.tablet.monitor_court(court_id);
    // EIN Config-Zugriff für alles (vorher zwei, jeder mit voller Kopie
    // inklusive Turnierlogo) und die Werbebild-Liste aus dem
    // Zwischenstand statt per Verzeichnis-Lesen (Analyse 18.08.2026).
    let cfg = ctx.app_config_arc();
    let state = monitor::build_monitor_state(
        court_id,
        label,
        hall_color_for(&ctx, &cfg, court_id),
        court,
        &cfg.court_monitor,
        &cfg.call_timer,
        ctx.ads(),
    );
    // Wie bei `/health` selbst serialisiert, um die Antwortgröße zu kennen
    // (Spec monitor-livestand-push, S0).
    let json = serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_string());
    ctx.tablet
        .perf()
        .note_court_state(perf::Quelle::aus_query(q.src.as_deref()), json.len() as u64);
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_TYPE, "application/json"),
        ],
        json,
    )
}

/// Effektive Hallen-Farbe des Felds (Spec hallen-farben) — über die
/// kanonische Turnier-Hallenliste aufgelöst; `None` bei Ein-Hallen-
/// Turnieren oder Feldern ohne Halle.
fn hall_color_for(ctx: &ServerCtx, config: &AppConfig, court_id: i64) -> Option<String> {
    let hall = ctx.tablet.court_hall(court_id);
    if hall.is_empty() {
        return None;
    }
    crate::hall_colors::color_for(config, &ctx.tablet.hall_names(), &hall)
}

/// Query-Parameter der Geräte-Modus-Abfrage: die Geräte-ID.
#[derive(serde::Deserialize)]
struct DeviceQuery {
    device: String,
    /// Wie bei [`DeviceHeartbeat`]: was den Abruf ausgelöst hat (Spec
    /// monitor-livestand-push, S0). Fehlt er, zählt der Abruf als `poll`.
    #[serde(default)]
    src: Option<String>,
}

/// Anzeige-Zustand für ein Monitor-Gerät: löst die Feld-Zuweisung auf,
/// registriert den Poll und hängt einen offenen Fernbefehl an.
async fn monitor_device_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceQuery>,
) -> impl IntoResponse {
    let device = q.device;
    if device.is_empty() || device.len() > 64 {
        return (StatusCode::BAD_REQUEST, "Ungültige Geräte-ID").into_response();
    }
    let command = ctx.tablet.record_monitor_poll(&device);
    let assignment = ctx.monitor_assignments().get(&device).cloned();
    let mut state = match assignment {
        Some(relay_proto::MonitorTarget::Court { court_id }) => {
            let label = court_label_for(&ctx, court_id);
            let court_data = ctx.tablet.monitor_court(court_id);
            // Wie in `monitor_state`: eine Config ohne Kopie, Werbebilder
            // aus dem Zwischenstand.
            let cfg = ctx.app_config_arc();
            monitor::build_monitor_state(
                court_id,
                label,
                hall_color_for(&ctx, &cfg, court_id),
                court_data,
                &cfg.court_monitor,
                &cfg.call_timer,
                ctx.ads(),
            )
        }
        // Nicht-Court-Targets (Info, Ad): der Pi soll auf die passende
        // Anzeige-HTML umleiten. Wir liefern einen minimalen MonitorState
        // mit `redirect_to`; die monitor.html springt darauf.
        Some(ref target) if target.redirect_path().is_some() => {
            let mut s = monitor::unassigned_monitor_state(&device);
            s.unassigned = false;
            let mut path = target.redirect_path();
            // Kombi nebeneinander (Hochformat je Feld): globaler Schalter aus
            // den Court-Monitor-Einstellungen hängt `&dir=v` an die Kombi-URL.
            if matches!(target, relay_proto::MonitorTarget::CourtCombo { .. })
                && ctx.app_config_arc().court_monitor.combo_vertical
            {
                if let Some(p) = path.as_mut() {
                    p.push_str("&dir=v");
                }
            }
            s.redirect_to = path;
            s
        }
        // Sollte unerreichbar sein (redirect_path() ist Some für alle
        // Nicht-Court-Varianten), aber strukturiert exhaustiv:
        Some(_) | None => monitor::unassigned_monitor_state(&device),
    };
    state.command = command;
    state.device_code = device_code(&device);
    // Wie `monitor_state` selbst serialisiert, um die Antwortgröße zu
    // kennen (Spec monitor-livestand-push, S0).
    let json = serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_string());
    ctx.tablet
        .perf()
        .note_court_state(perf::Quelle::aus_query(q.src.as_deref()), json.len() as u64);
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_TYPE, "application/json"),
        ],
        json,
    )
        .into_response()
}

// ─────────────────────────────── Info-Monitore ────────────────────────────
//
// Read-only Hallen-Displays, kein Bezug zu einem bestimmten Feld. Werden
// per Master-Image oder URL auf einem Pi geöffnet:
//   /info/overview      → Court-Übersicht (Hallen × Felder × aktuelles Spiel)
//   /info/preparation   → Spiele in Vorbereitung (Liste, gerufene zuerst)
// Beide unterstützen URL-Parameter:
//   ?halle=<Name>       → filtert auf eine Halle
//   ?rotate=90|180|270  → Pivot-Monitor um N° drehen (CSS-Transform).

/// Liefert die HTML der Court-Übersicht. Pollt selbst `/health`.
async fn info_overview_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::OVERVIEW_HTML),
    )
}

/// Liefert die HTML des Vorbereitungs-Monitors. Pollt
/// `/info/preparation/state`.
async fn info_preparation_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::PREPARATION_HTML),
    )
}

/// Sieger-/Podium-Monitor. Pollt `/info/winners/state` für die Disziplin-Podien.
async fn info_winners_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::WINNERS_HTML),
    )
}

/// JSON-Zustand für den Sieger-Monitor: Podien aller ausgespielten Disziplinen.
async fn info_winners_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    let results = ctx.tablet.discipline_results();
    // `selected` = vom Operator gewählte Disziplin (Draw-ID). Der Monitor zeigt
    // genau diese (keine Rotation); `null` → Begrüßungsbild.
    let selected = ctx.tablet.winners_selection();
    // Turniername für die Footer-Zeile (über der Disziplin) mitliefern.
    let tournament = ctx.tablet.tournament_name();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({
            "disciplines": results,
            "selected": selected,
            "tournament": tournament,
        })),
    )
}

#[derive(serde::Deserialize)]
struct ClubLogoQuery {
    /// BTP-Vereinsname (z. B. „BC Tempelhof (Berlin)").
    name: String,
}

/// Vereinslogo für den Sieger-Monitor: matcht den Vereinsnamen gegen die
/// Badhub-Vereinsliste und liefert das Bild lokal aus (auch für LAN-TVs ohne
/// eigenes Internet). Kein Treffer / kein Logo → 404 (der Monitor blendet das
/// `<img>` dann per `onerror` weg).
async fn info_club_logo(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<ClubLogoQuery>,
) -> impl IntoResponse {
    match crate::tablet::club_logos::resolve(&ctx.config.badhub, &ctx.http, &q.name).await {
        Some((content_type, bytes)) => (
            [
                (header::CONTENT_TYPE, content_type),
                // Logos sind stabil – TVs dürfen sie lange cachen.
                (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            bytes,
        )
            .into_response(),
        // Auch den Fehltreffer cachen: Ohne Cache-Header fragt der Browser bei
        // jedem Neuaufbau (Poll alle ~2 s, viele Vereine ohne Logo) erneut an.
        // Kürzer als der Treffer, damit ein später in badhub ergänztes Logo
        // binnen einer Stunde erscheint.
        None => (
            [(header::CACHE_CONTROL, "public, max-age=3600".to_string())],
            StatusCode::NOT_FOUND,
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
struct FreetextQuery {
    #[serde(default)]
    hall: String,
    #[serde(default)]
    since: u64,
}

/// Freitext-Ansagen für eine Halle (`id > since`). Ein Ansage-Slave pollt das
/// vom Master, um Freitexte seiner Halle (oder „alle") anzusagen.
async fn info_announce_freetext(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<FreetextQuery>,
) -> impl IntoResponse {
    let items = ctx.tablet.freetext_since(&q.hall, q.since);
    ([(header::CACHE_CONTROL, "no-store")], Json(items))
}

/// Ansage-Aufträge der Turnierleitung für eine Halle (`id > since`).
///
/// Derselbe Weg wie beim Freitext, nur mit Struktur statt fertigem Text: Der
/// Aufruf wird erst am Ansage-Gerät zu Worten — mit dessen Stimme, Gong und
/// Namenskorrektur, damit er klingt wie jeder andere Aufruf auch.
async fn info_announce_jobs(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<FreetextQuery>,
) -> impl IntoResponse {
    let items = ctx.tablet.announce_jobs_since(&q.hall, q.since, now_ms());
    ([(header::CACHE_CONTROL, "no-store")], Json(items))
}

/// Liefert die HTML der Werbe-Anzeige. Pollt `/info/ad/state` für die
/// Bilder-Liste; mode/file/device kommen über den Query-String.
async fn info_ad_page() -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-store")], Html(assets::AD_HTML))
}

/// JSON-Zustand für die Werbe-Anzeige: aktuelle Bilder-Liste +
/// Rotations-Intervall. Liest die Bilder aus dem Court-Ad-Verzeichnis
/// (gleicher Pool wie der Court-Monitor) und nutzt
/// `MonitorConfig.ad_interval_s` als Intervall.
async fn info_ad_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    let ads = ctx.ads();
    // Die als „Leisten-Sponsor" markierten Bilder gesondert ausweisen — die
    // obere Leiste zeigt genau diese (neben dem Turnierlogo), ohne die
    // Vollbild-Rotation (`ads`) anzufassen. Nur existierende Dateien.
    let bar = ctx.ad_bar();
    let bar_ads: Vec<&String> = ads.iter().filter(|f| bar.contains(*f)).collect();
    // Config nur EINMAL laden — sowohl Intervall als auch das Logo-Flag
    // daraus. Ohne Kopie: Hier wird nur gelesen, und die Fassung trägt
    // das Base64-Logo mit.
    let config = ctx.app_config_arc();
    let payload = serde_json::json!({
        "ads": ads,
        "barAds": bar_ads,
        "hasLogo": !config.tournament_logo.data.is_empty(),
        "intervalS": config.court_monitor.ad_interval_s.max(1),
    });
    ([(header::CACHE_CONTROL, "no-store")], Json(payload))
}

/// Liefert das **Turnierlogo** als Bild (für die obere Leiste der
/// Anzeigeseiten). Quelle ist die Host-Konfiguration (`tournament_logo`,
/// base64). Leeres Logo → 404, damit ein `onerror` in der Seite sauber
/// degradiert.
///
/// Caching bewusst asymmetrisch: Ein vorhandenes Logo ist stabil und darf lange
/// gecacht werden (datensparsam auf TVs, die es sonst dauernd neu laden). Der
/// **404** (noch kein Logo) wird nur kurz gecacht, damit ein frisch gesetztes
/// Logo binnen ~1 Min erscheint. Ein *Wechsel* eines bestehenden Logos schlägt
/// entsprechend erst nach dem 200-Cache-Fenster durch — akzeptabel, weil das
/// Logo praktisch einmal je Turnier gesetzt wird.
async fn info_tournament_logo(
    State(ctx): State<Arc<ServerCtx>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let config = ctx.app_config_arc();
    let logo = &config.tournament_logo;
    if logo.data.is_empty() {
        return (
            [(header::CACHE_CONTROL, "public, max-age=60")],
            StatusCode::NOT_FOUND,
        )
            .into_response();
    }
    // Kennung aus dem Inhalt: Damit kann eine Anzeige nach Ablauf der
    // Cache-Frist mit ~200 Byte bestätigt bekommen, dass ihr Bild noch
    // stimmt, statt megabyteweise dasselbe erneut zu laden — bei zwanzig
    // TVs macht das den Unterschied (Analyse 18.08.2026).
    //
    // Bewusst über den Inhalt und nicht über `(Länge, MIME)`: Diese Marke
    // ist zugleich der Schlüssel des Dekodier-Zwischenstands, und zwei
    // verschiedene Logos gleicher Base64-Länge und gleichen Typs hätten
    // sonst auch **frischen** Anzeigen dauerhaft die alten Bytes geliefert
    // (Review-Fund 18.08.2026). Den Logo-Schreibpfad (`save_config`)
    // erreicht der ServerCtx nicht, es gäbe also kein Netz darunter.
    let etag = bild_marke(logo.data.as_bytes());
    if marke_bekannt(&headers, &etag) {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, "public, max-age=300"),
            ],
        )
            .into_response();
    }
    // Dekodiert zwischengespeichert: Ohne das rechnete jeder Abruf die
    // vollen Base64-Daten neu aus.
    match ctx.logo_bytes(logo, &etag) {
        Some(bytes) => {
            let mime = if logo.mime.is_empty() {
                "image/png".to_string()
            } else {
                logo.mime.clone()
            };
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "public, max-age=300".to_string()),
                    (header::ETAG, etag),
                ],
                // Aus dem geteilten Zwischenstand in den Antwort-Body:
                // eine Kopie statt einer Neu-Dekodierung.
                bytes.as_ref().clone(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Liefert die HTML der Kombi-Anzeige (mehrere Felder als Bänder). Die
/// gewünschten CourtIDs + optionale `device`/`rotate` kommen über den
/// Query-String, die Live-Daten holt die Seite über `/combo/state`.
async fn combo_page() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(assets::COMBO_HTML),
    )
}

/// Query der Kombi-Anzeige: `courts=1,2,3` (kommasepariert) plus
/// optionaler `device`-Heartbeat.
#[derive(serde::Deserialize, Default)]
struct ComboQuery {
    #[serde(default)]
    courts: String,
    #[serde(default)]
    device: Option<String>,
}

/// JSON-Zustand für die Kombi-Anzeige: filtert die Felder-Übersicht auf
/// die in `?courts=` genannten CourtIDs und behält deren Reihenfolge.
/// Greift auf denselben `overview()`-Datenstand zurück wie `/health`.
async fn combo_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<ComboQuery>,
) -> impl IntoResponse {
    // Heartbeat (analog Info-Pages, v0.9.22): Poll als Lebenszeichen.
    if let Some(d) = q.device.as_deref() {
        if !d.is_empty() && d.len() <= 64 {
            let _ = ctx.tablet.record_monitor_poll(d);
        }
    }
    // Gewünschte CourtIDs in der angegebenen Reihenfolge parsen.
    // Max. 3 Felder (UI-Cap auch serverseitig spiegeln) und Duplikate
    // entfernen — sonst rendert combo.html bei einer manuell gebauten
    // URL (?courts=1,1,1,…) unleserlich viele/doppelte Bänder
    // (Code-Review v0.9.28 MEDIUM/LOW).
    let mut wanted: Vec<i64> = Vec::new();
    for id in q
        .courts
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
    {
        if !wanted.contains(&id) {
            wanted.push(id);
        }
        if wanted.len() >= 3 {
            break;
        }
    }
    let overview = ctx.tablet.overview();
    // Je gewünschter ID das passende Feld heraussuchen, Reihenfolge
    // beibehalten (nicht die overview-Reihenfolge). Unbekannte IDs
    // werden übersprungen.
    let courts: Vec<&crate::tablet::state::CourtOverview> = wanted
        .iter()
        .filter_map(|id| overview.iter().find(|c| c.court_id == *id))
        .collect();
    // serverNowMs reicht combo.html die Server-Zeit für den Pausen-Countdown
    // durch (Pi hat evtl. keine synchrone Uhr; endsAt steht in Server-Zeit).
    let payload = serde_json::json!({ "courts": courts, "serverNowMs": monitor::now_ms() });
    ([(header::CACHE_CONTROL, "no-store")], Json(payload))
}

/// JSON-Zustand für den Vorbereitungs-Monitor: alle eingeplanten,
/// ruf-baren Spiele (beide Teams stehen fest), gerufene zuerst sortiert.
/// Aufgerufene Spiele tragen `call.hall` + `call.called_at_ms` —
/// derselbe Datenstand, der auch `commands::preparation_candidates`
/// liefert, nur als reines HTTP-JSON statt Tauri-Command.
async fn info_preparation_state(
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<DeviceHeartbeat>,
) -> impl IntoResponse {
    note_heartbeat(&ctx, &q);
    let snapshot = match ctx.tablet.snapshot_clone() {
        Some(s) => s,
        None => {
            return (
                [(header::CACHE_CONTROL, "no-store")],
                Json(serde_json::json!({ "candidates": [] })),
            )
                .into_response();
        }
    };
    let calls = ctx.tablet.preparation_calls();
    let manual_halls = ctx.tablet.manual_halls();
    let auto_halls = ctx.tablet.auto_hall_store().halls();
    // Hallen-Farben (Spec hallen-farben) einmal je Antwort auflösen.
    let hallen_farben =
        crate::hall_colors::effective_hall_colors(&ctx.app_config_arc(), &ctx.tablet.hall_names());

    // Erst nur Ordnungsschlüssel + Halle sammeln (Muster `tl.rs::build_state`,
    // **derselbe** gemeinsame Helfer wie an den übrigen Sortier-Stellen —
    // sonst zeigte diese Liste eine andere Reihenfolge als TL-Web/Desktop).
    let mut ordered: Vec<(
        crate::tablet::assign::ManualOrderSortKey,
        &crate::btp::model::BtpMatch,
        String,
    )> = snapshot
        .matches
        .iter()
        .filter(|m| {
            m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
        })
        .map(|m| {
            let call = calls.iter().find(|c| c.match_id == m.id);
            let manual_hall = manual_halls.get(&m.id).map(String::as_str);
            let called_hall = call.and_then(|c| c.location_id).and_then(|lid| {
                snapshot
                    .locations
                    .iter()
                    .find(|l| l.id == lid)
                    .map(|l| l.name.as_str())
            });
            let (hall, _, key) = crate::tablet::assign::resolve_and_sort_key(
                &ctx.config,
                &snapshot,
                m,
                manual_hall,
                called_hall,
                auto_halls.get(&m.id).map(String::as_str),
                call.is_some(),
                ctx.tablet.queue_order_store(),
            );
            (key, m, hall)
        })
        .collect();
    ordered.sort_by_key(|(key, _, _)| *key);

    let candidates: Vec<serde_json::Value> = ordered
        .into_iter()
        .map(|(_, m, hall)| {
            let call_info = calls.iter().find(|c| c.match_id == m.id).map(|c| {
                let call_hall = c
                    .location_id
                    .and_then(|lid| {
                        snapshot
                            .locations
                            .iter()
                            .find(|l| l.id == lid)
                            .map(|l| l.name.clone())
                    })
                    .unwrap_or_default();
                (call_hall, c.called_at_ms)
            });
            // Die Farbe gehört zur ANGEZEIGTEN Halle (Review 2026-08-16):
            // preparation.html rendert den Punkt neben `call.hall` — bei
            // einem Aufruf muss dessen Halle die Farbe stellen (wie der
            // Cloud-Weg über build_prepared_list), sonst widersprächen
            // sich LAN- und Cloud-TV, sobald Kaskaden- und Aufruf-Halle
            // auseinanderfallen. Ohne Aufruf gilt die Kaskaden-Halle.
            let anzeige_halle = match &call_info {
                Some((call_hall, _)) if !call_hall.is_empty() => call_hall.clone(),
                _ => hall.clone(),
            };
            let call = call_info.map(|(call_hall, called_at_ms)| {
                serde_json::json!({
                    "hall": call_hall,
                    "called_at_ms": called_at_ms,
                })
            });
            let manual = ctx.tablet.queue_order_store().rank(m.id).is_some();
            serde_json::json!({
                "match_id": m.id,
                "label": format!("{} {}", m.draw_name, m.round_name).trim().to_string(),
                "team1": m.team1.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                "team2": m.team2.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                "match_num": m.match_num,
                "planned_time": m.planned_time,
                "draw_id": m.draw_id,
                "call": call,
                "hall": hall,
                "hall_color": crate::hall_colors::farbe_fuer(&hallen_farben, &anzeige_halle),
                "manual": manual,
            })
        })
        .collect();

    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "candidates": candidates })),
    )
        .into_response()
}

/// Liefert eine gebündelte SVG-Länderflagge (`/flags/GER.svg`).
async fn flag_route(Path(file): Path<String>) -> impl IntoResponse {
    match assets::flag_svg(&file) {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Flagge nicht gefunden").into_response(),
    }
}

/// Liefert ein hochgeladenes Werbebild aus dem `court-ads`-Verzeichnis.
///
/// Mit Marke (`ETag`) und kurzer Cache-Frist. Vorher stand hier `no-store`,
/// und weil die Anzeigen ihr Bild alle `ad_interval_s` (Standard 10 s)
/// wechseln, lud jedes Gerät dabei jedes Mal die vollen Bilddaten neu — bei
/// 1 MB rund 360 MB je Stunde und Gerät, im Cloud-Betrieb über die
/// Internetleitung (Analyse 18.08.2026).
///
/// Bewusst **kein** `immutable`: Zwar vergibt `add_court_ad` eindeutige
/// Namen (`ad-<ms>.<endung>`), aber das Verzeichnis liegt offen — wer eine
/// Datei von Hand hineinlegt und später ersetzt, bekäme sonst tagelang das
/// alte Bild. Die Marke aus Größe und Änderungszeit macht den Austausch
/// billig sichtbar, ohne das Bild dafür zu lesen.
async fn ad_image(
    State(ctx): State<Arc<ServerCtx>>,
    Path(file): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !monitor::is_safe_image_name(&file) {
        return (StatusCode::NOT_FOUND, "Nicht gefunden").into_response();
    }
    let pfad = ctx.monitor_dir.join(&file);
    let etag = match tokio::fs::metadata(&pfad).await {
        Ok(meta) => {
            // Nanosekunden statt Millisekunden: Eine gleich große
            // Ersetzung innerhalb derselben Millisekunde bliebe sonst
            // unbemerkt (Review-Fund 18.08.2026), und feiner kostet nichts.
            let zeit = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("\"ad-{}-{zeit}\"", meta.len())
        }
        Err(_) => return (StatusCode::NOT_FOUND, "Werbebild nicht gefunden").into_response(),
    };
    if marke_bekannt(&headers, &etag) {
        // Unverändert: ~200 Byte statt des ganzen Bildes, und die Datei
        // muss dafür nicht einmal gelesen werden.
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, AD_CACHE_CONTROL),
            ],
        )
            .into_response();
    }
    match tokio::fs::read(&pfad).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, monitor::image_mime(&file).to_string()),
                (header::CACHE_CONTROL, AD_CACHE_CONTROL.to_string()),
                (header::ETAG, etag),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Werbebild nicht gefunden").into_response(),
    }
}

/// Cache-Frist der Bild-Routen. Fünf Minuten sind lang genug, dass die
/// Rotation der Anzeigen (Sekundentakt) ohne eine einzige Anfrage auskommt,
/// und kurz genug, dass ein ausgetauschtes Bild zeitnah durchschlägt.
const AD_CACHE_CONTROL: &str = "public, max-age=300";

/// Marke eines Bildes: Länge plus ein Streuwert über den Inhalt. Muss nur
/// innerhalb eines Programmlaufs stabil sein und sich bei anderem Inhalt
/// ändern — beides leistet der Standard-Streuer. Gleiches Format wie im
/// Relay (`bild_marke` dort), damit beide Betriebsarten gleich aussehen.
fn bild_marke(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("\"img-{}-{:x}\"", bytes.len(), hasher.finish())
}

/// Kennt der Abrufer die Marke bereits? Prüft `If-None-Match` so, wie es
/// RFC 9110 vorsieht: eine **Liste** von Marken, `*` als Joker, und der
/// Vergleich ist der schwache (das `W/`-Präfix zählt nicht mit).
///
/// Der naive Gleichheitstest wäre nicht falsch, aber still wirkungslos:
/// Ein Zwischenspeicher auf dem Weg (nginx vor dem Relay) darf eine Marke
/// abschwächen, und dann käme dauerhaft der volle Inhalt zurück statt der
/// Bestätigung (Review-Fund 18.08.2026).
fn marke_bekannt(headers: &axum::http::HeaderMap, etag: &str) -> bool {
    let Some(feld) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let schwach = |m: &str| m.trim().trim_start_matches("W/").to_string();
    let gesucht = schwach(etag);
    feld.split(',')
        .any(|m| m.trim() == "*" || schwach(m) == gesucht)
}

// ─────────────────────────────── Ergebnis → BTP ───────────────────────────

/// Nimmt das Endergebnis vom Tablet entgegen und schreibt es nach BTP.
async fn result(
    State(ctx): State<Arc<ServerCtx>>,
    Json(body): Json<ResultBody>,
) -> Json<ResultResponse> {
    Json(process_result(&ctx, &body).await)
}

/// Liest den Zugang eines Turnierleitungs-Geräts aus dem `Authorization`-
/// Kopf und schlägt das Gerät nach.
///
/// Der Zugang steht bewusst **nur** im Kopf und nie im Pfad: Pfade landen in
/// Zugriffsprotokollen, Kopfzeilen nicht.
fn tl_device(ctx: &ServerCtx, headers: &axum::http::HeaderMap) -> Option<crate::config::TlDevice> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    crate::tablet::tl::authorize(&ctx.app_config_arc(), token)
}

/// Die Turnierleitungs-Oberfläche.
///
/// Bewusst **ohne** Zugangsprüfung: Die Seite selbst enthält keine
/// Turnierdaten, sondern holt sie erst über die Schnittstelle — und die
/// prüft. Wer sie ohne Zugang öffnet, sieht nur den Hinweis, wie er einen
/// bekommt. Eine Prüfung hier brächte nichts und würde den Kopplungsablauf
/// (Adresse aufrufen, Zugang aus dem Fragment übernehmen) unmöglich machen.
async fn tl_page() -> impl IntoResponse {
    Html(assets::TL_HTML)
}

/// Der Anzeige-Zustand für die Turnierleitungs-Oberfläche.
/// Antwort-Header mit dem Panel-Profil des aufrufenden Geräts (Spec
/// tl-web-panelsystem, ADR 0025) — derselbe Name wie `X_TL_ACTIVE_PROFILE`
/// im Relay (`relay/src/main.rs`), damit `tl.html` LAN und Cloud identisch
/// lesen kann. Auf `/tl/api/state` gesetzt, auch bei 304 (Header werden
/// unabhängig vom gecachten Body immer gesendet).
const X_TL_ACTIVE_PROFILE: &str = "x-tl-active-profile";

async fn tl_state(
    State(ctx): State<Arc<ServerCtx>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let Some(device) = tl_device(&ctx, &headers) else {
        return (StatusCode::UNAUTHORIZED, "Kein gültiger Zugang.").into_response();
    };
    // „Es sieht jemand zu" — hält den Erkennungstakt am Laufen, auch wenn
    // gerade kein Push-Kanal offen ist (Seite im Poll-Fallback).
    ctx.tablet.note_tl_request(now_ms());
    // Antwort-Cache des Erkennungstakts (Spec tl-web-push): Der Takt baut
    // den Zustand einmal zentral pro Sekunde — die Anfragen aller Geräte
    // werden damit zu Cache-Reads statt je Anfrage Snapshot zu klonen und
    // zweimal zu serialisieren. Nur FRISCHE Einträge zählen (3 s: Takt 1 s
    // plus Luft) — ist der Cache kalt oder alt (Takt läuft nicht, z. B.
    // Übertragung gerade gestartet), rechnet der Handler wie eh und je
    // selbst. Der Cache ist Beschleuniger, nicht Wahrheit.
    let (etag, json) = match ctx.tablet.tl_state_cache() {
        Some(cache) if now_ms().saturating_sub(cache.gebaut_ms) <= 3_000 => {
            (cache.etag, cache.json)
        }
        _ => {
            // Dieselbe Revision wie im Cloud-Weg: Ein Gerät im Hallennetz
            // und eines aus dem Internet müssen mit derselben Zahl
            // denselben Stand meinen, sonst träfe die Altersprüfung am
            // Turnier-PC zufällige Entscheidungen.
            let state = crate::tablet::tl::build_state_with_rev(
                &ctx.tablet,
                &ctx.app_config_arc(),
                now_ms(),
            );
            // Die Prozess-Kennung gehört in den ETag: Nach einem Neustart
            // der App beginnt die Revision wieder bei 1, und ein Gerät mit
            // gemerkter Fassung „1" bekäme sonst „unverändert" auf einen
            // völlig anderen Turnierstand.
            let etag = format!("\"{}-{}\"", ctx.tablet.process_tag(), state.rev);
            (etag, serde_json::to_string(&state).unwrap_or_default())
        }
    };
    // Die Seite schickt ihre letzte Fassung mit. Hat sich nichts geändert,
    // spart „unverändert" den ganzen Stand — auf demselben Rechner, der
    // nebenher BTP und die Tablets bedient.
    let unveraendert = marke_bekannt(&headers, &etag);
    let mut response = if unveraendert {
        (StatusCode::NOT_MODIFIED, [(header::ETAG, etag.as_str())]).into_response()
    } else {
        (
            StatusCode::OK,
            [
                (header::ETAG, etag.as_str()),
                (header::CONTENT_TYPE, "application/json"),
            ],
            json,
        )
            .into_response()
    };
    // Frisch bei JEDER Antwort, auch bei 304 — konsistent mit dem
    // Cloud-Pfad (`relay::tl_state_route`). Leer (kein Profil gewählt) ist
    // ein gültiger, erlaubter Wert (Standardprofil); nur ein ungültiger
    // Header-Wert (sollte am gepflegten `profile_id` nie vorkommen) bleibt
    // ohne Header statt die Antwort scheitern zu lassen.
    if let Ok(value) = axum::http::HeaderValue::from_str(&device.profile_id) {
        response.headers_mut().insert(X_TL_ACTIVE_PROFILE, value);
    }
    response
}

/// Punktverlauf eines Matches für die TL-Oberfläche (AK-5) — on-demand,
/// nie Teil des Zustands-Pushes (Mobilfunk-Budget). Gleicher Zugang wie
/// `tl_state`; 404 heißt ehrlich „kein Verlauf" (Papier-Ergebnis).
async fn tl_timeline(
    State(ctx): State<Arc<ServerCtx>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(match_id): axum::extract::Path<i64>,
) -> impl IntoResponse {
    if tl_device(&ctx, &headers).is_none() {
        return (StatusCode::UNAUTHORIZED, "Kein gültiger Zugang.").into_response();
    }
    match ctx.tablet.timeline_store().timeline_json(match_id) {
        Some(json) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            json,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "Zu diesem Spiel liegt kein Punktverlauf vor.",
        )
            .into_response(),
    }
}

/// Sperrlisten und Einsätze **eines** Schiedsrichters (Spec
/// schiedsrichter-management) — bewusst on-demand und nie Teil des
/// Zustands-Pushes: Sperrlisten kodieren persönliche Beziehungen und
/// gehören nicht in den Stand, den jedes gekoppelte Gerät bekommt.
/// Gleicher Zugang wie `tl_state` (Geräte-Token).
async fn tl_official_detail(
    State(ctx): State<Arc<ServerCtx>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(official_id): axum::extract::Path<i64>,
) -> impl IntoResponse {
    if tl_device(&ctx, &headers).is_none() {
        return (StatusCode::UNAUTHORIZED, "Kein gültiger Zugang.").into_response();
    }
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        crate::tablet::tl::official_detail_json(&ctx.tablet, official_id),
    )
        .into_response()
}

/// Rumpf eines Kommandos: die Aktion plus die Vorgangskennung, mit der eine
/// Wiederholung als solche erkannt wird.
#[derive(serde::Deserialize)]
struct TlCommandBody {
    /// Fehlt sie, entfällt nur der Schutz gegen Doppelausführung — die
    /// Aktion selbst wird ganz normal bearbeitet und beantwortet. Ohne
    /// `default` liefe ein Rumpf ohne Kennung in eine Verarbeitungsfehler-
    /// Antwort, die kein reguläres Ergebnis wäre und die die Seite nicht
    /// auswerten könnte.
    #[serde(rename = "opId", default)]
    op_id: String,
    /// Der Stand, auf dem die Entscheidung beruhte. Ohne Angabe 0 — dann
    /// fehlt nur die Angabe im Protokoll, geprüft wird ohnehin fachlich.
    #[serde(rename = "viewRev", default)]
    view_rev: u64,
    action: relay_proto::TlAction,
}

/// Eine Aktion eines Turnierleitungs-Geräts.
async fn tl_command(
    State(ctx): State<Arc<ServerCtx>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<TlCommandBody>,
) -> impl IntoResponse {
    let Some(device) = tl_device(&ctx, &headers) else {
        return (StatusCode::UNAUTHORIZED, "Kein gültiger Zugang.").into_response();
    };
    Json(
        crate::tablet::tl::execute(
            &ctx,
            &device,
            &body.op_id,
            now_ms(),
            body.view_rev,
            body.action,
        )
        .await,
    )
    .into_response()
}

/// Validiert ein Endergebnis vom Tablet und schreibt es per `SENDUPDATE`
/// nach BTP. Von beiden Modi genutzt: vom LAN-`/result`-Handler und vom
/// Cloud-Relay-Client. Die Validierung ist zugleich die Sicherheits-
/// Mitigation des Cloud-Modus (Match-ID muss zum Court-Match passen,
/// Satzstand plausibel).
/// Ergebnis von [`derive_result`]: (bereinigte Sätze, `team1_won`,
/// BTP-`ScoreStatus`).
type DerivedResult = (Vec<(i64, i64)>, bool, i64);

/// Ist ein Satz `(a, b)` regulär zu Ende gespielt? Gegen das Zählformat
/// des Matches: erreicht bei `target` (2 Punkte Vorsprung) bzw. spätestens
/// beim `cap` (dort reicht 1 Punkt). Der Tablet-Weg erzwingt das
/// clientseitig; die manuelle Ergebnis-Eingabe der Turnierleitung
/// (`enter_result`) hat keinen solchen Zwang und braucht diese Prüfung,
/// damit ein noch LAUFENDER Satz nicht als gewonnener gewertet wird
/// (Plan-12a2-Review). Sinnvolle Defaults, falls das Format fehlt: 21/30.
pub(crate) fn set_is_complete(a: i64, b: i64, target: i64, cap: i64) -> bool {
    let target = if target > 0 { target } else { 21 };
    let cap = if cap >= target { cap } else { target.max(30) };
    let hi = a.max(b);
    let lo = a.min(b);
    if hi < target || hi > cap {
        return false; // Ziel nicht erreicht oder über dem Deckel
    }
    hi >= cap || hi - lo >= 2 // am Deckel reicht 1 Punkt, sonst 2 Vorsprung
}

/// Prüft eine ganze Satzliste gegen die Zählweise des Matches.
///
/// **Eine** Quelle für beide Wege, auf denen ein Ergebnis hereinkommt: das
/// Zähltablett (`process_result`) und die Turnierleitung
/// (`build_manual_result_update`). Vorher hing sie nur am zweiten, weil der
/// Tablet-Weg „ohnehin clientseitig zählt" — für ein von Hand **getipptes**
/// Ergebnis gilt das aber nicht, und aus dem Betrieb kam ein 27:25 in einem
/// Turnier, das bis 15 mit Deckel 21 spielt (R5: geprüft wird am Host).
///
/// Nur für **regulär ausgespielte** Ergebnisse. Bei Aufgabe, Kampflos oder
/// Disqualifikation bricht das Spiel mitten im Satz ab; dann ist genau der
/// unfertige Stand das, was nach BTP gehört.
pub(crate) fn sets_fit_format(sets: &[(i64, i64)], target: i64, cap: i64) -> Result<(), String> {
    if let Some(&(a, b)) = sets
        .iter()
        .find(|&&(a, b)| !set_is_complete(a, b, target, cap))
    {
        return Err(format!(
            "Satz {a}:{b} ist nicht regulär zu Ende gespielt (bis {}, Deckel {}).",
            if target > 0 { target } else { 21 },
            if cap >= target { cap } else { 30 },
        ));
    }
    Ok(())
}

/// Prüft die Satzliste und die Sonderfälle (Kampflos/Aufgabe) und leitet
/// daraus Sieger (`team1_won`) und BTP-`ScoreStatus` ab. **Eine** Quelle
/// der Wahrheit für den Tablet-Ergebnisweg (`process_result`) UND die
/// Ergebnis-Eingabe aus der Turnierleitung (`enter_result`) — R5 gilt so
/// für beide identisch.
///
/// - `walkover` = Kampflos (`ScoreStatus 1`), Sätze werden verworfen.
/// - `retired` = Aufgabe (`ScoreStatus 2`), Sieger explizit.
/// - `disqualified` = Disqualifikation (`ScoreStatus 3`), Sieger (= Gegner)
///   explizit; bereits gespielte Sätze bleiben (kann mitten im Spiel fallen).
/// - sonst: regulär (`0`), Sieger aus der Satzmehrheit.
///
/// Rückgabe: (bereinigte Sätze, team1_won, score_status).
pub(crate) fn derive_result(
    mut sets: Vec<(i64, i64)>,
    walkover: bool,
    retired: bool,
    disqualified: bool,
    winner: Option<i64>,
) -> Result<DerivedResult, String> {
    // Die Sonderfälle schließen sich gegenseitig aus – mehr als einer gesetzt
    // ist ein Fehler (das Status-Mapping würde sonst still einen bevorzugen).
    if [walkover, retired, disqualified]
        .iter()
        .filter(|&&x| x)
        .count()
        > 1
    {
        return Err("Kampflos, Aufgabe und Disqualifikation schließen sich aus.".to_string());
    }
    if sets.len() > 9 {
        return Err("Ungültige Satzanzahl.".to_string());
    }
    if sets
        .iter()
        .any(|&(a, b)| !(0..=99).contains(&a) || !(0..=99).contains(&b))
    {
        return Err("Ungültiger Satzstand.".to_string());
    }
    let result = if walkover {
        sets.clear();
        match winner {
            Some(1) => (true, 1),
            Some(2) => (false, 1),
            _ => return Err("Kampflos ohne gültigen Sieger.".to_string()),
        }
    } else if retired {
        match winner {
            Some(1) => (true, 2),
            Some(2) => (false, 2),
            _ => return Err("Aufgabe ohne gültigen Sieger.".to_string()),
        }
    } else if disqualified {
        // Disqualifikation: der Gegner des Disqualifizierten gewinnt; bereits
        // gespielte Sätze bleiben erhalten (Status 3, kann mitten im Spiel sein).
        match winner {
            Some(1) => (true, 3),
            Some(2) => (false, 3),
            _ => return Err("Disqualifikation ohne gültigen Sieger.".to_string()),
        }
    } else {
        if sets.is_empty() {
            return Err("Ungültige Satzanzahl.".to_string());
        }
        let team1_sets = sets.iter().filter(|(a, b)| a > b).count();
        let team2_sets = sets.iter().filter(|(a, b)| b > a).count();
        if team1_sets == team2_sets {
            return Err("Unentschiedener Satzstand – kein Sieger ermittelbar.".to_string());
        }
        (team1_sets > team2_sets, 0)
    };
    Ok((sets, result.0, result.1))
}

/// Baut aus einem Match + den von der Turnierleitung eingegebenen Sätzen
/// den BTP-`MatchUpdate` für `enter_result` (Plan 12a2) — **rein &
/// testbar**, ohne Netz/State. Prüft: bereits gewertet, unvollständige
/// Paarung, R5 ([`derive_result`]) und Satz-Vollständigkeit
/// ([`set_is_complete`]). Steht das Spiel auf einem Feld (`court_id`), wird
/// es im selben Update freigegeben und die Spieler ausgecheckt;
/// `on_court_since` (Aufruf-Stempel) liefert die Spieldauer.
pub(crate) fn build_manual_result_update(
    m: &BtpMatch,
    sets: Vec<(i64, i64)>,
    on_court_since: Option<u64>,
    now: u64,
    officials: Option<(i64, i64)>,
) -> Result<proto::MatchUpdate, String> {
    build_manual_result_update_opt(m, sets, on_court_since, now, false, officials)
}

/// Wie [`build_manual_result_update`], aber mit ausdrücklicher
/// Überschreib-Erlaubnis.
///
/// Ohne sie bleibt „bereits gewertet" ein Riegel — versehentlich lässt sich
/// so kein Ergebnis ersetzen. Wer überschreiben will, muss es sagen, und der
/// Aufrufer muss vorher geprüft haben, dass es folgenlos möglich ist
/// (`tl::correction_blocker`).
pub(crate) fn build_manual_result_update_opt(
    m: &BtpMatch,
    sets: Vec<(i64, i64)>,
    on_court_since: Option<u64>,
    now: u64,
    overwrite: bool,
    officials: Option<(i64, i64)>,
) -> Result<proto::MatchUpdate, String> {
    if m.winner.is_some() && !overwrite {
        return Err("Dieses Spiel ist in BTP bereits gewertet.".to_string());
    }
    if m.team1.is_empty() || m.team2.is_empty() {
        return Err("Die Paarung steht noch nicht fest.".to_string());
    }
    let (sets, team1_won, score_status) = derive_result(sets, false, false, false, None)?;
    sets_fit_format(&sets, m.scoring.target_score, m.scoring.cap_score)?;
    let (free_court_id, player_ids, duration_mins, end_ts_ms) =
        manual_finish_fields(m, on_court_since, now);
    Ok(proto::MatchUpdate {
        btp_match_id: m.id,
        draw_id: m.draw_id,
        planning_id: m.planning_id,
        sets,
        team1_won,
        duration_mins,
        score_status,
        free_court_id,
        player_ids,
        end_ts_ms,
        officials,
    })
}

/// Feld-Abschlussfelder eines manuell (Turnierleitung) gewerteten Spiels:
/// steht es auf einem Feld, wird es freigegeben (`free_court_id`), die Spieler
/// werden ausgecheckt (`player_ids`) und die Spieldauer aus dem Aufruf-Stempel
/// berechnet. Ohne Feld bleibt alles leer. Geteilt von regulärem Eintrag und
/// Disqualifikation.
fn manual_finish_fields(
    m: &BtpMatch,
    on_court_since: Option<u64>,
    now: u64,
) -> (Option<i64>, Vec<i64>, i64, Option<u64>) {
    // Die Dauer hängt NICHT am Feld (Review 2026-08-16): auch ein bereits
    // freigegebenes Spiel trägt seine gemessene Bruttozeit (der Aufrufer
    // reicht sie aus dem Zeiten-Store herein). Nur Feldfreigabe,
    // Auschecken und Endzeit je Spieler brauchen ein Feld. Unplausible
    // Dauern (über Nacht geparkt) gelten als unbekannt.
    let dur = on_court_since
        .and_then(|since| crate::tablet::match_times::plausible_duration_mins(since, now))
        .unwrap_or(0);
    match m.court_id {
        Some(cid) => {
            let ids: Vec<i64> = m
                .team1
                .iter()
                .chain(m.team2.iter())
                .map(|p| p.id)
                .filter(|&id| id != 0)
                .collect();
            (Some(cid), ids, dur, Some(now))
        }
        None => (None, Vec::new(), dur, None),
    }
}

/// Baut den BTP-`MatchUpdate` für eine **Disqualifikation** aus der
/// Turnierleitung (P3, ScoreStatus 3): `loser_team` (1/2) wird disqualifiziert,
/// der Gegner gewinnt. Bereits gespielte `sets` bleiben erhalten — eine
/// Disqualifikation kann mitten im Spiel fallen, daher **keine**
/// Satz-Vollständigkeitsprüfung. Rein & testbar.
///
/// **Bewusst akzeptiertes Risiko (Change-Gate Punkt 5):** Der Teilstand wird
/// NICHT auf Scoring-Plausibilität geprüft (nur der 0..=99-Bereich + Satzanzahl
/// in `derive_result`). Ein von der Turnierleitung eingetippter Zwischenstand
/// geht so, wie er ist, nach BTP — die Verantwortung dafür liegt bei der
/// Turnierleitung (jede Satz-Regel-Prüfung würde den „mitten im Spiel"-Zweck
/// von P3 verhindern).
pub(crate) fn build_manual_dq_update(
    m: &BtpMatch,
    loser_team: i64,
    sets: Vec<(i64, i64)>,
    on_court_since: Option<u64>,
    now: u64,
    officials: Option<(i64, i64)>,
) -> Result<proto::MatchUpdate, String> {
    if m.winner.is_some() {
        return Err("Dieses Spiel ist in BTP bereits gewertet.".to_string());
    }
    if m.team1.is_empty() || m.team2.is_empty() {
        return Err("Die Paarung steht noch nicht fest.".to_string());
    }
    let winner = match loser_team {
        1 => 2,
        2 => 1,
        _ => return Err("Ungültiges disqualifiziertes Team (1 oder 2).".to_string()),
    };
    let (sets, team1_won, score_status) = derive_result(sets, false, false, true, Some(winner))?;
    let (free_court_id, player_ids, duration_mins, end_ts_ms) =
        manual_finish_fields(m, on_court_since, now);
    Ok(proto::MatchUpdate {
        btp_match_id: m.id,
        draw_id: m.draw_id,
        planning_id: m.planning_id,
        sets,
        team1_won,
        duration_mins,
        score_status,
        free_court_id,
        player_ids,
        end_ts_ms,
        officials,
    })
}

/// Zeitfenster, in dem ein Wiederholungs-POST mit **identischem** bereits
/// geschriebenem Ergebnis als Bestätigung (statt Fehler) quittiert wird. 5 min:
/// großzügig über dem Client-Retry-Takt (5 s), deckt auch ein Tablet ab, das
/// nach längerem WLAN-Aussetzer denselben Stand erneut sendet. Ein langes
/// Fenster ist HIER sicher, weil `settled_ok` feldgenau vergleicht — eine echte
/// spätere Korrektur (TL-Reopen, andere Sätze) hat abweichende Felder und wird
/// deshalb NIE quittiert (fällt weiter auf Fehler), unabhängig vom TTL. Zudem
/// überschreibt jeder neue Write den Merker, sodass er stets den letzten Stand
/// trägt.
const RESULT_IDEMPOTENCY_TTL: u64 = 300_000;

/// Ist dieser Ergebnis-POST die **Wiederholung** eines bereits erfolgreich
/// nach BTP geschriebenen, **identischen** Ergebnisses? Nur dann darf
/// `process_result` mit `ok()` statt Fehler quittieren, obwohl das Feld
/// inzwischen geräumt oder neu belegt ist — sonst löschte das Tablet sein
/// `pendingResult` nie und wiederholte endlos (und ein zeitgleicher zweiter
/// POST auf ein noch belegtes Feld löste einen Doppel-Write aus).
///
/// **R5 (korrektheitskritisch — ein zu breites `ok()` = stiller
/// Ergebnisverlust):** Verglichen werden ausschließlich die **entscheidenden
/// Felder** `(sets, team1_won, score_status)` gegen den feldgenauen
/// „zuletzt-geschrieben"-Merker. Ein abweichender Payload (auch eine
/// sieger-gleiche Korrektur mit anderen Sätzen) ist eine veraltete oder falsche
/// Einreichung und liefert `false` → der Aufrufer gibt den originalen Fehler
/// zurück. Bewusst KEIN Snapshot-Sieger-Netz (das nur die Sieger-Seite prüfte
/// und Korrekturen fälschlich abwürgte — Review-BLOCKER).
fn settled_ok(ctx: &ServerCtx, body: &ResultBody, now: u64) -> bool {
    // Eingehendes Ergebnis ableiten — braucht das Match `m` NICHT. Die
    // Format-Prüfung (`sets_fit_format`, R5) ist hier bewusst nicht nötig:
    // Verglichen wird nur gegen den bereits VALIDIERT geschriebenen Stand.
    let raw_sets: Vec<(i64, i64)> = body.sets.iter().map(|s| (s.a, s.b)).collect();
    let Ok((sets, team1_won, score_status)) =
        derive_result(raw_sets, body.walkover, body.retired, false, body.winner)
    else {
        return false; // ungültiger Payload → keine Bestätigung
    };

    // Der „zuletzt-geschrieben"-Merker, gesetzt in `write_result_settled` VOR
    // `clear_court`. Liegt er im TTL und stimmen die drei entscheidenden Felder
    // überein → identischer Retry → quittieren. BEWUSST NUR dieser feldgenaue
    // Vergleich (kein Snapshot-Sieger-Netz): Ein fertig geschriebenes Match trägt
    // im BTP-Snapshot dauerhaft einen Sieger; ein reiner Sieger-Seiten-Vergleich
    // würde eine spätere TL-Ergebniskorrektur (Reopen, gleicher Sieger, ANDERE
    // Sätze) fälschlich als „erledigt" quittieren und still verwerfen
    // (Review-BLOCKER). Der feldgenaue Merker quittiert ausschließlich einen
    // wirklich identischen Wiederholungs-POST — eine Korrektur (abweichende
    // Sätze/Status) fällt weiter auf Fehler. Das großzügige TTL deckt auch ein
    // Tablet ab, das nach längerem Offline denselben Stand erneut sendet.
    ctx.tablet
        .direct_btp_write_since(body.match_id, now.saturating_sub(RESULT_IDEMPOTENCY_TTL))
        .is_some_and(|prev| {
            prev.sets == sets && prev.team1_won == team1_won && prev.score_status == score_status
        })
}

pub(crate) async fn process_result(ctx: &ServerCtx, body: &ResultBody) -> ResultResponse {
    // Zeitquelle wie im ganzen Modul (Unix-ms), einmal für den
    // Idempotenz-Check ermittelt.
    let now = now_ms();
    let Some(m) = ctx.tablet.match_for_court(body.court_id) else {
        // Feld nach erfolgreichem Write geräumt: Bevor das ein Fehler wird,
        // prüfen, ob dies nur die WIEDERHOLUNG des bereits geschriebenen,
        // IDENTISCHEN Ergebnisses ist — dann quittieren (das Tablet löscht sein
        // `pendingResult`, der Retry stoppt). Ein abweichender Payload fällt
        // weiter auf den Fehler durch (R5, veraltete Einreichung).
        return if settled_ok(ctx, body, now) {
            ResultResponse::ok()
        } else {
            ResultResponse::err("Kein Match auf diesem Court.")
        };
    };
    if m.id != body.match_id {
        // Feld inzwischen mit einem anderen Match belegt: dieselbe
        // Idempotenz-Prüfung (identischer Alt-Retry → ok, sonst Fehler).
        return if settled_ok(ctx, body, now) {
            ResultResponse::ok()
        } else {
            ResultResponse::err("Das Match auf dem Court hat inzwischen gewechselt.")
        };
    }

    let raw_sets: Vec<(i64, i64)> = body.sets.iter().map(|s| (s.a, s.b)).collect();
    let (sets, team1_won, score_status) =
        match derive_result(raw_sets, body.walkover, body.retired, false, body.winner) {
            Ok(v) => v,
            Err(e) => return ResultResponse::err(e),
        };
    // Gegen die Zählweise des Matches prüfen — auch hier, nicht nur auf dem
    // Weg der Turnierleitung. Das Tablett zählt zwar selbst korrekt, aber es
    // hat auch eine Eingabe für getippte Endstände, und die kam bisher
    // ungeprüft durch: In einem Turnier bis 15 mit Deckel 21 ließ sich ein
    // 27:25 speichern. Nur bei regulär ausgespielten Ergebnissen — eine
    // Aufgabe bricht den Satz mitten drin ab.
    if score_status == 0 {
        if let Err(e) = sets_fit_format(&sets, m.scoring.target_score, m.scoring.cap_score) {
            return ResultResponse::err(e);
        }
    }
    // Spieldauer = Bruttozeit (Spec `spielzeiten-prognose`, E1): erste
    // Feldzuweisung → Ergebnis-Eingang, in ganzen Minuten wie beim
    // Original-BTS. Quelle ist der persistierte Zeiten-Store (überlebt
    // App-Neustart und Feldwechsel). Eine KORREKTUR rechnet mit dem
    // ursprünglichen Ende (E3-Stempel) statt mit „jetzt" — sonst
    // überschriebe sie eine korrekte Duration mit Stunden. Kampflos (E1)
    // bleibt 0, und jenseits der Plausibilitätsgrenze (über Nacht
    // geparktes Feld) gilt die Dauer als unbekannt.
    // Korrektur-Anker nur bei REGULÄREM Erst-Stempel (Review 2026-08-16,
    // Runde 3): Ein irrtümlicher Backend-/TL-Web-Stempel (regular=false)
    // darf dem echten Tablet-Ergebnis nicht sein altes Ende unterschieben —
    // sonst gingen Duration 0 und eine rückdatierte LastTimeOnCourt nach
    // BTP. Eine Korrektur eines ECHTEN Tablet-Endes rechnet weiter mit dem
    // Original (E3).
    let end_ms = ctx
        .tablet
        .match_times_store()
        .entry(m.id)
        .filter(|e| e.regular)
        .and_then(|e| e.finished_ms)
        .unwrap_or_else(now_ms);
    let duration_mins = if score_status == 1 {
        0
    } else {
        ctx.tablet
            .brutto_start_ms(m.id, Some(body.court_id))
            .and_then(|since| crate::tablet::match_times::plausible_duration_mins(since, end_ms))
            .unwrap_or(0)
    };
    // Spielende stempeln (E3): genau einmal, beim ersten Host-Eingang —
    // eine spätere Korrektur ändert weder Zeit noch Einstufung. Regulär
    // (E11, Messwert für die Prognose) ist nur der ausgespielte Tablet-Weg
    // (ScoreStatus 0, kein Walkover/keine Aufgabe).
    ctx.tablet
        .match_times_store()
        .stamp_finished(m.id, score_status == 0, end_ms);
    // Spieler-BTP-IDs beider Teams — bekommen im selben Request das
    // Spielende (`LastTimeOnCourt` + `CheckedIn: false`).
    let player_ids: Vec<i64> = m
        .team1
        .iter()
        .chain(m.team2.iter())
        .map(|p| p.id)
        .filter(|&id| id != 0)
        .collect();
    let update = proto::MatchUpdate {
        btp_match_id: m.id,
        draw_id: m.draw_id,
        planning_id: m.planning_id,
        sets,
        team1_won,
        duration_mins,
        score_status,
        // Ergebnis + Feldfreigabe in EINEM Request: Courts-Block gibt das
        // Feld frei, das Match BEHÄLT seine CourtID (Turnier-Doku „wo
        // wurde gespielt"). Der frühere separate Freigabe-Request mit
        // „nacktem" Match-Knoten konnte das Ergebnis wieder entwerten.
        free_court_id: Some(body.court_id),
        player_ids,
        end_ts_ms: Some(end_ms),
        officials: ctx.tablet.officials_for_result(m.id),
    };

    // Log-Label: Das Tablet liefert sein courtLabel nicht auf jedem Pfad
    // (Turnier-Log 19.07.: 7× „Feld 38 ('')") — dann den Feldnamen aus
    // dem Snapshot nachschlagen, damit die Zeile lesbar bleibt.
    let court_label = if body.court_label.is_empty() {
        ctx.tablet
            .court_name_map()
            .remove(&body.court_id)
            .unwrap_or_else(|| "?".to_string())
    } else {
        body.court_label.clone()
    };
    tracing::info!(
        "Ergebnis vom Tablet: Feld {} ('{}'), Match {}, Sätze {:?} – schreibe nach BTP",
        body.court_id,
        court_label,
        m.id,
        update.sets
    );
    // Nachschub-Eintrag löschen und Schreibzeit vermerken erledigt
    // `write_result_settled` — hier bleibt nur, was diesen Weg ausmacht.
    match write_result_settled(&ctx.config, &ctx.tablet, &update).await {
        Ok(()) => {
            ctx.tablet.clear_court(body.court_id);
            tracing::info!("BTP-Schreiben OK: Match {} (Feld freigegeben)", m.id);
            // Nach einer Aufgabe NUR dann einen Walkover-Vorschlag für die
            // restlichen Spiele der Disziplin hinterlegen, wenn das Tablet das
            // ausdrücklich gewählt hat (echte Verletzung → `cascade_walkover`).
            // Ohne das Flag zählt nur dieses eine Spiel als Aufgabe. Bei einem
            // echten Kampflos (score_status=1) ebenfalls nicht – das ist bereits
            // die finale Wertung dieses Spiels.
            if body.retired && body.cascade_walkover {
                register_walkover_proposal(ctx, &m, team1_won);
            }
            // Punktverlauf abschließen (AK-13): keine weiteren Rallies,
            // Aufgabe gekennzeichnet. Ohne aufgezeichneten Verlauf
            // (Papier, Walkover) entsteht bewusst kein Eintrag.
            ctx.tablet.timeline_store().finalize(m.id, body.retired);
            ResultResponse::ok()
        }
        Err(e) => {
            // Nachschub-Queue (A5): Das Tablet wiederholt zwar selbst, aber
            // wenn es aufgibt/offline geht, wäre das Ergebnis verloren. Der
            // Sync-Loop schiebt den Write nach, sobald BTP wieder antwortet.
            // Bezugszeitpunkt = Spielende (steuert das 5-Minuten-Fenster
            // des Spieler-Checkouts beim Nachschub).
            ctx.tablet.queue_btp_retry(update.clone(), end_ms);
            tracing::warn!(
                "BTP-Schreiben fehlgeschlagen (Match {}): {e} — in Nachschub-Queue eingereiht",
                m.id
            );
            ResultResponse::err(e)
        }
    }
}

/// Hinterlegt nach einer Aufgabe einen Walkover-Vorschlag für die
/// restlichen Spiele der aufgebenden Mannschaft – aber nur, wenn es in
/// derselben Disziplin überhaupt noch wertbare Spiele gibt.
fn register_walkover_proposal(ctx: &ServerCtx, m: &BtpMatch, team1_won: bool) {
    // Die aufgebende Mannschaft ist der Verlierer der Begegnung.
    let (entry_id, retired_players) = if team1_won {
        (m.entry2_id, &m.team2)
    } else {
        (m.entry1_id, &m.team1)
    };
    if entry_id == 0 {
        return; // Mannschaft nicht eindeutig auflösbar
    }
    if ctx.tablet.walkover_candidates(entry_id).is_empty() {
        return; // keine weiteren Spiele – kein Vorschlag nötig
    }
    let retired_team = retired_players
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(" / ");
    tracing::info!(
        "Aufgabe Entry {entry_id} ({retired_team}, {}) – Walkover-Vorschlag hinterlegt",
        m.draw_name
    );
    ctx.tablet
        .add_walkover_proposal(crate::tablet::state::WalkoverProposal {
            id: entry_id.to_string(),
            entry_id,
            retired_team,
            draw_name: m.draw_name.clone(),
            created_at_ms: now_ms(),
        });
}

/// Aktuelle Unix-Zeit in Millisekunden.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// LOGIN → Session-Schlüssel → `SENDUPDATE`. Schreibt ein einzelnes
/// Match-Ergebnis nach BTP – auch für kampflose Wertungen (Walkover)
/// aus der Turnierleitung wiederverwendet.
pub(crate) async fn write_result_to_btp(
    config: &AppConfig,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw = client::send_request(host, port, &proto::update_request(update, &session, pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Schreibt eine Wertung nach BTP und führt die Buchführung, die **jedes
/// Mal** dieselbe ist: den Nachschub-Eintrag dieses Spiels löschen und den
/// Schreibvorgang für die Selbstheilung vermerken.
///
/// Der Zeitstempel wird bewusst **nach** dem Schreiben genommen: Der
/// Nachschub-Lauf vergleicht ihn gegen seinen eigenen Startzeitpunkt, um zu
/// erkennen, ob ihn eine neuere Wertung überholt hat. Ein vor dem Schreiben
/// abgelesener Wert (BTP-Anmeldung + Aktualisierung dauern) kann älter sein
/// als dieser Start — dann bliebe die frische Wertung überschrieben.
/// Genau diese Falle ist der Grund, warum es die Funktion gibt und der
/// Ablauf nicht mehr an vier Stellen abgeschrieben steht.
pub(crate) async fn write_result_settled(
    config: &AppConfig,
    tablet: &TabletState,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    write_result_to_btp(config, update).await?;
    tablet.clear_btp_retry(update.btp_match_id);
    tablet.note_direct_btp_write(update.clone(), now_ms());
    Ok(())
}

/// LOGIN → Session-Schlüssel → `SENDUPDATE` mit Courts-Block. Schreibt
/// **Feld-Zuweisungen** nach BTP (Match auf Feld setzen / Feld freigeben) –
/// nach dem Vorbild des Original-BTS. Bidirektional: das, was hier geschrieben
/// wird, liest bts-light beim nächsten Poll als OnCourt zurück.
pub(crate) async fn write_courts_to_btp(
    config: &AppConfig,
    courts: &[proto::CourtAssignment],
    match_courts: &[proto::MatchCourt],
) -> Result<(), String> {
    if courts.is_empty() && match_courts.is_empty() {
        return Ok(());
    }
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw = client::send_request(
        host,
        port,
        &proto::court_assign_request(courts, match_courts, &session, pw),
    )
    .await
    .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Schreibt **nur** die Schiedsrichter-Besetzung nach BTP (ADR 0021),
/// Muster [`write_courts_to_btp`]: eigene Sitzung, ein `SENDUPDATE`, Antwort
/// geprüft. Der Aufrufer übernimmt den Stand erst bei `Ok` — ein Fehlschlag
/// wird im nächsten Sync-Zyklus wiederholt.
///
/// Läuft über [`proto::court_assign_request`] mit leerem `courts`-Block —
/// jeder Eintrag trägt seine aktuelle `CourtID` mit (siehe
/// [`proto::MatchCourt::court_id`]), damit dieser eigenständige Write nie
/// eine gerade erst angekommene Feldzuweisung überschreiben kann.
pub(crate) async fn write_officials_to_btp(
    config: &AppConfig,
    entries: &[proto::MatchCourt],
) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw = client::send_request(
        host,
        port,
        &proto::court_assign_request(&[], entries, &session, pw),
    )
    .await
    .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Schreibt `Match.Highlight`-Flags nach BTP (P1): macht „in Vorbereitung"-
/// Aufrufe im BTP-Planer sichtbar. Eigene Sitzung, ein `SENDUPDATE`
/// (`proto::highlight_request`, Match-Knoten nur mit Identität +
/// Highlight, kein `Status`/Ergebnis). Best-effort: Aufrufer
/// (Aufruf/Rücknahme) fangen den Fehler ab — der interne Aufruf-Zustand
/// bleibt davon unberührt.
pub(crate) async fn write_highlight_to_btp(
    config: &AppConfig,
    entries: &[proto::HighlightEntry],
) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw =
        client::send_request(host, port, &proto::highlight_request(entries, &session, pw))
            .await
            .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

// ─────────────────────────────── WebSocket ────────────────────────────────

/// A2 / ADR 0017 (Reconnect-Wahrheit): Ist der Server im OWNERSHIP-Modus? Dann
/// berechnet er die Autorität (Slot-Halter gewinnt) und das Tablet folgt
/// `authoritative`. Im Legacy-Modus (`reconnect_legacy_rev = true`) liefert er
/// `ownership_active = false`, worauf das Tablet die bestehende rev-Logik nutzt
/// — das hält den Laufzeit-Rollback sauber (das Tablet befolgt `authoritative`
/// eben NICHT blind, siehe ADR 0017). Reine Funktion → unit-testbar.
fn ownership_active(config: &AppConfig) -> bool {
    !config.reconnect_legacy_rev
}

/// Baut die Match-Kurzinfo fürs Tablet. BTP liefert das Spielsystem nicht
/// zuverlässig – Standard ist Best-of-3 bis 21 (Badminton-Normalfall).
pub(crate) fn match_brief(
    m: &BtpMatch,
    scorekeeper: Vec<String>,
    scorekeeper_assigned: bool,
    display: &crate::config::DisplayConfig,
    finalized: bool,
    officials: (Vec<String>, Vec<String>),
) -> MatchBrief {
    let team = |players: &[crate::btp::model::BtpPlayer], base: i64| {
        players
            .iter()
            .enumerate()
            .map(|(i, p)| PlayerBrief {
                id: base + i as i64,
                name: p.name.clone(),
                nationality: p.nationality.clone(),
                club: p.club.clone(),
            })
            .collect()
    };
    MatchBrief {
        match_id: m.id,
        team_a: team(&m.team1, 1),
        team_b: team(&m.team2, 11),
        event_label: format!("{} {}", m.draw_name, m.round_name)
            .trim()
            .to_string(),
        best_of_sets: m.scoring.best_of,
        target_score: m.scoring.target_score,
        cap_score: m.scoring.cap_score,
        interval_at: m.scoring.interval_at,
        discipline: m.discipline.as_str().to_string(),
        class_label: m.class_label.clone(),
        match_number: m.match_num,
        scorekeeper,
        scorekeeper_assigned,
        show_club_names: display.show_club_names,
        show_club_logos: display.show_club_logos,
        finalized,
        sr_names: officials.0,
        ar_names: officials.1,
    }
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(ctx): State<Arc<ServerCtx>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, ctx))
}

/// Query der Monitor-Nudge-WS: optionale CourtID. Fehlt sie, abonniert der
/// Client Nudges ALLER Felder (Feld-Übersicht `overview.html`); ist sie
/// gesetzt, nur die dieses Felds (Court-Monitor `monitor.html`).
#[derive(serde::Deserialize)]
struct MonitorWsQuery {
    court: Option<i64>,
}

/// Upgrade der Court-Monitor-Nudge-WS (A1, ADR 0016). Kein `identify`-
/// Handshake nötig: eine Anzeige liest nur, der Court steht im Query.
async fn monitor_ws_upgrade(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<ServerCtx>>,
    Query(q): Query<MonitorWsQuery>,
) -> impl IntoResponse {
    let court = q.court;
    ws.on_upgrade(move |socket| monitor_socket(socket, ctx, court))
}

/// Eine Court-Monitor-Nudge-Verbindung (A1, ADR 0016). Der Server schickt
/// hier nur winzige „Feld X geändert, seq N"-Signale; die Anzeige holt den
/// Vollstand daraufhin über ihre bestehende Poll-Route. So bleibt der
/// Poll-Endpunkt die **einzige** Datenquelle (ein Renderpfad, kein Flackern).
///
/// TODO(A1): Match-Zuweisung wird noch nicht angestoßen — sie ist
/// BTP-Snapshot-getrieben (Sync-Loop), nicht ein einzelner State-Aufruf wie
/// Score/Alert/Räumung. Bis dahin deckt der ~250-ms-Poll-Fallback die
/// Zuweisungs-Latenz ab (Score ist das Muss, Spec Paket A).
async fn monitor_socket(mut socket: WebSocket, ctx: Arc<ServerCtx>, court: Option<i64>) {
    // Kanal hier anlegen und das Sende-Ende dem State reichen (wie im Relay),
    // damit wir es am Verbindungsende per `unsubscribe_monitor` gezielt wieder
    // austragen können.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    // Fan-out-Deckel (A1, ADR 0016): über `MAX_MONITOR_SUBS` lehnt der State
    // das Abo ab — die Verbindung sauber schließen, die Anzeige bedient sich
    // dann aus ihrem 250-ms-Poll-Fallback (kein stiller Hänger, kein DoS).
    if !ctx.tablet.subscribe_monitor(court, &tx) {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    // Herzschlag: hält die Leitung wach und erkennt tote Sockets (analog zum
    // Tablet-WS-Ping). Fällt die Verbindung weg, endet die Schleife und wir
    // tragen das Abo unten explizit wieder aus.
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            nudge = rx.recv() => {
                match nudge {
                    Some(json) => {
                        if socket.send(Message::Text(Utf8Bytes::from(json))).await.is_err() {
                            break;
                        }
                    }
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
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
    // Verbindungsende: Abo explizit austragen (nicht nur lazy beim nächsten
    // Nudge) — sonst hielte ein stiller Court tote Sender beliebig lange.
    ctx.tablet.unsubscribe_monitor(court, &tx);
}

/// TL-Push-Kanal (Spec tl-web-push): reine Anstöße `{"rev":n}` an die
/// Turnierleitungs-Seite — die Daten holt sie über `/tl/api/state`
/// (eine Wahrheit für Auth/ETag/Profile-Header, Muster ADR 0016).
async fn tl_ws_upgrade(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<ServerCtx>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| tl_ws_socket(socket, ctx))
}

/// Erste Nachricht des Clients auf `/tl-ws`: der Zugang.
#[derive(serde::Deserialize)]
struct TlWsAuth {
    #[serde(default)]
    token: String,
}

async fn tl_ws_socket(mut socket: WebSocket, ctx: Arc<ServerCtx>) {
    // In-Band-Auth: Die erste Nachricht muss binnen 10 s `{"token":…}`
    // sein. Vor erfolgreicher Prüfung sendet der Server NICHTS — auch
    // keinen Ablehnungsgrund. Warum in-band: Browser-WebSockets können
    // keine Header setzen, und der Zugang gehört nie in eine URL (Pfade
    // landen in Zugriffsprotokollen — dieselbe Regel wie bei den
    // HTTP-Routen, siehe `tl_device`).
    let erste = tokio::time::timeout(Duration::from_secs(10), socket.recv()).await;
    let token = match erste {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<TlWsAuth>(&text)
            .map(|a| a.token)
            .unwrap_or_default(),
        _ => String::new(),
    };
    if token.is_empty() || crate::tablet::tl::authorize(&ctx.app_config_arc(), &token).is_none() {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    // Fan-out-Deckel wie beim Monitor-Nudge: Über der Grenze sauber
    // schließen, die Seite bedient sich aus ihrem Poll-Fallback.
    if !ctx.tablet.subscribe_tl(&tx) {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    // Den aktuellen Stand sofort melden: Eine Revision, die während des
    // Verbindens durchrutschte, läge sonst bis zum 30-s-Fallback-Poll.
    if let Some(cache) = ctx.tablet.tl_state_cache() {
        let _ = tx.send(format!("{{\"rev\":{}}}", cache.rev));
    }
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            nudge = rx.recv() => {
                match nudge {
                    Some(json) => {
                        if socket.send(Message::Text(Utf8Bytes::from(json))).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            incoming = socket.recv() => {
                // Nach der Auth sendet die Seite nichts Fachliches; wir
                // lesen nur, um Close/Fehler zu bemerken.
                match incoming {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
    ctx.tablet.unsubscribe_tl(&tx);
}

/// Sendet eine `ServerMsg` über den Tablet-Socket.
async fn send_msg(socket: &mut WebSocket, msg: &ServerMsg) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = socket.send(Message::Text(Utf8Bytes::from(json))).await;
    }
}

/// Eine Tablet-Verbindung: empfängt identify/score_update/alert, pusht alle
/// 2 s das aktuell von BTP zugewiesene Match. Pro Court schiedst genau ein
/// Tablet aktiv – ein zweites Gerät kann den Court übernehmen.
async fn handle_socket(mut socket: WebSocket, ctx: Arc<ServerCtx>) {
    // Feld-Identität dieser Verbindung: die CourtID, sobald sich das Tablet
    // per `identify` gebunden hat.
    let mut court: Option<i64> = None;
    // Zuletzt ans Tablet gemeldete (Match-ID, finalized). Sentinel
    // `Some((i64::MIN, false))` = „in dieser Verbindung noch nichts gesendet",
    // damit der ERSTE push_match immer feuert – auch ein `MatchCleared`, wenn
    // das Feld leer ist. Sonst behielte ein nach Inaktivität neu verbundenes
    // Tablet sein altes (längst entferntes) Match, weil `None == None` (kein
    // Match) den Dedup auslöste. `finalized` im Schlüssel, weil der Übergang
    // OnCourt→Finished die matchId nicht ändert, das Tablet aber erreichen muss.
    let mut last_match: Option<(i64, bool, String)> = Some((i64::MIN, false, String::new()));
    // Token der Court-Übernahme: `Some`, wenn dieses Tablet aktiv schiedst.
    let mut my_token: Option<u64> = None;
    let mut superseded = false;
    // Persistente Geräte-Kennung des Tablets (aus identify/take_over) —
    // leer bei alten Tablet-Seiten. Für die Reconnect-Erkennung.
    let mut my_device = String::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Zeitpunkt der letzten empfangenen Nachricht (jede Art, inkl. App-Ping
    // und Protokoll-Pong). Bricht der Router weg, liefert der Browser oft
    // KEIN Close – die TCP-Verbindung bleibt serverseitig minutenlang als
    // „offen" hängen und hält das Feld belegt, sodass das zurückkehrende
    // Tablet beim Reconnect „belegt" zu hören bekommt. Erkennt der Server
    // nach STALE_AFTER kein Lebenszeichen mehr, schließt er die Verbindung
    // selbst → das Feld wird frei und kann sofort neu belegt werden.
    let mut last_seen = std::time::Instant::now();
    // 10 s, BEWUSST KÜRZER als der Tablet-Watchdog (15 s): Bricht der Router
    // weg, gibt der Server das Feld schon nach 10 s frei – also bevor das
    // Tablet (frühestens nach 15 s Stille) sich neu meldet. So ist das Feld
    // beim Reconnect bereits frei, das „Feld belegt"-Overlay erscheint gar
    // nicht erst und das Tablet belegt direkt selbst neu. Auf einer gesunden
    // Verbindung trifft der Browser den Protokoll-Ping alle ~2 s mit Pong →
    // last_seen bleibt frisch, 10 s lösen also keinen Fehlschluss aus. Ein
    // seltener Fehlschluss unter Last wäre harmlos: das Tablet verbindet sich
    // sofort neu (Stand ist persistiert und wird re-gepusht).
    const STALE_AFTER: Duration = Duration::from_secs(10);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                last_seen = std::time::Instant::now();
                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<TabletMsg>(text.as_str()) {
                            Ok(TabletMsg::Identify { court_id, device_id, .. }) => {
                                court = Some(court_id);
                                last_match = None;
                                my_device = device_id;
                                // Reconnect-Erkennung: Hält DIESES Gerät das Feld
                                // bereits (tote Vorgänger-Session nach Netz-Abriss),
                                // darf es nahtlos neu claimen — kein „Feld belegt"-
                                // Overlay für das eigene Gerät. Fremde Geräte sehen
                                // weiterhin den Übernehmen-Dialog.
                                let occupied = ctx.tablet.court_occupied(court_id)
                                    && !ctx.tablet.court_held_by_device(court_id, &my_device);
                                if occupied {
                                    tracing::info!("Feld {court_id} belegt – Tablet wartet auf Übernahme");
                                    send_msg(&mut socket, &ServerMsg::CourtOccupied).await;
                                } else {
                                    // A2 / ADR 0017: Den Slot-Halter VOR dem
                                    // Reclaim festhalten — nach `claim_court` sind
                                    // wir selbst der Halter, die Konfliktregel
                                    // braucht aber den Zustand von vorher.
                                    let owner_before = ctx.tablet.court_owner(court_id);
                                    let token = ctx.tablet.claim_court(court_id, &my_device);
                                    my_token = Some(token);
                                    ctx.tablet.attach_tablet(court_id);
                                    tracing::info!("Tablet verbunden für Feld {court_id}");
                                    // Gespeicherten Spielstand auch beim normalen
                                    // Verbinden wiederherstellen (nicht nur bei
                                    // Übernahme): so startet ein neu verbundenes
                                    // ODER Ersatz-Tablet nach einem Crash nicht bei
                                    // 0:0. Das Tablet behält den Stand nur, wenn die
                                    // matchId zum gleich gepushten Match passt
                                    // (tablet.html), sonst überschreibt push_match.
                                    if let Some(state) = ctx.tablet.court_state(court_id) {
                                        // A2 / ADR 0017: Autorität bestimmen. Der
                                        // Legacy-Schalter wird bei JEDER Entscheidung
                                        // frisch aus der config.json gelesen
                                        // (Laufzeit-Rollback). Er steuert AUSSCHLIESSLICH
                                        // `ownership_active`: ist er gesetzt, meldet der
                                        // Server `ownership_active=false` und das Tablet
                                        // nutzt weiter die rev-Logik (statt `authoritative`
                                        // blind zu befolgen — sonst öffnete „Legacy" den
                                        // Reconnect-Bug wieder). `authoritative` selbst ist
                                        // NUR noch die reine Konfliktentscheidung.
                                        let cfg = ctx.app_config();
                                        let ownership = ownership_active(&cfg);
                                        // A2 / ADR 0017, Regel b: Ist das Match des
                                        // Felds in BTP finalisiert (per Hand fertig),
                                        // tritt der Rückkehrer zurück → StandDown
                                        // (überbügelt das Hand-Ergebnis nicht). Der
                                        // Merker wird vom Sync-Loop aus dem BTP-Status
                                        // gesetzt (`recently_finalized`).
                                        let finalized =
                                            ctx.tablet.recently_finalized(court_id).is_some();
                                        let authoritative = matches!(
                                            reconnect_decision(&my_device, owner_before, finalized),
                                            ReconnectDecision::KeepLocal
                                        );
                                        send_msg(
                                            &mut socket,
                                            &ServerMsg::StateRestore {
                                                state,
                                                ownership_active: ownership,
                                                authoritative,
                                                owner_epoch: token,
                                                owner_device: my_device.clone(),
                                            },
                                        )
                                        .await;
                                    }
                                    push_match(court_id, &ctx, &mut socket, &mut last_match).await;
                                }
                            }
                            Ok(TabletMsg::TakeOver { device_id }) => {
                                if let (Some(c), None, false) = (court, my_token, superseded) {
                                    if !device_id.is_empty() {
                                        my_device = device_id;
                                    }
                                    let token = ctx.tablet.claim_court(c, &my_device);
                                    my_token = Some(token);
                                    ctx.tablet.attach_tablet(c);
                                    last_match = None;
                                    tracing::info!("Tablet übernimmt Feld {c}");
                                    if let Some(state) = ctx.tablet.court_state(c) {
                                        // A2 / ADR 0017: Eine BEWUSSTE Übernahme
                                        // adoptiert den laufenden Stand des Felds —
                                        // das übernehmende Gerät hat keine eigene
                                        // „lokale Wahrheit", die es verteidigen
                                        // müsste. Daher authoritative=false
                                        // (adoptieren). Im Legacy-Modus meldet
                                        // `ownership_active=false`, dann ignoriert das
                                        // Tablet `authoritative` und entscheidet per
                                        // rev — Laufzeit-Rollback.
                                        let cfg = ctx.app_config();
                                        send_msg(
                                            &mut socket,
                                            &ServerMsg::StateRestore {
                                                state,
                                                ownership_active: ownership_active(&cfg),
                                                authoritative: false,
                                                owner_epoch: token,
                                                owner_device: my_device.clone(),
                                            },
                                        )
                                        .await;
                                    }
                                    push_match(c, &ctx, &mut socket, &mut last_match).await;
                                }
                            }
                            // Score/Alert/StateSync nur vom AKTUELLEN Halter des
                            // Felds annehmen (is_court_active), nicht von jeder
                            // Session mit irgendeinem Token: Nach einem Reconnect-
                            // Reclaim lebt die abgelöste Session evtl. noch kurz
                            // weiter (Ticker erkennt das erst nach bis zu 2 s) —
                            // ihre nachlaufenden Frames würden sonst den Cache/
                            // Liveticker wieder mit dem ALTEN Stand füllen.
                            Ok(TabletMsg::ScoreUpdate { score_a, score_b, sets_history, match_id }) => {
                                if let (Some(c), Some(t)) = (court, my_token) {
                                    if ctx.tablet.is_court_active(c, t) {
                                        handle_score(c, score_a, score_b, &sets_history, match_id, &ctx).await;
                                    }
                                }
                            }
                            Ok(TabletMsg::Battery { percent, charging }) => {
                                if let Some(c) = court {
                                    ctx.tablet.record_battery(c, percent, charging);
                                }
                            }
                            Ok(TabletMsg::Alert { injury, official }) => {
                                if let (Some(c), Some(t)) = (court, my_token) {
                                    if ctx.tablet.is_court_active(c, t) {
                                        ctx.tablet.record_alert(c, injury, official);
                                    }
                                }
                            }
                            Ok(TabletMsg::StateSync { state }) => {
                                if let (Some(c), Some(t)) = (court, my_token) {
                                    if ctx.tablet.is_court_active(c, t) {
                                        // Stale-Filter (A4): State eines ALTEN Matches
                                        // (Tablet hing nach Doze/Reconnect noch im
                                        // vorigen Spiel) nicht in den Cache übernehmen.
                                        let stale = relay_proto::state_sync_match_id(&state)
                                            .zip(ctx.tablet.match_for_court(c))
                                            .is_some_and(|(sm, m)| sm != m.id);
                                        if stale {
                                            tracing::info!(
                                                "State von Feld {c} verworfen: Tablet-State \
                                                 trägt ein anderes Match als das Feld"
                                            );
                                        } else {
                                            ctx.tablet.set_court_state(c, state);
                                        }
                                    }
                                }
                            }
                            Ok(TabletMsg::Ping) => {
                                // Lebenszeichen → sofort Pong zurück, damit das
                                // Tablet eine tote Verbindung erkennen kann.
                                send_msg(&mut socket, &ServerMsg::Pong).await;
                            }
                            // Punktverlauf (ADR 0014): nur vom aktiven Halter
                            // und nur fürs aktuelle Court-Match (HM-03-Filter,
                            // AK-3/AK-11) — sonst könnte ein nach Doze im alten
                            // Spiel hängendes Tablet fremde Verläufe beschreiben.
                            Ok(TabletMsg::Rally { match_id, set, n, winner, score_a, score_b }) => {
                                if let (Some(c), Some(t)) = (court, my_token) {
                                    if ctx.tablet.is_court_active(c, t)
                                        && ctx.tablet.match_for_court(c).is_some_and(|m| m.id == match_id)
                                    {
                                        ctx.tablet.timeline_store().apply_rally(
                                            match_id, set, n, &winner, score_a, score_b,
                                        );
                                    }
                                }
                            }
                            Ok(TabletMsg::RallySync { match_id, timeline }) => {
                                if let (Some(c), Some(t)) = (court, my_token) {
                                    if ctx.tablet.is_court_active(c, t)
                                        && ctx.tablet.match_for_court(c).is_some_and(|m| m.id == match_id)
                                    {
                                        ctx.tablet.timeline_store().apply_sync(match_id, timeline);
                                    }
                                }
                            }
                            Err(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                // Tote Verbindung (Router weg, kein Close vom Browser) erkennen
                // und schließen, damit das Feld nicht dauerhaft belegt bleibt.
                if last_seen.elapsed() > STALE_AFTER {
                    tracing::info!(
                        "Tablet-Verbindung still seit >{}s – schließe (Feld {court:?})",
                        STALE_AFTER.as_secs()
                    );
                    break;
                }
                // Protokoll-Ping: hält die Leitung wach und lässt auch ältere
                // Tablets (ohne App-Ping) durch ihr Pong als „lebend" gelten;
                // schlägt das Senden fehl, ist die Verbindung tot → Schluss.
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
                if let (Some(c), Some(token)) = (court, my_token) {
                    if ctx.tablet.is_court_active(c, token) {
                        push_match(c, &ctx, &mut socket, &mut last_match).await;
                    } else {
                        my_token = None;
                        superseded = true;
                        tracing::info!("Tablet für Feld {c} wurde abgelöst");
                        send_msg(&mut socket, &ServerMsg::SessionSuperseded).await;
                    }
                }
            }
        }
    }

    // Aufräumen: nur das noch aktive Tablet gibt das Feld frei.
    if let (Some(c), Some(token)) = (court, my_token) {
        if ctx.tablet.is_court_active(c, token) {
            ctx.tablet.detach_tablet(c);
            ctx.tablet.release_court(c, token);
            tracing::info!("Tablet getrennt für Feld {c}");
        }
    }
}

/// Fingerabdruck der Besetzung für den Push-Schlüssel: Ändert sich
/// Schiedsrichter oder Aufschlagrichter, ändert sich der Schlüssel — nur so
/// erreicht eine Zuweisung mitten im Spiel das Tablet.
pub(crate) fn officials_key(officials: &(Vec<String>, Vec<String>)) -> String {
    format!("{}|{}", officials.0.join("/"), officials.1.join("/"))
}

/// Sendet `match_assigned`/`match_cleared`, sobald sich das Match des
/// Felds (per CourtID) gegenüber dem zuletzt gemeldeten Stand geändert hat.
async fn push_match(
    court_id: i64,
    ctx: &ServerCtx,
    socket: &mut WebSocket,
    last: &mut Option<(i64, bool, String)>,
) {
    // A2 / ADR 0017, Regel b: Ein gerade in BTP finalisiertes Match liefert
    // `match_for_court` nicht mehr (Status Finished ≠ OnCourt), das Tablet trägt
    // aber noch dessen matchId. Solange der kurzlebige Merker steht, schicken
    // wir das finalisierte Match mit `finalized:true` weiter (Rücktritt statt
    // bloßer `MatchCleared`), damit das Tablet das Hand-Ergebnis nicht
    // überbügelt. R2 gewahrt: die Wahrheit bleibt BTP, wir spiegeln nur den
    // Finished-Status.
    let current = ctx.tablet.match_for_court(court_id);
    let (effective, finalized) = match current {
        Some(m) => (Some(m), false),
        None => match ctx.tablet.recently_finalized(court_id) {
            Some(mid) => (ctx.tablet.snapshot_match(mid), true),
            None => (None, false),
        },
    };
    // `finalized` nur echt, wenn wir das Match auch nachreichen können.
    let finalized = finalized && effective.is_some();
    // Zustands-Schlüssel inkl. `finalized` UND Besetzung: Der Übergang
    // OnCourt→Finished ändert die matchId nicht, muss das Tablet aber
    // erreichen — und ebenso wenig ändert sie sich, wenn die Turnierleitung
    // mitten im Spiel einen Schiedsrichter einteilt. Ohne die Besetzung im
    // Schlüssel bliebe das Tablet beim Stand der Zuweisung stehen.
    let key = effective.as_ref().map(|m| {
        (
            m.id,
            finalized,
            officials_key(&ctx.tablet.match_officials(m)),
        )
    });
    if key == *last {
        return;
    }
    *last = key;
    let msg = match &effective {
        Some(m) => {
            tracing::info!(
                "Feld {court_id}: Match {} ans Tablet zugewiesen (finalized={finalized})",
                m.id
            );
            ServerMsg::MatchAssigned {
                match_brief: {
                    let (sk, ska) = ctx.tablet.scorekeeper_display(court_id);
                    match_brief(
                        m,
                        sk,
                        ska,
                        &ctx.app_config_arc().display,
                        finalized,
                        ctx.tablet.match_officials(m),
                    )
                },
            }
        }
        None => {
            tracing::info!("Feld {court_id}: Match-Zuweisung aufgehoben");
            ServerMsg::MatchCleared
        }
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(Utf8Bytes::from(json))).await;
    }
}

/// Entscheiden die bereits abgeschlossenen Sätze das Match schon? (Ein Team hat
/// die Mehrheit der Best-of-N-Sätze gewonnen.) Damit unterscheiden wir einen
/// 0:0-„Geistersatz" NACH Spielende von einem echten neuen Satz zwischen zwei
/// Sätzen – ohne dafür ein separates `finished`-Signal zu brauchen (das im
/// Cloud-Pfad nicht vorliegt). Funktioniert in LAN- und Cloud-Modus identisch.
fn match_decided(best_of: i64, completed: &[(i64, i64)]) -> bool {
    let need = best_of / 2 + 1;
    let (mut a, mut b) = (0, 0);
    for &(sa, sb) in completed {
        if sa > sb {
            a += 1;
        } else if sb > sa {
            b += 1;
        }
    }
    a >= need || b >= need
}

/// Verarbeitet einen Live-Punktestand vom Tablet: merken + an den
/// Liveticker pushen. Von LAN-Server und Cloud-Relay-Client genutzt.
pub(crate) async fn handle_score(
    court_id: i64,
    score_a: i64,
    score_b: i64,
    history: &[SetAb],
    match_id: i64,
    ctx: &ServerCtx,
) {
    // A2 / ADR 0017, Regel b (Finalisiert-Gate): Ein Score fürs bereits in BTP
    // finalisierte Match dieses Felds wird verworfen — das per Hand eingegebene
    // Ergebnis darf nicht überbügelt werden. Steht VOR dem Match-Lookup, weil
    // ein finalisiertes Match nicht mehr OnCourt ist (`match_for_court` läge
    // `None`) und der Merker die matchId trägt. matchId 0 (alte Tablet-Seite)
    // kann nicht zugeordnet werden → läuft wie bisher weiter. Das Gate ERGÄNZT
    // den Stale-Filter; R5 bleibt (`process_result` validiert weiter).
    if match_id != 0 && ctx.tablet.is_match_finalized(court_id, match_id) {
        tracing::info!(
            "Score von Feld {court_id} verworfen: Match {match_id} ist in BTP finalisiert"
        );
        return;
    }
    let Some(m) = ctx.tablet.match_for_court(court_id) else {
        return;
    };
    // Stale-Filter (A4, Turnier-Befund HM-03): Nennt das Tablet ein
    // Match (≠ 0) und das Feld hat inzwischen ein ANDERES, ist der Score
    // ein Nachzügler des alten Spiels — verwerfen statt den frisch
    // geleerten Stand des neuen Spiels zu überschreiben. matchId 0 =
    // alte Tablet-Seite → Verhalten wie bisher.
    if match_id != 0 && m.id != match_id {
        tracing::info!(
            "Score von Feld {court_id} verworfen: Tablet zählt Match {match_id}, \
             Feld hat Match {}",
            m.id
        );
        return;
    }
    if history.len() > 9 {
        return; // unplausibel viele Sätze – Nachricht verwerfen
    }
    // Vollständige Satzliste: abgeschlossene Sätze + laufender Satz.
    // Den laufenden 0:0-Satz NUR dann weglassen, wenn er ein „Geistersatz"
    // NACH Spielende ist – d. h. die abgeschlossenen Sätze entscheiden das
    // Match bereits (das Tablet setzt currentSet beim Match-Ende auf 0:0).
    // ZWISCHEN den Sätzen (Match noch offen) MUSS der 0:0-Satz erhalten
    // bleiben, sonst klebt der Court-Monitor nach der Satzpause am alten
    // Satzstand, bis der erste Punkt fällt. Erster Satz (history leer): bleibt.
    let mut sets: Vec<(i64, i64)> = history.iter().map(|s| (s.a, s.b)).collect();
    let ghost_after_finish =
        score_a == 0 && score_b == 0 && match_decided(m.scoring.best_of, &sets);
    if !ghost_after_finish {
        sets.push((score_a, score_b));
    }
    ctx.tablet.record_score(court_id, m.id, sets.clone());
    // Nettostart (Spec `spielzeiten-prognose`, E2/ADR 0027): der erste beim
    // Host eingehende Stand > 0 stempelt — host-seitig, eine Uhr für LAN,
    // Cloud und Zähltafel. Steht hinter Finalisiert-Gate und Stale-Filter:
    // ein verworfener Score stempelt nicht. Ein Undo zurück auf 0:0 löscht
    // den Stempel bewusst nicht (der erste Punkt ist gefallen).
    // `match_id != 0`: Legacy-Scores alter Tablet-Seiten passieren beide
    // Gates ungeprüft — ein Nachzügler des Vorspiels dürfte sonst dem
    // neuen Feld-Match den Nettostart stempeln (Review 2026-08-16, F2).
    // Solche Seiten liefern dann eben keinen Netto-Messwert.
    if match_id != 0 && sets.iter().any(|&(a, b)| a > 0 || b > 0) {
        ctx.tablet
            .match_times_store()
            .stamp_first_point(m.id, now_ms());
    }

    let mut live = m;
    live.sets = sets;
    let update = Update::Single(build_tupdate(&live, ctx.next_rid()));
    // Einstellen statt warten: Der Push läuft hinter der Verbindung, je
    // Feld serialisiert und gebündelt (siehe `ScorePushQueue`). Vorher
    // hing hier die Tablet-WebSocket bis zu 15 s an einem lahmen badhub —
    // und wurde dabei vom eigenen Server als tot geschlossen, samt
    // Freigabe des Felds.
    ctx.queue_score_push(court_id, live.id, update);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpPlayer, BtpSnapshot, Discipline, MatchResult, ScoringFormat};
    use crate::btp::wire;
    use crate::btp::xml::{self, Node, Value};
    use crate::config::BtpConfig;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // ───────────────────────── Test-Helfer (BTP-Ergebnis-Pfad) ──────────────

    /// Antwort-Frame im BTP-Format: Action{ID=REPLY, Result=1, [extra]}.
    fn mock_reply(extra: Vec<Node>) -> Vec<u8> {
        let mut c = vec![Node::string("ID", "REPLY"), Node::integer("Result", 1)];
        c.extend(extra);
        wire::encode_message(&xml::encode(&[Node::group("Action", c)]))
    }

    /// Mock-BTP: LOGIN → Session, SENDUPDATE → aufzeichnen + bestätigen.
    /// Liefert Port und den Aufzeichnungs-Puffer der SENDUPDATE-Requests.
    async fn spawn_mock_btp() -> (u16, Arc<Mutex<Vec<Vec<Node>>>>) {
        let recorded: Arc<Mutex<Vec<Vec<Node>>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut header = [0u8; 4];
                if sock.read_exact(&mut header).await.is_err() {
                    continue;
                }
                let len = i32::from_be_bytes(header) as usize;
                let mut payload = vec![0u8; len];
                if sock.read_exact(&mut payload).await.is_err() {
                    continue;
                }
                let mut full = header.to_vec();
                full.extend_from_slice(&payload);
                let nodes = proto::decode_response(&full).unwrap();
                let action = xml::find(&nodes, "Action").unwrap();
                let id = xml::find(action.children(), "ID")
                    .and_then(Node::value)
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if id == "LOGIN" {
                    sock.write_all(&mock_reply(vec![Node::string("Unicode", "SESSION")]))
                        .await
                        .unwrap();
                } else {
                    rec.lock().unwrap().push(nodes.clone());
                    sock.write_all(&mock_reply(vec![])).await.unwrap();
                }
            }
        });
        (port, recorded)
    }

    fn player(n: &str) -> BtpPlayer {
        BtpPlayer {
            // Stabile Pseudo-PlayerID aus dem Namen — für die
            // Players-Block-Assertions (Spielende je Spieler).
            id: n.bytes().map(|b| b as i64).sum::<i64>().max(1),
            name: n.to_string(),
            first: String::new(),
            last: n.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        }
    }

    /// Match id=42 auf Court 101 (OnCourt), zwei Einzel-Spieler.
    fn match_on_court() -> BtpMatch {
        BtpMatch {
            display_order: None,
            from1: None,
            from2: None,
            id: 42,
            draw_id: 7,
            planning_id: 1001,
            draw_name: "HE".into(),
            discipline: Discipline::MensSingles,
            class_label: String::new(),
            round_name: "G1".into(),
            match_num: Some(1),
            planned_time: None,
            team1: vec![player("A")],
            team2: vec![player("B")],
            entry1_id: 0,
            entry2_id: 0,
            court: Some("1".into()),
            court_id: Some(101),
            location_id: None,
            sets: vec![],
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::OnCourt,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            official1_id: None,
            official2_id: None,
            scoring: ScoringFormat::default(),
        }
    }

    #[test]
    fn match_brief_carries_club_and_display_flags() {
        let mut m = match_on_court();
        m.team1 = vec![BtpPlayer {
            club: Some("SC Musterstadt".into()),
            ..player("A")
        }];
        // Flags eingeschaltet → landen im Brief; der Verein reist am Spieler mit.
        let on = crate::config::DisplayConfig {
            show_club_names: true,
            show_club_logos: true,
        };
        let brief = match_brief(&m, Vec::new(), false, &on, false, (Vec::new(), Vec::new()));
        assert!(brief.show_club_names);
        assert!(brief.show_club_logos);
        assert!(!brief.finalized);
        assert_eq!(brief.team_a[0].club.as_deref(), Some("SC Musterstadt"));
        // Standard (aus) → Flags aus, Verein reist trotzdem mit (Tablet blendet
        // ihn dann nur nicht ein).
        let brief_off = match_brief(
            &m,
            Vec::new(),
            false,
            &crate::config::DisplayConfig::default(),
            false,
            (Vec::new(), Vec::new()),
        );
        assert!(!brief_off.show_club_names);
        assert!(!brief_off.show_club_logos);
        assert_eq!(brief_off.team_a[0].club.as_deref(), Some("SC Musterstadt"));
    }

    /// ServerCtx mit Match 42 auf Court 101; BTP zeigt auf 127.0.0.1:`port`.
    /// Für Ablehnungs-Tests genügt ein toter Port (es kommt nie zum Schreiben).
    /// Wie [`make_ctx`], aber mit einer bestimmten Zählweise am Match — für
    /// die Prüfung getippter Ergebnisse gegen Ziel und Deckel.
    fn make_ctx_scoring(port: u16, scoring: ScoringFormat) -> ServerCtx {
        let mut m = match_on_court();
        m.scoring = scoring;
        make_ctx_with(port, m)
    }

    fn make_ctx(port: u16) -> ServerCtx {
        make_ctx_with(port, match_on_court())
    }

    fn make_ctx_with(port: u16, m: BtpMatch) -> ServerCtx {
        let tablet = Arc::new(TabletState::default());
        tablet.set_snapshot(BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches: vec![m],
            courts: vec!["1".into()],
            locations: vec![],
            court_infos: vec![],
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        });
        let config = AppConfig {
            btp: BtpConfig {
                host: "127.0.0.1".into(),
                port,
                password: None,
            },
            // Toter Port statt echtem badhub: Tests, die bis zum
            // Liveticker-Push laufen (handle_score), bleiben hermetisch.
            badhub: crate::config::BadhubConfig {
                url: "http://127.0.0.1:1/".into(),
                password: String::new(),
                live_url: String::new(),
            },
            ..Default::default()
        };
        let tmp = std::env::temp_dir();
        let shared_config = Arc::new(std::sync::Mutex::new(config.clone()));
        ServerCtx::new(
            tablet,
            config,
            reqwest::Client::new(),
            tmp.clone(),
            tmp.join("bts_test_config.json"),
            tmp.join("bts_test_assign.json"),
            tmp,
            shared_config,
        )
    }

    #[test]
    fn monitor_state_for_multi_hall_court_carries_its_hall_color() {
        // Spec hallen-farben: Der LAN-Monitor bekommt die Farbe seines
        // Felds über die kanonische Hallenliste — alphabetisch bekommt
        // „Halle A" Ton 0, „Halle B" (mit Feld 101) Ton 1.
        let ctx = make_ctx(1);
        ctx.tablet.set_snapshot(BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches: Vec::new(),
            courts: vec!["1".into()],
            locations: vec![
                crate::btp::model::BtpLocation {
                    id: 1,
                    name: "Halle A".into(),
                },
                crate::btp::model::BtpLocation {
                    id: 2,
                    name: "Halle B".into(),
                },
            ],
            court_infos: vec![crate::btp::model::BtpCourt {
                id: 101,
                name: "1".into(),
                location_id: Some(2),
                sort_order: 1,
            }],
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        });
        let cfg = ctx.app_config();
        assert_eq!(
            hall_color_for(&ctx, &cfg, 101).as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[1])
        );
        assert_eq!(hall_color_for(&ctx, &cfg, 999), None, "unbekanntes Feld");
    }

    /// Standard-Ergebnis-Body (Match 42 / Court 101) mit gegebenen Sätzen.
    fn body_with(sets: &[(i64, i64)]) -> ResultBody {
        ResultBody {
            match_id: 42,
            court_id: 101,
            court_label: "1".into(),
            sets: sets.iter().map(|&(a, b)| SetAb { a, b }).collect(),
            retired: false,
            walkover: false,
            winner: None,
            cascade_walkover: false,
        }
    }

    /// Kinder des Match-Knotens aus einem aufgezeichneten SENDUPDATE-Request.
    fn match_fields(req: &[Node]) -> Vec<Node> {
        let upd = xml::find(req, "Update").unwrap();
        let tour = xml::find(upd.children(), "Tournament").unwrap();
        let matches = xml::find(tour.children(), "Matches").unwrap();
        xml::find(matches.children(), "Match")
            .unwrap()
            .children()
            .to_vec()
    }

    fn int(children: &[Node], id: &str) -> Option<i64> {
        xml::find(children, id)
            .and_then(Node::value)
            .and_then(Value::as_int)
    }

    // Die Logik hinter dem 0:0-Geistersatz-Fix: Zwischen den Sätzen ist das
    // Match NICHT entschieden → der laufende 0:0-Satz bleibt erhalten (Monitor
    // zeigt sofort 0:0). Erst wenn die Mehrheit der Sätze gewonnen ist, gilt
    // ─────────────── Geteilte Ergebnis-Ableitung (derive_result) ───────────────

    #[test]
    fn derive_result_regular_winner_from_sets() {
        // 2:0 → Team 1 gewinnt, Status 0, Sätze unverändert.
        let (sets, t1, status) =
            derive_result(vec![(21, 10), (21, 15)], false, false, false, None).unwrap();
        assert_eq!(sets, vec![(21, 10), (21, 15)]);
        assert!(t1);
        assert_eq!(status, 0);
        // Team 2 gewinnt 1:2.
        let (_, t1b, _) = derive_result(
            vec![(21, 10), (15, 21), (18, 21)],
            false,
            false,
            false,
            None,
        )
        .unwrap();
        assert!(!t1b);
    }

    #[test]
    fn derive_result_rejects_drawn_and_empty_and_oversized() {
        assert!(derive_result(vec![(21, 10), (15, 21)], false, false, false, None).is_err()); // 1:1
        assert!(derive_result(vec![], false, false, false, None).is_err()); // kein Satz
        assert!(derive_result(vec![(1, 2); 10], false, false, false, None).is_err()); // >9 Sätze
        assert!(derive_result(vec![(100, 0)], false, false, false, None).is_err());
        // außer 0..=99
    }

    #[test]
    fn derive_result_walkover_clears_sets_and_needs_winner() {
        let (sets, t1, status) = derive_result(vec![(21, 0)], true, false, false, Some(2)).unwrap();
        assert!(sets.is_empty(), "Kampflos verwirft die Sätze");
        assert!(!t1);
        assert_eq!(status, 1);
        assert!(derive_result(vec![], true, false, false, None).is_err()); // ohne Sieger
    }

    #[test]
    fn set_is_complete_enforces_target_margin_and_cap() {
        // 21er-Format, Deckel 30.
        assert!(set_is_complete(21, 10, 21, 30)); // klar durch
        assert!(set_is_complete(21, 19, 21, 30)); // 2 Punkte Vorsprung
        assert!(!set_is_complete(21, 20, 21, 30)); // nur 1 Vorsprung, nicht am Deckel
        assert!(set_is_complete(30, 29, 21, 30)); // am Deckel reicht 1
        assert!(!set_is_complete(31, 29, 21, 30)); // über dem Deckel → ungültig
        assert!(!set_is_complete(5, 3, 21, 30)); // laufender Satz → nicht fertig
        assert!(!set_is_complete(11, 7, 21, 30)); // 1. Satz läuft → nicht fertig
        assert!(!set_is_complete(20, 18, 21, 30)); // Ziel nicht erreicht
                                                   // 15er-Format, Deckel 21.
        assert!(set_is_complete(15, 5, 15, 21));
        assert!(!set_is_complete(15, 14, 15, 21));
        assert!(set_is_complete(21, 20, 15, 21));
        // Fehlendes Format → Defaults 21/30.
        assert!(set_is_complete(21, 15, 0, 0));
        assert!(!set_is_complete(10, 5, 0, 0));
    }

    #[test]
    fn derive_result_retired_keeps_sets_status_two() {
        let (sets, t1, status) =
            derive_result(vec![(21, 10), (5, 3)], false, true, false, Some(1)).unwrap();
        assert_eq!(sets, vec![(21, 10), (5, 3)]);
        assert!(t1);
        assert_eq!(status, 2);
        assert!(derive_result(vec![(1, 0)], true, true, false, Some(1)).is_err());
        // beides zugleich
    }

    #[test]
    fn derive_result_disqualified_keeps_sets_status_three() {
        // Disqualifikation: bereits gespielte Sätze bleiben, Sieger explizit,
        // ScoreStatus 3. Ohne Sieger → Fehler; mehr als ein Sonderfall → Fehler.
        let (sets, t1, status) =
            derive_result(vec![(21, 10), (11, 8)], false, false, true, Some(2)).unwrap();
        assert_eq!(sets, vec![(21, 10), (11, 8)], "Sätze bleiben erhalten");
        assert!(!t1, "Team 2 gewinnt (Team 1 disqualifiziert)");
        assert_eq!(status, 3);
        assert!(derive_result(vec![], false, false, true, None).is_err());
        assert!(derive_result(vec![(1, 0)], false, true, true, Some(1)).is_err());
        // retired+dq
    }

    // ─────────────── Turnierleitungs-Ergebnis (build_manual_result_update) ───────────────

    #[test]
    fn manual_result_reasserts_the_given_officials() {
        // Live-Befund 14.08.2026: Ohne dieses Feld verlor BTP die
        // Schiedsrichter-Besetzung eines Matches, sobald das Ergebnis
        // eintraf. `build_manual_result_update` muss den übergebenen Wert
        // unverändert in den `MatchUpdate` durchreichen.
        let m = match_on_court();
        let u = build_manual_result_update(
            &m,
            vec![(21, 10), (21, 15)],
            Some(1_000),
            61_000,
            Some((4, 0)),
        )
        .unwrap();
        assert_eq!(u.officials, Some((4, 0)));
    }

    #[test]
    fn manual_result_on_court_frees_field_and_checks_out() {
        // Match 42 auf Feld 101, 2:0 → Feld wird freigegeben, Spieler
        // ausgecheckt (Endzeit gesetzt), Dauer aus dem Aufruf-Stempel.
        let m = match_on_court(); // court_id 101, scoring default (21/30)
        let u = build_manual_result_update(&m, vec![(21, 10), (21, 15)], Some(1_000), 61_000, None)
            .unwrap();
        assert_eq!(u.btp_match_id, 42);
        assert!(u.team1_won);
        assert_eq!(u.score_status, 0);
        assert_eq!(u.free_court_id, Some(101));
        assert_eq!(u.end_ts_ms, Some(61_000));
        assert_eq!(u.duration_mins, 1); // (61000-1000)/60000
        assert!(!u.player_ids.is_empty());
    }

    #[test]
    fn manual_result_not_on_court_has_no_field_or_players() {
        // Spiel ohne Feld (nie zugewiesen): nur das Ergebnis, kein
        // Feld-/Spieler-Block.
        let mut m = match_on_court();
        m.court_id = None;
        m.status = MatchStatus::Scheduled;
        let u =
            build_manual_result_update(&m, vec![(21, 10), (21, 15)], None, 61_000, None).unwrap();
        assert_eq!(u.free_court_id, None);
        assert!(u.player_ids.is_empty());
        assert_eq!(u.end_ts_ms, None);
        assert_eq!(u.duration_mins, 0);
    }

    #[test]
    fn manual_result_rejects_already_decided_and_incomplete() {
        // Bereits gewertet → nie überschreiben.
        let mut done = match_on_court();
        done.winner = Some(1);
        assert!(
            build_manual_result_update(&done, vec![(21, 10), (21, 15)], None, 0, None).is_err()
        );
        // Laufender Satz (5:3) → abgelehnt (nicht regulär zu Ende).
        let m = match_on_court();
        let err =
            build_manual_result_update(&m, vec![(21, 10), (5, 3)], Some(0), 0, None).unwrap_err();
        assert!(
            err.contains("5:3"),
            "Fehler nennt den unfertigen Satz: {err}"
        );
        // Unentschiedener Satzstand (1:1) → kein Sieger.
        assert!(
            build_manual_result_update(&m, vec![(21, 10), (15, 21)], Some(0), 0, None).is_err()
        );
    }

    #[test]
    fn manual_dq_frees_field_keeps_partial_sets() {
        // Disqualifikation (P3): Team 1 disqualifiziert → Team 2 gewinnt,
        // ScoreStatus 3, ein LAUFENDER Satz (5:3) bleibt erhalten (anders als
        // beim regulären Eintrag, der ihn ablehnt), Feld wird freigegeben.
        let m = match_on_court(); // court_id 101
        let u = build_manual_dq_update(&m, 1, vec![(21, 10), (5, 3)], Some(1_000), 61_000, None)
            .unwrap();
        assert_eq!(u.score_status, 3);
        assert!(!u.team1_won, "disqualifiziertes Team 1 verliert");
        assert_eq!(u.sets, vec![(21, 10), (5, 3)], "Teil-Satz bleibt");
        assert_eq!(u.free_court_id, Some(101));
        assert!(!u.player_ids.is_empty());
        assert_eq!(u.duration_mins, 1);
    }

    #[test]
    fn manual_dq_rejects_invalid_team_and_already_decided() {
        let m = match_on_court();
        // Ungültiges Team (nur 1/2 erlaubt).
        assert!(build_manual_dq_update(&m, 3, vec![], None, 0, None).is_err());
        // Bereits gewertet → nie überschreiben.
        let mut done = match_on_court();
        done.winner = Some(2);
        assert!(build_manual_dq_update(&done, 1, vec![], None, 0, None).is_err());
    }

    #[test]
    fn manual_dq_without_court_has_no_field_or_players() {
        // Disqualifikation eines nie zugewiesenen Spiels (kein Feld): nur die
        // Wertung (Status 3), kein Feld-/Spieler-/Endzeit-Block.
        let mut m = match_on_court();
        m.court_id = None;
        m.status = MatchStatus::Scheduled;
        let u = build_manual_dq_update(&m, 2, vec![], None, 61_000, None).unwrap();
        assert_eq!(u.score_status, 3);
        assert!(u.team1_won, "Team 2 disqualifiziert → Team 1 gewinnt");
        assert_eq!(u.free_court_id, None);
        assert!(u.player_ids.is_empty());
        assert_eq!(u.end_ts_ms, None);
        assert_eq!(u.duration_mins, 0);
    }

    // ein 0:0 als Geistersatz nach Spielende und wird weggelassen.
    #[test]
    fn match_decided_best_of_3() {
        assert!(!match_decided(3, &[])); // erster Satz – offen
        assert!(!match_decided(3, &[(21, 7)])); // 1:0 – Satzpause, neuer Satz
        assert!(!match_decided(3, &[(21, 7), (15, 21)])); // 1:1 – Entscheidungssatz
        assert!(match_decided(3, &[(21, 7), (21, 15)])); // 2:0 – entschieden
        assert!(match_decided(3, &[(21, 7), (15, 21), (21, 18)])); // 2:1 – entschieden
    }

    #[test]
    fn score_push_queue_serialises_per_court_and_keeps_only_the_newest() {
        // Der Kern der Warteschlange (Spec-frei, reine Zustandslogik):
        // je Feld genau EIN Arbeiter, und während er läuft, überlebt nur
        // der neueste Stand — ein Punkteregen wird gebündelt statt in
        // eine Anfragen-Lawine übersetzt.
        let q = ScorePushQueue::default();
        let update = |n: i64| {
            crate::badhub::diff::Update::Single(build_tupdate(&match_on_court(), n as u64))
        };

        assert!(
            q.einstellen(1, 42, update(1)),
            "erster startet den Arbeiter"
        );
        assert!(
            !q.einstellen(1, 42, update(2)),
            "zweiter reiht sich beim laufenden Arbeiter ein"
        );
        assert!(
            !q.einstellen(1, 42, update(3)),
            "dritter ebenso — er ersetzt nur den wartenden Stand"
        );
        // Ein anderes Feld ist davon unberührt: eigener Arbeiter.
        assert!(q.einstellen(2, 7, update(9)), "Feld 2 hat seinen eigenen");

        // Der Arbeiter bekommt genau EINEN Stand — den zuletzt
        // eingestellten (die dazwischen sind überholt).
        let (match_id, _) = q.naechster(1).expect("ein Stand wartet");
        assert_eq!(match_id, 42);
        assert!(
            q.naechster(1).is_none(),
            "danach ist nichts mehr da — der Arbeiter meldet sich ab"
        );
        // Abgemeldet heißt: Der nächste Aufrufer startet wieder einen.
        assert!(q.einstellen(1, 42, update(4)), "neuer Arbeiter nötig");
    }

    #[test]
    fn match_decided_best_of_1_and_5() {
        assert!(!match_decided(1, &[])); // einziger Satz läuft
        assert!(match_decided(1, &[(21, 15)])); // 1:0 in Bo1 → entschieden
        assert!(!match_decided(5, &[(21, 1), (21, 2)])); // 2:0 in Bo5 – noch offen
        assert!(match_decided(5, &[(21, 1), (21, 2), (21, 3)])); // 3:0 – entschieden
    }

    /// Stale-Filter (A4, Turnier-Befund HM-03): Feld 101 hat Match 42 —
    /// ein Score-Update, das ein ANDERES Match nennt (hängengebliebenes
    /// Tablet nach Doze/Reconnect), wird verworfen, bevor Cache oder
    /// Liveticker angefasst werden. matchId 0 (alte Tablet-Seite) und die
    /// passende matchId laufen weiter durch.
    #[tokio::test]
    async fn handle_score_drops_score_of_foreign_match() {
        let ctx = make_ctx(1); // toter Port — es kommt nie zu BTP/Netz
        ctx.tablet.record_score(101, 42, vec![(10, 8)]);
        // Nachzügler des alten Matches 7 → verworfen (kein Netz-Push, da
        // die Funktion vor dem record_score/Push zurückkehrt).
        handle_score(101, 14, 16, &[], 7, &ctx).await;
        assert_eq!(
            ctx.tablet.monitor_court(101).sets,
            vec![(10, 8)],
            "Stand des aktuellen Matches bleibt unangetastet"
        );
    }

    /// Spec `spielzeiten-prognose` (E2): Der Host stempelt den Nettostart
    /// beim ERSTEN eingehenden Punktestand > 0 — genau einmal; 0:0 und
    /// Folgestände ändern nichts.
    #[tokio::test]
    async fn handle_score_stempelt_den_ersten_punkt_genau_einmal() {
        let ctx = make_ctx(1); // tote Ports — kein BTP, kein badhub
        handle_score(101, 0, 0, &[], 42, &ctx).await; // 0:0 stempelt nicht
        assert!(ctx
            .tablet
            .match_times_store()
            .entry(42)
            .is_none_or(|e| e.first_point_ms.is_none()));

        handle_score(101, 1, 0, &[], 42, &ctx).await; // erster Punkt
        let first = ctx
            .tablet
            .match_times_store()
            .entry(42)
            .and_then(|e| e.first_point_ms);
        assert!(first.is_some(), "erster Stand > 0 stempelt");

        handle_score(101, 5, 3, &[], 42, &ctx).await; // Folgestand
        assert_eq!(
            ctx.tablet
                .match_times_store()
                .entry(42)
                .and_then(|e| e.first_point_ms),
            first,
            "Folgestände ändern den Stempel nicht"
        );
    }

    /// Spec `spielzeiten-prognose` (E2): Verworfene Scores (Stale-Filter,
    /// Finalisiert-Gate) stempeln keinen ersten Punkt.
    #[tokio::test]
    async fn ein_verworfener_score_stempelt_keinen_ersten_punkt() {
        let ctx = make_ctx(1);
        // Stale: Tablet zählt Match 7, Feld hat Match 42.
        handle_score(101, 5, 3, &[], 7, &ctx).await;
        assert!(ctx.tablet.match_times_store().entry(42).is_none());
        assert!(ctx.tablet.match_times_store().entry(7).is_none());

        // Finalisiert-Gate: Match 42 ist in BTP fertig eingegeben.
        ctx.tablet.mark_finalized(101, 42);
        handle_score(101, 5, 3, &[], 42, &ctx).await;
        assert!(ctx.tablet.match_times_store().entry(42).is_none());
    }

    /// Review 2026-08-16 (F2): Eine alte Tablet-Seite sendet matchId 0 und
    /// passiert Finalisiert-Gate und Stale-Filter ungeprüft — so ein
    /// Nachzügler-Score des Vorspiels darf dem NEUEN Feld-Match keinen
    /// Nettostart stempeln (er wäre nie korrigierbar und verfälschte den
    /// Klassen-Median massiv).
    #[tokio::test]
    async fn ein_legacy_score_ohne_match_id_stempelt_keinen_ersten_punkt() {
        let ctx = make_ctx(1);
        handle_score(101, 15, 12, &[], 0, &ctx).await;
        assert!(
            ctx.tablet.match_times_store().entry(42).is_none(),
            "matchId 0 ist nicht zuordenbar — kein Stempel"
        );
    }

    /// Spec `spielzeiten-prognose` (E1): Nach einem App-Neustart mitten im
    /// Spiel liefert der persistierte Erst-Stempel die BTP-`Duration` —
    /// wo bisher 0 stand (`on_court_since` lebt nur im RAM).
    #[tokio::test]
    async fn process_result_nimmt_die_dauer_aus_dem_zeiten_store() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        // Neustart-Lage: kein on_court_since, aber der Store kennt die
        // erste Feldzuweisung von vor 10 Minuten.
        ctx.tablet.match_times_store().reconcile(
            &[(42, "A", "HE", "")],
            &std::collections::HashSet::new(),
            now_ms().saturating_sub(600_000),
        );
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        let reqs = recorded.lock().unwrap();
        let fields = match_fields(&reqs[0]);
        assert_eq!(int(&fields, "Duration"), Some(10));
    }

    /// Spec `spielzeiten-prognose` (E3/E11): Der Tablet-Pfad stempelt das
    /// Spielende beim Host-Eingang — regulär nur bei ScoreStatus 0.
    #[tokio::test]
    async fn process_result_stempelt_das_ende_als_regulaer() {
        let (port, _recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        let e = ctx.tablet.match_times_store().entry(42).unwrap();
        assert!(e.finished_ms.is_some());
        assert!(e.regular, "regulär ausgespielt → Messwert");
    }

    /// Spec `spielzeiten-prognose` (E3): Eine Ergebniskorrektur überschreibt
    /// den Ende-Stempel nicht; E11: eine Aufgabe zählt nicht als regulär.
    #[tokio::test]
    async fn process_result_ueberschreibt_den_ende_stempel_nicht() {
        let (port, _recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        // Das Ende steht schon (z. B. frühere Wertung) …
        ctx.tablet
            .match_times_store()
            .stamp_finished(42, false, 123);
        // … eine Korrektur mit anderem Ergebnis ändert daran nichts.
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 17)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        let e = ctx.tablet.match_times_store().entry(42).unwrap();
        assert_eq!(e.finished_ms, Some(123));
        assert!(!e.regular, "die Erst-Einstufung bleibt");
    }

    /// Spec `spielzeiten-prognose` (E11): Aufgabe (retired) stempelt das
    /// Ende, zählt aber nicht als regulärer Messwert.
    #[tokio::test]
    async fn process_result_aufgabe_ist_kein_regulaerer_messwert() {
        let (port, _recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let mut body = body_with(&[(21, 10), (5, 2)]);
        body.retired = true;
        body.winner = Some(1);
        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);
        let e = ctx.tablet.match_times_store().entry(42).unwrap();
        assert!(e.finished_ms.is_some());
        assert!(!e.regular, "Aufgabe → kein Messwert für die Statistik");
    }

    /// Review-Befund 2026-08-16: Ein manuelles Ergebnis für ein Spiel, das
    /// nicht (mehr) auf einem Feld steht, muss die Dauer trotzdem tragen —
    /// nur Feldfreigabe, Auschecken und Endzeit hängen am Feld.
    #[test]
    fn ein_manuelles_ergebnis_ohne_feld_traegt_trotzdem_die_dauer() {
        let mut m = match_on_court();
        m.court_id = None;
        m.court = None;
        let u = build_manual_result_update(
            &m,
            vec![(21, 10), (21, 15)],
            Some(1_000),
            1_000 + 40 * 60_000,
            None,
        )
        .unwrap();
        assert_eq!(u.duration_mins, 40);
        assert_eq!(u.free_court_id, None, "kein Feld freizugeben");
        assert_eq!(u.end_ts_ms, None, "kein Spielende je Spieler ohne Feld");
        assert!(u.player_ids.is_empty());
    }

    /// Spec `spielzeiten-prognose` (E1): Auch ein über das TABLET gemeldeter
    /// Walkover sendet `Duration: 0` — kampflos wurde nicht gespielt.
    #[tokio::test]
    async fn process_result_walkover_sendet_dauer_null() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        ctx.tablet.match_times_store().reconcile(
            &[(42, "A", "HE", "")],
            &std::collections::HashSet::new(),
            now_ms().saturating_sub(2_400_000),
        );
        let mut body = body_with(&[]);
        body.walkover = true;
        body.winner = Some(1);
        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);
        let reqs = recorded.lock().unwrap();
        assert_eq!(int(&match_fields(&reqs[0]), "Duration"), Some(0));
    }

    /// Review-Befund 2026-08-16: Eine Korrektur rechnet mit dem
    /// URSPRÜNGLICHEN Ende (E3-Stempel), nicht mit „jetzt" — sonst
    /// überschriebe sie eine korrekte BTP-Duration mit Stunden.
    #[tokio::test]
    async fn eine_korrektur_rechnet_mit_dem_urspruenglichen_ende() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let start = now_ms().saturating_sub(7_200_000); // vor 2 h zugewiesen
        ctx.tablet.match_times_store().reconcile(
            &[(42, "A", "HE", "")],
            &std::collections::HashSet::new(),
            start,
        );
        // Ursprüngliches Ende nach 40 Minuten.
        ctx.tablet
            .match_times_store()
            .stamp_finished(42, true, start + 40 * 60_000);
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 17)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        let reqs = recorded.lock().unwrap();
        assert_eq!(int(&match_fields(&reqs[0]), "Duration"), Some(40));
    }

    /// Review-Befund 2026-08-16 (bestätigt, Runde 3): Eine irrtümliche
    /// NICHT-reguläre Wertung (Backend/TL-Web, z. B. fürs falsche Spiel
    /// während einer Störung) darf dem später eintreffenden ECHTEN
    /// Tablet-Ergebnis nicht ihr altes Ende unterschieben — sonst gingen
    /// `Duration: 0` und eine rückdatierte `LastTimeOnCourt` nach BTP
    /// (Phantom-Mindestpause). Der Tablet-Pfad vertraut deshalb nur einem
    /// REGULÄREN Erst-Stempel als Korrektur-Anker.
    #[tokio::test]
    async fn eine_fremde_backend_wertung_datiert_das_tablet_ende_nicht_zurueck() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let start = now_ms().saturating_sub(40 * 60_000);
        ctx.tablet.match_times_store().reconcile(
            &[(42, "A", "HE", "")],
            &std::collections::HashSet::new(),
            start,
        );
        // Irrtümlicher manueller Stempel, eine Stunde VOR dem Bruttostart.
        ctx.tablet
            .match_times_store()
            .stamp_finished(42, false, start.saturating_sub(3_600_000));
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        let reqs = recorded.lock().unwrap();
        assert_eq!(
            int(&match_fields(&reqs[0]), "Duration"),
            Some(40),
            "Ende = jetzt — der fremde Stempel zählt nicht"
        );
    }

    /// Review-Befund 2026-08-16: Ein über Nacht auf dem Feld „geparktes"
    /// Spiel (Mehrtages-Turnier) darf keine absurde Duration nach BTP
    /// melden — jenseits der Plausibilitätsgrenze gilt „unbekannt" (0).
    #[tokio::test]
    async fn eine_unplausible_dauer_geht_als_unbekannt_nach_btp() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        ctx.tablet.match_times_store().reconcile(
            &[(42, "A", "HE", "")],
            &std::collections::HashSet::new(),
            now_ms().saturating_sub(16 * 3_600_000), // gestern Abend
        );
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        let reqs = recorded.lock().unwrap();
        assert_eq!(int(&match_fields(&reqs[0]), "Duration"), Some(0));
    }

    /// A2 / ADR 0017: `ownership_active` spiegelt exakt den Legacy-Schalter.
    /// Default (`reconnect_legacy_rev = false`) → Ownership aktiv, das Tablet
    /// folgt der server-berechneten Autorität. Legacy an → `ownership_active`
    /// false, das Tablet fällt auf seine rev-Logik zurück (Laufzeit-Rollback).
    #[test]
    fn ownership_active_reflects_legacy_flag() {
        let mut cfg = AppConfig::default();
        assert!(!cfg.reconnect_legacy_rev, "Default: Ownership-Verhalten");
        assert!(ownership_active(&cfg), "Default → Ownership aktiv");
        cfg.reconnect_legacy_rev = true;
        assert!(
            !ownership_active(&cfg),
            "Legacy an → Ownership inaktiv (rev)"
        );
    }

    /// Finalisiert-Gate (A2 / ADR 0017, Regel b): Ist das Match des Felds in
    /// BTP finalisiert (per Hand fertig eingegeben), verwirft `handle_score`
    /// einen nachlaufenden Score fürs selbe Match — das Hand-Ergebnis wird
    /// nicht überbügelt. Ein Score mit fremder/0-matchId bleibt vom Gate
    /// unberührt (greift dort nur der bestehende Stale-Filter).
    #[tokio::test]
    async fn handle_score_drops_score_of_finalized_match() {
        let ctx = make_ctx(1); // toter Port — es kommt nie zu BTP/Netz
        ctx.tablet.record_score(101, 42, vec![(21, 10)]);
        // Match 42 auf Feld 101 ist in BTP finalisiert.
        ctx.tablet.mark_finalized(101, 42);
        // Nachzügler fürs finalisierte Match 42 → verworfen (kein Push, kein
        // Überschreiben des Hand-Ergebnisses).
        handle_score(101, 21, 15, &[], 42, &ctx).await;
        assert_eq!(
            ctx.tablet.monitor_court(101).sets,
            vec![(21, 10)],
            "finalisiertes Match: nachlaufender Score verworfen"
        );
    }

    /// Nachschub-Queue (A5): Schlägt der BTP-Write fehl, landet der
    /// komplette MatchUpdate in der Queue — der Sync-Loop reicht ihn nach,
    /// sobald BTP wieder antwortet.
    #[tokio::test]
    async fn process_result_failure_queues_btp_retry() {
        let ctx = make_ctx(1); // Port 1: Verbindung wird sofort abgewiesen
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(!resp.ok, "Write gegen toten BTP-Port schlägt fehl");
        let q = ctx.tablet.btp_retries();
        assert_eq!(q.len(), 1, "Fehlschlag ist eingereiht");
        assert_eq!(q[0].update.btp_match_id, 42);
        assert_eq!(q[0].update.sets, vec![(21, 10), (21, 15)]);
        assert!(q[0].enqueued_ms > 0, "Bezugszeitpunkt = Spielende gesetzt");
    }

    /// Nach einem Ergebnis gibt `process_result` das Feld in BTP frei — seit
    /// dem Regressions-Fix (Turnier 17.07.2026) in EINEM kombinierten
    /// SENDUPDATE zusammen mit dem Ergebnis: Der frühere zweite, „nackte"
    /// Match-Knoten konnte das Ergebnis in BTP wieder entwerten.
    #[tokio::test]
    async fn process_result_frees_court_in_btp() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);

        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(
            resp.ok,
            "Ergebnis sollte erfolgreich sein: {:?}",
            resp.error
        );

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs.len(), 1, "Ergebnis + Feldfreigabe = EIN SENDUPDATE");

        // Der eine SENDUPDATE trägt beides: Feldfreigabe (Court 101 OHNE
        // MatchID, Match.CourtID=0) UND das vollständige Ergebnis inkl.
        // `Status` (Abschluss-Trigger, Regression v0.9.103).
        let upd = xml::find(&reqs[0], "Update").expect("Update");
        let tour = xml::find(upd.children(), "Tournament").expect("Tournament");
        let courts = xml::find(tour.children(), "Courts").expect("Courts-Block (Feldfreigabe)");
        let court = xml::find(courts.children(), "Court").expect("Court");
        assert_eq!(int(court.children(), "ID"), Some(101));
        assert!(
            xml::find(court.children(), "MatchID").is_none(),
            "frei = Court ohne MatchID"
        );
        let matches = xml::find(tour.children(), "Matches").expect("Matches");
        let mnode = xml::find(matches.children(), "Match").expect("Match");
        // Das Match BEHÄLT seine echte CourtID (Turnier-Doku „wo wurde
        // gespielt", Tilo-Feedback 19.07.) — frei wird nur der Court-Block.
        assert_eq!(int(mnode.children(), "CourtID"), Some(101));
        assert_eq!(int(mnode.children(), "Winner"), Some(1));
        assert_eq!(int(mnode.children(), "Status"), Some(0));
        assert!(
            xml::find(mnode.children(), "Sets").is_some(),
            "Sätze müssen im kombinierten Request stehen"
        );
        // Spielende je Spieler: Players-Block mit LastTimeOnCourt +
        // CheckedIn=false für alle Spieler des Matches.
        let players = xml::find(tour.children(), "Players").expect("Players-Block (Spielende)");
        assert!(!players.children().is_empty());
        for p in players.children() {
            assert!(xml::find(p.children(), "LastTimeOnCourt").is_some());
            assert_eq!(
                xml::find(p.children(), "CheckedIn").and_then(|n| n.value()?.as_bool()),
                Some(false)
            );
        }
    }

    /// Punktverlauf (AK-13): Ein erfolgreiches Ergebnis finalisiert den
    /// aufgezeichneten Verlauf — bei Aufgabe mit Kennzeichnung; danach
    /// nimmt der Store keine Rallies mehr an. Ohne Aufzeichnung (Papier)
    /// entsteht kein Geister-Eintrag.
    #[tokio::test]
    async fn process_result_finalizes_timeline() {
        let (port, _recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        // Verlauf wie vom Tablet gezählt (Match 42 aus body_with).
        assert!(ctx.tablet.timeline_store().apply_rally(42, 1, 1, "A", 1, 0));

        let mut body = body_with(&[(21, 10), (5, 2)]);
        body.retired = true;
        body.winner = Some(1);
        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);

        let tl = ctx.tablet.timeline_store().timeline(42).expect("Verlauf");
        assert!(tl.finished && tl.retired, "finalisiert + Aufgabe markiert");
        assert!(
            !ctx.tablet.timeline_store().apply_rally(42, 1, 2, "B", 1, 1),
            "nach Abschluss keine Rallies mehr"
        );
        // Papier-Spiel ohne Aufzeichnung: finalize legt nichts an.
        assert!(ctx.tablet.timeline_store().timeline(999).is_none());
    }

    /// Aufgabe vom Tablet: der EINE kombinierte SENDUPDATE trägt
    /// `ScoreStatus=2` + `Status` + Feldfreigabe (Courts-Block; das Match
    /// behält seine CourtID) — Sonderfall aus dem Turnier-Befund 17.07.2026.
    #[tokio::test]
    async fn process_result_retired_combines_result_and_court_release() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);

        let mut body = body_with(&[(21, 10), (5, 2)]);
        body.retired = true;
        body.winner = Some(1);
        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs.len(), 1, "auch bei Aufgabe genau EIN SENDUPDATE");
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(1));
        assert_eq!(int(&m, "ScoreStatus"), Some(2), "2 = Aufgabe");
        assert_eq!(int(&m, "Status"), Some(0));
        assert_eq!(int(&m, "CourtID"), Some(101), "echte CourtID bleibt");
        let upd = xml::find(&reqs[0], "Update").unwrap();
        let tour = xml::find(upd.children(), "Tournament").unwrap();
        let courts = xml::find(tour.children(), "Courts").expect("Feldfreigabe im selben Request");
        let court = xml::find(courts.children(), "Court").unwrap();
        assert_eq!(int(court.children(), "ID"), Some(101));
    }

    /// Sieger wird aus den Sätzen abgeleitet (Team 2 gewinnt 0:2) und als
    /// `Winner=2`, `ScoreStatus=0`, mit beiden Sätzen nach BTP geschrieben.
    #[tokio::test]
    async fn result_winner_derived_from_sets() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);

        let resp = process_result(&ctx, &body_with(&[(10, 21), (15, 21)])).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(2), "Team 2 gewinnt");
        assert_eq!(int(&m, "ScoreStatus"), Some(0), "regulär ausgespielt");
        let sets = xml::find(&m, "Sets").expect("Sets");
        assert_eq!(sets.children().len(), 2, "beide Sätze übertragen");
    }

    /// Ein Satz über dem Deckel wird abgelehnt — auch vom Tablet.
    ///
    /// Aus dem Betrieb gemeldet: In einem Turnier, das bis 15 mit Deckel 21
    /// spielt, ließ sich am Tablet über „Ergebnis eintragen" ein 27:25
    /// speichern. Die Prüfung gab es längst, sie hing aber nur am Weg der
    /// Turnierleitung; der Tablet-Weg verließ sich darauf, dass die Seite
    /// selbst nichts Ungültiges zählen lässt. Für getippte Ergebnisse gilt
    /// das nicht — und ein falscher Satz wandert von hier direkt nach BTP
    /// und in den Liveticker.
    #[tokio::test]
    async fn a_set_beyond_the_cap_is_rejected_from_the_tablet_too() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx_scoring(
            port,
            ScoringFormat {
                best_of: 3,
                target_score: 15,
                cap_score: 21,
                interval_at: Some(8),
            },
        );

        let resp = process_result(&ctx, &body_with(&[(27, 25), (15, 9)])).await;

        assert!(!resp.ok, "27:25 darf bei Deckel 21 nicht durchgehen");
        let text = resp.error.unwrap_or_default();
        assert!(text.contains("27:25"), "der Grund nennt den Satz: {text}");
        assert!(
            recorded.lock().unwrap().is_empty(),
            "nichts darf nach BTP gegangen sein"
        );
    }

    /// Ein regulärer Satz nach demselben Format geht durch — sonst prüfte der
    /// Test oben nur, dass überhaupt etwas abgelehnt wird.
    #[tokio::test]
    async fn a_valid_set_for_the_format_still_passes() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx_scoring(
            port,
            ScoringFormat {
                best_of: 3,
                target_score: 15,
                cap_score: 21,
                interval_at: Some(8),
            },
        );

        // 21:20 ist am Deckel erlaubt (dort genügt ein Punkt Vorsprung).
        let resp = process_result(&ctx, &body_with(&[(15, 9), (21, 20)])).await;

        assert!(resp.ok, "{:?}", resp.error);
        assert_eq!(recorded.lock().unwrap().len(), 1);
    }

    /// Bei einer **Aufgabe** darf der Satz unfertig sein — jemand hört mitten
    /// im Spiel auf, und genau dieser Stand gehört nach BTP.
    #[tokio::test]
    async fn a_retirement_may_carry_an_unfinished_set() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx_scoring(
            port,
            ScoringFormat {
                best_of: 3,
                target_score: 15,
                cap_score: 21,
                interval_at: Some(8),
            },
        );
        let mut body = body_with(&[(15, 9), (3, 5)]);
        body.retired = true;
        body.winner = Some(2);

        let resp = process_result(&ctx, &body).await;

        assert!(resp.ok, "{:?}", resp.error);
        assert_eq!(recorded.lock().unwrap().len(), 1);
    }

    /// Aufgabe (Retired): Sieger explizit, `ScoreStatus=2`.
    #[tokio::test]
    async fn result_retired_sets_score_status_2() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let mut body = body_with(&[(21, 10), (5, 11)]);
        body.retired = true;
        body.winner = Some(1);

        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(1));
        assert_eq!(int(&m, "ScoreStatus"), Some(2), "Aufgabe");
    }

    /// Kampflos (Walkover): `ScoreStatus=1`, Satzliste wird verworfen.
    #[tokio::test]
    async fn result_walkover_clears_sets() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        let mut body = body_with(&[(21, 10), (21, 15)]); // Sätze werden ignoriert
        body.walkover = true;
        body.winner = Some(2);

        let resp = process_result(&ctx, &body).await;
        assert!(resp.ok, "{:?}", resp.error);

        let reqs = recorded.lock().unwrap();
        let m = match_fields(&reqs[0]);
        assert_eq!(int(&m, "Winner"), Some(2));
        assert_eq!(int(&m, "ScoreStatus"), Some(1), "Kampflos");
        let sets = xml::find(&m, "Sets").expect("Sets");
        assert!(sets.children().is_empty(), "Kampflos verwirft Sätze");
    }

    // ── Idempotenz (Hebel B, Teil c): Retry auf geräumtem/gewechseltem Feld ──

    /// Setzt den Snapshot auf genau diese Matches (Feld „1"), sonst leer —
    /// simuliert einen frischen BTP-Poll. Leere Liste = Feld geräumt.
    fn set_matches(ctx: &ServerCtx, matches: Vec<BtpMatch>) {
        ctx.tablet.set_snapshot(BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches,
            courts: vec!["1".into()],
            locations: vec![],
            court_infos: vec![],
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        });
    }

    /// Ein „zuletzt-geschrieben"-Merker für Match 42 (nur die entscheidenden
    /// Felder zählen für den Idempotenz-Vergleich).
    fn merker_update(
        sets: Vec<(i64, i64)>,
        team1_won: bool,
        score_status: i64,
    ) -> proto::MatchUpdate {
        proto::MatchUpdate {
            btp_match_id: 42,
            draw_id: 7,
            planning_id: 1001,
            sets,
            team1_won,
            duration_mins: 0,
            score_status,
            free_court_id: Some(101),
            player_ids: vec![],
            end_ts_ms: None,
            officials: None,
        }
    }

    /// (1) Erfolgreicher Write, Feld danach geräumt, **identischer** Retry →
    /// `ok` (nicht „Kein Match auf diesem Court") → das Tablet löscht sein
    /// pendingResult, der Retry stoppt.
    #[tokio::test]
    async fn idempotent_retry_on_cleared_court_is_acked() {
        let (port, _rec) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        // 1. Erfolgreicher Write (setzt den Merker via write_result_settled).
        assert!(
            process_result(&ctx, &body_with(&[(21, 10), (21, 15)]))
                .await
                .ok
        );
        // BTP-Poll nach dem Write: Feld geräumt (Match nicht mehr OnCourt).
        set_matches(&ctx, vec![]);
        // 2. Identischer Retry → quittiert.
        let retry = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(
            retry.ok,
            "identischer Retry auf geräumtem Feld quittiert: {:?}",
            retry.error
        );
    }

    /// (2) R5: Retry mit **abweichenden** Sätzen auf geräumtem Feld →
    /// **Fehler** (veraltete/falsche Einreichung, keine Falsch-Bestätigung).
    #[tokio::test]
    async fn diverging_retry_on_cleared_court_still_errors() {
        let (port, _rec) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        assert!(
            process_result(&ctx, &body_with(&[(21, 10), (21, 15)]))
                .await
                .ok
        );
        set_matches(&ctx, vec![]); // Feld geräumt, Snapshot ohne Sieger
                                   // Abweichende Sätze (Team 1 gewinnt weiter, aber andere Punkte).
        let resp = process_result(&ctx, &body_with(&[(21, 18), (21, 19)])).await;
        assert!(!resp.ok, "abweichender Payload darf nicht quittiert werden");
    }

    /// (3) Ein **genuin neues** Ergebnis auf einem noch belegten Feld läuft
    /// durch die normale Validierung + Write — die Idempotenz greift dort
    /// NICHT (auch wenn für das Match bereits ein Merker existiert).
    #[tokio::test]
    async fn genuine_new_result_on_occupied_court_writes_through() {
        let (port, recorded) = spawn_mock_btp().await;
        let ctx = make_ctx(port);
        // Vorheriger Write setzt den Merker; das Feld bleibt aber belegt
        // (Snapshot unverändert, Match 42 weiter OnCourt).
        assert!(
            process_result(&ctx, &body_with(&[(21, 10), (21, 15)]))
                .await
                .ok
        );
        // Abweichendes neues Ergebnis auf dem noch belegten Feld → Write.
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 18)])).await;
        assert!(resp.ok, "{:?}", resp.error);
        assert_eq!(
            recorded.lock().unwrap().len(),
            2,
            "beide Ergebnisse gingen als eigener SENDUPDATE nach BTP"
        );
    }

    /// (4) Liegt der Merker **älter** als das TTL, wird ein Retry wieder mit
    /// Fehler beantwortet — eine echte spätere Korrektur soll nicht als „schon
    /// erledigt" abgewürgt werden.
    #[tokio::test]
    async fn expired_marker_no_longer_acks_retry() {
        let ctx = make_ctx(1); // toter Port — es kommt zu keinem Write
        let old = now_ms().saturating_sub(RESULT_IDEMPOTENCY_TTL + 5_000);
        ctx.tablet
            .note_direct_btp_write(merker_update(vec![(21, 10), (21, 15)], true, 0), old);
        set_matches(&ctx, vec![]); // Feld geräumt, kein Sieger im Snapshot
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(!resp.ok, "Merker älter als TTL → Fehler statt ok");
    }

    /// (5) Feld hat gewechselt (`m.id` ≠ `match_id`), der Merker trägt das
    /// identische Alt-Ergebnis → `ok` (der Retry des alten Matches wird
    /// quittiert, auch wenn inzwischen ein anderes Match auf dem Feld steht).
    #[tokio::test]
    async fn identical_old_result_on_switched_court_is_acked() {
        let ctx = make_ctx(1); // toter Port — reine Idempotenz, kein Write
        ctx.tablet
            .note_direct_btp_write(merker_update(vec![(21, 10), (21, 15)], true, 0), now_ms());
        // Feld 101 ist inzwischen mit einem ANDEREN Match (99) belegt.
        let mut other = match_on_court();
        other.id = 99;
        set_matches(&ctx, vec![other]);
        // Retry des alten Match-42-Ergebnisses (body_with adressiert Match 42).
        let resp = process_result(&ctx, &body_with(&[(21, 10), (21, 15)])).await;
        assert!(
            resp.ok,
            "identischer Alt-Retry auf gewechseltem Feld quittiert: {:?}",
            resp.error
        );
    }

    /// (6) Regression zum Review-BLOCKER: Ein fertig geschriebenes Match trägt im
    /// BTP-Snapshot dauerhaft einen Sieger. Ein **abweichendes** (sieger-gleiches)
    /// Ergebnis auf dem geräumten Feld darf NICHT allein wegen der passenden
    /// Sieger-Seite quittiert werden (das wäre ein stiller Verlust einer echten
    /// TL-Korrektur) — ohne feldgenauen Merker liefert `settled_ok` `false`.
    #[tokio::test]
    async fn diverging_result_is_not_acked_by_snapshot_winner() {
        let ctx = make_ctx(1); // toter Port — kein Write, also KEIN Merker
                               // Snapshot: Match 42 ist fertig (Sieger Team 1) und NICHT mehr auf Feld
                               // 101 (geräumt) → match_for_court(101) == None → settled_ok-Pfad.
        let mut finished = match_on_court();
        finished.winner = Some(1);
        finished.status = MatchStatus::Finished;
        finished.court_id = None;
        set_matches(&ctx, vec![finished]);
        // Abweichende Sätze, aber Team 1 gewinnt weiter (Sieger-Seite passt).
        let resp = process_result(&ctx, &body_with(&[(21, 18), (21, 19)])).await;
        assert!(
            !resp.ok,
            "sieger-gleiches, aber abweichendes Ergebnis darf NICHT quittiert werden"
        );
    }

    // ── Ablehnungen: ungültige Ergebnisse werden NICHT nach BTP geschrieben ──
    // (process_result bricht vor jedem Netzwerkzugriff ab; toter Port genügt.)

    async fn rejected(body: ResultBody) -> super::ResultResponse {
        let ctx = make_ctx(1); // Port 1 wird nie kontaktiert
        process_result(&ctx, &body).await
    }

    #[tokio::test]
    async fn rejects_empty_sets_without_walkover_or_retired() {
        assert!(!rejected(body_with(&[])).await.ok);
    }

    #[tokio::test]
    async fn rejects_drawn_sets() {
        // 1:1 → kein Sieger ableitbar.
        assert!(!rejected(body_with(&[(21, 10), (10, 21)])).await.ok);
    }

    #[tokio::test]
    async fn rejects_too_many_sets() {
        let many: Vec<(i64, i64)> = (0..10).map(|_| (21, 0)).collect();
        assert!(!rejected(body_with(&many)).await.ok);
    }

    #[tokio::test]
    async fn rejects_invalid_set_score() {
        assert!(!rejected(body_with(&[(100, 0)])).await.ok);
    }

    #[tokio::test]
    async fn rejects_walkover_without_winner() {
        let mut b = body_with(&[]);
        b.walkover = true; // winner bleibt None
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_retired_without_winner() {
        let mut b = body_with(&[(21, 10)]);
        b.retired = true; // winner bleibt None
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_walkover_and_retired_together() {
        let mut b = body_with(&[]);
        b.walkover = true;
        b.retired = true;
        b.winner = Some(1);
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_when_court_match_changed() {
        let mut b = body_with(&[(21, 10), (21, 12)]);
        b.match_id = 999; // anderes Match als auf dem Court (42)
        assert!(!rejected(b).await.ok);
    }

    #[tokio::test]
    async fn rejects_when_no_match_on_court() {
        let mut b = body_with(&[(21, 10), (21, 12)]);
        b.court_id = 999; // kein Match auf diesem Feld
        assert!(!rejected(b).await.ok);
    }

    // ───────────── Panel-Profile (Spec tl-web-panelsystem, ADR 0025) ────────

    /// `ServerCtx` mit einer `config.json` in einem eigenen Temp-Verzeichnis
    /// (nicht der geteilte `bts_test_config.json`-Pfad von [`make_ctx`], der
    /// bei parallel laufenden Tests kollidieren könnte) und den gegebenen
    /// Turnierleitungs-Geräten. Gibt das `TempDir` mit zurück, damit es nicht
    /// vor Testende gelöscht wird — sowie den geteilten `Arc<Mutex<AppConfig>>`,
    /// denselben, den `ServerCtx::mutate_app_config` benutzt (Lost-Update-
    /// Regressionstest unten braucht Zugriff darauf, um den zweiten,
    /// unabhängigen Schreibpfad — `commands::mutate_config_at` — auf
    /// demselben Schloss nachzustellen).
    fn make_tl_ctx(
        devices: Vec<crate::config::TlDevice>,
    ) -> (
        ServerCtx,
        Arc<std::sync::Mutex<AppConfig>>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let mut config = AppConfig::default();
        config.tl_web.enabled = true;
        config.tl_web.devices = devices;
        config.save_to(&config_path).unwrap();

        let tablet = Arc::new(TabletState::default());
        let tmp = std::env::temp_dir();
        let shared_config = Arc::new(std::sync::Mutex::new(config.clone()));
        let ctx = ServerCtx::new(
            tablet,
            config,
            reqwest::Client::new(),
            tmp.clone(),
            config_path,
            dir.path().join("bts_test_assign_tl.json"),
            tmp,
            shared_config.clone(),
        );
        (ctx, shared_config, dir)
    }

    fn bearer_headers(token: &str) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        headers
    }

    #[tokio::test]
    async fn tl_state_lan_sets_active_profile_header_matching_device() {
        let (ctx, _shared, _dir) = make_tl_ctx(vec![crate::config::TlDevice {
            id: "dev-a".into(),
            token: "tok-a".into(),
            label: "Tablet A".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: "profil-wand".into(),
        }]);
        let response = tl_state(State(Arc::new(ctx)), bearer_headers("tok-a"))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(X_TL_ACTIVE_PROFILE).unwrap(),
            "profil-wand"
        );
    }

    #[tokio::test]
    async fn tl_state_lan_no_header_leak_across_devices() {
        // Zwei Geräte mit unterschiedlichem Zugang UND unterschiedlichem
        // Profil, in derselben Testreihe abgefragt — jedes bekommt exakt
        // sein eigenes Profil im Header, nie das des anderen.
        let (ctx, _shared, _dir) = make_tl_ctx(vec![
            crate::config::TlDevice {
                id: "dev-a".into(),
                token: "tok-a".into(),
                label: "Tablet A".into(),
                created_at_ms: 1,
                hall: String::new(),
                profile_id: "profil-a".into(),
            },
            crate::config::TlDevice {
                id: "dev-b".into(),
                token: "tok-b".into(),
                label: "Tablet B".into(),
                created_at_ms: 1,
                hall: String::new(),
                profile_id: "profil-b".into(),
            },
        ]);
        let ctx = Arc::new(ctx);

        let response_a = tl_state(State(ctx.clone()), bearer_headers("tok-a"))
            .await
            .into_response();
        assert_eq!(
            response_a.headers().get(X_TL_ACTIVE_PROFILE).unwrap(),
            "profil-a"
        );

        let response_b = tl_state(State(ctx.clone()), bearer_headers("tok-b"))
            .await
            .into_response();
        assert_eq!(
            response_b.headers().get(X_TL_ACTIVE_PROFILE).unwrap(),
            "profil-b",
            "Gerät B darf niemals das Profil von Gerät A bekommen"
        );
    }

    /// Regressionstest für den kritischen Review-Fund am
    /// `AppState.config`/`ServerCtx.shared_config`-Umbau: **echtes**
    /// Temp-Verzeichnis, **echtes** Schreiben auf beiden Wegen — kein reiner
    /// Unit-Test einer isolierten Funktion (das täuschte beim vorherigen
    /// `commands::tests::keep_host_managed_fields_preserves_the_given_current_profiles`
    /// -Test (damals noch `save_config_keeps_live_edited_profiles`) Sicherheit
    /// vor, ohne die reale In-Memory/Platte-Divergenz abzubilden).
    ///
    /// Szenario: (a) ein TL speichert ein Panel-Profil über den Profil-Pfad
    /// (`tl::execute` → `execute_profile_action` → `ServerCtx
    /// ::mutate_app_config`) — genau der Weg, den `tl.html` beim Speichern
    /// eines Profils nimmt. (b) Direkt danach ändert der **bestehende**
    /// Tauri-Command-Schreibpfad (`commands::mutate_config_at`, die reale
    /// Kernlogik hinter `save_config`/`tl_device_add`/…) irgendeine andere
    /// Einstellung — auf demselben geteilten `Arc<Mutex<AppConfig>>`, wie
    /// es beide Wege in der echten App auch tun (`AppState.config` ==
    /// `ServerCtx.shared_config`). (c) Das Profil aus (a) muss danach
    /// sowohl auf der Platte als auch im geteilten In-Memory-Stand noch da
    /// sein — vorher (getrennte Schreibpfade: `mutate_app_config` direkt
    /// auf der Platte, `mutate_config` nur im veralteten In-Memory-Stand)
    /// hätte (b) das Profil aus (a) kommentarlos wieder gelöscht.
    #[tokio::test]
    async fn profile_save_survives_a_later_settings_save_lost_update_regression() {
        let (ctx, shared, dir) = make_tl_ctx(vec![]);
        let config_path = dir.path().join("config.json");
        let ctx = Arc::new(ctx);
        let device = crate::config::TlDevice {
            id: "dev-a".into(),
            token: "tok-a".into(),
            label: "Tablet A".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        };

        // (a) Profil-Pfad: TL legt in tl.html ein Profil an.
        let response = crate::tablet::tl::execute(
            &ctx,
            &device,
            "op-profile-save",
            1,
            0,
            relay_proto::TlAction::ProfileSave {
                profile: relay_proto::TlPanelProfileWire {
                    id: String::new(),
                    name: "Wandmonitor Halle 2".into(),
                    panels: vec![],
                    display: relay_proto::TlDisplaySettingsWire::default(),
                    updated_at_ms: 0, // wird vom Host gestempelt, s. profile_save
                    ..Default::default()
                },
            },
        )
        .await;
        assert!(response.ok, "Profil-Speichern soll gelingen: {response:?}");

        // (b) Bestehender Tauri-Command-Pfad: irgendeine andere Einstellung
        // wird gespeichert — über dieselbe Kernlogik wie `save_config`, auf
        // demselben geteilten Arc<Mutex<AppConfig>>.
        crate::commands::mutate_config_at(&config_path, &shared, |cfg| {
            cfg.badhub.url = "https://geaendert.example".to_string();
            Ok(())
        })
        .expect("Einstellungsänderung soll sich speichern lassen");

        // (c) Das Profil aus (a) ist NICHT verschwunden — weder auf der
        // Platte noch im geteilten In-Memory-Stand.
        let on_disk = AppConfig::load_from(&config_path).expect("config.json lesbar");
        assert_eq!(
            on_disk.tl_web.profiles.len(),
            1,
            "Profil darf durch die spätere Einstellungsänderung nicht verloren gehen"
        );
        assert_eq!(on_disk.tl_web.profiles[0].name, "Wandmonitor Halle 2");
        assert_eq!(
            on_disk.badhub.url, "https://geaendert.example",
            "die spätere Einstellungsänderung selbst muss ebenfalls ankommen"
        );

        let in_memory = shared.lock().expect("Config-Mutex nicht vergiftet");
        assert_eq!(
            in_memory.tl_web.profiles.len(),
            1,
            "auch der geteilte In-Memory-Stand kennt das Profil noch"
        );
    }

    /// Finding 2 (Review): Profil-Aktionen sind turnierunabhängige, reine
    /// Layout-Einstellungen — `execute()` darf sie nicht hinter dem
    /// „kein Turnier geladen"-Gate verstecken (das griff früher VOR der
    /// Profil-Verzweigung). Ein TL, der morgens vor dem ersten BTP-Import
    /// schon einen Wandmonitor einrichten will, muss das können.
    #[tokio::test]
    async fn profile_action_succeeds_without_a_loaded_tournament_snapshot() {
        let (ctx, _shared, _dir) = make_tl_ctx(vec![]);
        // Kein `ctx.tablet.set_snapshot(...)` — `snapshot_clone()` liefert
        // `None`, genau das Szenario „noch kein Turnier geladen".
        assert!(ctx.tablet.snapshot_clone().is_none());
        let ctx = Arc::new(ctx);
        let device = crate::config::TlDevice {
            id: "dev-a".into(),
            token: "tok-a".into(),
            label: "Tablet A".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        };

        let response = crate::tablet::tl::execute(
            &ctx,
            &device,
            "op-profile-no-snapshot",
            1,
            0,
            relay_proto::TlAction::ProfileSave {
                profile: relay_proto::TlPanelProfileWire {
                    id: String::new(),
                    name: "Vor dem ersten Import".into(),
                    panels: vec![],
                    display: relay_proto::TlDisplaySettingsWire::default(),
                    updated_at_ms: 0,
                    ..Default::default()
                },
            },
        )
        .await;

        assert!(
            response.ok,
            "Profil-Aktionen dürfen nicht am Snapshot-Gate scheitern: {response:?}"
        );
    }

    // ─────────────────────── Bild-Auslieferung (Cache) ──────────────────────

    /// Kontext mit **eigenem** Werbebild-Verzeichnis. `make_ctx` teilt sich
    /// `temp_dir()` mit allen anderen Tests — für Bild-Tests, die Dateien
    /// anlegen und ändern, wäre das ein Rennen.
    fn make_bild_ctx() -> (Arc<ServerCtx>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let shared_config = Arc::new(std::sync::Mutex::new(config.clone()));
        let ctx = ServerCtx::new(
            Arc::new(TabletState::default()),
            config,
            reqwest::Client::new(),
            dir.path().to_path_buf(),
            dir.path().join("config.json"),
            dir.path().join("assign.json"),
            dir.path().to_path_buf(),
            shared_config,
        );
        (Arc::new(ctx), dir)
    }

    fn if_none_match(marke: &str) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, marke.parse().unwrap());
        headers
    }

    /// Wartet, bis die Datei-Änderungszeit garantiert weitergesprungen ist.
    /// Windows aktualisiert sie nur im Takt des Systemtimers (~15,6 ms);
    /// zwei gleich große Schreibvorgänge unmittelbar hintereinander wären
    /// sonst nicht auseinanderzuhalten. In der Praxis liegen zwischen zwei
    /// Änderungen Sekunden — im Test müssen wir es erzwingen.
    fn zeit_tickt_weiter() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    #[tokio::test]
    async fn ein_unveraendertes_werbebild_wird_nur_bestaetigt_statt_geschickt() {
        // Die Anzeigen wechseln ihr Bild alle paar Sekunden. Ohne Marke lud
        // jedes Gerät dabei jedes Mal die vollen Bilddaten neu.
        let (ctx, dir) = make_bild_ctx();
        std::fs::write(dir.path().join("ad-1.png"), b"bilddaten").unwrap();

        let erst = ad_image(
            State(ctx.clone()),
            Path("ad-1.png".to_string()),
            axum::http::HeaderMap::new(),
        )
        .await
        .into_response();
        assert_eq!(erst.status(), StatusCode::OK);
        let marke = erst
            .headers()
            .get(header::ETAG)
            .expect("Werbebilder brauchen eine Marke")
            .to_str()
            .unwrap()
            .to_string();

        let zweit = ad_image(
            State(ctx),
            Path("ad-1.png".to_string()),
            if_none_match(&marke),
        )
        .await
        .into_response();
        assert_eq!(zweit.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn ein_ausgetauschtes_werbebild_bekommt_eine_neue_marke() {
        // Ein Turnierleiter kann eine Datei auch von Hand ins
        // `court-ads`-Verzeichnis legen und später ersetzen — dann darf die
        // alte Marke nicht mehr passen (deshalb kein `immutable`).
        let (ctx, dir) = make_bild_ctx();
        let pfad = dir.path().join("sponsor.png");
        // Bewusst GLEICH LANG ersetzt: Sonst genügte der Größenanteil der
        // Marke, und der eigentlich tragende Teil — die Änderungszeit —
        // bliebe ungeprüft (Review-Fund 18.08.2026).
        std::fs::write(&pfad, b"alt").unwrap();
        let erst = ad_image(
            State(ctx.clone()),
            Path("sponsor.png".to_string()),
            axum::http::HeaderMap::new(),
        )
        .await
        .into_response();
        let alte_marke = erst
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        zeit_tickt_weiter();
        std::fs::write(&pfad, b"neu").unwrap();
        let zweit = ad_image(
            State(ctx),
            Path("sponsor.png".to_string()),
            if_none_match(&alte_marke),
        )
        .await
        .into_response();
        assert_eq!(
            zweit.status(),
            StatusCode::OK,
            "Ein geändertes Bild muss neu ausgeliefert werden"
        );
    }

    #[test]
    fn der_gemerkte_leisten_marker_folgt_der_datei() {
        // Der Zwischenstand darf eine Änderung nicht verschlucken: Wer im
        // Setup ein Häkchen setzt, will es sofort in der Leiste sehen.
        let (ctx, dir) = make_bild_ctx();
        let pfad = dir.path().join(monitor::AD_BAR_FILE);
        std::fs::write(&pfad, br#"["ad-1.png"]"#).unwrap();
        assert!(ctx.ad_bar().contains("ad-1.png"));

        // Umhaken statt Hinzufügen: Die Datei bleibt **exakt gleich groß**,
        // also trägt allein die Änderungszeit. Der realistische Fall — und
        // der einzige, der den Zwischenstand wirklich prüft.
        zeit_tickt_weiter();
        std::fs::write(&pfad, br#"["ad-2.png"]"#).unwrap();
        let leiste = ctx.ad_bar();
        assert!(
            leiste.contains("ad-2.png") && !leiste.contains("ad-1.png"),
            "Ein umgehaktes Bild muss sofort in der Leiste stehen, das alte weg"
        );
    }

    #[tokio::test]
    async fn ein_ausgetauschtes_turnierlogo_bekommt_eine_neue_marke() {
        // Die Marke hing früher an (Base64-Länge, MIME) und war zugleich
        // der Schlüssel des Dekodier-Zwischenstands: Zwei verschiedene
        // Logos gleicher Länge und gleichen Typs hätten auch frischen
        // Anzeigen dauerhaft das alte Bild geliefert.
        let (ctx, _dir) = make_bild_ctx();
        let setze = |daten: &'static str| {
            ctx.mutate_app_config(|c| {
                c.tournament_logo.data = daten.into();
                c.tournament_logo.mime = "image/png".into();
                Ok(())
            })
            .unwrap();
        };
        setze("aGFsbG8="); // "hallo"
        let erst = info_tournament_logo(State(ctx.clone()), axum::http::HeaderMap::new())
            .await
            .into_response();
        let alte_marke = erst
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        setze("d2VsdGU="); // "welte" — gleich lang, anderer Inhalt
        let zweit = info_tournament_logo(State(ctx.clone()), if_none_match(&alte_marke))
            .await
            .into_response();
        assert_eq!(
            zweit.status(),
            StatusCode::OK,
            "Ein gleich langes, aber anderes Logo muss neu ausgeliefert werden"
        );
        let bytes = axum::body::to_bytes(zweit.into_body(), 4096).await.unwrap();
        assert_eq!(
            &bytes[..],
            b"welte",
            "Der Dekodier-Zwischenstand darf nicht das alte Logo festhalten"
        );
    }

    #[tokio::test]
    async fn eine_marken_liste_und_eine_geschwaechte_marke_werden_erkannt() {
        // RFC 9110: `If-None-Match` darf eine Liste sein, `*` enthalten und
        // schwache Marken (`W/"…"`) tragen. Ein Zwischenspeicher auf dem Weg
        // darf eine Marke abschwächen — ein reiner Gleichheitstest wäre dann
        // still wirkungslos (Review-Fund 18.08.2026).
        let etag = "\"img-5-abc\"";
        assert!(marke_bekannt(&if_none_match(etag), etag));
        assert!(marke_bekannt(
            &if_none_match(&format!("\"img-9-xyz\", {etag}")),
            etag
        ));
        assert!(marke_bekannt(&if_none_match(&format!("W/{etag}")), etag));
        assert!(marke_bekannt(&if_none_match("*"), etag));
        assert!(!marke_bekannt(&if_none_match("\"img-9-xyz\""), etag));
        assert!(!marke_bekannt(&axum::http::HeaderMap::new(), etag));
    }

    #[tokio::test]
    async fn das_turnierlogo_wird_beim_zweiten_abruf_nur_bestaetigt() {
        // Das Logo hängt in der Kopfleiste JEDER Anzeigeseite und wurde
        // bisher bei jedem Neuaufbau vollständig neu übertragen.
        let (ctx, _dir) = make_bild_ctx();
        ctx.mutate_app_config(|c| {
            c.tournament_logo.data = "aGFsbG8=".into(); // "hallo"
            c.tournament_logo.mime = "image/png".into();
            Ok(())
        })
        .unwrap();

        let erst = info_tournament_logo(State(ctx.clone()), axum::http::HeaderMap::new())
            .await
            .into_response();
        assert_eq!(erst.status(), StatusCode::OK);
        let marke = erst
            .headers()
            .get(header::ETAG)
            .expect("Das Logo braucht eine Marke")
            .to_str()
            .unwrap()
            .to_string();

        let zweit = info_tournament_logo(State(ctx), if_none_match(&marke))
            .await
            .into_response();
        assert_eq!(zweit.status(), StatusCode::NOT_MODIFIED);
    }

    // ── Perf-Messung der Anzeige-Strecke (Spec monitor-livestand-push, S0) ──

    /// Query wie ihn eine Anzeige schickt.
    fn takt(src: Option<&str>) -> Query<DeviceHeartbeat> {
        Query(DeviceHeartbeat {
            device: None,
            src: src.map(|s| s.to_string()),
        })
    }

    #[tokio::test]
    async fn health_zaehlt_push_und_poll_getrennt_mit_bytes() {
        // Die Trennung ist der Kern der Vorher-Messung: Wie viel Last
        // erzeugt der Nudge-Weg, wie viel der Fallback-Takt?
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(Some("push")), axum::http::HeaderMap::new()).await;
        let _ = health(State(ctx.clone()), takt(Some("poll")), axum::http::HeaderMap::new()).await;
        // Eine Seite aus einem älteren Stand kennt `src` nicht.
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;

        let s = ctx.tablet.perf().snapshot();
        assert_eq!(s.health_push, 1);
        assert_eq!(s.health_poll, 2, "ohne `src` zählt der Abruf als Poll");
        assert!(
            s.health_push_bytes > 0 && s.health_poll_bytes > 0,
            "die Antwortgröße ist die Kennzahl, um die es geht (Bytes/s)"
        );
        assert!(
            s.health_poll_bytes > s.health_push_bytes,
            "zwei Poll-Antworten wiegen mehr als eine Push-Antwort"
        );
    }

    #[tokio::test]
    async fn court_state_zaehlt_seinen_abruf() {
        // Der feste Court-Monitor ist der billige Fall — gezählt wird er
        // trotzdem, sonst fehlte der Vergleich zur teuren Übersicht.
        let ctx = Arc::new(make_ctx(1));
        let _ = monitor_state(State(ctx.clone()), Path(101), takt(Some("push"))).await;
        let _ = monitor_state(State(ctx.clone()), Path(101), takt(None)).await;

        let s = ctx.tablet.perf().snapshot();
        assert_eq!(s.court_state_push, 1);
        assert_eq!(s.court_state_poll, 1);
        assert!(s.court_state_push_bytes > 0 && s.court_state_poll_bytes > 0);
    }

    // ── Antwortcache für /health (Spec monitor-livestand-push, S1) ─────────

    /// Körper einer Antwort als geparstes JSON.
    async fn koerper(antwort: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(antwort.into_body(), 1024 * 1024)
            .await
            .expect("Antwort lesbar");
        serde_json::from_slice(&bytes).expect("JSON")
    }

    #[tokio::test]
    async fn zwei_abrufe_ohne_aenderung_bauen_den_zustand_nur_einmal() {
        // Der Kern von S1. Gemessen mit dem Zähler aus S0 — die Messung
        // dieser Etappe belegt die nächste.
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        let nach_erstem = ctx.tablet.perf().snapshot().overview_builds;
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        let nach_zweitem = ctx.tablet.perf().snapshot().overview_builds;

        assert_eq!(nach_erstem, 1, "der erste Abruf baut");
        assert_eq!(nach_zweitem, 1, "der zweite bedient sich am Cache");
    }

    #[tokio::test]
    async fn die_cache_antwort_traegt_dieselben_felder_wie_der_direktbau() {
        // Der Cache ist Beschleuniger, nicht Wahrheit: Was er ausliefert,
        // muss Zeichen für Zeichen das sein, was der Direktbau geliefert
        // hätte. (Nur `serverNowMs` im Umschlag ist naturgemäß neu.)
        let ctx = Arc::new(make_ctx(1));
        let kalt = koerper(
            health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new())
                .await
                .into_response(),
        )
        .await;
        let warm = koerper(
            health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new())
                .await
                .into_response(),
        )
        .await;
        assert_eq!(kalt["courts"], warm["courts"]);
        assert_eq!(kalt["callTimer"], warm["callTimer"]);
        assert_eq!(kalt["ok"], warm["ok"]);
    }

    #[tokio::test]
    async fn ein_nudge_macht_den_cache_ungueltig() {
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        ctx.tablet.notify_monitor(101);
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        assert_eq!(
            ctx.tablet.perf().snapshot().overview_builds,
            2,
            "nach einem Nudge muss neu gebaut werden"
        );
    }

    #[tokio::test]
    async fn ein_neuer_btp_stand_macht_den_cache_ungueltig() {
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        ctx.tablet.set_snapshot(BtpSnapshot {
            tournament_name: "T".into(),
            rest_minutes: None,
            matches: vec![match_on_court()],
            courts: vec!["1".into()],
            locations: vec![],
            court_infos: vec![],
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        });
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        assert_eq!(ctx.tablet.perf().snapshot().overview_builds, 2);
    }

    #[tokio::test]
    async fn eine_gemeldete_config_aenderung_macht_den_cache_ungueltig() {
        // Die Hallen-Farben stecken in der Konfiguration und reisen in der
        // Feld-Liste mit — ein Cache, der das überlebt, zeigte alte Farben.
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        ctx.tablet.bump_overview_rev();
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        assert_eq!(ctx.tablet.perf().snapshot().overview_builds, 2);
    }

    #[tokio::test]
    async fn nach_der_hart_ttl_wird_trotzdem_neu_gebaut() {
        // Sicherheitsnetz gegen eine vergessene Invalidierungs-Quelle: Auch
        // ohne jedes Ereignis ist der Cache nach 250 ms abgestanden.
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        let c = ctx.tablet.overview_cache().expect("Cache steht");
        // Denselben Eintrag um 300 ms zurückdatieren — schneller und
        // verlässlicher als 300 ms zu warten.
        ctx.tablet.set_overview_cache(
            c.rev,
            c.etag.clone(),
            c.courts_json.clone(),
            c.gebaut_ms.saturating_sub(300),
        );
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;
        assert_eq!(ctx.tablet.perf().snapshot().overview_builds, 2);
    }

    #[tokio::test]
    async fn ein_unveraenderter_stand_wird_mit_304_bestaetigt() {
        let ctx = Arc::new(make_ctx(1));
        let erst = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new())
            .await
            .into_response();
        assert_eq!(erst.status(), StatusCode::OK);
        let marke = erst
            .headers()
            .get(header::ETAG)
            .expect("Antwort trägt eine Marke")
            .to_str()
            .unwrap()
            .to_string();

        let zweit = health(State(ctx.clone()), takt(None), if_none_match(&marke))
            .await
            .into_response();
        assert_eq!(zweit.status(), StatusCode::NOT_MODIFIED);

        // Nach einer Änderung gilt die alte Marke nicht mehr.
        ctx.tablet.notify_monitor(101);
        let dritt = health(State(ctx.clone()), takt(None), if_none_match(&marke))
            .await
            .into_response();
        assert_eq!(dritt.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auch_der_geraete_monitor_zaehlt_seinen_abruf() {
        // `monitor.html` im Geräte-Modus fragt `/monitor/state` statt
        // `/court/{id}/state`. Ohne diese Zählung fehlte in der Messung
        // genau die Bauform, die im Verleih-Set auf den Pis läuft.
        let ctx = Arc::new(make_ctx(1));
        let q = |src: Option<&str>| {
            Query(DeviceQuery {
                device: "pi-1".to_string(),
                src: src.map(|s| s.to_string()),
            })
        };
        let _ = monitor_device_state(State(ctx.clone()), q(Some("push"))).await;
        let _ = monitor_device_state(State(ctx.clone()), q(None)).await;

        let s = ctx.tablet.perf().snapshot();
        assert_eq!(s.court_state_push, 1);
        assert_eq!(s.court_state_poll, 1);
        assert!(s.court_state_push_bytes > 0 && s.court_state_poll_bytes > 0);
    }

    #[tokio::test]
    async fn debug_perf_liefert_die_zaehler_als_zahlen() {
        // Die Ableseroute der Vorher-Messung: Sie gibt aus, was die Zähler
        // stehen haben — und ausschließlich Zahlen (Wächter in `perf.rs`).
        let ctx = Arc::new(make_ctx(1));
        let _ = health(State(ctx.clone()), takt(Some("push")), axum::http::HeaderMap::new()).await;
        let _ = health(State(ctx.clone()), takt(None), axum::http::HeaderMap::new()).await;

        let antwort = debug_perf(State(ctx.clone())).await.into_response();
        assert_eq!(antwort.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(antwort.into_body(), 64 * 1024)
            .await
            .expect("Antwort lesbar");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");

        assert_eq!(v["health_push"], 1);
        assert_eq!(v["health_poll"], 1);
        // Seit S1 bauen zwei aufeinanderfolgende Abrufe nur EINMAL — dieser
        // Test hielt bis dahin `>= 2` fest, und dass er umgeschrieben werden
        // musste, ist der Beleg der Etappe. Die Gegenprobe dazu führt
        // `zwei_abrufe_ohne_aenderung_bauen_den_zustand_nur_einmal`.
        assert_eq!(
            v["overview_builds"].as_u64().unwrap(),
            1,
            "der zweite Abruf bedient sich am Antwortcache: {v}"
        );
        // Die Uhrzeit gehört dazu, sonst ist keine Rate zu bilden.
        assert!(v["serverNowMs"].as_u64().unwrap_or(0) > 0);
    }
}
