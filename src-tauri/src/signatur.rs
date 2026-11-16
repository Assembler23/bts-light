//! Prüfung von minisign-Signaturen mit dem Schlüssel des Updaters
//! (Spec `hotfix-ohne-neustart`, ADR 0099).
//!
//! Nachgeladene Geräte-Seiten sind Code, der auf den Tablets und im TL-Web
//! läuft. Sie werden deshalb mit demselben Schlüssel geprüft, mit dem der
//! Updater den Installer prüft — eine zweite Vertrauenswurzel gäbe es nur
//! mit einem zweiten Geheimnis, das jemand hüten müsste.
//!
//! Das Format ist das des Tauri-Signierwerkzeugs: Schlüssel und Signatur
//! sind jeweils die base64-kodierte minisign-Textdatei.

use std::sync::OnceLock;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

/// Warum eine Prüfung scheiterte. Bewusst grob: Für die Entscheidung
/// „ablehnen" spielt der Grund keine Rolle, er dient nur der Log-Zeile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignaturFehler {
    /// Der öffentliche Schlüssel ließ sich nicht lesen.
    SchluesselUnlesbar,
    /// Die Signatur ist kein gültiger minisign-Text.
    SignaturUnlesbar,
    /// Die Signatur passt nicht zu Daten oder Schlüssel.
    Ungueltig,
}

impl std::fmt::Display for SignaturFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SignaturFehler::SchluesselUnlesbar => "öffentlicher Schlüssel unlesbar",
            SignaturFehler::SignaturUnlesbar => "Signatur unlesbar",
            SignaturFehler::Ungueltig => "Signatur passt nicht",
        })
    }
}

/// Ein öffentlicher minisign-Schlüssel.
pub struct Schluessel(PublicKey);

impl Schluessel {
    /// Liest einen Schlüssel in der Schreibweise von `tauri.conf.json`
    /// (`plugins.updater.pubkey`): base64 über die minisign-Schlüsseldatei.
    pub fn aus_tauri_text(kodiert: &str) -> Result<Self, SignaturFehler> {
        let text = base64_text(kodiert).ok_or(SignaturFehler::SchluesselUnlesbar)?;
        PublicKey::decode(&text)
            .map(Schluessel)
            .map_err(|_| SignaturFehler::SchluesselUnlesbar)
    }
}

/// Der Schlüssel des Updaters, gelesen aus der eingebetteten
/// `tauri.conf.json` — **eine** Quelle für Updater und Seiten-Pakete, damit
/// ein Schlüsselwechsel nicht an einer zweiten Stelle vergessen wird.
pub fn updater_schluessel() -> Result<&'static Schluessel, SignaturFehler> {
    static SCHLUESSEL: OnceLock<Option<Schluessel>> = OnceLock::new();
    SCHLUESSEL
        .get_or_init(|| {
            let conf: serde_json::Value =
                serde_json::from_str(include_str!("../tauri.conf.json")).ok()?;
            let kodiert = conf
                .pointer("/plugins/updater/pubkey")
                .and_then(|v| v.as_str())?;
            Schluessel::aus_tauri_text(kodiert).ok()
        })
        .as_ref()
        .ok_or(SignaturFehler::SchluesselUnlesbar)
}

/// Prüft `signatur` (Schreibweise des Tauri-Signierwerkzeugs) über die rohen
/// `daten`.
///
/// Angenommen wird nur die Form **mit Vorab-Hash**, wie sie das
/// Signierwerkzeug erzeugt. Die ältere Form ohne Vorab-Hash bleibt draußen:
/// Die zwei Bytes, die die Form nennen, sind selbst nicht signiert — ließe
/// man beide zu, könnte jemand eine veröffentlichte Installer-Signatur
/// umetikettieren und sie gälte für den 64-Byte-Hash des Installers als
/// „Daten". Daraus wird zwar kein Paket, aber die ganze Klasse ist weg, wenn
/// es nur eine Form gibt (Security-Review 16.11.2026).
///
/// Nur in Tests ist die ältere Form erlaubt: Die Testhilfe erzeugt sie, weil
/// die Form mit Vorab-Hash BLAKE2 als weitere Abhängigkeit bräuchte. Dass die
/// echte Form angenommen wird, hält die Probe-Signatur unten fest.
pub fn pruefen(
    schluessel: &Schluessel,
    daten: &[u8],
    signatur: &str,
) -> Result<(), SignaturFehler> {
    let text = base64_text(signatur.trim()).ok_or(SignaturFehler::SignaturUnlesbar)?;
    let signatur = Signature::decode(&text).map_err(|_| SignaturFehler::SignaturUnlesbar)?;
    schluessel
        .0
        .verify(daten, &signatur, cfg!(test))
        .map_err(|_| SignaturFehler::Ungueltig)
}

fn base64_text(kodiert: &str) -> Option<String> {
    let roh = base64::engine::general_purpose::STANDARD
        .decode(kodiert)
        .ok()?;
    String::from_utf8(roh).ok()
}

/// Schlüsselpaar für Tests: erzeugt echte minisign-Signaturen (ältere Form
/// ohne Vorab-Hash, damit kein BLAKE2 als Abhängigkeit nötig ist).
#[cfg(test)]
pub(crate) mod testhilfe {
    use base64::Engine;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::Schluessel;

    pub(crate) struct Testschluessel {
        paar: Ed25519KeyPair,
        kennung: [u8; 8],
    }

    impl Testschluessel {
        /// Ein frisches Paar; `kennung` unterscheidet zwei Testschlüssel.
        pub(crate) fn neu(kennung: u8) -> Self {
            let rng = ring::rand::SystemRandom::new();
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("Schlüssel erzeugen");
            let paar = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("Schlüssel lesen");
            Self {
                paar,
                kennung: [kennung; 8],
            }
        }

        pub(crate) fn oeffentlich(&self) -> Schluessel {
            let mut bin = b"Ed".to_vec();
            bin.extend_from_slice(&self.kennung);
            bin.extend_from_slice(self.paar.public_key().as_ref());
            let datei = format!("untrusted comment: Testschluessel\n{}\n", b64(&bin));
            Schluessel::aus_tauri_text(&b64(datei.as_bytes())).expect("Testschlüssel lesbar")
        }

        /// Signatur in der Schreibweise des Tauri-Signierwerkzeugs.
        pub(crate) fn signieren(&self, daten: &[u8]) -> String {
            let sig = self.paar.sign(daten);
            let mut bin = b"Ed".to_vec();
            bin.extend_from_slice(&self.kennung);
            bin.extend_from_slice(sig.as_ref());
            let kommentar = "Test";
            let mut gesamt = sig.as_ref().to_vec();
            gesamt.extend_from_slice(kommentar.as_bytes());
            let gesamt_sig = self.paar.sign(&gesamt);
            let datei = format!(
                "untrusted comment: Test\n{}\ntrusted comment: {kommentar}\n{}\n",
                b64(&bin),
                b64(gesamt_sig.as_ref())
            );
            b64(datei.as_bytes())
        }
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::testhilfe::Testschluessel;
    use super::*;

    // Echte Ausgabe von `tauri signer sign` (Form mit Vorab-Hash) mit einem
    // Wegwerf-Schlüssel, dessen privater Teil nie gespeichert wurde. Hält
    // fest, dass die Prüfung das Format des Werkzeugs versteht, mit dem die
    // CI signiert — die Tests sonst erzeugen ihre Signaturen selbst.
    const PROBE_SCHLUESSEL: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDRFRUUxQjBGMTVCQ0JGREQKUldUZHY3d1ZEeHZ1VGtjbkRnMnJYVXJtWHpIMDVZcWJtMFlVMGRycHAzNUdTeGJTQ0d0VU43YmkK";
    const PROBE_SIGNATUR: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUZHY3d1ZEeHZ1VG5ORDZNaXU1MEx3MXdUU2N4eTI4bGFtN3NHK1pSZHdza2Z2Z1JWakNaalFBbmdnZXMrUyt3K0ZsaEJic3RVWlR3anpCTlY1aGZ0bHlZTnhIZHhpZkE0PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0ODEwMjg0CWZpbGU6cHJvYmUuYmluCm9XWFpkZHU2cE90U1Z2VGZkOWZKMUNPbUdHOHpRWlB5ckxqZmxsM1RKaXR3eUJyU1dId3c3UmlyMjRVaWRucmZlczRXUE9ZU3J1bndxYSs1QkE1T0FnPT0K";
    const PROBE_DATEN: &[u8] = b"bts-light Signatur-Probe\n";

    #[test]
    fn signatur_des_tauri_werkzeugs_wird_angenommen() {
        let schluessel = Schluessel::aus_tauri_text(PROBE_SCHLUESSEL).unwrap();
        assert_eq!(pruefen(&schluessel, PROBE_DATEN, PROBE_SIGNATUR), Ok(()));
        // Ein Zeilenumbruch am Ende der `.sig`-Datei darf nicht stören.
        let mit_umbruch = format!("{PROBE_SIGNATUR}\n");
        assert_eq!(pruefen(&schluessel, PROBE_DATEN, &mit_umbruch), Ok(()));
    }

    #[test]
    fn veraenderte_daten_werden_abgelehnt() {
        let schluessel = Schluessel::aus_tauri_text(PROBE_SCHLUESSEL).unwrap();
        assert_eq!(
            pruefen(&schluessel, b"bts-light Signatur-Probe!\n", PROBE_SIGNATUR),
            Err(SignaturFehler::Ungueltig)
        );
    }

    #[test]
    fn der_updater_schluessel_ist_lesbar_und_nicht_der_probeschluessel() {
        let schluessel = updater_schluessel().expect("Schlüssel aus tauri.conf.json");
        assert_eq!(
            pruefen(schluessel, PROBE_DATEN, PROBE_SIGNATUR),
            Err(SignaturFehler::Ungueltig),
            "eine Signatur mit fremdem Schlüssel darf nie gelten"
        );
    }

    #[test]
    fn testschluessel_signiert_gueltig_und_fremder_schluessel_scheitert() {
        let a = Testschluessel::neu(1);
        let b = Testschluessel::neu(2);
        let sig = a.signieren(b"Daten");
        assert_eq!(pruefen(&a.oeffentlich(), b"Daten", &sig), Ok(()));
        assert_eq!(
            pruefen(&a.oeffentlich(), b"daten", &sig),
            Err(SignaturFehler::Ungueltig)
        );
        assert_eq!(
            pruefen(&b.oeffentlich(), b"Daten", &sig),
            Err(SignaturFehler::Ungueltig)
        );
    }

    /// Gleiche Schlüssel-Kennung, anderer Schlüssel: Die Kennung ist nur ein
    /// Hinweis und darf die eigentliche Prüfung nie ersetzen.
    #[test]
    fn gleiche_kennung_mit_anderem_schluessel_scheitert() {
        let a = Testschluessel::neu(7);
        let b = Testschluessel::neu(7);
        let sig = a.signieren(b"Daten");
        assert_eq!(
            pruefen(&b.oeffentlich(), b"Daten", &sig),
            Err(SignaturFehler::Ungueltig)
        );
    }

    #[test]
    fn unlesbares_wird_als_unlesbar_gemeldet() {
        let a = Testschluessel::neu(1);
        for kaputt in ["", "kein base64 !!", "aGFsbG8="] {
            assert_eq!(
                pruefen(&a.oeffentlich(), b"Daten", kaputt),
                Err(SignaturFehler::SignaturUnlesbar),
                "{kaputt:?}"
            );
        }
        assert!(matches!(
            Schluessel::aus_tauri_text("aGFsbG8="),
            Err(SignaturFehler::SchluesselUnlesbar)
        ));
    }
}
