//! Vereinslogos vom Badhub holen und für den Sieger-Monitor bereitstellen.
//!
//! Badhub liefert über `GET {base}/api/v1/club-logos` eine offene (key-freie),
//! **verbandsübergreifende** Liste `{ "clubs": [{name, logo_url}, …] }` — so
//! bekommen auch Teilnehmer aus anderen Landesverbänden ihr Logo. (Der frühere
//! `clubfinder` war geo-/verbandsgebunden, `federations/…/clubs` braucht einen
//! API-Key.) Wir matchen den BTP-Vereinsnamen (den bts-light kennt) gegen diese
//! und liefern das Logo über einen lokalen Endpoint aus — so funktioniert es
//! auch auf reinen LAN-TVs ohne eigenes Internet (der Turnier-PC holt das Bild,
//! die Anzeige lädt es vom lokalen Server).
//!
//! Bewusst konservativ: lieber **kein** Logo als ein **falsches**. Kein
//! Treffer → der Monitor blendet das `<img>` per `onerror` weg.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Erfolgreich geladene Vereinsliste gilt 6 h; eine leere/fehlgeschlagene wird
/// nur kurz gehalten, damit ein später aktives Internet zügig nachgezogen wird.
const MAP_TTL_OK: Duration = Duration::from_secs(6 * 60 * 60);
const MAP_TTL_EMPTY: Duration = Duration::from_secs(60);
/// Logos sind klein; größere Antworten lehnen wir ab (Schutz vor Fehlrouten).
const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;

/// Name→Logo-URL-Zuordnung (verbandsübergreifend, zwei Schlüsselebenen, s. `lookup`).
struct ClubMap {
    fetched_at: Instant,
    /// Exakt-normalisierter Name → Logo-URL.
    exact: HashMap<String, String>,
    /// „Loser" Name (ohne Klammern/Satzzeichen) → Logo-URL; `None` markiert
    /// mehrdeutige Schlüssel (zwei Vereine, gleicher loser Name) → nicht nutzen.
    loose: HashMap<String, Option<String>>,
}

impl ClubMap {
    fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.loose.is_empty()
    }
}

/// Antwort von `club-logos`: `{ "clubs": [ … ] }`.
#[derive(Deserialize)]
struct ClubLogosResp {
    #[serde(default)]
    clubs: Vec<ApiClub>,
}

#[derive(Deserialize)]
struct ApiClub {
    name: String,
    #[serde(default)]
    logo_url: Option<String>,
}

fn club_map() -> &'static RwLock<Option<ClubMap>> {
    static MAP: OnceLock<RwLock<Option<ClubMap>>> = OnceLock::new();
    MAP.get_or_init(|| RwLock::new(None))
}

/// Bild-Cache: Logo-URL → geladenes Bild (`Some`) bzw. „nicht vorhanden"
/// (`None`, negativer Cache gegen wiederholte Fehlversuche).
type CachedImg = Option<(String, Vec<u8>)>;
fn img_cache() -> &'static RwLock<HashMap<String, CachedImg>> {
    static C: OnceLock<RwLock<HashMap<String, CachedImg>>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Voll-Normalisierung: klein, Ränder weg, Mehrfach-Leerzeichen zu einem.
fn norm_full(s: &str) -> String {
    s.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lose Normalisierung: zusätzlich Klammer-Inhalte und Satzzeichen entfernen
/// (z. B. „BC Tempelhof (Berlin)" → „bc tempelhof"). Für Vereine, deren
/// BTP-Schreibweise ohne Ortszusatz daherkommt.
fn norm_loose(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth > 0 {
                    depth -= 1
                }
            }
            _ if depth > 0 => {}
            _ if c.is_alphanumeric() || c.is_whitespace() => out.push(c),
            _ => out.push(' '),
        }
    }
    norm_full(&out)
}

/// Baut die Zuordnung aus der API-Antwort. Nur Vereine **mit** Logo.
fn build_map(clubs: Vec<ApiClub>) -> ClubMap {
    let mut exact = HashMap::new();
    let mut loose: HashMap<String, Option<String>> = HashMap::new();
    for c in clubs {
        let url = match c.logo_url {
            Some(u) if !u.trim().is_empty() => u,
            _ => continue,
        };
        let full = norm_full(&c.name);
        if full.is_empty() {
            continue;
        }
        exact.entry(full).or_insert_with(|| url.clone());
        let l = norm_loose(&c.name);
        if !l.is_empty() {
            // Bei Kollision (gleicher loser Name, andere URL) → mehrdeutig.
            loose
                .entry(l)
                .and_modify(|e| {
                    if e.as_deref() != Some(url.as_str()) {
                        *e = None;
                    }
                })
                .or_insert_with(|| Some(url.clone()));
        }
    }
    ClubMap {
        fetched_at: Instant::now(),
        exact,
        loose,
    }
}

/// Sucht die Logo-URL zu einem BTP-Vereinsnamen: erst exakt, dann lose.
fn lookup(map: &ClubMap, name: &str) -> Option<String> {
    if let Some(u) = map.exact.get(&norm_full(name)) {
        return Some(u.clone());
    }
    match map.loose.get(&norm_loose(name)) {
        Some(Some(u)) => Some(u.clone()),
        _ => None,
    }
}

/// Basis-Origin (`https://badhub.de`) aus der Push-URL. `None` bei Unsinn →
/// dann keine Logos. (Verband-Slug wird nicht mehr gebraucht: `club-logos` ist
/// verbandsübergreifend.)
fn base_url(cfg: &crate::config::BadhubConfig) -> Option<String> {
    let base = reqwest::Url::parse(&cfg.url)
        .ok()
        .map(|u| u.origin().ascii_serialization())?;
    if base == "null" {
        return None;
    }
    Some(base)
}

/// Stellt sicher, dass die Vereinsliste (frisch genug) geladen ist.
async fn ensure_map(http: &reqwest::Client, base: &str) {
    let stale = match &*club_map().read().unwrap() {
        Some(m) => {
            let ttl = if m.is_empty() {
                MAP_TTL_EMPTY
            } else {
                MAP_TTL_OK
            };
            m.fetched_at.elapsed() > ttl
        }
        None => true,
    };
    if !stale {
        return;
    }
    let url = format!("{base}/api/v1/club-logos");
    let fetched = async {
        let resp = http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<ClubLogosResp>().await.ok().map(|r| r.clubs)
    }
    .await;
    let map = match fetched {
        Some(list) => build_map(list),
        // Fehlschlag: leere Map mit aktuellem Zeitstempel → kurzer Retry-Takt.
        None => ClubMap {
            fetched_at: Instant::now(),
            exact: HashMap::new(),
            loose: HashMap::new(),
        },
    };
    *club_map().write().unwrap() = Some(map);
    // Bild-Cache an die Map koppeln: bei jedem (Neu-)Laden leeren — so
    // verschwinden negative Einträge eines früheren Fehlversuchs und geänderte
    // Logos werden neu geholt (kein dauerhaft „kein Logo" nach Startfehler).
    img_cache().write().unwrap().clear();
}

/// Lädt ein Logo-Bild (mit negativem Cache). Nur `image/*`, größenbegrenzt.
async fn fetch_image(http: &reqwest::Client, url: &str) -> CachedImg {
    let resp = http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
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
    // Frühes Limit über die angekündigte Länge (spart das Puffern großer Bodies).
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
    ensure_map(http, &base).await;

    let url = {
        let guard = club_map().read().unwrap();
        lookup(guard.as_ref()?, name)?
    };
    // SSRF-Schutz: nur Bilder von der konfigurierten badhub-Origin laden —
    // Origin strukturell vergleichen (ein reiner Präfix-Check ließe sich mit
    // z. B. „badhub.de.evil.com" umgehen).
    let same_origin = reqwest::Url::parse(&url)
        .map(|u| u.origin().ascii_serialization() == base)
        .unwrap_or(false);
    if !same_origin {
        return None;
    }
    if let Some(cached) = img_cache().read().unwrap().get(&url).cloned() {
        return cached;
    }
    let fetched = fetch_image(http, &url).await;
    img_cache().write().unwrap().insert(url, fetched.clone());
    fetched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn club(name: &str, logo: Option<&str>) -> ApiClub {
        ApiClub {
            name: name.to_string(),
            logo_url: logo.map(|s| s.to_string()),
        }
    }

    #[test]
    fn exact_match_is_case_and_space_insensitive() {
        let m = build_map(vec![club(
            "BC  Tempelhof (Berlin)",
            Some("https://badhub.de/assets/logos/42.png"),
        )]);
        assert_eq!(
            lookup(&m, "bc tempelhof (berlin)").as_deref(),
            Some("https://badhub.de/assets/logos/42.png")
        );
    }

    #[test]
    fn loose_match_ignores_parenthetical_suffix() {
        let m = build_map(vec![club(
            "BC Tempelhof (Berlin)",
            Some("https://badhub.de/assets/logos/42.png"),
        )]);
        // BTP-Name ohne Ortszusatz trifft trotzdem.
        assert_eq!(
            lookup(&m, "BC Tempelhof").as_deref(),
            Some("https://badhub.de/assets/logos/42.png")
        );
    }

    #[test]
    fn clubs_without_logo_are_skipped() {
        let m = build_map(vec![
            club("SV Ohne Logo", None),
            club("SV Leer", Some("  ")),
        ]);
        assert!(lookup(&m, "SV Ohne Logo").is_none());
        assert!(lookup(&m, "SV Leer").is_none());
    }

    #[test]
    fn ambiguous_loose_key_is_not_used() {
        let m = build_map(vec![
            club(
                "Post SV (Berlin)",
                Some("https://badhub.de/assets/logos/1.png"),
            ),
            club(
                "Post SV (Hamburg)",
                Some("https://badhub.de/assets/logos/2.png"),
            ),
        ]);
        // Exakt funktioniert weiter …
        assert_eq!(
            lookup(&m, "Post SV (Berlin)").as_deref(),
            Some("https://badhub.de/assets/logos/1.png")
        );
        // … aber der lose Name „post sv" ist mehrdeutig → kein Logo.
        assert!(lookup(&m, "Post SV").is_none());
    }

    #[test]
    fn unknown_club_has_no_logo() {
        let m = build_map(vec![club(
            "A",
            Some("https://badhub.de/assets/logos/1.png"),
        )]);
        assert!(lookup(&m, "Völlig Anderer Verein").is_none());
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
