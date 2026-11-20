//! Lokales Protokoll eines badhub-Turniers unter der Lease (Paket L3):
//! `turniere/<uuid>/protokoll.jsonl`.
//!
//! Jede Zeile ist ein **Posten**: ein Kernbefehl samt den Ereignissen, die
//! der Kern dafür erzeugt hat — genau die Form, die badhub beim Hochladen
//! erwartet (`posten: [{befehl, ereignisse}]`). Dazu, nur lokal, wer
//! gehandelt hat und wann.
//!
//! **Haltbarkeit:** Angehängt wird mit einem einzigen `write_all` und
//! anschließendem `sync_data` (wie `btp-retry.json`). Ein Absturz mitten im
//! Schreiben hinterlässt höchstens eine angefangene letzte Zeile; die wird
//! beim Laden verworfen (und abgeschnitten), alles davor bleibt gültig.
//! Kein Personenname steht in dieser Datei: Befehle und Ereignisse tragen nur
//! Nummern (Meldung, Person, Schiedsrichter-ID).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use turnierkern::{TurnierBefehl, TurnierEreignis};

/// Dateiname im Turnierordner.
pub const DATEI: &str = "protokoll.jsonl";

/// Ein Posten: Befehl + erzeugte Ereignisse (+ lokale Herkunft).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Posten {
    pub befehl: TurnierBefehl,
    pub ereignisse: Vec<TurnierEreignis>,
    /// Woher der Befehl kam (`bts-light`, `badhub`). Nur lokal, geht nicht
    /// an badhub.
    #[serde(default)]
    pub akteur: String,
    /// Lokale Zeit der Annahme (Unix-ms).
    #[serde(default)]
    pub zeit_ms: u64,
}

impl Posten {
    /// Höchstes `n` dieses Postens (0 ohne Ereignisse).
    pub fn n(&self) -> u64 {
        self.ereignisse
            .iter()
            .map(TurnierEreignis::n)
            .max()
            .unwrap_or(0)
    }

    /// Die Form, die badhub beim Hochladen erwartet (ohne lokale Felder).
    pub fn fuer_badhub(&self) -> serde_json::Value {
        serde_json::json!({ "befehl": self.befehl, "ereignisse": self.ereignisse })
    }
}

/// Das Protokoll auf der Platte und im Speicher.
#[derive(Debug, Clone)]
pub struct Protokoll {
    datei: PathBuf,
    posten: Vec<Posten>,
}

impl Protokoll {
    /// Lädt `dir/protokoll.jsonl` (fehlt die Datei: leer). Eine defekte
    /// letzte Zeile (Absturz beim Schreiben) wird verworfen und aus der Datei
    /// geschnitten; eine defekte Zeile mitten in der Datei wird übergangen
    /// (gemeldet ohne Inhalt). Posten, deren Ereignisse nicht lückenlos an
    /// die vorigen anschließen, werden ebenfalls übergangen.
    pub fn laden(dir: &Path) -> Result<Self, String> {
        let datei = dir.join(DATEI);
        let roh = match std::fs::read(&datei) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    datei,
                    posten: Vec::new(),
                })
            }
            Err(e) => return Err(format!("Protokoll nicht lesbar: {e}")),
        };
        let mut posten: Vec<Posten> = Vec::new();
        let mut gut_bis = 0usize; // Byte-Länge des gültigen Anfangs
        let mut pos = 0usize;
        let mut defekt_in_der_mitte = 0usize;
        while pos < roh.len() {
            let ende = roh[pos..].iter().position(|b| *b == b'\n');
            let (zeile, naechste, vollstaendig) = match ende {
                Some(i) => (&roh[pos..pos + i], pos + i + 1, true),
                None => (&roh[pos..], roh.len(), false),
            };
            let gelesen = (!zeile.iter().all(u8::is_ascii_whitespace))
                .then(|| serde_json::from_slice::<Posten>(zeile).ok());
            match gelesen {
                None => gut_bis = naechste, // Leerzeile
                Some(Some(p)) if vollstaendig => {
                    let vorher = posten.last().map_or(0, Posten::n);
                    let anschluss = p
                        .ereignisse
                        .first()
                        .is_none_or(|e| vorher == 0 || e.n() == vorher + 1);
                    if anschluss {
                        posten.push(p);
                    } else {
                        defekt_in_der_mitte += 1;
                    }
                    gut_bis = naechste;
                }
                // Lesbar, aber ohne Zeilenende: der Absturz kam nach dem
                // Text, vor dem `\n`. Behalten und das Zeilenende ergänzen.
                Some(Some(p)) => {
                    posten.push(p);
                    gut_bis = naechste;
                }
                Some(None) if naechste >= roh.len() => {
                    // Defekte LETZTE Zeile: verwerfen, `gut_bis` bleibt.
                }
                Some(None) => {
                    defekt_in_der_mitte += 1;
                    gut_bis = naechste;
                }
            }
            pos = naechste;
        }
        if defekt_in_der_mitte > 0 {
            tracing::warn!(
                "badhub-Turnier: {defekt_in_der_mitte} unlesbare Zeile(n) im Protokoll übergangen"
            );
        }
        let ohne_zeilenende = gut_bis > 0 && roh.get(gut_bis - 1) != Some(&b'\n');
        if gut_bis < roh.len() || ohne_zeilenende {
            tracing::warn!("badhub-Turnier: angefangene letzte Protokollzeile verworfen");
            let mut neu = roh[..gut_bis].to_vec();
            if ohne_zeilenende {
                neu.push(b'\n');
            }
            super::schluessel::schreiben_privat(&datei, &neu)?;
        }
        Ok(Self { datei, posten })
    }

    pub fn posten(&self) -> &[Posten] {
        &self.posten
    }

    /// Höchstes `n` im Protokoll (0 = leer).
    pub fn n(&self) -> u64 {
        self.posten.last().map_or(0, Posten::n)
    }

    /// Hängt Posten an — erst auf die Platte (ein Schreibvorgang, `fsync`),
    /// dann in den Speicher. Scheitert die Platte, ändert sich nichts.
    pub fn anhaengen(&mut self, neu: Vec<Posten>) -> Result<(), String> {
        if neu.is_empty() {
            return Ok(());
        }
        let mut puffer = Vec::new();
        for p in &neu {
            serde_json::to_writer(&mut puffer, p).map_err(|e| e.to_string())?;
            puffer.push(b'\n');
        }
        if let Some(dir) = self.datei.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Turnierordner: {e}"))?;
        }
        let mut opt = std::fs::OpenOptions::new();
        opt.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opt.mode(0o600);
        }
        let mut f = opt
            .open(&self.datei)
            .map_err(|e| format!("Protokoll: {e}"))?;
        // Länge vorher merken: Scheitert das Schreiben mittendrin (Platte
        // voll), wird der angefangene Rest wieder abgeschnitten — sonst klebte
        // der nächste gültige Posten an Müll und ginge beim Laden verloren.
        let vorher = f.metadata().map(|m| m.len()).ok();
        if let Err(e) = f.write_all(&puffer).and_then(|_| f.sync_data()) {
            if let Some(l) = vorher {
                let _ = f.set_len(l);
                let _ = f.sync_data();
            }
            return Err(format!("Protokoll: {e}"));
        }
        self.posten.extend(neu);
        Ok(())
    }

    /// Schneidet alle Posten mit `n > bis_n` ab und legt sie daneben als
    /// `verworfen-<ms>.jsonl` ab (Steuerung neu übernehmen nach einer
    /// Rückholung). Liefert die Zahl der verworfenen Posten.
    pub fn kuerzen_auf(&mut self, bis_n: u64, jetzt_ms: u64) -> Result<usize, String> {
        // Erst schreiben, dann den Speicher ersetzen: Scheitert die Platte,
        // bleibt der Stand im Speicher wie auf der Platte.
        let (bleiben, weg): (Vec<Posten>, Vec<Posten>) =
            self.posten.iter().cloned().partition(|p| p.n() <= bis_n);
        let zahl = weg.len();
        let zeilen = |ps: &[Posten]| -> Result<Vec<u8>, String> {
            let mut b = Vec::new();
            for p in ps {
                serde_json::to_writer(&mut b, p).map_err(|e| e.to_string())?;
                b.push(b'\n');
            }
            Ok(b)
        };
        if zahl > 0 {
            if let Some(dir) = self.datei.parent() {
                super::schluessel::schreiben_privat(
                    &dir.join(format!("verworfen-{jetzt_ms}.jsonl")),
                    &zeilen(&weg)?,
                )?;
            }
        }
        super::schluessel::schreiben_privat(&self.datei, &zeilen(&bleiben)?)?;
        self.posten = bleiben;
        Ok(zahl)
    }

    /// Leert das Protokoll ohne Sicherungskopie — nur für Posten, die
    /// badhub nachweislich bestätigt hat (sie kommen über Paket und
    /// Ereignisse zurück).
    pub fn leeren(&mut self) -> Result<(), String> {
        super::schluessel::schreiben_privat(&self.datei, b"")?;
        self.posten.clear();
        Ok(())
    }

    /// Die Posten nach `basis_n` (noch nicht von badhub bestätigt).
    pub fn offen_nach(&self, basis_n: u64) -> impl Iterator<Item = &Posten> {
        self.posten.iter().filter(move |p| p.n() > basis_n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posten(n: u64) -> Posten {
        Posten {
            befehl: TurnierBefehl::AufrufZuruecknehmen {
                id: format!("bts-{n}"),
                konkurrenz: "HE".into(),
                spiel: 1,
            },
            ereignisse: vec![TurnierEreignis::AufrufZurueckgenommen {
                id: format!("bts-{n}"),
                n,
                konkurrenz: "HE".into(),
                spiel: 1,
            }],
            akteur: "bts-light".into(),
            zeit_ms: n,
        }
    }

    #[test]
    fn anhaengen_und_wieder_laden() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Protokoll::laden(dir.path()).unwrap();
        assert_eq!(p.n(), 0);
        p.anhaengen(vec![posten(11), posten(12)]).unwrap();
        p.anhaengen(vec![posten(13)]).unwrap();
        let q = Protokoll::laden(dir.path()).unwrap();
        assert_eq!(q.posten(), p.posten());
        assert_eq!(q.n(), 13);
        assert_eq!(q.offen_nach(11).count(), 2);
    }

    #[test]
    fn abgeschnittene_letzte_zeile_wird_verworfen_der_rest_bleibt() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Protokoll::laden(dir.path()).unwrap();
        p.anhaengen(vec![posten(11), posten(12)]).unwrap();
        // Absturz mitten im dritten Schreibvorgang: halbe Zeile ohne Ende.
        let datei = dir.path().join(DATEI);
        let mut roh = std::fs::read(&datei).unwrap();
        let dritte = serde_json::to_vec(&posten(13)).unwrap();
        roh.extend_from_slice(&dritte[..dritte.len() / 2]);
        std::fs::write(&datei, &roh).unwrap();

        let mut q = Protokoll::laden(dir.path()).unwrap();
        assert_eq!(q.n(), 12, "die angefangene Zeile zählt nicht");
        // Und sie ist aus der Datei: Weiterschreiben ergibt saubere Zeilen.
        q.anhaengen(vec![posten(13)]).unwrap();
        let r = Protokoll::laden(dir.path()).unwrap();
        assert_eq!(r.n(), 13);
        assert_eq!(r.posten().len(), 3);
        let text = String::from_utf8(std::fs::read(&datei).unwrap()).unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn vollstaendige_zeile_ohne_zeilenende_bleibt() {
        let dir = tempfile::tempdir().unwrap();
        let datei = dir.path().join(DATEI);
        let mut roh = serde_json::to_vec(&posten(11)).unwrap();
        roh.push(b'\n');
        roh.extend(serde_json::to_vec(&posten(12)).unwrap());
        std::fs::write(&datei, &roh).unwrap();
        let p = Protokoll::laden(dir.path()).unwrap();
        assert_eq!(p.n(), 12);
        assert!(std::fs::read(&datei).unwrap().ends_with(b"\n"));
    }

    #[test]
    fn kuerzen_legt_das_verworfene_daneben() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Protokoll::laden(dir.path()).unwrap();
        p.anhaengen(vec![posten(11), posten(12), posten(13)])
            .unwrap();
        assert_eq!(p.kuerzen_auf(11, 99).unwrap(), 2);
        assert_eq!(Protokoll::laden(dir.path()).unwrap().n(), 11);
        let verworfen = std::fs::read_to_string(dir.path().join("verworfen-99.jsonl")).unwrap();
        assert_eq!(verworfen.lines().count(), 2);
    }

    #[test]
    fn posten_fuer_badhub_ohne_lokale_felder() {
        let v = posten(11).fuer_badhub();
        assert!(v.get("akteur").is_none());
        assert_eq!(v["befehl"]["art"], "aufruf_zuruecknehmen");
        assert_eq!(v["ereignisse"][0]["n"], 11);
    }
}
