// Zeichenzähler der Cloud-Stimmen und Arbeitsspeicher-Cache der Ansage-Seiten
// (Spec `ansage-elevenlabs`, Erweiterung E8/E9).
//
// Der Zähler hält je Tag und Dienst fest, wie viele Zeichen tatsächlich an
// einen Anbieter gingen — damit nach einem Turnier eine echte Zahl statt einer
// Schätzung dasteht. Er speichert **nur Zahlen**, nie Text oder Namen.
//
// Der Seiten-Cache spart Wiederholungen der Ansage-Seiten (Nachruf, zweite
// Halle, „nochmal"). Er lebt bewusst **nur im Arbeitsspeicher** und kurz:
// Audio der Seiten enthält Spielernamen und kommt nie auf die Platte (Spec
// `ansage-im-browser`, Kriterium 10).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;

/// Dateiname im App-Datenverzeichnis.
pub const VERBRAUCH_DATEI: &str = "tts-zeichen.json";
/// So viele Tage bleiben in der Datei; ältere fallen heraus.
const MAX_TAGE: usize = 90;

/// Zähler eines Dienstes an einem Tag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posten {
    /// Zeichen, die an den Anbieter gingen (nur neu vertonte Ansagen).
    pub zeichen: u64,
    /// Neu vertonte Ansagen (kosten).
    pub neu: u64,
    /// Ansagen aus einem Cache (kosten nichts).
    pub aus_cache: u64,
}

/// Alle Zähler: Tag (`JJJJ-MM-TT`) → Dienst-Wert → Posten.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verbrauch {
    #[serde(default)]
    pub tage: BTreeMap<String, BTreeMap<String, Posten>>,
}

/// Eine Zeile für die Anzeige.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerbrauchZeile {
    pub tag: String,
    pub dienst: String,
    pub zeichen: u64,
    pub neu: u64,
    pub aus_cache: u64,
}

impl Verbrauch {
    /// Bucht eine Vertonung. Aus dem Cache zählt nur als Treffer, ohne Zeichen.
    pub fn buchen(&mut self, tag: &str, dienst: &str, zeichen: u64, aus_cache: bool) {
        let p = self
            .tage
            .entry(tag.to_string())
            .or_default()
            .entry(dienst.to_string())
            .or_default();
        if aus_cache {
            p.aus_cache += 1;
        } else {
            p.neu += 1;
            p.zeichen += zeichen;
        }
        // BTreeMap sortiert JJJJ-MM-TT chronologisch → vorn ist der älteste.
        while self.tage.len() > MAX_TAGE {
            let Some(aeltester) = self.tage.keys().next().cloned() else {
                break;
            };
            self.tage.remove(&aeltester);
        }
    }

    /// Zeilen der letzten `max_tage` Tage, neuester Tag zuerst.
    pub fn zeilen(&self, max_tage: usize) -> Vec<VerbrauchZeile> {
        self.tage
            .iter()
            .rev()
            .take(max_tage)
            .flat_map(|(tag, dienste)| {
                dienste.iter().map(move |(dienst, p)| VerbrauchZeile {
                    tag: tag.clone(),
                    dienst: dienst.clone(),
                    zeichen: p.zeichen,
                    neu: p.neu,
                    aus_cache: p.aus_cache,
                })
            })
            .collect()
    }
}

/// Liest die Zähler; fehlt oder kaputt → leer (ein Zähler darf nie stören).
pub fn laden(datei: &Path) -> Verbrauch {
    std::fs::read(datei)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Serialisiert Lesen-Ändern-Schreiben, damit zwei gleichzeitige Ansagen
/// sich nicht gegenseitig einen Zähler überschreiben.
static DATEI_SPERRE: Mutex<()> = Mutex::new(());

/// Bucht in die Datei und liefert den neuen Tagesstand des Dienstes.
/// Schreibfehler sind nicht fatal (Ansage geht vor Statistik).
pub fn buchen_in_datei(
    datei: &Path,
    tag: &str,
    dienst: &str,
    zeichen: u64,
    aus_cache: bool,
) -> Posten {
    let _g = DATEI_SPERRE.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = laden(datei);
    v.buchen(tag, dienst, zeichen, aus_cache);
    if let Some(dir) = datei.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(&v) {
        let tmp = datei.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, datei);
        }
    }
    v.tage
        .get(tag)
        .and_then(|d| d.get(dienst))
        .copied()
        .unwrap_or_default()
}

/// Heutiges Datum (Ortszeit) als `JJJJ-MM-TT`.
pub fn heute() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

// ─── Arbeitsspeicher-Cache der Ansage-Seiten ──────────────────────────────

/// So lange bleibt eine Seiten-Ansage im Speicher.
pub const SEITEN_CACHE_TTL_MS: u64 = 2 * 60 * 60 * 1000;
/// Obergrenze des Speichers (Gemini liefert WAV, ~0,7 MB je Ansage).
pub const SEITEN_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;

struct Eintrag {
    schluessel: String,
    mime: String,
    bytes: Vec<u8>,
    seit_ms: u64,
}

/// Kleiner Cache im Arbeitsspeicher: ältester Eintrag fliegt zuerst.
#[derive(Default)]
pub struct SeitenCache {
    eintraege: VecDeque<Eintrag>,
    bytes: usize,
}

impl SeitenCache {
    /// Gespeichertes Audio zum Schlüssel, solange es nicht abgelaufen ist.
    pub fn holen(&mut self, schluessel: &str, jetzt_ms: u64) -> Option<(String, Vec<u8>)> {
        self.aufraeumen(jetzt_ms);
        self.eintraege
            .iter()
            .find(|e| e.schluessel == schluessel)
            .map(|e| (e.mime.clone(), e.bytes.clone()))
    }

    /// Legt Audio ab; zu Großes wird gar nicht gespeichert.
    pub fn ablegen(&mut self, schluessel: String, mime: String, bytes: Vec<u8>, jetzt_ms: u64) {
        if bytes.len() > SEITEN_CACHE_MAX_BYTES {
            return;
        }
        self.aufraeumen(jetzt_ms);
        if let Some(i) = self
            .eintraege
            .iter()
            .position(|e| e.schluessel == schluessel)
        {
            if let Some(alt) = self.eintraege.remove(i) {
                self.bytes -= alt.bytes.len();
            }
        }
        while self.bytes + bytes.len() > SEITEN_CACHE_MAX_BYTES {
            let Some(alt) = self.eintraege.pop_front() else {
                break;
            };
            self.bytes -= alt.bytes.len();
        }
        self.bytes += bytes.len();
        self.eintraege.push_back(Eintrag {
            schluessel,
            mime,
            bytes,
            seit_ms: jetzt_ms,
        });
    }

    /// Wirft Abgelaufenes weg (Einträge stehen in Ablage-Reihenfolge).
    fn aufraeumen(&mut self, jetzt_ms: u64) {
        while let Some(e) = self.eintraege.front() {
            if jetzt_ms.saturating_sub(e.seit_ms) < SEITEN_CACHE_TTL_MS {
                break;
            }
            if let Some(alt) = self.eintraege.pop_front() {
                self.bytes -= alt.bytes.len();
            }
        }
    }

    #[cfg(test)]
    fn laenge(&self) -> usize {
        self.eintraege.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buchen_zaehlt_zeichen_nur_bei_neuer_vertonung() {
        let mut v = Verbrauch::default();
        v.buchen("2026-11-03", "elevenlabs_v3", 120, false);
        v.buchen("2026-11-03", "elevenlabs_v3", 120, true);
        v.buchen("2026-11-03", "elevenlabs_v3", 80, false);
        let p = v.tage["2026-11-03"]["elevenlabs_v3"];
        assert_eq!(
            p,
            Posten {
                zeichen: 200,
                neu: 2,
                aus_cache: 1
            }
        );
    }

    #[test]
    fn dienste_und_tage_getrennt_neuester_zuerst() {
        let mut v = Verbrauch::default();
        v.buchen("2026-11-02", "azure", 10, false);
        v.buchen("2026-11-03", "openai", 20, false);
        v.buchen("2026-11-03", "azure", 30, false);
        let z = v.zeilen(14);
        let kurz: Vec<_> = z
            .iter()
            .map(|z| (z.tag.as_str(), z.dienst.as_str(), z.zeichen))
            .collect();
        assert_eq!(
            kurz,
            vec![
                ("2026-11-03", "azure", 30),
                ("2026-11-03", "openai", 20),
                ("2026-11-02", "azure", 10)
            ]
        );
        assert_eq!(v.zeilen(1).len(), 2);
    }

    #[test]
    fn alte_tage_fallen_heraus() {
        let mut v = Verbrauch::default();
        for i in 0..(MAX_TAGE + 5) {
            v.buchen(
                &format!("2026-{:02}-{:02}", 1 + i / 28, 1 + i % 28),
                "azure",
                1,
                false,
            );
        }
        assert_eq!(v.tage.len(), MAX_TAGE);
        assert!(!v.tage.contains_key("2026-01-01"));
    }

    #[test]
    fn datei_rundlauf_und_kaputte_datei() {
        let dir = std::env::temp_dir().join(format!("bts-tts-zaehler-{}", std::process::id()));
        let datei = dir.join(VERBRAUCH_DATEI);
        let _ = std::fs::remove_file(&datei);
        buchen_in_datei(&datei, "2026-11-03", "gemini_38_flash", 50, false);
        let p = buchen_in_datei(&datei, "2026-11-03", "gemini_38_flash", 50, true);
        assert_eq!(p.zeichen, 50);
        assert_eq!(p.aus_cache, 1);
        assert_eq!(laden(&datei).tage["2026-11-03"]["gemini_38_flash"].neu, 1);
        std::fs::write(&datei, b"{kaputt").unwrap();
        assert_eq!(laden(&datei), Verbrauch::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zaehler_datei_enthaelt_keinen_text() {
        // Wächter: In die Datei kommen nur Tag, Dienst und Zahlen.
        let mut v = Verbrauch::default();
        v.buchen("2026-11-03", "azure", 42, false);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(
            json,
            r#"{"tage":{"2026-11-03":{"azure":{"zeichen":42,"neu":1,"aus_cache":0}}}}"#
        );
    }

    #[test]
    fn seiten_cache_trifft_und_laeuft_ab() {
        let mut c = SeitenCache::default();
        c.ablegen("a".into(), "audio/mpeg".into(), vec![1, 2, 3], 1000);
        assert_eq!(
            c.holen("a", 2000),
            Some(("audio/mpeg".into(), vec![1, 2, 3]))
        );
        assert_eq!(c.holen("b", 2000), None);
        assert_eq!(c.holen("a", 1000 + SEITEN_CACHE_TTL_MS), None);
        assert_eq!(c.laenge(), 0);
    }

    #[test]
    fn seiten_cache_haelt_die_obergrenze() {
        let mut c = SeitenCache::default();
        let gross = SEITEN_CACHE_MAX_BYTES / 2 + 1;
        c.ablegen("a".into(), "m".into(), vec![0; gross], 0);
        c.ablegen("b".into(), "m".into(), vec![0; gross], 1);
        assert_eq!(c.holen("a", 2), None, "ältester muss weichen");
        assert!(c.holen("b", 2).is_some());
        assert!(c.bytes <= SEITEN_CACHE_MAX_BYTES);
        c.ablegen(
            "riesig".into(),
            "m".into(),
            vec![0; SEITEN_CACHE_MAX_BYTES + 1],
            3,
        );
        assert_eq!(c.holen("riesig", 3), None);
        // Gleicher Schlüssel ersetzt statt doppelt zu zählen.
        c.ablegen("b".into(), "m".into(), vec![0; 10], 4);
        assert_eq!(c.bytes, 10);
    }
}
