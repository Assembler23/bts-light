//! Turnierlogo von badhub übernehmen (badhub ist Master, bts-light Ersatz).
//!
//! Seit badhub das Logo zentral am Turnier führt (Einstellungen der
//! Turnierorganisation, badhub ADR-0080 Nachtrag), fragt bts-light es über
//! `GET /api/checkin-branding/logo` ab — dieselbe Bearer-Anmeldung wie der
//! Branding-Push. Die Regel:
//!
//! - badhub meldet ein Logo aus den Turnier-Einstellungen (`quelle = turnier`):
//!   bts-light übernimmt es (nur wenn sich die Prüfsumme geändert hat) und
//!   zeigt es im Setup als „kommt aus badhub“ (lokal nicht änderbar).
//! - badhub hat keins (oder nur das, was bts-light selbst geschickt hat):
//!   bts-light behält sein eigenes Logo und schickt es wie bisher — die Info
//!   kann also auch von bts-light kommen.
//! - Hatte bts-light ein Logo von badhub übernommen und badhub hat es entfernt:
//!   bts-light entfernt es ebenfalls (Master).
//!
//! Die Entscheidung ([`entscheiden`]) ist rein und ohne Netz testbar; der Abruf
//! ([`abrufen`], [`bild_laden`]) prüft Host, Größe und Bildformat.

use serde::Deserialize;

use super::push::PushError;
use crate::config::LogoConfig;

/// Obergrenze für das Logo (badhub nimmt höchstens 1 MB an, Spielraum für alte Dateien).
pub const MAX_BYTES: usize = 2 * 1024 * 1024;

/// Antwort von `GET /api/checkin-branding/logo`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Antwort {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub logo: Option<LogoInfo>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LogoInfo {
    pub url: String,
    #[serde(default)]
    pub md5: Option<String>,
    #[serde(default)]
    pub quelle: String,
}

/// Was nach einem Abruf zu tun ist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schritt {
    /// Logo von dieser Adresse laden und übernehmen (`md5` merken).
    Uebernehmen { url: String, md5: String },
    /// Das früher übernommene Logo entfernen (badhub hat seins gelöscht).
    Entfernen,
    /// Nichts ändern.
    Nichts,
}

/// Leitet die Abruf-Adresse aus der Liveticker-URL ab (Nachbar von
/// `checkin-branding`, folgt damit einem abweichenden Test-Host).
pub fn abruf_url(live_url: &str, tournament_uuid: Option<&str>) -> String {
    let basis = super::push::checkin_branding_url(live_url);
    match tournament_uuid {
        Some(g) if !g.is_empty() => format!("{basis}/logo?tournament_uuid={g}"),
        _ => format!("{basis}/logo"),
    }
}

/// Reine Entscheidung aus dem lokalen Logo und der badhub-Antwort.
pub fn entscheiden(lokal: &LogoConfig, antwort: &Antwort) -> Schritt {
    match &antwort.logo {
        Some(l) if l.quelle == "turnier" => {
            // Kennung des Stands: die Prüfsumme, ohne sie die Adresse (trägt `?v=`)
            let md5 = l.md5.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| l.url.clone());
            let gleich = lokal.von_badhub && !lokal.data.is_empty() && md5 == lokal.badhub_md5;
            if gleich {
                Schritt::Nichts
            } else {
                Schritt::Uebernehmen { url: l.url.clone(), md5 }
            }
        }
        _ if lokal.von_badhub => Schritt::Entfernen,
        _ => Schritt::Nichts,
    }
}

/// Eigener Client ohne Weiterleitungen: `gleicher_host` prüft nur die Adresse,
/// die badhub nennt — ein 3xx dürfte sonst auf fremde oder interne Ziele führen
/// (Security-Review). Wie `tp_upload` und `seiten_angebot`.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Fragt badhub nach dem Logo. `Ok(None)` heißt „keine verwertbare Antwort“
/// (404/400: badhub kennt den Abruf nicht, unlesbarer Rumpf, `ok:false`) — dann
/// ändert bts-light **nichts**, auch ein übernommenes Logo bleibt.
pub async fn abrufen(client: &reqwest::Client, url: &str, password: &str) -> Result<Option<Antwort>, PushError> {
    let r = client.get(url).bearer_auth(password).send().await?;
    match r.status().as_u16() {
        // reqwest läuft ohne `json`-Feature: Text lesen, selbst parsen
        200 => Ok(serde_json::from_str::<Antwort>(&r.text().await?).ok().filter(|a| a.ok)),
        404 | 400 => Ok(None),
        401 => Err(PushError::Unauthorized),
        s => Err(PushError::Status(s)),
    }
}

/// Bildformat aus den ersten Bytes (nie dem mitgeschickten Content-Type
/// trauen): nur PNG, JPEG, WebP — dieselbe Liste wie badhub.
pub fn mime_aus_bytes(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Die Logo-Adresse muss auf demselben Host liegen wie der konfigurierte
/// Liveticker — sonst lädt bts-light nichts (die Adresse kommt vom Server, ein
/// fremder Host wäre ein Weg, bts-light beliebige Adressen abrufen zu lassen).
pub fn gleicher_host(live_url: &str, logo_url: &str) -> bool {
    fn ursprung(u: &str) -> Option<(String, String)> {
        let (schema, rest) = u.split_once("://")?;
        let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
        Some((schema.to_ascii_lowercase(), host))
    }
    match (ursprung(live_url), ursprung(logo_url)) {
        // Auch das Schema muss passen: kein Herunterstufen von https auf http
        (Some((sa, a)), Some((sb, b))) => a == b && sa == sb && (sb == "https" || sb == "http") && !b.is_empty(),
        _ => false,
    }
}

/// Lädt das Bild (Größe und Format geprüft).
pub async fn bild_laden(client: &reqwest::Client, url: &str) -> Result<(Vec<u8>, &'static str), String> {
    let mut r = client.get(url).send().await.map_err(|e| e.to_string())?;
    if r.status().as_u16() != 200 {
        return Err(format!("HTTP {}", r.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = r.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err("Logo ist größer als 2 MB".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    let mime = mime_aus_bytes(&bytes).ok_or_else(|| "kein PNG, JPEG oder WebP".to_string())?;
    Ok((bytes, mime))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lokal(data: &str, von_badhub: bool, md5: &str) -> LogoConfig {
        LogoConfig {
            data: data.into(),
            mime: "image/png".into(),
            von_badhub,
            badhub_md5: md5.into(),
            ..Default::default()
        }
    }

    fn antwort(quelle: &str, md5: Option<&str>) -> Antwort {
        Antwort {
            ok: true,
            logo: Some(LogoInfo {
                url: "https://badhub.de/assets/logos/turnier_7.png?v=1".into(),
                md5: md5.map(str::to_string),
                quelle: quelle.into(),
            }),
        }
    }

    #[test]
    fn badhub_logo_wird_uebernommen_auch_ueber_ein_eigenes() {
        let s = entscheiden(&lokal("AAAA", false, ""), &antwort("turnier", Some("abc")));
        assert_eq!(s, Schritt::Uebernehmen { url: "https://badhub.de/assets/logos/turnier_7.png?v=1".into(), md5: "abc".into() });
    }

    #[test]
    fn unveraendertes_badhub_logo_nicht_neu_laden() {
        assert_eq!(entscheiden(&lokal("AAAA", true, "abc"), &antwort("turnier", Some("abc"))), Schritt::Nichts);
    }

    #[test]
    fn geaendertes_badhub_logo_neu_laden() {
        assert!(matches!(entscheiden(&lokal("AAAA", true, "abc"), &antwort("turnier", Some("def"))), Schritt::Uebernehmen { .. }));
    }

    #[test]
    fn ohne_turnierlogo_bleibt_das_eigene() {
        // badhub zeigt nur, was bts-light selbst geschickt hat (Check-In) — bts-light bleibt Quelle
        assert_eq!(entscheiden(&lokal("AAAA", false, ""), &antwort("checkin", Some("x"))), Schritt::Nichts);
        assert_eq!(entscheiden(&lokal("AAAA", false, ""), &Antwort { ok: true, logo: None }), Schritt::Nichts);
    }

    #[test]
    fn von_badhub_entfernt_wird_lokal_entfernt() {
        assert_eq!(entscheiden(&lokal("AAAA", true, "abc"), &Antwort { ok: true, logo: None }), Schritt::Entfernen);
        assert_eq!(entscheiden(&lokal("AAAA", true, "abc"), &antwort("checkin", None)), Schritt::Entfernen);
    }

    #[test]
    fn antwort_wird_gelesen() {
        let a: Antwort = serde_json::from_str(r#"{"ok":true,"logo":{"url":"https://badhub.de/a.png","pfad":"/a.png","md5":"m","quelle":"turnier"}}"#).unwrap();
        assert_eq!(a.logo.unwrap().quelle, "turnier");
        let leer: Antwort = serde_json::from_str(r#"{"ok":true,"logo":null}"#).unwrap();
        assert!(leer.logo.is_none());
    }

    #[test]
    fn abruf_url_neben_branding() {
        assert_eq!(abruf_url("https://badhub.de/api/live_update.php", None), "https://badhub.de/api/checkin-branding/logo");
        assert_eq!(
            abruf_url("https://test.badhub.de/api/live_update.php", Some("ABCD-1")),
            "https://test.badhub.de/api/checkin-branding/logo?tournament_uuid=ABCD-1"
        );
    }

    #[test]
    fn ohne_pruefsumme_zaehlt_die_adresse() {
        let a = antwort("turnier", None);
        let url = a.logo.as_ref().unwrap().url.clone();
        assert_eq!(entscheiden(&lokal("AAAA", true, &url), &a), Schritt::Nichts);
        assert!(matches!(entscheiden(&lokal("AAAA", true, "alte-adresse"), &a), Schritt::Uebernehmen { .. }));
    }

    #[test]
    fn kein_herunterstufen_auf_http() {
        assert!(!gleicher_host("https://badhub.de/api/live_update.php", "http://badhub.de/assets/logos/x.png"));
        assert!(gleicher_host("http://localhost:8080/api/live_update.php", "http://localhost:8080/assets/logos/x.png"));
    }

    #[test]
    fn nur_derselbe_host() {
        let live = "https://badhub.de/api/live_update.php";
        assert!(gleicher_host(live, "https://badhub.de/assets/logos/turnier_7.png?v=1"));
        assert!(!gleicher_host(live, "https://evil.example/x.png"));
        assert!(!gleicher_host(live, "https://badhub.de.evil.example/x.png"));
        assert!(!gleicher_host(live, "file:///etc/passwd"));
        assert!(!gleicher_host(live, "kein-url"));
    }

    /// Mini-HTTP-Server: eine Anfrage, feste Antwort.
    async fn mock(antwort: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            sock.write_all(antwort.as_bytes()).await.unwrap();
        });
        format!("http://{addr}/api/checkin-branding/logo")
    }

    #[tokio::test]
    async fn alter_server_404_aendert_nichts() {
        let url = mock("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        assert_eq!(abrufen(&build_client(), &url, "pw").await.unwrap(), None);
    }

    #[tokio::test]
    async fn unlesbare_antwort_aendert_nichts() {
        let url = mock("HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n<html>x</").await;
        assert_eq!(abrufen(&build_client(), &url, "pw").await.unwrap(), None);
    }

    #[tokio::test]
    async fn weiterleitung_wird_nicht_verfolgt() {
        let url = mock("HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/x.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        assert!(bild_laden(&build_client(), &url).await.is_err());
    }

    #[test]
    fn bildformat_aus_dem_inhalt() {
        assert_eq!(mime_aus_bytes(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some("image/png"));
        assert_eq!(mime_aus_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(mime_aus_bytes(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(mime_aus_bytes(b"<svg xmlns="), None);
    }
}
