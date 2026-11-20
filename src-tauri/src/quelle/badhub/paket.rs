//! Turnierpaket von badhub (`GET /api/torg/v1/turnier/paket`, badhub
//! `docs/features/bts_light_steuerung.md`, Abschnitt „Turnierpaket“).
//!
//! Alles, was bts-light offline zum Anzeigen eines badhub-Turniers braucht:
//! die Kern-Ereignisse und die Stammdaten, die der Kern nicht kennt
//! (Konkurrenz-Namen, Hallen und Feldnamen, Rundentexte, Personen).
//!
//! **Gespeichert** (unter `turniere/<uuid>/`, siehe `ablage.rs`): Die
//! Personenfelder (`meldungen`, `schiedsrichter`) liegen dabei verschlüsselt
//! getrennt vom Rest. Unbekannte Zusatzfelder übergeht serde — das Paket darf
//! auf badhub-Seite additiv wachsen.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use turnierkern::TurnierEreignis;

/// Paket-Schema, das diese Version versteht.
pub const SCHEMA: u32 = 1;

/// Das ganze Paket, so wie badhub es liefert.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Paket {
    pub schema: u32,
    /// Version des Kern-Dienstes in badhub. Zum Lesen genügt ein gleiches
    /// Ereignisschema; zum Schreiben (L3) muss sie gleich der eingebundenen
    /// Crate sein.
    #[serde(default)]
    pub kern_version: String,
    pub turnier: PaketTurnier,
    /// Alle Kern-Ereignisse des Turniers (aufsteigend nach `n`).
    #[serde(default)]
    pub ereignisse: Vec<TurnierEreignis>,
    #[serde(default)]
    pub konkurrenzen: Vec<PaketKonkurrenz>,
    #[serde(default)]
    pub rahmen: PaketRahmen,
    /// Rundentexte je Konkurrenz und Spielnummer (`{"HE-A": {"5": "Viertelfinale"}}`),
    /// gebaut in badhub wie auf der Turnierseite.
    #[serde(default)]
    pub runden: BTreeMap<String, BTreeMap<String, String>>,
    /// Personenauszug (ADR-0073 in badhub): nur Name, Verein, Nation.
    #[serde(default)]
    pub meldungen: Vec<PaketMeldung>,
    #[serde(default)]
    pub schiedsrichter: Vec<PaketSchiedsrichter>,
    /// Fingerabdruck der Stammdaten (Rückfall, wenn kein `ETag`-Header kommt).
    #[serde(default)]
    pub stammdaten_etag: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketTurnier {
    pub id: i64,
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub beginn: Option<String>,
    /// Letzter Turniertag (`JJJJ-MM-TT`), Grundlage der Löschfrist des
    /// Personenauszugs (14 Tage danach).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ende: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketKonkurrenz {
    /// Schlüssel im Kern (`HE-A`).
    pub key: String,
    #[serde(default)]
    pub name: String,
    /// Disziplin-Kürzel (`HE`, `DE`, `HD`, `DD`, `GD`/`MX`).
    #[serde(default)]
    pub disziplin: String,
    /// Laufende Nummer im Turnier; Grundlage der stabilen Spiel-ID
    /// `nr * 100000 + spiel`.
    pub nr: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketRahmen {
    #[serde(default)]
    pub hallen: Vec<PaketHalle>,
    #[serde(default)]
    pub mindestpause_min: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketHalle {
    /// Kennung wie im Kern (`Planzeit::halle`).
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Feldnamen in Anzeige-Reihenfolge.
    #[serde(default)]
    pub felder: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketMeldung {
    pub meldung: u32,
    pub konkurrenz: String,
    #[serde(default)]
    pub personen: Vec<PaketPerson>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketPerson {
    pub person: u32,
    #[serde(default)]
    pub vorname: String,
    #[serde(default)]
    pub nachname: String,
    #[serde(default)]
    pub verein: Option<String>,
    #[serde(default)]
    pub nation: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaketSchiedsrichter {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Meldeperson, falls die Person auch spielt — gehört in
    /// `offizielle_setzen` (badhub prüft ID **und** Person gegen die
    /// Schiedsrichterliste). Fehlt sie im Paket, geht der Befehl ohne Person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<u32>,
}

/// Der verschlüsselt abgelegte Teil des Pakets.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Personen {
    #[serde(default)]
    pub meldungen: Vec<PaketMeldung>,
    #[serde(default)]
    pub schiedsrichter: Vec<PaketSchiedsrichter>,
}

impl Paket {
    /// Trennt den Personenauszug ab (für die verschlüsselte Ablage).
    pub fn ohne_personen(&self) -> (Paket, Personen) {
        let mut rest = self.clone();
        let personen = Personen {
            meldungen: std::mem::take(&mut rest.meldungen),
            schiedsrichter: std::mem::take(&mut rest.schiedsrichter),
        };
        (rest, personen)
    }

    /// Fügt den Personenauszug wieder ein.
    pub fn mit_personen(mut self, personen: Personen) -> Paket {
        self.meldungen = personen.meldungen;
        self.schiedsrichter = personen.schiedsrichter;
        self
    }

    /// Höchste Ereignisnummer (0 = keine Ereignisse).
    pub fn n(&self) -> u64 {
        self.ereignisse
            .iter()
            .map(TurnierEreignis::n)
            .max()
            .unwrap_or(0)
    }

    /// Hängt neue Ereignisse an: schon bekannte Nummern werden übergangen,
    /// danach aufsteigend geordnet. Liefert, wie viele neu waren.
    pub fn ereignisse_anhaengen(&mut self, neu: Vec<TurnierEreignis>) -> usize {
        let mut bekannt: std::collections::BTreeSet<u64> =
            self.ereignisse.iter().map(TurnierEreignis::n).collect();
        let mut zahl = 0;
        for e in neu {
            if bekannt.insert(e.n()) {
                self.ereignisse.push(e);
                zahl += 1;
            }
        }
        self.ereignisse.sort_by_key(TurnierEreignis::n);
        zahl
    }

    /// Prüft, ob diese Version das Paket lesen kann. Fehlertext ohne Inhalt.
    pub fn pruefen(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!(
                "Das Turnierpaket hat Schema {}, diese Version versteht Schema {SCHEMA}. \
                 Bitte bts-light aktualisieren.",
                self.schema
            ));
        }
        // Die Konkurrenz-Nummer bildet die stabile Spiel-ID: positiv und eindeutig.
        let mut nummern = std::collections::BTreeSet::new();
        for k in &self.konkurrenzen {
            if k.nr <= 0 || k.nr > 90_000 || !nummern.insert(k.nr) {
                return Err(
                    "Das Turnierpaket nennt eine ungültige oder doppelte Konkurrenz-Nummer.".into(),
                );
            }
        }
        if !super::uuid_gueltig(&self.turnier.uuid) {
            return Err("Das Turnierpaket nennt keine gültige Turnier-UUID.".into());
        }
        Ok(())
    }
}
