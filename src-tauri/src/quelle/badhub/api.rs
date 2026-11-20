//! Lesende Aufrufe gegen die badhub-API `/api/torg/v1` (badhub-Spec,
//! Abschnitt „API“): Kopplung, Turnierpaket, Ereignisse.
//!
//! Fehlertexte nennen nur Statuscodes, nie Antwortinhalte — die Antworten
//! tragen Personennamen.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use turnierkern::TurnierEreignis;

use super::paket::Paket;

/// Höchstgröße einer Antwort (Paket eines großen Turniers mit allen
/// Ereignissen liegt weit darunter).
const MAX_ANTWORT: usize = 32 * 1024 * 1024;

/// Fehler eines Aufrufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiFehler {
    /// Keine Verbindung, Zeitüberschreitung.
    Netz(String),
    /// 401/403: Token ungültig oder widerrufen.
    Zugang,
    /// Kopplungscode abgelehnt (ungültig, abgelaufen, schon eingelöst).
    CodeAbgelehnt,
    /// Zu viele Versuche (429).
    Drossel,
    /// Anderer Status.
    Status(u16),
    /// Antwort nicht im erwarteten Format.
    Format(String),
}

impl std::fmt::Display for ApiFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiFehler::Netz(e) => write!(f, "badhub nicht erreichbar ({e})"),
            ApiFehler::Zugang => write!(
                f,
                "badhub lehnt den Zugang dieses PCs ab — Kopplung widerrufen? Bitte neu koppeln."
            ),
            ApiFehler::CodeAbgelehnt => write!(
                f,
                "Der Kopplungscode ist ungültig oder abgelaufen. Bitte in badhub einen neuen erzeugen."
            ),
            ApiFehler::Drossel => write!(f, "Zu viele Versuche — bitte kurz warten."),
            ApiFehler::Status(s) => write!(f, "badhub antwortet mit Status {s}"),
            ApiFehler::Format(e) => write!(f, "Antwort von badhub unlesbar ({e})"),
        }
    }
}

#[derive(Debug, Serialize)]
struct KoppelnAnfrage<'a> {
    code: &'a str,
    install_id: &'a str,
    name: &'a str,
}

/// Antwort auf `POST /koppeln`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KopplungAntwort {
    pub token: String,
    pub turnier: KopplungTurnier,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KopplungTurnier {
    pub id: i64,
    pub uuid: String,
    pub name: String,
}

/// Antwort auf `GET /turnier/ereignisse` — als Liste oder als Objekt mit
/// `ereignisse` (die Spec legt die Hülle nicht fest).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EreignisAntwort {
    Liste(Vec<TurnierEreignis>),
    Objekt { ereignisse: Vec<TurnierEreignis> },
}

/// Ergebnis von `GET /turnier/paket`.
#[derive(Debug, Clone, PartialEq)]
pub enum PaketAntwort {
    /// 304: Stammdaten unverändert.
    Unveraendert,
    Neu {
        paket: Box<Paket>,
        etag: Option<String>,
    },
}

fn client(zeit: Duration) -> Result<reqwest::Client, ApiFehler> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(zeit)
        // Keine Weiterleitungen: ein https→http-Sprung auf demselben Host
        // schickte das Token sonst im Klartext.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiFehler::Netz(e.to_string()))
}

fn status_fehler(status: reqwest::StatusCode) -> ApiFehler {
    match status.as_u16() {
        401 | 403 => ApiFehler::Zugang,
        429 => ApiFehler::Drossel,
        s => ApiFehler::Status(s),
    }
}

async fn koerper(antwort: reqwest::Response) -> Result<Vec<u8>, ApiFehler> {
    let bytes = antwort
        .bytes()
        .await
        .map_err(|e| ApiFehler::Netz(e.to_string()))?;
    if bytes.len() > MAX_ANTWORT {
        return Err(ApiFehler::Format("Antwort zu groß".into()));
    }
    Ok(bytes.to_vec())
}

fn netz(e: reqwest::Error) -> ApiFehler {
    // Ohne URL: die Basis ist bekannt, die Meldung soll kurz bleiben.
    ApiFehler::Netz(e.without_url().to_string())
}

/// Löst einen Kopplungscode ein.
pub async fn koppeln(
    basis: &str,
    code: &str,
    install_id: &str,
    name: &str,
) -> Result<KopplungAntwort, ApiFehler> {
    let body = serde_json::to_vec(&KoppelnAnfrage {
        code,
        install_id,
        name,
    })
    .map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .post(format!("{basis}/koppeln"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            400 | 401 | 403 | 404 | 409 | 410 | 422 => ApiFehler::CodeAbgelehnt,
            _ => status_fehler(status),
        });
    }
    let text = koerper(antwort).await?;
    serde_json::from_slice(&text).map_err(|_| ApiFehler::Format("Kopplung".into()))
}

/// Lädt das Turnierpaket; mit `etag` nur, wenn sich die Stammdaten geändert
/// haben.
pub async fn paket(
    basis: &str,
    token: &str,
    etag: Option<&str>,
) -> Result<PaketAntwort, ApiFehler> {
    let mut anfrage = client(Duration::from_secs(60))?
        .get(format!("{basis}/turnier/paket"))
        .bearer_auth(token)
        .header("Accept", "application/json");
    if let Some(e) = etag {
        anfrage = anfrage.header("If-None-Match", e);
    }
    let antwort = anfrage.send().await.map_err(netz)?;
    let status = antwort.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(PaketAntwort::Unveraendert);
    }
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let header_etag = antwort
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let text = koerper(antwort).await?;
    let paket: Paket = serde_json::from_slice(&text).map_err(|e| {
        // Nur Zeile/Spalte, kein Inhalt (Namen) ins Log.
        ApiFehler::Format(format!(
            "Turnierpaket, Zeile {} Spalte {}",
            e.line(),
            e.column()
        ))
    })?;
    let etag = header_etag.or_else(|| paket.stammdaten_etag.clone());
    Ok(PaketAntwort::Neu {
        paket: Box::new(paket),
        etag,
    })
}

/// Holt die Ereignisse mit Kern-`n > ab`.
pub async fn ereignisse(
    basis: &str,
    token: &str,
    ab: u64,
) -> Result<Vec<TurnierEreignis>, ApiFehler> {
    let antwort = client(Duration::from_secs(10))?
        .get(format!("{basis}/turnier/ereignisse?ab={ab}&max=500"))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let text = koerper(antwort).await?;
    match serde_json::from_slice::<EreignisAntwort>(&text)
        .map_err(|_| ApiFehler::Format("Ereignisse".into()))?
    {
        EreignisAntwort::Liste(l) | EreignisAntwort::Objekt { ereignisse: l } => Ok(l),
    }
}
