//! Ansage-Ton aus dem Kern (Spec `ansage-ausgabegeraet`, ADR 0065/0066).
//!
//! Das WebView rechnet nur noch (Gong rendern, Azure-MP3 dekodieren), der
//! Kern gibt aus: über WASAPI auf einem eigenen Audio-Thread, mit dem
//! Windows-Standardgerät **je Stück neu aufgelöst** — ein dauerhaft offener
//! Strom bleibt nach dem Abziehen des Geräts still (Senior-Cup 31.10.2026).
//! Die Windows-Stimme (OneCore) synthetisiert ebenfalls hier.
//!
//! Die Warteschlange bleibt in TypeScript: Jeder Aufruf spielt **ein**
//! Stück und kehrt nach dessen Ende zurück. Unter Linux meldet jede
//! Abspiel-Funktion [`TonFehler::NichtUnterstuetzt`], und das Frontend
//! bleibt ohne Banner beim alten Weg.

pub mod pcm;
pub mod stimme;

#[cfg(windows)]
mod windows;

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

pub use stimme::StimmeInfo;

/// Deckel der Windows-Stimme: Die Abschnitte sind schon gekappt (32 ×
/// 1000 Zeichen), aber eine lange Freitext-Ansage darf länger als die 60 s
/// der Raw-IPC-Schnittstelle sein. Zwei Minuten halten den Speicher klein
/// (Mono-f32 bei 48 kHz ≈ 23 MB).
pub const MAX_SEKUNDEN_STIMME: u32 = 120;

/// Was gespielt wird — entscheidet, was bei einem Geräteabriss mitten im
/// Stück passiert (Spec D8): Sprache beginnt einmal von vorn, der Gong
/// wird weggelassen (ein halber Gong von vorn wäre verwirrender als keiner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    Gong,
    Sprache,
}

/// Was beim Abspielen schiefgehen kann. Das Frontend wählt seinen Rückfall
/// über [`TonFehler::code`]; die Commands geben `"<code>: <Text>"` weiter.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TonFehler {
    #[error("Ansage-Ausgabe über den Kern gibt es nur unter Windows")]
    NichtUnterstuetzt,
    #[error("Kein Ausgabegerät gefunden")]
    KeinGeraet,
    #[error("Keine Windows-Stimme für diese Sprache installiert")]
    KeineStimme,
    #[error("Ansage abgebrochen")]
    Abgebrochen,
    #[error("Ungültige Ansage: {0}")]
    Ungueltig(String),
    #[error("Interner Fehler der Ansage-Ausgabe: {0}")]
    Intern(String),
}

impl TonFehler {
    /// Stabiler Code für das Frontend (Teil vor dem ersten `:`).
    pub fn code(&self) -> &'static str {
        match self {
            TonFehler::NichtUnterstuetzt => "nicht_unterstuetzt",
            TonFehler::KeinGeraet => "kein_geraet",
            TonFehler::KeineStimme => "keine_stimme",
            TonFehler::Abgebrochen => "abgebrochen",
            TonFehler::Ungueltig(_) => "ungueltig",
            TonFehler::Intern(_) => "intern",
        }
    }

    /// Fehlertext im Format der Commands: `"<code>: <Text>"`.
    pub fn fuer_frontend(&self) -> String {
        format!("{}: {}", self.code(), self)
    }
}

/// Über welches Gerät ein Stück lief (Spec `ansage-ausgabegeraet`, PR 2) —
/// bewusst nur die **Art**, nie der Name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Geraet {
    /// Keine Wahl getroffen: Windows-Standardgerät.
    #[default]
    Standard,
    /// Das in den Einstellungen gewählte Gerät.
    Gewaehlt,
    /// Ein Gerät war gewählt, fehlte aber (oder ließ sich nicht öffnen) —
    /// das Stück lief über den Windows-Standard.
    Ersatz,
}

/// Ergebnis eines gespielten Stücks — **ohne Gerätenamen** (Datenschutz,
/// Bluetooth-Geräte tragen oft Personennamen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Abspielbericht {
    /// Abgebrochen (über [`abbrechen`]) statt zu Ende gespielt.
    pub abgebrochen: bool,
    /// Wie oft das Gerät neu geöffnet werden musste (gescheiterte Öffnungen,
    /// Neustart nach Abriss, Neuaufsetzen des Audio-Threads).
    pub neu_geoeffnet: u8,
    /// Über welche Art Gerät das Stück zuletzt geöffnet wurde. Bei einem
    /// Abbruch vor dem ersten Öffnen [`Geraet::Standard`].
    pub geraet: Geraet,
}

impl Abspielbericht {
    pub(crate) fn abgebrochen(neu_geoeffnet: u8) -> Self {
        Self {
            abgebrochen: true,
            neu_geoeffnet,
            geraet: Geraet::Standard,
        }
    }
}

/// Ein aktives Ausgabegerät für die Auswahl in den Einstellungen. Der Name
/// ist nur für die Anzeige — nie ins Log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeraetInfo {
    /// cpal-Geräte-ID als Text (`wasapi:{…}.{…}`), so wie sie in
    /// `announce.output_device` gespeichert wird.
    pub id: String,
    pub name: String,
}

/// Generation der Ansage: Jedes Stück merkt sich beim Einreichen den Stand;
/// [`abbrechen`] zählt hoch, und alles Ältere — das laufende wie die noch
/// wartenden Stücke — verfällt.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Aktuelle Generation (siehe [`GENERATION`]).
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Bricht das laufende Stück ab und lässt wartende verfallen. Der wartende
/// Aufruf kehrt mit `abgebrochen: true` zurück.
pub fn abbrechen() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Wie [`abbrechen`], gibt aber zusätzlich den Audio-Thread auf. Für den
/// Fall, dass die Frist im Frontend abgelaufen ist: Hängt der Thread in einem
/// blockierenden Treiberaufruf, warteten sonst alle folgenden Stücke hinter
/// ihm bis zu ihrer eigenen Frist — jede Ansage käme verspätet. Das nächste
/// Stück startet einen frischen Thread; der alte endet, sobald er sich löst.
pub fn abbrechen_hart() {
    abbrechen();
    #[cfg(windows)]
    windows::ausgabe::faden_aufgeben();
}

/// Ist der COM-Fehler `RPC_E_CHANGED_MODE` (0x80010106)? cpal reicht ihn nur
/// als Text weiter („… Threadmodus … (os error -2147417850)"). Gemessen
/// 31.10.2026: Wiederholen hilft nicht (~8 s Stille), der Audio-Thread muss
/// neu aufgesetzt werden.
pub fn ist_com_moduswechsel(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("-2147417850")
        || t.contains("0x80010106")
        || t.contains("threadmodus")
        || t.contains("thread mode")
}

/// Läuft der Ton über den Kern? Unter Linux nicht.
pub fn unterstuetzt() -> bool {
    cfg!(windows)
}

/// Spielt ein Mono-PCM16-Stück und kehrt nach dessen Ende zurück.
/// `gewaehlt` ist die gespeicherte Geräte-ID (`announce.output_device`);
/// `None` oder ein fehlendes Gerät → aktuelles Windows-Standardgerät
/// (der Bericht sagt, welches von beiden).
pub async fn spielen(
    samples: Vec<i16>,
    rate: u32,
    art: Art,
    gewaehlt: Option<String>,
) -> Result<Abspielbericht, TonFehler> {
    spielen_in(generation(), samples, rate, art, gewaehlt).await
}

/// Wie [`spielen`], aber mit vorab festgehaltener Generation — für Stücke,
/// die erst noch synthetisiert werden: Ein Abbruch während der Synthese
/// muss das Stück schon erwischen.
async fn spielen_in(
    gen: u64,
    samples: Vec<i16>,
    rate: u32,
    art: Art,
    gewaehlt: Option<String>,
) -> Result<Abspielbericht, TonFehler> {
    if samples.is_empty() {
        return Err(TonFehler::Ungueltig("leeres Stück".to_string()));
    }
    #[cfg(windows)]
    {
        self::windows::ausgabe::spielen(gen, samples, rate, art, gewaehlt).await
    }
    #[cfg(not(windows))]
    {
        let _ = (gen, rate, art, gewaehlt);
        Err(TonFehler::NichtUnterstuetzt)
    }
}

/// Die **aktiven** Ausgabegeräte (ID + Name) für die Auswahl. Namen nur für
/// die Anzeige, nie fürs Log.
pub async fn geraete() -> Result<Vec<GeraetInfo>, TonFehler> {
    #[cfg(windows)]
    {
        self::windows::ausgabe::geraete().await
    }
    #[cfg(not(windows))]
    {
        Err(TonFehler::NichtUnterstuetzt)
    }
}

/// Ist das Gerät mit dieser ID gerade aktiv? Für die Anzeige „<Gerät> nicht
/// verfügbar …". Ein Fehler beim Aufzählen zählt als „nicht verfügbar"
/// (dann spielt die Ansage ohnehin über den Standard).
pub async fn geraet_verfuegbar(id: &str) -> bool {
    let gelistet = match geraete().await {
        Ok(liste) => id_in_liste(&liste, id),
        Err(_) => false,
    };
    #[cfg(windows)]
    {
        // Gelistet reicht nicht: Ein Endpunkt kann aktiv sein und sich doch
        // nicht öffnen lassen (Review PR 2) — dann spielt der Kern über den
        // Ersatz, und die Anzeige darf nicht „verfügbar" sagen.
        gelistet && !self::windows::ausgabe::oeffnen_zuletzt_gescheitert(id)
    }
    #[cfg(not(windows))]
    {
        gelistet
    }
}

/// Vergleich der Geräte-IDs: Windows-Endpunkt-IDs sind GUID-Texte, deren
/// Groß-/Kleinschreibung nicht garantiert gleich bleibt.
pub(crate) fn gleiche_id(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn id_in_liste(liste: &[GeraetInfo], id: &str) -> bool {
    !id.trim().is_empty() && liste.iter().any(|g| gleiche_id(&g.id, id))
}

/// Name des aktuellen Windows-Standardgeräts — nur für die **Anzeige** im
/// Frontend, nie fürs Log. `Ok(None)`: kein Ausgabegerät aktiv.
pub async fn standard_geraet_name() -> Result<Option<String>, TonFehler> {
    #[cfg(windows)]
    {
        self::windows::ausgabe::standard_geraet_name().await
    }
    #[cfg(not(windows))]
    {
        Err(TonFehler::NichtUnterstuetzt)
    }
}

/// Die installierten Windows-Stimmen (OneCore).
pub async fn stimmen() -> Result<Vec<StimmeInfo>, TonFehler> {
    #[cfg(windows)]
    {
        blockierend(self::windows::winrt_stimme::stimmen).await
    }
    #[cfg(not(windows))]
    {
        Err(TonFehler::NichtUnterstuetzt)
    }
}

/// Kann der PC Seiten-Ansagen mit der Windows-Stimme vertonen — gibt es
/// eine deutsche oder englische OneCore-Stimme? Einmal je Programmlauf
/// abgefragt (ein neues Sprachpaket wirkt nach dem Neustart). Die Seite
/// meldet sich nur als Ansage-Gerät, wenn der PC vertonen kann oder sie
/// selbst eine Stimme hat (`hostAudio`, E3-Kriterium 25).
pub fn seiten_stimme_vorhanden() -> bool {
    #[cfg(windows)]
    {
        static DA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *DA.get_or_init(|| {
            self::windows::winrt_stimme::stimmen()
                .map(|v| v.iter().any(|s| stimme::ist_ansage_sprache(&s.sprache)))
                .unwrap_or(false)
        })
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Synthetisiert die Abschnitte mit der Windows-Stimme zu **einem**
/// Mono-PCM16-Stück (300 ms Fuge). Läuft in `spawn_blocking` (MTA), nie auf
/// dem Audio-Thread — dort würde WinRT den COM-Modus kippen.
pub async fn stimme_pcm(
    segmente: Vec<String>,
    lang: String,
    stimme: String,
    rate: f64,
) -> Result<(Vec<i16>, u32), TonFehler> {
    #[cfg(windows)]
    {
        blockierend(move || self::windows::winrt_stimme::pcm_fuer(&segmente, &lang, &stimme, rate))
            .await
    }
    #[cfg(not(windows))]
    {
        let _ = (segmente, lang, stimme, rate);
        Err(TonFehler::NichtUnterstuetzt)
    }
}

/// Synthetisiert und spielt eine Ansage mit der Windows-Stimme (Art
/// Sprache). Ein Abbruch während der Synthese verwirft das Stück.
/// `gewaehlt` wie bei [`spielen`].
pub async fn stimme_spielen(
    segmente: Vec<String>,
    lang: String,
    stimme: String,
    rate: f64,
    gewaehlt: Option<String>,
) -> Result<Abspielbericht, TonFehler> {
    let gen = generation();
    let (samples, sr) = stimme_pcm(segmente, lang, stimme, rate).await?;
    if generation() != gen {
        return Ok(Abspielbericht::abgebrochen(0));
    }
    pcm::dauer_pruefen(samples.len(), sr, MAX_SEKUNDEN_STIMME)?;
    spielen_in(gen, samples, sr, Art::Sprache, gewaehlt).await
}

#[cfg(windows)]
async fn blockierend<T, F>(f: F) -> Result<T, TonFehler>
where
    F: FnOnce() -> Result<T, TonFehler> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| TonFehler::Intern(format!("Synthese-Task: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fehlercodes_sind_stabil() {
        let faelle = [
            (TonFehler::NichtUnterstuetzt, "nicht_unterstuetzt"),
            (TonFehler::KeinGeraet, "kein_geraet"),
            (TonFehler::KeineStimme, "keine_stimme"),
            (TonFehler::Abgebrochen, "abgebrochen"),
            (TonFehler::Ungueltig("x".into()), "ungueltig"),
            (TonFehler::Intern("y".into()), "intern"),
        ];
        for (f, code) in faelle {
            assert_eq!(f.code(), code);
            let text = f.fuer_frontend();
            assert_eq!(text.split(':').next(), Some(code), "{text}");
        }
    }

    #[test]
    fn bericht_ist_camel_case_ohne_geraetenamen() {
        let j = serde_json::to_value(Abspielbericht {
            abgebrochen: true,
            neu_geoeffnet: 2,
            geraet: Geraet::Ersatz,
        })
        .unwrap();
        assert_eq!(
            j,
            serde_json::json!({"abgebrochen": true, "neuGeoeffnet": 2, "geraet": "ersatz"})
        );
        // Ohne Wahl: Standard.
        let j = serde_json::to_value(Abspielbericht::default()).unwrap();
        assert_eq!(j["geraet"], "standard");
        assert_eq!(
            serde_json::to_value(Geraet::Gewaehlt).unwrap(),
            serde_json::json!("gewaehlt")
        );
    }

    #[test]
    fn geraet_info_ist_camel_case() {
        let j = serde_json::to_value(GeraetInfo {
            id: "wasapi:{0.0.0.00000000}.{abc}".to_string(),
            name: "USB-Box".to_string(),
        })
        .unwrap();
        assert_eq!(
            j,
            serde_json::json!({"id": "wasapi:{0.0.0.00000000}.{abc}", "name": "USB-Box"})
        );
    }

    #[test]
    fn geraete_id_vergleich_ignoriert_schreibweise_und_leere() {
        let liste = vec![GeraetInfo {
            id: "wasapi:{0.0.0.00000000}.{ABC-def}".to_string(),
            name: "x".to_string(),
        }];
        assert!(id_in_liste(&liste, "wasapi:{0.0.0.00000000}.{abc-DEF}"));
        assert!(!id_in_liste(&liste, "wasapi:{0.0.0.00000000}.{andere}"));
        assert!(!id_in_liste(&liste, ""));
        assert!(!id_in_liste(&liste, "   "));
        assert!(!id_in_liste(&[], "wasapi:{x}"));
    }

    #[test]
    fn abbrechen_zaehlt_die_generation_hoch() {
        let vorher = generation();
        abbrechen();
        assert!(generation() > vorher);
    }

    #[test]
    fn com_moduswechsel_wird_am_text_erkannt() {
        assert!(ist_com_moduswechsel(
            "Für den Thread wurde bereits ein anderer Threadmodus festgelegt. (os error -2147417850)"
        ));
        assert!(ist_com_moduswechsel(
            "Cannot change thread mode after it is set."
        ));
        assert!(ist_com_moduswechsel("HRESULT 0x80010106"));
        assert!(!ist_com_moduswechsel(
            "The requested device is not available."
        ));
    }

    #[tokio::test]
    async fn leeres_stueck_ist_ungueltig() {
        assert!(matches!(
            spielen(vec![], 24000, Art::Gong, None).await,
            Err(TonFehler::Ungueltig(_))
        ));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn ohne_windows_ist_alles_nicht_unterstuetzt() {
        assert!(!unterstuetzt());
        assert_eq!(
            spielen(vec![1, 2, 3], 24000, Art::Sprache, None).await,
            Err(TonFehler::NichtUnterstuetzt)
        );
        assert_eq!(
            standard_geraet_name().await,
            Err(TonFehler::NichtUnterstuetzt)
        );
        assert_eq!(stimmen().await, Err(TonFehler::NichtUnterstuetzt));
        assert_eq!(
            stimme_spielen(vec!["Feld 1".into()], "de".into(), String::new(), 0.8, None).await,
            Err(TonFehler::NichtUnterstuetzt)
        );
        assert_eq!(geraete().await, Err(TonFehler::NichtUnterstuetzt));
        assert!(!geraet_verfuegbar("wasapi:{x}").await);
    }
}
