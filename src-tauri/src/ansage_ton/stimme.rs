//! Reine Stimmenlogik der Windows-Stimme im Kern (ADR 0066): welche
//! installierte Stimme zu einer gespeicherten Wahl passt, welche Rate
//! WinRT bekommt und wie die Sprech-Abschnitte zu einem Stück werden.
//!
//! Plattformfrei und getestet; `windows/winrt_stimme.rs` liefert nur die
//! Liste und das Audio.

use super::pcm;
use serde::{Deserialize, Serialize};

/// Pause zwischen zwei Sprech-Abschnitten — dieselbe Atempause, die die
/// Browser-Stimme zwischen ihren Äußerungen macht.
pub const PAUSE_ZWISCHEN_SEGMENTEN_MS: u32 = 300;

/// Eine installierte Windows-Stimme, wie sie die Einstellungen zeigen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StimmeInfo {
    /// Anzeigename, z. B. „Microsoft Katja".
    pub name: String,
    /// BCP-47-Sprache, z. B. „de-DE".
    pub sprache: String,
}

/// Spricht `stimme` die Sprache `lang` („de"/„en")? Verglichen wird der
/// Sprachteil vor dem Bindestrich, ohne Groß-/Kleinschreibung.
fn spricht(stimme: &StimmeInfo, lang: &str) -> bool {
    let haupt = stimme.sprache.split(['-', '_']).next().unwrap_or("");
    haupt.eq_ignore_ascii_case(lang)
}

/// Bevorzugte Region je Sprache, wenn keine bestimmte Stimme passt.
fn hauptregion(lang: &str) -> &'static str {
    if lang.eq_ignore_ascii_case("en") {
        "en-US"
    } else {
        "de-DE"
    }
}

/// Ordnet die gespeicherte Stimmen-Wahl einer installierten Stimme zu.
///
/// Gespeichert ist heute meist eine Chromium-`voiceURI`
/// („Microsoft Katja - German (Germany)"), WinRT nennt dieselbe Stimme
/// „Microsoft Katja". Reihenfolge: exakter Name → Teil vor „ - " → erste
/// Stimme der Hauptregion (de-DE/en-US) → erste Stimme der Sprache.
///
/// **Nie eine Stimme einer anderen Sprache** (Spec D9): Eine englische
/// Stimme liest deutsche Namen und Hallen unverständlich vor. Auch eine
/// gespeicherte Wahl zählt nur, wenn sie die Sprache spricht. `None` heißt:
/// keine Stimme für diese Sprache installiert.
pub fn stimme_waehlen(gespeichert: &str, lang: &str, stimmen: &[StimmeInfo]) -> Option<usize> {
    let passend = |s: &StimmeInfo| spricht(s, lang);
    let gespeichert = gespeichert.trim();
    if !gespeichert.is_empty() {
        if let Some(i) = stimmen
            .iter()
            .position(|s| passend(s) && s.name == gespeichert)
        {
            return Some(i);
        }
        if let Some((vorn, _)) = gespeichert.split_once(" - ") {
            let vorn = vorn.trim();
            if let Some(i) = stimmen.iter().position(|s| passend(s) && s.name == vorn) {
                return Some(i);
            }
        }
    }
    let region = hauptregion(lang);
    stimmen
        .iter()
        .position(|s| s.sprache.eq_ignore_ascii_case(region))
        .or_else(|| stimmen.iter().position(passend))
}

/// Sprech-Geschwindigkeit für WinRT. Dieselben Grenzen wie `clampRate` im
/// Frontend und `ansage_entwurf_aus`; Unsinn (NaN, ∞) wird zum
/// Standardwert 0,8 statt eine Ausnahme in WinRT auszulösen.
pub fn winrt_rate(rate: f64) -> f64 {
    if rate.is_finite() {
        rate.clamp(0.5, 1.5)
    } else {
        0.8
    }
}

/// Fügt die Sprech-Abschnitte mit [`PAUSE_ZWISCHEN_SEGMENTEN_MS`] Stille zu
/// **einem** Stück zusammen — der Kern spielt die Ansage als Ganzes, damit
/// kein Gerätewechsel zwischen die Abschnitte fällt.
///
/// Alle Teile laufen auf die Rate des ersten (WinRT liefert in der Praxis
/// überall dieselbe; weicht eine ab, wird sie angeglichen statt die Ansage
/// zu verzerren). `None` bei leerer Liste.
pub fn fuege_segmente(teile: Vec<(Vec<i16>, u32)>, pause_ms: u32) -> Option<(Vec<i16>, u32)> {
    let rate = teile.first()?.1;
    let pause = pcm::stille(rate, pause_ms);
    let mut aus: Vec<i16> = Vec::new();
    for (i, (samples, r)) in teile.into_iter().enumerate() {
        if i > 0 {
            aus.extend_from_slice(&pause);
        }
        if r == rate {
            aus.extend(samples);
        } else {
            aus.extend(
                pcm::resample_linear(&samples, r, rate)
                    .into_iter()
                    .map(|v| (v * 32768.0).round().clamp(-32768.0, 32767.0) as i16),
            );
        }
    }
    Some((aus, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str, sprache: &str) -> StimmeInfo {
        StimmeInfo {
            name: name.to_string(),
            sprache: sprache.to_string(),
        }
    }

    /// Die am 31.10.2026 gemessene Liste (OneCore), absichtlich mit einer
    /// englischen Stimme vorn.
    fn liste() -> Vec<StimmeInfo> {
        vec![
            s("Microsoft David", "en-US"),
            s("Microsoft Hedda", "de-AT"),
            s("Microsoft Katja", "de-DE"),
            s("Microsoft Stefan", "de-DE"),
            s("Microsoft George", "en-GB"),
            s("Microsoft Zira", "en-US"),
        ]
    }

    #[test]
    fn exakter_name_gewinnt() {
        assert_eq!(stimme_waehlen("Microsoft Stefan", "de", &liste()), Some(3));
        assert_eq!(stimme_waehlen("Microsoft George", "en", &liste()), Some(4));
    }

    #[test]
    fn chromium_voice_uri_wird_auf_den_winrt_namen_abgebildet() {
        assert_eq!(
            stimme_waehlen("Microsoft Stefan - German (Germany)", "de", &liste()),
            Some(3)
        );
        assert_eq!(
            stimme_waehlen("Microsoft Hedda - German (Austria)", "de", &liste()),
            Some(1)
        );
    }

    #[test]
    fn unbekannte_oder_leere_wahl_nimmt_die_hauptregion() {
        // de-DE vor de-AT, obwohl Hedda weiter vorn steht.
        assert_eq!(stimme_waehlen("", "de", &liste()), Some(2));
        assert_eq!(
            stimme_waehlen(
                "Microsoft Katja Online (Natural) - German (Germany)",
                "de",
                &liste()
            ),
            Some(2)
        );
        assert_eq!(stimme_waehlen("", "en", &liste()), Some(0));
    }

    #[test]
    fn ohne_hauptregion_die_erste_stimme_der_sprache() {
        let l = vec![s("Microsoft David", "en-US"), s("Microsoft Hedda", "de-AT")];
        assert_eq!(stimme_waehlen("", "de", &l), Some(1));
        let l = vec![s("Microsoft Hazel", "en-GB")];
        assert_eq!(stimme_waehlen("", "en", &l), Some(0));
    }

    #[test]
    fn nie_eine_stimme_einer_anderen_sprache() {
        let nur_englisch = vec![s("Microsoft David", "en-US"), s("Microsoft Zira", "en-US")];
        assert_eq!(stimme_waehlen("", "de", &nur_englisch), None);
        // Auch nicht, wenn die gespeicherte Wahl eine englische Stimme nennt.
        assert_eq!(stimme_waehlen("Microsoft David", "de", &nur_englisch), None);
        assert_eq!(stimme_waehlen("Microsoft David", "de", &liste()), Some(2));
        assert_eq!(stimme_waehlen("", "de", &[]), None);
        // „deu"/„dex" sind nicht „de".
        assert_eq!(stimme_waehlen("", "de", &[s("X", "dex-XX")]), None);
    }

    #[test]
    fn sprachvergleich_ohne_gross_klein() {
        let l = vec![s("A", "DE-de"), s("B", "en_us")];
        assert_eq!(stimme_waehlen("", "de", &l), Some(0));
        assert_eq!(stimme_waehlen("", "en", &l), Some(1));
    }

    #[test]
    fn rate_grenzen() {
        assert_eq!(winrt_rate(0.8), 0.8);
        assert_eq!(winrt_rate(0.1), 0.5);
        assert_eq!(winrt_rate(9.0), 1.5);
        assert_eq!(winrt_rate(f64::NAN), 0.8);
        assert_eq!(winrt_rate(f64::INFINITY), 0.8);
        assert_eq!(winrt_rate(f64::NEG_INFINITY), 0.8);
    }

    #[test]
    fn segment_fuge_mit_pause() {
        let (pcm, rate) = fuege_segmente(
            vec![(vec![1, 2], 1000), (vec![3], 1000), (vec![4], 1000)],
            PAUSE_ZWISCHEN_SEGMENTEN_MS,
        )
        .unwrap();
        assert_eq!(rate, 1000);
        assert_eq!(pcm.len(), 2 + 300 + 1 + 300 + 1);
        assert_eq!(&pcm[..2], &[1, 2]);
        assert!(pcm[2..302].iter().all(|&v| v == 0));
        assert_eq!(pcm[302], 3);
        assert_eq!(pcm[603], 4);
    }

    #[test]
    fn segment_fuge_gleicht_die_rate_an_und_leere_liste_ist_none() {
        let (pcm, rate) =
            fuege_segmente(vec![(vec![100; 10], 2000), (vec![100; 10], 1000)], 0).unwrap();
        assert_eq!(rate, 2000);
        assert_eq!(pcm.len(), 10 + 20);
        assert!(pcm.iter().all(|&v| v == 100));
        assert!(fuege_segmente(vec![], 300).is_none());
        let (einzeln, _) = fuege_segmente(vec![(vec![7], 8000)], 300).unwrap();
        assert_eq!(einzeln, vec![7]);
    }
}
