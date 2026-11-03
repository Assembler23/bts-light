// Gemeinsame Umwandlung des Ansage-SSML in Klartext für die Cloud-Dienste
// ohne (volles) SSML: ElevenLabs, OpenAI, Gemini (Spec `ansage-elevenlabs`).
//
// Das Frontend baut für jeden Dienst dasselbe Azure-SSML. Hier fallen alle
// Tags weg (der Inhalt bleibt — bei `<phoneme>` also der geschriebene Name),
// das Tempo wird aus `<prosody rate>` gelesen und die Pausen werden je Dienst
// abgebildet: als `<break>`-Tag, wo er dokumentiert ist, sonst als Satzzeichen,
// damit kein Tag laut vorgelesen wird.

/// Ab dieser Pausenlänge wird ohne Break-Tags ein Satzende gesetzt; kürzere
/// Pausen (die 120 ms um Namen) entfallen dann ganz.
const SATZPAUSE_MS: u32 = 300;

/// Wie ein Dienst Pausen versteht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pausen {
    /// `<break time="0.35s" />` im Text (ElevenLabs Multilingual v2).
    BreakTag,
    /// Satzpunkt statt Pause.
    Satzzeichen,
}

/// Ergebnis der Umwandlung.
#[derive(Debug, Clone, PartialEq)]
pub struct TtsText {
    pub text: String,
    /// Tempo aus `<prosody rate>`, 1.0 ohne Angabe; noch NICHT auf die
    /// Spanne eines Dienstes begrenzt.
    pub speed: f32,
    /// ISO-639-1 aus `xml:lang` des `<speak>` („de"/„en").
    pub sprache: &'static str,
}

enum Stueck {
    Text(String),
    Pause(u32),
}

/// SSML (wie es `ssmlDokument` baut) → Klartext. Rein & testbar.
pub fn ssml_zu_text(ssml: &str, pausen: Pausen) -> TtsText {
    let mut stuecke: Vec<Stueck> = Vec::new();
    let mut speed: Option<f32> = None;
    let mut sprache = "de";
    let mut rest = ssml;
    while !rest.is_empty() {
        if let Some(start) = rest.find('<') {
            if start > 0 {
                stuecke.push(Stueck::Text(entities_aufloesen(&rest[..start])));
            }
            let Some(ende) = rest[start..].find('>') else {
                // Kaputtes Markup: Rest als Text, ohne die spitze Klammer.
                stuecke.push(Stueck::Text(entities_aufloesen(&rest[start + 1..])));
                break;
            };
            let tag = &rest[start + 1..start + ende];
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("");
            match name {
                "break" => {
                    if let Some(ms) = attribut(tag, "time").and_then(|t| zeit_ms(&t)) {
                        stuecke.push(Stueck::Pause(ms));
                    }
                }
                "prosody" if speed.is_none() && !tag.starts_with('/') => {
                    speed = attribut(tag, "rate").and_then(|r| rate_zu_speed(&r));
                }
                "speak" if !tag.starts_with('/') => {
                    if let Some(l) = attribut(tag, "xml:lang") {
                        sprache = if l.to_ascii_lowercase().starts_with("en") {
                            "en"
                        } else {
                            "de"
                        };
                    }
                }
                _ => {}
            }
            rest = &rest[start + ende + 1..];
        } else {
            stuecke.push(Stueck::Text(entities_aufloesen(rest)));
            break;
        }
    }
    let mut text = String::new();
    for s in stuecke {
        match s {
            Stueck::Text(t) => text.push_str(&t),
            Stueck::Pause(ms) if pausen == Pausen::Satzzeichen => {
                if ms >= SATZPAUSE_MS {
                    let ohne = text.trim_end().len();
                    text.truncate(ohne);
                    if !text.is_empty() && !text.ends_with(['.', '!', '?', ':']) {
                        text.push('.');
                    }
                }
                text.push(' ');
            }
            Stueck::Pause(ms) => {
                text.push_str(&format!(" <break time=\"{}s\" /> ", sekunden(ms)));
            }
        }
    }
    TtsText {
        text: leerraum_falten(&text),
        speed: speed.unwrap_or(1.0),
        sprache,
    }
}

/// `"0.35"` aus 350 ms — ohne überflüssige Nullen.
fn sekunden(ms: u32) -> String {
    let s = format!("{:.2}", f64::from(ms) / 1000.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Leerraum auf ein Zeichen falten und vor Satzzeichen streichen — die
/// 120-ms-Pause hinter einem Namen stünde sonst als „Yue ." im Text.
fn leerraum_falten(s: &str) -> String {
    let mut t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for z in [".", ",", "!", "?", ":"] {
        t = t.replace(&format!(" {z}"), z);
    }
    t
}

/// Attributwert aus einem Tag-Inhalt (`break time="350ms"`), Anführungszeichen
/// einfach oder doppelt.
fn attribut(tag: &str, name: &str) -> Option<String> {
    let mut suche = tag;
    while let Some(pos) = suche.find(name) {
        let davor_ok = pos == 0 || suche[..pos].ends_with(char::is_whitespace);
        let nach = suche[pos + name.len()..].trim_start();
        if davor_ok {
            if let Some(wert) = nach.strip_prefix('=') {
                let wert = wert.trim_start();
                let q = wert.chars().next()?;
                if q == '"' || q == '\'' {
                    let inhalt = &wert[1..];
                    let ende = inhalt.find(q)?;
                    return Some(inhalt[..ende].to_string());
                }
            }
        }
        suche = &suche[pos + name.len()..];
    }
    None
}

/// `"350ms"` / `"0.35s"` → Millisekunden (gedeckelt auf 3 s, mehr nimmt
/// ElevenLabs nicht).
fn zeit_ms(t: &str) -> Option<u32> {
    let t = t.trim();
    let ms = if let Some(z) = t.strip_suffix("ms") {
        z.trim().parse::<f64>().ok()?
    } else {
        t.strip_suffix('s')?.trim().parse::<f64>().ok()? * 1000.0
    };
    (ms.is_finite() && ms > 0.0).then(|| ms.min(3000.0).round() as u32)
}

/// `"-20%"` → 0.8, `"+10%"` → 1.1. Andere Formen (Wörter wie „slow") → `None`.
fn rate_zu_speed(r: &str) -> Option<f32> {
    let p: f32 = r.trim().strip_suffix('%')?.trim().parse().ok()?;
    let v = 1.0 + p / 100.0;
    v.is_finite().then_some(v)
}

/// XML-Entities zurück in Zeichen. Spitze Klammern fliegen danach raus: Ein
/// Name darf bei ElevenLabs nie als `<break>`-Tag gelesen werden.
fn entities_aufloesen(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace(['<', '>'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: Pausen = Pausen::BreakTag;
    const V3: Pausen = Pausen::Satzzeichen;

    fn doc(inner: &str) -> String {
        format!(
            r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="de-DE"><voice name="de-DE-SeraphinaMultilingualNeural">{inner}</voice></speak>"#
        )
    }

    #[test]
    fn v2_behaelt_pausen_als_break_tag_und_liest_das_tempo() {
        let s = doc(
            r#"<prosody rate="-20%">Feld 3</prosody><break time="350ms"/><prosody rate="-20%">Herreneinzel</prosody>"#,
        );
        let t = ssml_zu_text(&s, V2);
        assert_eq!(t.text, r#"Feld 3 <break time="0.35s" /> Herreneinzel"#);
        assert!((t.speed - 0.8).abs() < 1e-6);
        assert_eq!(t.sprache, "de");
    }

    #[test]
    fn v3_macht_aus_langen_pausen_ein_satzende_und_laesst_kurze_weg() {
        let s = doc(
            r#"Feld 3<break time="350ms"/>Herreneinzel.<break time="350ms"/>Anna<break time="120ms"/><lang xml:lang="vi-VN">Nguyen</lang>"#,
        );
        let t = ssml_zu_text(&s, V3);
        assert_eq!(t.text, "Feld 3. Herreneinzel. Anna Nguyen");
        assert!(!t.text.contains("break"), "v3 kennt keine Break-Tags");
    }

    #[test]
    fn echte_app_ausgabe_mit_abgesetzten_namen() {
        // Wörtlich aus `prepareAnnouncement` (ElevenLabs, 02.11.2026).
        let s = doc(
            r#"<prosody rate="-20%">Feld drei.<break time="350ms"/>Herreneinzel.<break time="350ms"/><break time="120ms"/></prosody><lang xml:lang="zh-CN"><prosody rate="-20%">Zhang Yue</prosody></lang><prosody rate="-20%"><break time="120ms"/>.<break time="350ms"/>gegen Tomm <break time="120ms"/></prosody><lang xml:lang="vi-VN"><prosody rate="-20%">Nguyen</prosody></lang><prosody rate="-20%"><break time="120ms"/>.<break time="350ms"/>Feld drei.</prosody>"#,
        );
        assert_eq!(
            ssml_zu_text(&s, V3).text,
            "Feld drei. Herreneinzel. Zhang Yue. gegen Tomm Nguyen. Feld drei."
        );
        let v2 = ssml_zu_text(&s, V2).text;
        assert!(
            v2.starts_with(r#"Feld drei. <break time="0.35s" /> Herreneinzel."#),
            "{v2}"
        );
        assert!(
            v2.contains(r#"<break time="0.12s" /> Zhang Yue <break time="0.12s" />."#),
            "{v2}"
        );
    }

    #[test]
    fn lang_und_phoneme_fallen_weg_der_name_bleibt() {
        let s = doc(
            r#"<lang xml:lang="vi-VN"><prosody rate="-20%">Chử Thị Linh</prosody></lang> gegen <phoneme alphabet="ipa" ph="ˈdʒaŋ">Zhang Yue</phoneme>"#,
        );
        assert_eq!(ssml_zu_text(&s, V2).text, "Chử Thị Linh gegen Zhang Yue");
    }

    #[test]
    fn entities_werden_aufgeloest_aber_nie_zu_tags() {
        let s = doc("Tom &amp; Jerry &lt;break time=&quot;3s&quot;/&gt; O&apos;Neil");
        let t = ssml_zu_text(&s, V2);
        assert_eq!(t.text, "Tom & Jerry break time=\"3s\"/ O'Neil");
        assert!(!t.text.contains('<'));
    }

    #[test]
    fn tempo_kommt_roh_aus_prosody() {
        assert_eq!(
            ssml_zu_text(&doc(r#"<prosody rate="-50%">x</prosody>"#), V2).speed,
            0.5
        );
        assert_eq!(
            ssml_zu_text(&doc(r#"<prosody rate="+50%">x</prosody>"#), V2).speed,
            1.5
        );
        assert_eq!(
            ssml_zu_text(&doc("x"), V2).speed,
            1.0,
            "ohne prosody = normal"
        );
    }

    #[test]
    fn englischer_rahmen_ergibt_englisch() {
        let s = r#"<speak version="1.0" xml:lang="en-US"><voice name="v">Court 3</voice></speak>"#;
        assert_eq!(ssml_zu_text(s, V2).sprache, "en");
    }

    #[test]
    fn zeiten_und_sekunden() {
        assert_eq!(zeit_ms("350ms"), Some(350));
        assert_eq!(zeit_ms("0.12s"), Some(120));
        assert_eq!(zeit_ms("10s"), Some(3000));
        assert_eq!(zeit_ms("lang"), None);
        assert_eq!(sekunden(350), "0.35");
        assert_eq!(sekunden(1000), "1");
        assert_eq!(sekunden(120), "0.12");
    }
}

/// Gemeinsame Bausteine der Cloud-Dienste ohne SSML (Cache + HTTP-Fehler).
pub mod netz {
    use std::path::Path;

    /// Liest eine gecachte Vertonung; `None`, wenn keine (oder leer).
    pub fn cache_holen(dir: &Path, schluessel: &str) -> Option<Vec<u8>> {
        let bytes = std::fs::read(dir.join(crate::azure_tts::cache_name(schluessel))).ok()?;
        (!bytes.is_empty()).then_some(bytes)
    }

    /// Legt eine Vertonung best-effort im Cache ab.
    pub fn cache_ablegen(dir: &Path, schluessel: &str, bytes: &[u8]) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(crate::azure_tts::cache_name(schluessel)), bytes);
    }

    /// Fehlermeldung aus einer HTTP-Antwort ≠ 2xx, Body auf 200 Zeichen
    /// gekürzt (die Dienste schicken lange JSON-Diagnosen, nie den Schlüssel).
    pub async fn http_fehler(dienst: &str, resp: reqwest::Response) -> String {
        let code = resp.status();
        let body: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        format!("{dienst} HTTP {code}: {body}")
    }

    /// HTTP-Client mit Zeitlimit — offline soll die Ersatzstimme schnell
    /// übernehmen, nicht erst nach einer halben Minute.
    pub fn client(sekunden: u64) -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(sekunden))
            .build()
            .map_err(|e| format!("HTTP-Client: {e}"))
    }

    /// Stimmen-Namen/IDs landen im URL-Pfad oder JSON: nur ASCII-Buchstaben
    /// und Ziffern, höchstens 64 Zeichen.
    pub fn stimme_gueltig(voice: &str) -> bool {
        !voice.is_empty() && voice.len() <= 64 && voice.chars().all(|c| c.is_ascii_alphanumeric())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn stimmen_id_darf_den_pfad_nicht_verlassen() {
            assert!(super::stimme_gueltig("EXAVITQu4vr4xnSDxMaL"));
            assert!(super::stimme_gueltig("Kore"));
            assert!(!super::stimme_gueltig("../user"));
            assert!(!super::stimme_gueltig("a?b=c"));
            assert!(!super::stimme_gueltig(""));
        }
    }
}
