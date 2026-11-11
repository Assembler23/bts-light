//! HTTP-Push der Liveticker-Updates an badhub.de.
//!
//! Sendet `tset`/`tupdate_match`-Nachrichten per HTTPS-POST an den Empfänger
//! `live_update.php` (Bearer-Authentifizierung). Siehe Badhub-Doku
//! `docs/features/liveticker_bts.md`.

use std::time::Duration;

use crate::badhub::diff::Update;
use crate::badhub::payload::CheckinBrandingMessage;
use crate::badhub::strecke::{self, Ausgang, Kanal};

/// Gesamtfrist einer Anfrage.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Frist für den Verbindungsaufbau allein. Ohne sie wartete eine Anfrage in
/// ein „schwarzes Loch" (Pakete versickern) die volle Gesamtfrist ab.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// So viel vom Antwortkörper wird höchstens gelesen. badhubs Antworten sind
/// wenige Dutzend Bytes; alles darüber ist nicht badhub.
const MAX_ANTWORT_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("HTTP-Anfrage an Badhub fehlgeschlagen: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Badhub lehnte die Anmeldung ab – Passwort prüfen")]
    Unauthorized,
    #[error(
        "Der Server lässt die Anfrage gar nicht durch: Er verlangt eine \n         HTTP-Anmeldung (Basic Auth), bevor bts-light überhaupt antworten \n         darf. Das Liveticker-Passwort ist NICHT die Ursache. Bei \n         test.badhub.de liegt eine solche Wand vor der ganzen Seite – die \n         Maschinen-Endpunkte müssen dort erst freigegeben werden."
    )]
    BasicAuthWall,
    #[error("Badhub antwortete mit HTTP-Status {0}")]
    Status(u16),
}

/// Wertet eine abgelehnte Antwort aus – und trennt dabei zwei Dinge, die
/// sich sonst zum Verwechseln ähnlich sehen.
///
/// Ein 401 mit `WWW-Authenticate: Basic` kommt **nicht** von badhub, sondern
/// von einem vorgelagerten Webserver, der eine HTTP-Anmeldung verlangt – der
/// Bearer-Token wurde nie geprüft. Genau so steht `test.badhub.de` hinter
/// einer htpasswd-Wand (gemessen 26.08.2026: 401 + `Basic realm="…Staging"`,
/// auch mit korrektem Bearer-Header). Ohne diese Unterscheidung meldete
/// bts-light „Passwort prüfen", und man sucht stundenlang am richtigen
/// Token. Auflösen lässt sich das **nur serverseitig**: HTTP hat genau einen
/// `Authorization`-Header, und den beansprucht der Bearer-Token.
fn abgelehnt(response: &reqwest::Response) -> PushError {
    let status = response.status().as_u16();
    match status {
        401 if ist_wand(response) => PushError::BasicAuthWall,
        401 | 403 => PushError::Unauthorized,
        other => PushError::Status(other),
    }
}

/// Verlangt ein vorgelagerter Webserver eine HTTP-Anmeldung
/// (`WWW-Authenticate: Basic`)?
fn ist_wand(response: &reqwest::Response) -> bool {
    // Bewusst über die Bytes und nicht über `to_str()`: Der echte Header
    // lautet `Basic realm="test.badhub.de — Staging"` — mit einem Gedankenstrich
    // außerhalb von ASCII. `to_str()` liefert dafür `Err`, und die Erkennung
    // finge ausgerechnet am Ernstfall ins Leere.
    response
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .map(|v| {
            let anfang: Vec<u8> = v
                .as_bytes()
                .iter()
                .copied()
                .skip_while(|b| b.is_ascii_whitespace())
                .take(5)
                .collect();
            anfang.eq_ignore_ascii_case(b"basic")
        })
        .unwrap_or(false)
}

/// Baut einen wiederverwendbaren HTTP-Client mit den Fristen des Betriebs.
pub fn build_client() -> reqwest::Client {
    build_client_mit_fristen(CONNECT_TIMEOUT, REQUEST_TIMEOUT)
}

/// Wie [`build_client`], mit eigenen Fristen (Tests).
pub fn build_client_mit_fristen(verbindung: Duration, gesamt: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(verbindung)
        .timeout(gesamt)
        .build()
        .expect("HTTP-Client-Erzeugung kann nicht fehlschlagen")
}

/// Kanal und Körper eines Liveticker-Updates; `None`, wenn nichts zu senden
/// ist.
pub fn update_koerper(update: &Update) -> Option<(Kanal, Vec<u8>)> {
    match update {
        Update::Full(msg) => Some((Kanal::Tset, serde_json::to_vec(msg))),
        Update::Single(msg) => Some((Kanal::Tupdate, serde_json::to_vec(msg))),
        Update::None => None,
    }
    .map(|(k, body)| {
        (
            k,
            body.expect("tset/tupdate-Serialisierung kann nicht fehlschlagen"),
        )
    })
}

/// Sendet eine Nachricht an den Liveticker-Endpunkt und beschreibt, was
/// dabei herauskam — für alle fünf Kanäle (`tset`, `tupdate_match`,
/// `centry_list`, `sched`, `tverlauf`): gleicher Endpunkt, gleiche
/// Bearer-Anmeldung.
///
/// Bewertet wird hier nichts; das tut [`strecke::klassifiziere`]. Ein
/// Status 200 allein genügt nicht als Erfolg: Ein Captive Portal antwortet
/// mit 200 und einer Anmeldeseite — deshalb wird der Körper (begrenzt)
/// gelesen und geprüft, ob er nach badhub aussieht.
pub async fn sende(
    client: &reqwest::Client,
    url: &str,
    password: &str,
    kanal: Kanal,
    body: Vec<u8>,
) -> Ausgang {
    let bytes = body.len();
    let beginn = std::time::Instant::now();
    let antwort = client
        .post(url)
        .bearer_auth(password)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await;
    let mut ausgang = match antwort {
        // Ohne die Adresse: Sie ist ein Freitextfeld der Einstellungen und
        // könnte ein Geheimnis tragen (`?key=…`) — und diese Zeile geht mit
        // dem Diagnose-Log hinaus. Das Bearer-Passwort steht nur im Kopf und
        // nie im Fehlertext.
        Err(e) => Ausgang {
            fehler: Some(e.without_url().to_string()),
            ..Default::default()
        },
        Ok(response) => {
            let status = response.status().as_u16();
            let wand = status == 401 && ist_wand(&response);
            let retry_after = strecke::retry_after_lesen(
                response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
            );
            let koerper = koerper_begrenzt(response).await;
            Ausgang {
                status: Some(status),
                wand,
                retry_after,
                echte_antwort: strecke::ist_badhub_antwort(&koerper),
                verworfen: strecke::verworfen_lesen(&koerper),
                ..Default::default()
            }
        }
    };
    ausgang.bytes = bytes;
    ausgang.dauer = beginn.elapsed();
    // Messzeile je Sendung (Spec `badhub-push-entkoppeln`, Messschritt B):
    // nur Kanal, Status, Größe, Dauer.
    tracing::debug!(
        kanal = kanal.name(),
        status = ausgang.status.unwrap_or(0),
        bytes,
        dauer_ms = ausgang.dauer.as_millis() as u64,
        "badhub-Sendung"
    );
    ausgang
}

/// Der Weg zum Liveticker-Endpunkt, als schmale Schnittstelle: Im Betrieb
/// der HTTP-Client, in den Tests des Senders eine Attrappe ohne Netz (echtes
/// HTTP verträgt sich nicht mit angehaltener Test-Uhr — die Fristen feuerten,
/// während auf den Socket gewartet wird).
pub trait Transport: Send + Sync {
    fn sende<'a>(
        &'a self,
        url: &'a str,
        password: &'a str,
        kanal: Kanal,
        body: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Ausgang> + Send + 'a>>;
}

impl Transport for reqwest::Client {
    fn sende<'a>(
        &'a self,
        url: &'a str,
        password: &'a str,
        kanal: Kanal,
        body: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Ausgang> + Send + 'a>> {
        Box::pin(sende(self, url, password, kanal, body))
    }
}

/// Antwortkörper gedeckelt lesen; mehr als [`MAX_ANTWORT_BYTES`] wird
/// verworfen (dann ist es keine badhub-Antwort).
async fn koerper_begrenzt(mut antwort: reqwest::Response) -> Vec<u8> {
    let mut out = Vec::new();
    while let Ok(Some(chunk)) = antwort.chunk().await {
        if out.len() + chunk.len() > MAX_ANTWORT_BYTES {
            out.clear();
            break;
        }
        out.extend_from_slice(&chunk);
    }
    out
}

/// Leitet die Branding-Endpunkt-URL aus der konfigurierten Liveticker-URL ab.
///
/// Der Sponsor-Endpunkt `POST /api/checkin-branding` ist — anders als die
/// Meldeliste — **keine** per `kind` dispatchte `live_update.php`-Nachricht,
/// sondern eine eigene badhub-Route direkt **neben** `live_update.php` (beide
/// unter `/api/`). Wir ersetzen deshalb nur das letzte Pfadsegment der
/// konfigurierten URL. So folgt der Push automatisch einer abweichenden
/// Basis-URL (Test-Host), statt einen fest verdrahteten Host zu erzwingen.
pub fn checkin_branding_url(live_url: &str) -> String {
    match live_url.rfind('/') {
        Some(i) => format!("{}/checkin-branding", &live_url[..i]),
        // Entartete URL: auf das System zurückfallen, gegen das die
        // Installation gerade läuft — nicht blind auf das Produktivsystem.
        None => crate::badhub_host::api_url("checkin-branding"),
    }
}

/// Lädt die Sponsor-Bilder der Leiste an den badhub-Check-In (Phase 3).
///
/// Gleiche Bearer-Authentifizierung wie Liveticker/Meldeliste. Wie beim
/// Roster-Push bedeuten **HTTP 404/400 „badhub kennt den Endpunkt noch
/// nicht"** (bts-light kommt per Auto-Update, badhub deployt unabhängig) — der
/// Aufrufer behandelt das additiv (nur Log, kein Nutzerfehler).
pub async fn push_checkin_branding(
    client: &reqwest::Client,
    url: &str,
    password: &str,
    branding: &CheckinBrandingMessage,
) -> Result<(), PushError> {
    let body = serde_json::to_vec(branding)
        .expect("checkin-branding-Serialisierung kann nicht fehlschlagen");

    let response = client
        .post(url)
        .bearer_auth(password)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;

    match response.status().as_u16() {
        200 => Ok(()),
        _ => Err(abgelehnt(&response)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::badhub::payload::{TupdateMatch, TupdateMessage};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Mini-HTTP-Mock: nimmt eine Anfrage entgegen und antwortet mit der
    /// vorgegebenen Statuszeile.
    async fn spawn_http_mock(status_line: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let body = r#"{"type":"answer","status":"ok"}"#;
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{addr}/api/live_update.php")
    }

    /// Wie `spawn_http_mock`, nur mit einer zusätzlichen Kopfzeile — für die
    /// Basic-Auth-Wand, die sich allein am `WWW-Authenticate` erkennen lässt.
    async fn spawn_http_mock_mit_kopfzeile(
        status_line: &'static str,
        kopfzeile: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 {status_line}\r\n{kopfzeile}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            sock.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{addr}/api/live_update.php")
    }

    use crate::badhub::testhilfe::{self, Antwort};

    fn sample_update() -> Update {
        Update::Single(TupdateMessage {
            kind: "tupdate_match",
            match_update: TupdateMatch {
                id: "btp_1".to_string(),
                s: vec![[5, 3]],
            },
            rid: 1,
            tournament_uuid: None,
        })
    }

    async fn sende_probe(url: &str) -> Ausgang {
        let (kanal, body) = update_koerper(&sample_update()).unwrap();
        sende(&build_client(), url, "pw", kanal, body).await
    }

    #[tokio::test]
    async fn push_succeeds_on_http_200() {
        let url = spawn_http_mock("200 OK").await;
        let a = sende_probe(&url).await;
        assert_eq!(a.status, Some(200));
        assert!(a.echte_antwort);
        assert_eq!(
            strecke::klassifiziere(Kanal::Tupdate, &a, false),
            strecke::Klasse::Ok
        );
        assert!(a.bytes > 0);
    }

    #[tokio::test]
    async fn push_reports_unauthorized_on_401() {
        let url = spawn_http_mock("401 Unauthorized").await;
        let a = sende_probe(&url).await;
        assert_eq!(
            strecke::klassifiziere(Kanal::Tupdate, &a, false),
            strecke::Klasse::Halt(strecke::HaltGrund::Passwort)
        );
    }

    #[tokio::test]
    async fn eine_basic_auth_wand_ist_kein_falsches_passwort() {
        // `test.badhub.de` liegt komplett hinter htpasswd — auch die
        // Maschinen-Endpunkte. nginx antwortet mit 401, BEVOR PHP den
        // Bearer-Token überhaupt sieht (gemessen 26.08.2026, auch mit
        // korrektem Token). Ohne diese Unterscheidung meldet bts-light
        // „Passwort prüfen", und man sucht stundenlang am richtigen Token.
        //
        // Der Realm trägt bewusst den echten Gedankenstrich: `to_str()` gibt
        // für solche Kopfzeilen `Err`, eine Erkennung darüber finge ins Leere.
        let url = spawn_http_mock_mit_kopfzeile(
            "401 Unauthorized",
            "WWW-Authenticate: Basic realm=\"test.badhub.de — Staging\"",
        )
        .await;
        let a = sende_probe(&url).await;
        assert!(a.wand, "als Passwortfehler durchgegangen: {a:?}");
        assert_eq!(
            strecke::klassifiziere(Kanal::Tset, &a, false),
            strecke::Klasse::Halt(strecke::HaltGrund::Wand)
        );
        // Die Meldung fürs Log muss die Ursache benennen.
        let text = PushError::BasicAuthWall.to_string();
        assert!(text.contains("Basic Auth"), "unklare Meldung: {text}");
        assert!(
            text.contains("NICHT die Ursache"),
            "unklare Meldung: {text}"
        );
    }

    #[tokio::test]
    async fn ein_401_ohne_wand_bleibt_ein_passwortfehler() {
        // Gegenprobe: Ohne `WWW-Authenticate: Basic` ist der 401 die Antwort
        // von badhub selbst — dann stimmt das Liveticker-Passwort wirklich nicht.
        let url = spawn_http_mock_mit_kopfzeile("401 Unauthorized", "X-Egal: 1").await;
        let a = sende_probe(&url).await;
        assert!(!a.wand, "war: {a:?}");
    }

    #[tokio::test]
    async fn ein_bearer_realm_ist_keine_wand() {
        // `WWW-Authenticate: Bearer …` schickt badhub selbst — kein
        // vorgelagerter Webserver. Die Prüfung darf nur auf „Basic" anspringen.
        let url =
            spawn_http_mock_mit_kopfzeile("401 Unauthorized", "WWW-Authenticate: Bearer realm=x")
                .await;
        let a = sende_probe(&url).await;
        assert!(!a.wand, "war: {a:?}");
    }

    #[test]
    fn push_none_sends_nothing() {
        assert!(update_koerper(&Update::None).is_none());
    }

    // ── Spec badhub-push-entkoppeln, PR 1 ────────────────────────────────

    #[tokio::test]
    async fn haengender_server_laeuft_in_die_gesamtfrist() {
        let mock = testhilfe::starten(vec![Antwort::Haengt]).await;
        let client =
            build_client_mit_fristen(Duration::from_millis(200), Duration::from_millis(300));
        let (kanal, body) = update_koerper(&sample_update()).unwrap();
        let beginn = std::time::Instant::now();
        let a = sende(&client, &mock.url, "pw", kanal, body).await;
        assert_eq!(a.status, None);
        assert!(a.fehler.is_some());
        assert!(beginn.elapsed() < Duration::from_secs(5));
        assert_eq!(
            strecke::klassifiziere(kanal, &a, false),
            strecke::Klasse::Stoerung
        );
    }

    #[tokio::test]
    async fn captive_portal_200_html_ist_stoerung() {
        let mock = testhilfe::starten(vec![Antwort::http(
            "200 OK",
            "<html><body>Bitte im WLAN anmelden</body></html>",
        )])
        .await;
        let a = sende_probe(&mock.url).await;
        assert_eq!(a.status, Some(200));
        assert!(!a.echte_antwort);
        assert_eq!(
            strecke::klassifiziere(Kanal::Tset, &a, false),
            strecke::Klasse::Stoerung
        );
    }

    #[tokio::test]
    async fn status_429_liefert_retry_after() {
        let mock = testhilfe::starten(vec![
            Antwort::status("429 Too Many Requests").mit_kopf("Retry-After: 900")
        ])
        .await;
        let a = sende_probe(&mock.url).await;
        assert_eq!(a.retry_after, Some(900));
        assert_eq!(
            strecke::klassifiziere(Kanal::Tupdate, &a, false),
            strecke::Klasse::Gedrosselt(Duration::from_secs(900))
        );
    }

    #[tokio::test]
    async fn antwortkoerper_wird_begrenzt_gelesen() {
        // Ein riesiger „Erfolg" ist nicht badhub — und wird nicht ganz in
        // den Speicher geholt.
        let riesig = format!(r#"{{"type":"answer","x":"{}"}}"#, "a".repeat(200 * 1024));
        let mock = testhilfe::starten(vec![Antwort::http("200 OK", &riesig)]).await;
        let a = sende_probe(&mock.url).await;
        assert_eq!(a.status, Some(200));
        assert!(!a.echte_antwort);
    }

    #[tokio::test]
    async fn grosser_koerper_kommt_vollstaendig_an() {
        let mock = testhilfe::starten(vec![Antwort::ok()]).await;
        let body = vec![b'x'; 3 * 1024 * 1024];
        let a = sende(&build_client(), &mock.url, "pw", Kanal::Tset, body).await;
        assert_eq!(a.status, Some(200));
        assert_eq!(a.bytes, 3 * 1024 * 1024);
        assert_eq!(mock.anfragen()[0].koerper.len(), 3 * 1024 * 1024);
        assert_eq!(mock.anzahl(), 1);
    }

    #[test]
    fn branding_url_replaces_last_segment_of_live_url() {
        // Der Sponsor-Endpunkt sitzt NEBEN live_update.php unter /api/.
        assert_eq!(
            checkin_branding_url("https://badhub.de/api/live_update.php"),
            "https://badhub.de/api/checkin-branding"
        );
        // Abweichender Test-Host wird respektiert (kein fester Host).
        assert_eq!(
            checkin_branding_url("http://localhost:8080/api/live_update.php"),
            "http://localhost:8080/api/checkin-branding"
        );
        // Entartete URL ohne „/" → sicherer Default auf dem System, gegen
        // das die Installation läuft. Im Test steht der Prozess-Schalter auf
        // Produktiv (kein Testturnier geladen).
        assert_eq!(
            checkin_branding_url("live_update.php"),
            "https://badhub.de/api/checkin-branding"
        );
    }

    #[tokio::test]
    async fn branding_push_succeeds_on_http_200() {
        let url = spawn_http_mock("200 OK").await;
        let msg = CheckinBrandingMessage {
            sponsors: Some(vec!["QUJD".to_string()]),
            logo: None,
            tournament_uuid: None,
        };
        let result = push_checkin_branding(&build_client(), &url, "pw", &msg).await;
        assert!(result.is_ok(), "erwartet Ok, war {result:?}");
    }

    #[tokio::test]
    async fn branding_push_maps_404_to_status_for_additive_handling() {
        // 404 = badhub kennt den Endpunkt noch nicht → der Aufrufer schluckt es.
        let url = spawn_http_mock("404 Not Found").await;
        let msg = CheckinBrandingMessage {
            sponsors: Some(vec![]),
            logo: None,
            tournament_uuid: None,
        };
        let result = push_checkin_branding(&build_client(), &url, "pw", &msg).await;
        assert!(matches!(result, Err(PushError::Status(404))));
    }

    #[test]
    fn branding_message_omits_none_fields() {
        // Feld-unabhängig: ein `None`-Feld darf NICHT im JSON stehen, sonst
        // würde badhub es als „ersetzen/löschen" deuten statt „unberührt lassen".
        let sponsors_only = CheckinBrandingMessage {
            sponsors: Some(vec!["QUJD".to_string()]),
            logo: None,
            tournament_uuid: None,
        };
        let j = serde_json::to_string(&sponsors_only).unwrap();
        assert!(j.contains("sponsors"), "sponsors da: {j}");
        assert!(!j.contains("logo"), "logo NICHT da: {j}");

        let logo_only = CheckinBrandingMessage {
            sponsors: None,
            logo: Some("QUJD".to_string()),
            tournament_uuid: None,
        };
        let j = serde_json::to_string(&logo_only).unwrap();
        assert!(j.contains("logo"), "logo da: {j}");
        assert!(!j.contains("sponsors"), "sponsors NICHT da: {j}");

        // Leeres Logo (löschen) wird als "" gesendet — present, nicht weggelassen.
        let clear = CheckinBrandingMessage {
            sponsors: None,
            logo: Some(String::new()),
            tournament_uuid: None,
        };
        assert_eq!(serde_json::to_string(&clear).unwrap(), r#"{"logo":""}"#);
    }
}
