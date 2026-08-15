//! Vereinslogos vom Badhub holen und für den Sieger-Monitor bereitstellen.
//!
//! Badhub löst Vereinsnamen SELBST auf, über
//! `GET {base}/api/v1/club-logo?name=<verein>` (derselbe Weg, den auch der
//! Cloud-Modus direkt aus dem Browser heraus nimmt) — inklusive gängiger
//! Abkürzungen („BC" für „Badminton Club") und Klammer-Zusätzen
//! („(Berlin)"). Eine frühere Version hat eine EIGENE, schwächere Zuordnung
//! gegen die volle Vereinsliste (`/api/v1/club-logos`) gepflegt
//! (Exakt-/Klammer-Normalisierung) — die traf genau solche Abkürzungen
//! NICHT (Befund 15.08.2026: „Köpenicker BC" in BTP vs. „Köpenicker
//! Badminton Club" in badhub, vom Singular-Resolver trotzdem korrekt
//! aufgelöst). Wir delegieren jetzt direkt an ihn, statt seine Logik zu
//! duplizieren — LAN- und Cloud-Modus lösen Vereinsnamen damit identisch
//! auf.
//!
//! So funktioniert das trotzdem auch auf reinen LAN-TVs ohne eigenes
//! Internet: Der Turnier-PC ruft badhub auf und liefert das Bild lokal aus,
//! die Anzeige selbst braucht kein Internet.
//!
//! Bewusst konservativ: lieber **kein** Logo als ein **falsches**. Kein
//! Treffer → der Monitor blendet das `<img>` per `onerror` weg.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Erfolgreich aufgelöstes Logo gilt 6 h; ein Fehlschlag (kein Treffer, kein
/// Internet) nur kurz, damit ein später aktives Internet bzw. ein frisch
/// hinterlegtes Logo zügig nachgezogen wird.
const CACHE_TTL_OK: Duration = Duration::from_secs(6 * 60 * 60);
const CACHE_TTL_EMPTY: Duration = Duration::from_secs(60);
/// Logos sind klein; größere Antworten lehnen wir ab (Schutz vor Fehlrouten).
const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;

type CachedImg = Option<(String, Vec<u8>)>;

struct CacheEntry {
    fetched_at: Instant,
    value: CachedImg,
}

/// Normalisierter BTP-Vereinsname → zwischengespeichertes Ergebnis. Der
/// Cache-Schlüssel dient nur Wiederholungsanfragen (derselbe Verein taucht
/// in einer Spielliste oft mehrfach auf) — die eigentliche Zuordnung macht
/// badhub bei jedem (nicht gecachten) Aufruf neu.
fn img_cache() -> &'static RwLock<HashMap<String, CacheEntry>> {
    static C: OnceLock<RwLock<HashMap<String, CacheEntry>>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Klein + Mehrfach-Leerzeichen zu einem — reicht als Cache-Schlüssel, ohne
/// badhubs eigentliche (unbekannte, serverseitige) Abgleichlogik
/// nachzubilden.
fn cache_key(name: &str) -> String {
    name.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Basis-Origin (`https://badhub.de`) aus der Push-URL. `None` bei Unsinn →
/// dann keine Logos.
fn base_url(cfg: &crate::config::BadhubConfig) -> Option<String> {
    let base = reqwest::Url::parse(&cfg.url)
        .ok()
        .map(|u| u.origin().ascii_serialization())?;
    if base == "null" {
        return None;
    }
    Some(base)
}

/// Fragt badhubs Singular-Resolver für EINEN Vereinsnamen ab und liefert das
/// Bild direkt — der Endpoint antwortet mit einem Redirect auf die
/// Bilddatei, `reqwest` folgt dem automatisch (Standard-Redirect-Policy).
async fn fetch(http: &reqwest::Client, base: &str, name: &str) -> CachedImg {
    let mut url = reqwest::Url::parse(base)
        .ok()?
        .join("/api/v1/club-logo")
        .ok()?;
    url.query_pairs_mut().append_pair("name", name);
    let resp = http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    // SSRF-Schutz: Das (ggf. per Redirect erreichte) Bild muss von derselben
    // Origin kommen wie der konfigurierte badhub-Endpoint — Origin
    // strukturell vergleichen (ein reiner Präfix-Check ließe sich mit z. B.
    // „badhub.de.evil.com" umgehen).
    if resp.url().origin().ascii_serialization() != base {
        return None;
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !ct.starts_with("image/") {
        return None;
    }
    if resp
        .content_length()
        .is_some_and(|n| n as usize > MAX_LOGO_BYTES)
    {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_LOGO_BYTES {
        return None;
    }
    Some((ct, bytes.to_vec()))
}

/// Auflösung für den Endpoint: BTP-Vereinsname → (Content-Type, Bildbytes).
/// `None` = kein Logo (kein Treffer, kein Internet, oder Verein ohne Logo).
pub async fn resolve(
    cfg: &crate::config::BadhubConfig,
    http: &reqwest::Client,
    club_name: &str,
) -> CachedImg {
    let name = club_name.trim();
    if name.is_empty() {
        return None;
    }
    let base = base_url(cfg)?;
    let key = cache_key(name);

    if let Some(entry) = img_cache().read().unwrap().get(&key) {
        let ttl = if entry.value.is_some() {
            CACHE_TTL_OK
        } else {
            CACHE_TTL_EMPTY
        };
        if entry.fetched_at.elapsed() <= ttl {
            return entry.value.clone();
        }
    }

    let value = fetch(http, &base, name).await;
    img_cache().write().unwrap().insert(
        key,
        CacheEntry {
            fetched_at: Instant::now(),
            value: value.clone(),
        },
    );
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_case_and_space_insensitive() {
        assert_eq!(cache_key("Köpenicker  BC"), cache_key("köpenicker bc"));
    }

    #[test]
    fn base_url_is_origin_of_push_url() {
        let cfg = crate::config::BadhubConfig {
            url: "https://badhub.de/api/live_update.php".into(),
            password: String::new(),
            live_url: String::new(),
        };
        // Verbandsunabhängig: kein Slug nötig, nur die Origin.
        assert_eq!(base_url(&cfg).as_deref(), Some("https://badhub.de"));
    }

    #[test]
    fn base_url_none_on_garbage() {
        let cfg = crate::config::BadhubConfig {
            url: "not a url".into(),
            password: String::new(),
            live_url: String::new(),
        };
        assert_eq!(base_url(&cfg), None);
    }
}
