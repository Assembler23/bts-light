// OpenAI (`/v1/audio/speech`) als Sprachdienst der Ansage (Spec
// `ansage-elevenlabs`, Erweiterung 03.11.2026).
//
// Kein SSML: Pausen werden Satzzeichen. `gpt-4o-mini-tts` ignoriert den
// `speed`-Parameter, nimmt aber eine Sprechanweisung (`instructions`) — dort
// stehen Ton und Tempo.

use crate::tts_text::{netz, ssml_zu_text, Format, Pausen, TtsText};
use std::path::Path;

/// Sprechanweisung je Ansagesprache; das Tempo wird angehängt.
fn anweisung(t: &TtsText) -> String {
    let (grund, langsam, schnell) = if t.sprache == "en" {
        (
            "You are the hall announcer at a badminton tournament. Speak clearly, calmly and \
             distinctly. Pronounce player names the way they are said in their language of origin.",
            " Speak somewhat slower than normal.",
            " Speak somewhat faster than normal.",
        )
    } else {
        (
            "Du bist Hallensprecher bei einem Badmintonturnier. Sprich Deutsch, klar, ruhig und \
             deutlich. Sprich Spielernamen so aus, wie man sie in ihrer Herkunftssprache sagt.",
            " Sprich etwas langsamer als normal.",
            " Sprich etwas schneller als normal.",
        )
    };
    let mut s = grund.to_string();
    if t.speed < 0.95 {
        s.push_str(langsam);
    } else if t.speed > 1.05 {
        s.push_str(schnell);
    }
    s
}

fn cache_schluessel(voice: &str, modell: &str, t: &TtsText, format: Format) -> String {
    let f = match format {
        Format::Mp3 => "",
        Format::Wav24 => "|wav24",
    };
    format!(
        "openai|{modell}|{voice}|{}|{}|{}{f}",
        t.sprache,
        anweisung(t),
        t.text
    )
}

/// Vertont; mit `cache_dir` über den Plattencache, ohne nur im Speicher.
pub async fn vertonen(
    key: &str,
    voice: &str,
    modell: &str,
    ssml: &str,
    cache_dir: Option<&Path>,
    format: Format,
) -> Result<(Vec<u8>, bool), String> {
    let t = ssml_zu_text(ssml, Pausen::Satzzeichen);
    let schluessel = cache_schluessel(voice, modell, &t, format);
    if let Some(bytes) = cache_dir.and_then(|d| netz::cache_holen(d, &schluessel)) {
        return Ok((bytes, true));
    }
    if !netz::stimme_gueltig(voice) {
        return Err("OpenAI: ungültige Stimme".to_string());
    }
    if t.text.is_empty() {
        return Err("OpenAI: leerer Ansagetext".to_string());
    }
    let body = serde_json::json!({
        "model": modell,
        "voice": voice,
        "input": t.text,
        "instructions": anweisung(&t),
        // `pcm` = rohes 24-kHz-PCM; wird unten zu WAV.
        "response_format": match format {
            Format::Mp3 => "mp3",
            Format::Wav24 => "pcm",
        },
    });
    let resp = netz::client(10)?
        .post("https://api.openai.com/v1/audio/speech")
        .bearer_auth(key)
        .header("User-Agent", "bts-light")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI-Anfrage: {e}"))?;
    if !resp.status().is_success() {
        return Err(netz::http_fehler("OpenAI", resp).await);
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("OpenAI-Antwort: {e}"))?
        .to_vec();
    if bytes.is_empty() {
        return Err("OpenAI lieferte leeres Audio".to_string());
    }
    let bytes = match format {
        Format::Mp3 => bytes,
        Format::Wav24 => crate::tts_teile::wav_aus_pcm16(&bytes, crate::tts_teile::RATE),
    };
    if let Some(d) = cache_dir {
        netz::cache_ablegen(d, &schluessel, &bytes);
    }
    Ok((bytes, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(speed: f32, sprache: &'static str) -> TtsText {
        TtsText {
            text: "Feld 3.".into(),
            speed,
            sprache,
        }
    }

    #[test]
    fn tempo_steht_in_der_anweisung() {
        assert!(anweisung(&t(0.8, "de")).ends_with("langsamer als normal."));
        assert!(anweisung(&t(1.2, "de")).ends_with("schneller als normal."));
        assert!(!anweisung(&t(1.0, "de")).contains("als normal"));
        assert!(anweisung(&t(0.8, "en")).contains("slower"));
    }

    #[test]
    fn cache_schluessel_kennt_tempo_und_stimme() {
        let a = cache_schluessel("coral", "m", &t(0.8, "de"), Format::Mp3);
        assert_ne!(
            a,
            cache_schluessel("coral", "m", &t(1.0, "de"), Format::Mp3)
        );
        assert_ne!(a, cache_schluessel("nova", "m", &t(0.8, "de"), Format::Mp3));
        assert_ne!(
            a,
            cache_schluessel("coral", "m", &t(0.8, "de"), Format::Wav24)
        );
    }
}

/// Messwerkzeug: `OPENAI_TEST_KEY=sk-… cargo test --lib openai_echt -- --ignored`
#[cfg(test)]
mod echt {
    #[tokio::test]
    #[ignore]
    async fn openai_echt() {
        let key = std::env::var("OPENAI_TEST_KEY").expect("OPENAI_TEST_KEY setzen");
        let ssml = r#"<speak version="1.0" xml:lang="de-DE"><voice name="x"><prosody rate="-20%">Feld 3</prosody><break time="350ms"/>Zhang Yue gegen Nguyen</voice></speak>"#;
        let mp3 = super::vertonen(
            &key,
            crate::config::OPENAI_STANDARD_STIMME,
            "gpt-4o-mini-tts",
            ssml,
            None,
            crate::tts_text::Format::Mp3,
        )
        .await
        .unwrap()
        .0;
        assert!(mp3.len() > 1000);
    }
}
