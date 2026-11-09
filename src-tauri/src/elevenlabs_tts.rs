// ElevenLabs als Sprachdienst der Ansage (Spec `ansage-elevenlabs`).
//
// Das Frontend baut weiter EIN Azure-SSML je Ansage; `tts_text` macht daraus
// Klartext. Nur Multilingual v2 bekommt `<break>`-Tags (dort dokumentiert),
// die übrigen Modelle Satzzeichen. Eine Sprache wird bewusst NICHT
// vorgegeben: `language_code` würde fremde Namen eindeutschen — genau das,
// was der Hörvergleich prüfen soll.

use crate::tts_text::{netz, ssml_zu_text, Format, Pausen, TtsText};
use std::path::Path;

/// MP3-Ausgabe; das Frontend dekodiert wie bei Azure über `decodeAudioData`.
const AUSGABEFORMAT: &str = "mp3_44100_128";
/// Rohes PCM 24 kHz für die Teile einer geteilten Ansage; wird hier zu WAV.
const AUSGABEFORMAT_PCM: &str = "pcm_24000";

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
fn cache_schluessel(voice: &str, modell: &str, t: &TtsText, format: Format) -> String {
    let f = match format {
        Format::Mp3 => "",
        Format::Wav24 => "|wav24",
    };
    format!("elevenlabs|{modell}|{voice}|{:.2}|{}{f}", t.speed, t.text)
}

/// Vertont; mit `cache_dir` über den Plattencache, ohne (Ansage-Seiten) nur
/// im Speicher.
pub async fn vertonen(
    key: &str,
    voice: &str,
    modell: &str,
    ssml: &str,
    cache_dir: Option<&Path>,
    format: Format,
) -> Result<(Vec<u8>, bool), String> {
    let t = text_fuer(ssml, modell);
    let schluessel = cache_schluessel(voice, modell, &t, format);
    if let Some(bytes) = cache_dir.and_then(|d| netz::cache_holen(d, &schluessel)) {
        return Ok((bytes, true));
    }
    let mut bytes = anfragen(key, voice, modell, &t, format).await?;
    if format == Format::Wav24 {
        bytes = crate::tts_teile::wav_aus_pcm16(&bytes, crate::tts_teile::RATE);
    }
    if let Some(d) = cache_dir {
        netz::cache_ablegen(d, &schluessel, &bytes);
    }
    Ok((bytes, false))
}

async fn anfragen(
    key: &str,
    voice: &str,
    modell: &str,
    t: &TtsText,
    format: Format,
) -> Result<Vec<u8>, String> {
    if !netz::stimme_gueltig(voice) {
        return Err("ElevenLabs: ungültige Stimmen-ID".to_string());
    }
    if t.text.is_empty() {
        return Err("ElevenLabs: leerer Ansagetext".to_string());
    }
    let (ausgabe, accept) = match format {
        Format::Mp3 => (AUSGABEFORMAT, "audio/mpeg"),
        Format::Wav24 => (AUSGABEFORMAT_PCM, "*/*"),
    };
    let url =
        format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format={ausgabe}");
    let body = serde_json::json!({
        "text": t.text,
        "model_id": modell,
        "voice_settings": { "speed": t.speed },
    });
    let resp = netz::client(10)?
        .post(&url)
        .header("xi-api-key", key)
        .header("Accept", accept)
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
        let m = Format::Mp3;
        let a = cache_schluessel("s1", "eleven_v3", &t, m);
        assert_ne!(a, cache_schluessel("s2", "eleven_v3", &t, m));
        assert_ne!(a, cache_schluessel("s1", "eleven_v4", &t, m));
        assert_ne!(a, cache_schluessel("s1", "eleven_v3", &t, Format::Wav24));
        let mut schneller = t.clone();
        schneller.speed = 1.1;
        assert_ne!(a, cache_schluessel("s1", "eleven_v3", &schneller, m));
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
                crate::tts_text::Format::Mp3,
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

/// Messwerkzeug für die geteilte Ansage (Spec `ansage-in-teilen`): vertont
/// die Teile als WAV 24 kHz und setzt sie zusammen; legt das Ergebnis zum
/// Anhören unter `BTS_TEILE_WAV` ab, falls gesetzt.
/// `ELEVENLABS_TEST_KEY=sk_… cargo test --lib elevenlabs_teile_echt -- --ignored`
#[cfg(test)]
mod echt_teile {
    #[tokio::test]
    #[ignore]
    async fn elevenlabs_teile_echt() {
        let key = std::env::var("ELEVENLABS_TEST_KEY").expect("ELEVENLABS_TEST_KEY setzen");
        let ssml = r#"<speak version="1.0" xml:lang="de-DE"><voice name="x"><prosody rate="-20%">Feld 3.<break time="350ms"/>Herrendoppel O40.<break time="350ms"/>Viertelfinale.<break time="350ms"/>Zhang Yue und Peter Müller.<break time="350ms"/>gegen Tomasz Nowak und Nguyen Van An.<break time="350ms"/>Feld 3.</prosody></voice></speak>"#;
        let teile = crate::tts_teile::teile_aus_ssml(ssml).expect("Teile");
        assert_eq!(teile.len(), 6);
        let mut wavs = Vec::new();
        for t in &teile {
            let (wav, _) = super::vertonen(
                &key,
                crate::config::ELEVENLABS_STANDARD_STIMME,
                "eleven_flash_v2_5",
                t,
                None,
                crate::tts_text::Format::Wav24,
            )
            .await
            .unwrap_or_else(|e| panic!("{t}: {e}"));
            assert_eq!(
                crate::tts_teile::pcm_aus_wav(&wav).map(|p| p.1),
                Some(24_000)
            );
            wavs.push(wav);
        }
        let gesamt = crate::tts_teile::zusammensetzen(&wavs).unwrap();
        if let Ok(ziel) = std::env::var("BTS_TEILE_WAV") {
            std::fs::write(ziel, &gesamt).unwrap();
        }
        assert!(gesamt.len() > 24_000 * 2);
    }
}
