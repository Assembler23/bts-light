//! Seiten-Paket: Format, Prüfung und Ablage (Spec `hotfix-ohne-neustart`,
//! ADR 0099).
//!
//! Ein Paket ersetzt **alle** Geräte-Seiten dieses PCs auf einmal. Es ist
//! Code, der auf Tablets mit Ergebnis-Eingabe und im TL-Web läuft — deshalb
//! wird nichts daraus gelesen, bevor die Signatur über die rohen Bytes
//! stimmt, und jede weitere Prüfung lehnt beim ersten Fehler ab.
//!
//! Dieses Modul lädt nichts aus dem Netz und bietet nichts an; es prüft
//! Bytes und verwaltet die Dateien im App-Datenverzeichnis.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::signatur::{self, Schluessel, SignaturFehler};
use crate::tablet::assets::{self, Quelle, Seite, Seitenstand};

/// Obergrenze der Paket-Datei. Alle Seiten zusammen sind unkomprimiert rund
/// 1,5 MB; 8 MB lassen Luft und halten einen Irrläufer vom Speicher fern.
pub const MAX_PAKET_BYTES: usize = 8 * 1024 * 1024;

/// Obergrenze nach dem Entpacken — Schutz vor einer gzip-Bombe. Greift auch
/// bei einem gültig signierten Paket, damit ein Fehler im Bau-Skript die App
/// nicht in den Speichermangel treibt.
pub const MAX_ENTPACKT_BYTES: usize = 32 * 1024 * 1024;

/// Einziges bekanntes Paket-Format.
const FORMAT: u64 = 1;
const ART: &str = "seiten";

/// Name des Verzeichnisses im App-Datenverzeichnis.
pub const ORDNER: &str = "seiten";

const DATEI_AKTIV: &str = "aktiv.paket";
const DATEI_VORHER: &str = "vorher.paket";
const DATEI_NEU: &str = "neu.paket";
const DATEI_ZUSTAND: &str = "zustand.json";
const DATEI_ZUSTAND_NEU: &str = "zustand.json.neu";
const DATEI_VORHER_NEU: &str = "vorher.paket.neu";
/// Ein beim Start abgelehntes Paket bleibt unter diesem Namen liegen — für
/// die Fehlersuche; geladen wird es nie.
const DATEI_ABGELEHNT: &str = "abgelehnt.paket";

/// Warum ein Paket abgelehnt wurde — in der Reihenfolge der Prüfschritte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ablehnung {
    /// Schritt 1: Datei größer als [`MAX_PAKET_BYTES`].
    ZuGross { bytes: usize },
    /// Schritt 2: Signatur über die rohen Bytes stimmt nicht.
    Signatur(SignaturFehler),
    /// Schritt 3: kein gültiges gzip.
    Entpacken,
    /// Schritt 3: entpackt größer als [`MAX_ENTPACKT_BYTES`].
    ZuGrossEntpackt,
    /// Schritt 4: Inhalt ist kein lesbares Paket.
    Inhalt,
    /// Schritt 4: unbekanntes Format.
    Format { format: u64 },
    /// Schritt 4: signiert, aber kein Seiten-Paket (z. B. ein Kern-Kopf).
    Art,
    /// Schritt 5: Seitenmenge weicht von der eingebauten ab.
    Seitenmenge { fehlend: usize, fremd: usize },
    /// Schritt 6: einer Seite fehlt ein Pflicht-Platzhalter.
    Platzhalter {
        seite: &'static str,
        platzhalter: &'static str,
    },
    /// Schritt 6: eine Seite ist leer. Seiten ohne Platzhalter (`tl.html`)
    /// fielen sonst durch keine Prüfung — und das TL-Web wäre weg.
    LeereSeite { seite: &'static str },
    /// Schritt 7: Paket gilt für eine andere Kern-Version.
    Kern,
    /// Schritt 8: Folge nicht höher als die aktive.
    Folge { paket: u32, aktiv: u32 },
    /// Schritt 8: Folge wurde zurückgenommen und ist gesperrt.
    Gesperrt { folge: u32 },
}

impl std::fmt::Display for Ablehnung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ablehnung::ZuGross { bytes } => write!(f, "Paket zu groß ({bytes} Bytes)"),
            Ablehnung::Signatur(grund) => write!(f, "Signatur: {grund}"),
            Ablehnung::Entpacken => f.write_str("Paket lässt sich nicht entpacken"),
            Ablehnung::ZuGrossEntpackt => f.write_str("Paket entpackt zu groß"),
            Ablehnung::Inhalt => f.write_str("Paket-Inhalt unlesbar"),
            Ablehnung::Format { format } => write!(f, "unbekanntes Format {format}"),
            Ablehnung::Art => f.write_str("kein Seiten-Paket"),
            Ablehnung::Seitenmenge { fehlend, fremd } => {
                write!(
                    f,
                    "Seitenmenge passt nicht ({fehlend} fehlen, {fremd} fremd)"
                )
            }
            Ablehnung::Platzhalter { seite, platzhalter } => {
                write!(f, "{seite} ohne Platzhalter {platzhalter}")
            }
            Ablehnung::LeereSeite { seite } => write!(f, "{seite} ist leer"),
            Ablehnung::Kern => f.write_str("Paket gilt für eine andere Version"),
            Ablehnung::Folge { paket, aktiv } => {
                write!(f, "Folge {paket} ist nicht neuer als die aktive {aktiv}")
            }
            Ablehnung::Gesperrt { folge } => write!(f, "Folge {folge} wurde zurückgenommen"),
        }
    }
}

/// Wogegen ein Paket geprüft wird.
pub struct Erwartung<'a> {
    /// Version dieser App; das Paket muss für genau sie gebaut sein.
    pub kern: &'a str,
    /// Die Folge muss **größer** sein. Der eingebaute Stand ist 0.
    pub aktive_folge: u32,
    /// Zurückgenommene Folgen.
    pub gesperrt: &'a [u32],
}

/// Ein Paket, das alle Prüfschritte bestanden hat.
#[derive(Debug)]
pub struct Paket {
    pub folge: u32,
    /// Commit, aus dem das Paket gebaut wurde — nur für die Log-Zeile, auf
    /// Hex-Zeichen gekürzt.
    pub commit: String,
    pub stand: Seitenstand,
}

/// Nur die zwei Felder, an denen sich entscheidet, **was** das signierte
/// Ding überhaupt ist. Getrennt gelesen, damit ein fremdes signiertes
/// Dokument als „falsche Art" abgelehnt wird und nicht als Lesefehler.
#[derive(Deserialize)]
struct Kopf {
    format: u64,
    art: String,
}

#[derive(Deserialize)]
struct Rumpf {
    kern: String,
    folge: u32,
    #[serde(default)]
    commit: String,
    seiten: BTreeMap<String, String>,
}

/// Prüft ein Paket in der festen Reihenfolge der Spec und baut daraus einen
/// Seitenstand. Verändert nichts — weder den aktiven Stand noch die Platte.
pub fn pruefen(
    bytes: &[u8],
    signatur: &str,
    schluessel: &Schluessel,
    erwartung: &Erwartung<'_>,
) -> Result<Paket, Ablehnung> {
    // 1. Größe — vor allem anderen, auch vor der Signatur.
    if bytes.len() > MAX_PAKET_BYTES {
        return Err(Ablehnung::ZuGross { bytes: bytes.len() });
    }
    // 2. Signatur über die ROHEN Bytes, bevor irgendetwas entpackt wird.
    signatur::pruefen(schluessel, bytes, signatur).map_err(Ablehnung::Signatur)?;
    // 3. Entpacken mit Deckel.
    let json = entpacken(bytes)?;
    // 4. Format und Art.
    let kopf: Kopf = serde_json::from_slice(&json).map_err(|_| Ablehnung::Inhalt)?;
    if kopf.format != FORMAT {
        return Err(Ablehnung::Format {
            format: kopf.format,
        });
    }
    if kopf.art != ART {
        return Err(Ablehnung::Art);
    }
    let mut rumpf: Rumpf = serde_json::from_slice(&json).map_err(|_| Ablehnung::Inhalt)?;
    drop(json);
    // 5. Seitenmenge exakt wie eingebaut. Namen werden nur verglichen, nie
    //    als Pfad benutzt.
    let fehlend = Seite::ALLE
        .iter()
        .filter(|s| !rumpf.seiten.contains_key(s.datei()))
        .count();
    let fremd = rumpf.seiten.len() + fehlend - Seite::ALLE.len();
    if fehlend > 0 || fremd > 0 {
        return Err(Ablehnung::Seitenmenge { fehlend, fremd });
    }
    // 6. Pflicht-Platzhalter je Seite.
    for seite in Seite::ALLE {
        let text = &rumpf.seiten[seite.datei()];
        if text.trim().is_empty() {
            return Err(Ablehnung::LeereSeite {
                seite: seite.datei(),
            });
        }
        if let Some(platzhalter) = seite
            .pflicht_platzhalter()
            .iter()
            .find(|p| !text.contains(**p))
        {
            return Err(Ablehnung::Platzhalter {
                seite: seite.datei(),
                platzhalter,
            });
        }
    }
    // 7. Genau diese Kern-Version.
    if rumpf.kern != erwartung.kern {
        return Err(Ablehnung::Kern);
    }
    // 8. Folge streng höher und nicht gesperrt.
    if rumpf.folge <= erwartung.aktive_folge {
        return Err(Ablehnung::Folge {
            paket: rumpf.folge,
            aktiv: erwartung.aktive_folge,
        });
    }
    if erwartung.gesperrt.contains(&rumpf.folge) {
        return Err(Ablehnung::Gesperrt { folge: rumpf.folge });
    }

    // Nach Schritt 5 ist jede Seite da; der Ersatzwert ist unerreichbar.
    let stand = Seitenstand::aus_seiten(rumpf.folge, Quelle::Paket, |seite| {
        Cow::Owned(rumpf.seiten.remove(seite.datei()).unwrap_or_default())
    });
    let commit = rumpf
        .commit
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(40)
        .collect();
    Ok(Paket {
        folge: rumpf.folge,
        commit,
        stand,
    })
}

fn entpacken(bytes: &[u8]) -> Result<Vec<u8>, Ablehnung> {
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_ENTPACKT_BYTES as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|_| Ablehnung::Entpacken)?;
    if json.len() > MAX_ENTPACKT_BYTES {
        return Err(Ablehnung::ZuGrossEntpackt);
    }
    Ok(json)
}

/// Was neben den Paket-Dateien gemerkt wird.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Zustand {
    format: u64,
    /// Kern-Version, zu der die abgelegten Pakete gehören.
    kern: String,
    /// Zurückgenommene Folgen dieser Kern-Version.
    #[serde(default)]
    gesperrt: Vec<u32>,
}

/// Ergebnis des Zustand-Lesens. „Fehlt" und „unlesbar" sind verschieden:
/// Ohne Datei gab es nie ein Paket; eine kaputte Datei heißt, dass die
/// Sperrliste verloren ist.
enum ZustandLesung {
    Fehlt,
    Unlesbar,
    Da(Zustand),
}

/// Warum ein Paket nicht abgelegt wurde.
#[derive(Debug)]
pub enum AblageFehler {
    /// Das Paket hat die Prüfung nicht bestanden.
    Abgelehnt(Ablehnung),
    /// Schreiben oder Umbenennen scheiterte (z. B. Platte voll).
    Platte(std::io::Error),
}

impl std::fmt::Display for AblageFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AblageFehler::Abgelehnt(grund) => write!(f, "abgelehnt: {grund}"),
            AblageFehler::Platte(e) => write!(f, "Ablegen gescheitert: {e}"),
        }
    }
}

impl From<std::io::Error> for AblageFehler {
    fn from(e: std::io::Error) -> Self {
        AblageFehler::Platte(e)
    }
}

/// Die Paket-Dateien im App-Datenverzeichnis (`seiten/`).
///
/// Ein Paket liegt als **eine** Datei: erste Zeile die Signatur, danach die
/// rohen Paket-Bytes. So wechselt beides mit einem einzigen Umbenennen —
/// nie liegt ein neues Paket neben einer alten Signatur.
///
/// **Vertrauen:** Das Verzeichnis gehört demselben Windows-Benutzer wie die
/// Installation. Wer darin schreiben kann, kann auch die Programmdatei
/// ersetzen; dagegen schützt die Ablage nicht. Sie schützt davor, dass etwas
/// **ohne gültige Signatur** aktiv wird — deshalb wird bei jedem Start neu
/// geprüft und die Sperrliste nur aus einem lesbaren Zustand geglaubt.
pub struct Ablage {
    dir: PathBuf,
}

impl Ablage {
    pub fn neu(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Liest das aktive Paket und prüft es **vollständig neu** — die Platte
    /// ist kein Vertrauensanker. Jeder Fehler führt zum eingebauten Stand
    /// (`None`) und einer Log-Zeile, nie zu einem Abbruch.
    pub fn beim_start_laden(&self, kern: &str, schluessel: &Schluessel) -> Option<Paket> {
        if !self.dir.is_dir() {
            return None;
        }
        let zustand = self.zustand_lesen();
        if let ZustandLesung::Da(z) = &zustand {
            if z.kern != kern {
                // Nach einem Kern-Update gelten die Seiten der neuen
                // Programmdatei; ein Paket der alten Version würde sie
                // zurückdrehen.
                tracing::info!(
                    "Seiten-Paket gehört zu einer anderen Version – verworfen, es gilt der eingebaute Stand"
                );
                self.alles_verwerfen();
                return None;
            }
        }
        // Nur eine gewöhnliche Datei öffnen: Eine Pipe oder ein Gerät unter
        // diesem Namen hielte den App-Start an.
        match std::fs::symlink_metadata(self.dir.join(DATEI_AKTIV)) {
            Ok(m) if m.is_file() => {}
            Ok(_) => {
                tracing::warn!(
                    "Seiten-Paket ist keine gewöhnliche Datei – es gilt der eingebaute Stand"
                );
                return None;
            }
            Err(_) => return None,
        }
        let gesperrt = match zustand {
            ZustandLesung::Da(z) => z.gesperrt,
            ZustandLesung::Fehlt | ZustandLesung::Unlesbar => {
                // `ablegen` schreibt den Zustand VOR dem Paket. Ein Paket
                // ohne lesbaren Zustand heißt: Die Sperrliste ist verloren,
                // und eine zurückgenommene Folge käme sonst still zurück.
                tracing::warn!(
                    "Seiten-Paket ohne lesbaren Zustand – beiseitegelegt, es gilt der eingebaute Stand"
                );
                self.beiseitelegen();
                return None;
            }
        };
        let (signatur, bytes) = match self.paket_lesen(DATEI_AKTIV) {
            Ok(Some(paar)) => paar,
            Ok(None) => return None,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                tracing::warn!(
                    "Seiten-Paket unlesbar ({e}) – beiseitegelegt, es gilt der eingebaute Stand"
                );
                self.beiseitelegen();
                return None;
            }
            Err(e) => {
                // Vorübergehend (Virenscanner hält die Datei, Zugriff
                // verweigert): Die Datei bleibt liegen und wird beim
                // nächsten Start wieder versucht.
                tracing::warn!(
                    "Seiten-Paket gerade nicht lesbar ({e}) – für diesen Lauf gilt der eingebaute Stand"
                );
                return None;
            }
        };
        let erwartung = Erwartung {
            kern,
            aktive_folge: 0,
            gesperrt: &gesperrt,
        };
        match pruefen(&bytes, &signatur, schluessel, &erwartung) {
            Ok(paket) => Some(paket),
            Err(grund) => {
                tracing::warn!(
                    "Seiten-Paket auf der Platte abgelehnt ({grund}) – beiseitegelegt, es gilt der eingebaute Stand"
                );
                self.beiseitelegen();
                None
            }
        }
    }

    /// Prüft ein Paket und legt es als aktiv ab; das bisher aktive bleibt
    /// als „vorher" liegen (den Rückweg dorthin bringt ein späterer
    /// Schritt). Geprüft wird **hier**, nicht beim Aufrufer — so kann nichts
    /// Ungeprüftes auf die Platte kommen.
    ///
    /// Reihenfolge ohne Lücke: Das neue Paket wird vollständig in eine
    /// Nebendatei geschrieben, das aktive nach „vorher" **kopiert**, und erst
    /// dann ersetzt ein einziges Umbenennen das aktive. Scheitert ein
    /// Schritt davor (Platte voll), bleibt das aktive Paket unberührt; ein
    /// Absturz mittendrin hinterlässt nie ein Verzeichnis ohne aktives
    /// Paket.
    pub fn ablegen(
        &self,
        kern: &str,
        aktive_folge: u32,
        schluessel: &Schluessel,
        bytes: &[u8],
        signatur: &str,
    ) -> Result<Paket, AblageFehler> {
        let signatur = signatur.trim();
        let gesperrt = match self.zustand_lesen() {
            ZustandLesung::Da(z) if z.kern == kern => z.gesperrt,
            ZustandLesung::Da(_) => {
                // Reste einer anderen Kern-Version: weg damit, sonst läge
                // unter „vorher" ein Paket, das hier nie gelten darf.
                self.alles_verwerfen();
                Vec::new()
            }
            ZustandLesung::Fehlt => Vec::new(),
            ZustandLesung::Unlesbar => {
                // Bewusst weiter statt für immer zu sperren: Die Sperrliste
                // ist verloren, aber ein neuer Fix soll ankommen können.
                tracing::warn!(
                    "Zustand der Seiten-Pakete unlesbar – wird neu angelegt, frühere Sperren sind verloren"
                );
                Vec::new()
            }
        };
        let erwartung = Erwartung {
            kern,
            aktive_folge,
            gesperrt: &gesperrt,
        };
        let paket =
            pruefen(bytes, signatur, schluessel, &erwartung).map_err(AblageFehler::Abgelehnt)?;

        std::fs::create_dir_all(&self.dir)?;
        // Zustand zuerst: Ohne ihn würde der nächste Start das Paket
        // beiseitelegen. Sperren bleiben erhalten.
        self.zustand_schreiben(kern, &gesperrt)?;

        self.nebendatei_schreiben(DATEI_NEU, &[signatur.as_bytes(), b"\n", bytes])?;
        let aktiv = self.dir.join(DATEI_AKTIV);
        let ergebnis = (|| {
            if aktiv.is_file() {
                let vorher_neu = self.dir.join(DATEI_VORHER_NEU);
                self.entfernen(DATEI_VORHER_NEU);
                std::fs::copy(&aktiv, &vorher_neu)?;
                std::fs::rename(&vorher_neu, self.dir.join(DATEI_VORHER))?;
            } else {
                // Vorher galt der eingebaute Stand — ein liegengebliebenes
                // „vorher" wäre ein falscher Rückweg.
                self.entfernen(DATEI_VORHER);
            }
            // Der eine Schritt, der zählt: ersetzt das aktive Paket.
            std::fs::rename(self.dir.join(DATEI_NEU), &aktiv)
        })();
        if let Err(e) = ergebnis {
            self.entfernen(DATEI_NEU);
            self.entfernen(DATEI_VORHER_NEU);
            return Err(AblageFehler::Platte(e));
        }
        Ok(paket)
    }

    /// Nimmt das aktive Paket zurück: Seine Folge wird **gesperrt** (sie wird
    /// nie wieder angenommen), und das vorige Paket wird wieder aktiv.
    /// `Ok(None)` heißt: Es gilt wieder der eingebaute Stand — weil es kein
    /// voriges Paket gab oder es die Prüfung nicht mehr besteht.
    ///
    /// Die Sperre wird **zuerst** geschrieben. Scheitert das, bleibt alles
    /// wie es war; bricht es danach ab, lädt der nächste Start das gesperrte
    /// Paket nicht mehr — der Rückweg gilt also auch dann.
    pub fn zurueck(
        &self,
        kern: &str,
        aktive_folge: u32,
        schluessel: &Schluessel,
    ) -> Result<Option<Paket>, AblageFehler> {
        if aktive_folge == 0 {
            return Err(AblageFehler::Platte(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "es ist kein Seiten-Paket aktiv",
            )));
        }
        let mut gesperrt = self.gesperrt(kern);
        if !gesperrt.contains(&aktive_folge) {
            gesperrt.push(aktive_folge);
        }
        std::fs::create_dir_all(&self.dir)?;
        self.zustand_schreiben(kern, &gesperrt)?;

        let voriges = match self.paket_lesen(DATEI_VORHER) {
            Ok(Some((signatur, bytes))) => {
                let erwartung = Erwartung {
                    kern,
                    aktive_folge: 0,
                    gesperrt: &gesperrt,
                };
                match pruefen(&bytes, &signatur, schluessel, &erwartung) {
                    Ok(paket) => Some(paket),
                    Err(grund) => {
                        tracing::warn!(
                            "Voriges Seiten-Paket abgelehnt ({grund}) – zurück zum eingebauten Stand"
                        );
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    "Voriges Seiten-Paket unlesbar ({e}) – zurück zum eingebauten Stand"
                );
                None
            }
        };
        // Ein Umbenennen ersetzt das zurückgenommene Paket. Scheitert es,
        // geht es zum eingebauten Stand — nie bleibt das gesperrte Paket
        // „aktiv" liegen, während der Speicher schon etwas anderes zeigt.
        let voriges = match voriges {
            Some(paket) => {
                match std::fs::rename(self.dir.join(DATEI_VORHER), self.dir.join(DATEI_AKTIV)) {
                    Ok(()) => Some(paket),
                    Err(e) => {
                        tracing::warn!(
                        "Voriges Seiten-Paket ließ sich nicht aktivieren ({e}) – zurück zum eingebauten Stand"
                    );
                        None
                    }
                }
            }
            None => None,
        };
        if voriges.is_none() {
            self.entfernen(DATEI_AKTIV);
            self.entfernen(DATEI_VORHER);
        }
        Ok(voriges)
    }

    /// Die zurückgenommenen Folgen dieser Kern-Version. Leer, wenn der
    /// Zustand fehlt, unlesbar ist oder zu einer anderen Version gehört.
    pub fn gesperrt(&self, kern: &str) -> Vec<u32> {
        match self.zustand_lesen() {
            ZustandLesung::Da(z) if z.kern == kern => z.gesperrt,
            _ => Vec::new(),
        }
    }

    fn zustand_schreiben(&self, kern: &str, gesperrt: &[u32]) -> std::io::Result<()> {
        self.nebendatei_schreiben(
            DATEI_ZUSTAND_NEU,
            &[&serde_json::to_vec_pretty(&Zustand {
                format: FORMAT,
                kern: kern.to_string(),
                gesperrt: gesperrt.to_vec(),
            })
            .map_err(std::io::Error::other)?],
        )?;
        std::fs::rename(
            self.dir.join(DATEI_ZUSTAND_NEU),
            self.dir.join(DATEI_ZUSTAND),
        )
    }

    /// Schreibt eine Nebendatei vollständig und bis auf die Platte. Sie wird
    /// **neu angelegt** (nie eine vorhandene geöffnet): Ein vorab
    /// hingelegter Verweis unter diesem Namen wird so nicht verfolgt.
    fn nebendatei_schreiben(&self, name: &str, teile: &[&[u8]]) -> std::io::Result<()> {
        let pfad = self.dir.join(name);
        match std::fs::remove_file(&pfad) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let geschrieben = (|| {
            let mut datei = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pfad)?;
            for teil in teile {
                datei.write_all(teil)?;
            }
            datei.sync_all()
        })();
        if geschrieben.is_err() {
            let _ = std::fs::remove_file(&pfad);
        }
        geschrieben
    }

    /// Liest eine Paket-Datei: Signatur-Zeile und rohe Bytes. `Ok(None)`,
    /// wenn es die Datei nicht gibt. Ein Formfehler der Datei ist
    /// `InvalidData`; alles andere ist ein Lesefehler des Systems.
    fn paket_lesen(&self, name: &str) -> std::io::Result<Option<(String, Vec<u8>)>> {
        let pfad = self.dir.join(name);
        let datei = match std::fs::File::open(&pfad) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        // Deckel schon beim Lesen: Eine riesige Datei soll nicht erst in den
        // Speicher, um dann an Schritt 1 zu scheitern. 4 KB Luft für die
        // Signatur-Zeile.
        let deckel = MAX_PAKET_BYTES as u64 + 4096;
        let mut roh = Vec::new();
        datei.take(deckel + 1).read_to_end(&mut roh)?;
        let unlesbar = |text| std::io::Error::new(std::io::ErrorKind::InvalidData, text);
        if roh.len() as u64 > deckel {
            return Err(unlesbar("Datei zu groß"));
        }
        let trenner = roh
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| unlesbar("Signatur-Zeile fehlt"))?;
        let bytes = roh.split_off(trenner + 1);
        roh.pop();
        let signatur = String::from_utf8(roh).map_err(|_| unlesbar("Signatur-Zeile kein Text"))?;
        Ok(Some((signatur, bytes)))
    }

    fn zustand_lesen(&self) -> ZustandLesung {
        let roh = match std::fs::read(self.dir.join(DATEI_ZUSTAND)) {
            Ok(roh) => roh,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ZustandLesung::Fehlt,
            Err(_) => return ZustandLesung::Unlesbar,
        };
        match serde_json::from_slice::<Zustand>(&roh) {
            Ok(z) if z.format == FORMAT => ZustandLesung::Da(z),
            _ => ZustandLesung::Unlesbar,
        }
    }

    /// Nimmt ein abgelehntes aktives Paket aus dem Weg, ohne die Spur zu
    /// löschen. Gelingt das Umbenennen nicht, bleibt es liegen und wird beim
    /// nächsten Start erneut abgelehnt.
    fn beiseitelegen(&self) {
        if let Err(e) = std::fs::rename(self.dir.join(DATEI_AKTIV), self.dir.join(DATEI_ABGELEHNT))
        {
            tracing::warn!("Seiten-Paket ließ sich nicht beiseitelegen ({e})");
        }
    }

    fn alles_verwerfen(&self) {
        for name in [
            DATEI_AKTIV,
            DATEI_VORHER,
            DATEI_NEU,
            DATEI_VORHER_NEU,
            DATEI_ABGELEHNT,
            DATEI_ZUSTAND,
            DATEI_ZUSTAND_NEU,
        ] {
            self.entfernen(name);
        }
    }

    fn entfernen(&self, name: &str) {
        let pfad = self.dir.join(name);
        if let Err(e) = std::fs::remove_file(&pfad) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Seiten-Paket: {name} ließ sich nicht entfernen ({e})");
            }
        }
    }
}

/// Lädt beim App-Start ein abgelegtes Paket und macht es zum aktiven
/// Seitenstand. Ohne gültiges Paket bleibt der eingebaute Stand.
///
/// Muss laufen, **bevor** der Tablet-Server Seiten ausliefert — sonst
/// bekämen die ersten Tablets nach einem Neustart den eingebauten Stand und
/// lüden gleich darauf noch einmal.
pub fn beim_start_anwenden(dir: &Path) {
    let schluessel = match signatur::updater_schluessel() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Seiten-Pakete aus: {e}");
            return;
        }
    };
    let kern = env!("CARGO_PKG_VERSION");
    if let Some(paket) = Ablage::neu(dir).beim_start_laden(kern, schluessel) {
        tracing::info!(
            "Seiten-Paket Folge {} aktiv (Commit {})",
            paket.folge,
            paket.commit
        );
        assets::tauschen(Arc::new(paket.stand));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatur::testhilfe::Testschluessel;

    const KERN: &str = "1.2.3";

    /// Bauplan eines Test-Pakets; `bauen` liefert die gzip-Bytes.
    struct Bauplan {
        format: u64,
        art: &'static str,
        kern: &'static str,
        folge: u32,
        seiten: BTreeMap<String, String>,
    }

    impl Bauplan {
        fn neu(folge: u32) -> Self {
            let eingebaut = assets::aktiv();
            Self {
                format: 1,
                art: "seiten",
                kern: KERN,
                folge,
                seiten: Seite::ALLE
                    .iter()
                    .map(|s| {
                        (
                            s.datei().to_string(),
                            format!("<!-- folge {folge} -->{}", eingebaut.seite(*s)),
                        )
                    })
                    .collect(),
            }
        }

        fn bauen(&self) -> Vec<u8> {
            let json = serde_json::json!({
                "format": self.format,
                "art": self.art,
                "kern": self.kern,
                "folge": self.folge,
                "commit": "0123abcd",
                "gebaut": "2026-11-16T14:03:00Z",
                "seiten": self.seiten,
            });
            gzip(&serde_json::to_vec(&json).unwrap())
        }
    }

    fn gzip(roh: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(roh).unwrap();
        enc.finish().unwrap()
    }

    fn erwartung() -> Erwartung<'static> {
        Erwartung {
            kern: KERN,
            aktive_folge: 0,
            gesperrt: &[],
        }
    }

    /// Prüft einen Bauplan, signiert mit `schluessel`, gegen `erw`.
    fn pruefe(plan: &Bauplan, erw: &Erwartung<'_>) -> Result<Paket, Ablehnung> {
        let schluessel = Testschluessel::neu(1);
        let bytes = plan.bauen();
        let sig = schluessel.signieren(&bytes);
        pruefen(&bytes, &sig, &schluessel.oeffentlich(), erw)
    }

    #[test]
    fn gueltiges_paket_wird_angenommen() {
        let paket = pruefe(&Bauplan::neu(2), &erwartung()).expect("gültig");
        assert_eq!(paket.folge, 2);
        assert_eq!(paket.commit, "0123abcd");
        assert_eq!(paket.stand.folge(), 2);
        assert_eq!(paket.stand.quelle(), Quelle::Paket);
        for s in Seite::ALLE {
            assert!(
                paket.stand.seite(*s).starts_with("<!-- folge 2 -->"),
                "{s:?}"
            );
        }
        assert_ne!(paket.stand.marke(), assets::aktiv().marke());
    }

    #[test]
    fn unsigniertes_paket_abgelehnt() {
        let schluessel = Testschluessel::neu(1);
        let bytes = Bauplan::neu(1).bauen();
        for sig in ["", "aGFsbG8="] {
            assert_eq!(
                pruefen(&bytes, sig, &schluessel.oeffentlich(), &erwartung()).unwrap_err(),
                Ablehnung::Signatur(SignaturFehler::SignaturUnlesbar)
            );
        }
    }

    #[test]
    fn veraenderte_bytes_abgelehnt() {
        let schluessel = Testschluessel::neu(1);
        let mut bytes = Bauplan::neu(1).bauen();
        let sig = schluessel.signieren(&bytes);
        let mitte = bytes.len() / 2;
        bytes[mitte] ^= 0x01;
        assert_eq!(
            pruefen(&bytes, &sig, &schluessel.oeffentlich(), &erwartung()).unwrap_err(),
            Ablehnung::Signatur(SignaturFehler::Ungueltig)
        );
    }

    #[test]
    fn fremder_schluessel_abgelehnt() {
        let eigener = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let bytes = Bauplan::neu(1).bauen();
        let sig = fremder.signieren(&bytes);
        assert_eq!(
            pruefen(&bytes, &sig, &eigener.oeffentlich(), &erwartung()).unwrap_err(),
            Ablehnung::Signatur(SignaturFehler::Ungueltig)
        );
    }

    /// Die Signatur wird VOR dem Entpacken geprüft: Ein unsignierter
    /// Müllhaufen darf nie bis zum gzip-Leser kommen.
    #[test]
    fn signatur_wird_vor_dem_entpacken_geprueft() {
        let schluessel = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let muell = b"kein gzip";
        assert_eq!(
            pruefen(
                muell,
                &fremder.signieren(muell),
                &schluessel.oeffentlich(),
                &erwartung()
            )
            .unwrap_err(),
            Ablehnung::Signatur(SignaturFehler::Ungueltig)
        );
        // Richtig signierter Müll scheitert erst am Entpacken.
        assert_eq!(
            pruefen(
                muell,
                &schluessel.signieren(muell),
                &schluessel.oeffentlich(),
                &erwartung()
            )
            .unwrap_err(),
            Ablehnung::Entpacken
        );
    }

    #[test]
    fn andere_kern_version_abgelehnt() {
        let mut plan = Bauplan::neu(1);
        plan.kern = "1.2.4";
        assert_eq!(pruefe(&plan, &erwartung()).unwrap_err(), Ablehnung::Kern);
        // Auch eine ÄLTERE Kern-Version: exakte Bindung, kein „ab Version".
        plan.kern = "1.2.2";
        assert_eq!(pruefe(&plan, &erwartung()).unwrap_err(), Ablehnung::Kern);
    }

    #[test]
    fn aeltere_folge_abgelehnt() {
        let erw = Erwartung {
            aktive_folge: 5,
            ..erwartung()
        };
        assert_eq!(
            pruefe(&Bauplan::neu(4), &erw).unwrap_err(),
            Ablehnung::Folge { paket: 4, aktiv: 5 }
        );
    }

    #[test]
    fn gleiche_folge_abgelehnt() {
        let erw = Erwartung {
            aktive_folge: 5,
            ..erwartung()
        };
        assert_eq!(
            pruefe(&Bauplan::neu(5), &erw).unwrap_err(),
            Ablehnung::Folge { paket: 5, aktiv: 5 }
        );
        // Folge 0 ist der eingebaute Stand und nie ein Paket.
        assert_eq!(
            pruefe(&Bauplan::neu(0), &erwartung()).unwrap_err(),
            Ablehnung::Folge { paket: 0, aktiv: 0 }
        );
    }

    #[test]
    fn gesperrte_folge_abgelehnt() {
        let erw = Erwartung {
            gesperrt: &[3],
            ..erwartung()
        };
        assert_eq!(
            pruefe(&Bauplan::neu(3), &erw).unwrap_err(),
            Ablehnung::Gesperrt { folge: 3 }
        );
        // Eine höhere Folge ist wieder willkommen.
        assert!(pruefe(&Bauplan::neu(4), &erw).is_ok());
    }

    #[test]
    fn fehlende_seite_abgelehnt() {
        let mut plan = Bauplan::neu(1);
        plan.seiten.remove("tl.html");
        assert_eq!(
            pruefe(&plan, &erwartung()).unwrap_err(),
            Ablehnung::Seitenmenge {
                fehlend: 1,
                fremd: 0
            }
        );
    }

    #[test]
    fn fremde_seite_abgelehnt() {
        let mut plan = Bauplan::neu(1);
        plan.seiten.insert("../config.json".into(), "boese".into());
        assert_eq!(
            pruefe(&plan, &erwartung()).unwrap_err(),
            Ablehnung::Seitenmenge {
                fehlend: 0,
                fremd: 1
            }
        );
        // Umbenannt: eine fehlt, eine ist fremd.
        let mut plan = Bauplan::neu(1);
        let text = plan.seiten.remove("tl.html").unwrap();
        plan.seiten.insert("TL.html".into(), text);
        assert_eq!(
            pruefe(&plan, &erwartung()).unwrap_err(),
            Ablehnung::Seitenmenge {
                fehlend: 1,
                fremd: 1
            }
        );
    }

    #[test]
    fn fehlender_platzhalter_abgelehnt() {
        let mut plan = Bauplan::neu(1);
        let ohne = plan.seiten["tablet.html"].replace("__SEITEN_MARKE__", "");
        plan.seiten.insert("tablet.html".into(), ohne);
        assert_eq!(
            pruefe(&plan, &erwartung()).unwrap_err(),
            Ablehnung::Platzhalter {
                seite: "tablet.html",
                platzhalter: "__SEITEN_MARKE__"
            }
        );
    }

    #[test]
    fn falsche_art_abgelehnt() {
        let mut plan = Bauplan::neu(1);
        plan.art = "kern";
        assert_eq!(pruefe(&plan, &erwartung()).unwrap_err(), Ablehnung::Art);

        // Ein signierter Kern-Kopf (ganz andere Felder) ist „falsche Art",
        // kein Lesefehler — und nie ein Seiten-Paket.
        let schluessel = Testschluessel::neu(1);
        let kopf = gzip(br#"{"format":1,"art":"kern","version":"1.2.3","exe_sha256":"00"}"#);
        assert_eq!(
            pruefen(
                &kopf,
                &schluessel.signieren(&kopf),
                &schluessel.oeffentlich(),
                &erwartung()
            )
            .unwrap_err(),
            Ablehnung::Art
        );
    }

    #[test]
    fn unbekanntes_format_abgelehnt() {
        let mut plan = Bauplan::neu(1);
        plan.format = 2;
        assert_eq!(
            pruefe(&plan, &erwartung()).unwrap_err(),
            Ablehnung::Format { format: 2 }
        );
    }

    #[test]
    fn gzip_bombe_abgelehnt() {
        let schluessel = Testschluessel::neu(1);
        // Wenige KB auf der Leitung, entpackt über dem Deckel.
        let bombe = gzip(&vec![b' '; MAX_ENTPACKT_BYTES + 1]);
        assert!(bombe.len() < MAX_PAKET_BYTES);
        assert_eq!(
            pruefen(
                &bombe,
                &schluessel.signieren(&bombe),
                &schluessel.oeffentlich(),
                &erwartung()
            )
            .unwrap_err(),
            Ablehnung::ZuGrossEntpackt
        );
    }

    #[test]
    fn zu_grosse_datei_abgelehnt_vor_der_signatur() {
        let schluessel = Testschluessel::neu(1);
        let riesig = vec![0u8; MAX_PAKET_BYTES + 1];
        // Bewusst mit unbrauchbarer Signatur: Schritt 1 greift zuerst.
        assert_eq!(
            pruefen(&riesig, "", &schluessel.oeffentlich(), &erwartung()).unwrap_err(),
            Ablehnung::ZuGross {
                bytes: MAX_PAKET_BYTES + 1
            }
        );
    }

    /// Der Bau-Weg der CI gegen die Prüfung der App: `scripts/
    /// build-seiten-paket.mjs` baut aus den echten Seiten ein Paket, und
    /// `pruefen` muss es annehmen. Driften Skript und App auseinander (Feld
    /// umbenannt, Seite vergessen), wäre sonst jedes veröffentlichte Paket
    /// auf jedem Turnier-PC wertlos — und das fiele erst im Turnier auf.
    #[test]
    fn paket_aus_dem_bauskript_besteht_die_app_pruefung() {
        let dir = tempfile::tempdir().unwrap();
        let datei = dir.path().join("seiten.btsp");
        let wurzel = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let lauf = std::process::Command::new("node")
            .arg(wurzel.join("scripts").join("build-seiten-paket.mjs"))
            .arg("nutzlast")
            .arg("--aus-dateien")
            .arg(&wurzel)
            .args(["--kern", KERN, "--folge", "7"])
            .args(["--commit", "0123456789abcdef0123456789abcdef01234567"])
            .arg("--out")
            .arg(&datei)
            .output()
            .expect("node muss für diesen Test vorhanden sein");
        assert!(
            lauf.status.success(),
            "Bau-Skript gescheitert: {}",
            String::from_utf8_lossy(&lauf.stderr)
        );

        let bytes = std::fs::read(&datei).unwrap();
        let schluessel = Testschluessel::neu(1);
        let paket = pruefen(
            &bytes,
            &schluessel.signieren(&bytes),
            &schluessel.oeffentlich(),
            &erwartung(),
        )
        .expect("ein Paket aus dem Bau-Skript muss die Prüfung bestehen");
        assert_eq!(paket.folge, 7);
        assert_eq!(paket.commit, "0123456789abcdef0123456789abcdef01234567");
        // Unverändert gebaut: derselbe Inhalt wie eingebaut, also auch
        // dieselbe Marke — die Tablets lüden nach so einem Paket nicht neu.
        let eingebaut = assets::aktiv();
        for s in Seite::ALLE {
            assert_eq!(paket.stand.seite(*s), eingebaut.seite(*s), "{s:?}");
        }
        assert_eq!(paket.stand.marke(), eingebaut.marke());
    }

    #[test]
    fn leere_seite_abgelehnt() {
        // `tl.html` hat keinen Platzhalter — ohne diese Prüfung ginge eine
        // leere Seite durch und das TL-Web wäre weg.
        for leer in ["", "  \n\t"] {
            let mut plan = Bauplan::neu(1);
            plan.seiten.insert("tl.html".into(), leer.into());
            assert_eq!(
                pruefe(&plan, &erwartung()).unwrap_err(),
                Ablehnung::LeereSeite { seite: "tl.html" }
            );
        }
    }

    /// Legt ein gültiges Paket der Folge ab und gibt Bytes und Signatur
    /// zurück.
    fn lege_ab(ablage: &Ablage, schluessel: &Testschluessel, folge: u32) -> (Vec<u8>, String) {
        let bytes = Bauplan::neu(folge).bauen();
        let sig = schluessel.signieren(&bytes);
        ablage
            .ablegen(KERN, 0, &schluessel.oeffentlich(), &bytes, &sig)
            .expect("ablegen");
        (bytes, sig)
    }

    fn lade(ablage: &Ablage, schluessel: &Testschluessel) -> Option<u32> {
        ablage
            .beim_start_laden(KERN, &schluessel.oeffentlich())
            .map(|p| p.folge)
    }

    #[test]
    fn abgelegtes_paket_wird_beim_start_geladen() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path().join("seiten"));
        assert_eq!(lade(&ablage, &schluessel), None);

        lege_ab(&ablage, &schluessel, 2);
        assert_eq!(lade(&ablage, &schluessel), Some(2));
        // Keine Reste der Nebendateien, kein „vorher" beim ersten Paket.
        for name in [DATEI_NEU, DATEI_VORHER, DATEI_VORHER_NEU, DATEI_ZUSTAND_NEU] {
            assert!(!dir.path().join("seiten").join(name).exists(), "{name}");
        }
    }

    #[test]
    fn zweites_paket_macht_das_erste_zum_vorigen() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        let (erstes, erste_sig) = lege_ab(&ablage, &schluessel, 1);
        lege_ab(&ablage, &schluessel, 2);

        assert_eq!(lade(&ablage, &schluessel), Some(2));
        let (sig, bytes) = ablage.paket_lesen(DATEI_VORHER).unwrap().expect("vorher");
        assert_eq!(bytes, erstes);
        assert_eq!(sig, erste_sig);
        assert!(!dir.path().join(DATEI_VORHER_NEU).exists());
    }

    /// `ablegen` prüft selbst: Nichts Ungeprüftes kommt auf die Platte, und
    /// ein abgelehntes Paket lässt den Bestand unberührt.
    #[test]
    fn ablegen_lehnt_ungueltiges_ab_und_schreibt_nichts() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let ablage = Ablage::neu(dir.path().join("seiten"));
        let bytes = Bauplan::neu(1).bauen();

        let fehler = ablage
            .ablegen(
                KERN,
                0,
                &schluessel.oeffentlich(),
                &bytes,
                &fremder.signieren(&bytes),
            )
            .unwrap_err();
        assert!(matches!(
            fehler,
            AblageFehler::Abgelehnt(Ablehnung::Signatur(SignaturFehler::Ungueltig))
        ));
        assert!(!dir.path().join("seiten").exists(), "nichts angelegt");

        // Nicht neuer als die aktive Folge: abgelehnt, Bestand bleibt.
        let (erstes, _) = lege_ab(&ablage, &schluessel, 3);
        let alt = Bauplan::neu(2).bauen();
        let fehler = ablage
            .ablegen(
                KERN,
                3,
                &schluessel.oeffentlich(),
                &alt,
                &schluessel.signieren(&alt),
            )
            .unwrap_err();
        assert!(matches!(
            fehler,
            AblageFehler::Abgelehnt(Ablehnung::Folge { paket: 2, aktiv: 3 })
        ));
        assert_eq!(ablage.paket_lesen(DATEI_AKTIV).unwrap().unwrap().1, erstes);
    }

    #[test]
    fn kaputtes_paket_auf_platte_faellt_auf_eingebaut_zurueck() {
        let schluessel = Testschluessel::neu(1);
        type Verderber = fn(&Path);
        let faelle: [(&str, Verderber); 4] = [
            ("abgeschnitten", |p| {
                let roh = std::fs::read(p).unwrap();
                std::fs::write(p, &roh[..roh.len() / 2]).unwrap();
            }),
            ("ein Byte gekippt", |p| {
                let mut roh = std::fs::read(p).unwrap();
                let n = roh.len() - 10;
                roh[n] ^= 0x40;
                std::fs::write(p, roh).unwrap();
            }),
            ("leer", |p| std::fs::write(p, b"").unwrap()),
            ("ohne Signatur-Zeile", |p| {
                std::fs::write(p, b"keine zeile").unwrap()
            }),
        ];
        for (name, verderben) in faelle {
            let dir = tempfile::tempdir().unwrap();
            let ablage = Ablage::neu(dir.path());
            lege_ab(&ablage, &schluessel, 1);
            let aktiv = dir.path().join(DATEI_AKTIV);
            verderben(&aktiv);
            let kaputt = std::fs::read(&aktiv).unwrap();

            assert_eq!(lade(&ablage, &schluessel), None, "{name}");
            assert!(!aktiv.exists(), "{name}: nicht mehr aktiv");
            assert_eq!(
                std::fs::read(dir.path().join(DATEI_ABGELEHNT)).unwrap(),
                kaputt,
                "{name}: die Spur bleibt für die Fehlersuche liegen"
            );
            // Auch beim zweiten Start: eingebaut, kein Absturz.
            assert_eq!(lade(&ablage, &schluessel), None, "{name}");
        }
    }

    /// Ein Paket, das mit einem fremden Schlüssel signiert auf der Platte
    /// liegt (jemand hat die Datei ausgetauscht), wird nie aktiv.
    #[test]
    fn fremd_signiertes_paket_auf_platte_wird_nicht_geladen() {
        let dir = tempfile::tempdir().unwrap();
        let eigener = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &fremder, 1);
        assert_eq!(lade(&ablage, &eigener), None);
    }

    /// Ein Lesefehler des Systems (hier: Zugriff verweigert, im Feld ein
    /// Virenscanner, der die Datei kurz hält) darf ein gutes Paket nicht
    /// kosten — es bleibt liegen und gilt beim nächsten Start wieder.
    #[cfg(unix)]
    #[test]
    fn voruebergehender_lesefehler_laesst_das_paket_liegen() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        let aktiv = dir.path().join(DATEI_AKTIV);

        std::fs::set_permissions(&aktiv, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::File::open(&aktiv).is_ok() {
            // Als root greift die Sperre nicht; dann sagt der Test nichts.
            return;
        }
        assert_eq!(lade(&ablage, &schluessel), None);
        assert!(aktiv.exists(), "die Datei bleibt liegen");
        assert!(!dir.path().join(DATEI_ABGELEHNT).exists());

        std::fs::set_permissions(&aktiv, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(lade(&ablage, &schluessel), Some(1));
    }

    /// Unter dem Namen des aktiven Pakets liegt keine gewöhnliche Datei:
    /// nicht öffnen (eine Pipe hielte den Start an), nichts anfassen.
    #[test]
    fn keine_gewoehnliche_datei_wird_nicht_geoeffnet() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        let aktiv = dir.path().join(DATEI_AKTIV);
        std::fs::remove_file(&aktiv).unwrap();
        std::fs::create_dir(&aktiv).unwrap();
        assert_eq!(lade(&ablage, &schluessel), None);
        assert!(aktiv.is_dir());
    }

    #[test]
    fn paket_fuer_alte_kern_version_wird_nach_update_verworfen() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        lege_ab(&ablage, &schluessel, 2);

        // Die App läuft jetzt als neuere Version.
        assert!(ablage
            .beim_start_laden("1.2.4", &schluessel.oeffentlich())
            .is_none());
        for name in [DATEI_AKTIV, DATEI_VORHER, DATEI_ZUSTAND] {
            assert!(!dir.path().join(name).exists(), "{name} muss weg sein");
        }
    }

    /// Liegen noch Pakete einer anderen Kern-Version da, wenn ein neues
    /// abgelegt wird, darf keines davon zum „vorher" werden.
    #[test]
    fn ablegen_nach_kernwechsel_uebernimmt_kein_altes_vorher() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        lege_ab(&ablage, &schluessel, 2);

        let mut plan = Bauplan::neu(1);
        plan.kern = "1.2.4";
        let bytes = plan.bauen();
        ablage
            .ablegen(
                "1.2.4",
                0,
                &schluessel.oeffentlich(),
                &bytes,
                &schluessel.signieren(&bytes),
            )
            .expect("ablegen für die neue Version");
        assert!(!dir.path().join(DATEI_VORHER).exists());
        assert_eq!(
            ablage
                .beim_start_laden("1.2.4", &schluessel.oeffentlich())
                .map(|p| p.folge),
            Some(1)
        );
    }

    fn sperre(ablage: &Ablage, folgen: &[u32]) {
        let json = serde_json::to_vec(&Zustand {
            format: 1,
            kern: KERN.into(),
            gesperrt: folgen.to_vec(),
        })
        .unwrap();
        std::fs::write(ablage.dir.join(DATEI_ZUSTAND), json).unwrap();
    }

    fn gesperrt(ablage: &Ablage) -> Vec<u32> {
        match ablage.zustand_lesen() {
            ZustandLesung::Da(z) => z.gesperrt,
            _ => panic!("Zustand muss lesbar sein"),
        }
    }

    #[test]
    fn gesperrte_folge_auf_platte_wird_nicht_geladen() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 3);
        sperre(&ablage, &[3]);
        assert_eq!(lade(&ablage, &schluessel), None);

        // Dieselbe Folge lässt sich auch nicht erneut ablegen …
        let bytes = Bauplan::neu(3).bauen();
        assert!(matches!(
            ablage
                .ablegen(
                    KERN,
                    0,
                    &schluessel.oeffentlich(),
                    &bytes,
                    &schluessel.signieren(&bytes)
                )
                .unwrap_err(),
            AblageFehler::Abgelehnt(Ablehnung::Gesperrt { folge: 3 })
        ));
        // … eine höhere schon, und die Sperre bleibt stehen.
        lege_ab(&ablage, &schluessel, 4);
        assert_eq!(gesperrt(&ablage), vec![3]);
        assert_eq!(lade(&ablage, &schluessel), Some(4));
    }

    /// Ohne lesbaren Zustand ist die Sperrliste verloren. Das Paket auf der
    /// Platte wird dann NICHT geladen — sonst käme eine zurückgenommene
    /// Folge still zurück, sobald jemand die Zustandsdatei löscht.
    #[test]
    fn paket_ohne_lesbaren_zustand_wird_nicht_geladen() {
        let schluessel = Testschluessel::neu(1);
        type Verderber = fn(&Path);
        let faelle: [(&str, Verderber); 3] = [
            ("kaputt", |p| std::fs::write(p, b"{ kaputt").unwrap()),
            ("gelöscht", |p| std::fs::remove_file(p).unwrap()),
            ("fremdes Format", |p| {
                std::fs::write(p, br#"{"format":9,"kern":"1.2.3","gesperrt":[]}"#).unwrap()
            }),
        ];
        for (name, verderben) in faelle {
            let dir = tempfile::tempdir().unwrap();
            let ablage = Ablage::neu(dir.path());
            lege_ab(&ablage, &schluessel, 1);
            verderben(&dir.path().join(DATEI_ZUSTAND));
            assert_eq!(lade(&ablage, &schluessel), None, "{name}");
            assert!(!dir.path().join(DATEI_AKTIV).exists(), "{name}");
            assert!(dir.path().join(DATEI_ABGELEHNT).exists(), "{name}");
        }
    }

    /// Bekannte Grenze, bewusst so: Ist der Zustand kaputt, nimmt `ablegen`
    /// trotzdem ein neues Paket an und legt den Zustand neu an — die alten
    /// Sperren sind dann weg. Die Alternative wäre, dass auf diesem PC nie
    /// wieder ein Fix ankommt.
    #[test]
    fn ablegen_mit_kaputtem_zustand_legt_ihn_neu_an_ohne_alte_sperren() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        sperre(&ablage, &[2]);
        std::fs::write(dir.path().join(DATEI_ZUSTAND), b"{ kaputt").unwrap();

        lege_ab(&ablage, &schluessel, 2);
        assert_eq!(gesperrt(&ablage), Vec::<u32>::new());
        assert_eq!(lade(&ablage, &schluessel), Some(2));
    }

    #[test]
    fn zurueck_aktiviert_das_vorige_paket_und_sperrt_die_folge() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        lege_ab(&ablage, &schluessel, 2);

        let voriges = ablage
            .zurueck(KERN, 2, &schluessel.oeffentlich())
            .unwrap()
            .expect("voriges Paket");
        assert_eq!(voriges.folge, 1);
        assert_eq!(gesperrt(&ablage), vec![2]);
        assert_eq!(ablage.gesperrt(KERN), vec![2]);
        assert_eq!(ablage.gesperrt("9.9.9"), Vec::<u32>::new());
        assert!(
            !dir.path().join(DATEI_VORHER).exists(),
            "nur EIN Schritt zurück"
        );
        // Nach einem Neustart gilt dasselbe.
        assert_eq!(lade(&ablage, &schluessel), Some(1));

        // Noch einmal zurück: kein voriges Paket mehr → eingebaut.
        assert!(ablage
            .zurueck(KERN, 1, &schluessel.oeffentlich())
            .unwrap()
            .is_none());
        assert_eq!(gesperrt(&ablage), vec![2, 1]);
        assert!(!dir.path().join(DATEI_AKTIV).exists());
        assert_eq!(lade(&ablage, &schluessel), None);
    }

    /// Lässt sich das vorige Paket nicht an die Stelle des aktiven schieben
    /// (dort liegt ein Ordner), endet der Rückweg beim eingebauten Stand —
    /// Platte und Speicher sagen danach dasselbe.
    #[test]
    fn zurueck_bei_gescheitertem_umbenennen_endet_beim_eingebauten_stand() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        lege_ab(&ablage, &schluessel, 2);
        let aktiv = dir.path().join(DATEI_AKTIV);
        std::fs::remove_file(&aktiv).unwrap();
        std::fs::create_dir(&aktiv).unwrap();
        std::fs::write(aktiv.join("x"), b"x").unwrap();

        assert!(ablage
            .zurueck(KERN, 2, &schluessel.oeffentlich())
            .unwrap()
            .is_none());
        assert_eq!(gesperrt(&ablage), vec![2]);
        assert!(!dir.path().join(DATEI_VORHER).exists());
        assert_eq!(lade(&ablage, &schluessel), None);
    }

    #[test]
    fn zurueck_ohne_aktives_paket_ist_ein_fehler() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        assert!(ablage.zurueck(KERN, 0, &schluessel.oeffentlich()).is_err());
        assert!(!dir.path().join(DATEI_ZUSTAND).exists());
    }

    /// Ein voriges Paket, das die Prüfung nicht mehr besteht (hier: selbst
    /// gesperrt), wird nicht reaktiviert — dann gilt der eingebaute Stand.
    #[test]
    fn zurueck_auf_ungueltiges_voriges_paket_endet_beim_eingebauten_stand() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        lege_ab(&ablage, &schluessel, 1);
        lege_ab(&ablage, &schluessel, 2);
        sperre(&ablage, &[1]);
        assert!(ablage
            .zurueck(KERN, 2, &schluessel.oeffentlich())
            .unwrap()
            .is_none());
        assert_eq!(gesperrt(&ablage), vec![1, 2]);
        assert_eq!(lade(&ablage, &schluessel), None);
    }

    #[test]
    fn schreibfehler_laesst_aktiven_stand_unberuehrt() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        let (erstes, _) = lege_ab(&ablage, &schluessel, 1);
        let (zweites, _) = lege_ab(&ablage, &schluessel, 2);
        let bytes = Bauplan::neu(3).bauen();
        let sig = schluessel.signieren(&bytes);

        // Jede Nebendatei einmal unbeschreibbar machen (dort liegt ein
        // Ordner). Das steht für „Platte voll": Der Schritt scheitert, bevor
        // das aktive Paket ersetzt wird.
        for blockiert in [DATEI_ZUSTAND_NEU, DATEI_NEU, DATEI_VORHER_NEU] {
            std::fs::create_dir(dir.path().join(blockiert)).unwrap();
            let fehler = ablage
                .ablegen(KERN, 2, &schluessel.oeffentlich(), &bytes, &sig)
                .unwrap_err();
            assert!(matches!(fehler, AblageFehler::Platte(_)), "{blockiert}");
            std::fs::remove_dir(dir.path().join(blockiert)).unwrap();

            assert_eq!(
                ablage.paket_lesen(DATEI_AKTIV).unwrap().unwrap().1,
                zweites,
                "{blockiert}: aktiv bleibt das zweite"
            );
            assert_eq!(
                ablage.paket_lesen(DATEI_VORHER).unwrap().unwrap().1,
                erstes,
                "{blockiert}: vorher bleibt das erste"
            );
            assert!(!dir.path().join(DATEI_NEU).exists(), "{blockiert}");
            assert_eq!(lade(&ablage, &schluessel), Some(2), "{blockiert}");
        }

        // Danach geht es wieder.
        ablage
            .ablegen(KERN, 2, &schluessel.oeffentlich(), &bytes, &sig)
            .expect("ablegen");
        assert_eq!(lade(&ablage, &schluessel), Some(3));
        assert_eq!(
            ablage.paket_lesen(DATEI_VORHER).unwrap().unwrap().1,
            zweites
        );
    }
}

/// Für die Tests anderer Module: ein gültiges Paket als Bytes.
#[cfg(test)]
pub(crate) mod testhilfe {
    use std::io::Write;

    use crate::tablet::assets::{self, Seite};

    pub(crate) const KERN: &str = "1.2.3";

    /// Alle Seiten wie eingebaut, mit einem Kommentar je Folge davor.
    pub(crate) fn paket_bytes(folge: u32) -> Vec<u8> {
        let eingebaut = assets::eingebauter_stand();
        let seiten: std::collections::BTreeMap<String, String> = Seite::ALLE
            .iter()
            .map(|s| {
                (
                    s.datei().to_string(),
                    format!("<!-- folge {folge} -->{}", eingebaut.seite(*s)),
                )
            })
            .collect();
        let json = serde_json::json!({
            "format": 1, "art": "seiten", "kern": KERN, "folge": folge,
            "commit": "0123abcd", "gebaut": "", "seiten": seiten,
        });
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        enc.finish().unwrap()
    }
}
