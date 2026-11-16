//! Kernwechsel ohne Installer: der signierte **Kern-Kopf** und die Prüfung
//! der Programmdatei dagegen (Spec `hotfix-ohne-neustart`, ADR 0100/0101).
//!
//! Signiert wird nicht die Programmdatei, sondern ein kleiner Kopf, der sie
//! per SHA-256 und Größe benennt. So gibt es nie eine Signatur über eine
//! nackte exe, die sich einer Altversion über ein gefälschtes `latest.json`
//! als „Installer" unterschieben ließe — und der Kopf sagt mit `art`, was er
//! ist: Ein Kern-Kopf wird nie als Seiten-Paket angenommen und umgekehrt.
//!
//! Dieses Modul prüft nur. Laden, Tauschen und Wächter folgen in späteren
//! Schritten.

use serde::Deserialize;

use crate::signatur::{self, Schluessel, SignaturFehler};

/// Obergrenze des Kopfs — er ist wenige hundert Bytes groß.
pub const MAX_KOPF_BYTES: usize = 16 * 1024;

/// Obergrenze der Programmdatei. Heute rund 25 MB; der Deckel hält einen
/// Irrläufer vom Speicher fern.
pub const MAX_EXE_BYTES: u64 = 200 * 1024 * 1024;

const FORMAT: u64 = 1;
const ART: &str = "kern";

/// Warum ein Kopf oder eine Programmdatei abgelehnt wurde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ablehnung {
    /// Der Kopf ist größer als [`MAX_KOPF_BYTES`].
    KopfZuGross,
    /// Die Signatur über die rohen Kopf-Bytes stimmt nicht.
    Signatur(SignaturFehler),
    /// Der Kopf ist kein lesbares JSON mit den erwarteten Feldern.
    Inhalt,
    /// Unbekanntes Format.
    Format { format: u64 },
    /// Signiert, aber kein Kern-Kopf (z. B. ein Seiten-Paket).
    Art,
    /// Die Version im Kopf ist keine `X.Y.Z`.
    Version,
    /// Der Kopf nennt keine neuere Version als die laufende.
    NichtNeuer,
    /// Hash oder Größe im Kopf sind unbrauchbar.
    Angaben,
    /// Die Programmdatei beginnt nicht mit `MZ`.
    KeinProgramm,
    /// Die Programmdatei hat nicht die im Kopf genannte Größe.
    Groesse { kopf: u64, datei: u64 },
    /// Die Programmdatei hat nicht den im Kopf genannten SHA-256.
    Hash,
}

impl std::fmt::Display for Ablehnung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ablehnung::KopfZuGross => f.write_str("Kern-Kopf zu groß"),
            Ablehnung::Signatur(grund) => write!(f, "Signatur: {grund}"),
            Ablehnung::Inhalt => f.write_str("Kern-Kopf unlesbar"),
            Ablehnung::Format { format } => write!(f, "unbekanntes Format {format}"),
            Ablehnung::Art => f.write_str("kein Kern-Kopf"),
            Ablehnung::Version => f.write_str("Version im Kern-Kopf unlesbar"),
            Ablehnung::NichtNeuer => f.write_str("Kern-Kopf nennt keine neuere Version"),
            Ablehnung::Angaben => f.write_str("Hash oder Größe im Kern-Kopf unbrauchbar"),
            Ablehnung::KeinProgramm => f.write_str("Datei ist kein Windows-Programm"),
            Ablehnung::Groesse { kopf, datei } => {
                write!(f, "Programmdatei hat {datei} statt {kopf} Bytes")
            }
            Ablehnung::Hash => f.write_str("Programmdatei passt nicht zum Kern-Kopf"),
        }
    }
}

/// Eine Version `X.Y.Z`, vergleichbar nach Zahlen (nicht nach Zeichen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u32, u32, u32);

impl Version {
    /// Liest genau `X.Y.Z` aus Ziffern. Vorabversionen (`1.2.3-rc1`) und
    /// führende Nullen sind bewusst ungültig — es gibt sie hier nicht, und
    /// eine großzügige Lesart wäre nur eine Fläche für Verwechslungen.
    pub fn lesen(text: &str) -> Option<Self> {
        let mut teile = text.split('.');
        let mut zahl = || -> Option<u32> {
            let t = teile.next()?;
            let gueltig = !t.is_empty()
                && t.len() <= 9
                && t.bytes().all(|b| b.is_ascii_digit())
                && (t == "0" || !t.starts_with('0'));
            gueltig.then(|| t.parse().ok()).flatten()
        };
        let version = Version(zahl()?, zahl()?, zahl()?);
        teile.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Ein geprüfter Kern-Kopf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernKopf {
    pub version: Version,
    /// SHA-256 der Programmdatei.
    pub exe_sha256: [u8; 32],
    pub exe_groesse: u64,
    /// Älteste Version, die die Daten dieser Version noch lesen kann
    /// (ADR 0101). `None` = keine oder unlesbare Angabe = **nicht lesbar**.
    pub rueckwaerts_lesbar_ab: Option<Version>,
}

/// Zuerst nur, was das signierte Ding überhaupt ist — damit ein fremdes
/// signiertes Dokument als „falsche Art" abgelehnt wird, nicht als
/// Lesefehler.
#[derive(Deserialize)]
struct Vorspann {
    format: u64,
    art: String,
}

#[derive(Deserialize)]
struct Rumpf {
    version: String,
    exe_sha256: String,
    exe_groesse: u64,
    #[serde(default)]
    rueckwaerts_lesbar_ab: Option<String>,
}

/// Prüft einen Kern-Kopf: Größe, Signatur über die **rohen** Bytes, Art,
/// und dass er eine neuere Version nennt als die laufende. Lädt und
/// verändert nichts.
pub fn kopf_pruefen(
    bytes: &[u8],
    signatur: &str,
    schluessel: &Schluessel,
    eigene_version: &str,
) -> Result<KernKopf, Ablehnung> {
    if bytes.len() > MAX_KOPF_BYTES {
        return Err(Ablehnung::KopfZuGross);
    }
    signatur::pruefen(schluessel, bytes, signatur).map_err(Ablehnung::Signatur)?;
    let vorspann: Vorspann = serde_json::from_slice(bytes).map_err(|_| Ablehnung::Inhalt)?;
    if vorspann.format != FORMAT {
        return Err(Ablehnung::Format {
            format: vorspann.format,
        });
    }
    if vorspann.art != ART {
        return Err(Ablehnung::Art);
    }
    let rumpf: Rumpf = serde_json::from_slice(bytes).map_err(|_| Ablehnung::Inhalt)?;
    let version = Version::lesen(&rumpf.version).ok_or(Ablehnung::Version)?;
    // Die eigene Version stammt aus der Programmdatei und ist immer lesbar;
    // wäre sie es nicht, gilt nichts als neuer.
    let eigene = Version::lesen(eigene_version).ok_or(Ablehnung::NichtNeuer)?;
    if version <= eigene {
        return Err(Ablehnung::NichtNeuer);
    }
    let exe_sha256 = hex32(&rumpf.exe_sha256).ok_or(Ablehnung::Angaben)?;
    if rumpf.exe_groesse < 2 || rumpf.exe_groesse > MAX_EXE_BYTES {
        return Err(Ablehnung::Angaben);
    }
    Ok(KernKopf {
        version,
        exe_sha256,
        exe_groesse: rumpf.exe_groesse,
        rueckwaerts_lesbar_ab: rumpf
            .rueckwaerts_lesbar_ab
            .as_deref()
            .and_then(Version::lesen),
    })
}

/// Prüft die Programmdatei gegen den Kopf: Windows-Programm (`MZ`), Größe,
/// SHA-256. Erst danach darf sie überhaupt auf die Platte neben die
/// laufende Version.
pub fn exe_pruefen(kopf: &KernKopf, exe: &[u8]) -> Result<(), Ablehnung> {
    if !exe.starts_with(b"MZ") {
        return Err(Ablehnung::KeinProgramm);
    }
    if exe.len() as u64 != kopf.exe_groesse {
        return Err(Ablehnung::Groesse {
            kopf: kopf.exe_groesse,
            datei: exe.len() as u64,
        });
    }
    let hash = ring::digest::digest(&ring::digest::SHA256, exe);
    // Kein Geheimnis im Spiel — ein gewöhnlicher Vergleich genügt.
    if hash.as_ref() != kopf.exe_sha256 {
        return Err(Ablehnung::Hash);
    }
    Ok(())
}

/// Darf von einer Version mit der Grenze `rueckwaerts_lesbar_ab` zurück auf
/// `vorher` gewechselt werden? Nur, wenn die Vorversion die Daten noch lesen
/// kann. **Fehlt die Angabe, gilt „nicht lesbar"** (ADR 0101) — lieber kein
/// Rückweg als einer, nach dem die App im Einrichtungs-Assistenten landet.
pub fn rueckweg_erlaubt(rueckwaerts_lesbar_ab: Option<Version>, vorher: &str) -> bool {
    match (rueckwaerts_lesbar_ab, Version::lesen(vorher)) {
        (Some(grenze), Some(vorher)) => vorher >= grenze,
        _ => false,
    }
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    let b = text.as_bytes();
    if b.len() != 64 {
        return None;
    }
    let ziffer = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    };
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = ziffer(b[2 * i])? << 4 | ziffer(b[2 * i + 1])?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatur::testhilfe::Testschluessel;

    const EIGENE: &str = "1.2.3";

    fn exe() -> Vec<u8> {
        let mut bytes = b"MZ".to_vec();
        bytes.extend_from_slice(&[7u8; 4096]);
        bytes
    }

    fn sha_hex(bytes: &[u8]) -> String {
        ring::digest::digest(&ring::digest::SHA256, bytes)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn kopf_json(version: &str, lesbar_ab: Option<&str>) -> serde_json::Value {
        let exe = exe();
        let mut json = serde_json::json!({
            "format": 1, "art": "kern", "version": version,
            "exe_sha256": sha_hex(&exe), "exe_groesse": exe.len(),
        });
        if let Some(ab) = lesbar_ab {
            json["rueckwaerts_lesbar_ab"] = ab.into();
        }
        json
    }

    fn pruefe(json: &serde_json::Value) -> Result<KernKopf, Ablehnung> {
        let schluessel = Testschluessel::neu(1);
        let bytes = serde_json::to_vec(json).unwrap();
        kopf_pruefen(
            &bytes,
            &schluessel.signieren(&bytes),
            &schluessel.oeffentlich(),
            EIGENE,
        )
    }

    #[test]
    fn gueltiger_kopf_und_passende_exe_werden_angenommen() {
        let kopf = pruefe(&kopf_json("1.2.4", Some("1.2.0"))).expect("gültig");
        assert_eq!(kopf.version, Version(1, 2, 4));
        assert_eq!(kopf.rueckwaerts_lesbar_ab, Some(Version(1, 2, 0)));
        assert_eq!(exe_pruefen(&kopf, &exe()), Ok(()));
    }

    /// Der Bau-Weg der CI gegen die Prüfung der App: `scripts/
    /// build-kern-kopf.mjs` schreibt den Kopf, `kopf_pruefen` und
    /// `exe_pruefen` müssen ihn annehmen. Driften Skript und App auseinander
    /// (Feld umbenannt, Hash anders geschrieben), wäre sonst jeder
    /// veröffentlichte Kopf auf jedem Turnier-PC wertlos.
    #[test]
    fn kopf_aus_dem_bauskript_besteht_die_app_pruefung() {
        let dir = tempfile::tempdir().unwrap();
        let exe_pfad = dir.path().join("bts-light.exe");
        std::fs::write(&exe_pfad, exe()).unwrap();
        let datenformat = dir.path().join("datenformat.json");
        std::fs::write(&datenformat, r#"{"rueckwaerts_lesbar_ab":"1.2.3"}"#).unwrap();
        let kopf_pfad = dir.path().join("kopf.json");
        let wurzel = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let lauf = std::process::Command::new("node")
            .arg(wurzel.join("scripts").join("build-kern-kopf.mjs"))
            .arg("kopf")
            .arg("--exe")
            .arg(&exe_pfad)
            .args(["--version", "1.2.4", "--datenformat"])
            .arg(&datenformat)
            .arg("--out")
            .arg(&kopf_pfad)
            .output()
            .expect("node muss für diesen Test vorhanden sein");
        assert!(
            lauf.status.success(),
            "Bau-Skript gescheitert: {}",
            String::from_utf8_lossy(&lauf.stderr)
        );

        // Signiert werden genau die Bytes der Datei.
        let bytes = std::fs::read(&kopf_pfad).unwrap();
        let schluessel = Testschluessel::neu(1);
        let kopf = kopf_pruefen(
            &bytes,
            &schluessel.signieren(&bytes),
            &schluessel.oeffentlich(),
            EIGENE,
        )
        .expect("ein Kopf aus dem Bau-Skript muss die Prüfung bestehen");
        assert_eq!(kopf.version, Version(1, 2, 4));
        assert_eq!(kopf.rueckwaerts_lesbar_ab, Some(Version(1, 2, 3)));
        assert_eq!(exe_pruefen(&kopf, &exe()), Ok(()));
        assert!(rueckweg_erlaubt(kopf.rueckwaerts_lesbar_ab, EIGENE));
    }

    #[test]
    fn kopf_mit_falscher_signatur_abgelehnt() {
        let eigener = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let mut bytes = serde_json::to_vec(&kopf_json("1.2.4", None)).unwrap();
        // Fremder Schlüssel.
        assert_eq!(
            kopf_pruefen(
                &bytes,
                &fremder.signieren(&bytes),
                &eigener.oeffentlich(),
                EIGENE
            ),
            Err(Ablehnung::Signatur(SignaturFehler::Ungueltig))
        );
        // Nach dem Signieren verändert (Version hochgesetzt).
        let sig = eigener.signieren(&bytes);
        let stelle = bytes.windows(5).position(|w| w == b"1.2.4").unwrap();
        bytes[stelle + 4] = b'9';
        assert_eq!(
            kopf_pruefen(&bytes, &sig, &eigener.oeffentlich(), EIGENE),
            Err(Ablehnung::Signatur(SignaturFehler::Ungueltig))
        );
        // Gar keine Signatur.
        assert_eq!(
            kopf_pruefen(&bytes, "", &eigener.oeffentlich(), EIGENE),
            Err(Ablehnung::Signatur(SignaturFehler::SignaturUnlesbar))
        );
    }

    #[test]
    fn kopf_fuer_aeltere_version_abgelehnt() {
        for nicht_neuer in ["1.2.3", "1.2.2", "1.1.99", "0.9.999"] {
            assert_eq!(
                pruefe(&kopf_json(nicht_neuer, None)),
                Err(Ablehnung::NichtNeuer),
                "{nicht_neuer}"
            );
        }
        // Zahlen, nicht Zeichen: 1.2.10 ist neuer als 1.2.3.
        assert!(pruefe(&kopf_json("1.2.10", None)).is_ok());
        assert!(pruefe(&kopf_json("1.10.0", None)).is_ok());
        for krumm in ["1.2", "1.2.4-rc1", "v1.2.4", "1.2.04", "1.2.4.5", ""] {
            assert_eq!(
                pruefe(&kopf_json(krumm, None)),
                Err(Ablehnung::Version),
                "{krumm:?}"
            );
        }
    }

    #[test]
    fn falsche_art_und_fremdes_format_abgelehnt() {
        let mut seiten = kopf_json("1.2.4", None);
        seiten["art"] = "seiten".into();
        assert_eq!(pruefe(&seiten), Err(Ablehnung::Art));
        let mut neu = kopf_json("1.2.4", None);
        neu["format"] = 2.into();
        assert_eq!(pruefe(&neu), Err(Ablehnung::Format { format: 2 }));
        // Ein signiertes Seiten-Paket-Dokument ist „falsche Art", kein
        // Lesefehler.
        let fremd = serde_json::json!({"format": 1, "art": "seiten", "kern": "1.2.3", "folge": 1});
        assert_eq!(pruefe(&fremd), Err(Ablehnung::Art));
        assert_eq!(
            pruefe(&serde_json::json!({"hallo": 1})),
            Err(Ablehnung::Inhalt)
        );
    }

    #[test]
    fn unbrauchbare_angaben_abgelehnt() {
        let mut kurz = kopf_json("1.2.4", None);
        kurz["exe_sha256"] = "abc".into();
        assert_eq!(pruefe(&kurz), Err(Ablehnung::Angaben));
        let mut gross = kopf_json("1.2.4", None);
        gross["exe_sha256"] = "A".repeat(64).into();
        assert_eq!(
            pruefe(&gross),
            Err(Ablehnung::Angaben),
            "nur Kleinbuchstaben"
        );
        let mut leer = kopf_json("1.2.4", None);
        leer["exe_groesse"] = 0.into();
        assert_eq!(pruefe(&leer), Err(Ablehnung::Angaben));
        let mut riesig = kopf_json("1.2.4", None);
        riesig["exe_groesse"] = (MAX_EXE_BYTES + 1).into();
        assert_eq!(pruefe(&riesig), Err(Ablehnung::Angaben));
    }

    #[test]
    fn zu_grosser_kopf_abgelehnt_vor_der_signatur() {
        let schluessel = Testschluessel::neu(1);
        let riesig = vec![b' '; MAX_KOPF_BYTES + 1];
        assert_eq!(
            kopf_pruefen(&riesig, "", &schluessel.oeffentlich(), EIGENE),
            Err(Ablehnung::KopfZuGross)
        );
    }

    #[test]
    fn exe_hash_passt_nicht_abgelehnt() {
        let kopf = pruefe(&kopf_json("1.2.4", None)).unwrap();
        let mut andere = exe();
        let ende = andere.len() - 1;
        andere[ende] ^= 1;
        assert_eq!(exe_pruefen(&kopf, &andere), Err(Ablehnung::Hash));
    }

    #[test]
    fn exe_groesse_passt_nicht_abgelehnt() {
        let kopf = pruefe(&kopf_json("1.2.4", None)).unwrap();
        let mut laenger = exe();
        laenger.push(0);
        assert_eq!(
            exe_pruefen(&kopf, &laenger),
            Err(Ablehnung::Groesse {
                kopf: 4098,
                datei: 4099
            })
        );
        assert_eq!(
            exe_pruefen(&kopf, &exe()[..100]),
            Err(Ablehnung::Groesse {
                kopf: 4098,
                datei: 100
            })
        );
    }

    #[test]
    fn kein_mz_abgelehnt() {
        // Auch mit passendem Hash und passender Größe: Ein Zip oder ein
        // Skript ist kein Programm.
        let mut zip = b"PK".to_vec();
        zip.extend_from_slice(&[7u8; 4096]);
        let schluessel = Testschluessel::neu(1);
        let json = serde_json::json!({
            "format": 1, "art": "kern", "version": "1.2.4",
            "exe_sha256": sha_hex(&zip), "exe_groesse": zip.len(),
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let kopf = kopf_pruefen(
            &bytes,
            &schluessel.signieren(&bytes),
            &schluessel.oeffentlich(),
            EIGENE,
        )
        .unwrap();
        assert_eq!(exe_pruefen(&kopf, &zip), Err(Ablehnung::KeinProgramm));
        assert_eq!(exe_pruefen(&kopf, b""), Err(Ablehnung::KeinProgramm));
    }

    #[test]
    fn fehlende_angabe_heisst_nicht_lesbar() {
        // Ohne Angabe im Kopf: kein Rückweg.
        let ohne = pruefe(&kopf_json("1.2.4", None)).unwrap();
        assert_eq!(ohne.rueckwaerts_lesbar_ab, None);
        assert!(!rueckweg_erlaubt(ohne.rueckwaerts_lesbar_ab, "1.2.3"));
        // Unlesbare Angabe zählt wie keine.
        let krumm = pruefe(&kopf_json("1.2.4", Some("irgendwas"))).unwrap();
        assert_eq!(krumm.rueckwaerts_lesbar_ab, None);

        // Mit Grenze 1.2.0: 1.2.0 und neuer dürfen, ältere nicht.
        let grenze = Version::lesen("1.2.0");
        assert!(rueckweg_erlaubt(grenze, "1.2.0"));
        assert!(rueckweg_erlaubt(grenze, "1.2.3"));
        assert!(rueckweg_erlaubt(grenze, "1.10.0"));
        assert!(!rueckweg_erlaubt(grenze, "1.1.99"));
        assert!(!rueckweg_erlaubt(grenze, "0.9.0"));
        // Eine unlesbare Vorversion darf nie.
        assert!(!rueckweg_erlaubt(grenze, "alt"));
    }

    #[test]
    fn versionen_werden_nach_zahlen_verglichen() {
        assert!(Version::lesen("0.9.100").unwrap() > Version::lesen("0.9.99").unwrap());
        assert!(Version::lesen("0.10.0").unwrap() > Version::lesen("0.9.999").unwrap());
        assert_eq!(Version::lesen("0.9.434").unwrap().to_string(), "0.9.434");
        assert_eq!(Version::lesen("1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::lesen("1..3"), None);
        assert_eq!(Version::lesen("1.2.x"), None);
    }
}
