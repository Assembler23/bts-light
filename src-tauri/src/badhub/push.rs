//! HTTP-Push der Liveticker-Updates an badhub.de.
//!
//! Sendet `tset`/`tupdate_match`-Nachrichten per HTTPS-POST an den Empfänger
//! `live_update.php` (Bearer-Authentifizierung). Siehe Badhub-Doku
//! `docs/features/liveticker_bts.md`.

use std::time::Duration;

use crate::badhub::diff::Update;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("HTTP-Anfrage an Badhub fehlgeschlagen: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Badhub lehnte die Anmeldung ab – Passwort prüfen")]
    Unauthorized,
    #[error("Badhub antwortete mit HTTP-Status {0}")]
    Status(u16),
}

/// Baut einen wiederverwendbaren HTTP-Client mit angemessenem Timeout.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("HTTP-Client-Erzeugung kann nicht fehlschlagen")
}

/// Sendet ein Update an den Badhub-Liveticker-Endpunkt.
///
/// `Update::None` wird übersprungen – es gibt nichts zu senden.
pub async fn push_update(
    client: &reqwest::Client,
    url: &str,
    password: &str,
    update: &Update,
) -> Result<(), PushError> {
    let body = match update {
        Update::Full(msg) => serde_json::to_vec(msg),
        Update::Single(msg) => serde_json::to_vec(msg),
        Update::None => return Ok(()),
    }
    .expect("tset/tupdate-Serialisierung kann nicht fehlschlagen");

    let response = client
        .post(url)
        .bearer_auth(password)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;

    match response.status().as_u16() {
        200 => Ok(()),
        401 | 403 => Err(PushError::Unauthorized),
        other => Err(PushError::Status(other)),
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

    fn sample_update() -> Update {
        Update::Single(TupdateMessage {
            kind: "tupdate_match",
            match_update: TupdateMatch {
                id: "btp_1".to_string(),
                s: vec![[5, 3]],
            },
            rid: 1,
        })
    }

    #[tokio::test]
    async fn push_succeeds_on_http_200() {
        let url = spawn_http_mock("200 OK").await;
        let result = push_update(&build_client(), &url, "pw", &sample_update()).await;
        assert!(result.is_ok(), "erwartet Ok, war {result:?}");
    }

    #[tokio::test]
    async fn push_reports_unauthorized_on_401() {
        let url = spawn_http_mock("401 Unauthorized").await;
        let result = push_update(&build_client(), &url, "falsch", &sample_update()).await;
        assert!(matches!(result, Err(PushError::Unauthorized)));
    }

    #[tokio::test]
    async fn push_none_sends_nothing() {
        // Ungültige URL – darf nicht kontaktiert werden, da nichts zu senden ist.
        let result = push_update(
            &build_client(),
            "http://127.0.0.1:1/never",
            "pw",
            &Update::None,
        )
        .await;
        assert!(result.is_ok());
    }
}
