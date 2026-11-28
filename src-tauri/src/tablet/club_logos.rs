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

/// Höchstzahl der Cache-Einträge. Ein Turnier hat einige Dutzend Vereine;
/// die Grenze hält nur ein Gerät im Hallennetz davon ab, den Speicher des
/// Turnier-PCs mit erfundenen Namen zu füllen.
const CACHE_MAX: usize = 512;

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

/// Wonach badhub gefragt wird: nach dem Vereinsnamen (Turniere, der Name
/// kommt aus BTP) oder nach der Vereins-ID (Punktspiele — das Paket nennt
/// den Verein exakt, ADR 0109).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesucht<'a> {
    Name(&'a str),
    Id(u32),
}

impl Gesucht<'_> {
    /// Cache-Schlüssel mit Präfix: Ein Verein namens „17“ und die
    /// Vereins-ID 17 dürfen sich den Eintrag nicht teilen.
    fn schluessel(&self) -> String {
        match self {
            Gesucht::Name(n) => format!("n:{}", cache_key(n)),
            Gesucht::Id(id) => format!("i:{id}"),
        }
    }

    /// Hängt die Frage an die Resolver-Adresse. Die ID wird als Zahl
    /// geschrieben — aus der Anfrage gelangt kein Zeichen in die Adresse.
    fn an_adresse(&self, url: &mut reqwest::Url) {
        match self {
            Gesucht::Name(n) => url.query_pairs_mut().append_pair("name", n),
            Gesucht::Id(id) => url.query_pairs_mut().append_pair("id", &id.to_string()),
        };
    }
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

/// Darf ein Bild von `bild` (Origin) kommen, wenn `base` gefragt wurde?
/// Grundsätzlich nur von derselben Origin. **Einzige Ausnahme:** Das
/// Testsystem leitet seine Logos auf die Bilddateien des Produktivsystems
/// weiter (Befund 01.11.2026: `test.badhub.de/api/v1/club-logo` → 302 auf
/// `https://badhub.de/assets/logos/…`) — ohne die Ausnahme blieben im
/// Testmodus alle Logos leer. Beide Origins sind feste Konstanten, ein
/// beliebiges Ziel wird dadurch nicht erreichbar.
fn bild_herkunft_erlaubt(base: &str, bild: &str) -> bool {
    if bild == base {
        return true;
    }
    let test = format!("https://{}", crate::badhub_host::HOST_TEST);
    let live = format!("https://{}", crate::badhub_host::HOST_LIVE);
    base == test && bild == live
}

/// Fragt badhubs Singular-Resolver für EINEN Vereinsnamen ab und liefert das
/// Bild direkt — der Endpoint antwortet mit einem Redirect auf die
/// Bilddatei, `reqwest` folgt dem automatisch (Standard-Redirect-Policy).
async fn fetch(http: &reqwest::Client, base: &str, gesucht: Gesucht<'_>) -> CachedImg {
    let mut url = reqwest::Url::parse(base)
        .ok()?
        .join("/api/v1/club-logo")
        .ok()?;
    gesucht.an_adresse(&mut url);
    let resp = http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    // SSRF-Schutz: Das (ggf. per Redirect erreichte) Bild muss von derselben
    // Origin kommen wie der konfigurierte badhub-Endpoint — Origin
    // strukturell vergleichen (ein reiner Präfix-Check ließe sich mit z. B.
    // „badhub.de.evil.com" umgehen).
    if !bild_herkunft_erlaubt(base, &resp.url().origin().ascii_serialization()) {
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
    aufloesen(cfg, http, Gesucht::Name(name)).await
}

/// Auflösung über die Vereins-ID in badhub (Monitoransicht „Begegnung“,
/// ADR 0109). `None` bei ID 0, ohne Treffer oder ohne Internet.
pub async fn resolve_id(
    cfg: &crate::config::BadhubConfig,
    http: &reqwest::Client,
    verein_id: u32,
) -> CachedImg {
    if verein_id == 0 {
        return None;
    }
    aufloesen(cfg, http, Gesucht::Id(verein_id)).await
}

async fn aufloesen(
    cfg: &crate::config::BadhubConfig,
    http: &reqwest::Client,
    gesucht: Gesucht<'_>,
) -> CachedImg {
    let base = base_url(cfg)?;
    let key = gesucht.schluessel();

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

    let value = fetch(http, &base, gesucht).await;
    let mut cache = img_cache().write().unwrap();
    platz_schaffen(&mut cache, Instant::now());
    // Ist der Cache danach immer noch voll, wird das Ergebnis geliefert,
    // aber nicht gemerkt.
    if cache.len() < CACHE_MAX || cache.contains_key(&key) {
        cache.insert(
            key,
            CacheEntry {
                fetched_at: Instant::now(),
                value: value.clone(),
            },
        );
    }
    value
}

/// Räumt abgelaufene Einträge, sobald der Cache voll ist. Die Frist wirkt
/// sonst nur als „neu holen bei erneuter Frage“ — ein nie wieder gefragter
/// Eintrag bliebe für immer liegen.
fn platz_schaffen(cache: &mut HashMap<String, CacheEntry>, jetzt: Instant) {
    if cache.len() < CACHE_MAX {
        return;
    }
    cache.retain(|_, e| {
        let ttl = if e.value.is_some() {
            CACHE_TTL_OK
        } else {
            CACHE_TTL_EMPTY
        };
        jetzt.saturating_duration_since(e.fetched_at) <= ttl
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voller_cache_wirft_abgelaufene_eintraege_hinaus() {
        let jetzt = Instant::now();
        let mut cache = HashMap::new();
        let eintrag = |alter: Duration, treffer: bool| CacheEntry {
            fetched_at: jetzt - alter,
            value: treffer.then(|| ("image/png".to_string(), vec![1])),
        };
        // Unter der Grenze bleibt alles liegen, auch Abgelaufenes.
        cache.insert("alt".to_string(), eintrag(Duration::from_secs(120), false));
        platz_schaffen(&mut cache, jetzt);
        assert_eq!(cache.len(), 1);
        // Voll: abgelaufene Fehltreffer (über 60 s) gehen, frische Einträge
        // und Treffer innerhalb ihrer sechs Stunden bleiben.
        for i in 0..CACHE_MAX {
            cache.insert(format!("i:{i}"), eintrag(Duration::from_secs(120), false));
        }
        cache.insert("frisch".to_string(), eintrag(Duration::from_secs(5), false));
        cache.insert("logo".to_string(), eintrag(Duration::from_secs(3600), true));
        platz_schaffen(&mut cache, jetzt);
        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key("frisch") && cache.contains_key("logo"));
    }

    #[test]
    fn cache_key_is_case_and_space_insensitive() {
        assert_eq!(cache_key("Köpenicker  BC"), cache_key("köpenicker bc"));
    }

    #[test]
    fn name_und_id_teilen_sich_keinen_cache_eintrag() {
        assert_ne!(
            Gesucht::Name("17").schluessel(),
            Gesucht::Id(17).schluessel()
        );
        assert_eq!(
            Gesucht::Name("Köpenicker  BC").schluessel(),
            Gesucht::Name("köpenicker bc").schluessel()
        );
        assert_ne!(Gesucht::Id(1).schluessel(), Gesucht::Id(11).schluessel());
    }

    #[test]
    fn frage_an_den_resolver_per_name_oder_id() {
        let basis = || reqwest::Url::parse("https://badhub.de/api/v1/club-logo").unwrap();
        let mut url = basis();
        Gesucht::Id(42).an_adresse(&mut url);
        assert_eq!(url.as_str(), "https://badhub.de/api/v1/club-logo?id=42");
        let mut url = basis();
        Gesucht::Name("a&b=c").an_adresse(&mut url);
        assert_eq!(
            url.as_str(),
            "https://badhub.de/api/v1/club-logo?name=a%26b%3Dc"
        );
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
    fn bild_nur_von_derselben_origin_ausser_test_auf_live() {
        assert!(bild_herkunft_erlaubt(
            "https://badhub.de",
            "https://badhub.de"
        ));
        assert!(bild_herkunft_erlaubt(
            "https://test.badhub.de",
            "https://test.badhub.de"
        ));
        // Testsystem → Bilddateien des Produktivsystems.
        assert!(bild_herkunft_erlaubt(
            "https://test.badhub.de",
            "https://badhub.de"
        ));
        // Nicht umgekehrt, nicht fremd, nicht über http.
        assert!(!bild_herkunft_erlaubt(
            "https://badhub.de",
            "https://test.badhub.de"
        ));
        assert!(!bild_herkunft_erlaubt(
            "https://test.badhub.de",
            "http://badhub.de"
        ));
        assert!(!bild_herkunft_erlaubt(
            "https://test.badhub.de",
            "https://badhub.de.evil.com"
        ));
        assert!(!bild_herkunft_erlaubt(
            "https://eigene-instanz.example",
            "https://badhub.de"
        ));
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
