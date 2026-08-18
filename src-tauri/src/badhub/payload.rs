//! Übersetzt einen `BtpSnapshot` in das `tset`-Payload-Format von badhub.de.
//!
//! Das Schema ist wire-kompatibel zum bestehenden Empfänger
//! `live_update.php` (Badhub-Repo, `docs/features/liveticker_bts.md`).
//!
//! Der `tset` umfasst Turniername, belegte Courts mit den laufenden
//! Matches, die zuletzt beendeten Matches und die anstehenden Matches.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpSnapshot, Discipline, MatchResult, MatchStatus};
use crate::config::AppConfig;
use crate::hall_colors::farbe_fuer;
use crate::tablet::queue_order::QueueOrderStore;

/// Kontext für die manuelle Spielreihenfolge (Spec
/// `spielliste-manuelle-reihenfolge`, ADR 0023) — nötig, um
/// [`upcoming`] mit **derselben** Sortierung wie die übrigen vier Stellen zu
/// bauen (`assign::resolve_and_sort_key`). `build_tset`/`diff`/`plan` kannten
/// bisher nur den `BtpSnapshot`; die manuelle Reihenfolge lebt aber im
/// `TabletState`, außerhalb des Snapshots — deshalb dieser zusätzliche,
/// schlanke Parameter statt eines direkten `&TabletState`-Zugriffs (der
/// `badhub`-Modul unnötig an `tablet` koppeln würde).
pub struct LivetickerContext<'a> {
    pub config: &'a AppConfig,
    /// Von Hand gesetzte Hallen (`TabletState::manual_halls`) — bereits als
    /// eigenständige, geklonte `HashMap` geliefert, kein Lifetime-Problem.
    pub manual_halls: HashMap<i64, String>,
    /// Automatisch vorverteilte Hallen (Spec `hallen-vorverteilung`) —
    /// gleiche Lieferform wie `manual_halls`.
    pub auto_halls: HashMap<i64, String>,
    pub order: &'a QueueOrderStore,
}

impl<'a> LivetickerContext<'a> {
    pub fn new(
        config: &'a AppConfig,
        manual_halls: HashMap<i64, String>,
        auto_halls: HashMap<i64, String>,
        order: &'a QueueOrderStore,
    ) -> Self {
        Self {
            config,
            manual_halls,
            auto_halls,
            order,
        }
    }

    /// Kontext ohne Präfix/Hallen-Overrides — für Aufrufer (Tests, Fixtures),
    /// denen die manuelle Reihenfolge egal ist. Reines `sort_key`-Verhalten.
    pub fn bare(config: &'a AppConfig) -> Self {
        static EMPTY_ORDER: OnceLock<QueueOrderStore> = OnceLock::new();
        Self {
            config,
            manual_halls: HashMap::new(),
            auto_halls: HashMap::new(),
            order: EMPTY_ORDER.get_or_init(QueueOrderStore::default),
        }
    }
}

/// Höchstzahl der beendeten Matches im `tset`. Großzügig bemessen, damit an
/// einem Turniertag praktisch alle Spiele erscheinen; deckelt nur extrem
/// große Turniere, damit das Payload nicht unbegrenzt wächst.
const FINISHED_LIMIT: usize = 500;
/// Höchstzahl der „in Vorbereitung"-Einträge.
const UPCOMING_LIMIT: usize = 15;

/// Eine `tset`-Nachricht für `live_update.php`.
#[derive(Debug, Serialize, PartialEq)]
pub struct TsetMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub event: TsetEvent,
    pub rid: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetEvent {
    pub tournament_name: String,
    pub courts: Vec<TsetCourt>,
    /// Aktuell auf einem Court laufende Matches.
    pub matches: Vec<TsetMatch>,
    /// Zuletzt beendete Matches (neueste zuerst).
    pub recent_finished_matches: Vec<TsetMatch>,
    /// Anstehende Matches (in Vorbereitung).
    pub upcoming_matches: Vec<TsetMatch>,
    /// Turnierlogo (Base64, ohne `data:`-Präfix) für badhubs `#live-logo`.
    /// Wird in `sync` aus der Config injiziert. Gleiche Feldnamen wie
    /// Original-BTS.
    ///
    /// **Drei Zustände, weil badhub drei unterscheidet** (Vertrag wie beim
    /// Check-In-Branding, badhub-PR #473):
    ///
    /// | Wert | Draht | Bedeutung für badhub |
    /// |---|---|---|
    /// | `None` | Feld fehlt | unverändert — behalte, was du hast |
    /// | `Some("")` | `""` | löschen |
    /// | `Some(daten)` | Base64 | setzen |
    ///
    /// Genau dafür ist `Option` hier nötig: Ein einfacher `String` könnte
    /// „unverändert" nicht ausdrücken, und ein leerer String hieße Löschen.
    /// Das Weglassen ist die eigentliche Ersparnis — das Logo wiegt bis zu
    /// 2,7 MB und ginge sonst in **jedem** vollen `tset` erneut hinaus,
    /// mindestens minütlich als Lebenszeichen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_logo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_logo_mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_logo_background_color: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetCourt {
    /// Court-Bezeichnung wie in BTP (z. B. "1" oder "Feld 9").
    pub num: String,
    /// Halle/Standort des Felds (BTP-`Location`-Name). Leer bei
    /// Ein-Hallen-Turnieren – der Liveticker-Monitor gruppiert erst, wenn
    /// die Halle gesetzt ist.
    pub hall: String,
    /// Hallen-Farbe (Hex `#rrggbb`, Spec hallen-farben) für den
    /// `display=monitor`-Aushang. Fehlt bei Ein-Hallen-Turnieren komplett
    /// im JSON — alte badhub-Parser sehen den bisherigen Payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hall_color: Option<String>,
    /// Verweist auf `TsetMatch._id`.
    pub match_id: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TsetMatch {
    #[serde(rename = "_id")]
    pub id: String,
    /// Anzeigename, z. B. "HE G1".
    pub n: String,
    /// Satz-Ergebnisse als `[Team1, Team2]`-Punktepaare.
    pub s: Vec<[i64; 2]>,
    pub p0: Vec<String>,
    pub p0_member_ids: Vec<Option<String>>,
    pub p0_nationalities: Vec<Option<String>>,
    pub p1: Vec<String>,
    pub p1_member_ids: Vec<Option<String>>,
    pub p1_nationalities: Vec<Option<String>>,
    /// Ende-Zeitstempel (nur bei beendeten Matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ts: Option<u64>,
    /// Hat Team 1 gewonnen? (nur bei beendeten Matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team1_won: Option<bool>,
    /// Spielnummer (nur bei anstehenden Matches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_num: Option<i64>,
    /// Disziplin als stabiler Schlüssel (`mens_singles`, `womens_doubles`, …).
    ///
    /// **Warum auch im `tset`:** Der Liveticker liest `n` (= `draw_name +
    /// round_name`), und `draw_name` ist bei Gruppenturnieren die
    /// AUSLOSUNGSGRUPPE ("Gruppe 1") — die Disziplin kam dort nie an. Sie
    /// ging bisher nur im `sched` raus, der die Spielerseite speist.
    /// `None` bei `Discipline::Unknown`, dann bleibt das Feld wie bisher weg.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discipline: Option<&'static str>,
    /// Klassenkürzel ("A", "B", "U15"); `None`, wenn keins erkennbar ist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_label: Option<String>,
    /// Nicht-regulärer Ausgang: "walkover" | "retired" | "disqualified".
    /// Fehlt bei regulär ausgespielten Matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<&'static str>,
    /// Zeitpunkt (Unix-Millisekunden), zu dem die Turnierleitung das Match
    /// „in Vorbereitung" gerufen hat (nur bei anstehenden Matches). Der
    /// `display=next`-Monitor zeigt damit „vor X Min aufgerufen". Der
    /// Wire-Feldname `preparation_call_ts` wird von badhub.de wörtlich
    /// gelesen – nicht umbenennen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation_call_ts: Option<u64>,
    /// Halle, für die der Aufruf gilt (nur bei hallenweise gerufenen
    /// Matches). Fehlt bei hallenunabhängigen Aufrufen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hall: Option<String>,
    /// Hallen-Farbe zum `hall`-Feld (Hex, Spec hallen-farben) für den
    /// `display=next`-Aushang. Fehlt ohne Halle und bei
    /// Ein-Hallen-Turnieren.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hall_color: Option<String>,
}

/// Stabile, turnierweit eindeutige Match-ID für den Badhub-Payload.
fn match_id(btp_match_id: i64) -> String {
    format!("btp_{btp_match_id}")
}

/// Basis-Konvertierung – ohne die zustandsabhängigen Zusatzfelder.
fn to_tset_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        id: match_id(m.id),
        n: format!("{} {}", m.draw_name, m.round_name)
            .trim()
            .to_string(),
        s: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
        p0: m.team1.iter().map(|p| p.name.clone()).collect(),
        p0_member_ids: m.team1.iter().map(|p| p.member_id.clone()).collect(),
        p0_nationalities: m.team1.iter().map(|p| p.nationality.clone()).collect(),
        p1: m.team2.iter().map(|p| p.name.clone()).collect(),
        p1_member_ids: m.team2.iter().map(|p| p.member_id.clone()).collect(),
        p1_nationalities: m.team2.iter().map(|p| p.nationality.clone()).collect(),
        end_ts: None,
        team1_won: None,
        match_num: None,
        discipline: match m.discipline {
            Discipline::Unknown => None,
            d => Some(d.as_str()),
        },
        class_label: if m.class_label.is_empty() {
            None
        } else {
            Some(m.class_label.clone())
        },
        outcome: None,
        preparation_call_ts: None,
        hall: None,
        hall_color: None,
    }
}

/// Effektive Hallen-Farben des Turniers (Spec hallen-farben) — leer bei
/// Ein-Hallen-Turnieren, dann fehlen die Felder komplett im Payload.
fn hallen_farben(snapshot: &BtpSnapshot, cfg: &AppConfig) -> Vec<(String, String)> {
    let hallen: Vec<String> = snapshot.locations.iter().map(|l| l.name.clone()).collect();
    crate::hall_colors::effective_hall_colors(cfg, &hallen)
}

/// Payload-Wert für die Ergebnisart; `None` bei regulärem Ausgang.
fn outcome_str(result: MatchResult) -> Option<&'static str> {
    match result {
        MatchResult::Normal => None,
        MatchResult::Walkover => Some("walkover"),
        MatchResult::Retired => Some("retired"),
        MatchResult::Disqualified => Some("disqualified"),
    }
}

/// Konvertierung für ein beendetes Match (mit Ende-Zeit, Sieger, Ausgang).
fn to_finished_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        end_ts: m.finished_at,
        team1_won: m.winner.map(|w| w == 1),
        outcome: outcome_str(m.result),
        ..to_tset_match(m)
    }
}

/// Konvertierung für ein anstehendes Match (mit Spielnummer und – falls
/// von der Turnierleitung gerufen – Vorbereitungs-Zeitstempel und Halle).
fn to_upcoming_match(m: &BtpMatch) -> TsetMatch {
    TsetMatch {
        match_num: m.match_num,
        preparation_call_ts: m.preparation_call_ts,
        hall: m.preparation_hall.clone(),
        ..to_tset_match(m)
    }
}

/// Alle beendeten Matches des laufenden Turniertags, neueste zuerst.
///
/// `finished_at` wird ausschließlich während des laufenden bts-light-Betriebs
/// gesetzt – die Liste umfasst damit alle an diesem Tag gespielten Matches.
/// `FINISHED_LIMIT` greift nur als Schutz bei extrem großen Turnieren.
fn recent_finished(snapshot: &BtpSnapshot) -> Vec<TsetMatch> {
    let mut finished: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Finished && m.winner.is_some())
        .filter(|m| m.finished_at.is_some())
        .collect();
    finished.sort_by_key(|m| std::cmp::Reverse(m.finished_at));
    finished.truncate(FINISHED_LIMIT);
    finished.iter().map(|m| to_finished_match(m)).collect()
}

/// Anstehende Matches (geplant, noch nicht auf Court, mit Spielern), max. 15.
fn upcoming(snapshot: &BtpSnapshot, ctx: &LivetickerContext) -> Vec<TsetMatch> {
    let mut scheduled: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Scheduled)
        .filter(|m| !m.team1.is_empty() || !m.team2.is_empty())
        .collect();
    // **Dieselbe Reihenfolge wie überall sonst** (`assign::resolve_and_sort_key`,
    // ADR 0023): gerufene zuerst, dann der manuelle Präfix je Halle, sonst
    // die Ansetzung des Turnierplans, erst danach die Spielnummer. Der
    // Liveticker ist die Ansicht mit den meisten Augen — zeigte er andere
    // „nächste Spiele" als der Plan der Turnierleitung, stünden Zuschauer am
    // falschen Feld, und bei nur 15 Einträgen fielen die tatsächlich
    // nächsten Spiele ganz heraus.
    scheduled.sort_by_key(|m| {
        let manual_hall = ctx.manual_halls.get(&m.id).map(String::as_str);
        let called_hall = m.preparation_hall.as_deref();
        let auto_hall = ctx.auto_halls.get(&m.id).map(String::as_str);
        let (_, _, key) = crate::tablet::assign::resolve_and_sort_key(
            ctx.config,
            snapshot,
            m,
            manual_hall,
            called_hall,
            auto_hall,
            m.preparation_call_ts.is_some(),
            ctx.order,
        );
        key
    });
    scheduled.truncate(UPCOMING_LIMIT);
    // Hallen-Farbe zum Aufruf (Spec hallen-farben) — einmal auflösen.
    let farben = hallen_farben(snapshot, ctx.config);
    scheduled
        .iter()
        .map(|m| {
            let mut t = to_upcoming_match(m);
            t.hall_color = t.hall.as_deref().and_then(|h| farbe_fuer(&farben, h));
            t
        })
        .collect()
}

/// Vollständiger Spielplan für badhub (Nachricht `sched`).
///
/// Zweiter Kanal neben `tset`, bewusst getrennt: der `tset` geht bei jeder
/// Liveticker-Änderung raus und trägt bereits das Base64-Turnierlogo. Ihn um
/// mehrere hundert Spiele zu erweitern, würde den Liveticker für alle
/// langsamer machen, damit eine Spielerseite vollständig ist.
///
/// Spezifikation im badhub-Repo:
/// `docs/superpowers/specs/2026-08-16-spieler-live-vollstaendiger-spielplan-design.md`
#[derive(Debug, Serialize, PartialEq)]
pub struct SchedMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub rid: u64,
    pub event: SchedEvent,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SchedEvent {
    pub tournament_name: String,
    /// ALLE Spiele mit Teilnehmern — keine Kappung. Das ist der Zweck des Kanals.
    pub matches: Vec<SchedMatch>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SchedMatch {
    #[serde(rename = "_id")]
    pub id: String,
    pub n: String,
    /// `scheduled` · `oncourt` · `finished`.
    pub status: &'static str,
    pub p0: Vec<String>,
    pub p0_member_ids: Vec<Option<String>>,
    pub p1: Vec<String>,
    pub p1_member_ids: Vec<Option<String>>,
    /// BTP `PlannedTime` als **Unix-ms**, nicht als YYYYMMDDHHMM: der
    /// Empfänger soll keine Zeitzone interpretieren müssen.
    pub planned_ts: Option<u64>,
    /// Prognose aus `tablet::predict`, nur bei `scheduled`.
    pub predicted_start_ts: Option<u64>,
    /// Position INNERHALB der Halle, 0-basiert, aus `resolve_and_sort_key`.
    pub queue_pos: Option<i64>,
    pub hall: Option<String>,
    /// Bei `scheduled` immer `None` — das Feld steht erst beim Aufruf fest.
    pub court: Option<String>,
    /// Farbmarke der Halle (`#rrggbb`), gleiche Quelle wie im `tset`.
    /// badhub zeigt damit dieselbe Marke wie im Liveticker.
    pub hall_color: Option<String>,
    /// Disziplin als stabiler Schlüssel (`mens_singles`, `womens_doubles`, …).
    ///
    /// **Warum das nötig ist:** `n` ist `draw_name + round_name` — und
    /// `draw_name` ist bei Gruppenturnieren die AUSLOSUNGSGRUPPE
    /// ("Gruppe 1"), nicht die Klasse. badhub konnte daraus die Disziplin
    /// nicht ableiten, egal was es tat.
    pub discipline: &'static str,
    /// Klassenkürzel ("A", "B", "U15"); `None`, wenn keins erkennbar ist.
    /// Zusammen mit `discipline` ergibt das "HE A".
    pub class_label: Option<String>,
    pub sets: Vec<[i64; 2]>,
    pub team1_won: Option<bool>,
    pub end_ts: Option<u64>,
    pub outcome: Option<&'static str>,
}

/// BTP `PlannedTime` (`YYYYMMDDHHMM`) → Unix-ms.
///
/// Die Zahl ist **lokale Wandzeit**, keine UTC — sie kommt aus einem
/// Turnierplan, den jemand in einer Halle aufgestellt hat, und der Rechner
/// steht am selben Ort (dasselbe `Local`-Muster wie `btp/proto.rs`). Ohne die
/// Zonen-Zuordnung läge im Sommer jede Anwurfzeit zwei Stunden daneben.
///
/// `None` bei unplausibler Zahl und bei mehrdeutiger Wandzeit (Zeitumstellung):
/// dann lieber keine Zeit senden als eine falsche — badhub zeigt das Feld
/// einfach nicht an.
fn planned_time_to_unix_ms(pt: i64) -> Option<u64> {
    use chrono::{Local, NaiveDate, TimeZone};

    let minute = (pt % 100) as u32;
    let rest = pt / 100;
    let stunde = (rest % 100) as u32;
    let rest = rest / 100;
    let tag = (rest % 100) as u32;
    let rest = rest / 100;
    let monat = (rest % 100) as u32;
    let jahr = (rest / 100) as i32;

    let naiv = NaiveDate::from_ymd_opt(jahr, monat, tag)?.and_hms_opt(stunde, minute, 0)?;

    Local
        .from_local_datetime(&naiv)
        .single()
        .map(|dt| dt.timestamp_millis() as u64)
}

/// Baut die `sched`-Nachricht: **alle** Spiele mit Teilnehmern, ohne Kappung.
///
/// `predicted` kommt aus `TabletState::predicted_starts_snapshot()` und wird
/// nur für wartende Spiele durchgereicht — bei einem laufenden oder beendeten
/// Spiel ist „wann bin ich dran" sinnlos, und ein stehengebliebener Wert wäre
/// schlimmer als keiner.
pub fn build_sched(
    snapshot: &BtpSnapshot,
    ctx: &LivetickerContext,
    predicted: &HashMap<i64, u64>,
    rid: u64,
) -> SchedMessage {
    // Nur Spiele mit Teilnehmern: leere Platzhalter einer noch nicht
    // ausgelosten Runde helfen auf einer Spielerseite niemandem.
    let mut relevant: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| !m.team1.is_empty() || !m.team2.is_empty())
        .collect();

    // **Dieselbe Reihenfolge wie überall sonst** (`assign::resolve_and_sort_key`,
    // ADR 0023) — siehe die ausführliche Begründung in `upcoming()`. Eine
    // eigene Sortierung für badhub würde Zuschauer ans falsche Feld schicken.
    relevant.sort_by_key(|m| sortier_schluessel(snapshot, ctx, m).1);

    // queue_pos zählt INNERHALB der Halle: „in 3 Spielen" beantwortet die
    // Frage „wie viele Spiele laufen vor mir auf meinen Feldern", nicht „wie
    // viele im ganzen Turnier".
    let mut je_halle: HashMap<String, i64> = HashMap::new();
    let farben = hallen_farben(snapshot, ctx.config);

    let matches = relevant
        .iter()
        .map(|m| {
            let wartend = m.status == MatchStatus::Scheduled;
            let halle = sortier_schluessel(snapshot, ctx, m).0;
            // Dieselbe Farbquelle wie im tset - eine zweite Zuordnung waere
            // eine zweite Wahrheit, und badhub zeigt beide Marken nebeneinander.
            let halle_farbe = if halle.is_empty() {
                None
            } else {
                farbe_fuer(&farben, &halle)
            };
            let queue_pos = if wartend {
                let zaehler = je_halle.entry(halle.clone()).or_insert(0);
                let pos = *zaehler;
                *zaehler += 1;
                Some(pos)
            } else {
                None
            };

            SchedMatch {
                id: match_id(m.id),
                n: format!("{} {}", m.draw_name, m.round_name)
                    .trim()
                    .to_string(),
                status: match m.status {
                    MatchStatus::Finished => "finished",
                    MatchStatus::OnCourt => "oncourt",
                    _ => "scheduled",
                },
                p0: m.team1.iter().map(|p| p.name.clone()).collect(),
                p0_member_ids: m.team1.iter().map(|p| p.member_id.clone()).collect(),
                p1: m.team2.iter().map(|p| p.name.clone()).collect(),
                p1_member_ids: m.team2.iter().map(|p| p.member_id.clone()).collect(),
                planned_ts: m.planned_time.and_then(planned_time_to_unix_ms),
                predicted_start_ts: if wartend {
                    predicted.get(&m.id).copied()
                } else {
                    None
                },
                queue_pos,
                hall: if halle.is_empty() { None } else { Some(halle) },
                court: if wartend { None } else { m.court.clone() },
                hall_color: halle_farbe.clone(),
                discipline: m.discipline.as_str(),
                class_label: if m.class_label.is_empty() {
                    None
                } else {
                    Some(m.class_label.clone())
                },
                sets: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
                team1_won: m.winner.map(|w| w == 1),
                end_ts: m.finished_at,
                outcome: outcome_str(m.result),
            }
        })
        .collect();

    SchedMessage {
        kind: "sched",
        rid,
        event: SchedEvent {
            tournament_name: snapshot.tournament_name.clone(),
            matches,
        },
    }
}

/// (Halle, Sortierschlüssel) eines Matches — ein Aufruf für beides, damit
/// Reihenfolge und Hallenzuordnung nicht auseinanderlaufen können.
fn sortier_schluessel(
    snapshot: &BtpSnapshot,
    ctx: &LivetickerContext,
    m: &BtpMatch,
) -> (String, crate::tablet::assign::ManualOrderSortKey) {
    let (halle, _quelle, key) = crate::tablet::assign::resolve_and_sort_key(
        ctx.config,
        snapshot,
        m,
        ctx.manual_halls.get(&m.id).map(String::as_str),
        m.preparation_hall.as_deref(),
        ctx.auto_halls.get(&m.id).map(String::as_str),
        m.preparation_call_ts.is_some(),
        ctx.order,
    );
    (halle, key)
}

/// Baut die `tset`-Nachricht aus einem Snapshot.
pub fn build_tset(snapshot: &BtpSnapshot, rid: u64, ctx: &LivetickerContext) -> TsetMessage {
    let on_court: Vec<&BtpMatch> = snapshot
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::OnCourt)
        .collect();

    let farben = hallen_farben(snapshot, ctx.config);
    let courts = on_court
        .iter()
        .filter_map(|m| {
            m.court.as_ref().map(|c| {
                // Halle des Felds für den Liveticker-Hallen-Monitor; bei
                // Ein-Hallen-Turnieren leer.
                let hall = m
                    .court_id
                    .map(|id| snapshot.court_location_name(id))
                    .unwrap_or_default();
                TsetCourt {
                    num: c.clone(),
                    hall_color: farbe_fuer(&farben, &hall),
                    hall,
                    match_id: match_id(m.id),
                }
            })
        })
        .collect();

    TsetMessage {
        kind: "tset",
        event: TsetEvent {
            tournament_name: snapshot.tournament_name.clone(),
            courts,
            matches: on_court.iter().map(|m| to_tset_match(m)).collect(),
            recent_finished_matches: recent_finished(snapshot),
            upcoming_matches: upcoming(snapshot, ctx),
            // Logo wird erst im Sync-Loop aus der Config gefüllt (build_tset
            // kennt die Config nicht) – hier offen lassen. `None` heißt auf
            // dem Draht „Feld fehlt", also „unverändert".
            tournament_logo: None,
            tournament_logo_mime: None,
            tournament_logo_background_color: None,
        },
        rid,
    }
}

/// Eine kleine `tupdate_match`-Nachricht – nur Match-ID und Satzstand.
/// Wird gesendet, wenn sich ausschließlich der Punktestand geändert hat.
#[derive(Debug, Serialize, PartialEq)]
pub struct TupdateMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(rename = "match")]
    pub match_update: TupdateMatch,
    pub rid: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TupdateMatch {
    #[serde(rename = "_id")]
    pub id: String,
    pub s: Vec<[i64; 2]>,
}

/// Baut eine `tupdate_match`-Nachricht für ein Match mit geändertem Score.
pub fn build_tupdate(m: &BtpMatch, rid: u64) -> TupdateMessage {
    TupdateMessage {
        kind: "tupdate_match",
        match_update: TupdateMatch {
            id: match_id(m.id),
            s: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
        },
        rid,
    }
}

// --- Hallen-Check-In: Meldeliste (ADR 0009) -------------------------------

/// Eine `centry_list`-Nachricht: die Meldeliste des Turniers je Spielklasse.
///
/// Anders als `tset` hängt sie **nicht** an Courts oder Matches, sondern an
/// den BTP-`Entries` — sie steht deshalb schon vor der Auslosung bereit.
/// Adressiert wird über die turnier.de-Turnier-GUID; authentifiziert wird wie
/// bei `tset` über das Liveticker-Passwort (ADR 0009).
#[derive(Debug, Serialize, PartialEq)]
pub struct CheckinRosterMessage {
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// turnier.de-Turnier-GUID — sagt badhub, zu welchem Turnier die Liste
    /// gehört.
    pub tournament_uuid: String,
    /// Turniername zur Anzeige auf der Check-In-Seite.
    pub tournament_name: String,
    /// Spielklassen des Turniers.
    pub classes: Vec<RosterClass>,
    /// Meldungen, klassenweise auflösbar über `event_id`.
    pub entries: Vec<RosterEntry>,
    pub rid: u64,
}

/// Branding (Sponsoren + Turnierlogo) für den badhub-Check-In (Phase 3 der
/// Sponsor-Leiste). `sponsors` sind roh-Base64-Rasterbilder (jpg/png/gif,
/// max. 4); `logo` ist das roh-Base64-Turnierlogo (png/jpg/webp). **Keine GUID
/// im Body**: anders als die Meldeliste adressiert dieser Endpunkt das Turnier
/// allein über das Bearer-Liveticker-Passwort (badhub löst `tournament_key` →
/// Check-In-UUID auf).
///
/// **Feld-unabhängig:** Ein `None`-Feld wird nicht gesendet → badhub lässt es
/// unberührt; `Some(…)` ersetzt es (leerer String / leere Liste = löschen). So
/// senden bts-lights zwei Auslöser je nur ihr Feld: das Sponsor-Häkchen die
/// Sponsoren, das Speichern der Einstellungen das (geänderte) Logo — ohne das
/// jeweils andere neu über die Leitung zu schicken (das Logo ist bis 2 MB groß).
#[derive(Debug, Serialize, PartialEq)]
pub struct CheckinBrandingMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sponsors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

/// Eine Spielklasse in der Meldeliste.
#[derive(Debug, Serialize, PartialEq)]
pub struct RosterClass {
    pub event_id: i64,
    /// Anzeigename der Klasse, wie in BTP gepflegt.
    pub name: String,
    /// Disziplin als snake_case-Schlüssel (`mens_singles`, `mixed` …);
    /// badhub lokalisiert selbst.
    pub discipline: Discipline,
}

/// Eine Meldung: ein Spieler bei Einzel, zwei bei Doppel.
#[derive(Debug, Serialize, PartialEq)]
pub struct RosterEntry {
    pub entry_id: i64,
    pub event_id: i64,
    pub players: Vec<RosterPlayer>,
}

/// Ein gemeldeter Spieler.
///
/// Bewusst **kein Geburtsjahr** (Projektregel) — auch nicht mittelbar. Verein
/// und Nationalität sind die einzigen Unterscheidungsmerkmale bei
/// Namensgleichheit und in BTP optional; fehlen sie, werden die Felder
/// weggelassen statt leer gesendet.
#[derive(Debug, Serialize, PartialEq)]
pub struct RosterPlayer {
    /// BTP-`PlayerID` — der Schlüssel, unter dem der Check-In gespeichert
    /// wird. Innerhalb eines Turniers stabil und immer vorhanden.
    pub player_id: i64,
    pub first: String,
    pub last: String,
    /// Lizenznummer (BTP `MemberID`), falls gepflegt. Brücke zu badhubs
    /// `players.dbv_licence_nr` — nötig fürs Anonymisierungs-Gate, aber nie
    /// Pflicht: ein Turnier ohne Lizenznummern funktioniert vollständig.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub club: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nationality: Option<String>,
}

impl CheckinRosterMessage {
    /// Inhaltsgleich zu einer zuvor gesendeten Meldeliste?
    ///
    /// Die `rid` bleibt außen vor — sie ist nur die laufende Nachrichtennummer
    /// und ändert sich bei jedem Zyklus. Verglichen wird bewusst die
    /// **Nachricht selbst** statt eines eigenen Fingerabdrucks: ein zweites
    /// Feldschema würde beim nächsten zusätzlichen Payload-Feld stillschweigend
    /// auseinanderlaufen, und die Meldeliste wäre dann nicht mehr aktuell.
    pub fn same_content_as(&self, other: &Self) -> bool {
        self.tournament_uuid == other.tournament_uuid
            && self.tournament_name == other.tournament_name
            && self.classes == other.classes
            && self.entries == other.entries
    }
}

/// Baut die `centry_list`-Nachricht aus dem Snapshot.
///
/// Klassen ohne jede Meldung werden weggelassen — sie hätten auf der
/// Check-In-Seite eine leere Liste ergeben, in die niemand einchecken kann.
pub fn build_checkin_roster(
    snapshot: &BtpSnapshot,
    tournament_uuid: &str,
    rid: u64,
) -> CheckinRosterMessage {
    let entries: Vec<RosterEntry> = snapshot
        .entries
        .iter()
        .map(|e| RosterEntry {
            entry_id: e.id,
            event_id: e.event_id,
            players: e
                .players
                .iter()
                .map(|p| RosterPlayer {
                    player_id: p.id,
                    first: p.first.clone(),
                    last: p.last.clone(),
                    member_id: p.member_id.clone(),
                    club: p.club.clone(),
                    nationality: p.nationality.clone(),
                })
                .collect(),
        })
        .collect();

    let classes: Vec<RosterClass> = snapshot
        .events
        .iter()
        .filter(|ev| entries.iter().any(|e| e.event_id == ev.id))
        .map(|ev| RosterClass {
            event_id: ev.id,
            name: ev.name.clone(),
            discipline: ev.discipline,
        })
        .collect();

    CheckinRosterMessage {
        kind: "centry_list",
        tournament_uuid: tournament_uuid.trim().to_string(),
        tournament_name: snapshot.tournament_name.clone(),
        classes,
        entries,
        rid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpCourt, BtpEntry, BtpEvent, BtpLocation, BtpPlayer, Discipline};

    /// Fester Bezugszeitpunkt für die Tests.
    const NOW: u64 = 1_700_000_000_000;

    fn player(name: &str, member: Option<&str>, nat: Option<&str>) -> BtpPlayer {
        BtpPlayer {
            id: 0,
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: member.map(String::from),
            nationality: nat.map(String::from),
            club: None,
        }
    }

    fn sample_match(id: i64, status: MatchStatus, court: Option<&str>) -> BtpMatch {
        BtpMatch {
            display_order: None,
            from1: None,
            from2: None,
            id,
            draw_id: 1,
            planning_id: 1000 + id,
            draw_name: "HE".to_string(),
            discipline: Discipline::MensSingles,
            class_label: String::new(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: vec![player("Anna Müller", Some("08-001234"), Some("GER"))],
            team2: vec![player("Ben Schmidt", None, None)],
            entry1_id: 0,
            entry2_id: 0,
            court: court.map(String::from),
            court_id: None,
            location_id: None,
            sets: vec![(21, 19), (21, 15)],
            winner: None,
            result: MatchResult::Normal,
            status,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            official1_id: None,
            official2_id: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    #[test]
    fn tset_matches_and_courts_cover_on_court_matches() {
        let snapshot = BtpSnapshot {
            tournament_name: "Test-Turnier".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![
                sample_match(1, MatchStatus::OnCourt, Some("Feld 9")),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let tset = build_tset(
            &snapshot,
            7,
            &LivetickerContext::bare(&AppConfig::default()),
        );
        assert_eq!(tset.kind, "tset");
        assert_eq!(tset.rid, 7);
        assert_eq!(tset.event.matches.len(), 1);
        assert_eq!(tset.event.courts.len(), 1);
        assert_eq!(tset.event.courts[0].num, "Feld 9");
        assert_eq!(tset.event.courts[0].match_id, "btp_1");
    }

    /// Zwei-Hallen-Fixture: Feld „1" (CourtID 101) in „Halle B", ein
    /// laufendes Spiel darauf, ein gerufenes Spiel für „Halle A".
    fn zwei_hallen_snapshot() -> BtpSnapshot {
        let mut laufend = sample_match(1, MatchStatus::OnCourt, Some("1"));
        laufend.court_id = Some(101);
        let mut gerufen = sample_match(2, MatchStatus::Scheduled, None);
        gerufen.preparation_call_ts = Some(NOW);
        gerufen.preparation_hall = Some("Halle A".to_string());
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: vec!["1".into()],
            locations: vec![
                BtpLocation {
                    id: 1,
                    name: "Halle A".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle B".to_string(),
                },
            ],
            court_infos: vec![BtpCourt {
                id: 101,
                name: "1".to_string(),
                location_id: Some(2),
                sort_order: 1,
            }],
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![laufend, gerufen],
        }
    }

    #[test]
    fn tset_courts_carry_their_hall_color_in_multi_hall() {
        // Spec hallen-farben: display=monitor gruppiert nach Halle — die
        // Farbe reist am Court mit (alphabetisch: „Halle B" → Ton 1).
        let cfg = AppConfig::default();
        let tset = build_tset(&zwei_hallen_snapshot(), 1, &LivetickerContext::bare(&cfg));
        assert_eq!(tset.event.courts[0].hall, "Halle B");
        assert_eq!(
            tset.event.courts[0].hall_color.as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[1])
        );
    }

    #[test]
    fn tset_upcoming_matches_carry_the_hall_color_of_their_call() {
        // display=next: das gerufene Spiel trägt die Farbe seiner Halle
        // („Halle A" → Ton 0); ungerufene ohne Halle bleiben ohne Farbe.
        let cfg = AppConfig::default();
        let tset = build_tset(&zwei_hallen_snapshot(), 1, &LivetickerContext::bare(&cfg));
        let gerufen = tset
            .event
            .upcoming_matches
            .iter()
            .find(|m| m.id == "btp_2")
            .expect("gerufenes Spiel ist gelistet");
        assert_eq!(gerufen.hall.as_deref(), Some("Halle A"));
        assert_eq!(
            gerufen.hall_color.as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[0])
        );
    }

    #[test]
    fn tset_omits_hall_color_for_single_hall_tournaments() {
        // Ein-Hallen-Turnier: das Feld fehlt KOMPLETT im JSON
        // (skip_serializing_if) — alte badhub-Parser sehen exakt den
        // bisherigen Payload.
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![
                sample_match(1, MatchStatus::OnCourt, Some("Feld 9")),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let cfg = AppConfig::default();
        let tset = build_tset(&snapshot, 1, &LivetickerContext::bare(&cfg));
        let json = serde_json::to_string(&tset).unwrap();
        assert!(
            !json.contains("hall_color"),
            "kein hall_color im Ein-Hallen-Payload: {json}"
        );
    }

    #[test]
    fn build_tset_laesst_die_logo_felder_offen() {
        // `build_tset` kennt die Konfiguration nicht — die Logo-Entscheidung
        // fällt erst im Sync-Zyklus (`logo_in_tset_legen`). Bis dahin müssen
        // die Felder **fehlen**: Auf dem Draht heißt das „unverändert",
        // während ein leerer String für badhub „löschen" bedeutete.
        // Ein voller `tset` ohne Zutun des Zyklus darf also nichts löschen.
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: Vec::new(),
        };
        let cfg = AppConfig::default();
        let tset = build_tset(&snapshot, 1, &LivetickerContext::bare(&cfg));
        let json = serde_json::to_string(&tset).unwrap();
        assert!(
            !json.contains("tournament_logo"),
            "ohne Zutun des Zyklus darf kein Logo-Feld im Payload stehen: {json}"
        );
    }

    #[test]
    fn tset_match_maps_players_and_score() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![sample_match(14, MatchStatus::OnCourt, Some("1"))],
        };
        let m = &build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .matches[0];
        assert_eq!(m.id, "btp_14");
        assert_eq!(m.n, "HE G1");
        assert_eq!(m.s, vec![[21, 19], [21, 15]]);
        assert_eq!(m.p0, ["Anna Müller"]);
        assert_eq!(m.p0_member_ids, [Some("08-001234".to_string())]);
        assert_eq!(m.p1, ["Ben Schmidt"]);
        assert_eq!(m.p1_member_ids, [None]);
    }

    #[test]
    fn recent_finished_keeps_all_matches_of_the_day() {
        // Kein Zeitfenster mehr: auch früh am Tag beendete Matches bleiben in
        // der Liste, solange bts-light läuft. Nur Matches ohne Zeitstempel
        // (noch nicht von der Sync-Engine erfasst) fallen raus.
        let mut early = sample_match(1, MatchStatus::Finished, None);
        early.winner = Some(1);
        early.finished_at = Some(NOW - 8 * 60 * 60 * 1000); // vor 8 Stunden
        let mut late = sample_match(2, MatchStatus::Finished, None);
        late.winner = Some(2);
        late.finished_at = Some(NOW - 60_000); // vor 1 Minute
        let mut unstamped = sample_match(3, MatchStatus::Finished, None);
        unstamped.winner = Some(1);
        unstamped.finished_at = None;

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![early, late, unstamped],
        };
        let finished = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .recent_finished_matches;
        // early + late bleiben, unstamped fällt raus; neueste zuerst.
        assert_eq!(finished.len(), 2);
        assert_eq!(finished[0].id, "btp_2");
        assert_eq!(finished[1].id, "btp_1");
        assert_eq!(finished[0].team1_won, Some(false));
        assert_eq!(finished[1].end_ts, Some(NOW - 8 * 60 * 60 * 1000));
    }

    #[test]
    fn recent_finished_sorted_newest_first() {
        let mut a = sample_match(1, MatchStatus::Finished, None);
        a.winner = Some(1);
        a.finished_at = Some(NOW - 600_000);
        let mut b = sample_match(2, MatchStatus::Finished, None);
        b.winner = Some(1);
        b.finished_at = Some(NOW - 60_000); // neuer

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![a, b],
        };
        let finished = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .recent_finished_matches;
        assert_eq!(finished[0].id, "btp_2");
        assert_eq!(finished[1].id, "btp_1");
    }

    #[test]
    fn upcoming_contains_scheduled_matches_with_num() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![
                sample_match(5, MatchStatus::Scheduled, None),
                sample_match(6, MatchStatus::OnCourt, Some("1")),
            ],
        };
        let upcoming = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .upcoming_matches;
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].id, "btp_5");
        assert_eq!(upcoming[0].match_num, Some(5));
    }

    #[test]
    fn upcoming_follows_the_tournament_plan_not_the_match_number() {
        // Der Liveticker ist die Ansicht mit den meisten Augen. Zeigt er
        // andere „nächste Spiele" als der Turnierplan, sucht die
        // Turnierleitung den Fehler bei sich — und Zuschauer warten am
        // falschen Feld. Maßgeblich ist die Ansetzung: erst die Zeit, dann
        // die Reihenfolge innerhalb des Zeitfensters — und die ergibt sich aus
        // der Auslosung (DrawID), nicht aus der Spielnummer.
        let mut frueh = sample_match(1, MatchStatus::Scheduled, None);
        frueh.match_num = Some(90); // hohe Nummer, aber zuerst angesetzt
        frueh.planned_time = Some(202_702_050_900);
        frueh.draw_id = 24; // Gruppe 3
        let mut spaet = sample_match(2, MatchStatus::Scheduled, None);
        spaet.match_num = Some(2); // kleine Nummer, aber später dran
        spaet.planned_time = Some(202_702_050_900);
        spaet.draw_id = 25; // Gruppe 4 — dieselbe Zeit, spätere Auslosung
        let mut viel_spaeter = sample_match(3, MatchStatus::Scheduled, None);
        viel_spaeter.match_num = Some(1);
        viel_spaeter.planned_time = Some(202_702_051_100);
        viel_spaeter.draw_id = 24;

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![viel_spaeter, spaet, frueh],
        };
        let ids: Vec<String> = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .upcoming_matches
        .into_iter()
        .map(|m| m.id)
        .collect();
        assert_eq!(
            ids,
            vec!["btp_1", "btp_2", "btp_3"],
            "Ansetzung schlägt Spielnummer"
        );
    }

    #[test]
    fn upcoming_puts_called_matches_first_and_carries_hall() {
        // Match 9 hat eine kleinere Spielnummer, aber Match 5 ist gerufen –
        // der Aufruf muss trotz höherer Nummer vorne stehen.
        let mut called = sample_match(5, MatchStatus::Scheduled, None);
        called.match_num = Some(50);
        called.preparation_call_ts = Some(NOW - 120_000);
        called.preparation_hall = Some("Halle 2".to_string());
        let mut uncalled = sample_match(9, MatchStatus::Scheduled, None);
        uncalled.match_num = Some(9);

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![uncalled, called],
        };
        let upcoming = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .upcoming_matches;
        assert_eq!(upcoming.len(), 2);
        // Gerufenes Match zuerst, trotz höherer Spielnummer.
        assert_eq!(upcoming[0].id, "btp_5");
        assert_eq!(upcoming[0].preparation_call_ts, Some(NOW - 120_000));
        assert_eq!(upcoming[0].hall.as_deref(), Some("Halle 2"));
        // Nicht gerufenes Match dahinter, ohne Vorbereitungs-Felder.
        assert_eq!(upcoming[1].id, "btp_9");
        assert_eq!(upcoming[1].preparation_call_ts, None);
        assert_eq!(upcoming[1].hall, None);
    }

    #[test]
    fn upcoming_respects_the_manual_prefix_like_every_other_view() {
        // Spec `spielliste-manuelle-reihenfolge`, Blocker 5: der Liveticker
        // darf als einzige der fünf Sortier-Stellen nicht von der manuellen
        // Reihenfolge abweichen — auch ohne Hallen-Trennung im Snapshot.
        let mut spaet = sample_match(7, MatchStatus::Scheduled, None);
        spaet.match_num = Some(7);
        spaet.planned_time = Some(202_702_051_100);
        let mut frueh = sample_match(1, MatchStatus::Scheduled, None);
        frueh.match_num = Some(1);
        frueh.planned_time = Some(202_702_050_900);

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![frueh, spaet],
        };
        let config = AppConfig::default();
        let order = QueueOrderStore::default();
        // 7 (später angesetzt) manuell vor 1 (früher angesetzt) ziehen.
        order.reorder(&[1, 7], 7, Some(1));
        let ctx = LivetickerContext::new(&config, HashMap::new(), HashMap::new(), &order);

        let ids: Vec<String> = build_tset(&snapshot, 1, &ctx)
            .event
            .upcoming_matches
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(
            ids,
            vec!["btp_7", "btp_1"],
            "manueller Präfix schlägt PlannedTime"
        );
    }

    #[test]
    fn upcoming_order_unchanged_when_nothing_is_called() {
        // Ohne Aufrufe degeneriert die Sortierung exakt zur Spielnummern-
        // Reihenfolge – das alte Verhalten bleibt unverändert.
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![
                sample_match(7, MatchStatus::Scheduled, None),
                sample_match(3, MatchStatus::Scheduled, None),
            ],
        };
        let upcoming = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .upcoming_matches;
        // sample_match setzt match_num = id → nach Nummer sortiert: 3, 7.
        assert_eq!(upcoming[0].id, "btp_3");
        assert_eq!(upcoming[1].id, "btp_7");
    }

    #[test]
    fn serializes_to_expected_json_keys() {
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("1"))],
        };
        let json = serde_json::to_string(&build_tset(
            &snapshot,
            42,
            &LivetickerContext::bare(&AppConfig::default()),
        ))
        .unwrap();
        assert!(json.contains(r#""type":"tset""#));
        assert!(json.contains(r#""recent_finished_matches":[]"#));
        assert!(json.contains(r#""upcoming_matches":[]"#));
        // Laufende Matches tragen keine Zusatzfelder.
        assert!(!json.contains("end_ts"));
        assert!(!json.contains("match_num"));
    }

    #[test]
    fn finished_walkover_carries_outcome() {
        let mut walkover = sample_match(1, MatchStatus::Finished, None);
        walkover.winner = Some(1);
        walkover.result = MatchResult::Walkover;
        walkover.finished_at = Some(NOW - 60_000);
        let mut regular = sample_match(2, MatchStatus::Finished, None);
        regular.winner = Some(2);
        regular.finished_at = Some(NOW - 60_000);

        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![walkover, regular],
        };
        let finished = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        )
        .event
        .recent_finished_matches;
        let by_id = |id: &str| finished.iter().find(|m| m.id == id).unwrap();
        assert_eq!(by_id("btp_1").outcome, Some("walkover"));
        assert_eq!(by_id("btp_2").outcome, None);
    }

    #[test]
    fn tset_court_carries_the_hall_for_multi_hall_tournaments() {
        // Mehr-Hallen-Turnier: der TsetCourt trägt die Halle des Felds,
        // aufgelöst über court_id → court_infos → locations.
        let mut m = sample_match(1, MatchStatus::OnCourt, Some("1"));
        m.court_id = Some(101);
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: vec![
                BtpLocation {
                    id: 1,
                    name: "Halle 1".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle 2".to_string(),
                },
            ],
            court_infos: vec![BtpCourt {
                id: 101,
                name: "1".to_string(),
                location_id: Some(2),
                sort_order: 1,
            }],
            matches: vec![m],
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        };
        let tset = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        );
        assert_eq!(tset.event.courts.len(), 1);
        assert_eq!(tset.event.courts[0].num, "1");
        assert_eq!(tset.event.courts[0].hall, "Halle 2");
    }

    #[test]
    fn tset_court_hall_is_empty_for_single_hall_tournaments() {
        // Ein-Hallen-Turnier (keine Locations): die Halle bleibt leer, der
        // Liveticker-Monitor zeigt dann wie bisher ein flaches Raster.
        let snapshot = BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("1"))],
        };
        let tset = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        );
        assert_eq!(tset.event.courts[0].hall, "");
    }

    // --- Hallen-Check-In: Meldeliste ---------------------------------------

    fn roster_player(id: i64, first: &str, last: &str) -> BtpPlayer {
        BtpPlayer {
            id,
            name: format!("{first} {last}"),
            first: first.to_string(),
            last: last.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        }
    }

    fn roster_snapshot() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "CP Open".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            matches: Vec::new(),
            events: vec![
                BtpEvent {
                    id: 1,
                    name: "HE A".to_string(),
                    discipline: Discipline::MensSingles,
                },
                BtpEvent {
                    id: 2,
                    name: "HD B".to_string(),
                    discipline: Discipline::MensDoubles,
                },
            ],
            entries: vec![
                BtpEntry {
                    id: 10,
                    event_id: 1,
                    players: vec![roster_player(1, "Anna", "Beispiel")],
                },
                BtpEntry {
                    id: 11,
                    event_id: 2,
                    players: vec![
                        roster_player(1, "Anna", "Beispiel"),
                        roster_player(2, "Bea", "Muster"),
                    ],
                },
            ],
            officials: Vec::new(),
        }
    }

    #[test]
    fn checkin_roster_serializes_to_expected_json_keys() {
        let json =
            serde_json::to_string(&build_checkin_roster(&roster_snapshot(), "GUID-1", 42)).unwrap();
        assert!(json.contains(r#""type":"centry_list""#));
        assert!(json.contains(r#""tournament_uuid":"GUID-1""#));
        assert!(json.contains(r#""tournament_name":"CP Open""#));
        assert!(json.contains(r#""rid":42"#));
        assert!(json.contains(r#""event_id":1"#));
        assert!(json.contains(r#""entry_id":10"#));
        assert!(json.contains(r#""player_id":2"#));
        // Disziplin als snake_case-Schluessel, badhub lokalisiert selbst.
        assert!(json.contains(r#""discipline":"mens_doubles""#));
        // Fehlende Optionalfelder werden weggelassen statt leer gesendet.
        assert!(!json.contains("member_id"));
        assert!(!json.contains("club"));
        assert!(!json.contains("nationality"));
        // Datenschutz: nie ein Geburtsjahr, auch nicht mittelbar.
        assert!(!json.contains("birth"));
    }

    #[test]
    fn checkin_roster_keeps_doubles_partners_together() {
        let msg = build_checkin_roster(&roster_snapshot(), "GUID-1", 1);
        let doubles = msg.entries.iter().find(|e| e.entry_id == 11).unwrap();
        assert_eq!(doubles.players.len(), 2);
        assert_eq!(
            doubles
                .players
                .iter()
                .map(|p| p.player_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        // Derselbe Spieler ist in beiden Klassen gemeldet und erscheint auch
        // zweimal — der Check-In gilt je Klasse, nicht je Person.
        let single = msg.entries.iter().find(|e| e.entry_id == 10).unwrap();
        assert_eq!(single.players[0].player_id, 1);
    }

    #[test]
    fn checkin_roster_carries_licence_and_club_when_btp_has_them() {
        let mut snapshot = roster_snapshot();
        snapshot.entries[0].players[0].member_id = Some("08-012002".to_string());
        snapshot.entries[0].players[0].club = Some("BC Beispiel".to_string());
        snapshot.entries[0].players[0].nationality = Some("GER".to_string());

        let json = serde_json::to_string(&build_checkin_roster(&snapshot, "G", 1)).unwrap();
        assert!(json.contains(r#""member_id":"08-012002""#));
        assert!(json.contains(r#""club":"BC Beispiel""#));
        assert!(json.contains(r#""nationality":"GER""#));
    }

    #[test]
    fn checkin_roster_drops_classes_without_entries() {
        // Eine Klasse ohne Meldung waere auf der Check-In-Seite eine leere
        // Liste, in die niemand einchecken kann.
        let mut snapshot = roster_snapshot();
        snapshot.entries.retain(|e| e.event_id == 1);
        let msg = build_checkin_roster(&snapshot, "G", 1);
        assert_eq!(
            msg.classes.iter().map(|c| c.event_id).collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn checkin_roster_trims_the_tournament_guid() {
        // Aus der Zwischenablage eingefuegte GUIDs tragen oft Leerzeichen.
        let msg = build_checkin_roster(&roster_snapshot(), "  GUID-1  ", 1);
        assert_eq!(msg.tournament_uuid, "GUID-1");
    }

    #[test]
    fn checkin_roster_is_empty_for_a_tournament_without_entries() {
        let mut snapshot = roster_snapshot();
        snapshot.events.clear();
        snapshot.entries.clear();
        let msg = build_checkin_roster(&snapshot, "G", 1);
        assert!(msg.classes.is_empty());
        assert!(msg.entries.is_empty());
    }

    // ── sched: vollständiger Spielplan für badhub ────────────────────────────
    //
    // Spezifikation (badhub-Repo):
    // docs/superpowers/specs/2026-08-16-spieler-live-vollstaendiger-spielplan-design.md

    fn sched_snapshot(matches: Vec<BtpMatch>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "Sched-Turnier".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches,
        }
    }

    #[test]
    fn build_sched_kappt_nicht() {
        // Der Grund für den ganzen zweiten Kanal: `upcoming()` kappt bei
        // UPCOMING_LIMIT = 15 Spielen des GESAMTEN Turniers. Wessen Spiel
        // weiter hinten liegt, taucht dort nie auf. sched darf das nicht.
        let matches: Vec<BtpMatch> = (1..=20)
            .map(|i| sample_match(i, MatchStatus::Scheduled, None))
            .collect();
        let snapshot = sched_snapshot(matches);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );

        assert_eq!(msg.kind, "sched");
        assert_eq!(msg.event.matches.len(), 20, "sched darf NICHT kappen");
    }

    #[test]
    fn build_sched_rechnet_planned_time_in_unix_ms() {
        // BTP liefert YYYYMMDDHHMM als sortierbaren i64 in LOKALER Wandzeit
        // (der Turnier-Laptop steht am Turnierort). Über die Leitung geht
        // Unix-ms, damit badhub keine Zeitzone interpretieren muss.
        //
        // Geprüft wird die RUNDREISE, nicht ein absoluter Millisekundenwert:
        // ein fester Wert wäre nur auf einem Rechner in der Zeitzone des
        // Autors grün und auf einem UTC-CI-Runner rot - aus einem Grund, der
        // nichts mit dem Code zu tun hat.
        use chrono::{Local, TimeZone};

        let mut m = sample_match(1, MatchStatus::Scheduled, None);
        m.planned_time = Some(202_608_161_430); // 2026-08-16 14:30 lokal
        let snapshot = sched_snapshot(vec![m]);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );

        let ms = msg.event.matches[0].planned_ts.expect("planned_ts gesetzt");
        let zurueck = Local.timestamp_millis_opt(ms as i64).unwrap();
        assert_eq!(zurueck.format("%Y%m%d%H%M").to_string(), "202608161430");
    }

    #[test]
    fn build_sched_liefert_court_auch_bei_beendeten() {
        // Im tset fehlt das Feld bei recent_finished_matches - dort ist das
        // Absicht (Monitor-Ansicht). Für die Spielerhistorie ist es genau die
        // Information, die fehlte.
        let mut m = sample_match(1, MatchStatus::Finished, Some("Feld 05"));
        m.winner = Some(1);
        m.finished_at = Some(NOW);
        let snapshot = sched_snapshot(vec![m]);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );

        assert_eq!(msg.event.matches[0].status, "finished");
        assert_eq!(msg.event.matches[0].court.as_deref(), Some("Feld 05"));
        assert_eq!(msg.event.matches[0].team1_won, Some(true));
        assert_eq!(msg.event.matches[0].end_ts, Some(NOW));
    }

    #[test]
    fn build_sched_gibt_wartenden_kein_feld_aber_eine_position() {
        // Das Feld steht erst beim Aufruf fest - eine Angabe wäre geraten.
        // Die Position dagegen ist bekannt und der Kern der Anzeige.
        let matches: Vec<BtpMatch> = (1..=3)
            .map(|i| sample_match(i, MatchStatus::Scheduled, Some("Feld 1")))
            .collect();
        let snapshot = sched_snapshot(matches);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );

        let positionen: Vec<Option<i64>> = msg.event.matches.iter().map(|m| m.queue_pos).collect();
        assert_eq!(positionen, vec![Some(0), Some(1), Some(2)]);
        assert!(
            msg.event.matches.iter().all(|m| m.court.is_none()),
            "wartende Spiele tragen kein Feld"
        );
    }

    #[test]
    fn build_sched_reicht_die_prognose_nur_fuer_wartende_durch() {
        // predicted_start_ts beantwortet "wann bin ich dran" - bei einem
        // laufenden oder beendeten Spiel ist die Frage sinnlos, und ein
        // stehengebliebener Wert wäre schlimmer als keiner.
        let mut laufend = sample_match(1, MatchStatus::OnCourt, Some("Feld 1"));
        laufend.planned_time = None;
        let wartend = sample_match(2, MatchStatus::Scheduled, None);
        let snapshot = sched_snapshot(vec![laufend, wartend]);
        let cfg = AppConfig::default();
        let mut prognosen = HashMap::new();
        prognosen.insert(1_i64, NOW + 60_000);
        prognosen.insert(2_i64, NOW + 900_000);

        let msg = build_sched(&snapshot, &LivetickerContext::bare(&cfg), &prognosen, 1);

        let laufend = msg.event.matches.iter().find(|m| m.id == "btp_1").unwrap();
        let wartend = msg.event.matches.iter().find(|m| m.id == "btp_2").unwrap();
        assert_eq!(
            laufend.predicted_start_ts, None,
            "laufendes Spiel braucht keine Prognose"
        );
        assert_eq!(wartend.predicted_start_ts, Some(NOW + 900_000));
    }

    #[test]
    fn build_sched_schickt_gaeste_als_null_nicht_als_leerstring() {
        // sample_match() gibt Team 2 bewusst keine member_id. Serde macht aus
        // Option::None ein JSON-null - die Gegenstelle muss das als "Gast"
        // lesen, nicht als leere Lizenznummer. Steht hier fest, weil die
        // badhub-Fixture denselben Fall abbilden muss.
        let snapshot = sched_snapshot(vec![sample_match(1, MatchStatus::Scheduled, None)]);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );
        let json = serde_json::to_value(&msg).unwrap();

        assert_eq!(
            json["event"]["matches"][0]["p1_member_ids"][0],
            serde_json::Value::Null
        );
    }

    #[test]
    fn tset_traegt_disziplin_und_klasse_fuer_den_liveticker() {
        // Der Liveticker liest `n` (draw_name + round_name) und zeigte
        // deshalb "Gruppe 1 G1" - die Disziplin kam dort nie an. Sie ging
        // bisher nur im sched-Kanal raus, der die Spielerseite speist.
        // Beide Felder sind optional (skip_serializing_if): ein alter
        // Empfaenger sieht keinen Unterschied.
        let mut m = sample_match(1, MatchStatus::OnCourt, Some("Feld 3"));
        m.discipline = Discipline::WomensDoubles;
        m.class_label = "B".to_string();
        let snapshot = sched_snapshot(vec![m]);

        let tset = build_tset(
            &snapshot,
            1,
            &LivetickerContext::bare(&AppConfig::default()),
        );

        assert_eq!(tset.event.matches[0].discipline, Some("womens_doubles"));
        assert_eq!(tset.event.matches[0].class_label.as_deref(), Some("B"));
    }

    #[test]
    fn build_sched_traegt_disziplin_klasse_und_hallenfarbe() {
        // badhub zeigte bisher nur "Gruppe 1 G1" - das ist draw_name +
        // round_name, und draw_name ist bei Gruppenturnieren die
        // AUSLOSUNGSGRUPPE, nicht die Klasse. Die Disziplin ("HE") und das
        // Klassenkuerzel ("A") liegen hier vor, wurden aber nie gesendet;
        // badhub konnte sie deshalb nicht anzeigen, egal was es tat.
        let mut m = sample_match(1, MatchStatus::Scheduled, None);
        m.discipline = Discipline::MensSingles;
        m.class_label = "A".to_string();
        let snapshot = sched_snapshot(vec![m]);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );

        assert_eq!(msg.event.matches[0].discipline, "mens_singles");
        assert_eq!(msg.event.matches[0].class_label.as_deref(), Some("A"));
    }

    #[test]
    fn build_sched_haelt_den_feldvertrag_mit_badhub() {
        // Die Gegenstelle liest tests/fixtures/sched_golden.json im
        // badhub-Repo. Weicht die Serialisierung ab, bricht der Spielplan
        // dort lautlos - badhub ignoriert unbekannte Felder.
        let snapshot = sched_snapshot(vec![sample_match(1, MatchStatus::Scheduled, None)]);
        let cfg = AppConfig::default();

        let msg = build_sched(
            &snapshot,
            &LivetickerContext::bare(&cfg),
            &HashMap::new(),
            1,
        );
        let json = serde_json::to_value(&msg).unwrap();

        assert_eq!(json["type"], "sched");
        let m = json["event"]["matches"][0].as_object().unwrap();
        let erwartet = [
            "_id",
            "n",
            "status",
            "p0",
            "p0_member_ids",
            "p1",
            "p1_member_ids",
            "planned_ts",
            "predicted_start_ts",
            "queue_pos",
            "hall",
            "court",
            "hall_color",
            "discipline",
            "class_label",
            "sets",
            "team1_won",
            "end_ts",
            "outcome",
        ];
        for feld in erwartet {
            assert!(m.contains_key(feld), "Feld {feld} fehlt im sched-Payload");
        }
        assert_eq!(m.len(), erwartet.len(), "unbekanntes Feld im sched-Payload");
    }
}
