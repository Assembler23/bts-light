//! Ablage eines badhub-Turniers auf der Platte, getrennt von allen BTP-Stores
//! (badhub-Spec „BTP bleibt unverändert“, Punkt 4):
//!
//! ```text
//! <App-Daten>/turniere/schluessel.bin      Schlüssel (DPAPI unter Windows)
//! <App-Daten>/turniere/<uuid>/paket.json   Paket ohne Personen + ETag
//! <App-Daten>/turniere/<uuid>/personen.bin Personenauszug, verschlüsselt
//! <App-Daten>/turniere/<uuid>/geraet.bin   Gerätetoken, verschlüsselt
//! ```
//!
//! Die UUID wird vor jedem Pfadbau geprüft ([`super::uuid_gueltig`]) — sie
//! kommt aus dem Netz.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paket::{Paket, Personen};
use super::schluessel;

/// Inhalt von `paket.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GespeichertesPaket {
    /// Paket **ohne** `meldungen`/`schiedsrichter`.
    pub paket: Paket,
    /// ETag der letzten Paket-Antwort (für `If-None-Match`).
    #[serde(default)]
    pub etag: Option<String>,
}

/// Verzeichnisse eines Turniers.
#[derive(Debug, Clone)]
pub struct Ablage {
    wurzel: PathBuf,
    dir: PathBuf,
}

impl Ablage {
    /// `daten` = App-Datenverzeichnis. Fehler bei ungültiger UUID.
    pub fn neu(daten: &Path, uuid: &str) -> Result<Self, String> {
        if !super::uuid_gueltig(uuid) {
            return Err("Ungültige Turnier-UUID".into());
        }
        let wurzel = daten.join("turniere");
        let dir = wurzel.join(uuid.to_ascii_lowercase());
        Ok(Self { wurzel, dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn schluessel(&self) -> Result<[u8; 32], String> {
        schluessel::laden_oder_erzeugen(&self.wurzel.join("schluessel.bin"))
    }

    /// Legt Paket (Personen verschlüsselt) und ETag ab.
    pub fn paket_speichern(&self, paket: &Paket, etag: Option<&str>) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("Turnierordner: {e}"))?;
        let (rest, personen) = paket.ohne_personen();
        let k = self.schluessel()?;
        let personen_json = serde_json::to_vec(&personen).map_err(|e| e.to_string())?;
        schluessel::schreiben_privat(
            &self.dir.join("personen.bin"),
            &schluessel::verschluesseln(&k, &personen_json)?,
        )?;
        let gespeichert = GespeichertesPaket {
            paket: rest,
            etag: etag.map(str::to_string),
        };
        let text = serde_json::to_vec(&gespeichert).map_err(|e| e.to_string())?;
        schluessel::schreiben_privat(&self.dir.join("paket.json"), &text)
    }

    /// Schreibt nur `paket.json` neu (neue Ereignisse, Personen unverändert).
    pub fn ereignisse_speichern(&self, paket: &Paket, etag: Option<&str>) -> Result<(), String> {
        let (rest, _) = paket.ohne_personen();
        let gespeichert = GespeichertesPaket {
            paket: rest,
            etag: etag.map(str::to_string),
        };
        let text = serde_json::to_vec(&gespeichert).map_err(|e| e.to_string())?;
        schluessel::schreiben_privat(&self.dir.join("paket.json"), &text)
    }

    /// Lädt Paket samt entschlüsseltem Personenauszug. `Ok(None)`, wenn noch
    /// keins abgelegt ist. Ein unlesbarer Personenauszug lässt das Paket
    /// stehen, nur ohne Namen — die Halle läuft weiter.
    pub fn paket_laden(&self) -> Result<Option<(Paket, Option<String>)>, String> {
        let text = match std::fs::read(self.dir.join("paket.json")) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Turnierpaket nicht lesbar: {e}")),
        };
        let g: GespeichertesPaket = serde_json::from_slice(&text)
            .map_err(|_| "Turnierpaket auf der Platte ist beschädigt".to_string())?;
        let personen = match std::fs::read(self.dir.join("personen.bin")) {
            Ok(daten) => self
                .schluessel()
                .and_then(|k| schluessel::entschluesseln(&k, &daten))
                .and_then(|klar| {
                    serde_json::from_slice::<Personen>(&klar)
                        .map_err(|_| "Personenauszug beschädigt".to_string())
                })
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "badhub-Turnier: Personenauszug nicht lesbar ({e}) — ohne Namen"
                    );
                    Personen::default()
                }),
            Err(_) => Personen::default(),
        };
        Ok(Some((g.paket.mit_personen(personen), g.etag)))
    }

    /// Legt das Gerätetoken verschlüsselt ab.
    pub fn token_speichern(&self, token: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("Turnierordner: {e}"))?;
        let k = self.schluessel()?;
        schluessel::schreiben_privat(
            &self.dir.join("geraet.bin"),
            &schluessel::verschluesseln(&k, token.as_bytes())?,
        )
    }

    /// Legt ein Geheimnis (z. B. den Relay-Zugang für badhub) verschlüsselt
    /// unter `name` ab.
    pub fn geheimnis_speichern(&self, name: &str, wert: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("Turnierordner: {e}"))?;
        let k = self.schluessel()?;
        schluessel::schreiben_privat(
            &self.dir.join(name),
            &schluessel::verschluesseln(&k, wert.as_bytes())?,
        )
    }

    /// Liest ein mit [`Self::geheimnis_speichern`] abgelegtes Geheimnis.
    pub fn geheimnis_laden(&self, name: &str) -> Option<String> {
        let daten = std::fs::read(self.dir.join(name)).ok()?;
        let k = self.schluessel().ok()?;
        String::from_utf8(schluessel::entschluesseln(&k, &daten).ok()?).ok()
    }

    /// Löscht den Personenauszug (nach der Rückgabe bzw. nach der Frist).
    pub fn personen_loeschen(&self) {
        match std::fs::remove_file(self.dir.join("personen.bin")) {
            Ok(()) => tracing::info!("badhub-Turnier: Personenauszug gelöscht"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("badhub-Turnier: Personenauszug nicht gelöscht ({e})"),
        }
    }

    /// Liest das Gerätetoken; `None`, wenn keins (lesbar) da ist.
    pub fn token_laden(&self) -> Option<String> {
        let daten = std::fs::read(self.dir.join("geraet.bin")).ok()?;
        let k = self.schluessel().ok()?;
        let klar = schluessel::entschluesseln(&k, &daten).ok()?;
        String::from_utf8(klar).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quelle::badhub::paket::{PaketMeldung, PaketPerson, PaketTurnier};

    const UUID: &str = "0f8b2c1e-1111-4222-8333-444455556666";

    fn paket() -> Paket {
        Paket {
            schema: 1,
            turnier: PaketTurnier {
                id: 17,
                uuid: UUID.into(),
                guid: None,
                name: "Testturnier".into(),
                beginn: None,
                ende: None,
                lease: None,
            },
            meldungen: vec![PaketMeldung {
                meldung: 1,
                konkurrenz: "HE".into(),
                personen: vec![PaketPerson {
                    person: 81,
                    vorname: "Erika".into(),
                    nachname: "Geheimname".into(),
                    verein: Some("TSV Beispiel".into()),
                    nation: Some("GER".into()),
                    lizenz: None,
                }],
            }],
            ..Paket::default()
        }
    }

    #[test]
    fn personen_liegen_nur_verschluesselt_auf_der_platte() {
        let daten = tempfile::tempdir().unwrap();
        let a = Ablage::neu(daten.path(), UUID).unwrap();
        a.paket_speichern(&paket(), Some("\"e1\"")).unwrap();
        a.token_speichern("geheimes-token").unwrap();
        // Kein Klartext-Name, kein Verein, kein Token in irgendeiner Datei.
        for eintrag in std::fs::read_dir(a.dir()).unwrap() {
            let inhalt = std::fs::read(eintrag.unwrap().path()).unwrap();
            for wort in [&b"Geheimname"[..], b"TSV Beispiel", b"geheimes-token"] {
                assert!(!inhalt.windows(wort.len()).any(|w| w == wort));
            }
        }
        let (geladen, etag) = a.paket_laden().unwrap().unwrap();
        assert_eq!(geladen, paket());
        assert_eq!(etag.as_deref(), Some("\"e1\""));
        assert_eq!(a.token_laden().as_deref(), Some("geheimes-token"));
    }

    #[test]
    fn ungueltige_uuid_baut_keinen_pfad() {
        let daten = tempfile::tempdir().unwrap();
        for boese in ["../x", "a/b", "", "..", "C:\\x"] {
            assert!(Ablage::neu(daten.path(), boese).is_err(), "{boese}");
        }
    }

    #[test]
    fn ohne_ablage_kein_paket() {
        let daten = tempfile::tempdir().unwrap();
        let a = Ablage::neu(daten.path(), UUID).unwrap();
        assert!(a.paket_laden().unwrap().is_none());
        assert!(a.token_laden().is_none());
    }
}
