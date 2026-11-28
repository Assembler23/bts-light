//! Turnierquelle „Punktspiel“: eine Ligabegegnung aus badhub (Live-Scoring
//! für Punktspiele, badhub ADR-0092, `docs/features/punktspiel.md`).
//!
//! Anders als ein badhub-Turnier braucht ein Punktspiel weder Turnierkern
//! noch Lease noch Ereignisprotokoll: badhub hält den Zustand (Aufstellung,
//! Feld, Sätze, Sieger je Einzelspiel), bts-light ist das Zählwerk.
//!
//! - **Lesen:** `GET /api/ls/v1/begegnung` liefert das Paket der Begegnung;
//!   [`abbilden`] macht daraus den `BtpSnapshot`, mit dem der Rest der App
//!   arbeitet (Tablets, Monitore, Liveticker). Abgefragt wird im Takt der
//!   Sync-Schleife mit `If-None-Match`.
//! - **Schreiben:** Feldvergabe, Ergebnis und — nach jedem Zyklus — der
//!   laufende Punktestand gehen als Zustandsmeldung je Spiel an
//!   `POST /api/ls/v1/begegnung/spiel`. Die Antwort ist das neue Paket.
//!
//! Auf der Platte liegt nur das Gerätetoken (verschlüsselt, wie bei den
//! badhub-Turnieren), unter `punktspiele/<uuid>/geraet.bin`. Das Paket wird
//! bei jedem Start frisch geholt — koppeln geht ohnehin nur mit Netz. Fällt
//! das Netz danach aus, gilt der zuletzt gelesene Stand weiter; schreiben
//! geht erst wieder mit Netz.
//!
//! **BTP- und badhub-Turniere laufen unverändert**: dieses Modul wird nur
//! über die Weichen in `quelle/mod.rs` erreicht.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::badhub::schluessel;
use crate::btp::model::{
    BtpCourt, BtpEntry, BtpEvent, BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult,
    MatchStatus, ScoringFormat,
};
use crate::btp::proto;
use crate::config::{AppConfig, PunktspielConfig};

/// Schema des Pakets, das diese Version versteht (badhub `Paket::SCHEMA`).
pub const SCHEMA: u32 = 1;

/// Obergrenze einer Antwort — ein Paket hat rund ein Dutzend Spiele.
const MAX_ANTWORT: usize = 1024 * 1024;

/// Paket einer Begegnung (`GET /api/ls/v1/begegnung`). Wird nicht
/// gespeichert, nur im Speicher gehalten.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Paket {
    pub schema: u32,
    pub begegnung: PaketBegegnung,
    /// Anzahl der Felder (1 bis 4).
    pub felder: i64,
    /// Zählweise als „<Sätze>x<Punkte>“, z. B. `3x21`.
    #[serde(default)]
    pub zaehlweise: String,
    pub spiele: Vec<PaketSpiel>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaketBegegnung {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub heim: String,
    #[serde(default)]
    pub gast: String,
    #[serde(default)]
    pub staffel: String,
    /// Vereine der beiden Mannschaften in badhub — für das Vereinslogo der
    /// Monitoransicht „Begegnung“ (ADR 0109). Ältere badhub-Stände liefern
    /// sie nicht; dann zeigt die Ansicht Initialen.
    #[serde(default, deserialize_with = "verein_id_tolerant")]
    pub heim_verein_id: Option<u32>,
    #[serde(default, deserialize_with = "verein_id_tolerant")]
    pub gast_verein_id: Option<u32>,
}

/// Liest eine Vereins-ID nachsichtig: Alles, was keine positive Zahl ist
/// (Text, negativ, zu groß, Kommazahl), wird zu `None`. Das Feld dient nur
/// dem Logo — eine unerwartete Form darf nicht das ganze Paket und damit
/// das Punktspiel lahmlegen.
fn verein_id_tolerant<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    let wert = serde_json::Value::deserialize(d)?;
    Ok(wert
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0))
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaketSpiel {
    /// Laufende Nummer in Spielreihenfolge — zugleich die Match-ID im Snapshot.
    pub nr: i64,
    /// Disziplin wie in nuLiga („1.HD“, „DD“, „2.HE“, „GD“, „OD“ …).
    pub disziplin: String,
    /// `single` oder `double`.
    #[serde(default)]
    pub art: String,
    #[serde(default)]
    pub heim: Vec<PaketPerson>,
    #[serde(default)]
    pub gast: Vec<PaketPerson>,
    #[serde(default)]
    pub feld: Option<i64>,
    /// `offen`, `laeuft` oder `beendet`.
    pub status: String,
    #[serde(default)]
    pub saetze: Vec<(i64, i64)>,
    /// `heim`, `gast` oder leer.
    #[serde(default)]
    pub sieger: Option<String>,
    /// `normal`, `kampflos`, `aufgabe` oder `disqualifikation`.
    #[serde(default)]
    pub ausgang: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaketPerson {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub vorname: String,
    #[serde(default)]
    pub nachname: String,
    #[serde(default)]
    pub lizenz: Option<String>,
}

impl Paket {
    /// Lässt sich das Paket verwenden? Sonst der Grund für die Anzeige.
    pub fn pruefen(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!(
                "badhub liefert das Punktspiel in einem neueren Format ({}). Bitte bts-light aktualisieren.",
                self.schema
            ));
        }
        if !(1..=8).contains(&self.felder) {
            return Err("Das Punktspiel hat keine gültige Felderzahl.".into());
        }
        Ok(())
    }
}

// ── Abbildung Paket → BtpSnapshot ───────────────────────────────────────────

/// Disziplin aus dem nuLiga-Kürzel („1.HD“ → Herrendoppel). Offene
/// Disziplinen (OD, OE) und Unbekanntes bleiben `Unknown`.
fn disziplin(name: &str) -> Discipline {
    let kurz: String = name
        .trim()
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
        .to_ascii_uppercase();
    match kurz.as_str() {
        "HE" | "JE" => Discipline::MensSingles,
        "DE" | "ME" => Discipline::WomensSingles,
        "HD" | "JD" => Discipline::MensDoubles,
        "DD" | "MD" => Discipline::WomensDoubles,
        "GD" | "MX" | "XD" => Discipline::Mixed,
        _ => Discipline::Unknown,
    }
}

/// Zählweise „3x21“ → Format. Unlesbares ergibt den Standard 3×21.
fn zaehlformat(z: &str) -> ScoringFormat {
    let mut teile = z.trim().split(['x', 'X']);
    let saetze = teile.next().and_then(|t| t.trim().parse::<i64>().ok());
    let punkte = teile.next().and_then(|t| t.trim().parse::<i64>().ok());
    match (saetze, punkte) {
        (Some(s), Some(21)) if (1..=5).contains(&s) => ScoringFormat {
            best_of: s,
            ..ScoringFormat::default()
        },
        (Some(s), Some(15)) if (1..=5).contains(&s) => ScoringFormat {
            best_of: s,
            target_score: 15,
            cap_score: 21,
            interval_at: Some(8),
        },
        (Some(s), Some(11)) if (1..=5).contains(&s) => ScoringFormat {
            best_of: s,
            target_score: 11,
            cap_score: 15,
            interval_at: None,
        },
        _ => ScoringFormat::default(),
    }
}

/// Eine Seite eines Spiels. Ist die Aufstellung der Seite unvollständig,
/// bleibt sie **leer**: ein Spiel mit leerer Seite gilt im ganzen Programm als
/// nicht spielbereit und wird weder von Hand noch automatisch aufgerufen. Ein
/// Platzhalter-Spieler wäre dagegen ein „Spieler“, den zwei Spiele teilen —
/// die Verfügbarkeitsprüfung hielte sie für gleichzeitig unmöglich.
fn seite(personen: &[PaketPerson], doppel: bool, mannschaft: &str) -> Vec<BtpPlayer> {
    let plaetze = if doppel { 2 } else { 1 };
    if personen.len() < plaetze {
        return Vec::new();
    }
    let verein = Some(mannschaft.trim().to_string()).filter(|m| !m.is_empty());
    personen
        .iter()
        .take(plaetze)
        .map(|p| BtpPlayer {
            id: p.id,
            name: p.name.trim().to_string(),
            first: p.vorname.trim().to_string(),
            last: if p.nachname.trim().is_empty() {
                p.name.trim().to_string()
            } else {
                p.nachname.trim().to_string()
            },
            member_id: p
                .lizenz
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
            nationality: None,
            club: verein.clone(),
        })
        .collect()
}

/// Bildet das Paket auf den Snapshot ab, mit dem Tablets, Monitore und
/// Liveticker arbeiten.
///
/// - Match-ID = Spielnummer, Feld-ID = Feldnummer (1 bis `felder`).
/// - Je Spiel eine eigene Klasse (`BtpEvent`) mit dem Disziplinnamen —
///   so zeigen alle Ansichten „1.HD“, „DE“ … als Klasse.
/// - Runde = Name der Begegnung.
pub fn abbilden(paket: &Paket) -> BtpSnapshot {
    let b = &paket.begegnung;
    let felder: Vec<i64> = (1..=paket.felder.clamp(1, 8)).collect();
    let scoring = zaehlformat(&paket.zaehlweise);
    let mut matches = Vec::with_capacity(paket.spiele.len());
    let mut events = Vec::with_capacity(paket.spiele.len());
    let mut entries = Vec::new();

    for s in &paket.spiele {
        let doppel = s.art != "single";
        let discipline = disziplin(&s.disziplin);
        let status = match s.status.as_str() {
            "beendet" => MatchStatus::Finished,
            "laeuft" if s.feld.is_some_and(|f| felder.contains(&f)) => MatchStatus::OnCourt,
            _ => MatchStatus::Scheduled,
        };
        let feld = s.feld.filter(|_| status == MatchStatus::OnCourt);
        let beendet = status == MatchStatus::Finished;
        let team1 = seite(&s.heim, doppel, &b.heim);
        let team2 = seite(&s.gast, doppel, &b.gast);
        let entry1_id = s.nr * 10 + 1;
        let entry2_id = s.nr * 10 + 2;

        events.push(BtpEvent {
            id: s.nr,
            name: s.disziplin.clone(),
            discipline,
        });
        for (id, team) in [(entry1_id, &team1), (entry2_id, &team2)] {
            if !team.is_empty() {
                entries.push(BtpEntry {
                    id,
                    event_id: s.nr,
                    players: team.clone(),
                    seed: None,
                });
            }
        }
        matches.push(BtpMatch {
            id: s.nr,
            draw_id: s.nr,
            planning_id: s.nr,
            display_order: Some(s.nr),
            highlight: 0,
            shuttles: 0,
            from1: None,
            from2: None,
            draw_name: s.disziplin.clone(),
            discipline,
            class_label: s.disziplin.clone(),
            round_name: b.name.clone(),
            match_num: Some(s.nr),
            planned_time: None,
            team1,
            team2,
            entry1_id,
            entry2_id,
            team1_seed: None,
            team2_seed: None,
            court: feld.map(|f| f.to_string()),
            court_id: feld,
            location_id: None,
            official1_id: None,
            official2_id: None,
            sets: if status == MatchStatus::Scheduled {
                Vec::new()
            } else {
                s.saetze.clone()
            },
            winner: match s.sieger.as_deref() {
                Some("heim") if beendet => Some(1),
                Some("gast") if beendet => Some(2),
                _ => None,
            },
            result: match s.ausgang.as_str() {
                "kampflos" if beendet => MatchResult::Walkover,
                "aufgabe" if beendet => MatchResult::Retired,
                "disqualifikation" if beendet => MatchResult::Disqualified,
                _ => MatchResult::Normal,
            },
            status,
            finished_at: None,
            pause_ms: None,
            preparation_call_ts: None,
            preparation_hall: None,
            preparation_call_rank: None,
            scoring: scoring.clone(),
        });
    }
    matches.sort_by_key(|m| m.id);

    BtpSnapshot {
        tournament_name: b.name.clone(),
        rest_minutes: None,
        matches,
        courts: felder.iter().map(|f| f.to_string()).collect(),
        locations: Vec::new(),
        court_infos: felder
            .iter()
            .map(|f| BtpCourt {
                id: *f,
                name: f.to_string(),
                location_id: None,
                sort_order: *f,
            })
            .collect(),
        events,
        entries,
        officials: Vec::new(),
    }
}

// ── Monitoransicht „Begegnung“ ──────────────────────────────────────────────

/// Bildet das Paket auf die Sicht der Monitoransicht „Begegnung“ ab (Spec
/// `punktspiel-monitoransicht`, ADR 0109).
///
/// Bewusst aus dem **Paket** und nicht aus dem Snapshot von [`abbilden`]:
/// Dort bleibt eine Seite mit unvollständiger Aufstellung leer, hier soll der
/// schon aufgestellte Doppelpartner sichtbar sein.
///
/// **Positivliste:** je Person nur der Name. Lizenznummer und Personen-ID
/// aus dem Paket gelangen nicht in die Sicht — sie geht an den Relay und
/// steht auf einem Hallen-TV.
pub fn sicht_aus(paket: &Paket) -> relay_proto::BegegnungSicht {
    use relay_proto::{BegegnungSeite, BegegnungStatus, BegegnungZeile, SetAb};
    let b = &paket.begegnung;
    let felder = 1..=paket.felder.clamp(1, 8);
    let scoring = zaehlformat(&paket.zaehlweise);
    let namen = |personen: &[PaketPerson], plaetze: usize| -> Vec<String> {
        personen
            .iter()
            .take(plaetze)
            .map(|p| p.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect()
    };
    let spiele = paket
        .spiele
        .iter()
        .map(|s| {
            let doppel = s.art != "single";
            let plaetze = if doppel { 2 } else { 1 };
            // Dieselbe Lesart wie `abbilden`: „läuft“ nur auf einem Feld,
            // das es gibt.
            let status = match s.status.as_str() {
                "beendet" => BegegnungStatus::Beendet,
                "laeuft" if s.feld.is_some_and(|f| felder.contains(&f)) => BegegnungStatus::Laeuft,
                _ => BegegnungStatus::Offen,
            };
            let beendet = status == BegegnungStatus::Beendet;
            BegegnungZeile {
                nr: s.nr,
                disziplin: s.disziplin.trim().to_string(),
                doppel,
                heim: namen(&s.heim, plaetze),
                gast: namen(&s.gast, plaetze),
                feld: s.feld.filter(|_| status == BegegnungStatus::Laeuft),
                status,
                saetze: if status == BegegnungStatus::Offen {
                    Vec::new()
                } else {
                    s.saetze.iter().map(|&(a, b)| SetAb { a, b }).collect()
                },
                sieger: match s.sieger.as_deref() {
                    Some("heim") if beendet => Some(BegegnungSeite::Heim),
                    Some("gast") if beendet => Some(BegegnungSeite::Gast),
                    _ => None,
                },
                ausgang: if beendet {
                    s.ausgang.trim().to_string()
                } else {
                    String::new()
                },
            }
        })
        .collect();
    let mut sicht = relay_proto::BegegnungSicht {
        heim: b.heim.trim().to_string(),
        gast: b.gast.trim().to_string(),
        staffel: b.staffel.trim().to_string(),
        heim_verein_id: b.heim_verein_id.filter(|id| *id > 0),
        gast_verein_id: b.gast_verein_id.filter(|id| *id > 0),
        best_of: scoring.best_of,
        ziel: scoring.target_score,
        kappe: scoring.cap_score,
        spiele,
    };
    // Dieselben Deckel wie am Relay — LAN und Cloud zeigen dasselbe.
    sicht.deckeln();
    sicht
}

/// Sicht des zuletzt gelesenen Pakets. `None`, solange kein Punktspiel
/// geladen oder noch kein Paket gelesen ist. Fragt badhub **nicht** — den
/// Abruf macht [`stand`] im Takt der Sync-Schleife.
pub fn begegnung_sicht() -> Option<relay_proto::BegegnungSicht> {
    zustand()
        .as_ref()
        .and_then(|z| z.paket.as_ref())
        .map(sicht_aus)
}

/// Was die Monitoransicht „Begegnung“ im LAN zeigt: die Sicht, dazu der
/// Live-Stand der Tablets. Dieselbe Funktion rechnet am Relay
/// (`relay_proto::begegnung_anzeige`); hier kommt der Live-Stand aus dem
/// Tablet-Zustand, zugeordnet über die Match-ID (= Spielnummer).
pub fn begegnung_anzeige(
    sicht: Option<&relay_proto::BegegnungSicht>,
    tablet: &crate::tablet::state::TabletState,
) -> relay_proto::BegegnungAnzeige {
    let Some(sicht) = sicht else {
        return relay_proto::BegegnungAnzeige::default();
    };
    let mut live: std::collections::HashMap<i64, relay_proto::BegegnungLive> =
        std::collections::HashMap::new();
    // Nach Feld sortiert: Tragen zwei Felder dieselbe Match-ID, gilt das
    // kleinere (wie am Relay).
    for spiel in tablet.laufende_saetze_alle() {
        live.entry(spiel.match_id)
            .or_insert_with(|| relay_proto::BegegnungLive {
                feld: Some(spiel.feld),
                saetze: spiel
                    .saetze
                    .into_iter()
                    .map(|(a, b)| relay_proto::SetAb { a, b })
                    .collect(),
            });
    }
    relay_proto::begegnung_anzeige(Some(sicht), |nr| live.get(&nr).cloned())
}

// ── Meldungen ───────────────────────────────────────────────────────────────

/// Zustand eines Spiels, wie ihn `POST /begegnung/spiel` erwartet. Immer der
/// ganze Zustand, nie eine Änderung — wiederholt gesendet ergibt er dasselbe.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meldung {
    pub nr: i64,
    pub status: &'static str,
    pub feld: Option<i64>,
    pub saetze: Vec<[i64; 2]>,
    pub sieger: Option<&'static str>,
    pub ausgang: &'static str,
}

fn saetze(s: &[(i64, i64)]) -> Vec<[i64; 2]> {
    s.iter().map(|(a, b)| [*a, *b]).collect()
}

/// Meldungen für eine Feldvergabe. Erst die Freigaben, dann die Aufrufe —
/// so wird ein Spiel beim Feldwechsel nicht versehentlich wieder „offen“.
///
/// - Aufruf (`match_id` gesetzt): das Spiel läuft auf dem Feld. Ein bereits
///   beendetes Spiel wird nicht wieder geöffnet.
/// - Freigabe (`match_id` leer): das Spiel, das auf dem Feld läuft, wird
///   wieder offen. Liegt dort kein laufendes Spiel (etwa nach dem Ergebnis),
///   ist nichts zu melden.
pub fn feld_meldungen(paket: &Paket, courts: &[proto::CourtAssignment]) -> Vec<Meldung> {
    let aufgerufen: Vec<i64> = courts.iter().filter_map(|c| c.match_id).collect();
    let mut meldungen = Vec::new();
    for c in courts.iter().filter(|c| c.match_id.is_none()) {
        for s in paket.spiele.iter().filter(|s| {
            s.status == "laeuft" && s.feld == Some(c.court_id) && !aufgerufen.contains(&s.nr)
        }) {
            meldungen.push(Meldung {
                nr: s.nr,
                status: "offen",
                feld: None,
                saetze: Vec::new(),
                sieger: None,
                ausgang: "normal",
            });
        }
    }
    for c in courts {
        let Some(id) = c.match_id else { continue };
        let Some(s) = paket.spiele.iter().find(|s| s.nr == id) else {
            continue;
        };
        if s.status == "beendet" || (s.status == "laeuft" && s.feld == Some(c.court_id)) {
            continue;
        }
        meldungen.push(Meldung {
            nr: s.nr,
            status: "laeuft",
            feld: Some(c.court_id),
            saetze: saetze(&s.saetze),
            sieger: None,
            ausgang: "normal",
        });
    }
    meldungen
}

/// Meldung für ein Ergebnis. `score_status` wie in BTP: 1 = kampflos,
/// 2 = Aufgabe, 3 = Disqualifikation, sonst regulär.
pub fn ergebnis_meldung(update: &proto::MatchUpdate) -> Meldung {
    Meldung {
        nr: update.btp_match_id,
        status: "beendet",
        feld: None,
        saetze: saetze(&update.sets),
        sieger: Some(if update.team1_won { "heim" } else { "gast" }),
        ausgang: match update.score_status {
            1 => "kampflos",
            2 => "aufgabe",
            3 => "disqualifikation",
            _ => "normal",
        },
    }
}

// ── HTTP ────────────────────────────────────────────────────────────────────

/// Fehler eines Aufrufs der Geräte-API.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiFehler {
    /// badhub nicht erreichbar (kein Netz, Zeitüberschreitung).
    Netz(String),
    /// 401: Token unbekannt, widerrufen oder abgelaufen.
    Zugang,
    /// 403: Die Begegnung ist in nuLiga bereits als gespielt eingetragen.
    Beendet,
    /// Der Kopplungscode wurde nicht angenommen.
    CodeAbgelehnt,
    /// Ein vorgelagerter Server verlangt eine eigene Anmeldung (Passwortschutz
    /// des Testsystems) — die Anfrage hat badhub gar nicht erreicht.
    Wand,
    /// 422: badhub lehnt die Meldung ab (Text von badhub).
    Abgelehnt(String),
    /// 429: zu viele Anfragen.
    Drossel,
    Status(u16),
    Format(String),
}

impl std::fmt::Display for ApiFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiFehler::Netz(e) => write!(f, "badhub ist nicht erreichbar ({e})."),
            ApiFehler::Zugang => write!(
                f,
                "badhub lehnt den Zugang ab. Die Kopplung wurde getrennt oder ist abgelaufen — in Mein Badhub unter Live-Scoring einen neuen Code erzeugen und neu koppeln."
            ),
            ApiFehler::Beendet => write!(
                f,
                "Die Begegnung ist in nuLiga bereits als gespielt eingetragen und lässt sich nicht mehr zählen."
            ),
            ApiFehler::CodeAbgelehnt => write!(
                f,
                "Der Kopplungscode ist falsch, abgelaufen oder schon eingelöst. In Mein Badhub unter Live-Scoring einen neuen erzeugen."
            ),
            ApiFehler::Wand => write!(
                f,
                "Der Server verlangt vor badhub eine eigene Anmeldung (Passwortschutz, etwa am Testsystem) — die Anfrage ist dort nicht angekommen. Das muss am Server freigegeben werden; am Kopplungscode liegt es nicht."
            ),
            ApiFehler::Abgelehnt(t) => write!(f, "badhub hat abgelehnt: {t}"),
            ApiFehler::Drossel => write!(f, "Zu viele Anfragen an badhub — bitte kurz warten."),
            ApiFehler::Status(s) => write!(f, "badhub antwortet mit Fehler {s}."),
            ApiFehler::Format(w) => write!(f, "Unerwartete Antwort von badhub ({w})."),
        }
    }
}

fn client(zeit: Duration) -> Result<reqwest::Client, ApiFehler> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(zeit)
        // Keine Weiterleitungen: ein https→http-Sprung schickte das Token
        // sonst im Klartext.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiFehler::Netz(e.to_string()))
}

fn netz(e: reqwest::Error) -> ApiFehler {
    ApiFehler::Netz(e.without_url().to_string())
}

async fn koerper(antwort: reqwest::Response) -> Result<Vec<u8>, ApiFehler> {
    let bytes = antwort.bytes().await.map_err(netz)?;
    if bytes.len() > MAX_ANTWORT {
        return Err(ApiFehler::Format("Antwort zu groß".into()));
    }
    Ok(bytes.to_vec())
}

fn status_fehler(status: reqwest::StatusCode) -> ApiFehler {
    match status.as_u16() {
        401 => ApiFehler::Zugang,
        403 => ApiFehler::Beendet,
        429 => ApiFehler::Drossel,
        s => ApiFehler::Status(s),
    }
}

fn paket_lesen(text: &[u8]) -> Result<Paket, ApiFehler> {
    let paket: Paket = serde_json::from_slice(text).map_err(|e| {
        ApiFehler::Format(format!(
            "Punktspiel, Zeile {} Spalte {}",
            e.line(),
            e.column()
        ))
    })?;
    paket.pruefen().map_err(ApiFehler::Format)?;
    Ok(paket)
}

#[derive(Serialize)]
struct KoppelnAnfrage<'a> {
    code: &'a str,
    install_id: &'a str,
    name: &'a str,
}

#[derive(Debug, Deserialize)]
struct KoppelnAntwort {
    token: String,
    begegnung: KoppelnBegegnung,
}

#[derive(Debug, Deserialize)]
struct KoppelnBegegnung {
    uuid: String,
    name: String,
}

async fn api_koppeln(
    basis: &str,
    code: &str,
    install_id: &str,
    name: &str,
) -> Result<KoppelnAntwort, ApiFehler> {
    let body = serde_json::to_vec(&KoppelnAnfrage {
        code,
        install_id,
        name,
    })
    .map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .post(format!("{basis}/koppeln"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        // Ein 401 mit „Basic“-Aufforderung kommt nicht von badhub, sondern von
        // einem Passwortschutz davor (test.badhub.de, 22.11.2026: sah aus wie
        // ein falscher Code und kostete eine Fehlersuche).
        let basic = antwort
            .headers()
            .get("WWW-Authenticate")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim_start().to_ascii_lowercase().starts_with("basic"));
        if status.as_u16() == 401 && basic {
            return Err(ApiFehler::Wand);
        }
        return Err(match status.as_u16() {
            400 | 401 | 403 | 404 | 409 | 410 | 422 => ApiFehler::CodeAbgelehnt,
            _ => status_fehler(status),
        });
    }
    serde_json::from_slice(&koerper(antwort).await?)
        .map_err(|_| ApiFehler::Format("Kopplung".into()))
}

/// Paket holen. `Ok(None)` = unverändert (304).
async fn api_begegnung(
    basis: &str,
    token: &str,
    etag: Option<&str>,
) -> Result<Option<(Paket, Option<String>)>, ApiFehler> {
    let mut anfrage = client(Duration::from_secs(15))?
        .get(format!("{basis}/begegnung"))
        .bearer_auth(token)
        .header("Accept", "application/json");
    if let Some(e) = etag {
        anfrage = anfrage.header("If-None-Match", e);
    }
    let antwort = anfrage.send().await.map_err(netz)?;
    let status = antwort.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let etag = antwort
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    Ok(Some((paket_lesen(&koerper(antwort).await?)?, etag)))
}

#[derive(Deserialize)]
struct FehlerAntwort {
    #[serde(default)]
    text: String,
}

async fn api_melden(
    basis: &str,
    token: &str,
    meldung: &Meldung,
) -> Result<(Paket, Option<String>), ApiFehler> {
    let body = serde_json::to_vec(meldung).map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .post(format!("{basis}/begegnung/spiel"))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if status.as_u16() == 422 {
        let text = serde_json::from_slice::<FehlerAntwort>(&koerper(antwort).await?)
            .map(|f| f.text)
            .unwrap_or_default();
        return Err(ApiFehler::Abgelehnt(text));
    }
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    let etag = antwort
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    Ok((paket_lesen(&koerper(antwort).await?)?, etag))
}

// ── Zustand ─────────────────────────────────────────────────────────────────

/// API-Basis aus der Push-Adresse (`https://badhub.de/api/live_update.php`
/// → `https://badhub.de/api/ls/v1`). Gleiche Regeln wie beim badhub-Turnier
/// (nur https, Klartext nur für localhost).
pub fn api_basis(push_url: &str) -> Result<String, String> {
    let torg = super::badhub::api_basis(push_url)?;
    Ok(format!(
        "{}/api/ls/v1",
        torg.trim_end_matches("/api/torg/v1")
    ))
}

fn ordner(daten: &Path) -> PathBuf {
    daten.join("punktspiele")
}

fn token_datei(daten: &Path, uuid: &str) -> Result<PathBuf, String> {
    if !super::badhub::uuid_gueltig(uuid) {
        return Err("Die Kennung der Begegnung ist ungültig.".into());
    }
    Ok(ordner(daten)
        .join(uuid.to_ascii_lowercase())
        .join("geraet.bin"))
}

fn token_speichern(daten: &Path, uuid: &str, token: &str) -> Result<(), String> {
    let datei = token_datei(daten, uuid)?;
    if let Some(dir) = datei.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let schluessel = schluessel::laden_oder_erzeugen(&ordner(daten).join("schluessel.bin"))?;
    schluessel::schreiben_privat(
        &datei,
        &schluessel::verschluesseln(&schluessel, token.as_bytes())?,
    )
}

fn token_laden(daten: &Path, uuid: &str) -> Option<String> {
    let datei = token_datei(daten, uuid).ok()?;
    let inhalt = std::fs::read(datei).ok()?;
    let schluessel = schluessel::laden_oder_erzeugen(&ordner(daten).join("schluessel.bin")).ok()?;
    String::from_utf8(schluessel::entschluesseln(&schluessel, &inhalt).ok()?).ok()
}

/// Geladenes Punktspiel: Zugang und der zuletzt gelesene Stand.
struct Zustand {
    basis: String,
    token: String,
    uuid: String,
    paket: Option<Paket>,
    etag: Option<String>,
    /// Zählt jede Meldung. Ein Abruf, der vor einer Meldung losging und
    /// danach ankommt, trägt einen älteren Stand und wird verworfen.
    meldungen: u64,
}

static ZUSTAND: Mutex<Option<Zustand>> = Mutex::new(None);

fn zustand() -> std::sync::MutexGuard<'static, Option<Zustand>> {
    ZUSTAND.lock().unwrap_or_else(PoisonError::into_inner)
}

const NICHT_GELADEN: &str =
    "Kein Punktspiel geladen — in der Einrichtung unter „Turnierquelle“ ein Punktspiel koppeln.";

/// Zugang und ETag für den nächsten Aufruf (ohne die Sperre über ein `await`
/// zu halten).
fn zugang() -> Result<(String, String, Option<String>, u64), String> {
    match zustand().as_ref() {
        Some(z) => Ok((
            z.basis.clone(),
            z.token.clone(),
            z.etag.clone(),
            z.meldungen,
        )),
        None => Err(NICHT_GELADEN.into()),
    }
}

/// Übernimmt ein Paket. `abruf_von` = Stand des Meldungszählers, als der
/// Abruf losging (`None` = Antwort auf eine eigene Meldung, immer aktuell).
fn uebernehmen(paket: Paket, etag: Option<String>, abruf_von: Option<u64>) {
    if let Some(z) = zustand().as_mut() {
        // Ein Paket einer anderen Begegnung kann nur ein Serverfehler sein —
        // nie anzeigen, was nicht gekoppelt ist.
        if !paket.begegnung.uuid.eq_ignore_ascii_case(&z.uuid) {
            return;
        }
        match abruf_von {
            // Seit dem Start des Abrufs kam eine Meldung dazwischen: der
            // Abruf ist älter als der Stand aus ihrer Antwort.
            Some(von) if von != z.meldungen => {}
            Some(_) => {
                z.paket = Some(paket);
                z.etag = etag;
            }
            None => {
                z.meldungen += 1;
                z.paket = Some(paket);
                z.etag = etag;
            }
        }
    }
}

/// Lädt das gekoppelte Punktspiel beim Start der Übertragung. `false`, wenn
/// keines gekoppelt ist oder das Gerätetoken fehlt.
pub fn starten(config: &AppConfig, daten: &Path) -> bool {
    let mut z = zustand();
    *z = None;
    let p = &config.punktspiel;
    if !p.gekoppelt() {
        return false;
    }
    // Die Basis kommt aus der Konfigurationsdatei: dieselbe Prüfung wie beim
    // Koppeln (nur https, Klartext nur lokal) — das Token reist in jeder Anfrage.
    if super::badhub::api_basis(&p.basis).is_err() {
        tracing::warn!(target: "punktspiel", "API-Adresse der Kopplung ungültig — bitte neu koppeln");
        return false;
    }
    let Some(token) = token_laden(daten, &p.uuid) else {
        tracing::warn!(target: "punktspiel", "Gerätetoken fehlt — bitte neu koppeln");
        return false;
    };
    *z = Some(Zustand {
        basis: p.basis.clone(),
        token,
        uuid: p.uuid.clone(),
        paket: None,
        etag: None,
        meldungen: 0,
    });
    true
}

/// Schlüssel der lokalen Stores (Zeiten, Aufrufe …): je Begegnung eigene.
pub fn turnier_schluessel() -> Option<String> {
    zustand()
        .as_ref()
        .map(|z| format!("punktspiel:{}", z.uuid.to_ascii_lowercase()))
}

pub fn schreiben_moeglich() -> bool {
    zustand().is_some()
}

/// Aktueller Stand. Fragt badhub (mit ETag) und fällt bei Netzfehlern auf
/// den zuletzt gelesenen Stand zurück; ein abgelehnter Zugang ist dagegen
/// immer ein Fehler — sonst zählte die Halle weiter, ohne dass etwas ankommt.
pub async fn stand() -> Result<BtpSnapshot, String> {
    let (basis, token, etag, von) = zugang()?;
    match api_begegnung(&basis, &token, etag.as_deref()).await {
        Ok(Some((paket, etag))) => uebernehmen(paket, etag, Some(von)),
        Ok(None) => {}
        Err(e @ (ApiFehler::Zugang | ApiFehler::Beendet)) => return Err(e.to_string()),
        Err(e) => {
            if zustand().as_ref().is_some_and(|z| z.paket.is_some()) {
                tracing::warn!(target: "punktspiel", "Abruf fehlgeschlagen, letzter Stand gilt weiter: {e}");
            } else {
                return Err(e.to_string());
            }
        }
    }
    match zustand().as_ref().and_then(|z| z.paket.as_ref()) {
        Some(p) => Ok(abbilden(p)),
        None => Err(NICHT_GELADEN.into()),
    }
}

/// Alle Meldungen an badhub gehen **nacheinander** hinaus, und jede wird erst
/// unter dieser Sperre aus dem dann aktuellen Paket berechnet. Eine Meldung
/// trägt immer den ganzen Zustand eines Spiels: Liefen Punktestand („läuft“)
/// und Ergebnis („beendet“) nebeneinander, könnte der später eintreffende
/// Punktestand das Ergebnis wieder aufheben — und das Ergebnis geht nur
/// einmal hinaus.
static SENDEN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Berechnet die Meldungen aus dem aktuellen Paket und sendet sie, alles
/// unter der Sende-Sperre.
async fn melden_aus(bauen: impl FnOnce(&Paket) -> Vec<Meldung>) -> Result<(), String> {
    let _sperre = SENDEN.lock().await;
    let paket = paket_kopie()?;
    for m in bauen(&paket) {
        let (basis, token, _, _) = zugang()?;
        let (paket, etag) = api_melden(&basis, &token, &m)
            .await
            .map_err(|e| e.to_string())?;
        uebernehmen(paket, etag, None);
    }
    Ok(())
}

fn paket_kopie() -> Result<Paket, String> {
    zustand()
        .as_ref()
        .and_then(|z| z.paket.clone())
        .ok_or_else(|| NICHT_GELADEN.to_string())
}

/// Feldvergabe an badhub melden.
pub async fn feld_vergabe(courts: &[proto::CourtAssignment]) -> Result<(), String> {
    melden_aus(|paket| feld_meldungen(paket, courts)).await
}

/// Ergebnis an badhub melden.
pub async fn ergebnis(update: &proto::MatchUpdate) -> Result<(), String> {
    melden_aus(|_| vec![ergebnis_meldung(update)]).await
}

/// Meldungen für den laufenden Punktestand: je laufendem Spiel eine, wenn der
/// Stand am Tablet von dem abweicht, den badhub zuletzt kennt. `stand_am_feld`
/// liefert zu einem Feld die Spielnummer und die Sätze vom Tablet.
pub fn punktestand_meldungen(
    paket: &Paket,
    stand_am_feld: impl Fn(i64) -> Option<(i64, Vec<(i64, i64)>)>,
) -> Vec<Meldung> {
    paket
        .spiele
        .iter()
        .filter(|s| s.status == "laeuft")
        .filter_map(|s| {
            let feld = s.feld?;
            let (nr, saetze_jetzt) = stand_am_feld(feld)?;
            if nr != s.nr || saetze_jetzt.is_empty() || saetze_jetzt == s.saetze {
                return None;
            }
            Some(Meldung {
                nr: s.nr,
                status: "laeuft",
                feld: Some(feld),
                saetze: saetze(&saetze_jetzt),
                sieger: None,
                ausgang: "normal",
            })
        })
        .collect()
}

/// Läuft gerade eine Punktestand-Meldung? Höchstens eine zugleich.
static PUNKTESTAND_UNTERWEGS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// Nach einem Fehler erst ab hier wieder versuchen — sonst wiederholte sich
/// eine abgelehnte oder gedrosselte Meldung in jedem Zyklus.
static PUNKTESTAND_PAUSE_BIS: Mutex<Option<std::time::Instant>> = Mutex::new(None);
const PUNKTESTAND_PAUSE: Duration = Duration::from_secs(30);

/// Meldet nach einem Zyklus den laufenden Punktestand an badhub — so steht er
/// öffentlich an der Begegnung, auch ohne Liveticker.
///
/// Der Zyklus wartet nicht darauf: gesendet wird in einer eigenen Aufgabe,
/// höchstens eine zugleich, nach einem Fehler mit Pause. Die Meldungen werden
/// erst unter der Sende-Sperre aus dem dann aktuellen Paket berechnet — ein
/// Spiel, dessen Ergebnis inzwischen gemeldet ist, bekommt keinen Punktestand
/// mehr.
pub fn punktestand_melden(tablet: &crate::tablet::state::TabletState) {
    use std::sync::atomic::Ordering;
    let jetzt = std::time::Instant::now();
    let pause = *PUNKTESTAND_PAUSE_BIS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if pause.is_some_and(|bis| jetzt < bis) {
        return;
    }
    let Ok(paket) = paket_kopie() else { return };
    // Stand der Tablets jetzt festhalten (die Aufgabe hat keinen Zugriff auf
    // den Tablet-Zustand); nichts zu melden → gar nicht erst starten.
    let staende: std::collections::HashMap<i64, (i64, Vec<(i64, i64)>)> =
        (1..=paket.felder.clamp(1, 8))
            .filter_map(|feld| tablet.laufende_saetze(feld).map(|s| (feld, s)))
            .collect();
    if punktestand_meldungen(&paket, |feld| staende.get(&feld).cloned()).is_empty() {
        return;
    }
    if PUNKTESTAND_UNTERWEGS.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        let ergebnis =
            melden_aus(|paket| punktestand_meldungen(paket, |feld| staende.get(&feld).cloned()))
                .await;
        if let Err(e) = ergebnis {
            tracing::warn!(target: "punktspiel", "Punktestand nicht gemeldet, nächster Versuch in {} s: {e}", PUNKTESTAND_PAUSE.as_secs());
            *PUNKTESTAND_PAUSE_BIS
                .lock()
                .unwrap_or_else(PoisonError::into_inner) =
                Some(std::time::Instant::now() + PUNKTESTAND_PAUSE);
        }
        PUNKTESTAND_UNTERWEGS.store(false, Ordering::SeqCst);
    });
}

// ── Punktverlauf (badhub ADR-0060) ──────────────────────────────────────────

/// Höchstens so viele Verläufe je Meldung (badhub `Durchfuehrung::VERLAUF_MAX_SPIELE`).
pub const VERLAUF_MAX_JE_MELDUNG: usize = 16;

/// Was badhub vom Punktverlauf zuletzt angenommen hat.
static VERLAUF_STAND: Mutex<Option<crate::badhub::verlauf::VerlaufStand>> = Mutex::new(None);
/// Zu welcher Begegnung `VERLAUF_STAND` gehört — eine neue Kopplung beginnt leer.
static VERLAUF_BEGEGNUNG: Mutex<String> = Mutex::new(String::new());
static VERLAUF_UNTERWEGS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static VERLAUF_PAUSE_BIS: Mutex<Option<std::time::Instant>> = Mutex::new(None);

#[derive(Serialize)]
struct VerlaufMeldung<'a> {
    matches: &'a [crate::badhub::verlauf::VerlaufEintrag],
}

#[derive(Deserialize)]
struct VerlaufAntwort {
    #[serde(default)]
    verworfen: u64,
}

async fn api_verlauf(
    basis: &str,
    token: &str,
    eintraege: &[crate::badhub::verlauf::VerlaufEintrag],
) -> Result<u64, ApiFehler> {
    let body = serde_json::to_vec(&VerlaufMeldung { matches: eintraege })
        .map_err(|e| ApiFehler::Format(e.to_string()))?;
    let antwort = client(Duration::from_secs(15))?
        .post(format!("{basis}/begegnung/verlauf"))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await
        .map_err(netz)?;
    let status = antwort.status();
    if !status.is_success() {
        return Err(status_fehler(status));
    }
    serde_json::from_slice::<VerlaufAntwort>(&koerper(antwort).await?)
        .map(|a| a.verworfen)
        .map_err(|e| ApiFehler::Format(e.to_string()))
}

/// Meldet nach einem Zyklus den Punktverlauf (Ballwechsel, Aufschlag, Pausen)
/// der Spiele an badhub — dieselben Einträge wie im Liveticker-Kanal
/// `tverlauf`, aber direkt an die Begegnung. So gibt es den Verlauf auch ohne
/// Liveticker (Punktspiel ohne Oberfläche).
///
/// Wie der Punktestand: eigene Aufgabe, höchstens eine zugleich, nach einem
/// Fehler mit Pause. Gesendet wird nur, was sich seit der letzten angenommenen
/// Meldung geändert hat.
pub fn verlauf_melden(snapshot: &BtpSnapshot, tablet: &crate::tablet::state::TabletState) {
    use crate::badhub::verlauf;
    use std::sync::atomic::Ordering;
    let pause = *VERLAUF_PAUSE_BIS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if pause.is_some_and(|bis| std::time::Instant::now() < bis) {
        return;
    }
    let Ok((basis, token, _, _)) = zugang() else {
        return;
    };
    let pausen: std::collections::HashMap<i64, serde_json::Value> = tablet
        .overview_from(snapshot)
        .into_iter()
        .filter_map(|c| c.pause.map(|p| (c.match_id, p)))
        .collect();
    let kandidaten = verlauf::kandidaten(
        snapshot,
        |id| tablet.timeline_store().marke(id),
        |id| tablet.sheet_store().marke(id),
        &pausen,
    );
    let (eintraege, gesendet) = {
        let mut begegnung = VERLAUF_BEGEGNUNG
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut stand = VERLAUF_STAND.lock().unwrap_or_else(PoisonError::into_inner);
        if *begegnung != snapshot.tournament_name || stand.is_none() {
            *begegnung = snapshot.tournament_name.clone();
            *stand = Some(verlauf::VerlaufStand::default());
        }
        let stand = stand.get_or_insert_with(Default::default);
        let mut eintraege = Vec::new();
        let mut gesendet = Vec::new();
        for k in stand
            .auswahl(&kandidaten)
            .into_iter()
            .take(VERLAUF_MAX_JE_MELDUNG)
        {
            let eintrag = snapshot
                .matches
                .iter()
                .find(|m| m.id == k.match_id)
                .and_then(|m| {
                    verlauf::eintrag_fuer(
                        m,
                        |id| tablet.timeline_store().timeline(id),
                        |id| tablet.sheet_store().sheet(id),
                        &pausen,
                    )
                });
            match eintrag {
                Some(e) => {
                    eintraege.push(e);
                    gesendet.push(k);
                }
                // nichts zu senden: gilt als erledigt, bis sich das Spiel ändert
                None => stand.angenommen(std::slice::from_ref(&k)),
            }
        }
        (eintraege, gesendet)
    };
    if eintraege.is_empty() || VERLAUF_UNTERWEGS.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        let ergebnis = api_verlauf(&basis, &token, &eintraege).await;
        {
            let mut stand = VERLAUF_STAND.lock().unwrap_or_else(PoisonError::into_inner);
            let stand = stand.get_or_insert_with(Default::default);
            match &ergebnis {
                Ok(0) => stand.angenommen(&gesendet),
                // badhub nennt nur die Zahl der verworfenen Einträge (Spiel
                // noch offen, Inhalt ungültig): nach einigen Versuchen geparkt
                Ok(_) => {
                    stand.verworfen(&gesendet);
                }
                Err(_) => {}
            }
        }
        if let Err(e) = ergebnis {
            tracing::warn!(target: "punktspiel", "Punktverlauf nicht gemeldet, nächster Versuch in {} s: {e}", PUNKTESTAND_PAUSE.as_secs());
            *VERLAUF_PAUSE_BIS
                .lock()
                .unwrap_or_else(PoisonError::into_inner) =
                Some(std::time::Instant::now() + PUNKTESTAND_PAUSE);
        }
        VERLAUF_UNTERWEGS.store(false, Ordering::SeqCst);
    });
}

/// Ergebnis einer Kopplung, für die Anzeige im Einrichtungsassistenten.
#[derive(Debug, Clone, Serialize)]
pub struct Kopplung {
    pub punktspiel: PunktspielConfig,
    pub spiele: usize,
    pub felder: i64,
    /// Spiele, deren Aufstellung noch nicht vollständig ist.
    pub offen: usize,
}

/// Löst einen Kopplungscode ein, legt das Gerätetoken verschlüsselt ab und
/// liest das Paket einmal zur Kontrolle.
pub async fn koppeln(
    basis: &str,
    daten: &Path,
    code: &str,
    install_id: &str,
    geraet_name: &str,
) -> Result<Kopplung, String> {
    let code: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .take(32)
        .collect();
    if code.is_empty() {
        return Err("Bitte den Kopplungscode eingeben.".into());
    }
    let antwort = api_koppeln(basis, &code, install_id, geraet_name)
        .await
        .map_err(|e| e.to_string())?;
    let uuid = antwort.begegnung.uuid.to_ascii_lowercase();
    // Die Kennung ist zugleich die GUID des Livetickers — nur eine echte GUID
    // gilt dort (sonst liefe der Ticker unter einem alten Turnier weiter).
    if !super::badhub::uuid_gueltig(&uuid) || crate::config::kanonische_guid(&uuid).is_none() {
        return Err(ApiFehler::Format("Kennung der Begegnung".into()).to_string());
    }
    let (paket, _) = api_begegnung(basis, &antwort.token, None)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| ApiFehler::Format("leeres Paket".into()).to_string())?;
    if !paket.begegnung.uuid.eq_ignore_ascii_case(&uuid) {
        return Err(ApiFehler::Format("Paket einer anderen Begegnung".into()).to_string());
    }
    token_speichern(daten, &uuid, &antwort.token)?;
    let offen = paket
        .spiele
        .iter()
        .filter(|s| {
            let plaetze = if s.art == "single" { 1 } else { 2 };
            s.heim.len() < plaetze || s.gast.len() < plaetze
        })
        .count();
    Ok(Kopplung {
        punktspiel: PunktspielConfig {
            uuid,
            name: antwort.begegnung.name,
            basis: basis.to_string(),
            geraet_name: geraet_name.trim().to_string(),
        },
        spiele: paket.spiele.len(),
        felder: paket.felder,
        offen,
    })
}

#[cfg(test)]
mod tests;
