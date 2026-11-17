// Azure Cognitive Services Speech (Neural TTS) — Synthese der hochwertigen
// Ansage. Wird NUR von CronJobs/Frontend über den `azure_tts_speak`-Command
// aufgerufen (Key bleibt im Backend). Ergebnis wird je SSML-Hash auf Platte
// gecacht, damit identische Ansagen / „nochmal aufrufen" kein Netz/Geld kosten.

use std::path::{Path, PathBuf};

use crate::tts_text::netz::{self, TtsFehler};
use crate::tts_text::Format;

/// MP3-Ausgabeformat (klein, gut genug für eine Hallen-Ansage).
const OUTPUT_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";
/// WAV für die Teile einer geteilten Ansage (Spec `ansage-in-teilen`).
const OUTPUT_FORMAT_WAV: &str = "riff-24khz-16bit-mono-pcm";

/// Cache-Dateiname aus dem SSML (inkl. Stimme, die im SSML steckt). FNV-1a —
/// bewusst NICHT `DefaultHasher` (dessen Ergebnis ist über Rust-Versionen/
/// Plattformen NICHT stabil → der ganze Cache würde nach einem Toolchain-Update
/// stillschweigend ungültig). Kein kryptografischer Hash nötig, nur stabil.
pub(crate) fn cache_name(ssml: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in ssml.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}.mp3")
}

/// Host des Dienstes für eine Region — zugleich der Schlüssel des
/// Ausfall-Merkers (`tts_netzlage`).
/// Azure-Regionen heißen wie „westeurope": nur Buchstaben, Ziffern,
/// Bindestrich.
pub fn region_gueltig(region: &str) -> bool {
    !region.is_empty()
        && region.len() <= 63
        && region
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn host(region: &str) -> String {
    format!("{region}.tts.speech.microsoft.com")
}

/// Cache-Datei einer Vertonung. WAV-Teile bekommen einen eigenen Namen, damit
/// sie nie als MP3 gelten.
fn cache_pfad(cache_dir: &Path, ssml: &str, format: Format) -> PathBuf {
    match format {
        Format::Mp3 => cache_dir.join(cache_name(ssml)),
        Format::Wav24 => {
            cache_dir.join(cache_name(&format!("wav24|{ssml}")).replace(".mp3", ".wav"))
        }
    }
}

/// Nur der Blick in den Plattencache, ohne Netz — für die Ansage bei
/// gesetztem Ausfall-Merker.
pub fn aus_cache(ssml: &str, cache_dir: &Path, format: Format) -> Option<Vec<u8>> {
    let bytes = std::fs::read(cache_pfad(cache_dir, ssml, format)).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

/// Synthetisiert das SSML zu MP3-Bytes (+ `true`, wenn aus dem Cache). Liefert gecachte Bytes, falls vorhanden;
/// sonst Aufruf der Azure-REST-API und Ablage im Cache. Fehler kommen mit
/// ihrer Klasse zurück (Netz, Drossel, Zugang, Inhalt) — der Aufrufer fällt
/// dann auf die Ersatzstimme zurück.
pub async fn synthesize(
    region: &str,
    key: &str,
    ssml: &str,
    cache_dir: &Path,
    format: Format,
) -> Result<(Vec<u8>, bool), TtsFehler> {
    let cached = cache_pfad(cache_dir, ssml, format);
    if let Some(bytes) = aus_cache(ssml, cache_dir, format) {
        return Ok((bytes, true));
    }
    let bytes = synthesize_ohne_cache(region, key, ssml, format).await?;
    // Best-effort cachen (Fehler beim Schreiben sind nicht fatal).
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&cached, &bytes);
    Ok((bytes, false))
}

/// Wie [`synthesize`], aber **ohne** Plattencache — für die Ansage-Seiten
/// (Spec `ansage-im-browser`): Deren Audio enthält Spielernamen und darf nur
/// 60 s im RAM leben, nie auf der Platte.
pub async fn synthesize_ohne_cache(
    region: &str,
    key: &str,
    ssml: &str,
    format: Format,
) -> Result<Vec<u8>, TtsFehler> {
    // Die Region steht im Hostnamen und kann aus einer geerbten Config
    // stammen: Mit „x.example#" ginge der Schlüssel sonst an einen fremden
    // Host.
    if !region_gueltig(region) {
        return Err(TtsFehler::inhalt("Azure: ungültige Region"));
    }
    let url = format!("https://{}/cognitiveservices/v1", host(region));
    let resp = netz::client()?
        .post(&url)
        .timeout(netz::ANFRAGE_FRIST)
        .header("Ocp-Apim-Subscription-Key", key)
        .header("Content-Type", "application/ssml+xml")
        .header(
            "X-Microsoft-OutputFormat",
            match format {
                Format::Mp3 => OUTPUT_FORMAT,
                Format::Wav24 => OUTPUT_FORMAT_WAV,
            },
        )
        .header("User-Agent", "bts-light")
        .body(ssml.to_string())
        .send()
        .await
        .map_err(|e| TtsFehler::anfrage("Azure", "Azure-TTS-Request", &e))?;

    if !resp.status().is_success() {
        return Err(netz::status_fehler("Azure", resp).await);
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| TtsFehler::antwort("Azure", "Azure-TTS-Antwort", &e))?
        .to_vec();
    if bytes.is_empty() {
        return Err(TtsFehler::inhalt("Azure-TTS lieferte leeres Audio"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_darf_den_host_nicht_verlassen() {
        assert!(region_gueltig("westeurope"));
        assert!(region_gueltig("germany-west-central"));
        for boese in ["", "x.example#", "a/b", "a@b", "a.b", "a:1", "a?b"] {
            assert!(!region_gueltig(boese), "{boese}");
        }
    }

    #[tokio::test]
    async fn ungueltige_region_schickt_keinen_schluessel() {
        let e = synthesize_ohne_cache("x.example#", "k", "<speak/>", Format::Mp3)
            .await
            .unwrap_err();
        assert_eq!(e.klasse, crate::tts_text::netz::Klasse::Inhalt);
    }

    #[test]
    fn cache_name_is_stable_and_differs() {
        assert_eq!(
            cache_name("<speak>a</speak>"),
            cache_name("<speak>a</speak>")
        );
        assert_ne!(
            cache_name("<speak>a</speak>"),
            cache_name("<speak>b</speak>")
        );
        assert!(cache_name("x").ends_with(".mp3"));
    }
}
