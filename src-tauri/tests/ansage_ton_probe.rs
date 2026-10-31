//! **Messwerkzeug, kein Regressionstest.** Spielt der Ansage-Ton aus dem
//! Kern (Spec `ansage-ausgabegeraet`, ADR 0065/0066) zuverlässig über das
//! aktuelle Windows-Standardgerät — auch über Gerätewechsel hinweg?
//!
//! Ablauf: alle 2 s ein kurzer Ton (Art Sprache) über die öffentliche API.
//! Während die Sonde läuft, von Hand prüfen:
//!
//! 1. **Wechsel zwischen zwei Tönen:** USB-Box/Klinke einstecken oder
//!    abziehen → der nächste Ton kommt über das neue Standardgerät.
//! 2. **Abriss mitten im Ton** (`ANSAGE_PROBE_MS=1500` für längere Töne):
//!    abziehen → der Ton beginnt auf dem neuen Gerät von vorn
//!    (`neuGeoeffnet` ≥ 1).
//! 3. **`RPC_E_CHANGED_MODE`:** fällt beim Wechsel ein Öffnen damit, setzt
//!    der Kern den Audio-Thread neu auf; spätestens der nächste Ton ist
//!    hörbar (kein mehrsekündiges Loch wie in Messung 5).
//!
//! Dazu: Zahl der Windows-Stimmen, Latenz der WinRT-Synthese und ein
//! gesprochener Satz über denselben Weg.
//!
//! Standardmäßig übersprungen (`#[ignore]`) — braucht ein Ausgabegerät:
//!
//! ```text
//! cargo test -p bts-light --test ansage_ton_probe -- --ignored --nocapture
//! ```
//!
//! Umgebungsvariablen: `ANSAGE_PROBE_RUNDEN` (Standard 5),
//! `ANSAGE_PROBE_MS` (Tonlänge, Standard 400).
//!
//! **Datenschutz:** Die Ausgabe nennt **keine Gerätenamen** (Bluetooth-
//! Geräte tragen oft Personennamen) — nur Bericht und Fehlercode.

use bts_light_lib::ansage_ton::{self as ton, Art};
use std::time::{Duration, Instant};

fn env_zahl(name: &str, standard: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(standard)
}

/// Sinuston 660 Hz mit weichem Ein-/Ausklang, Mono-PCM16 bei 24 kHz.
fn ton_pcm(ms: u64) -> (Vec<i16>, u32) {
    let rate = 24_000u32;
    let n = (u64::from(rate) * ms / 1000) as usize;
    let rampe = (rate / 100) as usize; // 10 ms
    let samples = (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            let huelle = (i.min(n - 1 - i).min(rampe) as f64) / rampe as f64;
            ((t * 660.0 * std::f64::consts::TAU).sin() * 9000.0 * huelle) as i16
        })
        .collect();
    (samples, rate)
}

#[tokio::test]
#[ignore]
async fn ansage_ton_probe() {
    let runden = env_zahl("ANSAGE_PROBE_RUNDEN", 5);
    let ms = env_zahl("ANSAGE_PROBE_MS", 400);
    println!("unterstuetzt: {}", ton::unterstuetzt());

    match ton::standard_geraet_name().await {
        Ok(Some(_)) => println!("Standardgerät: vorhanden (Name absichtlich nicht ausgegeben)"),
        Ok(None) => println!("Standardgerät: keins aktiv"),
        Err(e) => println!("Standardgerät: Fehler {}", e.code()),
    }

    let (pcm, rate) = ton_pcm(ms);
    for runde in 1..=runden {
        let start = Instant::now();
        let ergebnis = ton::spielen(pcm.clone(), rate, Art::Sprache).await;
        let dauer = start.elapsed().as_millis();
        match ergebnis {
            Ok(b) => println!(
                "Runde {runde:>2}: ok in {dauer} ms, abgebrochen={}, neuGeoeffnet={}",
                b.abgebrochen, b.neu_geoeffnet
            ),
            Err(e) => println!("Runde {runde:>2}: Fehler {} nach {dauer} ms", e.code()),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Abbruch mitten im Ton: muss zügig mit abgebrochen=true zurückkehren.
    let (lang_pcm, lang_rate) = ton_pcm(3000);
    let start = Instant::now();
    let laeuft = tokio::spawn(ton::spielen(lang_pcm, lang_rate, Art::Sprache));
    tokio::time::sleep(Duration::from_millis(500)).await;
    ton::abbrechen();
    match laeuft.await.expect("Task") {
        Ok(b) => println!(
            "Abbruch: abgebrochen={} nach {} ms",
            b.abgebrochen,
            start.elapsed().as_millis()
        ),
        Err(e) => println!("Abbruch: Fehler {}", e.code()),
    }

    match ton::stimmen().await {
        Ok(liste) => {
            let de = liste.iter().filter(|s| s.sprache.starts_with("de")).count();
            println!("Windows-Stimmen: {} (davon Deutsch: {de})", liste.len());
        }
        Err(e) => println!("Windows-Stimmen: Fehler {}", e.code()),
    }

    let segmente = vec![
        "Spiel Nummer 12, Herreneinzel.".to_string(),
        "Bitte auf Feld 3.".to_string(),
    ];
    for runde in 1..=2 {
        let start = Instant::now();
        match ton::stimme_pcm(segmente.clone(), "de".into(), String::new(), 0.8).await {
            Ok((p, r)) => println!(
                "WinRT-Synthese {runde}: {} ms, {} Samples @ {r} Hz ({:.1} s)",
                start.elapsed().as_millis(),
                p.len(),
                p.len() as f64 / f64::from(r)
            ),
            Err(e) => println!("WinRT-Synthese {runde}: Fehler {}", e.code()),
        }
    }
    let start = Instant::now();
    match ton::stimme_spielen(segmente, "de".into(), String::new(), 0.8).await {
        Ok(b) => println!(
            "Gesprochen: ok in {} ms, neuGeoeffnet={}",
            start.elapsed().as_millis(),
            b.neu_geoeffnet
        ),
        Err(e) => println!("Gesprochen: Fehler {}", e.code()),
    }
}
