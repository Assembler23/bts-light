// Gemini TTS (Google AI Studio, Interactions API) als Sprachdienst der Ansage
// (Spec `ansage-elevenlabs`, E6 und Umstieg auf 3.8 in E10). Nur der
// AI-Studio-Schlüssel, kein Vertex AI (Nutzer-Entscheidung).
//
// Seit 3.8 ist der Text ein **wörtliches Transkript**; Ton und Tempo stehen
// getrennt in `speech_metadata.style`. Damit kann das Modell die Anweisung
// nicht mehr mitsprechen (Eigenheit der 2.5-Vorschaumodelle, die Google am
// 17.11.2026 abschaltet). Antwort ist WAV (24 kHz, mono, 16 bit); kommt doch
// rohes PCM, wird es hier zu WAV.

use crate::tts_text::{netz, ssml_zu_text, Format, Pausen, TtsText};
use base64::Engine;
use std::path::Path;

const URL: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";

/// Sprechweise für `speech_metadata.style` — englisch, wie in Googles
/// Beispielen; die Sprache der Ansage erkennt das Modell am Text.
fn stil(t: &TtsText) -> String {
    let sprache = if t.sprache == "en" {
        "English"
    } else {
        "German"
    };
    let tempo = if t.speed < 0.95 {
        ", speaking a little slowly"
    } else if t.speed > 1.05 {
        ", speaking a little faster"
    } else {
        ""
    };
    format!(
        "calm, clear and distinct hall announcer at a badminton tournament, in {sprache}, \
         pronouncing player names as in their language of origin{tempo}"
    )
}

/// Anfrage an die Interactions API. `store: false`: Google soll die Anfrage
/// (mit den Spielernamen) nicht als Interaktion aufbewahren.
fn anfrage(modell: &str, voice: &str, t: &TtsText) -> serde_json::Value {
    serde_json::json!({
        "model": modell,
        "input": [{
            "type": "user_input",
            "content": [{
                "type": "text",
                "text": t.text,
                "annotations": [{ "type": "speech_metadata", "style": stil(t) }]
            }]
        }],
        "response_format": { "type": "audio", "mime_type": "audio/wav" },
        "generation_config": { "speech_config": [{ "voice": voice }] },
        "store": false
    })
}

/// Abtastrate aus dem MIME-Typ (`audio/l16;rate=24000`), sonst 24 kHz.
fn rate_aus_mime(mime: &str) -> u32 {
    mime.split(';')
        .find_map(|teil| teil.trim().strip_prefix("rate="))
        .and_then(|r| r.parse().ok())
        .filter(|r| (8_000..=96_000).contains(r))
        .unwrap_or(24_000)
}

/// Antwort → WAV: das letzte Audio der Modell-Ausgabe
/// (`steps[type=model_output].content[type=audio].data`). Rein & testbar.
fn wav_aus_antwort(json: &serde_json::Value) -> Result<Vec<u8>, String> {
    let audio = json["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["type"] == "model_output")
        .flat_map(|s| s["content"].as_array().into_iter().flatten())
        .rfind(|c| c["type"] == "audio")
        .ok_or("Gemini: Antwort ohne Audio")?;
    let daten = audio["data"].as_str().ok_or("Gemini: Audio ohne Daten")?;
    let roh = base64::engine::general_purpose::STANDARD
        .decode(daten)
        .map_err(|e| format!("Gemini: Audio nicht lesbar: {e}"))?;
    if roh.starts_with(b"RIFF") {
        if roh.len() <= 44 {
            return Err("Gemini lieferte leeres Audio".to_string());
        }
        return Ok(roh);
    }
    // Rohes PCM (16 bit, little endian) → WAV.
    if roh.len() < 2 {
        return Err("Gemini lieferte leeres Audio".to_string());
    }
    let mime = audio["mime_type"]
        .as_str()
        .or_else(|| audio["mimeType"].as_str())
        .unwrap_or("");
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
    format!("gemini|{modell}|{voice}|{}|{}", stil(t), t.text)
}

/// Vertont zu WAV (+ `true`, wenn aus dem Cache); mit `cache_dir` über den
/// Plattencache. Gemini liefert in beiden Formaten WAV 24 kHz, `format`
/// ändert deshalb weder Anfrage noch Cache-Eintrag.
pub async fn vertonen(
    key: &str,
    voice: &str,
    modell: &str,
    ssml: &str,
    cache_dir: Option<&Path>,
    _format: Format,
) -> Result<(Vec<u8>, bool), String> {
    let t = ssml_zu_text(ssml, Pausen::Satzzeichen);
    let schluessel = cache_schluessel(voice, modell, &t);
    if let Some(bytes) = cache_dir.and_then(|d| netz::cache_holen(d, &schluessel)) {
        return Ok((bytes, true));
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
    // Gemini braucht für eine Ansage spürbar länger als Azure; 20 s statt 10.
    let resp = netz::client(20)?
        .post(URL)
        .header("x-goog-api-key", key)
        .header("User-Agent", "bts-light")
        .json(&anfrage(modell, voice, &t))
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
    Ok((wav, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(b: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b)
    }

    fn t(speed: f32, sprache: &'static str) -> TtsText {
        TtsText {
            text: "Feld 3. Anna gegen Bo.".into(),
            speed,
            sprache,
        }
    }

    #[test]
    fn wav_antwort_wird_unveraendert_uebernommen() {
        let wav = crate::ansage_ton::pcm::wav_schreiben(&[0, 1000, -1000, 0], 24_000);
        let json = serde_json::json!({ "steps": [
            { "type": "thought" },
            { "type": "model_output", "content": [
                { "type": "text", "text": "x" },
                { "type": "audio", "mime_type": "audio/wav", "data": b64(&wav) }
            ]}
        ]});
        assert_eq!(wav_aus_antwort(&json).unwrap(), wav);
    }

    #[test]
    fn rohes_pcm_wird_zu_wav_mit_der_richtigen_rate() {
        let pcm: Vec<u8> = [0i16, 1000, -1000, 0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let json = serde_json::json!({ "steps": [{ "type": "model_output", "content": [
            { "type": "audio", "mime_type": "audio/l16;rate=16000", "data": b64(&pcm) }
        ]}]});
        let wav = wav_aus_antwort(&json).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(wav.len(), 44 + pcm.len());
    }

    #[test]
    fn antwort_ohne_audio_ist_ein_fehler() {
        let nur_text = serde_json::json!({ "steps": [{ "type": "model_output",
            "content": [{ "type": "text", "text": "nö" }] }] });
        assert!(wav_aus_antwort(&nur_text).is_err());
        assert!(wav_aus_antwort(&serde_json::json!({})).is_err());
        // Audio nur in einem Nicht-Ausgabe-Schritt zählt nicht.
        let falsch = serde_json::json!({ "steps": [{ "type": "user_input",
            "content": [{ "type": "audio", "data": b64(b"RIFF") }] }] });
        assert!(wav_aus_antwort(&falsch).is_err());
    }

    #[test]
    fn text_bleibt_woertlich_stil_traegt_ton_und_tempo() {
        let a = anfrage("gemini-3.8-flash-tts", "Kore", &t(0.8, "de"));
        let teil = &a["input"][0]["content"][0];
        assert_eq!(teil["text"], "Feld 3. Anna gegen Bo.");
        let s = teil["annotations"][0]["style"].as_str().unwrap();
        assert!(s.contains("German") && s.contains("slowly"), "{s}");
        assert_eq!(a["store"], false);
        assert_eq!(a["generation_config"]["speech_config"][0]["voice"], "Kore");
        assert!(!stil(&t(1.0, "en")).contains("slow"));
        let en = stil(&t(1.2, "en"));
        assert!(en.contains("English") && en.contains("faster"), "{en}");
    }

    #[test]
    fn cache_schluessel_kennt_tempo_modell_und_stimme() {
        let a = cache_schluessel("Kore", "m1", &t(0.8, "de"));
        assert_ne!(a, cache_schluessel("Kore", "m1", &t(1.0, "de")));
        assert_ne!(a, cache_schluessel("Kore", "m2", &t(0.8, "de")));
        assert_ne!(a, cache_schluessel("Puck", "m1", &t(0.8, "de")));
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
        for modell in ["gemini-3.8-flash-tts", "gemini-3.8-flash-lite-tts"] {
            let wav = super::vertonen(
                &key,
                crate::config::GEMINI_STANDARD_STIMME,
                modell,
                ssml,
                None,
                crate::tts_text::Format::Mp3,
            )
            .await
            .unwrap_or_else(|e| panic!("{modell}: {e}"))
            .0;
            assert_eq!(&wav[..4], b"RIFF", "{modell}");
        }
    }
}
