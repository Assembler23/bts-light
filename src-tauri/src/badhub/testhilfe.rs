//! HTTP-Attrappe für die Tests der badhub-Strecke.
//!
//! Die älteren Mini-Mocks (`push.rs`, `sync.rs`) bedienen eine Anfrage bzw.
//! lesen nur die ersten 64 KiB und können nicht hängen. Diese hier folgt
//! einem Antwortskript, zählt, liest den Körper **vollständig** und kann
//! eine Anfrage annehmen, ohne je zu antworten.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Was die Attrappe auf eine Anfrage antwortet.
#[derive(Clone)]
pub enum Antwort {
    /// Statuszeile (z. B. `"200 OK"`), zusätzliche Kopfzeilen, Körper.
    Http {
        status: &'static str,
        kopf: Vec<&'static str>,
        koerper: String,
    },
    /// Nimmt die Anfrage an und antwortet nie.
    Haengt,
}

impl Antwort {
    /// 200 mit einer echten badhub-Antwort.
    pub fn ok() -> Self {
        Self::http("200 OK", r#"{"type":"answer","status":"ok"}"#)
    }

    /// Beliebige Statuszeile mit badhubs Fehlerkörper.
    pub fn status(status: &'static str) -> Self {
        Self::http(status, r#"{"type":"error","message":"x"}"#)
    }

    pub fn http(status: &'static str, koerper: &str) -> Self {
        Self::Http {
            status,
            kopf: Vec::new(),
            koerper: koerper.to_string(),
        }
    }

    pub fn mit_kopf(mut self, zeile: &'static str) -> Self {
        if let Self::Http { kopf, .. } = &mut self {
            kopf.push(zeile);
        }
        self
    }
}

/// Eine gesehene Anfrage.
#[derive(Clone, Debug)]
pub struct Anfrage {
    /// Vollständig gelesener Körper.
    pub koerper: String,
}

/// Laufende Attrappe.
pub struct Mock {
    pub url: String,
    anfragen: Arc<Mutex<Vec<Anfrage>>>,
}

impl Mock {
    /// Alle bisher gesehenen Anfragen.
    pub fn anfragen(&self) -> Vec<Anfrage> {
        self.anfragen.lock().unwrap().clone()
    }

    pub fn anzahl(&self) -> usize {
        self.anfragen.lock().unwrap().len()
    }
}

/// Startet die Attrappe. Die n-te Anfrage bekommt die n-te Antwort des
/// Skripts; ist es zu Ende, gilt die letzte weiter.
pub async fn starten(skript: Vec<Antwort>) -> Mock {
    assert!(!skript.is_empty());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let anfragen = Arc::new(Mutex::new(Vec::new()));
    let gesehen = anfragen.clone();
    tokio::spawn(async move {
        let mut nummer = 0usize;
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let antwort = skript[nummer.min(skript.len() - 1)].clone();
            nummer += 1;
            let gesehen = gesehen.clone();
            tokio::spawn(async move {
                let koerper = lies_anfrage(&mut sock).await;
                gesehen.lock().unwrap().push(Anfrage { koerper });
                match antwort {
                    Antwort::Haengt => {
                        // Verbindung offen halten, nie antworten.
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    }
                    Antwort::Http {
                        status,
                        kopf,
                        koerper,
                    } => {
                        let mut text = format!("HTTP/1.1 {status}\r\n");
                        for zeile in kopf {
                            text.push_str(zeile);
                            text.push_str("\r\n");
                        }
                        text.push_str(&format!(
                            "Content-Length: {}\r\nConnection: close\r\n\r\n{koerper}",
                            koerper.len()
                        ));
                        let _ = sock.write_all(text.as_bytes()).await;
                    }
                }
            });
        }
    });
    Mock {
        url: format!("http://{addr}/api/live_update.php"),
        anfragen,
    }
}

/// Liest Kopf und — nach `Content-Length` — den ganzen Körper.
async fn lies_anfrage(sock: &mut tokio::net::TcpStream) -> String {
    let mut roh = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let kopf_ende = loop {
        let n = sock.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            return String::new();
        }
        roh.extend_from_slice(&buf[..n]);
        if let Some(i) = roh.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let kopf = String::from_utf8_lossy(&roh[..kopf_ende]).to_ascii_lowercase();
    let laenge = kopf
        .lines()
        .find_map(|z| z.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while roh.len() < kopf_ende + laenge {
        let n = sock.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        roh.extend_from_slice(&buf[..n]);
    }
    String::from_utf8_lossy(&roh[kopf_ende..]).to_string()
}
