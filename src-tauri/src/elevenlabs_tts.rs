// ElevenLabs als Sprachdienst der Ansage (Spec `ansage-elevenlabs`).
//
// Das Frontend baut weiter EIN Azure-SSML je Ansage; `tts_text` macht daraus
// Klartext. Nur Multilingual v2 bekommt `<break>`-Tags (dort dokumentiert),
// die übrigen Modelle Satzzeichen. Eine Sprache wird bewusst NICHT
// vorgegeben: `language_code` würde fremde Namen eindeutschen — genau das,
// was der Hörvergleich prüfen soll.

use crate::tts_text::{netz, ssml_zu_text, Pausen, TtsText};
use std::path::Path;

/// MP3-Ausgabe; das Frontend dekodiert wie bei Azure über `decodeAudioData`.
const AUSGABEFORMAT: &str = "mp3_44100_128";

/// Tempo-Spanne, die ElevenLabs annimmt (`voice_settings.speed`).
const TEMPO_MIN: f32 = 0.7;
const TEMPO_MAX: f32 = 1.2;

fn pausen(modell: &str) -> Pausen {
    if modell == "eleven_multilingual_v2" {
        Pausen::BreakTag
    } else {
        Pausen::Satzzeichen
    }
}

fn text_fuer(ssml: &str, modell: &str) -> TtsText {
    let mut t = ssml_zu_text(ssml, pausen(modell));
    t.speed = t.speed.clamp(TEMPO_MIN, TEMPO_MAX);
    t
}

/// Cache-Schlüssel über alles, was den Klang bestimmt.
fn cache_schluessel(voice: &str, modell: &str, t: &TtsText) -> String {
    format!("elevenlabs|{modell}|{voice}|{:.2}|{}", t.speed, t.text)
}

/// Vertont; mit `cache_dir` über den Plattencache, ohne (Ansage-Seiten) nur
/// im Speicher.
pub async fn vertonen(
    key: &str,
    voice: &str,
    modell: &str,
    ssml: &str,
    cache_dir: Option<&Path>,
) -> Result<(Vec<u8>, bool), String> {
    let t = text_fuer(ssml, modell);
    let schluessel = cache_schluessel(voice, modell, &t);
    if let Some(bytes) = cache_dir.and_then(|d| netz::cache_holen(d, &schluessel)) {
        return Ok((bytes, true));
    }
    let bytes = anfragen(key, voice, modell, &t).await?;
    if let Some(d) = cache_dir {
        netz::cache_ablegen(d, &schluessel, &bytes);
    }
    Ok((bytes, false))
}

async fn anfragen(key: &str, voice: &str, modell: &str, t: &TtsText) -> Result<Vec<u8>, String> {
    if !netz::stimme_gueltig(voice) {
        return Err("ElevenLabs: ungültige Stimmen-ID".to_string());
    }
    if t.text.is_empty() {
        return Err("ElevenLabs: leerer Ansagetext".to_string());
    }
    let url = format!(
        "https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format={AUSGABEFORMAT}"
    );
    let body = serde_json::json!({
        "text": t.text,
        "model_id": modell,
        "voice_settings": { "speed": t.speed },
    });
    let resp = netz::client(10)?
        .post(&url)
        .header("xi-api-key", key)
        .header("Accept", "audio/mpeg")
        .header("User-Agent", "bts-light")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ElevenLabs-Anfrage: {e}"))?;
    if !resp.status().is_success() {
        return Err(netz::http_fehler("ElevenLabs", resp).await);
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("ElevenLabs-Antwort: {e}"))?
        .to_vec();
    if bytes.is_empty() {
        return Err("ElevenLabs lieferte leeres Audio".to_string());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SSML: &str = r#"<speak version="1.0" xml:lang="de-DE"><voice name="x"><prosody rate="-50%">Feld 3</prosody><break time="350ms"/>Anna</voice></speak>"#;

    #[test]
    fn nur_v2_bekommt_break_tags() {
        assert!(text_fuer(SSML, "eleven_multilingual_v2")
            .text
            .contains("<break"));
        for m in ["eleven_v3", "eleven_v4", "eleven_flash_v2_5"] {
            assert_eq!(text_fuer(SSML, m).text, "Feld 3. Anna", "{m}");
        }
    }

    #[test]
    fn tempo_wird_auf_die_elevenlabs_spanne_begrenzt() {
        assert_eq!(text_fuer(SSML, "eleven_v3").speed, TEMPO_MIN);
    }

    #[test]
    fn cache_schluessel_trennt_modell_stimme_und_tempo() {
        let t = text_fuer(SSML, "eleven_v3");
        let a = cache_schluessel("s1", "eleven_v3", &t);
        assert_ne!(a, cache_schluessel("s2", "eleven_v3", &t));
        assert_ne!(a, cache_schluessel("s1", "eleven_v4", &t));
        let mut schneller = t.clone();
        schneller.speed = 1.1;
        assert_ne!(a, cache_schluessel("s1", "eleven_v3", &schneller));
    }
}

/// Messwerkzeug gegen den echten Dienst (kostet Kontingent, daher `ignore`):
/// `ELEVENLABS_TEST_KEY=sk_… cargo test --lib elevenlabs_echt -- --ignored`
#[cfg(test)]
mod echt {
    #[tokio::test]
    #[ignore]
    async fn elevenlabs_echt() {
        let key = std::env::var("ELEVENLABS_TEST_KEY").expect("ELEVENLABS_TEST_KEY setzen");
        let ssml = r#"<speak version="1.0" xml:lang="de-DE"><voice name="x"><prosody rate="-20%">Feld 3</prosody><break time="350ms"/><prosody rate="-20%">Zhang Yue gegen Nguyen</prosody></voice></speak>"#;
        for modell in [
            "eleven_multilingual_v2",
            "eleven_v3",
            "eleven_v4",
            "eleven_flash_v2_5",
        ] {
            let mp3 = super::vertonen(
                &key,
                crate::config::ELEVENLABS_STANDARD_STIMME,
                modell,
                ssml,
                None,
            )
            .await
            .unwrap_or_else(|e| panic!("{modell}: {e}"))
            .0;
            assert!(mp3.len() > 1000, "{modell}: zu kurz");
            assert!(
                mp3.starts_with(b"ID3") || mp3[0] == 0xFF,
                "{modell}: kein MP3"
            );
        }
    }
}
