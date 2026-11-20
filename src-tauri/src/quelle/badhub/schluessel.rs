//! Verschlüsselung des Personenauszugs und des Gerätetokens auf der Platte
//! (badhub-Spec, Abschnitt „Personendaten auf dem Hallen-PC“).
//!
//! AES-256-GCM über `ring` (liegt ohnehin im Baum). Der Schlüssel liegt in
//! `turniere/schluessel.bin` und ist unter Windows mit **DPAPI** an das
//! Benutzerkonto gebunden (`CryptProtectData`): Eine kopierte Datei ist auf
//! einem anderen PC oder unter einem anderen Konto unlesbar. Unter Linux
//! (nur Entwicklung und CI, die App ist eine Windows-App) liegt der Schlüssel
//! roh mit Dateirechten 0600 — als solcher gekennzeichnet (`ROH1`), damit eine
//! Datei nie unbemerkt zwischen den Arten wechselt.

use std::path::Path;

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};

/// Kopf einer verschlüsselten Datei (Format 1).
const MAGIC: &[u8] = b"BTSK1";
/// Kopf der Schlüsseldatei je Schutzart.
const KOPF_DPAPI: &[u8] = b"DPAPI1";
const KOPF_ROH: &[u8] = b"ROH1";

/// Lädt den Schlüssel oder legt ihn beim ersten Mal an.
pub fn laden_oder_erzeugen(datei: &Path) -> Result<[u8; 32], String> {
    match std::fs::read(datei) {
        Ok(inhalt) => entpacken(&inhalt),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut roh = [0u8; 32];
            SystemRandom::new()
                .fill(&mut roh)
                .map_err(|_| "Zufallsquelle nicht verfügbar".to_string())?;
            let verpackt = verpacken(&roh)?;
            if let Some(dir) = datei.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("Schlüsselordner: {e}"))?;
            }
            schreiben_privat(datei, &verpackt)?;
            Ok(roh)
        }
        Err(e) => Err(format!("Schlüssel nicht lesbar: {e}")),
    }
}

/// Verschlüsselt `klar` (Zufalls-Nonce je Aufruf).
pub fn verschluesseln(schluessel: &[u8; 32], klar: &[u8]) -> Result<Vec<u8>, String> {
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, schluessel).map_err(|_| "Schlüssel ungültig")?,
    );
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| "Zufallsquelle nicht verfügbar".to_string())?;
    let mut puffer = klar.to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(MAGIC),
        &mut puffer,
    )
    .map_err(|_| "Verschlüsseln fehlgeschlagen".to_string())?;
    let mut aus = Vec::with_capacity(MAGIC.len() + NONCE_LEN + puffer.len());
    aus.extend_from_slice(MAGIC);
    aus.extend_from_slice(&nonce);
    aus.extend_from_slice(&puffer);
    Ok(aus)
}

/// Entschlüsselt, was [`verschluesseln`] erzeugt hat. Falscher Schlüssel oder
/// veränderte Datei → Fehler (GCM prüft die Echtheit).
pub fn entschluesseln(schluessel: &[u8; 32], daten: &[u8]) -> Result<Vec<u8>, String> {
    let rest = daten
        .strip_prefix(MAGIC)
        .ok_or("Unbekanntes Dateiformat (verschlüsselt)")?;
    if rest.len() < NONCE_LEN {
        return Err("Verschlüsselte Datei zu kurz".into());
    }
    let (nonce, chiffre) = rest.split_at(NONCE_LEN);
    let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| "Nonce ungültig")?;
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, schluessel).map_err(|_| "Schlüssel ungültig")?,
    );
    let mut puffer = chiffre.to_vec();
    let klar = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(MAGIC),
            &mut puffer,
        )
        .map_err(|_| "Entschlüsseln fehlgeschlagen (anderer PC oder Benutzer?)".to_string())?;
    Ok(klar.to_vec())
}

/// Schreibt eine Datei nur für den eigenen Benutzer lesbar (Unix: 0600;
/// unter Windows erbt sie die Rechte des Profilordners).
pub fn schreiben_privat(datei: &Path, inhalt: &[u8]) -> Result<(), String> {
    let tmp = datei.with_extension("tmp");
    {
        use std::io::Write;
        let mut opt = std::fs::OpenOptions::new();
        opt.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opt.mode(0o600);
        }
        let mut f = opt.open(&tmp).map_err(|e| format!("Schreiben: {e}"))?;
        f.write_all(inhalt).map_err(|e| format!("Schreiben: {e}"))?;
        f.sync_all().map_err(|e| format!("Schreiben: {e}"))?;
    }
    std::fs::rename(&tmp, datei).map_err(|e| format!("Schreiben: {e}"))
}

#[cfg(windows)]
fn verpacken(roh: &[u8; 32]) -> Result<Vec<u8>, String> {
    let mut aus = KOPF_DPAPI.to_vec();
    aus.extend(dpapi(roh, true)?);
    Ok(aus)
}

#[cfg(not(windows))]
fn verpacken(roh: &[u8; 32]) -> Result<Vec<u8>, String> {
    let mut aus = KOPF_ROH.to_vec();
    aus.extend_from_slice(roh);
    Ok(aus)
}

fn entpacken(inhalt: &[u8]) -> Result<[u8; 32], String> {
    let roh: Vec<u8> = if let Some(rest) = inhalt.strip_prefix(KOPF_DPAPI) {
        dpapi_loesen(rest)?
    } else if let Some(rest) = inhalt.strip_prefix(KOPF_ROH) {
        rest.to_vec()
    } else {
        return Err("Schlüsseldatei hat ein unbekanntes Format".into());
    };
    roh.try_into()
        .map_err(|_| "Schlüssel hat die falsche Länge".to_string())
}

#[cfg(windows)]
fn dpapi_loesen(daten: &[u8]) -> Result<Vec<u8>, String> {
    dpapi(daten, false)
}

#[cfg(not(windows))]
fn dpapi_loesen(_daten: &[u8]) -> Result<Vec<u8>, String> {
    Err("Schlüssel ist mit DPAPI geschützt (nur unter Windows lesbar)".into())
}

/// DPAPI: Daten an das Windows-Benutzerkonto binden bzw. lösen.
#[cfg(windows)]
fn dpapi(daten: &[u8], schuetzen: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let ein = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(daten.len()).map_err(|_| "Daten zu groß")?,
        pbData: daten.as_ptr() as *mut u8,
    };
    let mut aus = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `ein` zeigt auf gültigen Speicher für die Dauer des Aufrufs;
    // `aus` füllt Windows mit einem per LocalAlloc reservierten Block, den wir
    // nach dem Kopieren mit LocalFree freigeben.
    unsafe {
        let ergebnis = if schuetzen {
            CryptProtectData(
                &ein,
                windows::core::PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut aus,
            )
        } else {
            CryptUnprotectData(
                &ein,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut aus,
            )
        };
        ergebnis.map_err(|e| format!("Windows-Schlüsselspeicher: {e}"))?;
        let kopie = std::slice::from_raw_parts(aus.pbData, aus.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(aus.pbData as *mut core::ffi::c_void)));
        Ok(kopie)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundreise_und_echtheit() {
        let dir = tempfile::tempdir().unwrap();
        let datei = dir.path().join("turniere").join("schluessel.bin");
        let k = laden_oder_erzeugen(&datei).unwrap();
        // Zweites Laden liefert denselben Schlüssel.
        assert_eq!(laden_oder_erzeugen(&datei).unwrap(), k);
        let chiffre = verschluesseln(&k, b"Max Mustermann").unwrap();
        assert!(
            !chiffre.windows(4).any(|w| w == b"Must"),
            "Klartext darf nicht in der Datei stehen"
        );
        assert_eq!(entschluesseln(&k, &chiffre).unwrap(), b"Max Mustermann");
        // Veränderte Datei fällt auf.
        let mut kaputt = chiffre.clone();
        let letzte = kaputt.len() - 1;
        kaputt[letzte] ^= 1;
        assert!(entschluesseln(&k, &kaputt).is_err());
        // Anderer Schlüssel liest nichts.
        assert!(entschluesseln(&[7u8; 32], &chiffre).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn schluesseldatei_nur_fuer_den_benutzer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let datei = dir.path().join("schluessel.bin");
        laden_oder_erzeugen(&datei).unwrap();
        let modus = std::fs::metadata(&datei).unwrap().permissions().mode() & 0o777;
        assert_eq!(modus, 0o600);
    }
}
