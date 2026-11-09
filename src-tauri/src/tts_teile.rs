// Ansage in Teilen vertonen (Spec `ansage-in-teilen`): Die Ansage wird an den
// 350-ms-Blockpausen aus `src/io/ssmlRahmen.mjs` zerlegt, jeder Teil einzeln
// vertont und gecacht und danach wieder zu EINER WAV zusammengesetzt. So
// kostet „Feld 3." oder „Herrendoppel O40." beim Anbieter nur einmal.
//
// Hier liegt nur Reines und Testbares: Zerlegen des SSML, WAV lesen/schreiben,
// Randstille kürzen, Zusammensetzen. Die Anfragen stehen in `commands.rs`.

/// Blockpause, an der geschnitten wird (`BLOCK_PAUSE_MS` in `ssmlRahmen.mjs`).
pub const BLOCK_PAUSE: &str = r#"<break time="350ms"/>"#;
/// Stille zwischen zwei zusammengesetzten Teilen.
pub const PAUSE_MS: u32 = 350;
/// Abtastrate, in der alle Dienste die Teile liefern.
pub const RATE: u32 = 24_000;
/// Unter diesem Betrag gilt ein Sample als Stille (≈ −38 dBFS).
const STILLE_SCHWELLE: i16 = 400;
/// So viel Rand bleibt beim Kürzen stehen, damit kein Laut abgeschnitten wird.
const RAND_MS: u32 = 25;

/// Zerlegt ein Ansage-SSML in eigenständige Teil-Dokumente. `None`, wenn das
/// SSML keinen `<voice>`-Rahmen hat oder es weniger als zwei Teile mit Text
/// gibt — dann lohnt sich das Teilen nicht, die Ansage geht als Ganzes.
///
/// Ein `<prosody>`, das über einen Schnitt reicht, wird im Teil davor
/// geschlossen und im Teil danach wieder geöffnet. `<prosody>` ist im SSML der
/// Ansagen nie verschachtelt und umschließt nie ein `<lang>` (siehe
/// `mitTempo`), und Blockpausen stehen nie innerhalb eines `<lang>`.
pub fn teile_aus_ssml(ssml: &str) -> Option<Vec<String>> {
    let voice_auf = ssml.find("<voice")?;
    let innen_start = voice_auf + ssml[voice_auf..].find('>')? + 1;
    let innen_ende = ssml.rfind("</voice>")?;
    if innen_ende < innen_start {
        return None;
    }
    let kopf = &ssml[..innen_start];
    let fuss = &ssml[innen_ende..];
    let mut offen: Option<String> = None;
    let mut teile = Vec::new();
    for stueck in ssml[innen_start..innen_ende].split(BLOCK_PAUSE) {
        let mut teil = String::new();
        if let Some(tag) = &offen {
            teil.push_str(tag);
        }
        teil.push_str(stueck);
        offen = prosody_nach(stueck, offen);
        if offen.is_some() {
            teil.push_str("</prosody>");
        }
        if hat_text(&teil) {
            teile.push(format!("{kopf}{teil}{fuss}"));
        }
    }
    (teile.len() >= 2).then_some(teile)
}

/// Offenes `<prosody …>` nach dem Stück, ausgehend vom Zustand davor.
fn prosody_nach(stueck: &str, mut offen: Option<String>) -> Option<String> {
    let mut rest = stueck;
    while let Some(i) = rest.find('<') {
        let Some(j) = rest[i..].find('>') else {
            break;
        };
        let tag = &rest[i..i + j + 1];
        if tag.starts_with("</prosody") {
            offen = None;
        } else if tag.starts_with("<prosody") && !tag.ends_with("/>") {
            offen = Some(tag.to_string());
        }
        rest = &rest[i + j + 1..];
    }
    offen
}

/// Steht außerhalb der Tags ein Buchstabe oder eine Ziffer?
fn hat_text(s: &str) -> bool {
    let mut in_tag = false;
    s.chars().any(|c| {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag && c.is_alphanumeric() => return true,
            _ => {}
        }
        false
    })
}

/// Rohes PCM (16 bit, little endian, mono) → WAV.
pub fn wav_aus_pcm16(roh: &[u8], rate: u32) -> Vec<u8> {
    let samples: Vec<i16> = roh
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    crate::ansage_ton::pcm::wav_schreiben(&samples, rate)
}

/// Liest eine WAV (PCM 16 bit mono) → (Samples, Abtastrate). Eine Datengröße
/// von `0xFFFFFFFF` (Streaming-Kopf) heißt „bis zum Ende".
pub fn pcm_aus_wav(wav: &[u8]) -> Option<(Vec<i16>, u32)> {
    if wav.len() < 12 || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let mut rate = None;
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let groesse = u32::from_le_bytes(wav[pos + 4..pos + 8].try_into().ok()?) as usize;
        let start = pos + 8;
        let ende = start.saturating_add(groesse).min(wav.len());
        match id {
            b"fmt " => {
                let f = wav.get(start..start + 16)?;
                let format = u16::from_le_bytes([f[0], f[1]]);
                let kanaele = u16::from_le_bytes([f[2], f[3]]);
                let bits = u16::from_le_bytes([f[14], f[15]]);
                // 1 = PCM; 0xFFFE = WAVE_FORMAT_EXTENSIBLE (bei 16 bit mono
                // ebenfalls schlichtes PCM, manche Dienste schreiben ihn so).
                if !matches!(format, 1 | 0xFFFE) || kanaele != 1 || bits != 16 {
                    return None;
                }
                rate = Some(u32::from_le_bytes(f[4..8].try_into().ok()?));
            }
            b"data" => {
                let samples = wav[start..ende]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| i16::from_le_bytes(*b))
                    .collect();
                return Some((samples, rate?));
            }
            _ => {}
        }
        // Chunks sind auf gerade Längen aufgefüllt.
        pos = ende + (groesse & 1);
    }
    None
}

/// Schneidet Stille am Anfang und Ende ab und lässt einen kleinen Rand stehen.
pub fn randstille_kuerzen(samples: &[i16], rate: u32) -> &[i16] {
    let laut = |s: &i16| s.unsigned_abs() > STILLE_SCHWELLE.unsigned_abs();
    let (Some(erst), Some(letzt)) = (
        samples.iter().position(laut),
        samples.iter().rposition(laut),
    ) else {
        return &[];
    };
    let rand = (rate * RAND_MS / 1000) as usize;
    let von = erst.saturating_sub(rand);
    let bis = (letzt + 1 + rand).min(samples.len());
    &samples[von..bis]
}

/// Setzt die Teile (je eine WAV) mit genau `PAUSE_MS` Stille dazwischen zu
/// einer WAV zusammen. Fehler, wenn ein Teil keine lesbare WAV ist oder die
/// Abtastraten nicht übereinstimmen.
pub fn zusammensetzen(teile: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let mut rate_gesamt = None;
    let mut out: Vec<i16> = Vec::new();
    for (i, wav) in teile.iter().enumerate() {
        let (samples, rate) =
            pcm_aus_wav(wav).ok_or_else(|| format!("Teil {} ist keine lesbare WAV", i + 1))?;
        match rate_gesamt {
            None => rate_gesamt = Some(rate),
            Some(r) if r != rate => {
                return Err(format!("Teil {} hat {rate} Hz statt {r} Hz", i + 1));
            }
            Some(_) => {}
        }
        let kern = randstille_kuerzen(&samples, rate);
        if kern.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.extend(std::iter::repeat_n(0i16, (rate * PAUSE_MS / 1000) as usize));
        }
        out.extend_from_slice(kern);
    }
    let rate = rate_gesamt.ok_or("Keine Teile")?;
    if out.is_empty() {
        return Err("Alle Teile sind stumm".to_string());
    }
    Ok(crate::ansage_ton::pcm::wav_schreiben(&out, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KOPF: &str = r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="de-DE"><voice name="de-DE-X">"#;
    const FUSS: &str = "</voice></speak>";

    fn doc(innen: &str) -> String {
        format!("{KOPF}{innen}{FUSS}")
    }

    fn innen(teil: &str) -> &str {
        teil.strip_prefix(KOPF).unwrap().strip_suffix(FUSS).unwrap()
    }

    #[test]
    fn echte_ansage_mit_lang_und_tempo_wird_sauber_geteilt() {
        // Form wie `ssmlDokument` sie baut (Test in tts_text.rs).
        let ssml = doc(concat!(
            r#"<prosody rate="-20%">Feld drei.<break time="350ms"/>Herreneinzel.<break time="350ms"/><break time="120ms"/></prosody>"#,
            r#"<lang xml:lang="zh-CN"><prosody rate="-20%">Zhang Yue</prosody></lang>"#,
            r#"<prosody rate="-20%"><break time="120ms"/>.<break time="350ms"/>gegen Tomm <break time="120ms"/></prosody>"#,
            r#"<lang xml:lang="vi-VN"><prosody rate="-20%">Nguyen</prosody></lang>"#,
            r#"<prosody rate="-20%"><break time="120ms"/>.<break time="350ms"/>Feld drei.</prosody>"#,
        ));
        let teile = teile_aus_ssml(&ssml).unwrap();
        let innen: Vec<&str> = teile.iter().map(|t| innen(t)).collect();
        assert_eq!(
            innen,
            [
                r#"<prosody rate="-20%">Feld drei.</prosody>"#,
                r#"<prosody rate="-20%">Herreneinzel.</prosody>"#,
                r#"<prosody rate="-20%"><break time="120ms"/></prosody><lang xml:lang="zh-CN"><prosody rate="-20%">Zhang Yue</prosody></lang><prosody rate="-20%"><break time="120ms"/>.</prosody>"#,
                r#"<prosody rate="-20%">gegen Tomm <break time="120ms"/></prosody><lang xml:lang="vi-VN"><prosody rate="-20%">Nguyen</prosody></lang><prosody rate="-20%"><break time="120ms"/>.</prosody>"#,
                r#"<prosody rate="-20%">Feld drei.</prosody>"#,
            ]
        );
        // Gleiche Teile ergeben gleiche Dokumente → gleicher Cache-Eintrag.
        assert_eq!(teile[0], teile[4]);
        // Kein Teil enthält eine Blockpause oder ein <prosody> um ein <lang>.
        for t in &teile {
            assert!(!t.contains(BLOCK_PAUSE));
            assert!(!t.contains("%\"><lang"), "{t}");
        }
        // Zusammen dieselben Wörter wie das Ganze, und das Tempo bleibt je Teil.
        let text = |s: &str| crate::tts_text::ssml_zu_text(s, crate::tts_text::Pausen::Satzzeichen);
        let woerter = |s: String| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        };
        let gesamt: String = teile.iter().map(|t| woerter(text(t).text)).collect();
        assert_eq!(gesamt, woerter(text(&ssml).text));
        for t in &teile {
            assert!((text(t).speed - 0.8).abs() < 1e-6, "{t}");
        }
    }

    #[test]
    fn ohne_tempo_und_ohne_rahmen() {
        let teile = teile_aus_ssml(&doc(r#"Feld 1.<break time="350ms"/>Anna gegen Bo."#)).unwrap();
        assert_eq!(innen(&teile[1]), "Anna gegen Bo.");
        assert_eq!(
            teile_aus_ssml("<speak>Feld 1.</speak>"),
            None,
            "ohne <voice>"
        );
        assert_eq!(teile_aus_ssml(&doc("Feld 1.")), None, "nur ein Teil");
        // Teile ohne Text (nur Satzzeichen/Pausen) fallen weg.
        assert_eq!(
            teile_aus_ssml(&doc(
                r#"Feld 1.<break time="350ms"/>.<break time="350ms"/>"#
            )),
            None
        );
    }

    fn ton(n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| if i % 2 == 0 { 8000 } else { -8000 })
            .collect()
    }

    #[test]
    fn wav_rundlauf_und_streaming_kopf() {
        let s = ton(10);
        let wav = crate::ansage_ton::pcm::wav_schreiben(&s, RATE);
        assert_eq!(pcm_aus_wav(&wav), Some((s.clone(), RATE)));
        let mut strom = wav.clone();
        strom[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(pcm_aus_wav(&strom), Some((s.clone(), RATE)));
        assert_eq!(pcm_aus_wav(b"ID3 kein wav"), None);
        let roh: Vec<u8> = s.iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(wav_aus_pcm16(&roh, RATE), wav);
    }

    #[test]
    fn randstille_wird_bis_auf_den_rand_gekuerzt() {
        let rand = (RATE * RAND_MS / 1000) as usize;
        let mut s = vec![0i16; 5000];
        s.extend(ton(100));
        s.extend(vec![10i16; 3000]);
        let k = randstille_kuerzen(&s, RATE);
        assert_eq!(k.len(), rand + 100 + rand);
        assert!(randstille_kuerzen(&[0, 5, -5], RATE).is_empty());
    }

    #[test]
    fn zusammensetzen_mit_genau_350_ms_pause() {
        let rand = (RATE * RAND_MS / 1000) as usize;
        let pause = (RATE * PAUSE_MS / 1000) as usize;
        let teil = |n| {
            let mut s = vec![0i16; 2000];
            s.extend(ton(n));
            s.extend(vec![0i16; 2000]);
            crate::ansage_ton::pcm::wav_schreiben(&s, RATE)
        };
        let wav = zusammensetzen(&[teil(100), teil(50)]).unwrap();
        let (s, rate) = pcm_aus_wav(&wav).unwrap();
        assert_eq!(rate, RATE);
        assert_eq!(s.len(), (rand + 100 + rand) + pause + (rand + 50 + rand));
        // Zwischen den Tönen liegt nur Stille: Rand + Pause + Rand.
        let erster_ton_ende = rand + 100;
        let still = &s[erster_ton_ende..erster_ton_ende + rand + pause + rand];
        assert!(still.iter().all(|x| *x == 0));
        let fremd = crate::ansage_ton::pcm::wav_schreiben(&ton(10), 16_000);
        assert!(zusammensetzen(&[teil(10), fremd]).is_err());
        assert!(zusammensetzen(&[b"kaputt".to_vec()]).is_err());
    }

    #[test]
    fn wav_mit_erweitertem_kopf_wird_gelesen() {
        let mut wav = wav_aus_pcm16(&[1, 0, 2, 0], 24_000);
        // Format-Feld im fmt-Chunk auf WAVE_FORMAT_EXTENSIBLE setzen.
        let fmt = wav.windows(4).position(|w| w == b"fmt ").unwrap() + 8;
        wav[fmt..fmt + 2].copy_from_slice(&0xFFFEu16.to_le_bytes());
        assert_eq!(pcm_aus_wav(&wav), Some((vec![1, 2], 24_000)));
        wav[fmt..fmt + 2].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(pcm_aus_wav(&wav), None, "Gleitkomma bleibt abgelehnt");
    }
}
