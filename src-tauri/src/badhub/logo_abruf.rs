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
            let md5 = l.md5.clone().unwrap_or_default();
            // Ohne Prüfsumme lässt sich „unverändert“ nicht feststellen: dann nur
            // übernehmen, wenn noch kein badhub-Logo lokal liegt.
            let gleich = lokal.von_badhub
                && !lokal.data.is_empty()
                && (md5.is_empty() || md5 == lokal.badhub_md5);
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

/// Fragt badhub nach dem Logo. 404/400 (badhub kennt den Abruf noch nicht)
/// ergibt eine leere Antwort — kein Fehler, bts-light behält dann sein Logo.
pub async fn abrufen(client: &reqwest::Client, url: &str, password: &str) -> Result<Antwort, PushError> {
    let r = client.get(url).bearer_auth(password).send().await?;
    match r.status().as_u16() {
        // reqwest läuft ohne `json`-Feature: Text lesen, selbst parsen (unbrauchbar = keine Antwort)
        200 => Ok(serde_json::from_str(&r.text().await?).unwrap_or(Antwort { ok: false, logo: None })),
        404 | 400 => Ok(Antwort { ok: false, logo: None }),
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
        (Some((_, a)), Some((s, b))) => a == b && (s == "https" || s == "http") && !b.is_empty(),
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
            background_color: String::new(),
            von_badhub,
            badhub_md5: md5.into(),
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
    fn nur_derselbe_host() {
        let live = "https://badhub.de/api/live_update.php";
        assert!(gleicher_host(live, "https://badhub.de/assets/logos/turnier_7.png?v=1"));
        assert!(!gleicher_host(live, "https://evil.example/x.png"));
        assert!(!gleicher_host(live, "https://badhub.de.evil.example/x.png"));
        assert!(!gleicher_host(live, "file:///etc/passwd"));
        assert!(!gleicher_host(live, "kein-url"));
    }

    #[test]
    fn bildformat_aus_dem_inhalt() {
        assert_eq!(mime_aus_bytes(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some("image/png"));
        assert_eq!(mime_aus_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(mime_aus_bytes(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(mime_aus_bytes(b"<svg xmlns="), None);
    }
}
