//! Parser: VISUALXML-Knotenbaum einer `SENDTOURNAMENTINFO`-Antwort →
//! `BtpSnapshot`.
//!
//! BTP speichert ein Round-Robin als Teilnehmer-Slots (Match-Einträge mit
//! `EntryID`) und Paarungen (Match-Einträge mit `From1`/`From2`, die auf die
//! `PlanningID` der Slots verweisen). Echte, anzeigbare Paarungen tragen
//! `IsMatch = true`. Siehe `docs/btp_protocol.md`.

use std::collections::HashMap;

use serde::Serialize;

use crate::btp::xml::{self, Node};

/// Spielzustand eines Matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchStatus {
    /// Eingeplant, aber weder auf einem Court noch beendet.
    Scheduled,
    /// Aktuell einem Court zugewiesen.
    OnCourt,
    /// Mit Sieger abgeschlossen.
    Finished,
}

/// Wie ein Match entschieden wurde (BTP-Feld `ScoreStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    /// Regulär ausgespielt.
    Normal,
    /// Kampfloser Sieg (Walkover) – kein Spiel stattgefunden.
    Walkover,
    /// Aufgabe während des Spiels.
    Retired,
    /// Disqualifikation.
    Disqualified,
}

impl MatchResult {
    /// Leitet das Ergebnis aus dem BTP-Feld `ScoreStatus` ab.
    fn from_score_status(score_status: i64) -> MatchResult {
        match score_status {
            1 => MatchResult::Walkover,
            2 => MatchResult::Retired,
            3 => MatchResult::Disqualified,
            _ => MatchResult::Normal,
        }
    }
}

/// Disziplin eines Matches, aus dem BTP-Event abgeleitet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Discipline {
    /// Herreneinzel.
    MensSingles,
    /// Dameneinzel.
    WomensSingles,
    /// Herrendoppel.
    MensDoubles,
    /// Damendoppel.
    WomensDoubles,
    /// Gemischtes Doppel.
    Mixed,
    /// Nicht bestimmbar (Event fehlt oder unbekannte IDs).
    #[default]
    Unknown,
}

impl Discipline {
    /// Leitet die Disziplin aus den BTP-Event-Feldern ab.
    /// `GameTypeID`: 1 = Einzel, 2 = Doppel. `GenderID`: 1 = Herren,
    /// 2 = Damen, 3 = Mixed.
    fn from_event(game_type_id: i64, gender_id: i64) -> Discipline {
        match (game_type_id, gender_id) {
            (_, 3) => Discipline::Mixed,
            (1, 1) => Discipline::MensSingles,
            (1, 2) => Discipline::WomensSingles,
            (2, 1) => Discipline::MensDoubles,
            (2, 2) => Discipline::WomensDoubles,
            _ => Discipline::Unknown,
        }
    }

    /// snake_case-Schlüssel der Disziplin – identisch zur serde-Form, für
    /// die Wire-Typen ([`relay_proto::MatchBrief`]). Der Court-Monitor und
    /// die Sprachansage lokalisieren ihn selbst.
    pub fn as_str(self) -> &'static str {
        match self {
            Discipline::MensSingles => "mens_singles",
            Discipline::WomensSingles => "womens_singles",
            Discipline::MensDoubles => "mens_doubles",
            Discipline::WomensDoubles => "womens_doubles",
            Discipline::Mixed => "mixed",
            Discipline::Unknown => "unknown",
        }
    }
}

/// Ein Spieler einer Paarung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtpPlayer {
    /// Anzeigename ("Vorname Nachname" bzw. nur Nachname).
    pub name: String,
    /// Vorname(n) (BTP `Firstname`) – getrennt geführt, damit der
    /// Court-Monitor Vor- und Nachnamen exakt darstellen kann (statt zu
    /// raten). Leer, wenn BTP keinen Vornamen liefert.
    pub first: String,
    /// Nachname (BTP `Lastname`) – getrennt geführt, siehe `first`.
    pub last: String,
    /// Lizenznummer (BTP `MemberID`, z. B. "08-010493"), falls vorhanden.
    pub member_id: Option<String>,
    /// Nationalität als ISO-Code (BTP `Country`, z. B. "GER"), falls vorhanden.
    pub nationality: Option<String>,
    /// Verein (BTP `Player.ClubID` → `Clubs > Club.Name`), falls zugeordnet.
    pub club: Option<String>,
}

/// Ein Standort/eine Halle des Turniers (BTP `Location`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtpLocation {
    /// Stabile BTP-interne LocationID.
    pub id: i64,
    /// Anzeigename, z. B. „Halle 1".
    pub name: String,
}

/// Ein Spielfeld des Turniers (BTP `Court`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtpCourt {
    /// Stabile BTP-interne CourtID – die Feld-Identität. Feldnamen können
    /// sich über Hallen hinweg wiederholen, die ID nicht.
    pub id: i64,
    /// Anzeigename, z. B. „1" oder „Feld 3".
    pub name: String,
    /// LocationID der Halle, der das Feld zugeordnet ist; `None`, wenn das
    /// Feld keiner Location zugeordnet ist.
    pub location_id: Option<i64>,
    /// Sortierreihenfolge innerhalb der Halle (BTP `SortOrder`).
    pub sort_order: i64,
}

/// Eine anzeigbare Paarung.
/// Aufgelöste Zählweise eines Matches (aus dem BTP-`ScoringFormat`). BTP führt
/// die Formate zentral (`ScoringFormats`) und ordnet sie je `Stage` zu; der
/// Draw hängt über `StageID` an der Stage. Aus dem `SetType` ergeben sich
/// Spielende, Cap und die Intervall-Pausenschwelle (offizielle BWF-Werte,
/// Tabelle wie im Original-BTS).
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringFormat {
    /// Anzahl Sätze (BTP `NumSets`), z. B. 3 → Best-of-3.
    pub best_of: i64,
    /// Punktzahl zum Satzgewinn (z. B. 21 oder 15).
    pub target_score: i64,
    /// Maximalpunktzahl/Cap: bei Gleichstand wird bis dahin gespielt, dann
    /// gewinnt der Führende (z. B. 30 bei 21, 21 bei 15).
    pub cap_score: i64,
    /// Punktestand, bei dem die Intervall-Pause (60 s) ausgelöst wird; `None`,
    /// wenn das Format keine reguläre Intervall-Pause je Satz kennt.
    pub interval_at: Option<i64>,
}

impl Default for ScoringFormat {
    /// Klassisch 3×21 (Cap 30, Intervall bei 11) – BTP-Standard und Fallback.
    fn default() -> Self {
        Self {
            best_of: 3,
            target_score: 21,
            cap_score: 30,
            interval_at: Some(11),
        }
    }
}

impl ScoringFormat {
    /// Leitet Spielende/Cap/Intervall aus BTP `NumSets`/`SetType`/`Score` ab.
    /// SetType-Tabelle (BWF, identisch zum Original-BTS):
    /// `0` = 3×21 (Ende 21, Cap 30, Intervall 11); `306` = 15 (21), Intervall 8;
    /// `301/304/305` = 11er-Sätze (Cap 11/15/13, keine reguläre Intervall-Pause
    /// in Mehrsatz-Formaten); `999` = Score-getrieben; sonst → klassisch 21.
    fn from_btp(num_sets: i64, set_type: i64, score: i64) -> Self {
        let best_of = if num_sets > 0 { num_sets } else { 3 };
        let (target_score, cap_score, interval_at) = match set_type {
            0 => (21, 30, Some(11)),
            301 => (11, 11, None),
            304 => (11, 15, None),
            305 => (11, 13, None),
            306 => (15, 21, Some(8)),
            999 if score > 0 => (score, score, None),
            // Unbekannter SetType: sicher auf klassisch 21 zurückfallen,
            // aber die echte Satzzahl behalten.
            _ => {
                return ScoringFormat {
                    best_of,
                    ..ScoringFormat::default()
                }
            }
        };
        ScoringFormat {
            best_of,
            target_score,
            cap_score,
            interval_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BtpMatch {
    pub id: i64,
    /// Draw-ID des Matches – zusammen mit `planning_id` adressiert es das
    /// Match beim Zurückschreiben nach BTP (`SENDUPDATE`).
    pub draw_id: i64,
    /// Planungsposition des Matches im Draw (`Match.PlanningID`).
    pub planning_id: i64,
    /// Name der Auslosung, z. B. "HE".
    pub draw_name: String,
    /// Disziplin des Matches (aus dem BTP-Event abgeleitet).
    pub discipline: Discipline,
    /// Runden-/Spielbezeichnung, z. B. "G1".
    pub round_name: String,
    /// Spielnummer (BTP `MatchNr`), falls vergeben.
    pub match_num: Option<i64>,
    /// Angesetzte Spielzeit (BTP `PlannedTime`) als sortierbarer Schlüssel
    /// `YYYYMMDDHHMM` (z. B. 202606141330), falls BTP eine Ansetzung liefert.
    /// Steuert die Reihenfolge der Auto-Feldvergabe („Zeitplan abspielen").
    /// `None`, wenn keine/ungültige Ansetzung.
    pub planned_time: Option<i64>,
    /// Team 1 (ein Spieler bei Einzel, zwei bei Doppel).
    pub team1: Vec<BtpPlayer>,
    pub team2: Vec<BtpPlayer>,
    /// EntryID von Team 1 – identifiziert die Mannschaft draw-weit
    /// eindeutig (0, falls der Platz noch offen ist). Damit lassen sich
    /// nach einer Aufgabe die übrigen Spiele derselben Mannschaft finden.
    pub entry1_id: i64,
    /// EntryID von Team 2 (0, falls der Platz noch offen ist).
    pub entry2_id: i64,
    /// Court-Name, falls dem Match ein Court zugewiesen ist. Achtung: bei
    /// Mehr-Hallen-Turnieren nicht eindeutig – für Identität `court_id`.
    pub court: Option<String>,
    /// CourtID (stabile Feld-Identität) des zugewiesenen Felds; `None`,
    /// wenn das Match keinem Feld zugewiesen ist.
    pub court_id: Option<i64>,
    /// Satz-Ergebnisse als (Team1, Team2)-Punkte.
    pub sets: Vec<(i64, i64)>,
    /// Sieger: 1 oder 2, falls entschieden.
    pub winner: Option<u8>,
    /// Art der Entscheidung (normal, Walkover, Aufgabe, Disqualifikation).
    pub result: MatchResult,
    pub status: MatchStatus,
    /// Zeitpunkt (Unix-Millisekunden), zu dem das Match erstmals als
    /// beendet erkannt wurde. BTP liefert keinen End-Zeitstempel – dieses
    /// Feld wird von der Sync-Engine gesetzt, nicht vom Parser.
    pub finished_at: Option<u64>,
    /// Zeitpunkt (Unix-Millisekunden), zu dem die Turnierleitung das Match
    /// „in Vorbereitung" gerufen hat. Transientes Feld: BTP kennt keinen
    /// Vorbereitungs-Zustand – der Parser schreibt immer `None`, gesetzt
    /// wird es ausschließlich von [`crate::tablet::state::TabletState::apply_preparation_calls`].
    pub preparation_call_ts: Option<u64>,
    /// Halle, für die der Aufruf gilt (aufgelöster `Location`-Name), falls
    /// hallenweise gerufen wurde. Transientes Feld wie `preparation_call_ts`:
    /// der Parser schreibt immer `None`.
    pub preparation_hall: Option<String>,
    /// Zählweise des Matches (Sätze, Spielende, Cap, Intervall-Schwelle) –
    /// aus dem BTP-`ScoringFormat` der Stage des Draws aufgelöst. Steuert auf
    /// dem Tablet Satzgewinn und Pausen. Default = 3×21.
    pub scoring: ScoringFormat,
}

/// Aufbereiteter Turnier-Stand aus einer `SENDTOURNAMENTINFO`-Antwort.
#[derive(Debug, Clone, PartialEq)]
pub struct BtpSnapshot {
    pub tournament_name: String,
    /// Mindest-Pause zwischen zwei Spielen eines Spielers in Minuten
    /// (BTP-Setting 1303). `None`, wenn BTP keine Pause gesetzt hat. Die
    /// Auto-Feldvergabe ruft einen Spieler erst nach dieser Pause wieder auf.
    pub rest_minutes: Option<i64>,
    pub matches: Vec<BtpMatch>,
    /// Alle Court-Namen des Turniers (BTP-Reihenfolge), auch leere Courts –
    /// damit der Tablet-Server jedem Court eine Adresse zuordnen kann.
    pub courts: Vec<String>,
    /// Standorte/Hallen des Turniers (BTP `Locations`). Leer bei Turnieren
    /// ohne Standort-Angabe; ab zwei Einträgen liegt ein Mehr-Hallen-
    /// Turnier vor.
    pub locations: Vec<BtpLocation>,
    /// Alle Felder mit Identität (CourtID), Hallen-Zuordnung und
    /// Sortierreihenfolge – sortiert nach Halle und BTP-`SortOrder`.
    pub court_infos: Vec<BtpCourt>,
}

impl BtpSnapshot {
    /// Ist das ein Mehr-Hallen-Turnier? Ab zwei `Locations` liegen mehrere
    /// Hallen vor – erst dann zeigt bts-light Hallen-Bezeichnungen an.
    pub fn is_multi_hall(&self) -> bool {
        self.locations.len() >= 2
    }

    /// Hallenname (BTP-`Location`-Name) eines Felds. Leer bei Ein-Hallen-
    /// Turnieren oder wenn das Feld keiner auflösbaren Halle zugeordnet ist.
    pub fn court_location_name(&self, court_id: i64) -> String {
        if !self.is_multi_hall() {
            return String::new();
        }
        let Some(location_id) = self
            .court_infos
            .iter()
            .find(|c| c.id == court_id)
            .and_then(|c| c.location_id)
        else {
            return String::new();
        };
        self.locations
            .iter()
            .find(|l| l.id == location_id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    /// Anzeige-Bezeichnung eines Felds für Monitore und Tablets. Bei einem
    /// Mehr-Hallen-Turnier mit auflösbarer Halle `"{Halle} · {Feld}"`
    /// (z. B. „Halle 2 · 6"), sonst nur der Feldname.
    pub fn court_display_label(&self, court_id: i64) -> String {
        let court_name = self
            .court_infos
            .iter()
            .find(|c| c.id == court_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let location = self.court_location_name(court_id);
        if location.is_empty() {
            court_name
        } else {
            format!("{location} · {court_name}")
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Antwort enthält keine <Tournament>-Daten")]
    NoTournament,
}

/// Parst den Knotenbaum einer `SENDTOURNAMENTINFO`-Antwort.
pub fn parse_snapshot(nodes: &[Node]) -> Result<BtpSnapshot, ModelError> {
    let tournament = xml::find(nodes, "Result")
        .and_then(|r| xml::find(r.children(), "Tournament"))
        .ok_or(ModelError::NoTournament)?;
    let t = tournament.children();

    let clubs = id_name_map(t, "Clubs");
    let players = player_map(t, &clubs);
    let entries = entry_map(t);
    let slots = slot_map(t);
    let courts = court_map(t);
    let draws = draw_map(t);
    let disciplines = draw_discipline_map(t);
    let scoring = scoring_by_draw(t);

    // Court-Namen nach CourtID sortiert – das ergibt die BTP-Anlegereihenfolge.
    let mut court_pairs: Vec<(&i64, &String)> = courts.iter().collect();
    court_pairs.sort_by_key(|(id, _)| **id);
    let court_names: Vec<String> = court_pairs.into_iter().map(|(_, n)| n.clone()).collect();

    Ok(BtpSnapshot {
        tournament_name: setting_str(t, 1001).unwrap_or_default(),
        // BTP-Setting 1303 = Mindest-Pause zwischen Spielen (Minuten).
        rest_minutes: setting_int(t, 1303).filter(|&m| m > 0),
        matches: parse_matches(
            t,
            &players,
            &entries,
            &slots,
            &courts,
            &draws,
            &disciplines,
            &scoring,
        ),
        courts: court_names,
        locations: location_list(t),
        court_infos: court_list(t),
    })
}

// --- Lookup-Tabellen ------------------------------------------------------

/// PlayerID → Spielerdaten.
fn player_map(t: &[Node], clubs: &HashMap<i64, String>) -> HashMap<i64, BtpPlayer> {
    let mut map = HashMap::new();
    let Some(players) = xml::find(t, "Players") else {
        return map;
    };
    for p in players.children() {
        let Some(id) = child_int(p, "ID") else {
            continue;
        };
        let last = child_str(p, "Lastname").unwrap_or_default();
        let first = child_str(p, "Firstname").unwrap_or_default();
        let name = if first.is_empty() {
            last.to_string()
        } else {
            format!("{first} {last}")
        };
        map.insert(
            id,
            BtpPlayer {
                name,
                first: first.to_string(),
                last: last.to_string(),
                member_id: child_str(p, "MemberID").map(String::from),
                nationality: child_str(p, "Country").map(String::from),
                // Verein über Player.ClubID auflösen (fehlt → kein Verein).
                club: child_int(p, "ClubID").and_then(|cid| clubs.get(&cid).cloned()),
            },
        );
    }
    map
}

/// EntryID → enthaltene PlayerIDs (eine bei Einzel, zwei bei Doppel).
fn entry_map(t: &[Node]) -> HashMap<i64, Vec<i64>> {
    let mut map = HashMap::new();
    let Some(entries) = xml::find(t, "Entries") else {
        return map;
    };
    for e in entries.children() {
        let Some(id) = child_int(e, "ID") else {
            continue;
        };
        let player_ids = ["Player1ID", "Player2ID"]
            .iter()
            .filter_map(|field| child_int(e, field))
            .collect();
        map.insert(id, player_ids);
    }
    map
}

/// (DrawID, PlanningID) eines Teilnehmer-Slots → EntryID.
///
/// PlanningIDs sind nur INNERHALB eines Draws eindeutig – BTP vergibt z. B.
/// in jedem Draw die Slots 1000, 2000, 3000 … Ohne die DrawID im Schlüssel
/// überschreiben sich gleichnamige Slots verschiedener Draws gegenseitig und
/// Paarungen lösen zu fremden Spielern auf ("Hilde gegen Hilde").
fn slot_map(t: &[Node]) -> HashMap<(i64, i64), i64> {
    let mut map = HashMap::new();
    let Some(matches) = xml::find(t, "Matches") else {
        return map;
    };
    for m in matches.children() {
        if let (Some(draw), Some(planning), Some(entry)) = (
            child_int(m, "DrawID"),
            child_int(m, "PlanningID"),
            child_int(m, "EntryID"),
        ) {
            map.insert((draw, planning), entry);
        }
    }
    map
}

/// CourtID → Court-Name.
fn court_map(t: &[Node]) -> HashMap<i64, String> {
    id_name_map(t, "Courts")
}

/// Standorte/Hallen in BTP-Dokumentreihenfolge.
fn location_list(t: &[Node]) -> Vec<BtpLocation> {
    let Some(group) = xml::find(t, "Locations") else {
        return Vec::new();
    };
    group
        .children()
        .iter()
        .filter_map(|n| {
            Some(BtpLocation {
                id: child_int(n, "ID")?,
                name: child_str(n, "Name").unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// Alle Felder mit Hallen-Zuordnung, sortiert nach LocationID und
/// BTP-`SortOrder` (so liegen die Felder hallenweise gruppiert vor).
fn court_list(t: &[Node]) -> Vec<BtpCourt> {
    let Some(group) = xml::find(t, "Courts") else {
        return Vec::new();
    };
    let mut courts: Vec<BtpCourt> = group
        .children()
        .iter()
        .filter_map(|n| {
            Some(BtpCourt {
                id: child_int(n, "ID")?,
                name: child_str(n, "Name").unwrap_or_default().to_string(),
                location_id: child_int(n, "LocationID"),
                sort_order: child_int(n, "SortOrder").unwrap_or(0),
            })
        })
        .collect();
    // Nach Halle (LocationID) und SortOrder gruppieren; die CourtID als
    // dritter Schlüssel macht die Reihenfolge bei fehlendem SortOrder
    // deterministisch. Felder ohne LocationID sortieren zuerst – in echten
    // BTP-Daten trägt jedes Feld eine LocationID.
    courts.sort_by_key(|c| (c.location_id, c.sort_order, c.id));
    courts
}

/// DrawID → Draw-Name.
fn draw_map(t: &[Node]) -> HashMap<i64, String> {
    id_name_map(t, "Draws")
}

/// DrawID → Disziplin. BTP führt die Disziplin am Event (`GameTypeID` +
/// `GenderID`); jeder Draw verweist per `EventID` auf sein Event.
fn draw_discipline_map(t: &[Node]) -> HashMap<i64, Discipline> {
    // EventID → Disziplin.
    let mut events: HashMap<i64, Discipline> = HashMap::new();
    if let Some(group) = xml::find(t, "Events") {
        for e in group.children() {
            if let Some(id) = child_int(e, "ID") {
                let game = child_int(e, "GameTypeID").unwrap_or(0);
                let gender = child_int(e, "GenderID").unwrap_or(0);
                events.insert(id, Discipline::from_event(game, gender));
            }
        }
    }
    // DrawID → Disziplin (über die EventID des Draws).
    let mut map = HashMap::new();
    if let Some(group) = xml::find(t, "Draws") {
        for d in group.children() {
            if let (Some(draw_id), Some(event_id)) = (child_int(d, "ID"), child_int(d, "EventID")) {
                map.insert(
                    draw_id,
                    events
                        .get(&event_id)
                        .copied()
                        .unwrap_or(Discipline::Unknown),
                );
            }
        }
    }
    map
}

/// DrawID → aufgelöste Zählweise. BTP ordnet die Formate je `Stage` zu
/// (`Stage.ScoringFormat` = Format-ID), der Draw verweist per `StageID` auf
/// seine Stage. Ohne auflösbare Zuordnung gilt das als Standard markierte
/// Format (`IsDefault`), sonst 3×21. (BTP erlaubt theoretisch auch eine
/// Zählweise pro Spiel; wie das Original-BTS lösen wir auf Stage-Ebene auf.)
fn scoring_by_draw(t: &[Node]) -> HashMap<i64, ScoringFormat> {
    // FormatID → ScoringFormat; zugleich das Default-Format merken.
    let mut formats: HashMap<i64, ScoringFormat> = HashMap::new();
    let mut default_format = ScoringFormat::default();
    if let Some(group) = xml::find(t, "ScoringFormats") {
        for f in group.children() {
            let Some(id) = child_int(f, "ID") else {
                continue;
            };
            let sf = ScoringFormat::from_btp(
                child_int(f, "NumSets").unwrap_or(3),
                child_int(f, "SetType").unwrap_or(0),
                child_int(f, "Score").unwrap_or(0),
            );
            // BTP kann `IsDefault` als Bool ODER Integer (1) liefern – beides
            // als „Standard" werten, sonst fiele die Default-Auflösung still
            // auf 3×21 zurück.
            if child_bool(f, "IsDefault") == Some(true) || child_int(f, "IsDefault") == Some(1) {
                default_format = sf.clone();
            }
            formats.insert(id, sf);
        }
    }
    // StageID → FormatID.
    let mut stage_format: HashMap<i64, i64> = HashMap::new();
    if let Some(group) = xml::find(t, "Stages") {
        for s in group.children() {
            if let (Some(id), Some(fmt)) = (child_int(s, "ID"), child_int(s, "ScoringFormat")) {
                stage_format.insert(id, fmt);
            }
        }
    }
    // DrawID → Format über die StageID des Draws.
    let mut map = HashMap::new();
    if let Some(group) = xml::find(t, "Draws") {
        for d in group.children() {
            let Some(draw_id) = child_int(d, "ID") else {
                continue;
            };
            let sf = child_int(d, "StageID")
                .and_then(|sid| stage_format.get(&sid))
                .and_then(|fid| formats.get(fid))
                .cloned()
                .unwrap_or_else(|| default_format.clone());
            map.insert(draw_id, sf);
        }
    }
    map
}

/// Generische ID→Name-Tabelle für Container mit gleichförmigen Einträgen.
fn id_name_map(t: &[Node], container: &str) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    let Some(group) = xml::find(t, container) else {
        return map;
    };
    for node in group.children() {
        if let (Some(id), Some(name)) = (child_int(node, "ID"), child_str(node, "Name")) {
            map.insert(id, name.to_string());
        }
    }
    map
}

// --- Match-Parsing --------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn parse_matches(
    t: &[Node],
    players: &HashMap<i64, BtpPlayer>,
    entries: &HashMap<i64, Vec<i64>>,
    slots: &HashMap<(i64, i64), i64>,
    courts: &HashMap<i64, String>,
    draws: &HashMap<i64, String>,
    disciplines: &HashMap<i64, Discipline>,
    scoring: &HashMap<i64, ScoringFormat>,
) -> Vec<BtpMatch> {
    let mut out = Vec::new();
    let Some(matches) = xml::find(t, "Matches") else {
        return out;
    };
    for m in matches.children() {
        // Nur echte, anzeigbare Paarungen – Slots und Spiegel-Einträge
        // tragen kein IsMatch=true.
        if child_bool(m, "IsMatch") != Some(true) {
            continue;
        }
        // From1/From2 verweisen auf Slots im SELBEN Draw wie das Match.
        let draw_id = child_int(m, "DrawID");
        let resolve =
            |planning: Option<i64>| resolve_team(draw_id, planning, slots, entries, players);
        let court_id = child_int(m, "CourtID");
        let court = court_id.and_then(|id| courts.get(&id).cloned());
        // BTP nutzt Winner=0 als „noch kein Sieger" — nur 1/2 sind echte
        // Sieger. Sonst gälte ein Match mit Winner=0 fälschlich als entschieden
        // (Status=beendet) und Team 2 würde als Sieger gewertet.
        let winner = child_int(m, "Winner")
            .and_then(|w| u8::try_from(w).ok())
            .filter(|&w| w == 1 || w == 2);
        // „Auf dem Feld" hängt an der CourtID, nicht am aufgelösten Namen:
        // eine CourtID, die (noch) in keinem Courts-Eintrag steht, ist
        // trotzdem eine Feld-Zuweisung.
        let status = if winner.is_some() {
            MatchStatus::Finished
        } else if court_id.is_some() {
            MatchStatus::OnCourt
        } else {
            MatchStatus::Scheduled
        };
        let (entry1_id, team1) = resolve(child_int(m, "From1"));
        let (entry2_id, team2) = resolve(child_int(m, "From2"));
        out.push(BtpMatch {
            id: child_int(m, "ID").unwrap_or_default(),
            draw_id: draw_id.unwrap_or_default(),
            planning_id: child_int(m, "PlanningID").unwrap_or_default(),
            draw_name: draw_id
                .and_then(|id| draws.get(&id).cloned())
                .unwrap_or_default(),
            discipline: draw_id
                .and_then(|id| disciplines.get(&id).copied())
                .unwrap_or(Discipline::Unknown),
            round_name: child_str(m, "RoundName").unwrap_or_default().to_string(),
            match_num: child_int(m, "MatchNr").filter(|&n| n > 0),
            planned_time: parse_planned_time(m),
            team1,
            team2,
            entry1_id,
            entry2_id,
            court,
            court_id,
            sets: parse_sets(m),
            winner,
            result: MatchResult::from_score_status(child_int(m, "ScoreStatus").unwrap_or(0)),
            status,
            // BTP liefert keinen End-Zeitstempel; die Sync-Engine setzt das.
            finished_at: None,
            // Vorbereitungs-Zustand ist bts-light-eigen; setzt
            // apply_preparation_calls, nicht der Parser.
            preparation_call_ts: None,
            preparation_hall: None,
            // Zählweise über die Stage des Draws; ohne Zuordnung 3×21.
            scoring: draw_id
                .and_then(|id| scoring.get(&id).cloned())
                .unwrap_or_default(),
        });
    }
    out
}

/// Löst die `From`-PlanningID über Slot → Entry → Player auf und liefert
/// `(EntryID, Spieler)`. Der Slot wird im Draw des Matches gesucht
/// (PlanningIDs sind nur dort eindeutig). `(0, [])`, wenn die Kette nicht
/// aufgeht (z. B. noch offener KO-Platz).
fn resolve_team(
    draw_id: Option<i64>,
    planning_id: Option<i64>,
    slots: &HashMap<(i64, i64), i64>,
    entries: &HashMap<i64, Vec<i64>>,
    players: &HashMap<i64, BtpPlayer>,
) -> (i64, Vec<BtpPlayer>) {
    let (Some(draw), Some(planning)) = (draw_id, planning_id) else {
        return (0, Vec::new());
    };
    let Some(entry_id) = slots.get(&(draw, planning)) else {
        return (0, Vec::new());
    };
    let Some(player_ids) = entries.get(entry_id) else {
        return (*entry_id, Vec::new());
    };
    let team = player_ids
        .iter()
        .filter_map(|id| players.get(id).cloned())
        .collect();
    (*entry_id, team)
}

/// Liest die Satz-Ergebnisse aus dem `Sets`-Container eines Matches.
fn parse_sets(m: &Node) -> Vec<(i64, i64)> {
    let Some(sets) = xml::find(m.children(), "Sets") else {
        return Vec::new();
    };
    sets.children()
        .iter()
        .filter_map(|set| Some((child_int(set, "T1")?, child_int(set, "T2")?)))
        .collect()
}

/// Parst den `PlannedTime`-Knoten eines Matches zu einem sortierbaren
/// `YYYYMMDDHHMM`-Schlüssel. BTP liefert die Ansetzung als verschachtelten
/// Knoten (Year/Month/Day/Hour/Minute), nicht als Text. Die Feldnamen werden
/// gross- UND kleingeschrieben akzeptiert (defensiv gegen XML-Casing).
/// `None`, wenn kein PlannedTime-Knoten existiert oder das Jahr fehlt/0 ist.
fn parse_planned_time(m: &Node) -> Option<i64> {
    let pt = xml::find(m.children(), "PlannedTime")?;
    let part = |a: &str, b: &str| child_int(pt, a).or_else(|| child_int(pt, b));
    // Jahr begrenzen – schützt den YYYYMMDDHHMM-Schlüssel vor i64-Overflow bei
    // korruptem XML (reale BTP-Jahre liegen weit innerhalb 1..9999).
    let year = part("Year", "year").filter(|&y| (1..10_000).contains(&y))?;
    let month = part("Month", "month").unwrap_or(0).clamp(0, 12);
    let day = part("Day", "day").unwrap_or(0).clamp(0, 31);
    let hour = part("Hour", "hour").unwrap_or(0).clamp(0, 23);
    let minute = part("Minute", "minute").unwrap_or(0).clamp(0, 59);
    Some(year * 100_000_000 + month * 1_000_000 + day * 10_000 + hour * 100 + minute)
}

// --- Kleine Knoten-Zugriffshelfer -----------------------------------------

fn child_int(node: &Node, id: &str) -> Option<i64> {
    xml::find(node.children(), id)?.value()?.as_int()
}

fn child_str<'a>(node: &'a Node, id: &str) -> Option<&'a str> {
    xml::find(node.children(), id)?.value()?.as_str()
}

fn child_bool(node: &Node, id: &str) -> Option<bool> {
    xml::find(node.children(), id)?.value()?.as_bool()
}

/// Wert eines `Setting` mit gegebener numerischer ID.
fn setting_str(t: &[Node], setting_id: i64) -> Option<String> {
    let settings = xml::find(t, "Settings")?;
    settings.children().iter().find_map(|s| {
        if child_int(s, "ID") == Some(setting_id) {
            child_str(s, "Value").map(String::from)
        } else {
            None
        }
    })
}

/// Wert eines `Setting` als Ganzzahl – akzeptiert sowohl einen Integer- als
/// auch einen String-`Value` (BTP typisiert Settings unterschiedlich).
fn setting_int(t: &[Node], setting_id: i64) -> Option<i64> {
    let settings = xml::find(t, "Settings")?;
    settings.children().iter().find_map(|s| {
        if child_int(s, "ID") == Some(setting_id) {
            let v = xml::find(s.children(), "Value")?.value()?;
            v.as_int()
                .or_else(|| v.as_str()?.trim().parse::<i64>().ok())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::xml::{Node, Value};

    #[test]
    fn missing_tournament_is_an_error() {
        assert!(matches!(parse_snapshot(&[]), Err(ModelError::NoTournament)));
    }

    #[test]
    fn empty_tournament_yields_name_without_matches() {
        // Result > Tournament > Settings > Setting(1001 = Name).
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![Node::group(
                    "Settings",
                    vec![Node::group(
                        "Setting",
                        vec![
                            Node::integer("ID", 1001),
                            Node::string("Value", "Leeres Turnier"),
                        ],
                    )],
                )],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.tournament_name, "Leeres Turnier");
        assert!(snapshot.matches.is_empty());
        // Fehlende Locations-/Courts-Gruppen ergeben leere Listen, kein Fehler.
        assert!(snapshot.locations.is_empty());
        assert!(snapshot.court_infos.is_empty());
    }

    #[test]
    fn match_status_derives_from_winner_and_court() {
        // Bool-Helfer, da xml::Node keinen Bool-Konstruktor hat.
        let is_match = || Node::Item {
            id: "IsMatch".to_string(),
            value: Value::Bool(true),
        };
        let finished = Node::group(
            "Match",
            vec![
                Node::integer("ID", 1),
                is_match(),
                Node::integer("Winner", 1),
            ],
        );
        let on_court = Node::group(
            "Match",
            vec![
                Node::integer("ID", 2),
                is_match(),
                Node::integer("CourtID", 9),
            ],
        );
        let scheduled = Node::group("Match", vec![Node::integer("ID", 3), is_match()]);
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "Courts",
                        vec![Node::group(
                            "Court",
                            vec![Node::integer("ID", 9), Node::string("Name", "Court 9")],
                        )],
                    ),
                    Node::group("Matches", vec![finished, on_court, scheduled]),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        let status: Vec<_> = snapshot.matches.iter().map(|m| &m.status).collect();
        assert_eq!(
            status,
            vec![
                &MatchStatus::Finished,
                &MatchStatus::OnCourt,
                &MatchStatus::Scheduled
            ]
        );
    }

    #[test]
    fn winner_zero_is_not_a_decided_match() {
        // BTP schreibt Winner=0 für „noch kein Sieger". Das darf NICHT als
        // beendetes Match (mit Team 2 als Sieger) durchgehen — sonst entstünde
        // ein falsches Podium in der Siegerermittlung.
        let is_match = || Node::Item {
            id: "IsMatch".to_string(),
            value: Value::Bool(true),
        };
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![Node::group(
                    "Matches",
                    vec![Node::group(
                        "Match",
                        vec![
                            Node::integer("ID", 1),
                            is_match(),
                            Node::integer("Winner", 0),
                        ],
                    )],
                )],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches[0].winner, None);
        assert_eq!(snapshot.matches[0].status, MatchStatus::Scheduled);
    }

    #[test]
    fn resolves_doubles_pairing_over_slot_and_entry() {
        // Doppel-Entry mit zwei Spielern; das Match verweist über From1 auf
        // den Teilnehmer-Slot (PlanningID 100 → Entry 10). From2 fehlt – das
        // Gegner-Team bleibt leer (wie bei einem Freilos).
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "Players",
                        vec![
                            Node::group(
                                "Player",
                                vec![
                                    Node::integer("ID", 1),
                                    Node::string("Lastname", "Müller"),
                                    Node::string("Firstname", "Anna"),
                                    Node::string("MemberID", "08-001234"),
                                    Node::string("Country", "GER"),
                                ],
                            ),
                            // Spieler 2 ohne MemberID/Country.
                            Node::group(
                                "Player",
                                vec![
                                    Node::integer("ID", 2),
                                    Node::string("Lastname", "Schmidt"),
                                    Node::string("Firstname", "Ben"),
                                ],
                            ),
                        ],
                    ),
                    Node::group(
                        "Entries",
                        vec![Node::group(
                            "Entry",
                            vec![
                                Node::integer("ID", 10),
                                Node::integer("Player1ID", 1),
                                Node::integer("Player2ID", 2),
                            ],
                        )],
                    ),
                    Node::group(
                        "Matches",
                        vec![
                            // Teilnehmer-Slot.
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("DrawID", 1),
                                    Node::integer("PlanningID", 100),
                                    Node::integer("EntryID", 10),
                                ],
                            ),
                            // Echtes Match, verweist per From1 auf den Slot.
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("ID", 5),
                                    Node::integer("DrawID", 1),
                                    Node::Item {
                                        id: "IsMatch".to_string(),
                                        value: Value::Bool(true),
                                    },
                                    Node::integer("From1", 100),
                                ],
                            ),
                        ],
                    ),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches.len(), 1);
        let team1 = &snapshot.matches[0].team1;
        let names: Vec<&str> = team1.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Anna Müller", "Ben Schmidt"]);
        assert_eq!(team1[0].member_id.as_deref(), Some("08-001234"));
        assert_eq!(team1[0].nationality.as_deref(), Some("GER"));
        assert_eq!(team1[1].member_id, None);
        assert!(snapshot.matches[0].team2.is_empty());
        // EntryID von Team 1 ist aufgelöst, Team 2 bleibt offen (0).
        assert_eq!(snapshot.matches[0].entry1_id, 10);
        assert_eq!(snapshot.matches[0].entry2_id, 0);
    }

    /// Regression: zwei Draws verwenden beide den Slot PlanningID 100. Ohne
    /// DrawID im Slot-Schlüssel lösen beide Matches zum selben Spieler auf
    /// ("Hilde gegen Hilde"). Jedes Match muss den Slot seines eigenen Draws
    /// treffen.
    #[test]
    fn slots_with_same_planning_id_in_different_draws_do_not_collide() {
        let is_match = || Node::Item {
            id: "IsMatch".to_string(),
            value: Value::Bool(true),
        };
        let player = |id, name| {
            Node::group(
                "Player",
                vec![Node::integer("ID", id), Node::string("Lastname", name)],
            )
        };
        let entry = |id, player_id| {
            Node::group(
                "Entry",
                vec![
                    Node::integer("ID", id),
                    Node::integer("Player1ID", player_id),
                ],
            )
        };
        let slot = |draw, planning, entry_id| {
            Node::group(
                "Match",
                vec![
                    Node::integer("DrawID", draw),
                    Node::integer("PlanningID", planning),
                    Node::integer("EntryID", entry_id),
                ],
            )
        };
        let pairing = |id, draw, from| {
            Node::group(
                "Match",
                vec![
                    Node::integer("ID", id),
                    Node::integer("DrawID", draw),
                    is_match(),
                    Node::integer("From1", from),
                ],
            )
        };
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group("Players", vec![player(1, "Anna"), player(2, "Hilde")]),
                    Node::group("Entries", vec![entry(10, 1), entry(20, 2)]),
                    Node::group(
                        "Matches",
                        vec![
                            // Beide Draws nutzen Slot-PlanningID 100.
                            slot(1, 100, 10),
                            slot(2, 100, 20),
                            pairing(5, 1, 100),
                            pairing(6, 2, 100),
                        ],
                    ),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        let team1_name = |i: usize| snapshot.matches[i].team1[0].name.as_str();
        assert_eq!(team1_name(0), "Anna");
        assert_eq!(team1_name(1), "Hilde");
    }

    #[test]
    fn discipline_resolves_over_draw_and_event() {
        // Match → Draw 1 → Event 7 (GameTypeID 2, GenderID 3) → Mixed.
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "Events",
                        vec![Node::group(
                            "Event",
                            vec![
                                Node::integer("ID", 7),
                                Node::integer("GameTypeID", 2),
                                Node::integer("GenderID", 3),
                            ],
                        )],
                    ),
                    Node::group(
                        "Draws",
                        vec![Node::group(
                            "Draw",
                            vec![
                                Node::integer("ID", 1),
                                Node::integer("EventID", 7),
                                Node::string("Name", "GD"),
                            ],
                        )],
                    ),
                    Node::group(
                        "Matches",
                        vec![Node::group(
                            "Match",
                            vec![
                                Node::integer("ID", 5),
                                Node::integer("DrawID", 1),
                                Node::Item {
                                    id: "IsMatch".to_string(),
                                    value: Value::Bool(true),
                                },
                            ],
                        )],
                    ),
                ],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches[0].discipline, Discipline::Mixed);
    }

    #[test]
    fn scoring_format_from_btp_maps_set_types() {
        let f = |s: ScoringFormat| (s.best_of, s.target_score, s.cap_score, s.interval_at);
        // 0 = klassisch 3×21 (Cap 30, Intervall 11).
        assert_eq!(f(ScoringFormat::from_btp(3, 0, 21)), (3, 21, 30, Some(11)));
        // 306 = „3×15 (21)" → Ende 15, Cap 21, Intervall bei 8 (Fall des Nutzers).
        assert_eq!(f(ScoringFormat::from_btp(3, 306, 15)), (3, 15, 21, Some(8)));
        // 11er-Sätze (301/304/305): Cap 11/15/13, kein reguläres Intervall.
        assert_eq!(f(ScoringFormat::from_btp(5, 301, 11)), (5, 11, 11, None));
        assert_eq!(f(ScoringFormat::from_btp(5, 304, 11)), (5, 11, 15, None));
        assert_eq!(f(ScoringFormat::from_btp(5, 305, 11)), (5, 11, 13, None));
        // Unbekannter SetType → klassisch 21, Satzzahl bleibt erhalten.
        assert_eq!(
            f(ScoringFormat::from_btp(1, 4242, 0)),
            (1, 21, 30, Some(11))
        );
    }

    #[test]
    fn scoring_resolves_via_stage_else_default() {
        // Format 1 = Default 3×21; Format 2 = 3×15 (21). Stage 10 → Format 2.
        // Draw 1 hängt an Stage 10, Draw 2 hat keine Stage → Default.
        let bool_item = |id: &str, v: bool| Node::Item {
            id: id.to_string(),
            value: Value::Bool(v),
        };
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![
                    Node::group(
                        "ScoringFormats",
                        vec![
                            Node::group(
                                "ScoringFormat",
                                vec![
                                    Node::integer("ID", 1),
                                    Node::integer("NumSets", 3),
                                    Node::integer("SetType", 0),
                                    Node::integer("Score", 21),
                                    bool_item("IsDefault", true),
                                ],
                            ),
                            Node::group(
                                "ScoringFormat",
                                vec![
                                    Node::integer("ID", 2),
                                    Node::integer("NumSets", 3),
                                    Node::integer("SetType", 306),
                                    Node::integer("Score", 15),
                                ],
                            ),
                        ],
                    ),
                    Node::group(
                        "Stages",
                        vec![Node::group(
                            "Stage",
                            vec![Node::integer("ID", 10), Node::integer("ScoringFormat", 2)],
                        )],
                    ),
                    Node::group(
                        "Draws",
                        vec![
                            Node::group(
                                "Draw",
                                vec![
                                    Node::integer("ID", 1),
                                    Node::integer("StageID", 10),
                                    Node::string("Name", "HE"),
                                ],
                            ),
                            Node::group(
                                "Draw",
                                vec![Node::integer("ID", 2), Node::string("Name", "DE")],
                            ),
                        ],
                    ),
                    Node::group(
                        "Matches",
                        vec![
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("ID", 5),
                                    Node::integer("DrawID", 1),
                                    bool_item("IsMatch", true),
                                ],
                            ),
                            Node::group(
                                "Match",
                                vec![
                                    Node::integer("ID", 6),
                                    Node::integer("DrawID", 2),
                                    bool_item("IsMatch", true),
                                ],
                            ),
                        ],
                    ),
                ],
            )],
        )];
        let snap = parse_snapshot(&tree).unwrap();
        let m1 = snap.matches.iter().find(|m| m.id == 5).unwrap();
        assert_eq!(
            (
                m1.scoring.target_score,
                m1.scoring.cap_score,
                m1.scoring.interval_at
            ),
            (15, 21, Some(8))
        );
        // Draw 2 ohne Stage → als Standard markiertes Format (3×21).
        let m2 = snap.matches.iter().find(|m| m.id == 6).unwrap();
        assert_eq!(
            (
                m2.scoring.target_score,
                m2.scoring.cap_score,
                m2.scoring.interval_at
            ),
            (21, 30, Some(11))
        );
    }

    /// Baut einen minimalen Snapshot mit gegebenen Hallen und Feldern –
    /// nur für die Tests der Anzeige-Helfer.
    fn label_snapshot(
        locations: Vec<(i64, &str)>,
        courts: Vec<(i64, &str, Option<i64>)>,
    ) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            matches: Vec::new(),
            courts: courts.iter().map(|(_, n, _)| n.to_string()).collect(),
            locations: locations
                .into_iter()
                .map(|(id, name)| BtpLocation {
                    id,
                    name: name.to_string(),
                })
                .collect(),
            court_infos: courts
                .into_iter()
                .map(|(id, name, loc)| BtpCourt {
                    id,
                    name: name.to_string(),
                    location_id: loc,
                    sort_order: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn display_label_prefixes_hall_only_for_multi_hall_tournaments() {
        // Mehr-Hallen-Turnier: Feld 6 liegt in „Halle 2" → Komposit-Label.
        let multi = label_snapshot(
            vec![(1, "Halle 1"), (2, "Halle 2")],
            vec![(101, "6", Some(2)), (102, "1", Some(1))],
        );
        assert!(multi.is_multi_hall());
        assert_eq!(multi.court_display_label(101), "Halle 2 · 6");
        assert_eq!(multi.court_location_name(101), "Halle 2");
        // Ein-Hallen-Turnier: kein Präfix, leerer Hallenname.
        let single = label_snapshot(vec![(1, "Main Location")], vec![(101, "6", Some(1))]);
        assert!(!single.is_multi_hall());
        assert_eq!(single.court_display_label(101), "6");
        assert_eq!(single.court_location_name(101), "");
        // Mehr-Hallen, aber Feld ohne auflösbare Halle → nur der Feldname.
        let orphan = label_snapshot(vec![(1, "Halle 1"), (2, "Halle 2")], vec![(101, "6", None)]);
        assert_eq!(orphan.court_display_label(101), "6");
        assert_eq!(orphan.court_location_name(101), "");
        // Unbekannte CourtID → leeres Label, kein Panik.
        assert_eq!(multi.court_display_label(999), "");
    }

    #[test]
    fn discipline_unknown_when_event_missing() {
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![Node::group(
                    "Matches",
                    vec![Node::group(
                        "Match",
                        vec![
                            Node::integer("ID", 5),
                            Node::integer("DrawID", 1),
                            Node::Item {
                                id: "IsMatch".to_string(),
                                value: Value::Bool(true),
                            },
                        ],
                    )],
                )],
            )],
        )];
        let snapshot = parse_snapshot(&tree).unwrap();
        assert_eq!(snapshot.matches[0].discipline, Discipline::Unknown);
    }

    /// `player_map` muss Vor- und Nachnamen getrennt am `BtpPlayer` ablegen
    /// (BTP `Firstname`/`Lastname`) – die Grundlage für die Broadcast-
    /// Darstellung auf dem Court-Monitor.
    #[test]
    fn player_map_fills_first_and_last_separately() {
        let tree = vec![Node::group(
            "Players",
            vec![
                // Mehrteiliger Nachname: zeigt, dass nichts geraten wird.
                Node::group(
                    "Player",
                    vec![
                        Node::integer("ID", 1),
                        Node::string("Lastname", "van der Berg"),
                        Node::string("Firstname", "Jan"),
                    ],
                ),
                // Spieler nur mit Nachname: Vorname bleibt leer.
                Node::group(
                    "Player",
                    vec![Node::integer("ID", 2), Node::string("Lastname", "Müller")],
                ),
            ],
        )];
        let map = player_map(&tree, &std::collections::HashMap::new());
        let p1 = &map[&1];
        assert_eq!(p1.first, "Jan");
        assert_eq!(p1.last, "van der Berg");
        // `name` bleibt unverändert die kombinierte Form.
        assert_eq!(p1.name, "Jan van der Berg");
        let p2 = &map[&2];
        assert_eq!(p2.first, "");
        assert_eq!(p2.last, "Müller");
        assert_eq!(p2.name, "Müller");
    }

    #[test]
    fn player_map_resolves_club_from_clubid() {
        let tree = vec![
            Node::group(
                "Clubs",
                vec![Node::group(
                    "Club",
                    vec![
                        Node::integer("ID", 7),
                        Node::string("Name", "VfL Lichtenrade"),
                    ],
                )],
            ),
            Node::group(
                "Players",
                vec![
                    Node::group(
                        "Player",
                        vec![
                            Node::integer("ID", 1),
                            Node::string("Lastname", "Anne"),
                            Node::integer("ClubID", 7),
                        ],
                    ),
                    // Spieler ohne ClubID → kein Verein.
                    Node::group(
                        "Player",
                        vec![Node::integer("ID", 2), Node::string("Lastname", "Bernd")],
                    ),
                ],
            ),
        ];
        let clubs = id_name_map(&tree, "Clubs");
        let map = player_map(&tree, &clubs);
        assert_eq!(map[&1].club.as_deref(), Some("VfL Lichtenrade"));
        assert_eq!(map[&2].club, None);
    }

    #[test]
    fn parse_planned_time_builds_sortable_key() {
        let m = Node::group(
            "Match",
            vec![Node::group(
                "PlannedTime",
                vec![
                    Node::integer("Year", 2025),
                    Node::integer("Month", 6),
                    Node::integer("Day", 14),
                    Node::integer("Hour", 13),
                    Node::integer("Minute", 30),
                ],
            )],
        );
        assert_eq!(parse_planned_time(&m), Some(202_506_141_330));
    }

    #[test]
    fn parse_planned_time_is_none_without_node() {
        let m = Node::group("Match", vec![Node::integer("ID", 1)]);
        assert_eq!(parse_planned_time(&m), None);
    }

    #[test]
    fn parse_snapshot_reads_rest_minutes_from_setting_1303() {
        let tree = vec![Node::group(
            "Result",
            vec![Node::group(
                "Tournament",
                vec![Node::group(
                    "Settings",
                    vec![
                        Node::group(
                            "Setting",
                            vec![Node::integer("ID", 1001), Node::string("Value", "T")],
                        ),
                        Node::group(
                            "Setting",
                            vec![Node::integer("ID", 1303), Node::string("Value", "20")],
                        ),
                    ],
                )],
            )],
        )];
        let snap = parse_snapshot(&tree).unwrap();
        assert_eq!(snap.rest_minutes, Some(20));
    }
}
