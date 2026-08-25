//! Umschalter zwischen dem Produktiv- und dem Testsystem von badhub.
//!
//! **Ein Turnier auf `test.badhub.de` fahren, ohne die Produktiv-Datenbank zu
//! berühren.** Der Modus wird bewusst *nicht* als eigenes Konfigurationsfeld
//! geführt, sondern aus der Push-URL (`badhub.url`) abgeleitet — sonst gäbe es
//! zwei Wahrheiten (Flag und URL), die auseinanderdriften können, und
//! ausgerechnet die stille Variante („Flag an, URL zeigt auf Produktiv")
//! schriebe Testdaten in den echten Liveticker.
//!
//! Alles, was ohnehin per [`crate::commands::badhub_origin`] aus der Push-URL
//! abgeleitet wird (Check-In, Aussprache-Wörterbuch, Vereinslogos,
//! Branding-Push), folgt dadurch von allein. Für die Strecken **ohne**
//! Config-Zugriff — Cloud-Relay, Diagnose-Log-Upload — hält dieses Modul einen
//! Prozess-Schalter bereit, der beim Start und bei jedem Speichern aus
//! derselben URL nachgezogen wird.
//!
//! Fremde Hosts bleiben immer unangetastet: Wer eine eigene badhub-Instanz
//! betreibt, bekommt seine Adresse nicht umgeschrieben.

use std::sync::atomic::{AtomicBool, Ordering};

/// Produktivsystem.
pub const HOST_LIVE: &str = "badhub.de";
/// Testsystem (gleiche Software, eigene Datenbank).
pub const HOST_TEST: &str = "test.badhub.de";

/// Prozessweiter Schalter für die Stellen ohne Config-Zugriff.
static TESTSYSTEM: AtomicBool = AtomicBool::new(false);

/// Erkennt die Hosts, die bts-light als „badhub" umschalten darf. Alles
/// andere (eigene Instanz, IP, localhost) bleibt unberührt.
fn ist_badhub_host(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    host == HOST_LIVE || host == format!("www.{HOST_LIVE}") || host == HOST_TEST
}

/// Der Host für das gewählte System.
pub fn host_fuer(test: bool) -> &'static str {
    if test {
        HOST_TEST
    } else {
        HOST_LIVE
    }
}

/// Zeigt diese URL auf das Testsystem? Fremde Hosts gelten als Produktiv —
/// eine eigene badhub-Instanz ist kein Testlauf.
pub fn ist_testsystem(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.eq_ignore_ascii_case(HOST_TEST)))
        .unwrap_or(false)
}

/// Biegt eine badhub-URL auf das gewählte System um. Pfad, Query und Fragment
/// bleiben erhalten — aus `https://badhub.de/live?t=bvbb` wird
/// `https://test.badhub.de/live?t=bvbb`. URLs auf fremde Hosts und
/// unparsbare Eingaben kommen unverändert zurück.
pub fn url_fuer(url: &str, test: bool) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(url.trim()) else {
        return url.to_string();
    };
    match parsed.host_str() {
        Some(h) if ist_badhub_host(h) => {}
        _ => return url.to_string(),
    }
    if parsed.set_host(Some(host_fuer(test))).is_err() {
        return url.to_string();
    }
    parsed.to_string()
}

/// Setzt den Prozess-Schalter aus der konfigurierten Push-URL. Aufrufer sind
/// der App-Start und jedes Speichern der Konfiguration — mehr Stellen darf es
/// nicht geben, sonst ist die Ableitung wieder eine zweite Wahrheit.
pub fn set_aus_push_url(url: &str) {
    let test = ist_testsystem(url);
    if TESTSYSTEM.swap(test, Ordering::Relaxed) != test {
        tracing::warn!(
            "badhub-Ziel umgeschaltet auf {} — Relay und Diagnose-Logs folgen",
            host_fuer(test)
        );
    }
}

/// Läuft die Installation gerade gegen das Testsystem?
pub fn testsystem() -> bool {
    TESTSYSTEM.load(Ordering::Relaxed)
}

/// Aktiver badhub-Host (`badhub.de` oder `test.badhub.de`).
pub fn host() -> &'static str {
    host_fuer(testsystem())
}

/// Basis-Origin des aktiven Systems, z. B. `https://badhub.de`.
pub fn basis() -> String {
    format!("https://{}", host())
}

/// Ein API-Endpunkt des aktiven Systems, z. B. `.../api/bts_log.php`.
pub fn api_url(pfad: &str) -> String {
    format!("https://{}/api/{}", host(), pfad.trim_start_matches('/'))
}

/// HTTPS-Basis des Cloud-Relays des aktiven Systems.
pub fn relay_https() -> String {
    format!("https://{}/bts-relay", host())
}

/// WSS-Basis des Cloud-Relays des aktiven Systems.
pub fn relay_wss() -> String {
    format!("wss://{}/bts-relay", host())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schaltet_die_push_url_um_und_zurueck() {
        let live = "https://badhub.de/api/live_update.php";
        let test = "https://test.badhub.de/api/live_update.php";
        assert_eq!(url_fuer(live, true), test);
        assert_eq!(url_fuer(test, false), live);
        // Zweimal dasselbe Ziel ist ein No-Op, kein doppeltes Präfix.
        assert_eq!(url_fuer(test, true), test);
        assert_eq!(url_fuer(live, false), live);
    }

    #[test]
    fn behaelt_pfad_und_query_der_live_seite() {
        assert_eq!(
            url_fuer("https://badhub.de/live?t=bvbb", true),
            "https://test.badhub.de/live?t=bvbb"
        );
        assert_eq!(
            url_fuer("https://test.badhub.de/live/bvbb/teilnehmer", false),
            "https://badhub.de/live/bvbb/teilnehmer"
        );
    }

    #[test]
    fn www_zaehlt_als_produktivsystem() {
        assert_eq!(
            url_fuer("https://www.badhub.de/live?t=bvbb", true),
            "https://test.badhub.de/live?t=bvbb"
        );
    }

    #[test]
    fn fremde_hosts_bleiben_unangetastet() {
        // Wer eine eigene badhub-Instanz betreibt, darf sie nicht verlieren.
        for url in [
            "https://liveticker.example.org/api/live_update.php",
            "http://192.168.1.50/api/live_update.php",
            "https://badhub.example.com/live?t=x",
            "kein-url",
            "",
        ] {
            assert_eq!(url_fuer(url, true), url, "umgeschaltet: {url}");
            assert_eq!(url_fuer(url, false), url, "umgeschaltet: {url}");
        }
    }

    #[test]
    fn erkennt_das_testsystem_an_der_url() {
        assert!(ist_testsystem("https://test.badhub.de/api/live_update.php"));
        assert!(ist_testsystem("https://TEST.BADHUB.DE/live?t=x"));
        assert!(!ist_testsystem("https://badhub.de/api/live_update.php"));
        assert!(!ist_testsystem("https://www.badhub.de/live?t=x"));
        // Ein fremder Host ist kein Testlauf, auch wenn „test" darin vorkommt.
        assert!(!ist_testsystem(
            "https://test.example.org/api/live_update.php"
        ));
        assert!(!ist_testsystem("unsinn"));
    }

    #[test]
    fn baut_die_adressen_des_gewaehlten_systems() {
        assert_eq!(host_fuer(false), HOST_LIVE);
        assert_eq!(host_fuer(true), HOST_TEST);
    }
}
