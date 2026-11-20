//! Kern-Stand + Turnierpaket → [`BtpSnapshot`] (Paket L2).
//!
//! Das heutige Domänenmodell bleibt `BtpSnapshot`: Tablets, Monitore, TL-Web,
//! Meetingpoint, Ansage und Auto-Vergabe lesen es unverändert. Diese Funktion
//! ist rein (kein Netz, keine Platte, keine Uhr) und daher vollständig
//! testbar.
//!
//! **Abbildung** (Begründungen in `docs/features/turnierquelle.md`):
//!
//! | BTP-Feld | Quelle im Kern-Turnier |
//! |---|---|
//! | `id` | `konkurrenz.nr * 100000 + spiel` (stabil, alle Stores hängen daran) |
//! | `draw_id` / `planning_id` | Konkurrenz-Nr. / Spielnummer — die Baumkante `from1/from2` zeigt auf die Spielnummer des Vorspiels |
//! | `from1/from2` | `Sieger`/`Verlierer` aus dem Gerüst; Losplatz und Gruppenrang = `None` |
//! | `entry1_id/entry2_id` | Meldungsnummer (turnierweit eindeutig), 0 = offen |
//! | `court_id` | 1 + laufender Index über `rahmen.hallen[].felder`; Halle = Location (1 + Index) |
//! | `planned_time`, `display_order`, `location_id` | `planung.zeiten` (Beginn, Reihenfolge, Halle) |
//! | `round_name` | `runden[konkurrenz][spiel]` aus dem Paket, sonst eigener Text |
//! | `scoring` | Zählweise des Spiels (`Spiel::zaehlweise`, Kern 0.14.0: Spiel → Phase/Konkurrenz → Turnier → Badminton) |
//! | `highlight`, `shuttles` | 0 — gibt es nur in BTP |
//! | Rückzug (Kern 0.13.0) | `entfaellt` → nicht im Modell; `durch_rueckzug` → Finished + Walkover; `gestrichen` ändert nichts (Tabellen rechnet bts-light nicht) |

use std::collections::BTreeMap;

use turnierkern::{
    Phase, Quelle, Seite, Setzung, Spiel, SpielStatus, Spielende, TurnierStand, Zaehlweise,
};

use super::paket::{Paket, PaketKonkurrenz, PaketPerson};
use crate::btp::model::{
    class_label, BtpCourt, BtpEntry, BtpEvent, BtpLocation, BtpMatch, BtpOfficial, BtpPlayer,
    BtpSnapshot, Discipline, MatchResult, MatchStatus, ScoringFormat,
};

/// Multiplikator der stabilen Spiel-ID.
pub const ID_FAKTOR: i64 = 100_000;

/// Stabile Spiel-ID eines Kern-Spiels.
pub fn spiel_id(konkurrenz_nr: i64, spiel: u32) -> i64 {
    konkurrenz_nr * ID_FAKTOR + i64::from(spiel)
}

/// Ein Feld: Identität, Anzeige und die Schreibweise im Kern.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FeldInfo {
    id: i64,
    location_id: i64,
    sort_order: i64,
    anzeige: String,
    kern: String,
}

/// Hallen und Felder aus dem Paket (Rückfall: Feldzahl aus dem Kern-Rahmen,
/// benannt wie badhub `Spielansicht::felder`).
fn felder(paket: &Paket, stand: &TurnierStand) -> (Vec<BtpLocation>, Vec<FeldInfo>) {
    let mut hallen: Vec<(String, String, Vec<String>)> = paket
        .rahmen
        .hallen
        .iter()
        .map(|h| {
            let name = if h.name.trim().is_empty() {
                h.id.clone()
            } else {
                h.name.clone()
            };
            (h.id.clone(), name, h.felder.clone())
        })
        .collect();
    if hallen.is_empty() {
        if let Some(r) = stand.planung.rahmen.as_ref() {
            hallen = r
                .hallen
                .iter()
                .map(|h| {
                    let felder = (1..=h.felder.min(100)).map(|i| i.to_string()).collect();
                    (h.id.clone(), h.id.clone(), felder)
                })
                .collect();
        }
    }
    let mehrere = hallen.len() > 1;
    let mut locations = Vec::new();
    let mut infos = Vec::new();
    for (hi, (id, name, felder)) in hallen.iter().enumerate() {
        let location_id = hi as i64 + 1;
        locations.push(BtpLocation {
            id: location_id,
            name: name.clone(),
        });
        let praefix = format!("{id} ");
        for (fi, feld) in felder.iter().enumerate() {
            // Steht die Halle schon im Feldnamen („H1 3“), ist das die
            // Kern-Schreibweise; angezeigt wird nur die Nummer.
            let (kern, anzeige) = match feld.strip_prefix(&praefix) {
                Some(rest) if mehrere => (feld.clone(), rest.to_string()),
                _ if mehrere => (format!("{praefix}{feld}"), feld.clone()),
                _ => (feld.clone(), feld.clone()),
            };
            infos.push(FeldInfo {
                id: infos.len() as i64 + 1,
                location_id,
                sort_order: fi as i64,
                anzeige,
                kern,
            });
        }
    }
    (locations, infos)
}

/// Rückweg der Feld-ID (Paket L3): die Schreibweise des Felds im Kern, wie
/// sie `spiel_aufrufen` braucht. `None` für eine unbekannte ID.
pub fn feld_kern_name(paket: &Paket, stand: &TurnierStand, court_id: i64) -> Option<String> {
    let (_, infos) = felder(paket, stand);
    infos.into_iter().find(|f| f.id == court_id).map(|f| f.kern)
}

/// Rückweg der Spiel-ID (Paket L3): Konkurrenz-Schlüssel und Spielnummer.
pub fn spiel_zu_id(paket: &Paket, match_id: i64) -> Option<(String, u32)> {
    if match_id <= 0 {
        return None;
    }
    let nr = match_id / ID_FAKTOR;
    let spiel = u32::try_from(match_id % ID_FAKTOR)
        .ok()
        .filter(|s| *s > 0)?;
    paket
        .konkurrenzen
        .iter()
        .find(|k| k.nr == nr)
        .map(|k| (k.key.clone(), spiel))
}

/// Feld zu einer Kern-Feldbezeichnung. Bei einer Halle zählt der Name; bei
/// mehreren die Schreibweise „<Halle> n“, ersatzweise ein eindeutiger
/// Anzeigename.
fn feld_zu<'a>(infos: &'a [FeldInfo], kern: &str) -> Option<&'a FeldInfo> {
    if let Some(f) = infos.iter().find(|f| f.kern == kern) {
        return Some(f);
    }
    let mut treffer = infos.iter().filter(|f| f.anzeige == kern);
    match (treffer.next(), treffer.next()) {
        (Some(f), None) => Some(f),
        _ => None,
    }
}

fn disziplin(kuerzel: &str) -> Discipline {
    match kuerzel.trim().to_ascii_uppercase().as_str() {
        "HE" | "JE" | "ME" => Discipline::MensSingles,
        "DE" => Discipline::WomensSingles,
        "HD" | "JD" => Discipline::MensDoubles,
        "DD" | "MD" => Discipline::WomensDoubles,
        "GD" | "MX" | "XD" => Discipline::Mixed,
        _ => Discipline::Unknown,
    }
}

/// Zählweise → BTP-Zählformat. Ohne Deckel ein unerreichbarer Wert. Pause bei 11 (Sätze bis 21)
/// bzw. 8 (bis 15); 11er-Sätze (5×11, Bundesliga) haben keine reguläre Pause im Satz — wie
/// BTP-Satztyp 304 in `ScoringFormat::from_btp`.
fn zaehlformat(z: &Zaehlweise) -> ScoringFormat {
    let ziel = i64::from(z.satz.ziel);
    ScoringFormat {
        best_of: i64::from(z.gewinnsaetze) * 2 - 1,
        target_score: ziel,
        cap_score: z.satz.deckel.map_or(999, i64::from),
        interval_at: match ziel {
            21 => Some(11),
            15 => Some(8),
            _ => None,
        },
    }
}

/// `JJJJ-MM-TTTHH:MM:SS` → `YYYYMMDDHHMM`.
fn planzeit_schluessel(zeit: &str) -> Option<i64> {
    let ziffern: String = zeit.chars().filter(char::is_ascii_digit).take(12).collect();
    if ziffern.len() < 12 {
        return None;
    }
    ziffern.parse().ok()
}

fn spieler(p: &PaketPerson) -> BtpPlayer {
    let name = format!("{} {}", p.vorname.trim(), p.nachname.trim())
        .trim()
        .to_string();
    BtpPlayer {
        id: i64::from(p.person),
        name,
        first: p.vorname.trim().to_string(),
        last: p.nachname.trim().to_string(),
        member_id: None,
        nationality: p.nation.clone().filter(|n| !n.trim().is_empty()),
        club: p.verein.clone().filter(|v| !v.trim().is_empty()),
    }
}

/// Setzplatz-Text wie auf dem Auslosungsbogen.
fn setzplatz_text(pos: usize, setzung: Setzung) -> String {
    if setzung == Setzung::Einzeln || pos <= 2 {
        return pos.to_string();
    }
    // 3/4, 5/8, 9/16 …
    let mut unten = 3;
    let mut oben = 4;
    while pos > oben {
        unten = oben + 1;
        oben *= 2;
    }
    format!("{unten}/{oben}")
}

/// Rundentext ohne Angabe aus badhub.
fn runde_ersatz(s: &Spiel, max_ko_runde: u32) -> String {
    match s.phase {
        Phase::Gruppe => {
            let g = s.gruppe.unwrap_or(1);
            let buchstabe = char::from_u32(u32::from(b'A') + g.saturating_sub(1)).unwrap_or('?');
            format!("Gruppe {buchstabe}")
        }
        Phase::Ko if s.platz3 => "Spiel um Platz 3".into(),
        Phase::Ko => match max_ko_runde.saturating_sub(s.runde) {
            0 => "Finale".into(),
            1 => "Halbfinale".into(),
            2 => "Viertelfinale".into(),
            3 => "Achtelfinale".into(),
            _ => format!("Runde {}", s.runde),
        },
    }
}

/// Bildet Kern-Stand und Paket auf das heutige Modell ab.
pub fn abbilden(stand: &TurnierStand, paket: &Paket) -> BtpSnapshot {
    let (locations, felder) = felder(paket, stand);
    let konkurrenzen: BTreeMap<&str, &PaketKonkurrenz> = paket
        .konkurrenzen
        .iter()
        .map(|k| (k.key.as_str(), k))
        .collect();
    let personen: BTreeMap<u32, Vec<BtpPlayer>> = paket
        .meldungen
        .iter()
        .map(|m| (m.meldung, m.personen.iter().map(spieler).collect()))
        .collect();
    // Setzplätze je Meldung.
    let mut setzplaetze: BTreeMap<u32, String> = BTreeMap::new();
    for (key, liste) in &stand.setzlisten {
        let setzung = stand.setzungen.get(key).copied().unwrap_or_default();
        for (i, m) in liste.iter().enumerate() {
            setzplaetze.insert(*m, setzplatz_text(i + 1, setzung));
        }
    }
    // Reihenfolge und Halle aus dem Zeitplan.
    let mut planung: BTreeMap<(&str, u32), (usize, &str, &str)> = BTreeMap::new();
    for (i, z) in stand.planung.zeiten.iter().enumerate() {
        planung.insert(
            (z.konkurrenz.as_str(), z.spiel),
            (i, z.beginn.as_str(), z.halle.as_str()),
        );
    }
    let halle_location = |halle: &str| -> Option<i64> {
        paket
            .rahmen
            .hallen
            .iter()
            .position(|h| h.id == halle)
            .or_else(|| {
                stand
                    .planung
                    .rahmen
                    .as_ref()
                    .and_then(|r| r.hallen.iter().position(|h| h.id == halle))
            })
            .map(|i| i as i64 + 1)
    };

    let mut matches = Vec::new();
    for (key, d) in &stand.durchfuehrung {
        let Some(k) = konkurrenzen.get(key.as_str()) else {
            // Konkurrenz ohne Stammdaten: keine stabile ID möglich.
            continue;
        };
        let geruest = stand.planung.systeme.get(key).map(|p| &p.geruest);
        let herkunft = |nr: u32| -> (Option<i64>, Option<i64>) {
            let kante = |q: &Quelle| match q {
                Quelle::Sieger { spiel } | Quelle::Verlierer { spiel } => Some(i64::from(*spiel)),
                _ => None,
            };
            geruest
                .and_then(|g| g.spiele.iter().find(|s| s.nr == nr))
                .map_or((None, None), |s| (kante(&s.oben), kante(&s.unten)))
        };
        let max_ko = d
            .spiele
            .iter()
            .filter(|s| s.phase == Phase::Ko && !s.platz3)
            .map(|s| s.runde)
            .max()
            .unwrap_or(0);
        let discipline = disziplin(&k.disziplin);
        let klasse = class_label(&k.name, &k.key);
        for s in &d.spiele {
            if s.nr >= ID_FAKTOR as u32 {
                continue;
            }
            // Entfällt wegen eines Rückzugs (Kern 0.13.0: Gruppe mit
            // „streichen“ oder beide Seiten zurückgezogen): Das Spiel findet
            // nicht statt. Es erscheint gar nicht erst im Modell — so ist es
            // weder aufrufbar noch in der Auto-Vergabe, und kein Store hält
            // ein Feld dafür fest. Gruppenspiele sind nie Vorspiel eines
            // KO-Spiels (`from1/from2` zeigt nur auf Sieger/Verlierer).
            if s.status == SpielStatus::Entfaellt {
                continue;
            }
            let (from1, from2) = herkunft(s.nr);
            // Zählweise je Spiel (Kern ab 0.14.0: Spiel → Konkurrenz/KO-Phase → Turnier → 3×21),
            // nicht mehr je Konkurrenz — Gruppen 3×15 und KO 3×21 oder ein Finale 5×11 gehen so durch.
            let scoring = zaehlformat(&s.zaehlweise);
            let team = |m: Option<u32>| -> (i64, Vec<BtpPlayer>, Option<String>) {
                match m {
                    Some(m) => (
                        i64::from(m),
                        personen.get(&m).cloned().unwrap_or_default(),
                        setzplaetze.get(&m).cloned(),
                    ),
                    None => (0, Vec::new(), None),
                }
            };
            let (entry1_id, team1, team1_seed) = team(s.oben);
            let (entry2_id, team2, team2_seed) = team(s.unten);
            let feld = s.feld.as_deref().and_then(|f| feld_zu(&felder, f));
            let winner = s.ergebnis.as_ref().map(|e| match e.sieger {
                Seite::Oben => 1u8,
                Seite::Unten => 2u8,
            });
            // Erschöpfend, ohne Platzhalter: Ein neuer Status im Kern soll
            // hier am Compiler scheitern, nicht still „eingeplant“ werden.
            // Ein durch Rückzug entschiedenes Spiel (`durch_rueckzug`) ist
            // `beendet` mit `ende: kampflos` und erscheint wie ein Walkover.
            let status = match s.status {
                SpielStatus::Beendet => MatchStatus::Finished,
                SpielStatus::Aufgerufen if feld.is_some() => MatchStatus::OnCourt,
                SpielStatus::Aufgerufen | SpielStatus::Wartet | SpielStatus::Bereit => {
                    MatchStatus::Scheduled
                }
                // oben schon übergangen
                SpielStatus::Entfaellt => continue,
            };
            let result = match s.ergebnis.as_ref().and_then(|e| e.ende) {
                Some(Spielende::Kampflos { .. }) => MatchResult::Walkover,
                Some(Spielende::Aufgabe { .. }) => MatchResult::Retired,
                None => MatchResult::Normal,
            };
            let plan = planung.get(&(key.as_str(), s.nr));
            let round_name = paket
                .runden
                .get(key)
                .and_then(|r| r.get(&s.nr.to_string()))
                .cloned()
                .unwrap_or_else(|| runde_ersatz(s, max_ko));
            matches.push(BtpMatch {
                id: spiel_id(k.nr, s.nr),
                draw_id: k.nr,
                planning_id: i64::from(s.nr),
                display_order: plan.map(|(i, _, _)| *i as i64 + 1),
                highlight: 0,
                shuttles: 0,
                from1,
                from2,
                draw_name: k.key.clone(),
                discipline,
                class_label: klasse.clone(),
                round_name,
                match_num: Some(i64::from(s.nr)),
                planned_time: plan.and_then(|(_, beginn, _)| planzeit_schluessel(beginn)),
                team1,
                team2,
                entry1_id,
                entry2_id,
                team1_seed,
                team2_seed,
                court: feld.map(|f| f.anzeige.clone()),
                court_id: feld.map(|f| f.id),
                location_id: plan.and_then(|(_, _, halle)| halle_location(halle)),
                official1_id: s.schiedsrichter.as_ref().map(|o| i64::from(o.id)),
                official2_id: s.aufschlagrichter.as_ref().map(|o| i64::from(o.id)),
                sets: s
                    .ergebnis
                    .as_ref()
                    .map(|e| {
                        e.saetze
                            .iter()
                            .map(|(a, b)| (i64::from(*a), i64::from(*b)))
                            .collect()
                    })
                    .unwrap_or_default(),
                winner,
                result,
                status,
                finished_at: None,
                pause_ms: None,
                preparation_call_ts: None,
                preparation_hall: None,
                preparation_call_rank: None,
                scoring,
            });
        }
    }
    matches.sort_by_key(|m| m.id);

    let mut events: Vec<BtpEvent> = paket
        .konkurrenzen
        .iter()
        .map(|k| BtpEvent {
            id: k.nr,
            name: if k.name.trim().is_empty() {
                k.key.clone()
            } else {
                k.name.clone()
            },
            discipline: disziplin(&k.disziplin),
        })
        .collect();
    events.sort_by_key(|e| e.id);

    let mut entries: Vec<BtpEntry> = paket
        .meldungen
        .iter()
        .filter_map(|m| {
            let k = konkurrenzen.get(m.konkurrenz.as_str())?;
            let players: Vec<BtpPlayer> = m.personen.iter().map(spieler).collect();
            (!players.is_empty()).then(|| BtpEntry {
                id: i64::from(m.meldung),
                event_id: k.nr,
                players,
                seed: setzplaetze.get(&m.meldung).cloned(),
            })
        })
        .collect();
    entries.sort_by_key(|e| e.id);

    let rest_minutes = paket.rahmen.mindestpause_min.or_else(|| {
        stand
            .planung
            .rahmen
            .as_ref()
            .map(|r| i64::from(r.pause))
            .filter(|p| *p > 0)
    });

    BtpSnapshot {
        tournament_name: paket.turnier.name.clone(),
        rest_minutes,
        matches,
        courts: felder.iter().map(|f| f.anzeige.clone()).collect(),
        locations,
        court_infos: felder
            .iter()
            .map(|f| BtpCourt {
                id: f.id,
                name: f.anzeige.clone(),
                location_id: Some(f.location_id),
                sort_order: f.sort_order,
            })
            .collect(),
        events,
        entries,
        officials: paket
            .schiedsrichter
            .iter()
            .map(|o| BtpOfficial {
                id: i64::from(o.id),
                name: o.name.clone(),
                first: String::new(),
                nationality: None,
            })
            .collect(),
    }
}
