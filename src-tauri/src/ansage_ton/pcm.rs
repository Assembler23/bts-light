//! Reine PCM-Rechnerei des Ansage-Tons (ADR 0065): Bytes lesen, WAV
//! zerlegen, Rate angleichen, Rand-Stille anfügen.
//!
//! Läuft auf beiden Plattformen und hat keinen Bezug zu Geräten — alles,
//! was hier steht, ist in der CI testbar. Die Ausgabe selbst
//! (`windows/ausgabe.rs`) fährt nur noch das Ergebnis ab.

use super::TonFehler;

/// Längste Ansage, die über Raw-IPC angenommen wird. Eine Feldansage dauert
/// rund 10–15 s; 60 s lassen Luft für Freitexte und halten den Speicher
/// (und einen Missbrauch der IPC-Schnittstelle) klein.
pub const MAX_SEKUNDEN: u32 = 60;

/// Stille vor dem Ton: Manche Geräte verschlucken sonst die ersten
/// Millisekunden, während der Strom anläuft.
pub const RAND_VORN_MS: u32 = 20;

/// Nachlauf nach dem Ton: „Fertig" gilt erst, wenn auch dieser geschrieben
/// ist — so ist der letzte echte Ton sicher aus dem Gerätepuffer heraus,
/// bevor der Strom geschlossen wird.
pub const RAND_HINTEN_MS: u32 = 150;

/// Legt PCM16 little-endian (wie es das WebView schickt) in Samples um.
/// Eine ungerade Byte-Zahl ist kein PCM16 — lieber ablehnen als ein halbes
/// Sample zu raten.
pub fn pcm16_aus_le_bytes(bytes: &[u8]) -> Result<Vec<i16>, TonFehler> {
    if !bytes.len().is_multiple_of(2) {
        return Err(TonFehler::Ungueltig(
            "PCM16 braucht eine gerade Byte-Zahl".to_string(),
        ));
    }
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| i16::from_le_bytes(*p))
        .collect())
}

/// Prüft die Länge einer Ansage gegen einen Deckel in Sekunden.
pub fn dauer_pruefen(samples: usize, rate: u32, max_sekunden: u32) -> Result<(), TonFehler> {
    if rate == 0 {
        return Err(TonFehler::Ungueltig("Abtastrate 0".to_string()));
    }
    if samples as u64 > u64::from(rate) * u64::from(max_sekunden) {
        return Err(TonFehler::Ungueltig(format!(
            "Ansage länger als {max_sekunden} s"
        )));
    }
    Ok(())
}

fn u16_le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// Zerlegt eine RIFF/WAVE-Datei (wie sie die Windows-Stimme liefert) in
/// Mono-PCM16 und Abtastrate.
///
/// Angenommen wird nur, was die Ausgabe sicher abspielen kann: PCM16, ein
/// oder zwei Kanäle (Stereo wird gemittelt). Die Chunks werden der Reihe
/// nach abgelaufen, statt feste Offsets anzunehmen — WinRT schreibt je nach
/// Version zusätzliche Chunks (z. B. `LIST`) vor die Daten.
pub fn wav_lesen(bytes: &[u8]) -> Result<(Vec<i16>, u32), TonFehler> {
    let kaputt = |was: &str| TonFehler::Ungueltig(format!("WAV: {was}"));
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(kaputt("kein RIFF/WAVE-Kopf"));
    }
    // (Format-Tag, Kanäle, Rate, Bits)
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut daten: Option<&[u8]> = None;
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let groesse = u32_le(bytes, pos + 4).ok_or_else(|| kaputt("Chunk-Kopf"))? as usize;
        let start = pos + 8;
        // Streams schreiben gelegentlich eine Platzhalter-Größe in den
        // data-Chunk — dann gilt, was tatsächlich da ist.
        let ende = start.saturating_add(groesse).min(bytes.len());
        let inhalt = &bytes[start..ende];
        match id {
            b"fmt " => {
                let tag = u16_le(inhalt, 0).ok_or_else(|| kaputt("fmt zu kurz"))?;
                let kanaele = u16_le(inhalt, 2).ok_or_else(|| kaputt("fmt zu kurz"))?;
                let rate = u32_le(inhalt, 4).ok_or_else(|| kaputt("fmt zu kurz"))?;
                let bits = u16_le(inhalt, 14).ok_or_else(|| kaputt("fmt zu kurz"))?;
                // WAVE_FORMAT_EXTENSIBLE: Das eigentliche Format steht in den
                // ersten zwei Bytes der Subformat-GUID (Offset 24).
                let tag = if tag == 0xFFFE {
                    u16_le(inhalt, 24).ok_or_else(|| kaputt("Extensible zu kurz"))?
                } else {
                    tag
                };
                fmt = Some((tag, kanaele, rate, bits));
            }
            b"data" => {
                daten = Some(inhalt);
            }
            _ => {}
        }
        // Chunks sind auf gerade Längen aufgefüllt.
        pos = start.saturating_add(groesse).saturating_add(groesse % 2);
    }
    let (tag, kanaele, rate, bits) = fmt.ok_or_else(|| kaputt("fmt-Chunk fehlt"))?;
    let daten = daten.ok_or_else(|| kaputt("data-Chunk fehlt"))?;
    if tag != 1 || bits != 16 {
        return Err(kaputt("nur PCM16 wird unterstützt"));
    }
    if rate == 0 {
        return Err(kaputt("Abtastrate 0"));
    }
    // Ein unvollständiges letztes Sample am Ende abschneiden statt
    // abzulehnen: Es ist ein Bruchteil einer Millisekunde.
    let gerade = daten.len() - daten.len() % 2;
    let roh = pcm16_aus_le_bytes(&daten[..gerade])?;
    let mono = match kanaele {
        1 => roh,
        2 => roh
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[l, r]| ((i32::from(*l) + i32::from(*r)) / 2) as i16)
            .collect(),
        _ => return Err(kaputt("nur Mono oder Stereo")),
    };
    Ok((mono, rate))
}

/// Lineare Interpolation von `von` Hz auf `nach` Hz, Ergebnis als `f32` im
/// Bereich −1…1 (das Format, in dem die Ausgabe weiterrechnet).
///
/// Bewusst schlicht: Für Sprache und Gong reicht das, eine Resampling-Crate
/// lohnt nicht. Klingt es im Hörtest rau, ist kubische Hermite-Interpolation
/// der Ersatz an genau dieser Stelle.
pub fn resample_linear(samples: &[i16], von: u32, nach: u32) -> Vec<f32> {
    const SKALA: f32 = 32768.0;
    if samples.is_empty() || von == 0 || nach == 0 {
        return Vec::new();
    }
    if von == nach {
        return samples.iter().map(|&s| f32::from(s) / SKALA).collect();
    }
    let n = samples.len();
    let laenge = ((n as f64) * f64::from(nach) / f64::from(von)).round() as usize;
    let schritt = f64::from(von) / f64::from(nach);
    (0..laenge)
        .map(|i| {
            let pos = i as f64 * schritt;
            let idx = (pos.floor() as usize).min(n - 1);
            let frac = (pos - idx as f64) as f32;
            let a = f32::from(samples[idx]);
            let b = f32::from(samples[(idx + 1).min(n - 1)]);
            (a + (b - a) * frac) / SKALA
        })
        .collect()
}

/// Anzahl Samples für `ms` Millisekunden bei `rate` Hz.
fn samples_fuer(rate: u32, ms: u32) -> usize {
    (u64::from(rate) * u64::from(ms) / 1000) as usize
}

/// Stille als PCM16 — für die Fuge zwischen zwei Sprech-Abschnitten.
pub fn stille(rate: u32, ms: u32) -> Vec<i16> {
    vec![0; samples_fuer(rate, ms)]
}

/// Setzt vorn und hinten Stille an (siehe [`RAND_VORN_MS`],
/// [`RAND_HINTEN_MS`]).
pub fn mit_rand(samples: Vec<f32>, rate: u32, vorn_ms: u32, hinten_ms: u32) -> Vec<f32> {
    let vorn = samples_fuer(rate, vorn_ms);
    let hinten = samples_fuer(rate, hinten_ms);
    let mut aus = Vec::with_capacity(vorn + samples.len() + hinten);
    aus.resize(vorn, 0.0);
    aus.extend(samples);
    aus.resize(aus.len() + hinten, 0.0);
    aus
}

/// Verteilt Mono auf `kanaele` verschränkte Kanäle — die Ansage soll aus
/// jedem Lautsprecher kommen, nicht nur links.
pub fn auf_kanaele(mono: &[f32], kanaele: usize) -> Vec<f32> {
    let k = kanaele.max(1);
    mono.iter()
        .flat_map(|&s| std::iter::repeat_n(s, k))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baut eine WAV-Datei von Hand — so prüft der Test den Parser gegen
    /// das Format, nicht gegen sich selbst.
    fn wav(tag: u16, kanaele: u16, rate: u32, bits: u16, daten: &[u8], extra: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend(tag.to_le_bytes());
        fmt.extend(kanaele.to_le_bytes());
        fmt.extend(rate.to_le_bytes());
        let block = kanaele * bits / 8;
        fmt.extend((rate * u32::from(block)).to_le_bytes());
        fmt.extend(block.to_le_bytes());
        fmt.extend(bits.to_le_bytes());
        let mut body = Vec::new();
        body.extend(b"WAVE");
        body.extend(b"fmt ");
        body.extend((fmt.len() as u32).to_le_bytes());
        body.extend(&fmt);
        body.extend(extra);
        body.extend(b"data");
        body.extend((daten.len() as u32).to_le_bytes());
        body.extend(daten);
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    fn le(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn pcm16_bytes_werden_little_endian_gelesen() {
        assert_eq!(
            pcm16_aus_le_bytes(&[0x01, 0x00, 0xFF, 0xFF, 0x00, 0x80]).unwrap(),
            vec![1, -1, i16::MIN]
        );
        assert_eq!(pcm16_aus_le_bytes(&[]).unwrap(), Vec::<i16>::new());
    }

    #[test]
    fn pcm16_ungerade_laenge_ist_ein_fehler() {
        assert!(matches!(
            pcm16_aus_le_bytes(&[1, 2, 3]),
            Err(TonFehler::Ungueltig(_))
        ));
    }

    #[test]
    fn wav_mono_pcm16_wird_gelesen() {
        let d = wav(1, 1, 22050, 16, &le(&[1, -2, 300]), &[]);
        assert_eq!(wav_lesen(&d).unwrap(), (vec![1, -2, 300], 22050));
    }

    #[test]
    fn wav_stereo_wird_zu_mono_gemittelt() {
        let d = wav(1, 2, 16000, 16, &le(&[100, 300, -10, -30]), &[]);
        assert_eq!(wav_lesen(&d).unwrap(), (vec![200, -20], 16000));
    }

    #[test]
    fn wav_fremde_chunks_vor_den_daten_werden_uebersprungen() {
        // LIST-Chunk mit ungerader Länge: Füllbyte muss mit übersprungen werden.
        let mut extra = Vec::new();
        extra.extend(b"LIST");
        extra.extend(3u32.to_le_bytes());
        extra.extend([7, 7, 7, 0]);
        let d = wav(1, 1, 24000, 16, &le(&[5, 6]), &extra);
        assert_eq!(wav_lesen(&d).unwrap(), (vec![5, 6], 24000));
    }

    #[test]
    fn wav_extensible_mit_pcm_subformat_wird_gelesen() {
        // fmt mit cbSize/Extensible-Teil von Hand: 40 Bytes.
        let mut fmt = Vec::new();
        fmt.extend(0xFFFEu16.to_le_bytes());
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(8000u32.to_le_bytes());
        fmt.extend(16000u32.to_le_bytes());
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(16u16.to_le_bytes());
        fmt.extend(22u16.to_le_bytes()); // cbSize
        fmt.extend(16u16.to_le_bytes()); // valid bits
        fmt.extend(4u32.to_le_bytes()); // channel mask
        fmt.extend(1u16.to_le_bytes()); // Subformat PCM …
        fmt.extend([0u8; 14]); // … Rest der GUID
        let mut body = Vec::new();
        body.extend(b"WAVE");
        body.extend(b"fmt ");
        body.extend((fmt.len() as u32).to_le_bytes());
        body.extend(&fmt);
        body.extend(b"data");
        body.extend(4u32.to_le_bytes());
        body.extend(le(&[9, -9]));
        let mut d = Vec::new();
        d.extend(b"RIFF");
        d.extend((body.len() as u32).to_le_bytes());
        d.extend(body);
        assert_eq!(wav_lesen(&d).unwrap(), (vec![9, -9], 8000));
    }

    #[test]
    fn wav_ablehnungen() {
        // Kein RIFF.
        assert!(wav_lesen(b"RIFX0000WAVE").is_err());
        assert!(wav_lesen(&[]).is_err());
        // 8 Bit.
        assert!(wav_lesen(&wav(1, 1, 8000, 8, &[1, 2], &[])).is_err());
        // Float.
        assert!(wav_lesen(&wav(3, 1, 8000, 16, &le(&[1]), &[])).is_err());
        // Drei Kanäle.
        assert!(wav_lesen(&wav(1, 3, 8000, 16, &le(&[1, 2, 3]), &[])).is_err());
        // data ohne fmt.
        let mut ohne_fmt = Vec::new();
        ohne_fmt.extend(b"RIFF");
        ohne_fmt.extend(12u32.to_le_bytes());
        ohne_fmt.extend(b"WAVEdata");
        ohne_fmt.extend(0u32.to_le_bytes());
        assert!(wav_lesen(&ohne_fmt).is_err());
    }

    #[test]
    fn wav_platzhalter_groesse_im_data_chunk_nimmt_was_da_ist() {
        let mut d = wav(1, 1, 8000, 16, &le(&[1, 2, 3]), &[]);
        let n = d.len();
        // data-Größe steht 4 Bytes vor den 6 Datenbytes.
        d[n - 10..n - 6].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(wav_lesen(&d).unwrap(), (vec![1, 2, 3], 8000));
    }

    #[test]
    fn resample_laenge_ist_gerundet_proportional() {
        let s = vec![0i16; 1000];
        assert_eq!(resample_linear(&s, 24000, 48000).len(), 2000);
        assert_eq!(resample_linear(&s, 48000, 44100).len(), 919); // 918,75
        assert_eq!(resample_linear(&s, 22050, 48000).len(), 2177); // 2176,87
        assert_eq!(resample_linear(&s, 16000, 16000).len(), 1000);
        assert!(resample_linear(&[], 16000, 48000).is_empty());
        assert!(resample_linear(&s, 0, 48000).is_empty());
    }

    #[test]
    fn resample_erhaelt_den_gleichanteil() {
        let s = vec![16384i16; 500];
        for &(von, nach) in &[(24000, 48000), (24000, 44100), (48000, 16000)] {
            let r = resample_linear(&s, von, nach);
            assert!(r.iter().all(|&v| (v - 0.5).abs() < 1e-6), "{von}->{nach}");
        }
    }

    #[test]
    fn resample_erhaelt_die_frequenz() {
        // 440 Hz, 1 s bei 24 kHz → 880 Nulldurchgänge (±1 je nach Phase).
        let von = 24000u32;
        let s: Vec<i16> = (0..von)
            .map(|i| {
                let t = f64::from(i) / f64::from(von);
                ((t * 440.0 * std::f64::consts::TAU).sin() * 20000.0) as i16
            })
            .collect();
        let zaehle = |v: &[f32]| {
            v.windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count() as i64
        };
        let ref_n = zaehle(&resample_linear(&s, von, von));
        for &nach in &[48000u32, 44100, 16000] {
            let n = zaehle(&resample_linear(&s, von, nach));
            assert!((n - ref_n).abs() <= 1, "{nach}: {n} vs {ref_n}");
        }
    }

    #[test]
    fn resample_endpunkte_bleiben_erhalten() {
        let s = [0i16, 32767 / 2, -16384];
        let r = resample_linear(&s, 1, 2);
        assert_eq!(r.len(), 6);
        assert!((r[0] - 0.0).abs() < 1e-6);
        // Zwischenwert liegt mittig.
        assert!((r[1] - (16383.0 / 2.0) / 32768.0).abs() < 1e-4);
        assert!((r[2] - 16383.0 / 32768.0).abs() < 1e-6);
        assert!((r[4] - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn mit_rand_setzt_stille_vorn_und_hinten() {
        let r = mit_rand(vec![1.0; 10], 1000, 20, 150);
        assert_eq!(r.len(), 20 + 10 + 150);
        assert!(r[..20].iter().all(|&v| v == 0.0));
        assert!(r[20..30].iter().all(|&v| v == 1.0));
        assert!(r[30..].iter().all(|&v| v == 0.0));
        assert_eq!(
            mit_rand(vec![], 48000, RAND_VORN_MS, RAND_HINTEN_MS).len(),
            960 + 7200
        );
    }

    #[test]
    fn stille_hat_die_richtige_laenge() {
        assert_eq!(stille(24000, 300).len(), 7200);
        assert!(stille(22050, 300).iter().all(|&v| v == 0));
        assert_eq!(stille(22050, 300).len(), 6615);
    }

    #[test]
    fn auf_kanaele_verteilt_mono() {
        assert_eq!(auf_kanaele(&[0.5, -0.5], 2), vec![0.5, 0.5, -0.5, -0.5]);
        assert_eq!(auf_kanaele(&[0.25], 6), vec![0.25; 6]);
        assert_eq!(auf_kanaele(&[0.25], 0), vec![0.25]);
    }

    #[test]
    fn deckel_von_sechzig_sekunden() {
        assert!(dauer_pruefen(24000 * 60, 24000, MAX_SEKUNDEN).is_ok());
        assert!(matches!(
            dauer_pruefen(24000 * 60 + 1, 24000, MAX_SEKUNDEN),
            Err(TonFehler::Ungueltig(_))
        ));
        assert!(dauer_pruefen(1, 0, MAX_SEKUNDEN).is_err());
    }
}
