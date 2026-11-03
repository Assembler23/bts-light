// Gemini (Google AI Studio, `generateContent` mit Audio-Ausgabe) als
// Sprachdienst der Ansage (Spec `ansage-elevenlabs`, Erweiterung 03.11.2026).
// Nur der AI-Studio-Schlüssel, kein Vertex AI (Nutzer-Entscheidung).
//
// Gemini liefert rohes PCM (16 bit, mono, meist 24 kHz) als Base64 im JSON;
// daraus wird hier eine WAV-Datei, die das Frontend wie MP3 dekodiert. Tempo
// und Ton stehen als Anweisung vor dem Text („Sag ruhig und deutlich: …") —
// einen Tempo-Parameter gibt es nicht.

use crate::tts_text::{netz, ssml_zu_text, Pausen, TtsText};
use base64::Engine;
use std::path::Path;

/// Ansagetext mit vorangestellter Sprechanweisung.
fn prompt(t: &TtsText) -> String {
    let tempo_de = if t.speed < 0.95 {
        " und etwas langsamer als normal"
    } else if t.speed > 1.05 {
        " und etwas schneller als normal"
    } else {
        ""
    };
    let tempo_en = if t.speed < 0.95 {
        " and somewhat slower than normal"
    } else if t.speed > 1.05 {
        " and somewhat faster than normal"
    } else {
        ""
    };
    if t.sprache == "en" {
        format!(
            "Say this as a calm, clear hall announcer at a badminton tournament{tempo_en}, \
             pronouncing names as in their language of origin: {}",
            t.text
        )
    } else {
        format!(
            "Sag das auf Deutsch als ruhiger, deutlicher Hallensprecher bei einem \
             Badmintonturnier{tempo_de}, Namen wie in ihrer Herkunftssprache: {}",
            t.text
        )
    }
}

/// Abtastrate aus dem MIME-Typ (`audio/L16;codec=pcm;rate=24000`), sonst 24 kHz.
fn rate_aus_mime(mime: &str) -> u32 {
    mime.split(';')
        .find_map(|teil| teil.trim().strip_prefix("rate="))
        .and_then(|r| r.parse().ok())
        .filter(|r| (8_000..=96_000).contains(r))
        .unwrap_or(24_000)
}

/// Gemini-Antwort → WAV. Rein & testbar.
fn wav_aus_antwort(json: &serde_json::Value) -> Result<Vec<u8>, String> {
    let teile = json["candidates"][0]["content"]["parts"]
        .as_array()
        .ok_or("Gemini: Antwort ohne Audio")?;
    let inline = teile
        .iter()
        .find_map(|p| p.get("inlineData").or_else(|| p.get("inline_data")))
        .ok_or("Gemini: Antwort ohne Audio")?;
    let mime = inline
        .get("mimeType")
        .or_else(|| inline.get("mime_type"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let daten = inline["data"].as_str().ok_or("Gemini: Audio ohne Daten")?;
    let roh = base64::engine::general_purpose::STANDARD
        .decode(daten)
        .map_err(|e| format!("Gemini: Audio nicht lesbar: {e}"))?;
    if roh.len() < 2 {
        return Err("Gemini lieferte leeres Audio".to_string());
    }
    let samples: Vec<i16> = roh
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    Ok(crate::ansage_ton::pcm::wav_schreiben(
        &samples,
        rate_aus_mime(mime),
    ))
}

fn cache_schluessel(voice: &str, modell: &str, t: &TtsText) -> String {
    format!("gemini|{modell}|{voice}|{}", prompt(t))
}

/// Vertont zu WAV; mit `cache_dir` über den Plattencache.
pub async fn vertonen(
    key: &str,
    voice: &str,
    modell: &str,
    ssml: &str,
    cache_dir: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let t = ssml_zu_text(ssml, Pausen::Satzzeichen);
    let schluessel = cache_schluessel(voice, modell, &t);
    if let Some(bytes) = cache_dir.and_then(|d| netz::cache_holen(d, &schluessel)) {
        return Ok(bytes);
    }
    if !netz::stimme_gueltig(voice)
        || !modell
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return Err("Gemini: ungültige Stimme oder Modell".to_string());
    }
    if t.text.is_empty() {
        return Err("Gemini: leerer Ansagetext".to_string());
    }
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{modell}:generateContent");
    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt(&t) }] }],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } }
            }
        }
    });
    // Gemini braucht für eine Ansage spürbar länger als Azure (Pro mehr als
    // Flash); 20 s statt 10.
    let resp = netz::client(20)?
        .post(&url)
        .header("x-goog-api-key", key)
        .header("User-Agent", "bts-light")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini-Anfrage: {e}"))?;
    if !resp.status().is_success() {
        return Err(netz::http_fehler("Gemini", resp).await);
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Gemini-Antwort: {e}"))?;
    let wav = wav_aus_antwort(&json)?;
    if let Some(d) = cache_dir {
        netz::cache_ablegen(d, &schluessel, &wav);
    }
    Ok(wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antwort_wird_zu_wav_mit_der_richtigen_rate() {
        let pcm: Vec<u8> = [0i16, 1000, -1000, 0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let json = serde_json::json!({
            "candidates": [{ "content": { "parts": [{ "inlineData": {
                "mimeType": "audio/L16;codec=pcm;rate=24000",
                "data": base64::engine::general_purpose::STANDARD.encode(&pcm)
            }}]}}]
        });
        let wav = wav_aus_antwort(&json).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert_eq!(wav.len(), 44 + pcm.len());
    }

    #[test]
    fn antwort_ohne_audio_ist_ein_fehler() {
        let json =
            serde_json::json!({ "candidates": [{ "content": { "parts": [{ "text": "nö" }] } }] });
        assert!(wav_aus_antwort(&json).is_err());
        assert!(wav_aus_antwort(&serde_json::json!({})).is_err());
    }

    #[test]
    fn rate_und_prompt() {
        assert_eq!(rate_aus_mime("audio/L16;codec=pcm;rate=16000"), 16_000);
        assert_eq!(rate_aus_mime("audio/L16"), 24_000);
        assert_eq!(rate_aus_mime("audio/L16;rate=5"), 24_000);
        let t = TtsText {
            text: "Feld 3.".into(),
            speed: 0.8,
            sprache: "de",
        };
        let p = prompt(&t);
        assert!(p.contains("langsamer") && p.ends_with(": Feld 3."), "{p}");
    }
}

/// Messwerkzeug: `GEMINI_TEST_KEY=… cargo test --lib gemini_echt -- --ignored`
#[cfg(test)]
mod echt {
    #[tokio::test]
    #[ignore]
    async fn gemini_echt() {
        let key = std::env::var("GEMINI_TEST_KEY").expect("GEMINI_TEST_KEY setzen");
        let ssml = r#"<speak version="1.0" xml:lang="de-DE"><voice name="x"><prosody rate="-20%">Feld 3</prosody><break time="350ms"/>Zhang Yue gegen Nguyen</voice></speak>"#;
        for modell in ["gemini-2.5-flash-preview-tts", "gemini-2.5-pro-preview-tts"] {
            let wav = super::vertonen(
                &key,
                crate::config::GEMINI_STANDARD_STIMME,
                modell,
                ssml,
                None,
            )
            .await
            .unwrap_or_else(|e| panic!("{modell}: {e}"));
            assert_eq!(&wav[..4], b"RIFF", "{modell}");
        }
    }
}
