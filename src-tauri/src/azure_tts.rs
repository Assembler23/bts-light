// Azure Cognitive Services Speech (Neural TTS) — Synthese der hochwertigen
// Ansage. Wird NUR von CronJobs/Frontend über den `azure_tts_speak`-Command
// aufgerufen (Key bleibt im Backend). Ergebnis wird je SSML-Hash auf Platte
// gecacht, damit identische Ansagen / „nochmal aufrufen" kein Netz/Geld kosten.

use std::path::Path;
use std::time::Duration;

/// MP3-Ausgabeformat (klein, gut genug für eine Hallen-Ansage).
const OUTPUT_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";

/// Cache-Dateiname aus dem SSML (inkl. Stimme, die im SSML steckt). FNV-1a —
/// bewusst NICHT `DefaultHasher` (dessen Ergebnis ist über Rust-Versionen/
/// Plattformen NICHT stabil → der ganze Cache würde nach einem Toolchain-Update
/// stillschweigend ungültig). Kein kryptografischer Hash nötig, nur stabil.
fn cache_name(ssml: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in ssml.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}.mp3")
}

/// Synthetisiert das SSML zu MP3-Bytes. Liefert gecachte Bytes, falls vorhanden;
/// sonst Aufruf der Azure-REST-API und Ablage im Cache. Fehler (Netz, 4xx/5xx)
/// werden als `Err(String)` gemeldet — der Aufrufer fällt dann auf die lokale
/// Web-Speech-Ansage zurück.
pub async fn synthesize(
    region: &str,
    key: &str,
    ssml: &str,
    cache_dir: &Path,
) -> Result<Vec<u8>, String> {
    let cached = cache_dir.join(cache_name(ssml));
    if let Ok(bytes) = std::fs::read(&cached) {
        if !bytes.is_empty() {
            return Ok(bytes);
        }
    }

    let url = format!("https://{region}.tts.speech.microsoft.com/cognitiveservices/v1");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP-Client: {e}"))?;
    let resp = client
        .post(&url)
        .header("Ocp-Apim-Subscription-Key", key)
        .header("Content-Type", "application/ssml+xml")
        .header("X-Microsoft-OutputFormat", OUTPUT_FORMAT)
        .header("User-Agent", "bts-light")
        .body(ssml.to_string())
        .send()
        .await
        .map_err(|e| format!("Azure-TTS-Request: {e}"))?;

    if !resp.status().is_success() {
        let code = resp.status();
        // Body nur gekürzt anhängen (Azure-Diagnosetext kann lang/verbose sein).
        let body: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        return Err(format!("Azure-TTS HTTP {code}: {body}"));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Azure-TTS-Antwort: {e}"))?
        .to_vec();
    if bytes.is_empty() {
        return Err("Azure-TTS lieferte leeres Audio".to_string());
    }
    // Best-effort cachen (Fehler beim Schreiben sind nicht fatal).
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&cached, &bytes);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

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
