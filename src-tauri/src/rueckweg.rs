//! Kern-Rückweg: Welche Vorversion liegt neben dem Programm, darf zu ihr
//! zurückgewechselt werden, und welche Versionen wurden zurückgenommen?
//! (Spec hotfix-ohne-neustart, E5; ADR 0101.)
//!
//! Genau **eine** Vorversion bleibt liegen (`bts-light.vorher.exe`). Was sie
//! ist, steht in `rueckweg.json` im App-Datenverzeichnis — die Datei selbst
//! sagt es nicht. Passt der Eintrag nicht zur laufenden Version (ein
//! Installer-Lauf von außen dazwischen), gilt die Vorversion als veraltet
//! und wird beim Start verworfen.
//!
//! Dazu das Aufräumen beim Start: Reste eines früheren Wechsels und alte
//! Installer aus dem Temp-Verzeichnis.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tausch::Pfade;
use crate::uebergabe::Version;

/// Dateiname im App-Datenverzeichnis.
pub const DATEI: &str = "rueckweg.json";

/// So viele zurückgenommene Versionen merkt sich die Sperrliste höchstens;
/// die ältesten fallen heraus.
pub const MAX_GESPERRT: usize = 8;

/// Inhalt von `rueckweg.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rueckweg {
    /// Version, die als `bts-light.exe` laufen soll, damit der Eintrag gilt.
    #[serde(default)]
    pub aktiv: Option<String>,
    /// Version der Datei `bts-light.vorher.exe`.
    #[serde(default)]
    pub vorher: Option<String>,
    /// SHA-256 (hex) der Vorversion — vor dem Rückweg wird nachgerechnet.
    #[serde(default)]
    pub vorher_sha256: Option<String>,
    /// Zurückgenommene Versionen: werden nicht erneut angeboten.
    #[serde(default)]
    pub gesperrt: Vec<String>,
}

impl Rueckweg {
    /// Die Vorversion — nur wenn der Eintrag zur laufenden Version gehört.
    pub fn vorversion(&self, eigene: &str) -> Option<&str> {
        match (&self.aktiv, &self.vorher) {
            (Some(aktiv), Some(vorher)) if aktiv == eigene => Some(vorher),
            _ => None,
        }
    }

    /// Merkt: `aktiv` läuft (bald), daneben liegt `vorher` mit diesem Hash.
    pub fn merke(&mut self, aktiv: &str, vorher: &str, sha256: &str) {
        self.aktiv = Some(aktiv.to_string());
        self.vorher = Some(vorher.to_string());
        self.vorher_sha256 = Some(sha256.to_string());
    }

    /// Vergisst die Vorversion; die Sperrliste bleibt.
    pub fn ohne_vorher(&mut self) {
        self.aktiv = None;
        self.vorher = None;
        self.vorher_sha256 = None;
    }

    pub fn sperre(&mut self, version: &str) {
        self.gesperrt.retain(|v| v != version);
        self.gesperrt.push(version.to_string());
        let zu_viel = self.gesperrt.len().saturating_sub(MAX_GESPERRT);
        self.gesperrt.drain(..zu_viel);
    }

    pub fn ist_gesperrt(&self, version: &str) -> bool {
        self.gesperrt.iter().any(|v| v == version)
    }
}

/// Liest `rueckweg.json`. Fehlt sie oder ist sie unlesbar, gibt es keinen
/// Rückweg und keine Sperre — nie ein Fehler, der den Start aufhält.
pub fn lesen(daten: &Path) -> Rueckweg {
    let mut rw: Rueckweg = std::fs::read(daten.join(DATEI))
        .ok()
        .and_then(|roh| serde_json::from_slice(&roh).ok())
        .unwrap_or_default();
    // Auch eine von Hand veränderte Datei sperrt nie mehr als die jüngsten
    // Einträge.
    let zu_viel = rw.gesperrt.len().saturating_sub(MAX_GESPERRT);
    rw.gesperrt.drain(..zu_viel);
    rw
}

/// Schreibt über eine Nebendatei, damit nie eine halbe liegt.
pub fn schreiben(daten: &Path, rueckweg: &Rueckweg) -> std::io::Result<()> {
    std::fs::create_dir_all(daten)?;
    let neben = daten.join(format!("{DATEI}.neu"));
    std::fs::write(
        &neben,
        serde_json::to_vec_pretty(rueckweg).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(&neben, daten.join(DATEI))
}

/// Hält die Sperrliste beim Start und nach einer Ankunft aktuell:
///
/// - Kam diese Version über einen Rückweg an, wird die `zurueckgenommene`
///   gesperrt — erst jetzt, wo der Rückweg bewiesen ist.
/// - Die **laufende** Version steht nie auf der Liste. Wer eine
///   zurückgenommene Version später doch von Hand installiert, hat sie
///   wieder — und sie soll danach als Vorversion taugen.
///
/// Schreibt nur, wenn sich etwas ändert; gibt den gültigen Stand zurück.
pub fn sperren_pflegen(daten: &Path, eigene: &str, zurueckgenommene: Option<&str>) -> Rueckweg {
    let alt = lesen(daten);
    let mut rw = alt.clone();
    if let Some(version) = zurueckgenommene {
        if !rw.ist_gesperrt(version) {
            rw.sperre(version);
        }
    }
    rw.gesperrt.retain(|v| v != eigene);
    if rw != alt {
        if let Err(e) = schreiben(daten, &rw) {
            tracing::warn!("Sperrliste nicht geschrieben ({e})");
        }
    }
    rw
}

/// Die Grenze dieser Version: die älteste, die ihre Daten noch lesen kann.
/// Dieselbe Quelle wie der Kern-Kopf im Release (`release/datenformat.json`),
/// zur Bauzeit eingebettet. Unlesbar heißt „nicht lesbar" (ADR 0101).
pub fn eigene_grenze() -> Option<Version> {
    grenze_aus(include_str!("../../release/datenformat.json"))
}

fn grenze_aus(json: &str) -> Option<Version> {
    let wert: serde_json::Value = serde_json::from_str(json).ok()?;
    Version::lesen(wert.get("rueckwaerts_lesbar_ab")?.as_str()?)
}

/// Was die Oberfläche zum Rückweg zeigt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Urteil {
    /// Keine brauchbare Vorversion: kein Knopf.
    Keiner,
    /// Zurück zu `version` ist möglich.
    Erlaubt { version: String },
    /// Die Vorversion liegt da, kann die heutigen Daten aber nicht lesen:
    /// Knopf sichtbar, aber aus.
    NichtLesbar { version: String },
}

/// Entscheidet rein aus Eintrag, laufender Version, Grenze und der Frage, ob
/// die Datei da ist.
pub fn urteil(rw: &Rueckweg, eigene: &str, grenze: Option<Version>, vorher_da: bool) -> Urteil {
    let Some(vorher) = rw.vorversion(eigene) else {
        return Urteil::Keiner;
    };
    // Eine zurückgenommene Version liegt nach dem Rückweg als „vorher" da —
    // zu ihr führt kein Knopf.
    if !vorher_da || rw.ist_gesperrt(vorher) {
        return Urteil::Keiner;
    }
    // Nur zurück, nie „zurück" auf etwas Neueres oder Gleiches.
    match (Version::lesen(vorher), Version::lesen(eigene)) {
        (Some(v), Some(e)) if v < e => {}
        _ => return Urteil::Keiner,
    }
    let version = vorher.to_string();
    if crate::uebergabe::rueckweg_erlaubt(grenze, vorher) {
        Urteil::Erlaubt { version }
    } else {
        Urteil::NichtLesbar { version }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Ist `vorher` noch die Datei, die der Eintrag beschreibt?
pub fn vorher_passt(pfade: &Pfade, rw: &Rueckweg) -> Result<(), String> {
    let erwartet = rw
        .vorher_sha256
        .as_deref()
        .ok_or("zur vorigen Programmdatei fehlt die Prüfsumme")?;
    let bytes = std::fs::read(&pfade.vorher)
        .map_err(|e| format!("die vorige Programmdatei ließ sich nicht lesen ({e})"))?;
    if sha256_hex(&bytes) != erwartet {
        return Err("die vorige Programmdatei ist nicht mehr die gesicherte".to_string());
    }
    Ok(())
}

/// Vor einem Installer-Lauf: die laufende Programmdatei als Vorversion
/// daneben legen und eintragen (Spec E8) — sonst gäbe es nach „Jetzt neu
/// starten" und „Beim Beenden einbauen" keinen Rückweg.
///
/// Kopiert über eine Nebendatei; eine ältere Vorversion weicht.
pub fn vor_installer_sichern(
    pfade: &Pfade,
    daten: &Path,
    eigene: &str,
    nach: &str,
) -> Result<(), String> {
    let bytes = std::fs::read(&pfade.aktiv)
        .map_err(|e| format!("die laufende Programmdatei ließ sich nicht lesen ({e})"))?;
    let mut neben = pfade.vorher.clone().into_os_string();
    neben.push(".teil");
    let neben = PathBuf::from(neben);
    // Neu anlegen statt öffnen: Ein Rest (oder Verweis) unter dem Namen wird
    // entfernt, nie beschrieben.
    let _ = std::fs::remove_file(&neben);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&neben)
        .and_then(|mut f| std::io::Write::write_all(&mut f, &bytes))
        .and_then(|()| std::fs::rename(&neben, &pfade.vorher))
        .map_err(|e| {
            let _ = std::fs::remove_file(&neben);
            format!("die Vorversion ließ sich nicht ablegen ({e})")
        })?;
    let mut rw = lesen(daten);
    rw.merke(nach, eigene, &sha256_hex(&bytes));
    schreiben(daten, &rw).map_err(|e| format!("rueckweg.json nicht geschrieben ({e})"))
}

/// Präfix der Installer, die „Beim Beenden einbauen" im Temp-Verzeichnis
/// ablegt (siehe `update::installer_dateiname`).
const INSTALLER_PRAEFIX: &str = "bts-light-update-";
const INSTALLER_SUFFIX: &str = "-setup.exe";

/// Aufräumen beim Start. Fehler halten nichts auf — sie kommen als Zeilen
/// fürs Log zurück, ebenso was entfernt wurde.
///
/// - `neu`: Rest eines Vorladens (wird bei Bedarf neu geladen und geprüft).
/// - `defekt` samt Varianten mit Zeitstempel: eine Version, die nicht anlief.
/// - `vorher`: nur behalten, wenn `rueckweg.json` zur laufenden Version
///   passt.
/// - alte Installer im Temp-Verzeichnis.
pub fn aufraeumen(pfade: &Pfade, daten: &Path, eigene: &str, temp: &Path) -> Vec<String> {
    let mut zeilen = Vec::new();
    fn weg(zeilen: &mut Vec<String>, pfad: &Path, was: &str) {
        match std::fs::remove_file(pfad) {
            Ok(()) => zeilen.push(format!("{was} entfernt ({})", name(pfad))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => zeilen.push(format!("{was} nicht entfernt ({}: {e})", name(pfad))),
        }
    }

    weg(&mut zeilen, &pfade.neu, "Rest eines Vorladens");
    weg(
        &mut zeilen,
        &crate::tausch::tausch_ablage(pfade),
        "Rest eines Rückwegs",
    );

    // `x.defekt.exe` und `x.defekt.exe.<ms>` (wenn sich ein früherer Rest
    // beim Rückfall nicht löschen ließ).
    if let (Some(dir), Some(defekt)) = (pfade.defekt.parent(), pfade.defekt.file_name()) {
        let defekt = defekt.to_string_lossy().into_owned();
        for eintrag in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if eintrag
                .file_name()
                .to_string_lossy()
                .starts_with(defekt.as_str())
            {
                weg(
                    &mut zeilen,
                    &eintrag.path(),
                    "Nicht angelaufene Programmdatei",
                );
            }
        }
    }

    let mut rw = lesen(daten);
    if rw.vorversion(eigene).is_none() {
        weg(&mut zeilen, &pfade.vorher, "Veraltete Vorversion");
        if rw.aktiv.is_some() || rw.vorher.is_some() {
            rw.ohne_vorher();
            if let Err(e) = schreiben(daten, &rw) {
                zeilen.push(format!("rueckweg.json nicht bereinigt ({e})"));
            }
        }
    }

    for eintrag in std::fs::read_dir(temp).into_iter().flatten().flatten() {
        let datei = eintrag.file_name().to_string_lossy().into_owned();
        if datei.starts_with(INSTALLER_PRAEFIX) && datei.ends_with(INSTALLER_SUFFIX) {
            weg(&mut zeilen, &eintrag.path(), "Alter Installer");
        }
    }
    zeilen
}

fn name(pfad: &Path) -> String {
    pfad.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pfade_in(dir: &Path) -> Pfade {
        Pfade::neben(&dir.join("bts-light.exe")).unwrap()
    }

    fn v(text: &str) -> Option<Version> {
        Version::lesen(text)
    }

    fn eintrag(aktiv: &str, vorher: &str) -> Rueckweg {
        let mut rw = Rueckweg::default();
        rw.merke(aktiv, vorher, "00");
        rw
    }

    #[test]
    fn rueckweg_nur_wenn_vorher_version_lesbar() {
        let rw = eintrag("1.2.4", "1.2.3");
        // Vorversion liegt auf oder über der Grenze: erlaubt.
        for grenze in ["1.2.3", "1.0.0"] {
            assert_eq!(
                urteil(&rw, "1.2.4", v(grenze), true),
                Urteil::Erlaubt {
                    version: "1.2.3".into()
                },
                "Grenze {grenze}"
            );
        }
    }

    /// Kann die Vorversion die heutigen Daten nicht lesen — oder fehlt die
    /// Angabe —, ist der Knopf da, aber aus.
    #[test]
    fn rueckweg_knopf_aus_mit_grund() {
        let rw = eintrag("1.2.4", "1.2.3");
        for grenze in [v("1.2.4"), None] {
            assert_eq!(
                urteil(&rw, "1.2.4", grenze, true),
                Urteil::NichtLesbar {
                    version: "1.2.3".into()
                }
            );
        }
    }

    #[test]
    fn kein_knopf_ohne_passende_vorversion() {
        let grenze = v("1.0.0");
        // Kein Eintrag.
        assert_eq!(
            urteil(&Rueckweg::default(), "1.2.4", grenze, true),
            Urteil::Keiner
        );
        // Eintrag gehört zu einer anderen laufenden Version (Installer von
        // außen dazwischen).
        assert_eq!(
            urteil(&eintrag("1.2.4", "1.2.3"), "1.2.5", grenze, true),
            Urteil::Keiner
        );
        // Datei fehlt.
        assert_eq!(
            urteil(&eintrag("1.2.4", "1.2.3"), "1.2.4", grenze, false),
            Urteil::Keiner
        );
        // „Vorher" ist neuer als die laufende Version (nach einem Rückweg
        // liegt die zurückgenommene dort): kein Weg „zurück" nach vorn.
        assert_eq!(
            urteil(&eintrag("1.2.3", "1.2.4"), "1.2.3", grenze, true),
            Urteil::Keiner
        );
        // Zurückgenommen.
        let mut rw = eintrag("1.2.4", "1.2.3");
        rw.sperre("1.2.3");
        assert_eq!(urteil(&rw, "1.2.4", grenze, true), Urteil::Keiner);
    }

    #[test]
    fn sperrliste_ohne_doppelte_und_begrenzt() {
        let mut rw = Rueckweg::default();
        rw.sperre("1.0.0");
        rw.sperre("1.0.0");
        assert_eq!(rw.gesperrt, vec!["1.0.0"]);
        for n in 1..=MAX_GESPERRT {
            rw.sperre(&format!("2.0.{n}"));
        }
        assert_eq!(rw.gesperrt.len(), MAX_GESPERRT);
        assert!(!rw.ist_gesperrt("1.0.0"), "die älteste fällt heraus");
        assert!(rw.ist_gesperrt(&format!("2.0.{MAX_GESPERRT}")));
    }

    /// Erst die Ankunft der Vorversion sperrt die zurückgenommene. Läuft die
    /// Vorversion nicht an, läuft die zurückgenommene wieder — und darf dann
    /// nicht gesperrt sein, sonst gäbe es zu ihr nie wieder einen Rückweg.
    #[test]
    fn gesperrt_wird_erst_bei_der_ankunft_und_nie_die_laufende() {
        let dir = tempfile::tempdir().unwrap();
        // 1.2.4 → Rückweg auf 1.2.3 begonnen: Der Eintrag steht, die Sperre
        // noch nicht.
        schreiben(dir.path(), &eintrag("1.2.3", "1.2.4")).unwrap();

        // Fall A: 1.2.3 lief nicht an, 1.2.4 läuft wieder.
        let rw = sperren_pflegen(dir.path(), "1.2.4", None);
        assert!(rw.gesperrt.is_empty());

        // Fall B: 1.2.3 ist angekommen.
        let rw = sperren_pflegen(dir.path(), "1.2.3", Some("1.2.4"));
        assert!(rw.ist_gesperrt("1.2.4"));
        assert_eq!(lesen(dir.path()), rw, "steht auch in der Datei");
        // Bei jedem weiteren Start dasselbe, ohne Doppelte.
        let rw = sperren_pflegen(dir.path(), "1.2.3", Some("1.2.4"));
        assert_eq!(rw.gesperrt, vec!["1.2.4"]);
        // Die Vorversion (jetzt „vorher" = 1.2.4) führt zu keinem Knopf.
        assert_eq!(urteil(&rw, "1.2.3", v("1.0.0"), true), Urteil::Keiner);

        // Später wird 1.2.4 doch von Hand installiert: Die laufende Version
        // steht nie auf der Liste.
        let rw = sperren_pflegen(dir.path(), "1.2.4", None);
        assert!(!rw.ist_gesperrt("1.2.4"));
        assert!(!lesen(dir.path()).ist_gesperrt("1.2.4"));
    }

    #[test]
    fn vorversion_ohne_pruefsumme_gilt_nicht() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.vorher, "v").unwrap();
        let mut rw = eintrag("1.2.4", "1.2.3");
        rw.vorher_sha256 = None;
        assert!(vorher_passt(&p, &rw).is_err());
        rw.vorher_sha256 = Some(sha256_hex(b"v"));
        vorher_passt(&p, &rw).unwrap();
        std::fs::remove_file(&p.vorher).unwrap();
        assert!(vorher_passt(&p, &rw).is_err());
    }

    #[test]
    fn datei_roundtrip_und_unlesbares_ist_leer() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(lesen(dir.path()), Rueckweg::default());
        let mut rw = eintrag("1.2.4", "1.2.3");
        rw.sperre("1.2.5");
        schreiben(dir.path(), &rw).unwrap();
        assert_eq!(lesen(dir.path()), rw);
        std::fs::write(dir.path().join(DATEI), b"{ halb").unwrap();
        assert_eq!(lesen(dir.path()), Rueckweg::default());
        // Eine überlange Sperrliste wird beim Lesen gekürzt.
        let viele: Vec<String> = (0..MAX_GESPERRT + 5).map(|n| format!("3.0.{n}")).collect();
        std::fs::write(
            dir.path().join(DATEI),
            serde_json::to_vec(&serde_json::json!({ "gesperrt": viele })).unwrap(),
        )
        .unwrap();
        let gekuerzt = lesen(dir.path());
        assert_eq!(gekuerzt.gesperrt.len(), MAX_GESPERRT);
        assert!(gekuerzt.ist_gesperrt(&format!("3.0.{}", MAX_GESPERRT + 4)));
        // Eine Datei mit unbekannten und fehlenden Feldern bleibt lesbar.
        std::fs::write(
            dir.path().join(DATEI),
            br#"{"gesperrt":["1.0.0"],"spaeter":1}"#,
        )
        .unwrap();
        assert!(lesen(dir.path()).ist_gesperrt("1.0.0"));
    }

    /// Die eingebettete Grenze ist lesbar und nicht neuer als diese Version
    /// — sonst erlaubte kein Release je einen Rückweg.
    #[test]
    fn eigene_grenze_ist_lesbar() {
        let grenze = eigene_grenze().expect("release/datenformat.json nennt eine Grenze");
        assert!(grenze <= v(env!("CARGO_PKG_VERSION")).unwrap());
        assert_eq!(grenze_aus("{}"), None);
        assert_eq!(grenze_aus(r#"{"rueckwaerts_lesbar_ab":"x"}"#), None);
        assert_eq!(grenze_aus("kaputt"), None);
    }

    #[test]
    fn vor_dem_installer_wird_die_laufende_datei_gesichert() {
        let dir = tempfile::tempdir().unwrap();
        let daten = dir.path().join("daten");
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "laufend").unwrap();
        std::fs::write(&p.vorher, "noch älter").unwrap();
        let mut alt = Rueckweg::default();
        alt.sperre("1.2.2");
        schreiben(&daten, &alt).unwrap();

        vor_installer_sichern(&p, &daten, "1.2.3", "1.2.4").unwrap();

        assert_eq!(std::fs::read_to_string(&p.vorher).unwrap(), "laufend");
        assert_eq!(std::fs::read_to_string(&p.aktiv).unwrap(), "laufend");
        let rw = lesen(&daten);
        assert_eq!(rw.vorversion("1.2.4"), Some("1.2.3"));
        assert!(rw.ist_gesperrt("1.2.2"), "die Sperrliste bleibt");
        vorher_passt(&p, &rw).unwrap();
        // Scheitert der Installer, läuft weiter 1.2.3 — der Eintrag gilt
        // dann nicht.
        assert_eq!(rw.vorversion("1.2.3"), None);

        // Wird die Datei danach ausgetauscht, fällt es vor dem Rückweg auf.
        std::fs::write(&p.vorher, "fremd").unwrap();
        assert!(vorher_passt(&p, &rw).is_err());
    }

    #[test]
    fn sichern_ohne_laufende_datei_aendert_nichts() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.vorher, "alt").unwrap();
        assert!(vor_installer_sichern(&p, dir.path(), "1.2.3", "1.2.4").is_err());
        assert_eq!(std::fs::read_to_string(&p.vorher).unwrap(), "alt");
        assert_eq!(lesen(dir.path()), Rueckweg::default());
    }

    #[test]
    fn veraltete_vorher_exe_wird_verworfen() {
        let dir = tempfile::tempdir().unwrap();
        let daten = dir.path().join("daten");
        let temp = dir.path().join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        let p = pfade_in(dir.path());
        std::fs::write(&p.aktiv, "a").unwrap();
        std::fs::write(&p.vorher, "v").unwrap();
        // Der Eintrag gehört zu 1.2.4 — es läuft aber 1.2.5 (Installer von
        // außen).
        let mut rw = eintrag("1.2.4", "1.2.3");
        rw.sperre("1.2.2");
        schreiben(&daten, &rw).unwrap();

        let zeilen = aufraeumen(&p, &daten, "1.2.5", &temp);
        assert!(!p.vorher.exists());
        assert!(p.aktiv.exists());
        assert!(zeilen.iter().any(|z| z.contains("Veraltete Vorversion")));
        let danach = lesen(&daten);
        assert_eq!(danach.vorher, None);
        assert!(danach.ist_gesperrt("1.2.2"), "die Sperrliste bleibt");

        // Ganz ohne Eintrag genauso.
        std::fs::write(&p.vorher, "v").unwrap();
        std::fs::remove_file(daten.join(DATEI)).unwrap();
        aufraeumen(&p, &daten, "1.2.5", &temp);
        assert!(!p.vorher.exists());
    }

    #[test]
    fn aufraeumen_laesst_aktiv_und_vorher() {
        let dir = tempfile::tempdir().unwrap();
        let daten = dir.path().join("daten");
        let temp = dir.path().join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        let p = pfade_in(dir.path());
        for (pfad, inhalt) in [
            (&p.aktiv, "a"),
            (&p.vorher, "v"),
            (&p.neu, "n"),
            (&p.defekt, "d"),
            (&crate::tausch::tausch_ablage(&p), "t"),
        ] {
            std::fs::write(pfad, inhalt).unwrap();
        }
        let variante = dir.path().join("bts-light.defekt.exe.1794855707920");
        std::fs::write(&variante, "d2").unwrap();
        let fremd = dir.path().join("anderes.exe");
        std::fs::write(&fremd, "x").unwrap();
        schreiben(&daten, &eintrag("1.2.4", "1.2.3")).unwrap();

        aufraeumen(&p, &daten, "1.2.4", &temp);

        assert!(
            p.aktiv.exists() && p.vorher.exists(),
            "aktiv und vorher bleiben"
        );
        assert!(!p.neu.exists() && !p.defekt.exists() && !variante.exists());
        assert!(!crate::tausch::tausch_ablage(&p).exists());
        assert!(fremd.exists(), "Fremdes wird nicht angefasst");
        assert_eq!(lesen(&daten).vorversion("1.2.4"), Some("1.2.3"));
        // Ein zweiter Lauf findet nichts mehr und meldet nichts.
        assert!(aufraeumen(&p, &daten, "1.2.4", &temp).is_empty());
    }

    #[test]
    fn temp_installer_werden_entfernt() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        let p = pfade_in(dir.path());
        let alt = temp.join("bts-light-update-1.2.3-setup.exe");
        let fremd = temp.join("anderes-setup.exe");
        let aehnlich = temp.join("bts-light-update-notizen.txt");
        for pfad in [&alt, &fremd, &aehnlich] {
            std::fs::write(pfad, "x").unwrap();
        }
        let zeilen = aufraeumen(&p, dir.path(), "1.2.4", &temp);
        assert!(!alt.exists());
        assert!(fremd.exists() && aehnlich.exists());
        assert_eq!(zeilen.len(), 1, "{zeilen:?}");
        // Ein fehlendes Temp-Verzeichnis ist kein Fehler.
        assert!(aufraeumen(&p, dir.path(), "1.2.4", &dir.path().join("fehlt")).is_empty());
    }
}
