//! Schreibweichen bei Quelle badhub → Kernbefehle (Paket L3).
//!
//! Die fünf BTP-Schreibwege (`write_*` in `quelle/mod.rs`) tragen
//! BTP-neutrale Angaben (Feld-ID, Spiel-ID, Sätze, Offizielle). Hier werden
//! sie in `turnierkern::TurnierBefehl` übersetzt — rein, ohne Platte, Netz
//! oder Uhr (die Zeit kommt als Argument), und damit vollständig testbar.
//!
//! | Weiche | Kernbefehl |
//! |---|---|
//! | Feld zuweisen / umhängen (`write_courts_to_btp`, Court mit Spiel) | `spiel_aufrufen {feld}` (Feldname wie im Paket-Rahmen) |
//! | Feld räumen ohne Ergebnis (Court ohne Spiel) | `aufruf_zuruecknehmen` für das Spiel, das dort aufgerufen war |
//! | Ergebnis, Walkover, Aufgabe, Disqualifikation (`write_result_*`) | `ergebnis_melden` (Sätze, `ende`, `beginn`/`bis`) |
//! | Schiedsrichter (`write_officials_to_btp`) | `offizielle_setzen` (IDs aus `paket.schiedsrichter`) |
//! | Zeilenfarbe (`write_highlight_to_btp`) | keiner — nur lokal |
//!
//! Ein Befehl, der am Stand nichts ändern würde (dasselbe Ergebnis zweimal
//! vom Tablet, derselbe Aufruf), entsteht gar nicht erst: So bleibt eine
//! Wiederholung wirkungslos, ohne dass das Protokoll wächst.

use turnierkern::{
    Offizieller, Seite, SpielStatus, Spielende, TurnierAblehnung, TurnierBefehl, TurnierStand,
};

use super::abbildung::{feld_kern_name, spiel_zu_id};
use super::paket::Paket;
use crate::btp::proto::{CourtAssignment, MatchCourt, MatchUpdate};

/// Unix-Millisekunden → Kern-Zeit `JJJJ-MM-TTTHH:MM:SS` (lokale Wandzeit des
/// Hallen-PCs, in Deutschland = Europe/Berlin).
pub fn wandzeit(ms: u64) -> String {
    use chrono::TimeZone;
    let ms = i64::try_from(ms).unwrap_or(i64::MAX);
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => {
            t.format("%Y-%m-%dT%H:%M:%S").to_string()
        }
        chrono::LocalResult::None => chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    }
}

/// Ein Befehl ohne Kennung: `id` vergibt der Aufrufer beim Ausführen.
pub type Entwurf = TurnierBefehl;

fn spiel_im_stand<'a>(
    stand: &'a TurnierStand,
    konkurrenz: &str,
    spiel: u32,
) -> Option<&'a turnierkern::Spiel> {
    stand
        .durchfuehrung
        .get(konkurrenz)?
        .spiele
        .iter()
        .find(|s| s.nr == spiel)
}

fn spiel_von(paket: &Paket, match_id: i64) -> Result<(String, u32), String> {
    spiel_zu_id(paket, match_id)
        .ok_or_else(|| format!("Spiel {match_id} gehört zu keiner Konkurrenz dieses Turniers."))
}

/// Feldvergabe: Court mit Spiel → `spiel_aufrufen`, Court ohne Spiel →
/// `aufruf_zuruecknehmen` für das dort aufgerufene Spiel (sofern es nicht im
/// selben Auftrag auf ein anderes Feld wandert — Umhängen). Erst die
/// Rücknahmen, dann die Aufrufe: so kann ein Spiel auf ein im selben Auftrag
/// geräumtes Feld.
pub fn aufrufe(
    stand: &TurnierStand,
    paket: &Paket,
    courts: &[CourtAssignment],
    jetzt: &str,
) -> Result<Vec<Entwurf>, String> {
    let mut aufrufe = Vec::new();
    let mut gerufen: Vec<(String, u32)> = Vec::new();
    for c in courts {
        let Some(match_id) = c.match_id else { continue };
        let (konkurrenz, spiel) = spiel_von(paket, match_id)?;
        let feld = feld_kern_name(paket, stand, c.court_id)
            .ok_or_else(|| format!("Feld {} ist in diesem Turnier unbekannt.", c.court_id))?;
        gerufen.push((konkurrenz.clone(), spiel));
        let schon = spiel_im_stand(stand, &konkurrenz, spiel).is_some_and(|s| {
            s.status == SpielStatus::Aufgerufen && s.feld.as_deref() == Some(feld.as_str())
        });
        if !schon {
            aufrufe.push(TurnierBefehl::SpielAufrufen {
                id: String::new(),
                konkurrenz,
                spiel,
                feld: Some(feld),
                jetzt: jetzt.to_string(),
            });
        }
    }
    let mut ruecknahmen = Vec::new();
    for c in courts.iter().filter(|c| c.match_id.is_none()) {
        let Some(feld) = feld_kern_name(paket, stand, c.court_id) else {
            continue;
        };
        for (konkurrenz, d) in &stand.durchfuehrung {
            for s in d.spiele.iter().filter(|s| {
                s.status == SpielStatus::Aufgerufen && s.feld.as_deref() == Some(feld.as_str())
            }) {
                if !gerufen.iter().any(|(k, n)| k == konkurrenz && *n == s.nr) {
                    ruecknahmen.push(TurnierBefehl::AufrufZuruecknehmen {
                        id: String::new(),
                        konkurrenz: konkurrenz.clone(),
                        spiel: s.nr,
                    });
                }
            }
        }
    }
    ruecknahmen.extend(aufrufe);
    Ok(ruecknahmen)
}

/// Ergebnis → `ergebnis_melden`. `None`, wenn genau dieses Ergebnis schon
/// gilt (das Tablet schickt es zweimal). Team 1 = oben.
///
/// BTP-`ScoreStatus`: 0 regulär, 1 kampflos, 2 Aufgabe, 3 Disqualifikation.
/// Der Kern kennt keine Disqualifikation: mit Sätzen als Aufgabe der
/// unterlegenen Seite, ohne Sätze als kampflos.
pub fn ergebnis(
    stand: &TurnierStand,
    paket: &Paket,
    u: &MatchUpdate,
    jetzt: &str,
) -> Result<Option<Entwurf>, String> {
    let (konkurrenz, spiel) = spiel_von(paket, u.btp_match_id)?;
    let sieger = if u.team1_won {
        Seite::Oben
    } else {
        Seite::Unten
    };
    let verlierer = if u.team1_won {
        Seite::Unten
    } else {
        Seite::Oben
    };
    let saetze_roh: Vec<(u16, u16)> = u
        .sets
        .iter()
        .map(|&(a, b)| {
            (
                u16::try_from(a.clamp(0, 99)).unwrap_or(0),
                u16::try_from(b.clamp(0, 99)).unwrap_or(0),
            )
        })
        .collect();
    let (saetze, ende) = match u.score_status {
        1 => (Vec::new(), Some(Spielende::Kampflos { sieger })),
        2 => (
            saetze_roh,
            Some(Spielende::Aufgabe {
                aufgebend: verlierer,
            }),
        ),
        3 if saetze_roh.is_empty() => (Vec::new(), Some(Spielende::Kampflos { sieger })),
        3 => (
            saetze_roh,
            Some(Spielende::Aufgabe {
                aufgebend: verlierer,
            }),
        ),
        _ => (saetze_roh, None),
    };
    if let Some(s) = spiel_im_stand(stand, &konkurrenz, spiel) {
        if s.status == SpielStatus::Beendet
            && s.ergebnis
                .as_ref()
                .is_some_and(|e| e.saetze == saetze && e.ende == ende)
        {
            return Ok(None);
        }
    }
    let bis = u.end_ts_ms.map(wandzeit);
    // Spielbeginn aus der gemessenen Dauer (bts-light-Zeitstempel); ohne
    // Messung (kampflos, unbekannt) nimmt der Kern den Aufruf.
    let beginn = match (u.end_ts_ms, u.duration_mins) {
        (Some(e), d) if d > 0 && u.score_status != 1 => {
            Some(wandzeit(e.saturating_sub(d.unsigned_abs() * 60_000)))
        }
        _ => None,
    };
    Ok(Some(TurnierBefehl::ErgebnisMelden {
        id: String::new(),
        konkurrenz,
        spiel,
        saetze,
        ende,
        jetzt: jetzt.to_string(),
        beginn,
        bis,
    }))
}

/// Ein Offizieller aus der bts-light-ID (= Schiedsrichter-ID im Paket), mit
/// der Meldeperson aus dem Paket. `0` = kein Dienst.
fn offizieller(paket: &Paket, id: i64) -> Option<Offizieller> {
    let id = u32::try_from(id).ok().filter(|i| *i > 0)?;
    let person = paket
        .schiedsrichter
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| s.person);
    Some(Offizieller { id, person })
}

/// Schiedsrichter-Einträge → je Spiel `offizielle_setzen` (`None` = schon
/// so). Einträge ohne `officials` werden übergangen.
pub fn offizielle(
    stand: &TurnierStand,
    paket: &Paket,
    entries: &[MatchCourt],
) -> Vec<(MatchCourt, Result<Option<Entwurf>, String>)> {
    entries
        .iter()
        .filter_map(|e| {
            let (sr, ar) = e.officials?;
            let r = spiel_von(paket, e.match_id).map(|(konkurrenz, spiel)| {
                let schiedsrichter = offizieller(paket, sr);
                let aufschlagrichter = offizieller(paket, ar);
                let gleich = spiel_im_stand(stand, &konkurrenz, spiel).is_some_and(|s| {
                    s.schiedsrichter.as_ref().map(|o| o.id) == schiedsrichter.as_ref().map(|o| o.id)
                        && s.aufschlagrichter.as_ref().map(|o| o.id)
                            == aufschlagrichter.as_ref().map(|o| o.id)
                });
                (!gleich).then(|| TurnierBefehl::OffizielleSetzen {
                    id: String::new(),
                    konkurrenz,
                    spiel,
                    schiedsrichter,
                    aufschlagrichter,
                })
            });
            Some((*e, r))
        })
        .collect()
}

/// Setzt die Kennung eines Entwurfs.
pub fn mit_id(befehl: TurnierBefehl, neu: String) -> TurnierBefehl {
    use TurnierBefehl as B;
    match befehl {
        B::SpielAufrufen {
            konkurrenz,
            spiel,
            feld,
            jetzt,
            ..
        } => B::SpielAufrufen {
            id: neu,
            konkurrenz,
            spiel,
            feld,
            jetzt,
        },
        B::AufrufZuruecknehmen {
            konkurrenz, spiel, ..
        } => B::AufrufZuruecknehmen {
            id: neu,
            konkurrenz,
            spiel,
        },
        B::ErgebnisMelden {
            konkurrenz,
            spiel,
            saetze,
            ende,
            jetzt,
            beginn,
            bis,
            ..
        } => B::ErgebnisMelden {
            id: neu,
            konkurrenz,
            spiel,
            saetze,
            ende,
            jetzt,
            beginn,
            bis,
        },
        B::OffizielleSetzen {
            konkurrenz,
            spiel,
            schiedsrichter,
            aufschlagrichter,
            ..
        } => B::OffizielleSetzen {
            id: neu,
            konkurrenz,
            spiel,
            schiedsrichter,
            aufschlagrichter,
        },
        // Andere Befehle entstehen hier nicht; sie behalten ihre Kennung.
        andere => andere,
    }
}

/// Die Befehle, die badhub unter der Lease annimmt (badhub `Lease::BEFEHLE`).
pub const ERLAUBTE_ARTEN: [&str; 6] = [
    "ergebnis_melden",
    "ergebnis_loeschen",
    "spiel_aufrufen",
    "aufruf_zuruecknehmen",
    "offizielle_setzen",
    "gruppenrang_entscheiden",
];

/// Ist die Befehls-ID für badhub zulässig? (`[A-Za-z0-9-]{1,64}`, wie
/// badhub `LeaseHochladen::postenGueltig`.)
pub fn id_gueltig(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Ablehnung des Kerns in Worten — ohne Personennamen (nur Nummern), weil
/// der Text auch ins Protokoll geht.
pub fn ablehnung_text(a: &TurnierAblehnung) -> String {
    use TurnierAblehnung as A;
    match a {
        A::FeldBelegt { feld, spiel, .. } => {
            format!("Feld {feld} ist belegt (dort läuft Spiel {spiel}).")
        }
        A::PersonBelegt { spiel, .. } => {
            format!("Eine Person dieses Spiels steht gerade in Spiel {spiel}.")
        }
        A::OffiziellerBelegt { spiel, .. } => {
            format!("Die Offizielle bzw. der Offizielle ist gerade in Spiel {spiel} eingeteilt.")
        }
        A::OffiziellerDoppelt { .. } => {
            "Schiedsrichter und Aufschlagrichter sind dieselbe Person.".into()
        }
        A::OffiziellerSpielt { .. } => "Die Offizielle bzw. der Offizielle spielt in diesem Spiel.".into(),
        A::SpielWartet { spiel, .. } => {
            format!("Spiel {spiel}: Die Paarung steht noch nicht fest.")
        }
        A::SpielBeendet { spiel, .. } => format!("Spiel {spiel} ist schon beendet."),
        A::ErgebnisUngueltig { .. } => "Das Ergebnis passt nicht zur Zählweise.".into(),
        A::ErgebnisNichtKorrigierbar { spiel, .. } => format!(
            "Das Ergebnis von Spiel {spiel} lässt sich nicht mehr ändern — ein Folgespiel hat schon ein Ergebnis."
        ),
        A::KeinErgebnis { spiel, .. } => format!("Spiel {spiel} hat kein Ergebnis."),
        A::NichtAufgerufen { spiel, .. } => format!("Spiel {spiel} ist nicht aufgerufen."),
        A::UnbekanntesSpiel { spiel, .. } => format!("Spiel {spiel} gibt es nicht."),
        A::FeldUngueltig => "Ungültige Feldangabe.".into(),
        A::UngueltigeZeit => "Ungültige Zeitangabe.".into(),
        A::AuslosungNichtFreigegeben { .. } => "Die Auslosung ist noch nicht freigegeben.".into(),
        A::GruppeNichtFertig { .. } => "Die Gruppe ist noch nicht fertig gespielt.".into(),
        A::RangEntscheidUngueltig { .. } => "Ungültige Reihenfolge für den Gruppenrang.".into(),
        andere => {
            let grund = serde_json::to_value(andere)
                .ok()
                .and_then(|v| v.get("grund").and_then(|g| g.as_str()).map(str::to_string))
                .unwrap_or_else(|| "unbekannt".into());
            format!("Der Turnierkern lehnt ab ({grund}).")
        }
    }
}
