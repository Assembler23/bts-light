//! Messwerkzeug: Wie lange wartet eine Ansage, wenn das Internet weg ist?
//! (Spec `ansage-offline-stimme`, Etappe E0, ADR 0105.)
//!
//! **Messwerkzeug, kein Regressionstest.** Die Ausfall-Erkennung (E1) steht
//! auf Annahmen, die sich im Repo nicht prüfen lassen:
//!
//! 1. Ohne Internet wartet eine Ansage **heute** die volle Gesamtfrist ab
//!    (10 s, Gemini 20 s) — oder scheitert sie von selbst früher?
//! 2. Eine **Verbindungsfrist** (Ausgangswert 1,5 s) kürzt das Warten — bei
//!    welcher Ausfallart, und bei welcher nicht?
//! 3. Die **harte Frist** (Ausgangswert 2,5 s) greift in jeder Ausfallart
//!    pünktlich, auch bei hängender Namensauflösung und stummer Gegenstelle.
//! 4. Bei **gutem Netz** bleibt der Verbindungsaufbau mit Abstand unter der
//!    Verbindungsfrist und die Vertonung unter der harten Frist — sonst löst
//!    ein langsames, aber lebendes Netz die Ersatzstimme aus.
//!
//! Die Sonde fragt die Adressen der vier Sprachdienste an, **ohne etwas zu
//! vertonen**: ein Abruf ohne Schlüssel bzw. der echte Weg der App mit einem
//! ungültigen Schlüssel. Das kostet keine Zeichen. Dazu stellt sie fünf
//! Ausfallarten selbst nach (siehe „Nachgestellte Fälle"), die auf jedem
//! Rechner laufen. Sie ändert nichts an der App und liest deren Einstellungen
//! nur, wenn `BTS_PROBE_ECHT` gesetzt ist.
//!
//! Standardmäßig übersprungen (`#[ignore]`):
//!
//! ```text
//! cargo test -p bts-light --test tts_ausfall_probe -- --ignored --nocapture
//! ```
//!
//! In der PowerShell mit Bezeichnung der Lage:
//!
//! ```text
//! $env:BTS_PROBE_LAGE="kabel"; cargo test -p bts-light --test tts_ausfall_probe -- --ignored --nocapture
//! ```
//!
//! **Vor dem ersten Lauf einmal mit Internet bauen** (`cargo test -p bts-light
//! --test tts_ausfall_probe --no-run`) — ohne Netz kann cargo nichts
//! nachladen.
//!
//! ## Was am Windows-PC zu messen ist
//!
//! Ein Lauf je Lage; `BTS_PROBE_LAGE` benennt sie. Bei den Ausfällen **zwei
//! Läufe direkt hintereinander** (`kabel`, dann `kabel-2`): Der zweite zeigt,
//! ob Windows sich den Ausfall merkt und schneller scheitert.
//!
//! | `BTS_PROBE_LAGE` | Wie herstellen | Frage |
//! |---|---|---|
//! | `normal` | Netz wie am Turniertag | Grundlinie: Wie lange dauern Aufbau und Antwort? Reicht die Verbindungsfrist mit Abstand? |
//! | `kabel` | Netzwerkkabel ziehen bzw. WLAN ausschalten | Scheitert es sofort, oder hängt die Namensauflösung? |
//! | `ohne-internet` | WLAN bleibt verbunden, der Router hat kein Internet (Kabel zum Anschluss ziehen; oder Handy-Hotspot mit abgeschalteten mobilen Daten) | **Der Fall der Spec.** Wie lange wartet die Ansage heute, wie lange mit Verbindungsfrist? |
//! | `hotspot` | Gast-WLAN mit Anmeldeseite, **nicht** angemeldet | Was meldet die Sonde, und wie schnell? |
//! | `langsam` | Handy-Hotspot mit schwachem Empfang (wenn zur Hand) | Löst ein langsames Netz die Fristen aus? |
//!
//! Dazu einmal bei gutem Netz mit `BTS_PROBE_ECHT=5` (siehe unten): Das ist
//! die einzige Messung, die sagt, ob die harte Frist für eine echte Vertonung
//! reicht.
//!
//! Alle Zeilen landen zusätzlich in einer Datei im Temp-Verzeichnis
//! (Windows: `%TEMP%\bts-light-tts-ausfall-messung.txt`); jeder Lauf hängt an.
//! **Diese Datei** gehört in ADR 0105 („Ergebnis der Messung") — nicht die
//! kopierte Konsole: cargo druckt dort Pfade des Rechners mit.
//!
//! Ein Lauf dauert bei gutem Netz etwa anderthalb Minuten, bei einem Ausfall
//! bis zu zehn (jede Messung wartet höchstens bis zur Decke).
//!
//! ## Messungen je Ziel
//!
//! | `messung=` | Was |
//! |---|---|
//! | `heute` | Abruf mit frischem Client und nur der Gesamtfrist — wie die App heute |
//! | `verbfrist` | wie `heute`, zusätzlich mit Verbindungsfrist |
//! | `hartefrist` | wie `heute`, von außen nach der harten Frist abgebrochen |
//! | `grundlinie` | nur wenn der Dienst antwortet: wiederholter Abruf wie `heute` (ohne Verbindungsfrist — ein langsames Netz soll sichtbar werden, nicht abgeschnitten) |
//! | `dns` | Namensauflösung allein, ohne Frist (bis zur Decke) |
//! | `tcp` | Verbindungsaufbau allein, ohne Frist (bis zur Decke); immer direkt, auch wenn am PC ein Proxy eingestellt ist — die Abrufe gehen dann einen anderen Weg |
//! | `produktiv` | der echte Weg der App (`vertonen`) mit ungültigem Schlüssel |
//! | `echt` | echte Vertonung mit dem gewählten Dienst (nur mit `BTS_PROBE_ECHT`) |
//!
//! Je Dienst beginnt die Sonde mit einer **anderen** Messung (`heute`,
//! `verbfrist`, `hartefrist`, `dns` mit `tcp`): Der erste Versuch nach einem
//! Ausfall ist der teure, spätere können von einem Zwischenspeicher des
//! Betriebssystems profitieren — Windows merkt sich auch gescheiterte
//! Namensauflösungen. So hat jede dieser Messarten in einem Lauf über alle
//! vier Dienste (ohne `BTS_PROBE_ZIELE`) einen „kalten" Wert: `kalt=1` in
//! der Zeile, `*` in der Übersicht. `produktiv` und `grundlinie` laufen immer danach, also nie
//! kalt. `nr=` und `t=` (Sekunden seit Start) zeigen die Reihenfolge.
//!
//! Ein `-` in der Übersicht heißt: nicht gemessen (`tcp` ohne aufgelösten
//! Namen, `grundlinie` ohne Antwort des Dienstes, rohe Stufen bei Zielen ohne
//! Namen).
//!
//! `echt` misst eine Vertonung **in einem Stück**. Mit „Ansage in Teilen"
//! stellt die App mehrere Anfragen; die misst die Sonde nicht.
//!
//! ## Nachgestellte Fälle
//!
//! | Ziel | Was |
//! |---|---|
//! | `sim-abgewiesen` | geschlossener Port: die Gegenstelle lehnt ab (Linux sofort; Windows wiederholt den Versuch erfahrungsgemäß etwa 2 s lang — ungemessen) |
//! | `sim-stumm-tls` | Gegenstelle nimmt an und schweigt — der TLS-Aufbau hängt |
//! | `sim-stumm-http` | Gegenstelle nimmt die Anfrage an und antwortet nie |
//! | `sim-schwarzes-loch` | Adresse, von der nichts zurückkommt (nur aussagekräftig **mit** Netz) |
//! | `sim-name-unbekannt` | Name, den es nicht gibt |
//!
//! Eine **hängende** Namensauflösung und die Anmeldeseite eines Hotspots
//! lassen sich nicht ehrlich nachstellen — die misst nur der Lauf am
//! Windows-PC.
//!
//! ## Umgebungsvariablen
//!
//! | Variable | Standard | Bedeutung |
//! |---|---|---|
//! | `BTS_PROBE_LAGE` | `unbenannt` | Bezeichnung des Laufs (höchstens 24 Zeichen; **keinen** Hallen-, Hotel- oder Netzwerknamen eintragen) |
//! | `BTS_PROBE_ZIELE` | alle | Auswahl, z. B. `azure,sim` |
//! | `BTS_PROBE_REGION` | `westeurope` | Azure-Region |
//! | `BTS_PROBE_VERBINDUNGSFRIST_MS` | `1500` | Verbindungsfrist |
//! | `BTS_PROBE_HARTE_FRIST_MS` | `2500` | harte Frist |
//! | `BTS_PROBE_DECKE_S` | `30` | so lange wartet die Sonde höchstens je Messung |
//! | `BTS_PROBE_WIEDERHOLUNGEN` | `5` | Abrufe für die Grundlinie (höchstens 50) |
//! | `BTS_PROBE_SCHWARZES_LOCH` | `192.0.2.1` | Adresse für `sim-schwarzes-loch` |
//! | `BTS_PROBE_DATEI` | Temp-Verzeichnis | Ergebnisdatei |
//! | `BTS_PROBE_ECHT` | aus | Anzahl echter Vertonungen mit dem gewählten Dienst — **kostet Zeichen** (etwa 110 je Vertonung) |
//! | `BTS_PROBE_CONFIG` | Einstellungen der installierten App | `config.json`, aus der `echt` Dienst und Schlüssel liest |
//!
//! ## Datenschutz
//!
//! Die Sonde gibt **keinen Schlüssel, keinen Gerätenamen und keinen
//! Netzwerknamen** aus. Der Satz für `echt` nennt nur erfundene Namen. Von
//! einer Dienst-Antwort erscheint nur der HTTP-Status, nie der Antworttext.
//! Von einem Fehler erscheinen nur **feste Kennwörter** (`fehler_art`), nie
//! der Fehlertext: Der nennt bei einer Anmeldeseite die Namen aus deren
//! Zertifikat (Router, Hotel) und bei einem Abruf die angefragte Adresse. Die
//! Sonde druckt auch keinen Pfad — er trüge unter Windows den Benutzernamen.

use std::future::Future;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bts_light_lib::config::{self, AppConfig, DienstArt};
use bts_light_lib::tts_text::netz::TtsFehler;
use bts_light_lib::tts_text::Format;
use bts_light_lib::{azure_tts, elevenlabs_tts, gemini_tts, openai_tts};

/// Offensichtlich ungültig: Die Dienste lehnen ab, bevor sie etwas vertonen.
const UNGUELTIGER_SCHLUESSEL: &str = "bts-light-messung-ungueltig";

/// Azure-Stimme für den Weg mit ungültigem Schlüssel (wird nie gesprochen).
const PROBE_STIMME_AZURE: &str = "de-DE-SeraphinaMultilingualNeural";

/// Name der Ergebnisdatei im Temp-Verzeichnis.
const DATEI_NAME: &str = "bts-light-tts-ausfall-messung.txt";

/// Satz für `echt`: Länge und Bau einer Spielansage, nur erfundene Namen.
const ECHT_SATZ: &str = "Auf Feld drei: Herrendoppel. Erika Mustermann und Max Mustermann \
gegen Otto Normalverbraucher und Hans Meier.";

fn env_zahl(name: &str, standard: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(standard)
}

fn env_text(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Nur Kleinbuchstaben, Ziffern und Bindestrich — der Wert landet in einer
/// Adresse bzw. in der Ergebniszeile.
fn schlicht(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Art eines Betriebssystem-Fehlers: der feste Name der Fehlerart und, wenn
/// vorhanden, die Fehlernummer des Systems (unter Windows z. B. 10060).
fn io_art(e: &std::io::Error) -> Option<String> {
    let art = format!("{:?}", e.kind());
    match e.raw_os_error() {
        Some(nr) => Some(format!("{art} os-{nr}")),
        // Ohne Nummer sagen die Sammelarten nichts.
        None if art == "Other" || art == "Uncategorized" => None,
        None => Some(art),
    }
}

/// Fehlertext-Merkmal → Kennwort. Geprüft wird die ganze Ursachenkette.
const FEHLER_MERKMALE: [(&str, &str); 8] = [
    ("certificate", "tls-zertifikat"),
    ("dns error", "name"),
    ("failed to lookup address", "name"),
    ("timed out", "zeit"),
    ("deadline has elapsed", "zeit"),
    ("handshake", "tls"),
    ("corrupt message", "tls"),
    ("tls", "tls"),
];

/// Macht aus einem Fehler **feste Kennwörter** — nie den Fehlertext selbst.
/// Der kann Fremdes tragen: Bei einer Anmeldeseite nennt der TLS-Fehler die
/// Namen aus deren Zertifikat, ein Abruf-Fehler die angefragte Adresse.
fn fehler_art(e: &(dyn std::error::Error + 'static)) -> String {
    let mut text = String::new();
    let mut teile: Vec<String> = Vec::new();
    let merke = |teile: &mut Vec<String>, wort: String| {
        if !teile.contains(&wort) {
            teile.push(wort);
        }
    };
    let mut glied = Some(e);
    while let Some(g) = glied {
        // Der Text des HTTP-Clients nennt die Adresse; ein Name darin
        // könnte zufällig ein Merkmal treffen. Seine Ursachen zählen.
        if !g.is::<reqwest::Error>() {
            text.push_str(&g.to_string().to_lowercase());
            text.push(' ');
        }
        if let Some(art) = g.downcast_ref::<std::io::Error>().and_then(io_art) {
            merke(&mut teile, art);
        }
        glied = g.source();
    }
    for (merkmal, wort) in FEHLER_MERKMALE {
        if text.contains(merkmal) {
            merke(&mut teile, wort.to_string());
        }
    }
    if teile.is_empty() {
        "unbekannt".to_string()
    } else {
        teile.join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anbieter {
    Azure,
    ElevenLabs,
    OpenAi,
    Gemini,
}

/// Ein Ziel der Messung: ein Sprachdienst oder ein nachgestellter Fall.
struct Ziel {
    name: &'static str,
    url: String,
    /// Für die rohen Stufen `dns` und `tcp`; `None` = entfällt.
    roh: Option<(String, u16)>,
    /// Gesamtfrist, mit der die App diesen Dienst heute anfragt.
    gesamt: Duration,
    /// `None` = nachgestellter Fall.
    anbieter: Option<Anbieter>,
}

impl Ziel {
    /// Echte Dienste fragt die Sonde wie die App an, also auch über einen
    /// eingestellten Proxy. Die nachgestellten Fälle gehen immer direkt —
    /// ein Proxy antwortete sonst an ihrer Stelle.
    fn direkt(&self) -> bool {
        self.anbieter.is_none()
    }
}

#[derive(Debug, Clone)]
enum Ausgang {
    /// Die Gegenstelle hat geantwortet (HTTP-Status) — das Netz trägt.
    Antwort(u16),
    /// Stufe gelungen (ohne HTTP-Status).
    Gelungen(String),
    Fehler(String),
    /// Von der harten Frist abgebrochen.
    HarteFrist,
    /// Die Sonde hat das Warten aufgegeben.
    Decke,
}

impl Ausgang {
    fn traegt(&self) -> bool {
        matches!(self, Ausgang::Antwort(_) | Ausgang::Gelungen(_))
    }

    fn text(&self) -> String {
        match self {
            Ausgang::Antwort(code) => format!("antwort http-{code}"),
            Ausgang::Gelungen(t) => format!("gelungen {t}"),
            Ausgang::Fehler(t) => format!("fehler {t}"),
            Ausgang::HarteFrist => "abgebrochen harte-frist".to_string(),
            Ausgang::Decke => "aufgegeben decke".to_string(),
        }
    }
}

struct Messwert {
    ziel: &'static str,
    messung: &'static str,
    ms: u128,
    traegt: bool,
    /// Erste Messung an diesem Ziel in diesem Lauf.
    kalt: bool,
}

/// Sammelt die Messwerte, druckt sie und hängt sie an die Ergebnisdatei.
struct Protokoll {
    lage: String,
    start: Instant,
    nr: u32,
    werte: Vec<Messwert>,
    datei: Option<std::fs::File>,
}

impl Protokoll {
    fn zeile(&mut self, text: &str) {
        println!("{text}");
        if let Some(d) = self.datei.as_mut() {
            let _ = writeln!(d, "{text}");
        }
    }

    fn melde(&mut self, ziel: &'static str, messung: &'static str, dauer: Duration, a: &Ausgang) {
        self.nr += 1;
        let ms = dauer.as_millis();
        let kalt = !self.werte.iter().any(|w| w.ziel == ziel);
        let text = format!(
            "MESSWERT lage={} nr={} t={:.1} ziel={ziel} messung={messung} ms={ms} kalt={} ausgang=\"{}\"",
            self.lage,
            self.nr,
            self.start.elapsed().as_secs_f64(),
            u8::from(kalt),
            a.text(),
        );
        self.zeile(&text);
        self.werte.push(Messwert {
            ziel,
            messung,
            ms,
            traegt: a.traegt(),
            kalt,
        });
    }
}

/// Misst eine Stufe; gibt nach `decke` auf, damit die Sonde nie hängen bleibt.
async fn gemessen<T>(decke: Duration, f: impl Future<Output = T>) -> (Duration, Option<T>) {
    let beginn = Instant::now();
    let erg = tokio::time::timeout(decke, f).await.ok();
    (beginn.elapsed(), erg)
}

/// Ordnet einen Fehler des HTTP-Clients ein: Stufe und Kennwörter der Ursache.
fn einordnen(e: &reqwest::Error) -> String {
    let art = if e.is_timeout() {
        "frist"
    } else if e.is_connect() {
        "verbindung"
    } else if e.is_request() {
        "anfrage"
    } else {
        "sonstig"
    };
    format!("{art}: {}", fehler_art(e))
}

/// Ein Abruf mit **frischem** Client — die App baut ihn heute je Anfrage neu,
/// also auch jedes Mal die Verbindung.
async fn abruf(z: &Ziel, verbindung: Option<Duration>) -> Ausgang {
    let mut bau = reqwest::Client::builder().timeout(z.gesamt);
    if let Some(v) = verbindung {
        bau = bau.connect_timeout(v);
    }
    if z.direkt() {
        bau = bau.no_proxy();
    }
    let client = match bau.build() {
        Ok(c) => c,
        Err(_) => return Ausgang::Fehler("client".to_string()),
    };
    match client
        .get(&z.url)
        .header("User-Agent", "bts-light")
        .send()
        .await
    {
        Ok(antwort) => Ausgang::Antwort(antwort.status().as_u16()),
        Err(e) => Ausgang::Fehler(einordnen(&e)),
    }
}

/// Grobe Art einer Adresse. Löst ein Dienst auf eine private Adresse auf,
/// antwortet nicht der Dienst, sondern etwas im Hallennetz (Anmeldeseite).
fn adress_art(ip: IpAddr) -> &'static str {
    match ip {
        IpAddr::V4(v4) if v4.is_loopback() => "eigener-rechner",
        IpAddr::V4(v4) if v4.is_private() || v4.is_link_local() => "privat",
        IpAddr::V6(v6) if v6.is_loopback() => "eigener-rechner",
        IpAddr::V6(v6) if (v6.segments()[0] & 0xfe00) == 0xfc00 => "privat",
        IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80 => "privat",
        _ => "oeffentlich",
    }
}

async fn aufloesen(host: &str, port: u16) -> (Ausgang, Option<SocketAddr>) {
    match tokio::net::lookup_host((host, port)).await {
        Ok(adressen) => {
            let alle: Vec<SocketAddr> = adressen.collect();
            match alle.first().copied() {
                Some(erste) => (
                    Ausgang::Gelungen(format!(
                        "{} adresse(n), {}",
                        alle.len(),
                        adress_art(erste.ip())
                    )),
                    Some(erste),
                ),
                None => (Ausgang::Fehler("keine adresse".to_string()), None),
            }
        }
        Err(e) => (Ausgang::Fehler(fehler_art(&e)), None),
    }
}

async fn verbinden(adresse: SocketAddr) -> Ausgang {
    match tokio::net::TcpStream::connect(adresse).await {
        Ok(_) => Ausgang::Gelungen("verbunden".to_string()),
        Err(e) => Ausgang::Fehler(fehler_art(&e)),
    }
}

/// SSML wie von der App: eine Stimme, ein Satz.
fn ssml(stimme: &str, satz: &str) -> String {
    let stimme = stimme
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    format!(
        r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="de-DE"><voice name="{stimme}">{satz}</voice></speak>"#
    )
}

/// Aus dem Fehlertext eines Dienst-Moduls **nur** den HTTP-Status ziehen. Der
/// Antworttext dahinter bleibt draußen: Manche Dienste wiederholen darin
/// Teile des Schlüssels.
fn http_status(fehler: &str) -> Option<u16> {
    let rest = fehler.split(" HTTP ").nth(1)?;
    rest.get(..3)?.parse().ok()
}

/// Die Dienst-Module melden ihre Fehler als Text. Daraus nur die Stufe: Der
/// Text nennt die angefragte Adresse — bei `echt` samt Region und Stimme aus
/// den Einstellungen. Was genau schiefging, zeigt die Dauer (Gesamtfrist oder
/// sofort) und die Messung `heute` am selben Ziel.
fn modul_fehler_art(fehler: &str) -> &'static str {
    if fehler.contains("Anfrage:") || fehler.contains("Request:") {
        "anfrage-ohne-antwort"
    } else if fehler.contains("Antwort:") {
        "antwort-abgerissen"
    } else {
        "sonstig"
    }
}

/// Fehlertext eines Dienst-Moduls → Ausgang, ohne Antworttext.
fn dienst_ausgang(erg: Result<(), TtsFehler>) -> Ausgang {
    match erg {
        Ok(()) => Ausgang::Gelungen("vertont".to_string()),
        Err(e) => match http_status(&e.meldung) {
            Some(code) => Ausgang::Antwort(code),
            None => Ausgang::Fehler(modul_fehler_art(&e.meldung).to_string()),
        },
    }
}

/// Der echte Weg der App mit ungültigem Schlüssel (seit E1 mit Verbindungsfrist,
/// ohne die harte Frist von außen) — misst genau das Warten,
/// das eine Ansage heute erlebt, ohne Zeichen zu verbrauchen.
async fn produktiv(anbieter: Anbieter, region: &str) -> Ausgang {
    let schluessel = UNGUELTIGER_SCHLUESSEL;
    let text = ssml(PROBE_STIMME_AZURE, "Probe.");
    let modell = |dienst: &str| match config::dienst_art(dienst) {
        DienstArt::ElevenLabs(m) | DienstArt::OpenAi(m) | DienstArt::Gemini(m) => m,
        DienstArt::Azure => "",
    };
    let erg = match anbieter {
        Anbieter::Azure => azure_tts::synthesize_ohne_cache(region, schluessel, &text, Format::Mp3)
            .await
            .map(|_| ()),
        Anbieter::ElevenLabs => elevenlabs_tts::vertonen(
            schluessel,
            config::ELEVENLABS_STANDARD_STIMME,
            modell("elevenlabs_v2"),
            &text,
            None,
            Format::Mp3,
        )
        .await
        .map(|_| ()),
        Anbieter::OpenAi => openai_tts::vertonen(
            schluessel,
            config::OPENAI_STANDARD_STIMME,
            modell("openai"),
            &text,
            None,
            Format::Mp3,
        )
        .await
        .map(|_| ()),
        Anbieter::Gemini => gemini_tts::vertonen(
            schluessel,
            config::GEMINI_STANDARD_STIMME,
            modell("gemini_38_flash"),
            &text,
            None,
            Format::Mp3,
        )
        .await
        .map(|_| ()),
    };
    dienst_ausgang(erg)
}

fn stimme_oder<'a>(stimme: &'a str, standard: &'a str) -> &'a str {
    if stimme.trim().is_empty() {
        standard
    } else {
        stimme.trim()
    }
}

/// Echte Vertonung mit dem in den Einstellungen gewählten Dienst, ohne Cache.
async fn echt_vertonen(az: &config::AzureTtsConfig) -> Ausgang {
    let erg = match config::dienst_art(&az.dienst) {
        DienstArt::Azure => azure_tts::synthesize_ohne_cache(
            az.region.trim(),
            az.key.trim(),
            &ssml(&az.voice, ECHT_SATZ),
            Format::Mp3,
        )
        .await
        .map(|_| ()),
        DienstArt::ElevenLabs(modell) => elevenlabs_tts::vertonen(
            az.elevenlabs_key.trim(),
            stimme_oder(&az.elevenlabs_voice, config::ELEVENLABS_STANDARD_STIMME),
            modell,
            &ssml("x", ECHT_SATZ),
            None,
            Format::Mp3,
        )
        .await
        .map(|_| ()),
        DienstArt::OpenAi(modell) => openai_tts::vertonen(
            az.openai_key.trim(),
            stimme_oder(&az.openai_voice, config::OPENAI_STANDARD_STIMME),
            modell,
            &ssml("x", ECHT_SATZ),
            None,
            Format::Mp3,
        )
        .await
        .map(|_| ()),
        DienstArt::Gemini(modell) => gemini_tts::vertonen(
            az.gemini_key.trim(),
            stimme_oder(&az.gemini_voice, config::GEMINI_STANDARD_STIMME),
            modell,
            &ssml("x", ECHT_SATZ),
            None,
            Format::Mp3,
        )
        .await
        .map(|_| ()),
    };
    dienst_ausgang(erg)
}

/// `config.json` der installierten App (Tauri: Einstellungs-Verzeichnis des
/// Benutzers + Kennung der App).
fn standard_config() -> Option<PathBuf> {
    let basis = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    basis.map(|b| b.join("de.badhub.btslight").join("config.json"))
}

/// Gegenstelle, die Verbindungen annimmt und nie antwortet. Die angenommenen
/// Verbindungen bleiben offen, solange die Aufgabe lebt.
async fn stumme_gegenstelle() -> std::io::Result<(u16, tokio::task::JoinHandle<()>)> {
    let lauscher = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = lauscher.local_addr()?.port();
    let aufgabe = tokio::spawn(async move {
        let mut offen = Vec::new();
        while let Ok((verbindung, _)) = lauscher.accept().await {
            offen.push(verbindung);
        }
    });
    Ok((port, aufgabe))
}

/// Ein Port, an dem gerade niemand lauscht.
fn geschlossener_port() -> std::io::Result<u16> {
    let lauscher = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    lauscher.local_addr().map(|a| a.port())
}

struct Fristen {
    verbindung: Duration,
    hart: Duration,
    decke: Duration,
}

const HEUTE: &str = "heute";
const VERBFRIST: &str = "verbfrist";
const HARTEFRIST: &str = "hartefrist";
/// Die rohen Stufen `dns` und `tcp` als ein Platz in der Messfolge.
const ROH: &str = "roh";

/// Eine der drei Abruf-Messungen an einem Ziel.
async fn abruf_messen(p: &mut Protokoll, z: &Ziel, messung: &'static str, f: &Fristen) -> bool {
    let (dauer, erg) = match messung {
        VERBFRIST => gemessen(f.decke, abruf(z, Some(f.verbindung))).await,
        HARTEFRIST => {
            gemessen(f.decke, async {
                tokio::time::timeout(f.hart, abruf(z, None))
                    .await
                    .unwrap_or(Ausgang::HarteFrist)
            })
            .await
        }
        _ => gemessen(f.decke, abruf(z, None)).await,
    };
    let ausgang = erg.unwrap_or(Ausgang::Decke);
    p.melde(z.name, messung, dauer, &ausgang);
    matches!(ausgang, Ausgang::Antwort(_))
}

/// Die rohen Stufen an einem Ziel: Namensauflösung, dann Verbindungsaufbau.
async fn roh_messen(p: &mut Protokoll, z: &Ziel, f: &Fristen) {
    let Some((host, port)) = &z.roh else {
        return;
    };
    let (dauer, erg) = gemessen(f.decke, aufloesen(host, *port)).await;
    let (ausgang, adresse) = erg.unwrap_or((Ausgang::Decke, None));
    p.melde(z.name, "dns", dauer, &ausgang);
    let Some(adresse) = adresse else {
        return;
    };
    // Der Name eines Sprachdienstes zeigt ins Hallennetz: Dort antwortet
    // nicht der Dienst. Die Übersicht zeigte das sonst als gelungen.
    if z.anbieter.is_some() && adress_art(adresse.ip()) != "oeffentlich" {
        p.zeile(&format!(
            "HINWEIS {}: der Name zeigt auf eine Adresse im eigenen Netz — vermutlich eine Anmeldeseite",
            z.name
        ));
    }
    let (dauer, erg) = gemessen(f.decke, verbinden(adresse)).await;
    p.melde(z.name, "tcp", dauer, &erg.unwrap_or(Ausgang::Decke));
}

/// Alle Messungen an einem Ziel. `erste` verschiebt, welche Messung der
/// Folge den kalten ersten Versuch bekommt.
async fn ziel_messen(
    p: &mut Protokoll,
    z: &Ziel,
    erste: usize,
    f: &Fristen,
    region: &str,
    wiederholungen: u64,
) {
    let folge = [HEUTE, VERBFRIST, HARTEFRIST, ROH];
    let mut erreichbar = false;
    for i in 0..folge.len() {
        match folge[(erste + i) % folge.len()] {
            ROH => roh_messen(p, z, f).await,
            messung => erreichbar |= abruf_messen(p, z, messung, f).await,
        }
    }

    // Grundlinie nur bei einem echten Dienst, der antwortet: Wie lange
    // dauern Aufbau und Antwort? Ohne Verbindungsfrist — ein langsames Netz
    // soll in der Reihe sichtbar werden, nicht an der Frist abgeschnitten.
    if erreichbar && z.anbieter.is_some() {
        for _ in 0..wiederholungen {
            let (dauer, erg) = gemessen(f.decke, abruf(z, None)).await;
            p.melde(z.name, "grundlinie", dauer, &erg.unwrap_or(Ausgang::Decke));
        }
    }

    if let Some(anbieter) = z.anbieter {
        let (dauer, erg) = gemessen(f.decke, produktiv(anbieter, region)).await;
        p.melde(z.name, "produktiv", dauer, &erg.unwrap_or(Ausgang::Decke));
    }
}

/// Echte Vertonungen mit dem gewählten Dienst (nur auf Wunsch).
async fn echt_messen(p: &mut Protokoll, anzahl: u64, decke: Duration) {
    let Some(pfad) = env_text("BTS_PROBE_CONFIG")
        .map(PathBuf::from)
        .or_else(standard_config)
    else {
        p.zeile("HINWEIS echt: kein Pfad zur config.json (BTS_PROBE_CONFIG setzen)");
        return;
    };
    // Eine fehlende Datei läse sich als frische Installation ohne Dienst.
    // Ohne den Pfad: Er trägt unter Windows den Benutzernamen.
    if !pfad.exists() {
        p.zeile(
            "HINWEIS echt: config.json nicht gefunden (anderer Windows-Benutzer? \
             BTS_PROBE_CONFIG setzen) — übersprungen",
        );
        return;
    }
    let cfg = match AppConfig::load_from(&pfad) {
        Ok(c) => c,
        // Ohne den Fehlertext: Er kann einen Wert aus der Datei zitieren.
        Err(_) => {
            p.zeile("HINWEIS echt: config.json nicht lesbar oder beschädigt — übersprungen");
            return;
        }
    };
    let az = cfg.azure_tts;
    if !az.vollstaendig() {
        p.zeile("HINWEIS echt: kein Sprachdienst eingeschaltet und eingerichtet — übersprungen");
        return;
    }
    let dienst = schlicht(&az.dienst.replace('_', "-"));
    p.zeile(&format!(
        "HINWEIS echt: {anzahl} Vertonung(en) mit dem gewählten Dienst ({}), je etwa {} Zeichen",
        if dienst.is_empty() { "azure" } else { &dienst },
        ECHT_SATZ.chars().count()
    ));
    for _ in 0..anzahl {
        let (dauer, erg) = gemessen(decke, echt_vertonen(&az)).await;
        // Hier zählt nur eine fertige Vertonung: Eine Absage des Dienstes
        // (falscher Schlüssel, Drossel) ist schnell und verfälschte die Reihe.
        let ausgang = match erg.unwrap_or(Ausgang::Decke) {
            Ausgang::Antwort(code) => Ausgang::Fehler(format!("absage http-{code}")),
            sonst => sonst,
        };
        p.melde("gewaehlt", "echt", dauer, &ausgang);
    }
}

/// Mittlerer Wert einer Messreihe.
fn median(werte: &mut [u128]) -> Option<u128> {
    if werte.is_empty() {
        return None;
    }
    werte.sort_unstable();
    Some(werte[werte.len() / 2])
}

/// Übersicht je Ziel; `!` hinter der Zahl = die Gegenstelle hat nicht
/// geantwortet (Fehler, Frist oder Decke), `*` = erste Messung am Ziel.
fn zusammenfassung(p: &mut Protokoll) {
    let spalten = [HEUTE, VERBFRIST, HARTEFRIST, "dns", "tcp", "produktiv"];
    let mut ziele: Vec<&'static str> = Vec::new();
    for w in &p.werte {
        if !ziele.contains(&w.ziel) {
            ziele.push(w.ziel);
        }
    }
    let mut zeilen = vec![format!(
        "UEBERSICHT lage={} (Millisekunden; ! = keine Antwort der Gegenstelle, * = erste Messung am Ziel)",
        p.lage
    )];
    let mut kopf = format!("UEBERSICHT {:<20}", "ziel");
    for s in spalten {
        kopf.push_str(&format!("{s:>11}"));
    }
    kopf.push_str(&format!("{:>26}", "grundlinie/echt"));
    zeilen.push(kopf);
    for ziel in ziele {
        let mut zeile = format!("UEBERSICHT {ziel:<20}");
        for s in spalten {
            let zelle = p
                .werte
                .iter()
                .find(|w| w.ziel == ziel && w.messung == s)
                .map(|w| {
                    format!(
                        "{}{}{}",
                        w.ms,
                        if w.traegt { "" } else { "!" },
                        if w.kalt { "*" } else { "" }
                    )
                })
                .unwrap_or_else(|| "-".to_string());
            zeile.push_str(&format!("{zelle:>11}"));
        }
        let versuche: Vec<&Messwert> = p
            .werte
            .iter()
            .filter(|w| w.ziel == ziel && (w.messung == "grundlinie" || w.messung == "echt"))
            .collect();
        // Median und Maximum nur über die gelungenen; `n=3/5` zeigt, dass
        // zwei von fünf Versuchen gescheitert sind.
        let mut reihe: Vec<u128> = versuche.iter().filter(|w| w.traegt).map(|w| w.ms).collect();
        let anzahl = reihe.len();
        let zelle = match median(&mut reihe) {
            Some(m) => format!(
                "{m} (max {}, n={anzahl}/{})",
                reihe[anzahl - 1],
                versuche.len()
            ),
            None if versuche.is_empty() => "-".to_string(),
            None => format!("- (n=0/{})", versuche.len()),
        };
        zeile.push_str(&format!("{zelle:>26}"));
        zeilen.push(zeile);
    }
    for z in zeilen {
        p.zeile(&z);
    }
}

#[test]
#[ignore]
fn tts_ausfall_probe() {
    let laufzeit = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Laufzeit startet");
    laufzeit.block_on(lauf());
    // Eine hängende Namensauflösung läuft in einem eigenen Thread des
    // Betriebssystems weiter, auch wenn die Sonde längst aufgegeben hat.
    // Darauf nicht warten — sonst hinge die Sonde nach der letzten Zeile.
    laufzeit.shutdown_timeout(Duration::from_secs(1));
}

async fn lauf() {
    let lage = env_text("BTS_PROBE_LAGE")
        .map(|l| schlicht(&l).chars().take(24).collect::<String>())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "unbenannt".to_string());
    let region = env_text("BTS_PROBE_REGION")
        .map(|r| schlicht(&r))
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "westeurope".to_string());
    let fristen = Fristen {
        verbindung: Duration::from_millis(env_zahl("BTS_PROBE_VERBINDUNGSFRIST_MS", 1500)),
        hart: Duration::from_millis(env_zahl("BTS_PROBE_HARTE_FRIST_MS", 2500)),
        decke: Duration::from_secs(env_zahl("BTS_PROBE_DECKE_S", 30)),
    };
    let wiederholungen = env_zahl("BTS_PROBE_WIEDERHOLUNGEN", 5).min(50);
    let auswahl: Option<Vec<String>> =
        env_text("BTS_PROBE_ZIELE").map(|a| a.split(',').map(schlicht).collect());
    let gewaehlt = |name: &str| match &auswahl {
        None => true,
        Some(a) => a.iter().any(|x| x == name),
    };

    // Wo die Ergebnisdatei liegt, sagt die Sonde ohne den Pfad: Er trägt
    // unter Windows den Benutzernamen, und die Ausgabe wird weitergegeben.
    let (datei_pfad, datei_ort) = match env_text("BTS_PROBE_DATEI") {
        Some(pfad) => (
            PathBuf::from(pfad),
            "die mit BTS_PROBE_DATEI benannte Datei".to_string(),
        ),
        None => (
            std::env::temp_dir().join(DATEI_NAME),
            if cfg!(windows) {
                format!("%TEMP%\\{DATEI_NAME}")
            } else {
                format!("{DATEI_NAME} im Temp-Verzeichnis")
            },
        ),
    };
    let datei = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&datei_pfad)
        .ok();
    let mut p = Protokoll {
        lage,
        start: Instant::now(),
        nr: 0,
        werte: Vec::new(),
        datei,
    };
    let kopf = format!(
        "LAUF lage={} system={} verbindungsfrist-ms={} harte-frist-ms={} decke-s={}",
        p.lage,
        std::env::consts::OS,
        fristen.verbindung.as_millis(),
        fristen.hart.as_millis(),
        fristen.decke.as_secs(),
    );
    p.zeile(&kopf);

    // Die vier Sprachdienste, mit der Gesamtfrist, die die App heute setzt.
    let dienst = |name, host: String, sekunden, anbieter| Ziel {
        name,
        url: format!("https://{host}/"),
        roh: Some((host, 443)),
        gesamt: Duration::from_secs(sekunden),
        anbieter: Some(anbieter),
    };
    let dienste = [
        dienst(
            "azure",
            format!("{region}.tts.speech.microsoft.com"),
            10,
            Anbieter::Azure,
        ),
        dienst(
            "elevenlabs",
            "api.elevenlabs.io".to_string(),
            10,
            Anbieter::ElevenLabs,
        ),
        dienst("openai", "api.openai.com".to_string(), 10, Anbieter::OpenAi),
        dienst(
            "gemini",
            "generativelanguage.googleapis.com".to_string(),
            20,
            Anbieter::Gemini,
        ),
    ];
    for (erste, z) in dienste.iter().filter(|z| gewaehlt(z.name)).enumerate() {
        ziel_messen(&mut p, z, erste, &fristen, &region, wiederholungen).await;
    }

    if gewaehlt("sim") {
        let sim = |name, url: String, roh| Ziel {
            name,
            url,
            roh,
            gesamt: Duration::from_secs(10),
            anbieter: None,
        };
        let mut faelle = Vec::new();
        // Erst die stumme Gegenstelle binden, dann den geschlossenen Port
        // suchen: Solange sie ihren Port hält, kann das System ihn nicht
        // noch einmal als „geschlossenen" herausgeben.
        // Die Aufgabe hält die angenommenen Verbindungen bis zum Ende offen.
        let stumm = stumme_gegenstelle().await;
        match geschlossener_port() {
            Ok(port) => faelle.push(sim(
                "sim-abgewiesen",
                format!("http://127.0.0.1:{port}/"),
                None,
            )),
            Err(e) => p.zeile(&format!(
                "HINWEIS sim-abgewiesen entfällt: {}",
                fehler_art(&e)
            )),
        }
        match &stumm {
            Ok((port, _)) => {
                faelle.push(sim(
                    "sim-stumm-tls",
                    format!("https://127.0.0.1:{port}/"),
                    None,
                ));
                faelle.push(sim(
                    "sim-stumm-http",
                    format!("http://127.0.0.1:{port}/"),
                    None,
                ));
            }
            Err(e) => p.zeile(&format!("HINWEIS sim-stumm entfällt: {}", fehler_art(e))),
        }
        let loch: IpAddr = env_text("BTS_PROBE_SCHWARZES_LOCH")
            .and_then(|a| a.parse().ok())
            .unwrap_or(IpAddr::from([192, 0, 2, 1]));
        faelle.push(sim(
            "sim-schwarzes-loch",
            format!("https://{}/", SocketAddr::new(loch, 443)),
            Some((loch.to_string(), 443)),
        ));
        let unbekannt = "bts-light-messung.invalid";
        faelle.push(sim(
            "sim-name-unbekannt",
            format!("https://{unbekannt}/"),
            Some((unbekannt.to_string(), 443)),
        ));
        for z in &faelle {
            ziel_messen(&mut p, z, 0, &fristen, &region, 0).await;
        }
        if let Ok((_, aufgabe)) = stumm {
            aufgabe.abort();
        }
    }

    let echt = env_zahl("BTS_PROBE_ECHT", 0);
    if echt > 0 {
        echt_messen(&mut p, echt.min(20), fristen.decke).await;
    }

    zusammenfassung(&mut p);
    if p.datei.is_some() {
        println!("Ergebnisdatei (jeder Lauf hängt an): {datei_ort}");
    } else {
        println!(
            "Ergebnisdatei nicht schreibbar ({datei_ort}) — die Zeilen LAUF, MESSWERT, \
             HINWEIS und UEBERSICHT von Hand sichern"
        );
    }
}
