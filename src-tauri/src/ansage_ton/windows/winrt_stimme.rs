//! Windows-Stimme im Kern (ADR 0066): `Windows.Media.SpeechSynthesis`
//! (OneCore) — gemessen dieselben Stimmen, die Chromium lokal anbietet.
//!
//! Läuft **nur** in `spawn_blocking` (WinRT, MTA), nie auf dem
//! Audio-Thread: Dort würde der COM-Modus kippen, und genau das endete in
//! der Messung in Stille. Das blockierende `join()` auf die
//! WinRT-Operationen ist in diesem Thread gewollt.

use super::super::{pcm, stimme, stimme::StimmeInfo, TonFehler};
use ::windows::core::HSTRING;
use ::windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceInformation};
use ::windows::Storage::Streams::DataReader;

/// Obergrenze eines einzelnen WAV-Stroms. Ein Abschnitt hat höchstens
/// 1000 Zeichen; das sind gemessen weit unter 8 MiB (14 Wörter ≈ 282 KB).
const MAX_WAV_BYTES: u64 = 16 * 1024 * 1024;

fn intern(was: &'static str) -> impl Fn(::windows::core::Error) -> TonFehler {
    move |e| TonFehler::Intern(format!("{was}: {}", e.message()))
}

/// Alle installierten Stimmen samt WinRT-Objekt (zum Setzen).
fn alle() -> Result<Vec<(VoiceInformation, StimmeInfo)>, TonFehler> {
    let liste = SpeechSynthesizer::AllVoices().map_err(intern("Stimmenliste"))?;
    let n = liste.Size().map_err(intern("Stimmenliste"))?;
    (0..n)
        .map(|i| {
            let v = liste.GetAt(i).map_err(intern("Stimme"))?;
            let info = StimmeInfo {
                name: v.DisplayName().map_err(intern("Stimmenname"))?.to_string(),
                sprache: v.Language().map_err(intern("Stimmensprache"))?.to_string(),
            };
            Ok((v, info))
        })
        .collect()
}

/// Die installierten Windows-Stimmen für die Einstellungen.
pub fn stimmen() -> Result<Vec<StimmeInfo>, TonFehler> {
    Ok(alle()?.into_iter().map(|(_, info)| info).collect())
}

/// Synthetisiert die Abschnitte zu einem Mono-PCM16-Stück.
///
/// Keine passende Stimme → [`TonFehler::KeineStimme`]; das Frontend nimmt
/// dann die Browser-Stimme und zeigt das Banner (nie eine fremdsprachige
/// Ersatzstimme, Spec D9).
pub fn pcm_fuer(
    segmente: &[String],
    lang: &str,
    gespeicherte_stimme: &str,
    rate: f64,
) -> Result<(Vec<i16>, u32), TonFehler> {
    let alle = alle()?;
    let infos: Vec<StimmeInfo> = alle.iter().map(|(_, i)| i.clone()).collect();
    let idx =
        stimme::stimme_waehlen(gespeicherte_stimme, lang, &infos).ok_or(TonFehler::KeineStimme)?;
    let synth = SpeechSynthesizer::new().map_err(intern("Sprachausgabe"))?;
    synth
        .SetVoice(&alle[idx].0)
        .map_err(intern("Stimme setzen"))?;
    synth
        .Options()
        .and_then(|o| o.SetSpeakingRate(stimme::winrt_rate(rate)))
        .map_err(intern("Sprechtempo"))?;
    let mut teile = Vec::new();
    for text in segmente.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        teile.push(abschnitt(&synth, text)?);
    }
    let _ = synth.Close();
    stimme::fuege_segmente(teile, stimme::PAUSE_ZWISCHEN_SEGMENTEN_MS)
        .ok_or_else(|| TonFehler::Ungueltig("keine Sprech-Abschnitte".to_string()))
}

/// Ein Abschnitt → WAV → Mono-PCM16.
fn abschnitt(synth: &SpeechSynthesizer, text: &str) -> Result<(Vec<i16>, u32), TonFehler> {
    let strom = synth
        .SynthesizeTextToStreamAsync(&HSTRING::from(text))
        .and_then(|op| op.join())
        .map_err(intern("Synthese"))?;
    let groesse = strom.Size().map_err(intern("Synthese-Größe"))?;
    if groesse > MAX_WAV_BYTES {
        return Err(TonFehler::Ungueltig("Sprach-Abschnitt zu lang".to_string()));
    }
    let eingang = strom
        .GetInputStreamAt(0)
        .map_err(intern("Synthese-Strom"))?;
    let leser = DataReader::CreateDataReader(&eingang).map_err(intern("Synthese-Leser"))?;
    let geladen = leser
        .LoadAsync(groesse as u32)
        .and_then(|op| op.join())
        .map_err(intern("Synthese laden"))?;
    let mut bytes = vec![0u8; geladen as usize];
    leser
        .ReadBytes(&mut bytes)
        .map_err(intern("Synthese lesen"))?;
    pcm::wav_lesen(&bytes)
}
