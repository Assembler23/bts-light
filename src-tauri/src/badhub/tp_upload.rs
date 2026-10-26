//! TP-Datei an badhub (Spec `docs/features/tp-datei-upload.md`, L1).
//!
//! Drei reine Bausteine, die der Tauri-Command `tp_upload_now` (commands.rs)
//! unter dem Schreib-Gate (ADR 0060) zusammensetzt:
//!
//! 1. [`tp_file_check`] — Endung, Jet-Kopf, Größe; liest nur 19 Bytes und die
//!    Metadaten. Dasselbe Urteil zeigt das Setup neben dem gewählten Pfad.
//! 2. [`lese_stabil`] — zweimal lesen mit Abstand, Bytes vergleichen, bei
//!    Abweichung einmal wiederholen. BTP hält die Datei offen; Eingaben des
//!    Turnierleiters sieht niemand, dagegen steht nur dieser Vergleich (und die
//!    Plausibilitätsprüfung am Server).
//! 3. [`sende`] — `POST <badhub.url ohne letztes Segment>/tp-upload`, Bearer,
//!    gzip, drei `X-`-Header; die Antwort wird auf [`UploadAntwort`] abgebildet,
//!    [`antwort_text`] macht daraus den Dashboard-Text. 403 ist **nicht** mit
//!    401 zusammengelegt (anders als `push::abgelehnt`): hier heißt 403 „Kürzel
//!    passt nicht zum Zugang“, und der Nutzer soll die Liveticker-Adresse
//!    prüfen, nicht das Passwort.
//!
//! Vertrag der Gegenseite: badhub `docs/features/liveticker_bts.md`, Abschnitt
//! „TP-Datei-Upload `/api/tp-upload`“.
//!
//! Datenschutz (R6): Die Datei enthält Anschriften, Telefonnummern und
//! Geburtsdaten. Sie lebt nur im Speicher für die Dauer des Uploads; nichts
//! hier schreibt auf Platte oder loggt Inhalt, Header-Werte oder Passwort.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Größendeckel wie am Server (413 „entpackt > 32 MB“).
pub const TP_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Jet-3/4-Kopf jeder TP-Datei (Access-Datenbank), 19 Bytes.
pub const JET_KOPF: &[u8; 19] = b"\x00\x01\x00\x00Standard Jet DB";
/// Abstand der beiden Lesevorgänge in [`lese_stabil`].
pub const LESEABSTAND: Duration = Duration::from_secs(2);
/// Timeout des Uploads (bis 32 MB roh, ~12 % als gzip; 15 s wie beim Push reichen nicht sicher).
pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// Status-Poll nach 202: alle 5 s, höchstens 2 Minuten.
pub const STATUS_ABSTAND: Duration = Duration::from_secs(5);
pub const STATUS_MAX: Duration = Duration::from_secs(120);

/// Urteil über einen gewählten Pfad — serialisiert ans Frontend (`art`-Tag).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum TpFileInfo {
    /// `anzeige` ist der fertige Text neben dem Pfad: „Turnierdatei erkannt, 3,6 MB“.
    Erkannt { groesse: u64, anzeige: String },
    /// Falsche Endung, kein Jet-Kopf, Verzeichnis oder leere Datei.
    KeineTpDatei,
    ZuGross { groesse: u64 },
    /// Pfad existiert nicht oder ist nicht lesbar (Laufwerk/UNC getrennt).
    NichtGefunden,
}

/// Endung `.tp` (Groß/Klein egal), Datei vorhanden und lesbar, Jet-Kopf, ≤ 32 MB.
/// Liest nur die ersten 19 Bytes — die volle Lesung passiert erst im Upload
/// unter dem Schreib-Gate.
pub fn tp_file_check(pfad: &Path) -> TpFileInfo {
    let endung_ok = pfad
        .extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("tp"))
        .unwrap_or(false);
    if !endung_ok {
        return TpFileInfo::KeineTpDatei;
    }
    let meta = match std::fs::metadata(pfad) {
        Ok(m) => m,
        Err(_) => return TpFileInfo::NichtGefunden,
    };
    if !meta.is_file() {
        return TpFileInfo::KeineTpDatei;
    }
    let groesse = meta.len();
    if groesse > TP_MAX_BYTES {
        return TpFileInfo::ZuGross { groesse };
    }
    let mut kopf = [0u8; JET_KOPF.len()];
    let gelesen = std::fs::File::open(pfad).and_then(|mut f| {
        use std::io::Read;
        f.read_exact(&mut kopf)
    });
    match gelesen {
        Ok(()) if &kopf == JET_KOPF => TpFileInfo::Erkannt {
            groesse,
            anzeige: format!("Turnierdatei erkannt, {}", mb_anzeige(groesse)),
        },
        Ok(()) => TpFileInfo::KeineTpDatei,
        // Zu kurz für den Kopf (leere Datei) → keine TP-Datei; sonst nicht lesbar.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => TpFileInfo::KeineTpDatei,
        Err(_) => TpFileInfo::NichtGefunden,
    }
}

/// Größe mit einer Nachkommastelle und deutschem Komma: 3.774.873 Bytes → „3,6 MB“.
pub fn mb_anzeige(bytes: u64) -> String {
    let zehntel = (bytes as f64 / (1024.0 * 1024.0) * 10.0).round() as u64;
    format!("{},{} MB", zehntel / 10, zehntel % 10)
}

/// Gründe, warum [`lese_stabil`] keine Bytes liefert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeseFehler {
    /// Zwei Lesungen hintereinander unterschiedlich — BTP schreibt gerade.
    Unruhig,
    KeineTpDatei,
    ZuGross,
    /// Verschoben, gelöscht, Laufwerk getrennt. `basisname` nur für die Meldung.
    NichtLesbar { basisname: String },
}

/// Basisname eines Pfads für Meldungen (nie der ganze Pfad, nie Inhalt). Ohne
/// Namen (`C:\`, `/`) derselbe Ersatztext, den der Server für leer verwendet.
pub fn basisname(pfad: &Path) -> String {
    pfad.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(ohne Namen)".to_string())
}

/// Datei vollständig lesen — mit hartem Deckel **beim** Lesen (`take`), nicht
/// nur aus den Metadaten davor: zwischen Prüfung und Lesen kann die Datei
/// gewachsen oder gegen etwas anderes getauscht worden sein (TOCTOU). Mehr als
/// 32 MB + 1 landen so nie im Speicher; `is_file` wird am geöffneten Handle
/// erneut geprüft (Gerät, FIFO, Verzeichnis hinter einem Symlink).
async fn lesen(pfad: &Path) -> Result<Vec<u8>, LeseFehler> {
    let eigner = pfad.to_path_buf();
    let nicht_lesbar = || LeseFehler::NichtLesbar {
        basisname: basisname(pfad),
    };
    let gelesen = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        use std::io::Read;
        let f = std::fs::File::open(&eigner)?;
        if !f.metadata()?.is_file() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "keine Datei"));
        }
        let mut v = Vec::new();
        f.take(TP_MAX_BYTES + 1).read_to_end(&mut v)?;
        Ok(v)
    })
    .await
    .map_err(|_| nicht_lesbar())?
    .map_err(|_| nicht_lesbar())?;
    if gelesen.len() as u64 > TP_MAX_BYTES {
        return Err(LeseFehler::ZuGross);
    }
    if !gelesen.starts_with(JET_KOPF) {
        return Err(LeseFehler::KeineTpDatei);
    }
    Ok(gelesen)
}

/// Zweimal lesen mit `abstand`, Bytes vergleichen; weichen sie ab, einmal
/// wiederholen (dritte Lesung gegen die zweite). Bleibt es unruhig →
/// [`LeseFehler::Unruhig`]. Schlägt eine spätere Lesung fehl (Datei weg), ist
/// das `NichtLesbar` — nie die erste Lesung als Ergebnis, denn genau die könnte
/// ein halb geschriebener Stand sein.
pub async fn lese_stabil(pfad: &Path, abstand: Duration) -> Result<Vec<u8>, LeseFehler> {
    match tp_file_check(pfad) {
        TpFileInfo::Erkannt { .. } => {}
        TpFileInfo::KeineTpDatei => return Err(LeseFehler::KeineTpDatei),
        TpFileInfo::ZuGross { .. } => return Err(LeseFehler::ZuGross),
        TpFileInfo::NichtGefunden => {
            return Err(LeseFehler::NichtLesbar {
                basisname: basisname(pfad),
            })
        }
    }
    lese_stabil_mit(abstand, || lesen(pfad)).await
}

/// Kern von [`lese_stabil`] mit injizierbarer Lesung — so prüfen die Tests die
/// Vergleichslogik deterministisch ohne Dateisystem-Rennen.
async fn lese_stabil_mit<F, Fut>(abstand: Duration, mut lesung: F) -> Result<Vec<u8>, LeseFehler>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, LeseFehler>>,
{
    let mut vorher = lesung().await?;
    for _ in 0..2 {
        tokio::time::sleep(abstand).await;
        let jetzt = lesung().await?;
        if jetzt == vorher {
            return Ok(jetzt);
        }
        vorher = jetzt;
    }
    Err(LeseFehler::Unruhig)
}

/// `…/api/live_update.php` → `…/api/tp-upload`. Folgt `badhub.url`, also auch
/// dem Testsystem-Umschalter; eine entartete URL fällt auf das System zurück,
/// gegen das die Installation gerade läuft (wie `push::checkin_branding_url`).
pub fn tp_upload_url(badhub_url: &str) -> String {
    // Nur hinter `://` suchen: `https://badhub.de` ohne Pfad hätte sonst den
    // Schrägstrich des Schemas getroffen und `https:/tp-upload` gebaut.
    let s = badhub_url.trim().trim_end_matches('/');
    let start = s.find("://").map(|i| i + 3).unwrap_or(0);
    match s[start..].rfind('/') {
        Some(i) => format!("{}/tp-upload", &s[..start + i]),
        None => crate::badhub_host::api_url("tp-upload"),
    }
}

/// `GET <tp_upload_url>/status?uuid=<GUID>` — die GUID ist kanonisch (nur Hex
/// und `-`, einziger Lieferant ist `tournament_uuid_kanonisch`), deshalb ohne
/// Query-Kodierung; die Sonde hält das fest.
pub fn status_url(tp_upload_url: &str, guid: &str) -> String {
    debug_assert_eq!(crate::config::kanonische_guid(guid).as_deref(), Some(guid));
    format!("{tp_upload_url}/status?uuid={guid}")
}

/// `X-File-Name`: nur der Basisname, percent-kodiert nach RFC 3986 (unreserved
/// bleiben, alles andere je UTF-8-Byte `%XX`). Header-Werte müssen ASCII sein;
/// „Köpi-Cup 2026.TP“ darf den Upload nie verhindern — der Server dekodiert.
pub fn x_file_name(pfad: &Path) -> String {
    let name = basisname(pfad);
    let mut out = String::with_capacity(name.len() * 3);
    for b in name.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Antwort des Servers, auf die Fälle der Spec-Tabelle abgebildet.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum UploadAntwort {
    /// 202 — Datei im Spool, der Worker läuft; danach Status pollen.
    Angenommen { job: String },
    /// 200 `unchanged` — gleiche Bytes wie die letzte übernommene Datei.
    Unveraendert { received_at: String },
    /// 200 `rejected` — gleiche Bytes wie eine zuvor abgelehnte Datei.
    Abgelehnt { result: String, message: String },
    /// 409 — Turnier ist schon endgültig importiert.
    BereitsEndgueltig,
    /// 429 — Cooldown der GUID oder zu viele Annahmen des Zugangs.
    ZuSchnell { retry_after_s: u32 },
    /// 403 — Kürzel passt nicht zum Zugang (oder GUID gehört einem anderen).
    VerbandFalsch { kuerzel: String },
    /// 401 vom Bearer-Check.
    Unauthorized,
    /// 400, 404 oder Basic-Auth-Wand: altes badhub bzw. gesperrtes Testsystem.
    NochNichtVerfuegbar,
    /// 413/415 — die Meldung des Servers.
    Serverfehler { message: String },
    /// Alles andere (5xx, Deploy-Fenster).
    Status { code: u16 },
    /// Netzwerkfehler, Timeout.
    NichtErreichbar,
}

/// `GET …/status` — nur die Felder, die das Dashboard braucht.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct StatusAntwort {
    pub result: Option<String>,
    pub matches: Option<u32>,
    pub message: Option<String>,
}

/// JSON-Körper der Antworten (`status`, dazu je Fall `job`/`received_at`/`result`/`message`).
/// badhub liefert bei jedem Fehler `status = "error"` — daran erkennen wir, ob
/// ein 403/409 fachlich vom Controller kommt oder von einem Proxy/WAF davor.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct AntwortKoerper {
    status: String,
    job: Option<String>,
    received_at: Option<String>,
    result: Option<String>,
    message: Option<String>,
}

/// Mehr Antwortkörper als das lesen wir nie (JSON von badhub ist wenige hundert Bytes).
const MAX_ANTWORT_BYTES: usize = 64 * 1024;

/// Eigener Client mit 60 s — der Liveticker-Push nimmt 15 s, zu knapp für
/// 32 MB. Keine Redirects: der Endpunkt leitet nie um, und ein 30x soll nicht
/// die PII-Datei an einen anderen Host weitertragen.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(UPLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("HTTP-Client-Erzeugung kann nicht fehlschlagen")
}

/// Antwortkörper gedeckelt lesen; mehr als `MAX_ANTWORT_BYTES` wird verworfen.
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

/// Alles, was [`sende`] außer den Bytes braucht.
pub struct UploadZiel<'a> {
    pub tp_upload_url: &'a str,
    pub password: &'a str,
    /// Kanonische GUID.
    pub guid: &'a str,
    /// Verbandskürzel aus der Liveticker-URL.
    pub kuerzel: &'a str,
    /// Fertiger `X-File-Name` ([`x_file_name`]).
    pub file_name: &'a str,
}

/// Ergebnis von [`sende`]: die Antwort plus die Zahlen für die Log-Zeile des Aufrufers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendeErgebnis {
    pub antwort: UploadAntwort,
    /// Größe des gesendeten gzip-Körpers.
    pub gzip_bytes: usize,
    /// HTTP-Status, `None` bei Netzwerkfehler.
    pub http_status: Option<u16>,
}

/// gzip + POST; gibt die abgebildete Antwort zurück. Loggt nichts — das macht
/// der Aufrufer mit Dateiname, Größen, Dauer und Statuscode.
pub async fn sende(client: &reqwest::Client, ziel: &UploadZiel<'_>, bytes: &[u8]) -> SendeErgebnis {
    debug_assert_eq!(crate::config::kanonische_guid(ziel.guid).as_deref(), Some(ziel.guid));
    // Kompression von bis zu 32 MB dauert eine halbe Sekunde — nicht auf dem
    // Tokio-Worker, sonst stocken Liveticker-Push und Relay solange.
    let roh = bytes.to_vec();
    let body = match tokio::task::spawn_blocking(move || crate::btp::wire::gzip_compress(&roh)).await {
        Ok(b) => b,
        Err(_) => {
            return SendeErgebnis {
                antwort: UploadAntwort::NichtErreichbar,
                gzip_bytes: 0,
                http_status: None,
            }
        }
    };
    let gzip_bytes = body.len();
    let antwort = client
        .post(ziel.tp_upload_url)
        .bearer_auth(ziel.password)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Encoding", "gzip")
        .header("X-Tournament-Uuid", ziel.guid)
        .header("X-Federation", ziel.kuerzel)
        .header("X-File-Name", ziel.file_name)
        .body(body)
        .send()
        .await;
    let antwort = match antwort {
        Ok(a) => a,
        Err(_) => {
            return SendeErgebnis {
                antwort: UploadAntwort::NichtErreichbar,
                gzip_bytes,
                http_status: None,
            }
        }
    };
    let http_status = Some(antwort.status().as_u16());
    SendeErgebnis {
        antwort: abbilden(antwort, ziel.kuerzel).await,
        gzip_bytes,
        http_status,
    }
}

/// Statuszeile + Kopfzeilen + JSON → [`UploadAntwort`].
async fn abbilden(antwort: reqwest::Response, kuerzel: &str) -> UploadAntwort {
    let code = antwort.status().as_u16();
    let basic_wand = antwort
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
        .unwrap_or(false);
    let retry_after = antwort
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u32>().ok());
    let koerper: AntwortKoerper =
        serde_json::from_slice(&koerper_begrenzt(antwort).await).unwrap_or_default();
    let von_badhub = koerper.status == "error";

    match code {
        202 => UploadAntwort::Angenommen {
            job: koerper.job.unwrap_or_default(),
        },
        200 if koerper.status == "unchanged" => UploadAntwort::Unveraendert {
            received_at: koerper.received_at.unwrap_or_default(),
        },
        200 if koerper.status == "rejected" => UploadAntwort::Abgelehnt {
            result: koerper.result.unwrap_or_default(),
            message: meldung_bereinigt(&koerper.message.unwrap_or_default()),
        },
        // 403/409 nur als Fachantwort, wenn der Körper von badhub stammt — ein
        // 403 der WAF davor schickte den Nutzer sonst zur Liveticker-Adresse.
        409 if von_badhub => UploadAntwort::BereitsEndgueltig,
        403 if von_badhub => UploadAntwort::VerbandFalsch {
            kuerzel: kuerzel.to_string(),
        },
        429 => UploadAntwort::ZuSchnell {
            retry_after_s: retry_after.unwrap_or(60),
        },
        401 if basic_wand => UploadAntwort::NochNichtVerfuegbar,
        401 => UploadAntwort::Unauthorized,
        400 | 404 => UploadAntwort::NochNichtVerfuegbar,
        413 | 415 => UploadAntwort::Serverfehler {
            message: koerper
                .message
                .map(|m| meldung_bereinigt(&m))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| format!("badhub antwortete mit HTTP-Status {code}")),
        },
        other => UploadAntwort::Status { code: other },
    }
}

/// Server-Meldungen sind Fremddaten, die ins Dashboard **und** in die Log-Zeile
/// gehen: Steuerzeichen raus (keine Log-Injection per Zeilenumbruch), auf 200
/// Zeichen gekürzt.
pub fn meldung_bereinigt(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// `GET …/status` nach einem 202. `None` bei Netzfehler, 404 oder unlesbarem
/// JSON — der Aufrufer pollt dann weiter bis zur Frist.
pub async fn status_abfragen(
    client: &reqwest::Client,
    status_url: &str,
    password: &str,
) -> Option<StatusAntwort> {
    let antwort = client
        .get(status_url)
        .bearer_auth(password)
        .send()
        .await
        .ok()?;
    if antwort.status().as_u16() != 200 {
        return None;
    }
    serde_json::from_slice(&koerper_begrenzt(antwort).await).ok()
}

/// Dashboard-Text je Antwort (Spec-Tabelle „Antwort → Status“, AK 8).
pub fn antwort_text(a: &UploadAntwort) -> String {
    match a {
        UploadAntwort::Angenommen { .. } => "angenommen, wird verarbeitet".to_string(),
        UploadAntwort::Unveraendert { received_at } => {
            format!("unverändert seit {}", uhrzeit_lokal(received_at))
        }
        UploadAntwort::Abgelehnt { message, result } => {
            if message.is_empty() {
                format!("abgelehnt ({result})")
            } else {
                message.clone()
            }
        }
        UploadAntwort::BereitsEndgueltig => "Turnier bereits endgültig importiert".to_string(),
        UploadAntwort::ZuSchnell { retry_after_s } => {
            format!("zu schnell hintereinander, in {retry_after_s} s wieder möglich")
        }
        UploadAntwort::VerbandFalsch { kuerzel } => format!(
            "Verbandskürzel ‹{kuerzel}› passt nicht zum badhub-Zugang — Liveticker-Adresse in „1 · Liveticker-Ziel“ prüfen"
        ),
        UploadAntwort::Unauthorized => crate::badhub::push::PushError::Unauthorized.to_string(),
        UploadAntwort::NochNichtVerfuegbar => "badhub kennt den Upload noch nicht".to_string(),
        UploadAntwort::Serverfehler { message } => message.clone(),
        UploadAntwort::Status { code } => format!("badhub antwortete mit HTTP-Status {code}"),
        UploadAntwort::NichtErreichbar => "badhub nicht erreichbar".to_string(),
    }
}

/// Text nach einem fertigen Status-Poll: `ok` → „übernommen, N Spiele“, sonst
/// die Meldung des Servers (oder der `result`-Code, falls sie fehlt).
pub fn ergebnis_text(status: &StatusAntwort) -> String {
    match status.result.as_deref() {
        Some("ok") => match status.matches {
            Some(1) => "übernommen, 1 Spiel".to_string(),
            Some(n) => format!("übernommen, {n} Spiele"),
            None => "übernommen".to_string(),
        },
        Some(code) => status
            .message
            .as_deref()
            .map(meldung_bereinigt)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| format!("abgelehnt ({code})")),
        None => String::new(),
    }
}

/// Text, wenn nach [`STATUS_MAX`] kein `result` vorliegt.
pub const TEXT_VERARBEITUNG_DAUERT: &str =
    "angenommen, Verarbeitung dauert noch — Stand beim nächsten Klick";

/// `2026-10-26T18:28:01Z` (UTC vom Server) → „19:28“ in der Zeitzone des
/// Rechners; unlesbar → der Rohwert, damit nie etwas verschwindet.
pub fn uhrzeit_lokal(iso: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(iso.trim()) {
        Ok(t) => t
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string(),
        Err(_) => iso.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bts-light-tp-upload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn jet(nutzlast: &[u8]) -> Vec<u8> {
        let mut v = JET_KOPF.to_vec();
        v.extend_from_slice(nutzlast);
        v
    }

    // ── tp_file_check ──────────────────────────────────────────────────────
    #[test]
    fn tp_file_check_erkannt_klein_und_gross_geschrieben() {
        for name in ["klein.tp", "GROSS.TP", "Köpi-Cup 2026.Tp"] {
            let p = tmp(name);
            std::fs::write(&p, jet(&[0u8; 100])).unwrap();
            assert_eq!(
                tp_file_check(&p),
                TpFileInfo::Erkannt {
                    groesse: 119,
                    anzeige: "Turnierdatei erkannt, 0,0 MB".to_string()
                },
                "{name}"
            );
        }
    }

    #[test]
    fn tp_file_check_falsche_endung_kopf_leer_verzeichnis_fehlt() {
        let txt = tmp("daten.txt");
        std::fs::write(&txt, jet(b"x")).unwrap();
        assert_eq!(tp_file_check(&txt), TpFileInfo::KeineTpDatei, "Endung");
        let kopf = tmp("kopf.tp");
        std::fs::write(&kopf, b"PK\x03\x04 kein jet, aber lang genug fuer den kopf").unwrap();
        assert_eq!(tp_file_check(&kopf), TpFileInfo::KeineTpDatei, "Kopf");
        let leer = tmp("leer.tp");
        std::fs::write(&leer, b"").unwrap();
        assert_eq!(tp_file_check(&leer), TpFileInfo::KeineTpDatei, "0 Byte");
        let dir = tmp("ordner.tp");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(tp_file_check(&dir), TpFileInfo::KeineTpDatei, "Verzeichnis");
        assert_eq!(tp_file_check(&tmp("gibt-es-nicht.tp")), TpFileInfo::NichtGefunden);
    }

    #[test]
    fn tp_file_check_zu_gross() {
        // Länge setzen statt 32 MB schreiben (auf NTFS trotzdem belegt → am Ende löschen).
        let p = tmp("riesig.tp");
        let f = std::fs::File::create(&p).unwrap();
        f.set_len(TP_MAX_BYTES + 1).unwrap();
        drop(f);
        assert_eq!(
            tp_file_check(&p),
            TpFileInfo::ZuGross {
                groesse: TP_MAX_BYTES + 1
            }
        );
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn lesen_deckelt_hart_beim_lesen_nicht_nur_vorab() {
        // Datei mit Jet-Kopf, die beim Lesen groesser ist als der Deckel: `lesen`
        // laedt hoechstens 32 MB + 1 und meldet ZuGross (TOCTOU-Schutz).
        let p = tmp("waechst.tp");
        let mut inhalt = jet(&[]);
        inhalt.resize((TP_MAX_BYTES + 2) as usize, 0);
        std::fs::write(&p, &inhalt).unwrap();
        assert_eq!(lesen(&p).await, Err(LeseFehler::ZuGross));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn basisname_ohne_namen_ersatztext() {
        assert_eq!(basisname(Path::new("/")), "(ohne Namen)");
        assert_eq!(basisname(Path::new("..")), "(ohne Namen)");
        assert_eq!(basisname(Path::new("x/y.tp")), "y.tp");
    }

    #[test]
    fn mb_anzeige_rundet_mit_komma() {
        assert_eq!(mb_anzeige(3_774_873), "3,6 MB");
        assert_eq!(mb_anzeige(0), "0,0 MB");
        assert_eq!(mb_anzeige(32 * 1024 * 1024), "32,0 MB");
    }

    // ── lese_stabil ────────────────────────────────────────────────────────
    /// Lesungen aus einer festen Folge — deterministisch, ohne Dateisystem-Rennen.
    fn folge(lesungen: Vec<Result<Vec<u8>, LeseFehler>>) -> impl FnMut() -> std::future::Ready<Result<Vec<u8>, LeseFehler>> {
        let rest = Arc::new(Mutex::new(lesungen.into_iter()));
        move || {
            let n = rest.lock().unwrap().next().expect("mehr Lesungen als vorgesehen");
            std::future::ready(n)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn lese_stabil_gleich_ok_nach_zwei_lesungen() {
        let a = jet(b"a");
        let r = lese_stabil_mit(LESEABSTAND, folge(vec![Ok(a.clone()), Ok(a.clone())])).await;
        assert_eq!(r, Ok(a));
    }

    #[tokio::test(start_paused = true)]
    async fn lese_stabil_einmal_anders_dann_ruhe_ok() {
        let (a, b) = (jet(b"a"), jet(b"b"));
        let r = lese_stabil_mit(LESEABSTAND, folge(vec![Ok(a), Ok(b.clone()), Ok(b.clone())])).await;
        assert_eq!(r, Ok(b), "die Wiederholung vergleicht gegen die ZWEITE Lesung");
    }

    #[tokio::test(start_paused = true)]
    async fn lese_stabil_zweimal_anders_unruhig() {
        let r = lese_stabil_mit(
            LESEABSTAND,
            folge(vec![Ok(jet(b"a")), Ok(jet(b"b")), Ok(jet(b"c"))]),
        )
        .await;
        assert_eq!(r, Err(LeseFehler::Unruhig));
    }

    #[tokio::test(start_paused = true)]
    async fn lese_stabil_datei_weg_nach_erster_lesung_nicht_lesbar() {
        let weg = LeseFehler::NichtLesbar {
            basisname: "x.tp".into(),
        };
        let r = lese_stabil_mit(LESEABSTAND, folge(vec![Ok(jet(b"a")), Err(weg.clone())])).await;
        assert_eq!(r, Err(weg), "nie die erste Lesung als Ergebnis");
    }

    #[tokio::test]
    async fn lese_stabil_echte_datei_ok_und_vorpruefung() {
        let p = tmp("stabil.tp");
        let inhalt = jet(&[7u8; 2048]);
        std::fs::write(&p, &inhalt).unwrap();
        assert_eq!(lese_stabil(&p, Duration::from_millis(10)).await, Ok(inhalt));
        assert_eq!(
            lese_stabil(&tmp("fehlt.tp"), Duration::from_millis(1)).await,
            Err(LeseFehler::NichtLesbar {
                basisname: "fehlt.tp".into()
            })
        );
        let kopf = tmp("kopf2.tp");
        std::fs::write(&kopf, b"kein jet, aber lang genug fuer den kopf").unwrap();
        assert_eq!(lese_stabil(&kopf, Duration::from_millis(1)).await, Err(LeseFehler::KeineTpDatei));
    }

    // ── URLs und Header ────────────────────────────────────────────────────
    #[test]
    fn tp_upload_url_prod_und_testsystem() {
        assert_eq!(
            tp_upload_url("https://badhub.de/api/live_update.php"),
            "https://badhub.de/api/tp-upload"
        );
        assert_eq!(
            tp_upload_url(" https://test.badhub.de/api/live_update.php "),
            "https://test.badhub.de/api/tp-upload"
        );
        assert_eq!(
            status_url("https://badhub.de/api/tp-upload", "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"),
            "https://badhub.de/api/tp-upload/status?uuid=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        assert!(tp_upload_url("kaputt").ends_with("/api/tp-upload"));
        // Ohne Pfad: nicht den Schrägstrich des Schemas treffen, sondern den Rückfall nehmen.
        assert!(tp_upload_url("https://badhub.de").ends_with("/api/tp-upload"));
        assert!(!tp_upload_url("https://badhub.de").starts_with("https:/tp"));
        assert!(tp_upload_url("https://badhub.de/").ends_with("/api/tp-upload"));
    }

    #[test]
    fn x_file_name_basisname_percent_kodiert() {
        // Plattformneutral gebaut: ein Backslash ist unter Linux kein Trenner (CI).
        assert_eq!(
            x_file_name(&Path::new("Turniere").join("Köpi-Cup 2026.TP")),
            "K%C3%B6pi-Cup%202026.TP"
        );
        assert_eq!(x_file_name(Path::new("/srv/tp/einfach_1.tp")), "einfach_1.tp");
        assert_eq!(x_file_name(Path::new("a&b=c.tp")), "a%26b%3Dc.tp");
    }

    // ── sende: Mock-Server ─────────────────────────────────────────────────
    /// HTTP-Mock, der die rohe Anfrage aufzeichnet und mit Statuszeile,
    /// Kopfzeilen und Körper antwortet. Der Körper wird komplett gelesen
    /// (`Content-Length`), damit auch 32-MB-Uploads nicht am Mock scheitern.
    async fn mock(
        status_line: &'static str,
        kopfzeilen: &'static str,
        body: &'static str,
    ) -> (String, Arc<Mutex<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let aufgezeichnet = Arc::new(Mutex::new(Vec::new()));
        let rec = aufgezeichnet.clone();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = sock.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let kopf = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
                    let laenge = kopf
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if buf.len() >= pos + 4 + laenge {
                        break;
                    }
                }
            }
            *rec.lock().unwrap() = buf;
            let extra = if kopfzeilen.is_empty() {
                String::new()
            } else {
                format!("{kopfzeilen}\r\n")
            };
            let response = format!(
                "HTTP/1.1 {status_line}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(response.as_bytes()).await.unwrap();
        });
        (format!("http://{addr}/api/tp-upload"), aufgezeichnet)
    }

    fn ziel<'a>(url: &'a str) -> UploadZiel<'a> {
        UploadZiel {
            tp_upload_url: url,
            password: "geheim",
            guid: "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA",
            kuerzel: "bvbb",
            file_name: "K%C3%B6pi-Cup%202026.TP",
        }
    }

    #[tokio::test]
    async fn sende_body_gzip_und_header_vollstaendig() {
        let (url, rec) = mock("202 Accepted", "", r#"{"status":"accepted","job":"abc123def456"}"#).await;
        let bytes = jet(&[1u8; 5000]);
        let a = sende(&build_client(), &ziel(&url), &bytes).await.antwort;
        assert_eq!(
            a,
            UploadAntwort::Angenommen {
                job: "abc123def456".into()
            }
        );
        let roh = rec.lock().unwrap().clone();
        let pos = roh.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let kopf = String::from_utf8_lossy(&roh[..pos]).to_string();
        let body = &roh[pos + 4..];
        assert!(kopf.starts_with("POST /api/tp-upload HTTP/1.1"), "{kopf}");
        for h in [
            "authorization: Bearer geheim",
            "content-type: application/octet-stream",
            "content-encoding: gzip",
            "x-tournament-uuid: 0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA",
            "x-federation: bvbb",
            "x-file-name: K%C3%B6pi-Cup%202026.TP",
        ] {
            assert!(
                kopf.to_ascii_lowercase().contains(&h.to_ascii_lowercase()),
                "Header fehlt: {h}\n{kopf}"
            );
        }
        assert_eq!(&body[..2], &[0x1f, 0x8b], "Body ist gzip");
        assert!(body.len() < bytes.len(), "komprimiert");
        assert!(!kopf.contains("Standard Jet"), "kein Dateiinhalt im Kopf");
    }

    #[tokio::test]
    async fn sende_mapping_aller_statuscodes() {
        let faelle: Vec<(&str, &str, &str, UploadAntwort)> = vec![
            ("200 OK", "", r#"{"status":"unchanged","received_at":"2026-10-26T08:00:00Z"}"#,
             UploadAntwort::Unveraendert { received_at: "2026-10-26T08:00:00Z".into() }),
            ("200 OK", "", r#"{"status":"rejected","result":"guid_mismatch","message":"GUID passt nicht"}"#,
             UploadAntwort::Abgelehnt { result: "guid_mismatch".into(), message: "GUID passt nicht".into() }),
            // Fremddaten: Steuerzeichen fliegen raus (Log-Injection), Rest bleibt.
            ("200 OK", "", "{\"status\":\"rejected\",\"result\":\"x\",\"message\":\"a\\nb\\u0007c\"}",
             UploadAntwort::Abgelehnt { result: "x".into(), message: "abc".into() }),
            ("409 Conflict", "", r#"{"status":"error","message":"Turnier bereits endgültig importiert"}"#,
             UploadAntwort::BereitsEndgueltig),
            ("429 Too Many Requests", "Retry-After: 37", r#"{"status":"error","message":"zu schnell"}"#,
             UploadAntwort::ZuSchnell { retry_after_s: 37 }),
            ("429 Too Many Requests", "", r#"{"status":"error"}"#,
             UploadAntwort::ZuSchnell { retry_after_s: 60 }),
            ("403 Forbidden", "", r#"{"status":"error","message":"Verband"}"#,
             UploadAntwort::VerbandFalsch { kuerzel: "bvbb".into() }),
            // 403/409 ohne badhub-JSON (WAF, Proxy) sind KEINE Fachantworten.
            ("403 Forbidden", "", "<html>cloudflare</html>", UploadAntwort::Status { code: 403 }),
            ("409 Conflict", "", "", UploadAntwort::Status { code: 409 }),
            ("401 Unauthorized", "", r#"{"status":"error","message":"Incorrect password"}"#,
             UploadAntwort::Unauthorized),
            ("401 Unauthorized", "WWW-Authenticate: Bearer realm=\"badhub\"", "",
             UploadAntwort::Unauthorized),
            ("200 OK", "", r#"{"status":"ok"}"#, UploadAntwort::Status { code: 200 }),
            ("302 Found", "Location: https://anderswo.example/", "", UploadAntwort::Status { code: 302 }),
            ("401 Unauthorized", "WWW-Authenticate: Basic realm=\"Staging\"", "",
             UploadAntwort::NochNichtVerfuegbar),
            ("404 Not Found", "", "<html>nope</html>", UploadAntwort::NochNichtVerfuegbar),
            ("400 Bad Request", "", r#"{"status":"error","message":"X-Tournament-Uuid fehlt"}"#,
             UploadAntwort::NochNichtVerfuegbar),
            ("413 Payload Too Large", "", r#"{"status":"error","message":"Datei zu groß"}"#,
             UploadAntwort::Serverfehler { message: "Datei zu groß".into() }),
            ("415 Unsupported Media Type", "", r#"{"status":"error","message":"keine TP-Datei"}"#,
             UploadAntwort::Serverfehler { message: "keine TP-Datei".into() }),
            ("500 Internal Server Error", "", "", UploadAntwort::Status { code: 500 }),
            ("503 Service Unavailable", "Retry-After: 5", "", UploadAntwort::Status { code: 503 }),
        ];
        for (status, kopf, body, erwartet) in faelle {
            let (url, _) = mock(status, kopf, body).await;
            let a = sende(&build_client(), &ziel(&url), &jet(b"x")).await.antwort;
            assert_eq!(a, erwartet, "{status} / {kopf}");
        }
    }

    #[tokio::test]
    async fn sende_ohne_server_nicht_erreichbar() {
        // Port 1 hört auf Loopback nie → Verbindung wird sofort abgewiesen.
        let a = sende(&build_client(), &ziel("http://127.0.0.1:1/api/tp-upload"), &jet(b"x")).await.antwort;
        assert_eq!(a, UploadAntwort::NichtErreichbar);
    }

    #[tokio::test]
    async fn status_abfragen_liest_result_und_none_bei_404() {
        let (url, _) = mock("200 OK", "", r#"{"received_at":"x","processed_at":"y","result":"ok","matches":6,"message":null}"#).await;
        let s = status_abfragen(&build_client(), &url, "pw").await;
        assert_eq!(
            s,
            Some(StatusAntwort {
                result: Some("ok".into()),
                matches: Some(6),
                message: None
            })
        );
        let (url, _) = mock("404 Not Found", "", r#"{"status":"error","message":"kein Upload"}"#).await;
        assert_eq!(status_abfragen(&build_client(), &url, "pw").await, None);
    }

    // ── Texte ──────────────────────────────────────────────────────────────
    #[test]
    fn antwort_texte_nach_spec_tabelle() {
        assert_eq!(antwort_text(&UploadAntwort::Angenommen { job: "j".into() }), "angenommen, wird verarbeitet");
        assert_eq!(antwort_text(&UploadAntwort::BereitsEndgueltig), "Turnier bereits endgültig importiert");
        assert_eq!(
            antwort_text(&UploadAntwort::ZuSchnell { retry_after_s: 30 }),
            "zu schnell hintereinander, in 30 s wieder möglich"
        );
        let v = antwort_text(&UploadAntwort::VerbandFalsch { kuerzel: "bvbb".into() });
        assert!(v.contains("‹bvbb›") && v.contains("1 · Liveticker-Ziel") && !v.contains("Passwort"), "{v}");
        assert_eq!(antwort_text(&UploadAntwort::NochNichtVerfuegbar), "badhub kennt den Upload noch nicht");
        assert_eq!(antwort_text(&UploadAntwort::Status { code: 500 }), "badhub antwortete mit HTTP-Status 500");
        assert_eq!(antwort_text(&UploadAntwort::NichtErreichbar), "badhub nicht erreichbar");
        assert!(antwort_text(&UploadAntwort::Unauthorized).contains("Passwort"));
        assert_eq!(
            antwort_text(&UploadAntwort::Abgelehnt { result: "implausible".into(), message: "zu wenige Spiele".into() }),
            "zu wenige Spiele"
        );
        assert_eq!(
            antwort_text(&UploadAntwort::Abgelehnt { result: "implausible".into(), message: String::new() }),
            "abgelehnt (implausible)"
        );
        let u = antwort_text(&UploadAntwort::Unveraendert { received_at: "2026-10-26T08:05:00Z".into() });
        assert!(u.starts_with("unverändert seit ") && u.len() == "unverändert seit 10:05".len(), "{u}");
        assert_eq!(uhrzeit_lokal("kaputt"), "kaputt", "Rohwert, nichts verschwindet");
    }

    #[test]
    fn ergebnis_text_ok_und_meldung() {
        assert_eq!(
            ergebnis_text(&StatusAntwort { result: Some("ok".into()), matches: Some(7), message: None }),
            "übernommen, 7 Spiele"
        );
        assert_eq!(
            ergebnis_text(&StatusAntwort { result: Some("ok".into()), matches: Some(1), message: None }),
            "übernommen, 1 Spiel"
        );
        assert_eq!(
            ergebnis_text(&StatusAntwort { result: Some("guid_mismatch".into()), matches: None, message: Some("GUID …".into()) }),
            "GUID …"
        );
        assert_eq!(
            ergebnis_text(&StatusAntwort { result: Some("parse_error".into()), matches: None, message: None }),
            "abgelehnt (parse_error)"
        );
        assert_eq!(ergebnis_text(&StatusAntwort::default()), "");
    }
}
