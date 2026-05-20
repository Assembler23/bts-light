//! Optionaler Upload der Diagnose-Logs an badhub.de.
//!
//! Nur aktiv, wenn der Nutzer es in den Einstellungen aktiviert hat. Lädt
//! periodisch die aktuelle Logdatei hoch, damit Fehler über alle
//! Installationen hinweg zentral auswertbar sind.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Endpunkt, der die Logs entgegennimmt (`public/api/bts_log.php`).
const UPLOAD_URL: &str = "https://badhub.de/api/bts_log.php";
/// Fester Bearer-Token, verbandsweit geteilt (wie das Liveticker-Token).
const UPLOAD_TOKEN: &str = "d896d5c45f1dfe72d324be2da0dcc8031e447809f9a3c1ce";
/// Abstand zwischen zwei Uploads.
const UPLOAD_INTERVAL: Duration = Duration::from_secs(600);

/// Sucht die aktuellste Logdatei (`bts-light.log.JJJJ-MM-TT`) im Verzeichnis.
/// Der Datums-Suffix sortiert lexikografisch = chronologisch.
fn current_log_file(log_dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<PathBuf> = None;
    for entry in std::fs::read_dir(log_dir).ok()?.flatten() {
        let path = entry.path();
        let is_log = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("bts-light.log"))
            .unwrap_or(false);
        if is_log && newest.as_ref().map(|p| path > *p).unwrap_or(true) {
            newest = Some(path);
        }
    }
    newest
}

/// Lädt die aktuelle Logdatei einmal hoch.
async fn upload_once(client: &reqwest::Client, log_dir: &Path, install_id: &str) {
    let Some(file) = current_log_file(log_dir) else {
        return;
    };
    let Ok(body) = std::fs::read(&file) else {
        return;
    };
    let result = client
        .post(UPLOAD_URL)
        .bearer_auth(UPLOAD_TOKEN)
        .header("X-Install-Id", install_id)
        .header("Content-Type", "text/plain")
        .body(body)
        .send()
        .await;
    match result {
        Ok(resp) if resp.status().is_success() => tracing::info!("Diagnose-Log hochgeladen"),
        Ok(resp) => tracing::warn!("Log-Upload abgelehnt: HTTP {}", resp.status()),
        Err(e) => tracing::warn!("Log-Upload fehlgeschlagen: {e}"),
    }
}

/// Endlosschleife: lädt die Logdatei alle 10 Minuten hoch. Wird nur
/// gestartet, wenn `upload_logs` aktiv ist und eine `install_id` vorliegt.
pub async fn upload_loop(client: reqwest::Client, log_dir: PathBuf, install_id: String) {
    if install_id.is_empty() {
        return;
    }
    // Kurz warten, damit beim Start schon erste Zeilen im Log stehen.
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        upload_once(&client, &log_dir, &install_id).await;
        tokio::time::sleep(UPLOAD_INTERVAL).await;
    }
}
