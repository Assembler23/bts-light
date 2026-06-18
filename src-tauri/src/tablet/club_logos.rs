//! Vereinslogos vom Badhub holen und für den Sieger-Monitor bereitstellen.
//!
//! Badhub liefert pro Verband eine offene Liste
//! `GET {base}/api/v1/federations/{slug}/clubs` → `[{name, logo_url}, …]`.
//! Wir matchen den BTP-Vereinsnamen (den bts-light kennt) gegen diese Liste
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

/// Name→Logo-URL-Zuordnung eines Verbands (zwei Schlüsselebenen, s. `lookup`).
struct ClubMap {
    /// Verband-Slug, für den diese Map gilt — wechselt der Verband, ist die
    /// Map stale (sonst würden Logos des falschen Verbands gezeigt).
    fed: String,
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
fn build_map(clubs: Vec<ApiClub>, fed: &str) -> ClubMap {
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
        fed: fed.to_string(),
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

/// Basis-Origin (`https://badhub.de`) aus der Push-URL + Verband-Slug aus der
/// `live_url` (`?t=…`). `None`, wenn etwas fehlt → dann keine Logos.
fn base_and_fed(cfg: &crate::config::BadhubConfig) -> Option<(String, String)> {
    let base = reqwest::Url::parse(&cfg.url)
        .ok()
        .map(|u| u.origin().ascii_serialization())?;
    if base == "null" {
        return None;
    }
    let fed = reqwest::Url::parse(&cfg.live_url)
        .ok()?
        .query_pairs()
        .find(|(k, _)| k == "t")
        .map(|(_, v)| v.into_owned())?;
    let fed = fed.trim().to_string();
    // Slug streng begrenzen (nur a–z0–9 und „-") — er wird in die URL-Pfad
    // interpoliert; alles andere wäre eine unerwartete Route.
    if fed.is_empty() || !fed.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some((base, fed))
}

/// Stellt sicher, dass die Vereinsliste (frisch genug) geladen ist.
async fn ensure_map(http: &reqwest::Client, base: &str, fed: &str) {
    let stale = match &*club_map().read().unwrap() {
        // Anderer Verband → sofort neu laden (sonst falsche Logos).
        Some(m) if m.fed != fed => true,
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
    let url = format!("{base}/api/v1/federations/{fed}/clubs");
    let fetched = async {
        let resp = http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<Vec<ApiClub>>().await.ok()
    }
    .await;
    let map = match fetched {
        Some(list) => build_map(list, fed),
        // Fehlschlag: leere Map mit aktuellem Zeitstempel → kurzer Retry-Takt.
        None => ClubMap {
            fed: fed.to_string(),
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
    let (base, fed) = base_and_fed(cfg)?;
    ensure_map(http, &base, &fed).await;

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
        let m = build_map(
            vec![club(
                "BC  Tempelhof (Berlin)",
                Some("https://badhub.de/assets/logos/42.png"),
            )],
            "bvbb",
        );
        assert_eq!(
            lookup(&m, "bc tempelhof (berlin)").as_deref(),
            Some("https://badhub.de/assets/logos/42.png")
        );
    }

    #[test]
    fn loose_match_ignores_parenthetical_suffix() {
        let m = build_map(
            vec![club(
                "BC Tempelhof (Berlin)",
                Some("https://badhub.de/assets/logos/42.png"),
            )],
            "bvbb",
        );
        // BTP-Name ohne Ortszusatz trifft trotzdem.
        assert_eq!(
            lookup(&m, "BC Tempelhof").as_deref(),
            Some("https://badhub.de/assets/logos/42.png")
        );
    }

    #[test]
    fn clubs_without_logo_are_skipped() {
        let m = build_map(
            vec![club("SV Ohne Logo", None), club("SV Leer", Some("  "))],
            "bvbb",
        );
        assert!(lookup(&m, "SV Ohne Logo").is_none());
        assert!(lookup(&m, "SV Leer").is_none());
    }

    #[test]
    fn ambiguous_loose_key_is_not_used() {
        let m = build_map(
            vec![
                club(
                    "Post SV (Berlin)",
                    Some("https://badhub.de/assets/logos/1.png"),
                ),
                club(
                    "Post SV (Hamburg)",
                    Some("https://badhub.de/assets/logos/2.png"),
                ),
            ],
            "bvbb",
        );
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
        let m = build_map(
            vec![club("A", Some("https://badhub.de/assets/logos/1.png"))],
            "bvbb",
        );
        assert!(lookup(&m, "Völlig Anderer Verein").is_none());
    }

    #[test]
    fn base_and_fed_extracted_from_config() {
        let cfg = crate::config::BadhubConfig {
            url: "https://badhub.de/api/live_update.php".into(),
            password: String::new(),
            live_url: "https://badhub.de/live?t=bvbb".into(),
        };
        assert_eq!(
            base_and_fed(&cfg),
            Some(("https://badhub.de".to_string(), "bvbb".to_string()))
        );
    }

    #[test]
    fn base_and_fed_none_without_slug() {
        let cfg = crate::config::BadhubConfig {
            url: "https://badhub.de/api/live_update.php".into(),
            password: String::new(),
            live_url: String::new(),
        };
        assert_eq!(base_and_fed(&cfg), None);
    }

    #[test]
    fn base_and_fed_rejects_unsafe_slug() {
        let cfg = crate::config::BadhubConfig {
            url: "https://badhub.de/api/live_update.php".into(),
            password: String::new(),
            live_url: "https://badhub.de/live?t=../../../etc/passwd".into(),
        };
        assert_eq!(base_and_fed(&cfg), None);
    }
}
