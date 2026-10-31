//! WASAPI-Ausgabe der Ansage über cpal (ADR 0065).
//!
//! **Ein eigener Audio-Thread** mit fester COM-Initialisierung: Gemessen am
//! 31.10.2026 scheiterte das Öffnen nach einem Gerätewechsel mit
//! `RPC_E_CHANGED_MODE`, als Auflösen und Öffnen auf einem Thread mit
//! wechselndem COM-Modus liefen — rund 8 s Stille. Tritt der Fehler trotzdem
//! auf, wird der Thread verworfen und neu aufgesetzt, statt nur zu
//! wiederholen.
//!
//! **Gerät je Stück neu auflösen:** Ein dauerhaft offener Strom stirbt beim
//! Abziehen und bleibt still, auch wenn das Gerät zurückkommt. Deshalb wird
//! für jedes Stück das aktuelle Windows-Standardgerät geholt, ein Strom
//! gebaut, abgespielt und wieder geschlossen.
//!
//! **Datenschutz:** Gerätenamen gehen nie ins Log (Bluetooth-Geräte tragen
//! oft Personennamen) — geloggt werden nur Ereignisarten.

use super::super::{generation, ist_com_moduswechsel, pcm, Abspielbericht, Art, TonFehler};
use ::windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, Sample, SampleFormat, SizedSample, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// Öffnen scheitert → neu auflösen und erneut versuchen, höchstens so oft.
const OEFFNEN_VERSUCHE: u8 = 3;
/// Pause zwischen zwei Öffnungsversuchen — Zeit für Windows, das neue
/// Standardgerät einzutragen.
const OEFFNEN_PAUSE: Duration = Duration::from_millis(300);
/// Kommt so lange kein Rückruf mehr, gilt der Strom als abgerissen. Ein
/// lebender WASAPI-Strom ruft alle ~10 ms; cpal meldet nicht jeden Tod
/// über den Fehler-Rückruf (z. B. wenn das abgezogene Gerät durch ein
/// neues Standardgerät ersetzt wird).
const STILLSTAND: Duration = Duration::from_millis(2000);
/// Takt, in dem der Audio-Thread auf Ende, Abbruch und Abriss schaut.
const TAKT: Duration = Duration::from_millis(20);

/// Antwort des Audio-Threads auf einen Auftrag.
enum Innen {
    Fertig(Abspielbericht),
    Fehler(TonFehler),
    /// `RPC_E_CHANGED_MODE`: Thread verwerfen, Auftrag auf einem neuen
    /// Thread wiederholen.
    ThreadNeu,
}

struct Auftrag {
    gen: u64,
    samples: Arc<Vec<i16>>,
    rate: u32,
    art: Art,
    antwort: oneshot::Sender<Innen>,
}

/// Der laufende Audio-Thread. Die Kennung verhindert, dass ein verspäteter
/// Aufrufer einen schon neu aufgesetzten Thread verwirft.
struct Faden {
    id: u64,
    tx: mpsc::Sender<Auftrag>,
}

static FADEN: Mutex<Option<Faden>> = Mutex::new(None);
static FADEN_ID: AtomicU64 = AtomicU64::new(0);

/// Holt den Audio-Thread und startet ihn beim ersten Bedarf.
fn faden() -> Result<(u64, mpsc::Sender<Auftrag>), TonFehler> {
    let mut slot = FADEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = slot.as_ref() {
        return Ok((f.id, f.tx.clone()));
    }
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("ansage-ton".to_string())
        .spawn(move || faden_haupt(rx))
        .map_err(|e| TonFehler::Intern(format!("Audio-Thread startet nicht: {e}")))?;
    let id = FADEN_ID.fetch_add(1, Ordering::SeqCst);
    *slot = Some(Faden { id, tx: tx.clone() });
    tracing::info!("Ansage-Ton: Audio-Thread gestartet");
    Ok((id, tx))
}

/// Verwirft den Audio-Thread `id` (falls er noch der aktuelle ist). Er
/// endet von selbst, sobald sein Kanal leer und geschlossen ist.
fn faden_verwerfen(id: u64) {
    let mut slot = FADEN.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().map(|f| f.id) == Some(id) {
        *slot = None;
    }
}

/// Gibt den aktuellen Audio-Thread auf (siehe `abbrechen_hart`), egal ob er
/// hängt oder nur noch ein abgebrochenes Stück beendet.
pub fn faden_aufgeben() {
    let mut slot = FADEN.lock().unwrap_or_else(|e| e.into_inner());
    if slot.take().is_some() {
        tracing::warn!("Ansage-Ton: Frist abgelaufen – Audio-Thread aufgegeben");
    }
}

/// Spielt ein Stück auf dem Audio-Thread und wartet auf dessen Ende.
pub async fn spielen(
    gen: u64,
    samples: Vec<i16>,
    rate: u32,
    art: Art,
) -> Result<Abspielbericht, TonFehler> {
    let samples = Arc::new(samples);
    let mut neu_aufgesetzt = 0u8;
    // Zwei Runden: die normale und eine auf einem frisch aufgesetzten Thread.
    for _ in 0..2 {
        if generation() != gen {
            return Ok(Abspielbericht::abgebrochen(neu_aufgesetzt));
        }
        let (id, tx) = faden()?;
        let (antwort, empfang) = oneshot::channel();
        let auftrag = Auftrag {
            gen,
            samples: Arc::clone(&samples),
            rate,
            art,
            antwort,
        };
        if tx.send(auftrag).is_err() {
            faden_verwerfen(id);
            neu_aufgesetzt = neu_aufgesetzt.saturating_add(1);
            continue;
        }
        match empfang.await {
            Ok(Innen::Fertig(mut bericht)) => {
                bericht.neu_geoeffnet = bericht.neu_geoeffnet.saturating_add(neu_aufgesetzt);
                return Ok(bericht);
            }
            Ok(Innen::Fehler(e)) => return Err(e),
            Ok(Innen::ThreadNeu) => {
                tracing::warn!(
                    "Ansage-Ton: COM-Moduswechsel beim Öffnen – Audio-Thread wird neu aufgesetzt"
                );
                faden_verwerfen(id);
                neu_aufgesetzt = neu_aufgesetzt.saturating_add(1);
            }
            Err(_) => {
                // Der Thread ist ohne Antwort beendet (Neuaufsetzen durch
                // einen anderen Auftrag oder Panik) — neu versuchen.
                tracing::warn!("Ansage-Ton: Audio-Thread ohne Antwort beendet – neuer Versuch");
                faden_verwerfen(id);
                neu_aufgesetzt = neu_aufgesetzt.saturating_add(1);
            }
        }
    }
    Err(TonFehler::Intern(
        "Audio-Thread ließ sich nicht neu aufsetzen".to_string(),
    ))
}

/// Hauptschleife des Audio-Threads: Aufträge strikt nacheinander.
fn faden_haupt(rx: mpsc::Receiver<Auftrag>) {
    // Feste COM-Initialisierung, bevor cpal das erste Mal etwas anfasst:
    // STA, wie cpal sie selbst wählt. Damit kann kein späterer Aufruf den
    // Modus dieses Threads mehr umstellen.
    // SAFETY: Erster COM-Aufruf auf diesem frischen Thread; das Gegenstück
    // `CoUninitialize` läuft unten auf demselben Thread.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if com.is_err() {
        tracing::warn!("Ansage-Ton: COM-Initialisierung des Audio-Threads gescheitert");
    }
    while let Ok(auftrag) = rx.recv() {
        let ergebnis = stueck(&auftrag);
        let neu = matches!(ergebnis, Innen::ThreadNeu);
        let _ = auftrag.antwort.send(ergebnis);
        if neu {
            break;
        }
    }
    if com.is_ok() {
        // SAFETY: Gegenstück zum erfolgreichen `CoInitializeEx` oben; alle
        // cpal-Objekte dieses Threads sind zu diesem Zeitpunkt freigegeben.
        unsafe { CoUninitialize() };
    }
}

/// Spielt ein Stück: auflösen, öffnen (mit Versuchen), abspielen, bei
/// Abriss Sprache einmal von vorn.
fn stueck(a: &Auftrag) -> Innen {
    let mut neu = 0u8;
    let mut von_vorn = false;
    loop {
        if generation() != a.gen {
            return Innen::Fertig(Abspielbericht::abgebrochen(neu));
        }
        let offen = match oeffnen_mit_versuchen(a, &mut neu) {
            Ok(o) => o,
            Err(innen) => return innen,
        };
        let ende = warten(&offen, a.gen);
        drop(offen);
        match ende {
            Ende::Fertig => {
                return Innen::Fertig(Abspielbericht {
                    abgebrochen: false,
                    neu_geoeffnet: neu,
                })
            }
            Ende::Abgebrochen => return Innen::Fertig(Abspielbericht::abgebrochen(neu)),
            Ende::Abriss => match a.art {
                Art::Gong => {
                    tracing::info!(
                        "Ansage-Ton: Gerät mitten im Gong weggefallen – Gong ausgelassen"
                    );
                    return Innen::Fertig(Abspielbericht {
                        abgebrochen: false,
                        neu_geoeffnet: neu,
                    });
                }
                Art::Sprache if !von_vorn => {
                    tracing::info!(
                        "Ansage-Ton: Gerät mitten in der Ansage weggefallen – Stück beginnt von vorn"
                    );
                    von_vorn = true;
                    neu = neu.saturating_add(1);
                }
                Art::Sprache => {
                    tracing::warn!("Ansage-Ton: Gerät zweimal mitten in der Ansage weggefallen");
                    return Innen::Fehler(TonFehler::Intern(
                        "Ausgabegerät mitten in der Ansage zweimal weggefallen".to_string(),
                    ));
                }
            },
        }
    }
}

/// Fehler beim Öffnen eines Stroms.
enum OeffnenFehler {
    /// `RPC_E_CHANGED_MODE` — der Thread muss neu aufgesetzt werden.
    Modus,
    Ton(TonFehler),
}

fn cpal_fehler(e: cpal::Error) -> OeffnenFehler {
    let text = e.to_string();
    if ist_com_moduswechsel(&text) {
        return OeffnenFehler::Modus;
    }
    match e.kind() {
        ErrorKind::DeviceNotAvailable => OeffnenFehler::Ton(TonFehler::KeinGeraet),
        kind => OeffnenFehler::Ton(TonFehler::Intern(format!("{kind:?}: {text}"))),
    }
}

/// Wartet `dauer`, schaut dabei aber auf einen Abbruch. `false` = abgebrochen.
fn schlafen_pruefend(dauer: Duration, gen: u64) -> bool {
    let ende = Instant::now() + dauer;
    while Instant::now() < ende {
        if generation() != gen {
            return false;
        }
        std::thread::sleep(TAKT);
    }
    generation() == gen
}

fn oeffnen_mit_versuchen(a: &Auftrag, neu: &mut u8) -> Result<Offen, Innen> {
    let mut letzter = TonFehler::KeinGeraet;
    for versuch in 0..OEFFNEN_VERSUCHE {
        if versuch > 0 {
            *neu = neu.saturating_add(1);
            if !schlafen_pruefend(OEFFNEN_PAUSE, a.gen) {
                return Err(Innen::Fertig(Abspielbericht::abgebrochen(*neu)));
            }
        }
        match oeffnen(a) {
            Ok(o) => return Ok(o),
            Err(OeffnenFehler::Modus) => return Err(Innen::ThreadNeu),
            Err(OeffnenFehler::Ton(e)) => {
                // Bei `intern` die cpal-Fehlerart und den Windows-Text mit
                // ausgeben — ohne sie ist ein Feldfehler nicht zu deuten.
                // cpal-Texte nennen keine Gerätenamen („Failed to get audio
                // client … (os error …)").
                let detail = match &e {
                    TonFehler::Intern(t) => t.as_str(),
                    _ => "",
                };
                tracing::warn!(
                    versuch = versuch + 1,
                    fehler = e.code(),
                    detail,
                    "Ansage-Ton: Öffnen des Ausgabegeräts gescheitert"
                );
                letzter = e;
            }
        }
    }
    Err(Innen::Fehler(letzter))
}

/// Laufzustand eines Stroms, geteilt zwischen Rückruf und Audio-Thread.
#[derive(Default)]
struct Zustand {
    /// Nächstes zu schreibendes Mono-Sample.
    pos: AtomicUsize,
    /// Abbruch: Rückruf schreibt nur noch Stille.
    stopp: AtomicBool,
    /// Fehler-Rückruf meldete ein totes Gerät.
    abriss: AtomicBool,
}

/// Ein offener, laufender Strom. Fallen lassen schließt ihn.
struct Offen {
    _strom: cpal::Stream,
    zustand: Arc<Zustand>,
    laenge: usize,
    signal: mpsc::Receiver<()>,
}

/// Endpunkt-ID des aktuellen Windows-Standardgeräts (Wiedergabe, „Konsole"
/// = das Gerät, das Windows als Standard anzeigt). Braucht COM auf dem
/// aufrufenden Thread.
fn standard_endpunkt_id() -> Option<String> {
    // SAFETY: COM ist auf dem aufrufenden Thread initialisiert (Audio-Thread
    // bzw. Namens-Thread); der von `GetId` belegte String wird genau einmal
    // mit `CoTaskMemFree` freigegeben.
    unsafe {
        let aufz: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let geraet = aufz.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let roh = geraet.GetId().ok()?;
        let id = roh.to_string().ok();
        CoTaskMemFree(Some(roh.0 as *const _));
        id
    }
}

/// Das aktuelle Standardgerät als **konkretes** Gerät. cpal 0.18 liefert bei
/// `default_output_device()` das virtuelle „Standard-Ausgabe"-Gerät von
/// Windows (`ActivateAudioInterfaceAsync`); darauf scheiterte in der App
/// schon `GetMixFormat` mit `AUDCLNT_E_UNSUPPORTED_FORMAT` und nach einem
/// Gerätewechsel das Öffnen mit `RPC_E_CHANGED_MODE` (Hörtest 31.10.2026).
/// Über die Endpunkt-ID geöffnet gab es beides nicht. Das virtuelle Gerät
/// bleibt nur Rückfall, falls Windows keine ID liefert.
fn standard_geraet(host: &cpal::Host) -> Option<cpal::Device> {
    if let Some(id) = standard_endpunkt_id() {
        let cpal_id = cpal::DeviceId::new(cpal::HostId::Wasapi, &id);
        if let Some(geraet) = host.device_by_id(&cpal_id) {
            return Some(geraet);
        }
    }
    host.default_output_device()
}

fn oeffnen(a: &Auftrag) -> Result<Offen, OeffnenFehler> {
    let host = cpal::default_host();
    let geraet = standard_geraet(&host).ok_or(OeffnenFehler::Ton(TonFehler::KeinGeraet))?;
    let konfig = geraet.default_output_config().map_err(cpal_fehler)?;
    let kanaele = usize::from(konfig.channels()).max(1);
    let rate = konfig.sample_rate();
    let format = konfig.sample_format();
    let quelle = Arc::new(pcm::mit_rand(
        pcm::resample_linear(&a.samples, a.rate, rate),
        rate,
        pcm::RAND_VORN_MS,
        pcm::RAND_HINTEN_MS,
    ));
    let zustand = Arc::new(Zustand::default());
    let (signal_tx, signal) = mpsc::sync_channel(1);
    let teile = Teile {
        kanaele,
        quelle: &quelle,
        zustand: &zustand,
        signal: signal_tx,
    };
    let sk = konfig.config();
    let strom = match format {
        SampleFormat::F32 => bauen::<f32>(&geraet, sk, teile),
        SampleFormat::I16 => bauen::<i16>(&geraet, sk, teile),
        SampleFormat::I32 => bauen::<i32>(&geraet, sk, teile),
        SampleFormat::U16 => bauen::<u16>(&geraet, sk, teile),
        SampleFormat::U32 => bauen::<u32>(&geraet, sk, teile),
        SampleFormat::I8 => bauen::<i8>(&geraet, sk, teile),
        SampleFormat::U8 => bauen::<u8>(&geraet, sk, teile),
        SampleFormat::F64 => bauen::<f64>(&geraet, sk, teile),
        SampleFormat::I64 => bauen::<i64>(&geraet, sk, teile),
        SampleFormat::U64 => bauen::<u64>(&geraet, sk, teile),
        andere => {
            return Err(OeffnenFehler::Ton(TonFehler::Intern(format!(
                "Sampleformat {andere:?} wird nicht unterstützt"
            ))))
        }
    }?;
    strom.play().map_err(cpal_fehler)?;
    Ok(Offen {
        _strom: strom,
        laenge: quelle.len(),
        zustand,
        signal,
    })
}

/// Was der Rückruf eines Stroms braucht.
struct Teile<'a> {
    kanaele: usize,
    quelle: &'a Arc<Vec<f32>>,
    zustand: &'a Arc<Zustand>,
    /// Weckt den Audio-Thread, sobald alles geschrieben ist oder ein Fehler
    /// kam — der Takt bleibt nur Sicherheitsnetz.
    signal: mpsc::SyncSender<()>,
}

fn bauen<T>(
    geraet: &cpal::Device,
    konfig: StreamConfig,
    teile: Teile<'_>,
) -> Result<cpal::Stream, OeffnenFehler>
where
    T: SizedSample + cpal::FromSample<f32>,
{
    let kanaele = teile.kanaele;
    let quelle = Arc::clone(teile.quelle);
    let z_daten = Arc::clone(teile.zustand);
    let z_fehler = Arc::clone(teile.zustand);
    let signal_daten = teile.signal.clone();
    let signal_fehler = teile.signal;
    geraet
        .build_output_stream::<T, _, _>(
            konfig,
            move |daten: &mut [T], _info: &cpal::OutputCallbackInfo| {
                let stopp = z_daten.stopp.load(Ordering::Relaxed);
                let mut pos = z_daten.pos.load(Ordering::Relaxed);
                for rahmen in daten.chunks_mut(kanaele) {
                    let wert = match quelle.get(pos) {
                        Some(&v) if !stopp => {
                            pos += 1;
                            v
                        }
                        _ => 0.0,
                    };
                    let s = <T as Sample>::from_sample(wert);
                    rahmen.fill(s);
                }
                z_daten.pos.store(pos, Ordering::Relaxed);
                if pos >= quelle.len() {
                    let _ = signal_daten.try_send(());
                }
            },
            move |fehler: cpal::Error| {
                let kind = fehler.kind();
                match kind {
                    // Strom läuft weiter: Standardgerät gewechselt (der
                    // alte Strom spielt bis zum Ende des Stücks auf dem
                    // alten Gerät, Spec: kein Umhängen mitten im Stück),
                    // kurzer Aussetzer, Priorität verweigert.
                    ErrorKind::StreamInvalidated
                    | ErrorKind::DeviceChanged
                    | ErrorKind::Xrun
                    | ErrorKind::RealtimeDenied => {
                        tracing::debug!(art = ?kind, "Ansage-Ton: Hinweis vom Ausgabestrom");
                    }
                    _ => {
                        tracing::warn!(art = ?kind, "Ansage-Ton: Ausgabestrom abgerissen");
                        z_fehler.abriss.store(true, Ordering::Relaxed);
                        let _ = signal_fehler.try_send(());
                    }
                }
            },
            None,
        )
        .map_err(cpal_fehler)
}

enum Ende {
    Fertig,
    Abgebrochen,
    Abriss,
}

/// Wartet, bis alles samt Nachlauf geschrieben ist, abgebrochen wurde oder
/// der Strom abreißt bzw. stehen bleibt.
fn warten(offen: &Offen, gen: u64) -> Ende {
    let z = &offen.zustand;
    let mut letzte_pos = usize::MAX;
    let mut seit = Instant::now();
    loop {
        // Signal oder Takt — beides führt zur selben Prüfung.
        let _ = offen.signal.recv_timeout(TAKT);
        if generation() != gen {
            z.stopp.store(true, Ordering::Relaxed);
            return Ende::Abgebrochen;
        }
        let pos = z.pos.load(Ordering::Relaxed);
        if pos >= offen.laenge {
            return Ende::Fertig;
        }
        if z.abriss.load(Ordering::Relaxed) {
            return Ende::Abriss;
        }
        if pos != letzte_pos {
            letzte_pos = pos;
            seit = Instant::now();
        } else if seit.elapsed() > STILLSTAND {
            tracing::warn!("Ansage-Ton: Ausgabestrom steht still – gilt als abgerissen");
            return Ende::Abriss;
        }
    }
}

/// Name des aktuellen Windows-Standardgeräts, nur für die Anzeige.
///
/// Auf einem kurzlebigen eigenen Thread statt auf dem Audio-Thread: Die
/// Anzeige fragt alle 3 s, und sie soll nicht hinter einer laufenden
/// Ansage warten. Ein frischer Thread hat keinen COM-Modus, den jemand
/// anderes vorher festgelegt haben könnte.
pub async fn standard_geraet_name() -> Result<Option<String>, TonFehler> {
    let (tx, rx) = oneshot::channel();
    std::thread::Builder::new()
        .name("ansage-ton-name".to_string())
        .spawn(move || {
            let _ = tx.send(name_lesen());
        })
        .map_err(|e| TonFehler::Intern(format!("Thread startet nicht: {e}")))?;
    rx.await
        .map_err(|_| TonFehler::Intern("Gerätename: Thread ohne Antwort".to_string()))?
}

fn name_lesen() -> Result<Option<String>, TonFehler> {
    // Frischer Thread: COM selbst einrichten (für `standard_endpunkt_id`).
    // SAFETY: Erster COM-Aufruf auf diesem kurzlebigen Thread; das
    // Gegenstück folgt unten auf demselben Thread.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let ergebnis = match standard_geraet(&cpal::default_host()) {
        None => Ok(None),
        Some(geraet) => geraet
            .description()
            .map(|d| Some(d.name().to_string()))
            .map_err(|e| TonFehler::Intern(format!("Gerätename: {:?}", e.kind()))),
    };
    if com.is_ok() {
        // SAFETY: Gegenstück zum erfolgreichen `CoInitializeEx` oben; alle
        // COM-Objekte dieses Threads sind freigegeben.
        unsafe { CoUninitialize() };
    }
    ergebnis
}
