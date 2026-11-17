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

/// Welches Audio ein Dienst liefern soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Ganze Ansage: kompakt (MP3; Gemini liefert immer WAV).
    Mp3,
    /// Teil einer geteilten Ansage (Spec `ansage-in-teilen`): WAV, 24 kHz,
    /// mono, 16 bit — so lassen sich die Teile ohne Decoder zusammensetzen.
    Wav24,
}

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

/// Gemeinsame Bausteine der Cloud-Dienste: Cache, HTTP-Client und die
/// Fehlerklassen der Ausfall-Erkennung (Spec `ansage-offline-stimme`, E1).
pub mod netz {
    use std::path::Path;
    use std::sync::OnceLock;
    use std::time::Duration;

    /// Frist für Verbindungsaufbau samt TLS. Gemessen (ADR 0105): Im guten
    /// Netz steht die Verbindung nach höchstens 0,5 s. Sie deckt **nicht** die
    /// Gegenstelle, die eine Anfrage annimmt und dann schweigt — dafür gibt es
    /// die harte Frist um die ganze Vertonung (`tts_netzlage::HARTE_FRIST`).
    pub const VERBINDUNGSFRIST: Duration = Duration::from_millis(1500);

    /// Gesamtfrist einer einzelnen Anfrage, wo keine harte Frist von außen
    /// greift („Dienste prüfen").
    pub const ANFRAGE_FRIST: Duration = Duration::from_secs(10);

    /// Offene Verbindungen nur kurz aufheben: Die Teile **einer** Ansage
    /// teilen sich den Aufbau, die nächste Ansage baut neu auf. So scheitert
    /// sie im Ausfall schon an der Verbindungsfrist statt erst an der harten
    /// Frist auf einer still gestorbenen Leitung.
    const LEERLAUF: Duration = Duration::from_secs(10);

    /// Fehlerklasse einer Vertonung (ADR 0105). Sie entscheidet, ob der
    /// Ausfall-Merker gesetzt wird und ob ein zweiter Versuch lohnt.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Klasse {
        /// Verbindung, Zeitüberschreitung, harte Frist, HTTP 5xx.
        Netz,
        /// HTTP 429.
        Drossel,
        /// HTTP 401/403.
        Zugang,
        /// Übrige HTTP-Fehler und unlesbare Antworten.
        Inhalt,
    }

    impl Klasse {
        /// Festes Wort fürs Log.
        pub fn wort(self) -> &'static str {
            match self {
                Self::Netz => "netz",
                Self::Drossel => "drossel",
                Self::Zugang => "zugang",
                Self::Inhalt => "inhalt",
            }
        }
    }

    /// Fehler einer Vertonung, an der Quelle eingeordnet.
    ///
    /// `Display` liefert den Text für Banner **und** Log: ohne Adresse, ohne
    /// Antworttext des Dienstes. Die HTTP-Bibliothek nennt in ihren Fehlern
    /// die angefragte Adresse und bei fremdem Zertifikat (Anmeldeseite eines
    /// Hotspots) dessen Namen; manche Dienste wiederholen in der Antwort Teile
    /// des Schlüssels. Beides bleibt in `meldung` und geht nie ins Log.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TtsFehler {
        pub klasse: Klasse,
        /// Festes Wort für die Fehlerart.
        pub kennwort: &'static str,
        /// Rohtext samt Antwort des Dienstes — nur für „Dienste prüfen", wo
        /// die Turnierleitung den Grund am eigenen Bildschirm sehen soll.
        pub meldung: String,
        anzeige: String,
    }

    impl std::fmt::Display for TtsFehler {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.anzeige)
        }
    }

    impl TtsFehler {
        /// Die Anfrage selbst taugt nicht (Stimme, leerer Text, leeres Audio):
        /// ein zweiter Versuch mit der ganzen Ansage kann gelingen.
        pub fn inhalt(meldung: impl Into<String>) -> Self {
            let meldung = meldung.into();
            Self {
                klasse: Klasse::Inhalt,
                kennwort: "inhalt",
                anzeige: meldung.clone(),
                meldung,
            }
        }

        /// Fehler beim Senden: Es kam keine Antwort.
        pub fn anfrage(dienst: &str, vorsatz: &str, e: &reqwest::Error) -> Self {
            let (klasse, kennwort, text) = if e.is_builder() {
                (
                    Klasse::Inhalt,
                    "anfrage-ungueltig",
                    "Anfrage an den Sprachdienst ungültig",
                )
            } else if e.is_connect() {
                (
                    Klasse::Netz,
                    "verbindung",
                    "keine Verbindung zum Sprachdienst",
                )
            } else if e.is_timeout() {
                (Klasse::Netz, "zeit", "Sprachdienst antwortet nicht")
            } else {
                (
                    Klasse::Netz,
                    "anfrage",
                    "Anfrage an den Sprachdienst gescheitert",
                )
            };
            Self {
                klasse,
                kennwort,
                meldung: format!("{vorsatz}: {e}"),
                anzeige: format!("{dienst}: {text}"),
            }
        }

        /// Fehler beim Lesen des Antwortkörpers: Die Leitung riss ab oder die
        /// Frist lief ab. (Unlesbarer **Inhalt** ist [`TtsFehler::inhalt`].)
        pub fn antwort(dienst: &str, vorsatz: &str, e: &reqwest::Error) -> Self {
            let (kennwort, text) = if e.is_timeout() {
                ("zeit", "Sprachdienst antwortet nicht")
            } else {
                ("antwort", "Antwort des Sprachdienstes abgerissen")
            };
            Self {
                klasse: Klasse::Netz,
                kennwort,
                meldung: format!("{vorsatz}: {e}"),
                anzeige: format!("{dienst}: {text}"),
            }
        }

        /// Die harte Frist um die ganze Vertonung ist abgelaufen.
        pub fn frist(dienst: &str) -> Self {
            let anzeige = format!("{dienst}: keine Antwort innerhalb der Frist");
            Self {
                klasse: Klasse::Netz,
                kennwort: "frist",
                meldung: anzeige.clone(),
                anzeige,
            }
        }

        /// Der Ausfall-Merker ist gesetzt: Es wurde gar nicht erst angefragt.
        pub fn gesperrt(dienst: &str) -> Self {
            let anzeige = format!(
                "{dienst}: zurzeit nicht erreichbar, die Ersatzstimme spricht, bis der Dienst \
                 wieder antwortet"
            );
            Self {
                klasse: Klasse::Netz,
                kennwort: "gesperrt",
                meldung: anzeige.clone(),
                anzeige,
            }
        }

        /// Stellt der Rohmeldung einen Ort voran („Teil 2").
        pub fn mit_vorsatz(mut self, vorsatz: &str) -> Self {
            self.meldung = format!("{vorsatz}: {}", self.meldung);
            self
        }
    }

    /// Klasse, Kennwort und Anzeigetext zu einem HTTP-Status ≠ 2xx.
    pub fn status_einordnen(code: u16) -> (Klasse, &'static str, &'static str) {
        match code {
            500..=599 => (Klasse::Netz, "http-5xx", "Störung beim Sprachdienst"),
            429 => (
                Klasse::Drossel,
                "drossel",
                "zu viele Anfragen oder Kontingent aufgebraucht",
            ),
            401 | 403 => (
                Klasse::Zugang,
                "zugang",
                "Schlüssel ungültig oder ohne Berechtigung",
            ),
            400..=499 => (Klasse::Inhalt, "http-4xx", "Anfrage abgelehnt"),
            _ => (Klasse::Inhalt, "http-sonst", "unerwartete Antwort"),
        }
    }

    /// Fehler aus einer HTTP-Antwort ≠ 2xx. Der Antworttext (auf 200 Zeichen
    /// gekürzt) steht nur in der Rohmeldung.
    pub async fn status_fehler(dienst: &str, resp: reqwest::Response) -> TtsFehler {
        let code = resp.status();
        let body: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        TtsFehler::aus_status(dienst, code.as_u16(), &body)
    }

    impl TtsFehler {
        /// Fehler zu einem HTTP-Status ≠ 2xx; `body` nur in der Rohmeldung.
        pub fn aus_status(dienst: &str, code: u16, body: &str) -> Self {
            let (klasse, kennwort, text) = status_einordnen(code);
            let grund = reqwest::StatusCode::from_u16(code)
                .ok()
                .and_then(|s| s.canonical_reason())
                .unwrap_or("");
            Self {
                klasse,
                kennwort,
                meldung: format!("{dienst} HTTP {code} {grund}: {body}"),
                anzeige: format!("{dienst}: {text} (HTTP {code})"),
            }
        }
    }

    /// Der eine HTTP-Client aller Sprachdienste, mit Verbindungsfrist. Die
    /// Gesamtfrist setzt jede Anfrage selbst.
    pub fn client() -> Result<&'static reqwest::Client, TtsFehler> {
        static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
        if let Some(c) = CLIENT.get() {
            return Ok(c);
        }
        let neu = reqwest::Client::builder()
            .connect_timeout(VERBINDUNGSFRIST)
            .pool_idle_timeout(LEERLAUF)
            // Schlüssel stehen in Kopfzeilen: keiner Umleitung folgen.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| TtsFehler::inhalt(format!("HTTP-Client: {e}")))?;
        Ok(CLIENT.get_or_init(|| neu))
    }

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

    /// Stimmen-Namen/IDs landen im URL-Pfad oder JSON: nur ASCII-Buchstaben
    /// und Ziffern, höchstens 64 Zeichen.
    pub fn stimme_gueltig(voice: &str) -> bool {
        !voice.is_empty() && voice.len() <= 64 && voice.chars().all(|c| c.is_ascii_alphanumeric())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::badhub::testhilfe::{starten, Antwort};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        #[test]
        fn stimmen_id_darf_den_pfad_nicht_verlassen() {
            assert!(super::stimme_gueltig("EXAVITQu4vr4xnSDxMaL"));
            assert!(super::stimme_gueltig("Kore"));
            assert!(!super::stimme_gueltig("../user"));
            assert!(!super::stimme_gueltig("a?b=c"));
            assert!(!super::stimme_gueltig(""));
        }

        /// Kriterien 1 und 5: Nur Störungen der Leitung und des Dienstes sind
        /// „Netz"; Drossel, Zugang und der Rest bleiben eigene Klassen.
        #[test]
        fn http_status_wird_nach_der_tabelle_der_spec_eingeordnet() {
            for code in [500, 502, 503, 504, 599] {
                assert_eq!(status_einordnen(code).0, Klasse::Netz, "{code}");
            }
            assert_eq!(status_einordnen(429).0, Klasse::Drossel);
            assert_eq!(status_einordnen(401).0, Klasse::Zugang);
            assert_eq!(status_einordnen(403).0, Klasse::Zugang);
            for code in [400, 402, 404, 408, 422, 301] {
                assert_eq!(status_einordnen(code).0, Klasse::Inhalt, "{code}");
            }
        }

        /// Der Antworttext eines Dienstes kann Teile des Schlüssels nennen: Er
        /// bleibt in der Rohmeldung, Banner und Log sehen nur den Status.
        #[tokio::test]
        async fn status_fehler_zeigt_den_antworttext_nicht_an() {
            let mock = starten(vec![Antwort::http(
                "401 Unauthorized",
                "Incorrect API key provided: sk-geheim",
            )])
            .await;
            let resp = reqwest::Client::new().get(&mock.url).send().await.unwrap();
            let f = status_fehler("OpenAI", resp).await;
            assert_eq!(f.klasse, Klasse::Zugang);
            assert_eq!(f.kennwort, "zugang");
            assert!(f.meldung.starts_with("OpenAI HTTP 401"), "{}", f.meldung);
            assert!(f.meldung.contains("sk-geheim"));
            let anzeige = f.to_string();
            assert!(anzeige.contains("HTTP 401"), "{anzeige}");
            assert!(!anzeige.contains("sk-geheim"), "{anzeige}");
        }

        /// Ein geschlossener Port ist „Netz" — und die Anzeige nennt die
        /// Adresse nicht.
        #[tokio::test]
        async fn abgelehnte_verbindung_ist_netz_ohne_adresse_in_der_anzeige() {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let adresse = l.local_addr().unwrap();
            drop(l);
            let e = reqwest::Client::new()
                .get(format!("http://{adresse}/x"))
                .send()
                .await
                .unwrap_err();
            let f = TtsFehler::anfrage("OpenAI", "OpenAI-Anfrage", &e);
            assert_eq!((f.klasse, f.kennwort), (Klasse::Netz, "verbindung"));
            assert!(f.meldung.starts_with("OpenAI-Anfrage: "), "{}", f.meldung);
            assert!(!f.to_string().contains("127.0.0.1"), "{f}");
        }

        /// Die Gegenstelle nimmt an und schweigt: Zeitüberschreitung = „Netz".
        #[tokio::test]
        async fn schweigende_gegenstelle_ist_netz() {
            let mock = starten(vec![Antwort::Haengt]).await;
            let e = reqwest::Client::new()
                .get(&mock.url)
                .timeout(Duration::from_millis(200))
                .send()
                .await
                .unwrap_err();
            let f = TtsFehler::anfrage("Gemini", "Gemini-Anfrage", &e);
            assert_eq!((f.klasse, f.kennwort), (Klasse::Netz, "zeit"));
        }

        /// Reißt die Leitung mitten in der Antwort ab, ist das „Netz" — nicht
        /// „unlesbare Antwort".
        #[tokio::test]
        async fn abgerissene_antwort_ist_netz() {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let adresse = l.local_addr().unwrap();
            tokio::spawn(async move {
                let (mut sock, _) = l.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nabc")
                    .await;
            });
            let resp = reqwest::Client::new()
                .get(format!("http://{adresse}/x"))
                .send()
                .await
                .unwrap();
            let e = resp.bytes().await.unwrap_err();
            let f = TtsFehler::antwort("Azure", "Azure-TTS-Antwort", &e);
            assert_eq!((f.klasse, f.kennwort), (Klasse::Netz, "antwort"));
        }

        #[test]
        fn frist_und_sperre_sind_netz_mit_eigenem_kennwort() {
            let f = TtsFehler::frist("Azure");
            assert_eq!((f.klasse, f.kennwort), (Klasse::Netz, "frist"));
            let g = TtsFehler::gesperrt("Azure");
            assert_eq!((g.klasse, g.kennwort), (Klasse::Netz, "gesperrt"));
            let i = TtsFehler::inhalt("OpenAI: leerer Ansagetext").mit_vorsatz("Teil 2");
            assert_eq!(i.klasse, Klasse::Inhalt);
            assert_eq!(i.meldung, "Teil 2: OpenAI: leerer Ansagetext");
            assert_eq!(i.to_string(), "OpenAI: leerer Ansagetext");
        }
    }
}
