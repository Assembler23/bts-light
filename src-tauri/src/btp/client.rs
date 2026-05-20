//! Async TCP-Client für die TP-Network-Schnittstelle von BTP.
//!
//! Jeder Request ist eine eigene, kurzlebige TCP-Verbindung – BTP schließt
//! sie nach der Antwort selbst. Siehe `docs/btp_protocol.md`.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::btp::model::{self, BtpSnapshot};
use crate::btp::proto;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Verbindung zu {addr} fehlgeschlagen: {source}")]
    Connect {
        addr: String,
        source: std::io::Error,
    },
    #[error("Zeitüberschreitung beim Verbindungsaufbau zu {0}")]
    ConnectTimeout(String),
    #[error("Zeitüberschreitung beim Lesen der Antwort von BTP")]
    ReadTimeout,
    #[error("Netzwerkfehler: {0}")]
    Io(#[source] std::io::Error),
    #[error("Protokollfehler: {0}")]
    Proto(#[from] proto::ProtoError),
    #[error("Auswertungsfehler: {0}")]
    Model(#[from] model::ModelError),
}

/// Sendet eine Wire-Nachricht an BTP und liest die vollständige Antwort.
pub async fn send_request(host: &str, port: u16, request: &[u8]) -> Result<Vec<u8>, ClientError> {
    let addr = format!("{host}:{port}");

    let mut stream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr)).await {
        Err(_) => return Err(ClientError::ConnectTimeout(addr)),
        Ok(Err(source)) => return Err(ClientError::Connect { addr, source }),
        Ok(Ok(stream)) => stream,
    };

    stream.write_all(request).await.map_err(ClientError::Io)?;
    stream.flush().await.map_err(ClientError::Io)?;

    // BTP schließt die Verbindung nach der Antwort – read_to_end terminiert
    // dann von selbst. Der Timeout ist nur ein Sicherheitsnetz.
    let mut response = Vec::new();
    match tokio::time::timeout(READ_TIMEOUT, stream.read_to_end(&mut response)).await {
        Err(_) => Err(ClientError::ReadTimeout),
        Ok(Err(e)) => Err(ClientError::Io(e)),
        Ok(Ok(_)) => Ok(response),
    }
}

/// Holt den aktuellen Turnier-Stand von BTP und parst ihn zu einem
/// `BtpSnapshot`.
pub async fn fetch_snapshot(
    host: &str,
    port: u16,
    password: Option<&str>,
) -> Result<BtpSnapshot, ClientError> {
    let request = proto::tournament_info_request(password);
    let raw = send_request(host, port, &request).await?;
    let nodes = proto::decode_response(&raw)?;
    Ok(model::parse_snapshot(&nodes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// Mini-BTP-Mock: liest einen Frame (4-Byte-Header + Payload) und
    /// antwortet mit den vorgegebenen Bytes, dann schließt er die Verbindung.
    async fn spawn_mock(reply: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut header = [0u8; 4];
            sock.read_exact(&mut header).await.unwrap();
            let len = i32::from_be_bytes(header) as usize;
            let mut payload = vec![0u8; len];
            sock.read_exact(&mut payload).await.unwrap();
            sock.write_all(&reply).await.unwrap();
        });
        addr.to_string()
    }

    #[tokio::test]
    async fn send_request_reads_full_response() {
        let reply = b"BTP-ANTWORT".to_vec();
        let addr = spawn_mock(reply.clone()).await;
        let (host, port) = addr.rsplit_once(':').unwrap();

        let request = crate::btp::wire::encode_message("<a/>");
        let got = send_request(host, port.parse().unwrap(), &request)
            .await
            .unwrap();
        assert_eq!(got, reply);
    }

    #[tokio::test]
    async fn send_request_reports_connection_failure() {
        // Auf Port 1 lauscht praktisch nie ein Dienst.
        let result = send_request("127.0.0.1", 1, b"x").await;
        assert!(matches!(result, Err(ClientError::Connect { .. })));
    }
}
