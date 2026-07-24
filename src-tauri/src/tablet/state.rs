//! Geteilter Zustand zwischen Sync-Loop und Tablet-Server.
//!
//! Der Sync-Loop legt hier den jeweils neuesten BTP-Snapshot ab, der
//! Tablet-Server pflegt die laufenden Court-Sessions. Beide Seiten teilen
//! sich ein `Arc<TabletState>`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use relay_proto::{MonitorCommand, MonitorCommandKind, MonitorDeviceInfo};

use crate::btp::model::{BtpCourt, BtpMatch, BtpSnapshot, Discipline, MatchStatus};

/// Aktuelle Unix-Zeit in Millisekunden.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Obergrenze verfolgter Monitor-Geräte (Missbrauchs-Schutz). Bei
/// Überschreitung wird das am längsten nicht gesehene Gerät verdrängt.
const MAX_MONITOR_DEVICES: usize = 128;

/// Flüchtiger Live-Zustand eines Court-Monitor-Geräts (nicht persistiert –
/// die Feld-Zuweisungen liegen in `monitor-assignments.json`).
#[derive(Debug, Clone, Default)]
struct MonitorLive {
    /// Zeitpunkt des letzten Polls (Unix-ms) – für den Online-Status.
    last_seen_ms: u64,
    /// Offener Fernbefehl (Neu laden / Identifizieren).
    command: Option<MonitorCommand>,
}

/// Akkustand eines Tablets. Liefern nur Android-/Chrome-Tablets – iPads
/// (Safari) geben den Akkustand grundsätzlich nicht her.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TabletBattery {
    /// Ladestand in Prozent (0–100).
    pub percent: i64,
    /// Lädt das Tablet gerade?
    pub charging: bool,
}

/// Laufende Tablet-Sitzung an einem Court.
#[derive(Debug, Clone)]
struct CourtSession {
    /// BTP-Match-ID, das dieses Tablet zählt (0 = noch keins).
    match_id: i64,
    /// Zuletzt vom Tablet gemeldeter Satzstand (Team1, Team2).
    sets: Vec<(i64, i64)>,
    /// Ist die WebSocket-Verbindung des Tablets offen?
    connected: bool,
    /// Zuletzt gemeldeter Akkustand (falls das Tablet ihn liefert).
    battery: Option<TabletBattery>,
    /// Verletzung/Behandlung – das Tablet hat das Spiel unterbrochen.
    injury: bool,
    /// Die Turnierleitung wurde an dieses Feld gerufen.
    official: bool,
}

/// Eine Court-Zeile für die Felder-Übersicht der Turnierleitung.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CourtOverview {
    /// Stabile BTP-CourtID des Felds – die Identität. Feldnamen wiederholen
    /// sich bei Mehr-Hallen-Turnieren, die CourtID nicht.
    pub court_id: i64,
    /// Feldname (Anzeige), z. B. „1" oder „Feld 3".
    pub court: String,
    /// Hallenname (BTP-`Location`) des Felds – Grundlage der hallenweisen
    /// Gruppierung im Frontend. Leerer String bei Ein-Hallen-Turnieren
    /// oder wenn das Feld keiner auflösbaren Halle zugeordnet ist.
    pub location: String,
    /// BTP-Match-ID des aktuellen Spiels (0 = kein Match). Damit erkennt
    /// die Oberfläche, wenn ein Feld ein neues Spiel bekommt (Sprachansage).
    pub match_id: i64,
    /// Anzeigename des Matches, z. B. "HE G1"; leer wenn kein Match.
    pub match_name: String,
    /// Reine Runde aus BTP (`RoundName`), z. B. "VF", "HF", "Finale",
    /// "Spiel um Platz 3" – ohne Draw-Präfix. Grundlage der K.-o.-Runden-Ansage
    /// (ab Viertelfinale). Leer, wenn kein Match / keine Runde.
    pub round_name: String,
    /// Disziplin des aktuellen Matches (für die Sprachansage).
    pub discipline: Discipline,
    /// Klassen-Kürzel („A", „B", …) für die Ansage „Herreneinzel A";
    /// leer, wenn keins erkennbar ist (siehe `model::class_label`).
    pub class_label: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Nationalitäten von Team 1, parallel zu `team1` (leerer String,
    /// wenn unbekannt) – Grundlage der automatischen DE/EN-Ansage.
    pub team1_nationalities: Vec<String>,
    pub team2_nationalities: Vec<String>,
    /// Aktueller Satzstand – vom Tablet, falls aktiv, sonst aus BTP.
    pub sets: Vec<(i64, i64)>,
    pub tablet_connected: bool,
    /// Akkustand des Tablets, falls es ihn liefert (Android/Chrome).
    pub battery: Option<TabletBattery>,
    /// Verletzung/Behandlung läuft an diesem Court.
    pub injury: bool,
    /// Die Turnierleitung wurde an diesen Court gerufen.
    pub official_call: bool,
    /// Welches Team schlägt gerade auf? 1 = team1, 2 = team2, None =
    /// unbekannt. Abgeleitet aus dem Tablet-`court_state`.
    pub serving_team: Option<u8>,
    /// Index (0/1) des konkret aufschlagenden Spielers innerhalb seines
    /// Teams (BWF-Doppelregel; vom Tablet berechnet). None bei Einzel oder
    /// altem Tablet-Stand ohne diese Info.
    pub serving_player: Option<u8>,
    /// Laufende Pause am Feld (BWF-Intervall/Satzpause/Behandlung), 1:1 aus
    /// dem Tablet-`court_state` übernommen: `{kind, endsAt}`. Damit zeigt die
    /// Kombi-Anzeige den Pausen-Countdown direkt am betroffenen Feld. None =
    /// keine Pause. `endsAt` steht in Server-Zeit (vom Tablet so gesetzt).
    pub pause: Option<serde_json::Value>,
    /// Zähltafelbediener für das aktuelle Spiel: bei aktiver Verwaltung der
    /// beim Aufruf zugewiesene Bediener, sonst der pro-Feld-Hinweis (Verlierer
    /// des Vorspiels). Leer, wenn keiner bekannt ist.
    pub scorekeeper: Vec<String>,
    /// `true`, wenn `scorekeeper` aus einer echten Zuweisung stammt (Verwaltung
    /// aktiv) — nur dann wird der Bediener angesagt (ADR 0007).
    pub scorekeeper_assigned: bool,
    /// Feld vom Operator gesperrt (bts-light-seitig): wird nicht automatisch
    /// belegt und im UI rot markiert. BTP kennt keinen Sperr-Zustand.
    pub locked: bool,
    /// Zeitpunkt (Unix-ms) des 1. Aufrufs = seit wann das Spiel auf dem Feld
    /// steht. `None`, wenn kein Spiel auf dem Feld ist. Grundlage des
    /// Aufruf-Timers (hochzählende Uhr + 2./3. Aufruf).
    pub on_court_since_ms: Option<u64>,
    /// Zählformat des aktuellen Matches (Sätze/Zielpunkt/Cap), damit die
    /// Felderübersicht Satz-/Matchball berechnen kann (Plan 16). 0 = kein
    /// Match / unbekannt (dann keine Satzball-Anzeige).
    pub best_of: i64,
    pub target_score: i64,
    pub cap_score: i64,
}

/// Ein noch nicht gespieltes Match, das nach einer Aufgabe kampflos
/// (Walkover) für den Gegner gewertet werden kann.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WalkoverCandidate {
    /// BTP-Match-ID.
    pub match_id: i64,
    /// Draw des Matches (`Match.DrawID`) – fürs Zurückschreiben nach BTP.
    pub draw_id: i64,
    /// Planungsposition im Draw (`Match.PlanningID`).
    pub planning_id: i64,
    /// Runden-/Spielbezeichnung, z. B. "G3".
    pub round_name: String,
    /// Anzeigename des Gegners, der den kampflosen Sieg erhielte.
    pub opponent: String,
    /// Steht die aufgebende Mannschaft auf Seite 1 des Matches? Bestimmt
    /// den Sieger des Walkovers (immer die jeweils andere Seite).
    pub retired_is_team1: bool,
}

/// Vorschlag, nach einer Aufgabe die restlichen Spiele derselben
/// Mannschaft in derselben Disziplin kampflos zu werten. Die konkreten
/// Kandidaten-Spiele werden bei Bedarf frisch aus dem Snapshot ermittelt
/// ([`TabletState::walkover_candidates`]) – so fallen bereits gewertete
/// Spiele von selbst heraus.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WalkoverProposal {
    /// Stabile ID des Vorschlags (= EntryID der aufgebenden Mannschaft).
    pub id: String,
    /// EntryID der aufgebenden Mannschaft.
    pub entry_id: i64,
    /// Anzeigename der aufgebenden Mannschaft.
    pub retired_team: String,
    /// Name der Disziplin/Auslosung, in der aufgegeben wurde, z. B. "HE".
    pub draw_name: String,
    /// Zeitpunkt der Aufgabe (Unix-Millisekunden).
    pub created_at_ms: u64,
}

/// Ein von der Turnierleitung „in Vorbereitung" gerufenes Spiel. BTP kennt
/// keinen Vorbereitungs-Zustand – bts-light verwaltet ihn selbst, genau wie
/// die Walkover-Vorschläge. Je Match gibt es höchstens einen Aufruf.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreparationCall {
    /// BTP-Match-ID des gerufenen Spiels.
    pub match_id: i64,
    /// LocationID der Halle, für die gerufen wurde; `None` bei einem
    /// hallenunabhängigen Aufruf (Ein-Hallen-Turnier).
    pub location_id: Option<i64>,
    /// Zeitpunkt des Aufrufs (Unix-Millisekunden).
    pub called_at_ms: u64,
}

/// Ein Wartender in der Zähltafelbediener-Warteschlange (ADR 0007, Phase 1).
/// Nach Tilos Vorbild ist das der Verlierer eines regulär beendeten Spiels;
/// die FIFO-Reihenfolge bestimmt, wer als Nächstes ein Feld bedient. Ein
/// Doppel steht als EIN Eintrag (das ganze Team).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScorekeeperEntry {
    /// Stabiler Schlüssel für die manuelle Pflege (Vorziehen/Entfernen).
    pub key: String,
    /// Spieler-Namen (1 bei Einzel, 2 bei Doppel).
    pub names: Vec<String>,
    /// BTP-CourtID des Felds, auf dem die Person zuletzt gespielt hat
    /// (0 = manuell hinzugefügt) — für die „bevorzugt aufs eigene Feld"-Regel.
    pub from_court_id: i64,
    /// Zeitpunkt des Einreihens (Unix-ms) — FIFO-Reihenfolge + Mindestpause.
    pub enqueued_ms: u64,
}

/// Geteilt zwischen Sync-Loop und Tablet-Server (`Arc<TabletState>`).
#[derive(Default)]
pub struct TabletState {
    snapshot: RwLock<Option<BtpSnapshot>>,
    /// CourtID → laufende Tablet-Session des Felds.
    courts: RwLock<HashMap<i64, CourtSession>>,
    /// CourtID → (Token, Geräte-ID) des aktuell schiedsenden Tablets
    /// (LAN-Tablet-Übernahme + Reconnect-Erkennung). Fehlt der Eintrag,
    /// ist der Court frei. Geräte-ID leer bei alten Tablet-Seiten.
    active: RwLock<HashMap<i64, (u64, String)>>,
    /// Fortlaufender Zähler, vergibt eindeutige Court-Tokens.
    token_seq: AtomicU64,
    /// CourtID → gespiegelter Spielzustand (JSON) des aktiven Tablets –
    /// wird einem übernehmenden Gerät übergeben.
    court_state: RwLock<HashMap<i64, String>>,
    /// Offene Walkover-Vorschläge nach Aufgaben (je EntryID höchstens einer).
    walkovers: RwLock<Vec<WalkoverProposal>>,
    /// „In Vorbereitung" gerufene Spiele (je Match-ID höchstens einer).
    preparation_calls: RwLock<Vec<PreparationCall>>,
    /// Geräte-ID → Live-Zustand der Court-Monitore (zuletzt gesehen +
    /// offener Fernbefehl). Im LAN-Modus vom Server gepflegt.
    monitor_live: RwLock<HashMap<String, MonitorLive>>,
    /// Im Cloud-Modus die vom Relay gemeldete Monitor-Geräteliste – der
    /// Relay-Client hält sie aktuell, die „Court-Monitore"-Seite liest sie.
    relay_monitor_devices: RwLock<Vec<MonitorDeviceInfo>>,
    /// CourtID → Namen des Verlierer-Teams des zuletzt auf diesem Feld
    /// beendeten Spiels (= voraussichtlicher Zähltafelbediener fürs nächste
    /// Spiel). Vom Sync-Loop beim Übergang OnCourt→Finished gepflegt, weil
    /// BTP beendete Spiele nicht zuverlässig dem Feld zugeordnet behält.
    scorekeeper_by_court: RwLock<HashMap<i64, Vec<String>>>,
    /// Globale FIFO-Warteschlange der Zähltafelbediener (ADR 0007, Phase 1):
    /// Verlierer regulär beendeter Spiele, in Reihenfolge des Einreihens.
    scorekeeper_queue: RwLock<Vec<ScorekeeperEntry>>,
    /// Match-IDs, deren Verlierer bereits eingereiht wurde — Dedup gegen
    /// Mehrfach-Einreihen desselben Spielendes.
    enqueued_finishes: RwLock<HashSet<i64>>,
    /// CourtID → (Match-ID, Bediener-Namen): der beim Feld-Aufruf aus der
    /// Warteschlange gezogene Zähltafelbediener dieses Felds (ADR 0007,
    /// Scheibe 2). Wird geräumt, sobald das Feld frei ist / das Spiel wechselt.
    assigned_scorekeeper: RwLock<HashMap<i64, (i64, Vec<String>)>>,
    /// Pfad der `live-scores.json` (CourtID → Match-ID + Satzstand). Beim
    /// Start gesetzt; jeder `record_score`/`clear_court` schreibt die Datei,
    /// damit ein App-Neustart den laufenden Live-Stand nicht verliert (sonst
    /// fiele der TV auf BTPs 0:0 zurück). `None` = Persistenz aus.
    scores_path: RwLock<Option<PathBuf>>,
    /// Serialisiert die Schreibvorgänge auf `live-scores.json` – mehrere
    /// Felder können (LAN, mehrere WS-Handler) gleichzeitig zählen; ohne das
    /// Lock könnten sich die Schreiber gegenseitig die Datei abschneiden.
    scores_persist_lock: Mutex<()>,
    /// Vom Operator gesperrte Felder (CourtID). bts-light-seitig (BTP kennt das
    /// nicht): gesperrte Felder werden nicht automatisch belegt und rot
    /// markiert. Beim Start aus der Config geseedet, bei Änderung persistiert.
    locked_courts: RwLock<HashSet<i64>>,
    /// CourtID → (Match-ID, Zeitpunkt des 1. Aufrufs in Unix-ms), seit wann
    /// das aktuelle Spiel auf dem Feld steht. Grundlage des Aufruf-Timers; vom
    /// Sync-Loop je Poll abgeglichen.
    on_court_since: RwLock<HashMap<i64, (i64, u64)>>,
    /// Aktuell für die Siegerehrung gewählte Disziplin (Draw-ID), die der
    /// Sieger-Monitor zeigt. `None` = nichts gewählt (Begrüßungsbild). Vom
    /// Operator in bts-light gesetzt; NICHT rotierend — die Ehrung wird
    /// bewusst gesteuert (Leute fotografieren das Podium).
    winners_selection: RwLock<Option<i64>>,
    /// Freitext-Ansagen (Master legt ab; Master + Slaves pollen + sprechen die
    /// für ihre Halle bestimmten). Dedup über die fortlaufende `id`.
    freetext: RwLock<Vec<FreetextItem>>,
    freetext_seq: AtomicU64,
    /// Fehlgeschlagene BTP-Ergebnis-Writes, die der Sync-Loop nachschiebt
    /// (Nachschub-Queue, Cluster A5 — needsync-Prinzip aus Tilos BTS,
    /// robuster: periodischer Retry statt nur beim Reconnect). Je Match
    /// höchstens ein Eintrag, der neueste Stand gewinnt.
    btp_retry: RwLock<Vec<PendingBtpWrite>>,
    /// Match-ID → (letzter ERFOLGREICHER Direkt-Write, Zeitpunkt Unix-ms).
    /// Schließt das Nachschub-Race: Landet ein (langsamer) Queue-Write NACH
    /// einer zwischenzeitlich erfolgreichen Korrektur, erkennt der Flush
    /// das hieran und schreibt die neuere Korrektur sofort erneut
    /// (Selbstheilung statt stillem Überschreiben).
    last_direct_btp_write: RwLock<HashMap<i64, (crate::btp::proto::MatchUpdate, u64)>>,
}

/// Ein fehlgeschlagener BTP-Ergebnis-Write in der Nachschub-Queue.
#[derive(Debug, Clone)]
pub struct PendingBtpWrite {
    pub update: crate::btp::proto::MatchUpdate,
    /// Bezugszeitpunkt (Unix-ms) — Spielende bzw. erste Einreihung. Steuert
    /// das 5-Minuten-Fenster des Spieler-Checkouts und die Höchst-Lebensdauer
    /// (bleibt beim Ersetzen durch einen neueren Stand erhalten).
    pub enqueued_ms: u64,
}

/// Kapazitäts-Deckel der Nachschub-Queue — weit über jedem realen Turnier
/// (148 Ergebnisse am stärksten Tag); schützt nur vor Endlos-Wachstum.
const BTP_RETRY_CAP: usize = 200;

/// Eine manuelle Freitext-Ansage. `hall` = Ziel-Halle (BTP-Location-Name;
/// leer = alle Hallen). `id` ist fortlaufend, damit Sprecher (Master/Slaves)
/// nur neue Einträge ansagen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreetextItem {
    pub id: u64,
    pub hall: String,
    pub text: String,
}

/// Auf Platte gesicherter Live-Stand eines Felds (für den App-Neustart).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedScore {
    match_id: i64,
    sets: Vec<(i64, i64)>,
}

impl TabletState {
    /// Den neuesten BTP-Snapshot ablegen (vom Sync-Loop aufgerufen).
    pub fn set_snapshot(&self, snapshot: BtpSnapshot) {
        *self.snapshot.write().unwrap() = Some(snapshot);
    }

    /// Reiht einen fehlgeschlagenen BTP-Ergebnis-Write in die
    /// Nachschub-Queue ein (Cluster A5). Existiert für das Match schon ein
    /// Eintrag, ersetzt der neuere Stand den alten — der Bezugszeitpunkt
    /// des ERSTEN Fehlschlags bleibt (er steuert das Spieler-Checkout-
    /// Fenster und die Höchst-Lebensdauer).
    pub fn queue_btp_retry(&self, update: crate::btp::proto::MatchUpdate, now: u64) {
        let mut q = self.btp_retry.write().unwrap();
        if let Some(e) = q
            .iter_mut()
            .find(|e| e.update.btp_match_id == update.btp_match_id)
        {
            e.update = update;
            return;
        }
        if q.len() >= BTP_RETRY_CAP {
            q.remove(0); // ältesten opfern — Queue darf nie unbegrenzt wachsen
        }
        q.push(PendingBtpWrite {
            update,
            enqueued_ms: now,
        });
    }

    /// Entfernt den Queue-Eintrag eines Matches — nach erfolgreichem Write
    /// (egal ob durch Nachschub oder den regulären Weg gelungen).
    pub fn clear_btp_retry(&self, match_id: i64) {
        self.btp_retry
            .write()
            .unwrap()
            .retain(|e| e.update.btp_match_id != match_id);
    }

    /// Kopie der aktuellen Nachschub-Queue (für den Flush im Sync-Loop).
    pub fn btp_retries(&self) -> Vec<PendingBtpWrite> {
        self.btp_retry.read().unwrap().clone()
    }

    /// Steht das Match noch in der Nachschub-Queue? Der Flush prüft das
    /// unmittelbar vor jedem Write erneut — ein zwischenzeitlich
    /// erfolgreicher Direkt-Write (Tablet-Retry) hat den Eintrag dann
    /// bereits geräumt und der Nachschub entfällt.
    pub fn btp_retry_pending(&self, match_id: i64) -> bool {
        self.btp_retry
            .read()
            .unwrap()
            .iter()
            .any(|e| e.update.btp_match_id == match_id)
    }

    /// Vermerkt einen ERFOLGREICHEN Direkt-Write (process_result /
    /// Turnierleitungs-Walkover) für die Race-Erkennung des Nachschubs.
    pub fn note_direct_btp_write(&self, update: crate::btp::proto::MatchUpdate, now: u64) {
        self.last_direct_btp_write
            .write()
            .unwrap()
            .insert(update.btp_match_id, (update, now));
    }

    /// Gab es seit `since_ms` einen erfolgreichen Direkt-Write für das
    /// Match? Liefert dessen Stand — der Flush schreibt ihn dann erneut,
    /// falls sein eigener (älterer) Write die Korrektur überholt hat.
    pub fn direct_btp_write_since(
        &self,
        match_id: i64,
        since_ms: u64,
    ) -> Option<crate::btp::proto::MatchUpdate> {
        self.last_direct_btp_write
            .read()
            .unwrap()
            .get(&match_id)
            .filter(|(_, ts)| *ts >= since_ms)
            .map(|(u, _)| u.clone())
    }

    /// Merkt den Zähltafelbediener (Verlierer-Team-Namen) für ein Feld.
    /// Vom Sync-Loop beim Spielende auf dem Feld gesetzt.
    pub fn set_scorekeeper(&self, court_id: i64, loser_names: Vec<String>) {
        self.scorekeeper_by_court
            .write()
            .unwrap()
            .insert(court_id, loser_names);
    }

    // ── Zähltafelbediener-Warteschlange (ADR 0007, Phase 1) ────────────────

    /// Reiht den Verlierer eines regulär beendeten Spiels in die globale
    /// FIFO-Warteschlange ein (Tilos Modell). Idempotent je `match_id`
    /// (Dedup über `enqueued_finishes`), damit ein Spielende nicht mehrfach
    /// zählt. Leere Namen werden ignoriert.
    pub fn enqueue_scorekeeper(
        &self,
        match_id: i64,
        names: Vec<String>,
        from_court_id: i64,
        now_ms: u64,
    ) {
        if names.is_empty() {
            return;
        }
        {
            let mut done = self.enqueued_finishes.write().unwrap();
            if !done.insert(match_id) {
                return; // schon eingereiht
            }
        }
        self.scorekeeper_queue
            .write()
            .unwrap()
            .push(ScorekeeperEntry {
                key: format!("m{match_id}-{now_ms}"),
                names,
                from_court_id,
                enqueued_ms: now_ms,
            });
    }

    /// Manuell einen Wartenden hinzufügen (nicht aus einem Spielende).
    pub fn add_scorekeeper_manual(&self, names: Vec<String>, now_ms: u64) {
        let names: Vec<String> = names
            .into_iter()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            return;
        }
        let mut q = self.scorekeeper_queue.write().unwrap();
        let key = format!("x{}-{}", now_ms, q.len());
        q.push(ScorekeeperEntry {
            key,
            names,
            from_court_id: 0,
            enqueued_ms: now_ms,
        });
    }

    /// Aktuelle Warteschlange (FIFO-Reihenfolge) für die Anzeige.
    pub fn scorekeeper_queue(&self) -> Vec<ScorekeeperEntry> {
        self.scorekeeper_queue.read().unwrap().clone()
    }

    /// Einen Wartenden aus der Schlange entfernen (per Schlüssel).
    pub fn remove_scorekeeper(&self, key: &str) {
        self.scorekeeper_queue
            .write()
            .unwrap()
            .retain(|e| e.key != key);
    }

    /// Einen Wartenden an den Anfang der Schlange ziehen (als Nächsten dran).
    pub fn advance_scorekeeper(&self, key: &str) {
        let mut q = self.scorekeeper_queue.write().unwrap();
        if let Some(pos) = q.iter().position(|e| e.key == key) {
            let e = q.remove(pos);
            q.insert(0, e);
        }
    }

    /// Weist dem Feld beim Aufruf einen Zähltafelbediener aus der Warteschlange
    /// zu (ADR 0007, Scheibe 2): bevorzugt jemanden, der zuletzt AUF DIESEM Feld
    /// gespielt hat (`from_court_id`), sonst den ältesten Wartenden. Idempotent
    /// je (Feld, Match): steht schon ein Bediener für genau dieses Spiel, passiert
    /// nichts. Ist die Schlange leer, bleibt das Feld ohne Bediener.
    pub fn assign_scorekeeper_for_court(&self, court_id: i64, match_id: i64) {
        {
            let assigned = self.assigned_scorekeeper.read().unwrap();
            if assigned.get(&court_id).map(|(m, _)| *m) == Some(match_id) {
                return; // schon zugewiesen für dieses Spiel
            }
        }
        let mut q = self.scorekeeper_queue.write().unwrap();
        // Bevorzugt „eigenes Feld", sonst der Älteste (Index 0, FIFO).
        let pos = q
            .iter()
            .position(|e| e.from_court_id == court_id)
            .or(if q.is_empty() { None } else { Some(0) });
        if let Some(pos) = pos {
            let e = q.remove(pos);
            self.assigned_scorekeeper
                .write()
                .unwrap()
                .insert(court_id, (match_id, e.names));
        }
    }

    /// Anzuzeigender Zähltafelbediener eines Felds für Tablet/ferne Halle:
    /// zugewiesener Bediener (Verwaltung aktiv) mit Flag `true`, sonst der
    /// pro-Feld-Hinweis mit `false`. Nur bei `true` wird er angesagt.
    pub fn scorekeeper_display(&self, court_id: i64) -> (Vec<String>, bool) {
        if let Some(names) = self.assigned_scorekeeper(court_id) {
            (names, true)
        } else {
            (self.scorekeeper(court_id), false)
        }
    }

    /// Zugewiesener Zähltafelbediener eines Felds (Namen), falls vorhanden.
    pub fn assigned_scorekeeper(&self, court_id: i64) -> Option<Vec<String>> {
        self.assigned_scorekeeper
            .read()
            .unwrap()
            .get(&court_id)
            .map(|(_, names)| names.clone())
    }

    /// Räumt Bediener-Zuweisungen für Felder, die nicht mehr mit demselben
    /// Match belegt sind (Feld frei / Spiel gewechselt / beendet). `active` =
    /// CourtID → aktuell dort laufende Match-ID.
    pub fn retain_scorekeeper_assignments(&self, active: &HashMap<i64, i64>) {
        self.assigned_scorekeeper
            .write()
            .unwrap()
            .retain(|court_id, (match_id, _)| active.get(court_id) == Some(match_id));
    }

    /// Alle Bediener-Zuweisungen löschen. Wird gerufen, sobald die Verwaltung
    /// aus ist, damit keine veraltete Zuweisung in der Anzeige hängen bleibt.
    pub fn clear_scorekeeper_assignments(&self) {
        self.assigned_scorekeeper.write().unwrap().clear();
    }

    /// Gesperrte Felder beim Start aus der Config übernehmen.
    pub fn set_locked_courts(&self, ids: impl IntoIterator<Item = i64>) {
        *self.locked_courts.write().unwrap() = ids.into_iter().collect();
    }

    /// Feld sperren (`true`) oder entsperren (`false`).
    pub fn set_court_locked(&self, court_id: i64, locked: bool) {
        let mut set = self.locked_courts.write().unwrap();
        if locked {
            set.insert(court_id);
        } else {
            set.remove(&court_id);
        }
    }

    /// Aktuelle Sperrliste (für Persistenz + Auto-Vergabe).
    pub fn locked_courts(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.locked_courts.read().unwrap().iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// Ist das Feld gesperrt?
    pub fn is_court_locked(&self, court_id: i64) -> bool {
        self.locked_courts.read().unwrap().contains(&court_id)
    }

    /// Gleicht je Poll ab, seit wann das aktuelle Spiel auf dem Feld steht
    /// (= 1. Aufruf). `oncourt` bildet CourtID → aktuelle Match-ID. Ein neues
    /// oder gewechseltes Spiel wird mit `now` gestempelt; verlässt ein Spiel
    /// das Feld, fällt sein Eintrag weg. Idempotent – mehrfacher Aufruf mit
    /// gleichem Stand ändert die Zeitstempel nicht.
    pub fn reconcile_on_court(&self, oncourt: &HashMap<i64, i64>, now: u64) {
        let mut map = self.on_court_since.write().unwrap();
        // Felder vergessen, auf denen jetzt kein bzw. ein anderes Spiel steht.
        map.retain(|court_id, (mid, _)| oncourt.get(court_id) == Some(mid));
        // Neu hinzugekommene Spiele stempeln (gewechselte sind oben rausgeflogen).
        for (&court_id, &mid) in oncourt {
            map.entry(court_id).or_insert((mid, now));
        }
    }

    /// Zeitpunkt (Unix-ms) des 1. Aufrufs für ein Feld, sofern dort das
    /// angegebene Match steht.
    pub(crate) fn on_court_since_ms(&self, court_id: i64, match_id: i64) -> Option<u64> {
        self.on_court_since
            .read()
            .unwrap()
            .get(&court_id)
            .filter(|(mid, _)| *mid == match_id)
            .map(|(_, ts)| *ts)
    }

    /// Voraussichtlicher Zähltafelbediener eines Felds (leer, wenn keiner).
    pub fn scorekeeper(&self, court_id: i64) -> Vec<String> {
        self.scorekeeper_by_court
            .read()
            .unwrap()
            .get(&court_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Kopie des aktuellen BTP-Snapshots (oder `None`, falls noch keiner
    /// geladen ist) – für Commands, die den Stand frisch auswerten.
    pub fn snapshot_clone(&self) -> Option<BtpSnapshot> {
        self.snapshot.read().unwrap().clone()
    }

    /// Turniername des aktuellen Snapshots (leer, falls noch keiner geladen
    /// ist) – für die Leerlauf-Anzeige des Court-Monitors.
    pub fn tournament_name(&self) -> String {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.tournament_name.clone())
            .unwrap_or_default()
    }

    /// Alle Court-Namen des Turniers (BTP-Reihenfolge) – nur für Tests und
    /// Anzeigen, die keine Identität brauchen. Adressen/QR-Codes nutzen
    /// [`TabletState::courts`].
    pub fn court_names(&self) -> Vec<String> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_infos.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Alle Felder des Turniers mit Identität (CourtID) und Anzeigenamen –
    /// Grundlage der Tablet-Adressen, QR-Codes und Monitor-Zuordnungen.
    pub fn courts(&self) -> Vec<BtpCourt> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_infos.clone())
            .unwrap_or_default()
    }

    /// CourtID → Feldname aller Felder (für die Monitor-Geräteliste).
    pub fn court_name_map(&self) -> HashMap<i64, String> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| {
                s.court_infos
                    .iter()
                    .map(|c| (c.id, c.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Anzeige-Bezeichnung eines Felds für Monitore und Tablets. Bei einem
    /// Mehr-Hallen-Turnier `"{Halle} · {Feld}"`, sonst nur der Feldname.
    /// Leer, wenn die CourtID kein bekanntes Feld ist.
    pub fn court_display_label(&self, court_id: i64) -> String {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_display_label(court_id))
            .unwrap_or_default()
    }

    /// Hallenname (BTP-Location) eines Felds; leer bei Ein-Hallen-Turnieren
    /// oder unbekanntem Feld. Für die hallengefilterte Cloud-Ansage.
    pub fn court_hall(&self, court_id: i64) -> String {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.court_location_name(court_id))
            .unwrap_or_default()
    }

    /// Das Match, das BTP gerade diesem Feld (per CourtID) zugewiesen hat.
    pub fn match_for_court(&self, court_id: i64) -> Option<BtpMatch> {
        let guard = self.snapshot.read().unwrap();
        let snap = guard.as_ref()?;
        snap.matches
            .iter()
            .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court_id))
            .cloned()
    }

    /// (DrawID, PlanningID) eines Matches per Match-ID – zum Adressieren eines
    /// `SENDUPDATE` (BTP braucht ID + DrawID + PlanningID). `None`, wenn das
    /// Match nicht im aktuellen Snapshot ist.
    pub fn match_planning(&self, match_id: i64) -> Option<(i64, i64)> {
        let guard = self.snapshot.read().unwrap();
        let snap = guard.as_ref()?;
        snap.matches
            .iter()
            .find(|m| m.id == match_id)
            .map(|m| (m.draw_id, m.planning_id))
    }

    /// Tablet hat sich für ein Feld verbunden. `match_id` startet auf 0 –
    /// den echten Wert setzt der erste `record_score`.
    pub fn attach_tablet(&self, court_id: i64) {
        self.courts
            .write()
            .unwrap()
            .entry(court_id)
            .or_insert(CourtSession {
                match_id: 0,
                sets: Vec::new(),
                connected: true,
                battery: None,
                injury: false,
                official: false,
            })
            .connected = true;
    }

    /// Tablet-WebSocket für ein Feld ist geschlossen.
    pub fn detach_tablet(&self, court_id: i64) {
        if let Some(session) = self.courts.write().unwrap().get_mut(&court_id) {
            session.connected = false;
        }
    }

    /// Satzstand vom Tablet übernehmen.
    pub fn record_score(&self, court_id: i64, match_id: i64, sets: Vec<(i64, i64)>) {
        {
            let mut courts = self.courts.write().unwrap();
            let session = courts.entry(court_id).or_insert(CourtSession {
                match_id,
                sets: Vec::new(),
                connected: true,
                battery: None,
                injury: false,
                official: false,
            });
            session.match_id = match_id;
            session.sets = sets;
        }
        // Stand auf Platte sichern, damit ein App-Neustart ihn behält.
        self.persist_scores();
    }

    /// Pfad der Live-Score-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_scores_path(&self, path: PathBuf) {
        *self.scores_path.write().unwrap() = Some(path);
    }

    /// Live-Stände beim Start aus der Datei laden. Die wiederhergestellten
    /// Sessions sind `connected: false` (kein Tablet-WebSocket offen) – der
    /// Stand wird trotzdem angezeigt/gepusht (siehe `apply_tablet_scores`),
    /// bis das Tablet zurückkehrt oder das Match wechselt.
    pub fn load_scores(&self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return; // keine Datei (erster Start) → nichts zu tun
        };
        let Ok(data) = serde_json::from_str::<HashMap<i64, PersistedScore>>(&text) else {
            tracing::warn!("live-scores.json unlesbar – ignoriere");
            return;
        };
        let mut courts = self.courts.write().unwrap();
        for (court_id, ps) in data {
            courts.entry(court_id).or_insert(CourtSession {
                match_id: ps.match_id,
                sets: ps.sets,
                connected: false,
                battery: None,
                injury: false,
                official: false,
            });
        }
    }

    /// Aktuellen Live-Stand aller Felder in die Datei schreiben (best effort:
    /// Schreibfehler dürfen das Zählen nie stören). No-op, wenn kein Pfad
    /// gesetzt ist (z. B. in Tests).
    fn persist_scores(&self) {
        let Some(path) = self.scores_path.read().unwrap().clone() else {
            return;
        };
        // Schreiber serialisieren: verhindert, dass zwei gleichzeitige
        // record_score den Temp-Pfad oder die Zieldatei gegenseitig zerlegen.
        let _guard = self.scores_persist_lock.lock().unwrap();
        let data: HashMap<i64, PersistedScore> = {
            let courts = self.courts.read().unwrap();
            courts
                .iter()
                .filter(|(_, s)| !s.sets.is_empty())
                .map(|(c, s)| {
                    (
                        *c,
                        PersistedScore {
                            match_id: s.match_id,
                            sets: s.sets.clone(),
                        },
                    )
                })
                .collect()
        };
        if let Ok(json) = serde_json::to_string(&data) {
            // Atomar schreiben: erst in eine Temp-Datei, dann umbenennen –
            // so liegt nie eine halb geschriebene live-scores.json vor (ein
            // Absturz mitten im Schreiben würde sie sonst korrumpieren).
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Akkustand des Tablets an einem Feld übernehmen.
    pub fn record_battery(&self, court_id: i64, percent: i64, charging: bool) {
        let mut courts = self.courts.write().unwrap();
        courts
            .entry(court_id)
            .or_insert(CourtSession {
                match_id: 0,
                sets: Vec::new(),
                connected: true,
                battery: None,
                injury: false,
                official: false,
            })
            .battery = Some(TabletBattery { percent, charging });
    }

    /// Meldungs-Zustand (Verletzung / Turnierleitung gerufen) des Felds setzen.
    pub fn record_alert(&self, court_id: i64, injury: bool, official: bool) {
        let mut courts = self.courts.write().unwrap();
        let session = courts.entry(court_id).or_insert(CourtSession {
            match_id: 0,
            sets: Vec::new(),
            connected: true,
            battery: None,
            injury: false,
            official: false,
        });
        session.injury = injury;
        session.official = official;
    }

    /// Beansprucht das Feld für ein Tablet und gibt dessen Token zurück.
    /// Ein bereits aktives Tablet wird dadurch abgelöst (Tablet-Übernahme).
    /// `device_id` = persistente Geräte-Kennung des Tablets (leer bei alten
    /// Tablet-Seiten) — Grundlage der Reconnect-Erkennung.
    pub fn claim_court(&self, court_id: i64, device_id: &str) -> u64 {
        let token = self.token_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.active
            .write()
            .unwrap()
            .insert(court_id, (token, device_id.to_string()));
        token
    }

    /// Ist `token` noch das aktive Tablet dieses Felds?
    pub fn is_court_active(&self, court_id: i64, token: u64) -> bool {
        self.active
            .read()
            .unwrap()
            .get(&court_id)
            .map(|(t, _)| *t == token)
            .unwrap_or(false)
    }

    /// Wird das Feld bereits von einem Tablet geschiedst?
    pub fn court_occupied(&self, court_id: i64) -> bool {
        self.active.read().unwrap().contains_key(&court_id)
    }

    /// Hält GENAU DIESES Gerät das Feld gerade? (Reconnect-Erkennung: das
    /// zurückkehrende Tablet darf seine eigene, tote Session nahtlos
    /// ablösen, ohne das „Feld belegt"-Overlay zu sehen.) Leere Geräte-IDs
    /// zählen nie als „dasselbe Gerät" (alte Tablet-Seiten).
    pub fn court_held_by_device(&self, court_id: i64, device_id: &str) -> bool {
        !device_id.is_empty()
            && self
                .active
                .read()
                .unwrap()
                .get(&court_id)
                .map(|(_, d)| d == device_id)
                .unwrap_or(false)
    }

    /// Gibt das Feld frei – nur, wenn `token` noch der aktive ist.
    pub fn release_court(&self, court_id: i64, token: u64) {
        let mut active = self.active.write().unwrap();
        if active.get(&court_id).map(|(t, _)| *t) == Some(token) {
            active.remove(&court_id);
        }
    }

    /// Spiegelt den Spielzustand des aktiven Tablets am Feld.
    pub fn set_court_state(&self, court_id: i64, state: String) {
        self.court_state.write().unwrap().insert(court_id, state);
    }

    /// Liefert den gespiegelten Spielzustand eines Felds (für die Übernahme).
    pub fn court_state(&self, court_id: i64) -> Option<String> {
        self.court_state.read().unwrap().get(&court_id).cloned()
    }

    /// Court-Session entfernen (nach übermitteltem Ergebnis).
    pub fn clear_court(&self, court_id: i64) {
        self.courts.write().unwrap().remove(&court_id);
        // Gespiegelten Spielstand löschen, sonst bekäme ein nach dem Ergebnis
        // neu/ersatzweise verbundenes Tablet via StateRestore kurz den BEENDETEN
        // Stand (Render-Blitz, im schmalen Fenster sogar Doppel-Submit). Wird nur
        // nach Ergebnis-Submit aufgerufen (nicht beim Disconnect), daher bleibt
        // der Crash-Restore eines laufenden Spiels unberührt.
        self.court_state.write().unwrap().remove(&court_id);
        // Entfernten Stand auch aus der Datei nehmen.
        self.persist_scores();
    }

    /// Hinterlegt einen Walkover-Vorschlag. Je EntryID gibt es höchstens
    /// einen – ein erneuter für dieselbe Mannschaft ersetzt den alten.
    pub fn add_walkover_proposal(&self, proposal: WalkoverProposal) {
        let mut list = self.walkovers.write().unwrap();
        list.retain(|p| p.entry_id != proposal.entry_id);
        list.push(proposal);
    }

    /// Alle offenen Walkover-Vorschläge.
    pub fn walkover_proposals(&self) -> Vec<WalkoverProposal> {
        self.walkovers.read().unwrap().clone()
    }

    /// Entfernt einen Walkover-Vorschlag (umgesetzt oder verworfen).
    pub fn remove_walkover_proposal(&self, id: &str) {
        self.walkovers.write().unwrap().retain(|p| p.id != id);
    }

    /// Noch nicht gespielte Matches einer Mannschaft (per EntryID) – die
    /// Kandidaten für eine kampflose Wertung nach deren Aufgabe. Nur Spiele
    /// mit bereits feststehendem Gegner; offene KO-Plätze bleiben außen vor.
    pub fn walkover_candidates(&self, entry_id: i64) -> Vec<WalkoverCandidate> {
        if entry_id == 0 {
            return Vec::new();
        }
        let guard = self.snapshot.read().unwrap();
        let Some(snap) = guard.as_ref() else {
            return Vec::new();
        };
        snap.matches
            .iter()
            .filter(|m| m.status == MatchStatus::Scheduled)
            .filter_map(|m| {
                let retired_is_team1 = m.entry1_id == entry_id;
                if !retired_is_team1 && m.entry2_id != entry_id {
                    return None;
                }
                let opponent = if retired_is_team1 { &m.team2 } else { &m.team1 };
                if opponent.is_empty() {
                    return None; // Gegner steht noch nicht fest
                }
                Some(WalkoverCandidate {
                    match_id: m.id,
                    draw_id: m.draw_id,
                    planning_id: m.planning_id,
                    round_name: m.round_name.clone(),
                    opponent: opponent
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    retired_is_team1,
                })
            })
            .collect()
    }

    // ─────────────────────────── Spiele in Vorbereitung ───────────────────

    /// Hinterlegt einen „in Vorbereitung"-Aufruf. Je Match-ID gibt es
    /// höchstens einen – ein erneuter für dasselbe Match ersetzt den alten.
    pub fn add_preparation_call(&self, call: PreparationCall) {
        let mut list = self.preparation_calls.write().unwrap();
        list.retain(|c| c.match_id != call.match_id);
        list.push(call);
    }

    /// Alle aktuell gerufenen „in Vorbereitung"-Spiele.
    pub fn preparation_calls(&self) -> Vec<PreparationCall> {
        self.preparation_calls.read().unwrap().clone()
    }

    /// Entfernt den Aufruf eines Matches (zurückgenommen).
    pub fn remove_preparation_call(&self, match_id: i64) {
        self.preparation_calls
            .write()
            .unwrap()
            .retain(|c| c.match_id != match_id);
    }

    /// Stempelt die aktiven Vorbereitungs-Aufrufe in den Snapshot. Aufrufe,
    /// deren Match nicht mehr ruf-bar ist (auf Court gewechselt, beendet,
    /// verschwunden oder eine Mannschaft nicht mehr gesetzt), werden dabei
    /// verworfen – so bleiben keine Geister-Aufrufe stehen. Für jeden
    /// überlebenden Aufruf werden die transienten Felder
    /// `preparation_call_ts` und `preparation_hall` des zugehörigen Matches
    /// gesetzt.
    pub fn apply_preparation_calls(&self, snapshot: &mut BtpSnapshot) {
        let mut calls = self.preparation_calls.write().unwrap();
        // Match-IDs, die im Snapshot noch ruf-bar sind: eingeplant und mit
        // zwei feststehenden Mannschaften – dieselbe Bedingung wie die
        // Kandidaten-Liste, damit kein Aufruf ohne sichtbares Match bleibt.
        let callable: std::collections::HashSet<i64> = snapshot
            .matches
            .iter()
            .filter(|m| {
                m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
            })
            .map(|m| m.id)
            .collect();
        // Aufrufe ohne (noch) ruf-bares Match fallen heraus.
        calls.retain(|c| callable.contains(&c.match_id));
        for call in calls.iter() {
            // Hallenname aus der LocationID auflösen (None → kein Halleneintrag).
            let hall = call.location_id.and_then(|lid| {
                snapshot
                    .locations
                    .iter()
                    .find(|l| l.id == lid)
                    .map(|l| l.name.clone())
            });
            if let Some(m) = snapshot.matches.iter_mut().find(|m| m.id == call.match_id) {
                m.preparation_call_ts = Some(call.called_at_ms);
                m.preparation_hall = hall;
            }
        }
    }

    /// Felder (CourtIDs) mit verbundenem Tablet – diese treiben ihren
    /// Live-Score selbst.
    pub fn active_courts(&self) -> Vec<i64> {
        self.courts
            .read()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.connected)
            .map(|(c, _)| *c)
            .collect()
    }

    /// Überschreibt im Snapshot die Sätze jedes tablet-getriebenen Matches
    /// mit dem Tablet-Stand. So pusht die Liveticker-Pipeline den
    /// Tablet-Score statt BTPs veraltetem Poll-Wert. Greift, sobald eine
    /// Session zum selben Match einen Stand hat – BEWUSST OHNE
    /// `connected`-Prüfung: Ein kurzer WebSocket-Aussetzer (Router weg,
    /// Display gesperrt) oder ein App-Neustart (Stand aus `live-scores.json`
    /// wiederhergestellt, Tablet noch nicht zurück) darf den Liveticker
    /// nicht auf BTPs 0:0 zurückwerfen. `match_id == m.id` schützt gegen
    /// Match-Wechsel, `!is_empty()` gegen das Überschreiben mit Leerstand.
    pub fn apply_tablet_scores(&self, snapshot: &mut BtpSnapshot) {
        let courts = self.courts.read().unwrap();
        for m in &mut snapshot.matches {
            let Some(court_id) = m.court_id else {
                continue;
            };
            if let Some(session) = courts.get(&court_id) {
                if session.match_id == m.id && !session.sets.is_empty() {
                    m.sets = session.sets.clone();
                }
            }
        }
    }

    /// Felder-Übersicht für die Turnierleitung – je Court das aktuelle
    /// Match mit Live-Satzstand und Tablet-Status.
    /// Podien aller ausgespielten Disziplinen (Sieger-Monitor). Leitet aus dem
    /// aktuellen Snapshot ab (siehe `tablet::winners`).
    pub fn discipline_results(&self) -> Vec<crate::tablet::winners::DisciplineResult> {
        let guard = self.snapshot.read().unwrap();
        match guard.as_ref() {
            Some(snap) => crate::tablet::winners::discipline_results(snap),
            None => Vec::new(),
        }
    }

    /// Setzt die für die Siegerehrung gewählte Disziplin (`None` = nichts).
    pub fn set_winners_selection(&self, draw_id: Option<i64>) {
        *self.winners_selection.write().unwrap() = draw_id;
    }

    /// Aktuell für die Siegerehrung gewählte Disziplin (Draw-ID), falls eine.
    pub fn winners_selection(&self) -> Option<i64> {
        *self.winners_selection.read().unwrap()
    }

    /// Eine Freitext-Ansage ablegen (Master). `hall` leer = alle Hallen.
    /// Liefert die neue laufende ID.
    pub fn publish_freetext(&self, hall: String, text: String) -> u64 {
        // Längen begrenzen – konsistent mit dem Relay-Cap, kein Byte-Panic.
        let text: String = text.chars().take(1000).collect();
        let hall: String = hall.chars().take(128).collect();
        // ID monoton AUCH über einen Master-Neustart: auf mindestens die
        // aktuelle Uhrzeit (ms) heben. Sonst begännen die IDs nach Neustart
        // wieder klein und ein Slave mit gemerkter `lastId` verstummte, bis die
        // ID seinen Stand übersteigt.
        self.freetext_seq.fetch_max(now_ms(), Ordering::Relaxed);
        let id = self.freetext_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let mut g = self.freetext.write().unwrap();
        g.push(FreetextItem { id, hall, text });
        // Nur die letzten 50 behalten (Speicher beschränken).
        let len = g.len();
        if len > 50 {
            g.drain(0..len - 50);
        }
        id
    }

    /// Freitexte mit `id > since`, die für `hall` bestimmt sind. Eine leere
    /// Instanz-Halle (`hall`) bekommt ALLE; sonst die an „alle" oder an genau
    /// diese Halle gerichteten.
    pub fn freetext_since(&self, hall: &str, since: u64) -> Vec<FreetextItem> {
        let h = hall.trim();
        self.freetext
            .read()
            .unwrap()
            .iter()
            .filter(|f| f.id > since)
            .filter(|f| {
                let target = f.hall.trim();
                h.is_empty() || target.is_empty() || target.eq_ignore_ascii_case(h)
            })
            .cloned()
            .collect()
    }

    pub fn overview(&self) -> Vec<CourtOverview> {
        let guard = self.snapshot.read().unwrap();
        let Some(snap) = guard.as_ref() else {
            return Vec::new();
        };
        let courts = self.courts.read().unwrap();
        snap.court_infos
            .iter()
            .map(|court| {
                let m = snap
                    .matches
                    .iter()
                    .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court.id));
                let session = courts.get(&court.id);
                let tablet_connected = session.map(|s| s.connected).unwrap_or(false);
                // Satzstand vom Tablet, sobald dessen Session zum selben Match
                // einen Stand hat – BEWUSST OHNE `connected`-Prüfung (wie
                // `monitor_court`/`apply_tablet_scores`): ein kurzer Aussetzer
                // oder ein App-Neustart darf die Übersicht nicht auf BTPs 0:0
                // zurückwerfen. `tablet_connected` bleibt rein der Online-Indikator.
                let sets = match (session, m) {
                    (Some(s), Some(mm)) if s.match_id == mm.id && !s.sets.is_empty() => {
                        s.sets.clone()
                    }
                    (_, Some(mm)) => mm.sets.clone(),
                    _ => Vec::new(),
                };
                let nationalities = |team: &[crate::btp::model::BtpPlayer]| {
                    team.iter()
                        .map(|p| p.nationality.clone().unwrap_or_default())
                        .collect::<Vec<String>>()
                };
                // Tablet-court_state EINMAL lesen + parsen — so sind Aufschlag-
                // und Pause-Info garantiert vom selben Stand abgeleitet (kein
                // zweiter Lock, kein doppeltes Parsen).
                let court_state_json: Option<serde_json::Value> = self
                    .court_state
                    .read()
                    .unwrap()
                    .get(&court.id)
                    .and_then(|cs| serde_json::from_str(cs).ok());
                // Aufschlag-Info aus dem court_state: (team 1/2, optional
                // Spieler-Index 0/1). Bevorzugt das Tablet-berechnete
                // `serving:{team,index}`; Fallback auf servingSide/teamOnSide.
                let serving_info: Option<(u8, Option<u8>)> =
                    court_state_json.as_ref().and_then(|v| {
                        if let Some(s) = v.get("serving").filter(|s| !s.is_null()) {
                            let team = if s.get("team")?.as_str()? == "a" {
                                1u8
                            } else {
                                2u8
                            };
                            let idx = s.get("index").and_then(|i| i.as_u64()).map(|i| i as u8);
                            return Some((team, idx));
                        }
                        // Fallback (altes Tablet ohne `serving`): nur Team.
                        let serving = v.get("servingSide")?.as_str()?;
                        let team_a = v.get("teamOnSide")?.get("a")?.as_str()?;
                        Some((if serving == team_a { 1u8 } else { 2u8 }, None))
                    });
                // Laufende Pause (BWF-Intervall/Satzpause/Behandlung) — 1:1 für
                // den Kombi-Pausen-Countdown.
                let pause_info: Option<serde_json::Value> = court_state_json
                    .as_ref()
                    .and_then(|v| v.get("pause").filter(|p| !p.is_null()).cloned());
                // Zugewiesener Zähltafelbediener (einmal lesen, für scorekeeper
                // + scorekeeper_assigned wiederverwendet).
                let assigned_sk = if m.is_some() {
                    self.assigned_scorekeeper(court.id)
                } else {
                    None
                };
                CourtOverview {
                    court_id: court.id,
                    court: court.name.clone(),
                    // Hallenname nur bei Mehr-Hallen-Turnieren; sonst leer.
                    location: snap.court_location_name(court.id),
                    match_id: m.map(|mm| mm.id).unwrap_or(0),
                    match_name: m
                        .map(|mm| {
                            format!("{} {}", mm.draw_name, mm.round_name)
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default(),
                    round_name: m.map(|mm| mm.round_name.clone()).unwrap_or_default(),
                    discipline: m.map(|mm| mm.discipline).unwrap_or(Discipline::Unknown),
                    class_label: m.map(|mm| mm.class_label.clone()).unwrap_or_default(),
                    team1: m
                        .map(|mm| mm.team1.iter().map(|p| p.name.clone()).collect())
                        .unwrap_or_default(),
                    team2: m
                        .map(|mm| mm.team2.iter().map(|p| p.name.clone()).collect())
                        .unwrap_or_default(),
                    team1_nationalities: m.map(|mm| nationalities(&mm.team1)).unwrap_or_default(),
                    team2_nationalities: m.map(|mm| nationalities(&mm.team2)).unwrap_or_default(),
                    sets,
                    tablet_connected,
                    battery: session.and_then(|s| s.battery),
                    injury: session.map(|s| s.injury).unwrap_or(false),
                    official_call: session.map(|s| s.official).unwrap_or(false),
                    // Aufschlagendes Team + konkreter Spieler aus dem
                    // Tablet-court_state. Bevorzugt das vom Tablet berechnete
                    // `serving: {team, index}` (BWF-Doppelregel, Spieler-genau);
                    // fällt sonst auf die servingSide/teamOnSide-Ableitung
                    // zurück (nur Team, für alte Tablet-Stände).
                    serving_team: serving_info.map(|(t, _)| t),
                    serving_player: serving_info.and_then(|(_, p)| p),
                    // Pause am Feld (für den Kombi-Pausen-Countdown).
                    pause: pause_info,
                    // Zähltafelbediener: bei aktiver Warteschlangen-Verwaltung
                    // (ADR 0007) der beim Aufruf ZUGEWIESENE Bediener; sonst der
                    // pro-Feld-Hinweis (Verlierer des zuletzt hier beendeten
                    // Spiels). Nur zeigen, wenn gerade ein Spiel läuft.
                    scorekeeper: if m.is_some() {
                        assigned_sk.clone().unwrap_or_else(|| {
                            self.scorekeeper_by_court
                                .read()
                                .unwrap()
                                .get(&court.id)
                                .cloned()
                                .unwrap_or_default()
                        })
                    } else {
                        Vec::new()
                    },
                    // true nur, wenn der scorekeeper aus einer echten Zuweisung
                    // stammt (Verwaltung an) — dann wird er auch angesagt; der
                    // reine pro-Feld-Hinweis wird nicht angesagt.
                    scorekeeper_assigned: assigned_sk.is_some(),
                    locked: self.locked_courts.read().unwrap().contains(&court.id),
                    on_court_since_ms: m.and_then(|mm| self.on_court_since_ms(court.id, mm.id)),
                    best_of: m.map(|mm| mm.scoring.best_of).unwrap_or(0),
                    target_score: m.map(|mm| mm.scoring.target_score).unwrap_or(0),
                    cap_score: m.map(|mm| mm.scoring.cap_score).unwrap_or(0),
                }
            })
            .collect()
    }

    /// Monitor-relevante Daten eines Feldes: das aktuelle Match mit
    /// effektivem Satzstand (Tablet-getrieben falls aktiv, sonst aus BTP)
    /// und der gespiegelte Tablet-Spielzustand (Aufschlag/Pause). Vom
    /// Court-Monitor-Endpunkt genutzt.
    pub fn monitor_court(&self, court_id: i64) -> MonitorCourt {
        let guard = self.snapshot.read().unwrap();
        let tournament_name = guard
            .as_ref()
            .map(|s| s.tournament_name.clone())
            .unwrap_or_default();
        let current_match = guard.as_ref().and_then(|snap| {
            snap.matches
                .iter()
                .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court_id))
                .cloned()
        });
        drop(guard);
        // Satzstand vom Tablet, sobald dessen Session zum selben Match
        // gehört – bewusst OHNE `connected`-Prüfung: ein kurzer Tablet-
        // Aussetzer (Browser zu, Display gesperrt) soll den Monitor nicht
        // auf 0:0 zurückwerfen; der zuletzt bekannte Stand bleibt stehen.
        let sets = match &current_match {
            Some(mm) => {
                let courts = self.courts.read().unwrap();
                match courts.get(&court_id) {
                    Some(s) if s.match_id == mm.id && !s.sets.is_empty() => s.sets.clone(),
                    _ => mm.sets.clone(),
                }
            }
            None => Vec::new(),
        };
        let on_court_since_ms = current_match
            .as_ref()
            .and_then(|mm| self.on_court_since_ms(court_id, mm.id));
        MonitorCourt {
            tournament_name,
            current_match,
            sets,
            court_state: self.court_state(court_id),
            on_court_since_ms,
        }
    }

    // ─────────────────────────── Court-Monitor-Geräte ─────────────────────

    /// Registriert einen Monitor-Poll (setzt „zuletzt gesehen") und liefert
    /// den offenen Fernbefehl des Geräts zurück. Bei erreichter Obergrenze
    /// wird das am längsten nicht gesehene Gerät verdrängt.
    pub fn record_monitor_poll(&self, device_id: &str) -> Option<MonitorCommand> {
        let mut live = self.monitor_live.write().unwrap();
        if !live.contains_key(device_id) && live.len() >= MAX_MONITOR_DEVICES {
            if let Some(oldest) = live
                .iter()
                .min_by_key(|(_, l)| l.last_seen_ms)
                .map(|(id, _)| id.clone())
            {
                live.remove(&oldest);
            }
        }
        let entry = live.entry(device_id.to_string()).or_default();
        entry.last_seen_ms = now_ms();
        entry.command
    }

    /// Hinterlegt einen Fernbefehl für ein Gerät. Die `id` zählt je Gerät
    /// hoch, damit der Monitor jeden Befehl genau einmal ausführt.
    pub fn set_monitor_command(&self, device_id: &str, kind: MonitorCommandKind) {
        let mut live = self.monitor_live.write().unwrap();
        let entry = live.entry(device_id.to_string()).or_default();
        // ID zeitstempel-basiert (ms seit Epoch) statt reiner +1-Zähler: der
        // Zähler lebt nur im RAM und startet nach jedem bts-light-Neustart
        // wieder bei 1, während die Monitore die zuletzt gesehene ID im
        // localStorage ÜBER den Neustart hinweg behalten. Eine kleinere ID
        // würde als „schon erledigt" verworfen → Identify/Neu-laden feuerten
        // erst nach mehrfachem Klicken. now_ms() ist über Neustarts hinweg
        // monoton; max(+1) sichert Eindeutigkeit bei zwei Befehlen je ms.
        // (Einziger Restfall: wird die Systemuhr zurückgestellt, kann genau ein
        // Befehl verworfen werden – für ein LAN-Tool akzeptabel.)
        let next_id = now_ms().max(entry.command.map(|c| c.id + 1).unwrap_or(0));
        entry.command = Some(MonitorCommand { id: next_id, kind });
    }

    /// Ist das Gerät aktuell online (letzter Poll innerhalb des
    /// Online-Fensters)? Unbekannte Geräte gelten als offline.
    pub fn is_monitor_online(&self, device_id: &str, now_ms: u64) -> bool {
        self.monitor_live
            .read()
            .unwrap()
            .get(device_id)
            .map(|l| now_ms.saturating_sub(l.last_seen_ms) <= relay_proto::MONITOR_ONLINE_WINDOW_MS)
            .unwrap_or(false)
    }

    /// Entfernt ein Gerät aus dem Live-State (vergisst es). Damit
    /// verschwindet es aus der Geräteliste, sofern es auch keine
    /// Zuweisung mehr hat (die räumt der Aufrufer separat ab).
    pub fn forget_monitor(&self, device_id: &str) {
        self.monitor_live.write().unwrap().remove(device_id);
    }

    /// Geräte-ID → letzter Poll (ms) aller bekannten Monitor-Geräte.
    pub fn monitor_live_seen(&self) -> HashMap<String, u64> {
        self.monitor_live
            .read()
            .unwrap()
            .iter()
            .map(|(id, l)| (id.clone(), l.last_seen_ms))
            .collect()
    }

    /// Geräte-ID → offener Fernbefehl (für den Cloud-Push zum Relay).
    pub fn monitor_commands(&self) -> HashMap<String, MonitorCommand> {
        self.monitor_live
            .read()
            .unwrap()
            .iter()
            .filter_map(|(id, l)| l.command.map(|c| (id.clone(), c)))
            .collect()
    }

    /// Übernimmt die vom Relay gemeldete Geräteliste (Cloud-Modus).
    pub fn set_relay_monitor_devices(&self, devices: Vec<MonitorDeviceInfo>) {
        *self.relay_monitor_devices.write().unwrap() = devices;
    }

    /// Vom Relay gemeldete Monitor-Geräteliste (Cloud-Modus).
    pub fn relay_monitor_devices(&self) -> Vec<MonitorDeviceInfo> {
        self.relay_monitor_devices.read().unwrap().clone()
    }
}

/// Monitor-relevante Daten eines Feldes (Rückgabe von
/// [`TabletState::monitor_court`]). Reiner Transport – nicht serialisiert.
pub struct MonitorCourt {
    /// Turniername (für die Werbe-/Leerlauf-Anzeige).
    pub tournament_name: String,
    /// Aktuelles Match auf dem Feld, falls eines zugewiesen ist.
    pub current_match: Option<BtpMatch>,
    /// Effektiver Satzstand (Tablet-getrieben falls aktiv, sonst BTP).
    pub sets: Vec<(i64, i64)>,
    /// Gespiegelter Tablet-Spielzustand (JSON-String), falls vorhanden.
    pub court_state: Option<String>,
    /// Zeitpunkt (Unix-ms) des 1. Aufrufs = seit wann das Spiel auf dem Feld
    /// steht; `None` = kein Spiel. Grundlage der Aufruf-Uhr am Monitor.
    pub on_court_since_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpPlayer, MatchResult};

    fn player(name: &str) -> BtpPlayer {
        BtpPlayer {
            id: 0,
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        }
    }

    /// Baut ein Match, das (per CourtID) einem Feld zugewiesen ist. `court`
    /// ist die CourtID des Felds (`None` = kein Feld).
    fn match_on(id: i64, court: Option<i64>, status: MatchStatus) -> BtpMatch {
        BtpMatch {
            id,
            draw_id: 1,
            planning_id: 1000 + id,
            draw_name: "HE".to_string(),
            discipline: Discipline::MensSingles,
            class_label: String::new(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: vec![player("Anna")],
            team2: vec![player("Ben")],
            entry1_id: 10,
            entry2_id: 20,
            // Court-Name spielt für die Identität keine Rolle – die
            // CourtID ist maßgeblich. Wir setzen einen Platzhalter-Namen.
            court: court.map(|cid| format!("C{cid}")),
            court_id: court,
            sets: vec![(5, 3)],
            winner: None,
            result: MatchResult::Normal,
            status,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    /// Baut einen Snapshot. `courts` ist eine Liste `(CourtID, Feldname)`.
    fn snapshot(matches: Vec<BtpMatch>, courts: Vec<(i64, &str)>) -> BtpSnapshot {
        let court_infos: Vec<BtpCourt> = courts
            .iter()
            .enumerate()
            .map(|(i, (id, name))| BtpCourt {
                id: *id,
                name: name.to_string(),
                location_id: Some(1),
                sort_order: i as i64,
            })
            .collect();
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            matches,
            courts: courts.into_iter().map(|(_, n)| n.to_string()).collect(),
            locations: Vec::new(),
            court_infos,
            events: Vec::new(),
            entries: Vec::new(),
        }
    }

    #[test]
    fn match_for_court_finds_the_on_court_match() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        assert_eq!(st.match_for_court(101).unwrap().id, 1);
        assert!(st.match_for_court(102).is_none());
    }

    /// Regression Mehr-Hallen-Turnier: zwei Felder heißen beide „1", haben
    /// aber verschiedene CourtIDs. Ein Tablet auf dem einen Feld darf den
    /// anderen Court nicht beeinflussen – ohne CourtID-Keying kollidierten
    /// beide auf demselben Namen.
    #[test]
    fn courts_with_same_name_but_different_id_do_not_collide() {
        let st = TabletState::default();
        // Halle 1 · Feld „1" (CourtID 101) und Halle 2 · Feld „1" (CourtID 401).
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, Some(401), MatchStatus::OnCourt),
            ],
            vec![(101, "1"), (401, "1")],
        ));
        // Jedes Feld findet sein eigenes Match.
        assert_eq!(st.match_for_court(101).unwrap().id, 1);
        assert_eq!(st.match_for_court(401).unwrap().id, 2);
        // Tablet bindet sich nur an Feld 101 und zählt dort.
        st.attach_tablet(101);
        st.record_score(101, 1, vec![(21, 5)]);
        // Feld 401 bleibt unberührt: keine Session, kein Satzstand.
        assert_eq!(st.active_courts(), vec![101]);
        let ov = st.overview();
        let c101 = ov.iter().find(|o| o.court_id == 101).unwrap();
        let c401 = ov.iter().find(|o| o.court_id == 401).unwrap();
        assert!(c101.tablet_connected);
        assert_eq!(c101.sets, vec![(21, 5)]);
        assert!(!c401.tablet_connected);
        assert_eq!(c401.sets, vec![(5, 3)]); // BTP-Stand, kein Tablet
                                             // Beide Felder tragen denselben Anzeigenamen.
        assert_eq!(c101.court, "1");
        assert_eq!(c401.court, "1");
    }

    #[test]
    fn apply_tablet_scores_overrides_only_active_matching_court() {
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.set_snapshot(snap.clone());
        st.record_score(101, 1, vec![(21, 19), (8, 6)]);
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 19), (8, 6)]);
    }

    #[test]
    fn apply_tablet_scores_ignores_session_for_other_match() {
        // Court hat inzwischen ein anderes Match – der Tablet-Stand darf
        // nicht aufs neue Match durchschlagen.
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(9, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.record_score(101, 1, vec![(21, 0)]);
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(5, 3)]);
    }

    #[test]
    fn detached_tablet_is_not_active() {
        let st = TabletState::default();
        st.attach_tablet(101);
        assert_eq!(st.active_courts(), vec![101]);
        st.detach_tablet(101);
        assert!(st.active_courts().is_empty());
    }

    #[test]
    fn overview_lists_each_court_with_its_match() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        st.record_score(101, 1, vec![(15, 12)]);
        st.attach_tablet(101);
        let ov = st.overview();
        assert_eq!(ov.len(), 2);
        let c1 = ov.iter().find(|o| o.court_id == 101).unwrap();
        assert_eq!(c1.court, "Court 1");
        assert_eq!(c1.team1, vec!["Anna".to_string()]);
        assert_eq!(c1.sets, vec![(15, 12)]);
        assert!(c1.tablet_connected);
        let c2 = ov.iter().find(|o| o.court_id == 102).unwrap();
        assert_eq!(c2.match_name, "");
        assert!(!c2.tablet_connected);
    }

    #[test]
    fn overview_carries_scoring_format_for_matchball_hint() {
        // Plan 16: overview() reicht das Zählformat (best_of/target/cap) des
        // Matches durch, damit die Felderübersicht Satz-/Matchball rechnen
        // kann. Belegtes Feld → Werte aus mm.scoring; leeres Feld → 0/0/0
        // (dann zeigt die Übersicht bewusst keinen „Ball").
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        let ov = st.overview();
        let c1 = ov.iter().find(|o| o.court_id == 101).unwrap();
        // ScoringFormat::default = 3×21, Cap 30.
        assert_eq!(c1.best_of, 3);
        assert_eq!(c1.target_score, 21);
        assert_eq!(c1.cap_score, 30);
        let c2 = ov.iter().find(|o| o.court_id == 102).unwrap();
        assert_eq!((c2.best_of, c2.target_score, c2.cap_score), (0, 0, 0));
    }

    #[test]
    fn overview_extracts_pause_and_serving_from_court_state() {
        // overview() übernimmt Pause + Aufschlag-Info 1:1 aus dem Tablet-
        // court_state (Grundlage für Kombi-Pausen-Countdown + Aufschlag-Punkt).
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        ));
        st.attach_tablet(101);
        st.set_court_state(
            101,
            r#"{"serving":{"team":"b","index":1},"pause":{"kind":"game","endsAt":1700000000000}}"#
                .to_string(),
        );
        let ov = st.overview();
        let c = ov.iter().find(|o| o.court_id == 101).unwrap();
        assert_eq!(c.serving_team, Some(2));
        assert_eq!(c.serving_player, Some(1));
        let pause = c.pause.as_ref().expect("pause present");
        assert_eq!(pause.get("kind").and_then(|v| v.as_str()), Some("game"));
        assert_eq!(
            pause.get("endsAt").and_then(|v| v.as_i64()),
            Some(1_700_000_000_000)
        );
    }

    #[test]
    fn overview_has_no_pause_without_court_state() {
        // Kein court_state (kein zählendes Tablet) → pause = None.
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        ));
        let ov = st.overview();
        let c = ov.iter().find(|o| o.court_id == 101).unwrap();
        assert!(c.pause.is_none());
    }

    #[test]
    fn overview_fills_location_only_for_multi_hall_tournaments() {
        use crate::btp::model::BtpLocation;
        // Ein-Hallen-Turnier (snapshot()-Helfer setzt locations leer):
        // location bleibt überall leer.
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        for c in st.overview() {
            assert_eq!(c.location, "");
        }
        // Mehr-Hallen-Turnier: Feld 101 in „Halle 1", Feld 401 in „Halle 2".
        let mut snap = snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, Some(401), MatchStatus::OnCourt),
            ],
            vec![(101, "1"), (401, "1")],
        );
        snap.locations = vec![
            BtpLocation {
                id: 1,
                name: "Halle 1".to_string(),
            },
            BtpLocation {
                id: 2,
                name: "Halle 2".to_string(),
            },
        ];
        // court_infos[1] ist Feld 401 → der Halle 2 zuordnen.
        snap.court_infos[1].location_id = Some(2);
        st.set_snapshot(snap);
        let ov = st.overview();
        assert_eq!(
            ov.iter().find(|o| o.court_id == 101).unwrap().location,
            "Halle 1"
        );
        assert_eq!(
            ov.iter().find(|o| o.court_id == 401).unwrap().location,
            "Halle 2"
        );
        // Das Komposit-Label kombiniert Halle und Feldname.
        assert_eq!(st.court_display_label(401), "Halle 2 · 1");
    }

    #[test]
    fn monitor_court_returns_match_with_effective_sets() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        // Ohne Tablet: Satzstand aus BTP (match_on setzt sets = [(5,3)]).
        let mc = st.monitor_court(101);
        assert_eq!(mc.tournament_name, "T");
        assert_eq!(mc.current_match.as_ref().unwrap().id, 1);
        assert_eq!(mc.sets, vec![(5, 3)]);
        assert!(mc.court_state.is_none());
        // Mit Tablet-Score: der Satzstand kommt vom Tablet.
        st.record_score(101, 1, vec![(21, 19), (8, 4)]);
        assert_eq!(st.monitor_court(101).sets, vec![(21, 19), (8, 4)]);
        // Tablet getrennt (Browser zu): der zuletzt bekannte Stand bleibt
        // stehen – der Monitor fällt NICHT auf den BTP-Stand zurück.
        st.detach_tablet(101);
        assert_eq!(st.monitor_court(101).sets, vec![(21, 19), (8, 4)]);
        // Leeres Feld: kein Match.
        assert!(st.monitor_court(102).current_match.is_none());
    }

    #[test]
    fn tablet_score_is_trusted_even_when_disconnected() {
        // Regression: Ein kurzer WS-Aussetzer (connected=false) darf weder
        // den Liveticker-Push (apply_tablet_scores) noch die Übersicht
        // (overview) auf BTPs 0:0/Poll-Stand zurückwerfen.
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.set_snapshot(snap.clone());
        st.record_score(101, 1, vec![(21, 19), (8, 4)]);
        st.detach_tablet(101); // Browser zu / Netz weg → connected=false
                               // apply_tablet_scores überschreibt den BTP-Stand trotzdem.
        st.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 19), (8, 4)]);
        // overview zeigt den Tablet-Stand, markiert das Tablet aber als offline.
        let ov = st.overview();
        let c = ov.iter().find(|o| o.court_id == 101).unwrap();
        assert_eq!(c.sets, vec![(21, 19), (8, 4)]);
        assert!(!c.tablet_connected);
    }

    #[test]
    fn live_scores_persist_and_reload_across_restart() {
        // Simuliert einen App-Neustart: Stand sichern, neue Instanz lädt ihn
        // und zeigt ihn (auch ohne verbundenes Tablet) statt BTPs 0:0.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live-scores.json");

        let st = TabletState::default();
        st.set_scores_path(path.clone());
        st.record_score(101, 7, vec![(21, 5), (2, 9)]);
        assert!(path.exists());

        // „Neustart": frische Instanz, gleiches Match noch OnCourt.
        let st2 = TabletState::default();
        st2.load_scores(&path);
        let mut snap = snapshot(
            vec![match_on(7, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st2.set_snapshot(snap.clone());
        // monitor_court + apply_tablet_scores liefern den wiederhergestellten Stand.
        assert_eq!(st2.monitor_court(101).sets, vec![(21, 5), (2, 9)]);
        st2.apply_tablet_scores(&mut snap);
        assert_eq!(snap.matches[0].sets, vec![(21, 5), (2, 9)]);

        // clear_court entfernt den Stand auch aus der Datei.
        st2.set_scores_path(path.clone());
        st2.clear_court(101);
        let st3 = TabletState::default();
        st3.load_scores(&path);
        st3.set_snapshot(snapshot(
            vec![match_on(7, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        ));
        assert_eq!(st3.monitor_court(101).sets, vec![(5, 3)]); // wieder BTP-Stand
    }

    #[test]
    fn walkover_candidates_lists_scheduled_matches_of_the_entry() {
        let st = TabletState::default();
        // match_on setzt entry1_id = 10, entry2_id = 20.
        st.set_snapshot(snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt), // läuft – kein Kandidat
                match_on(2, None, MatchStatus::Scheduled),
                match_on(3, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1")],
        ));
        let cands = st.walkover_candidates(10);
        let ids: Vec<i64> = cands.iter().map(|c| c.match_id).collect();
        assert_eq!(ids, vec![2, 3]);
        assert!(cands.iter().all(|c| c.retired_is_team1));
        assert_eq!(cands[0].opponent, "Ben");
        // Fremde Entry → keine Kandidaten; Entry 0 (unaufgelöst) ebenfalls.
        assert!(st.walkover_candidates(999).is_empty());
        assert!(st.walkover_candidates(0).is_empty());
    }

    /// Regression: Eine Aufgabe gilt NUR für die Disziplin, in der aufgegeben
    /// wurde (die EntryID ist pro Draw/Disziplin eindeutig). Annas Einzel und
    /// das andere Doppel ihrer Partnerin dürfen NICHT als Walkover-Kandidaten
    /// auftauchen, wenn das gemeinsame Herrendoppel aufgibt.
    #[test]
    fn walkover_candidates_stay_within_the_retiring_entrys_discipline() {
        // Übriges HD-Spiel des Paares "Anna / Cara" (EntryID 100) → Kandidat.
        let mut hd_next = match_on(1, None, MatchStatus::Scheduled);
        hd_next.draw_name = "HD".into();
        hd_next.discipline = Discipline::MensDoubles;
        hd_next.team1 = vec![player("Anna"), player("Cara")];
        hd_next.entry1_id = 100;
        hd_next.team2 = vec![player("Eva"), player("Fia")];
        hd_next.entry2_id = 110;

        // Annas Einzel (EntryID 200) → KEIN Kandidat (andere Disziplin/Entry).
        let mut he_anna = match_on(2, None, MatchStatus::Scheduled);
        he_anna.draw_name = "HE".into();
        he_anna.team1 = vec![player("Anna")];
        he_anna.entry1_id = 200;
        he_anna.team2 = vec![player("Gustav")];
        he_anna.entry2_id = 300;

        // Caras anderes Doppel (EntryID 400) → KEIN Kandidat.
        let mut cara_other = match_on(3, None, MatchStatus::Scheduled);
        cara_other.draw_name = "DD".into();
        cara_other.discipline = Discipline::WomensDoubles;
        cara_other.team1 = vec![player("Cara"), player("Hanna")];
        cara_other.entry1_id = 400;
        cara_other.team2 = vec![player("Ida"), player("Jana")];
        cara_other.entry2_id = 410;

        let st = TabletState::default();
        st.set_snapshot(snapshot(vec![hd_next, he_anna, cara_other], vec![]));

        // Aufgabe des HD-Paares (EntryID 100): nur dessen übriges HD-Spiel.
        let cands = st.walkover_candidates(100);
        let ids: Vec<i64> = cands.iter().map(|c| c.match_id).collect();
        assert_eq!(ids, vec![1]); // NICHT 2 (HE) und NICHT 3 (DD)
        assert!(cands[0].retired_is_team1);
        assert_eq!(cands[0].opponent, "Eva / Fia");

        // Gegenprobe: Annas Einzel-Entry ist davon unabhängig (nur ihr HE).
        let he_ids: Vec<i64> = st
            .walkover_candidates(200)
            .iter()
            .map(|c| c.match_id)
            .collect();
        assert_eq!(he_ids, vec![2]);
    }

    #[test]
    fn preparation_call_is_unique_per_match_and_removable() {
        let st = TabletState::default();
        let mk = |match_id: i64| PreparationCall {
            match_id,
            location_id: None,
            called_at_ms: 1000,
        };
        st.add_preparation_call(mk(5));
        st.add_preparation_call(mk(5)); // ersetzt – kein Duplikat
        assert_eq!(st.preparation_calls().len(), 1);
        st.add_preparation_call(mk(6));
        assert_eq!(st.preparation_calls().len(), 2);
        st.remove_preparation_call(5);
        let rest = st.preparation_calls();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].match_id, 6);
    }

    #[test]
    fn apply_preparation_calls_drops_calls_for_non_scheduled_matches() {
        use crate::btp::model::BtpLocation;
        let st = TabletState::default();
        // Match 2 ist eingeplant, Match 1 läuft (OnCourt).
        let mut snap = snapshot(
            vec![
                match_on(1, Some(101), MatchStatus::OnCourt),
                match_on(2, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1")],
        );
        snap.locations = vec![BtpLocation {
            id: 7,
            name: "Halle A".to_string(),
        }];
        // Aufruf für ein laufendes Match (1) und ein eingeplantes (2).
        st.add_preparation_call(PreparationCall {
            match_id: 1,
            location_id: None,
            called_at_ms: 1000,
        });
        st.add_preparation_call(PreparationCall {
            match_id: 2,
            location_id: Some(7),
            called_at_ms: 2000,
        });
        st.apply_preparation_calls(&mut snap);
        // Aufruf für Match 1 fällt heraus (kein Geister-Aufruf).
        let remaining: Vec<i64> = st.preparation_calls().iter().map(|c| c.match_id).collect();
        assert_eq!(remaining, vec![2]);
        // Match 1 trägt keinen Stempel, Match 2 schon.
        let m1 = snap.matches.iter().find(|m| m.id == 1).unwrap();
        let m2 = snap.matches.iter().find(|m| m.id == 2).unwrap();
        assert_eq!(m1.preparation_call_ts, None);
        assert_eq!(m2.preparation_call_ts, Some(2000));
        assert_eq!(m2.preparation_hall.as_deref(), Some("Halle A"));
    }

    #[test]
    fn apply_preparation_calls_leaves_hall_none_without_location() {
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(3, None, MatchStatus::Scheduled)],
            vec![(101, "Court 1")],
        );
        st.add_preparation_call(PreparationCall {
            match_id: 3,
            location_id: None,
            called_at_ms: 500,
        });
        st.apply_preparation_calls(&mut snap);
        let m = &snap.matches[0];
        assert_eq!(m.preparation_call_ts, Some(500));
        assert_eq!(m.preparation_hall, None);
    }

    #[test]
    fn walkover_proposal_is_unique_per_entry_and_removable() {
        let st = TabletState::default();
        let mk = |entry: i64| WalkoverProposal {
            id: entry.to_string(),
            entry_id: entry,
            retired_team: "X".to_string(),
            draw_name: "HE".to_string(),
            created_at_ms: 0,
        };
        st.add_walkover_proposal(mk(10));
        st.add_walkover_proposal(mk(10)); // ersetzt – kein Duplikat
        assert_eq!(st.walkover_proposals().len(), 1);
        st.add_walkover_proposal(mk(20));
        assert_eq!(st.walkover_proposals().len(), 2);
        st.remove_walkover_proposal("10");
        let rest = st.walkover_proposals();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].entry_id, 20);
    }

    #[test]
    fn on_court_since_stamps_holds_restamps_and_clears() {
        let st = TabletState::default();
        // 1. Aufruf: Match 100 kommt auf Feld 1 um t=1000.
        st.reconcile_on_court(&HashMap::from([(1, 100)]), 1000);
        assert_eq!(st.on_court_since_ms(1, 100), Some(1000));

        // Gleicher Stand später: Zeitstempel bleibt (idempotent).
        st.reconcile_on_court(&HashMap::from([(1, 100)]), 5000);
        assert_eq!(st.on_court_since_ms(1, 100), Some(1000));

        // Anderes Match auf demselben Feld: neu stempeln.
        st.reconcile_on_court(&HashMap::from([(1, 200)]), 8000);
        assert_eq!(st.on_court_since_ms(1, 100), None);
        assert_eq!(st.on_court_since_ms(1, 200), Some(8000));

        // Feld wird frei: Eintrag verschwindet.
        st.reconcile_on_court(&HashMap::new(), 9000);
        assert_eq!(st.on_court_since_ms(1, 200), None);
    }

    /// Reconnect-Erkennung (Turnier-Feedback 18.07.2026): Das Feld merkt
    /// sich die Geräte-Kennung des Halters — nur DASSELBE Gerät gilt beim
    /// Wiederverbinden als Halter, fremde und leere Kennungen nicht.
    #[test]
    fn claim_court_tracks_holder_device() {
        let st = TabletState::default();
        let token = st.claim_court(1, "dev-x");
        assert!(st.court_occupied(1));
        assert!(st.is_court_active(1, token));
        assert!(st.court_held_by_device(1, "dev-x"));
        assert!(!st.court_held_by_device(1, "dev-anders"));
        // Leere Kennung (alte Tablet-Seite) matcht nie — auch nicht leer↔leer.
        let st2 = TabletState::default();
        st2.claim_court(2, "");
        assert!(!st2.court_held_by_device(2, ""));
    }

    /// Ein Re-Claim desselben Geräts löst den alten Token ab; die alte
    /// Session kann das Feld danach nicht mehr freigeben.
    #[test]
    fn reclaim_supersedes_old_token() {
        let st = TabletState::default();
        let old = st.claim_court(1, "dev-x");
        let new = st.claim_court(1, "dev-x");
        assert!(!st.is_court_active(1, old), "alter Token ist abgelöst");
        assert!(st.is_court_active(1, new));
        // Aufräumen der toten alten Session darf das Feld NICHT freigeben.
        st.release_court(1, old);
        assert!(st.court_occupied(1), "Feld bleibt beim neuen Token");
        // Der aktive Halter kann regulär freigeben.
        st.release_court(1, new);
        assert!(!st.court_occupied(1));
        assert!(!st.court_held_by_device(1, "dev-x"));
    }

    // ───────────────────── Nachschub-Queue (A5) ─────────────────────

    fn upd(match_id: i64, duration: i64) -> crate::btp::proto::MatchUpdate {
        crate::btp::proto::MatchUpdate {
            btp_match_id: match_id,
            draw_id: 1,
            planning_id: 1000 + match_id,
            sets: vec![(21, 10)],
            team1_won: true,
            duration_mins: duration,
            score_status: 0,
            free_court_id: None,
            player_ids: Vec::new(),
            end_ts_ms: None,
        }
    }

    #[test]
    fn btp_retry_queue_dedups_per_match_and_keeps_first_timestamp() {
        let st = TabletState::default();
        st.queue_btp_retry(upd(7, 30), 1_000);
        // Zweiter Fehlschlag desselben Matches mit neuerem Stand: Update
        // ersetzt, Bezugszeitpunkt des ERSTEN Fehlschlags bleibt (steuert
        // das Spieler-Checkout-Fenster).
        st.queue_btp_retry(upd(7, 31), 9_000);
        st.queue_btp_retry(upd(8, 20), 2_000);
        let q = st.btp_retries();
        assert_eq!(q.len(), 2);
        let seven = q.iter().find(|e| e.update.btp_match_id == 7).unwrap();
        assert_eq!(seven.update.duration_mins, 31, "neuester Stand gewinnt");
        assert_eq!(seven.enqueued_ms, 1_000, "erster Zeitpunkt bleibt");
    }

    #[test]
    fn direct_btp_write_note_and_since() {
        // Race-Erkennung des Nachschubs: Nur Direkt-Writes AB dem
        // Vergleichszeitpunkt zählen (ts >= since).
        let st = TabletState::default();
        assert!(st.direct_btp_write_since(7, 0).is_none());
        st.note_direct_btp_write(upd(7, 30), 5_000);
        assert!(
            st.direct_btp_write_since(7, 6_000).is_none(),
            "Write liegt VOR dem Vergleichszeitpunkt"
        );
        let u = st.direct_btp_write_since(7, 5_000).expect("Write ab 5000");
        assert_eq!(u.duration_mins, 30);
        assert!(st.direct_btp_write_since(8, 0).is_none(), "fremdes Match");
    }

    #[test]
    fn btp_retry_clear_removes_only_that_match() {
        let st = TabletState::default();
        st.queue_btp_retry(upd(7, 30), 1_000);
        st.queue_btp_retry(upd(8, 20), 2_000);
        st.clear_btp_retry(7);
        let q = st.btp_retries();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].update.btp_match_id, 8);
    }

    // ── Zähltafelbediener-Warteschlange (ADR 0007, Phase 1) ────────────────

    #[test]
    fn overview_marks_assigned_scorekeeper_for_announcement() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        ));
        // Ohne Zuweisung: nicht angesagt (assigned = false).
        let c = st
            .overview()
            .into_iter()
            .find(|c| c.court_id == 101)
            .unwrap();
        assert!(!c.scorekeeper_assigned);
        // Bediener zuweisen → assigned = true, Namen im scorekeeper.
        st.enqueue_scorekeeper(9, vec!["A".into()], 101, 1_000);
        st.assign_scorekeeper_for_court(101, 1);
        let c = st
            .overview()
            .into_iter()
            .find(|c| c.court_id == 101)
            .unwrap();
        assert!(c.scorekeeper_assigned);
        assert_eq!(c.scorekeeper, vec!["A".to_string()]);
    }

    #[test]
    fn scorekeeper_enqueue_is_fifo_and_dedups_per_match() {
        let st = TabletState::default();
        st.enqueue_scorekeeper(1, vec!["A".into()], 5, 1_000);
        st.enqueue_scorekeeper(2, vec!["B".into(), "C".into()], 6, 2_000);
        // Zweiter Versuch für Match 1 → kein Duplikat.
        st.enqueue_scorekeeper(1, vec!["A".into()], 5, 3_000);
        let q = st.scorekeeper_queue();
        assert_eq!(q.len(), 2, "kein Doppel-Eintrag je Spielende");
        assert_eq!(q[0].names, vec!["A".to_string()]); // FIFO: ältester zuerst
        assert_eq!(q[1].names, vec!["B".to_string(), "C".to_string()]);
        // Leere Namen werden ignoriert.
        st.enqueue_scorekeeper(3, vec![], 7, 4_000);
        assert_eq!(st.scorekeeper_queue().len(), 2);
    }

    #[test]
    fn scorekeeper_display_flags_assigned_vs_hint() {
        let st = TabletState::default();
        // Nur pro-Feld-Hinweis (Verwaltung aus) → nicht als „zugewiesen".
        st.set_scorekeeper(5, vec!["Hint".into()]);
        assert_eq!(st.scorekeeper_display(5), (vec!["Hint".to_string()], false));
        // Zugewiesener Bediener gewinnt → als „zugewiesen" markiert (angesagt).
        st.enqueue_scorekeeper(1, vec!["Op".into()], 5, 1_000);
        st.assign_scorekeeper_for_court(5, 42);
        assert_eq!(st.scorekeeper_display(5), (vec!["Op".to_string()], true));
    }

    #[test]
    fn scorekeeper_assignment_prefers_own_court_then_oldest() {
        let st = TabletState::default();
        // A hat auf Feld 5 gespielt, B auf Feld 6, C manuell (Feld 0).
        st.enqueue_scorekeeper(1, vec!["A".into()], 5, 1_000);
        st.enqueue_scorekeeper(2, vec!["B".into()], 6, 2_000);
        st.add_scorekeeper_manual(vec!["C".into()], 3_000);
        // Feld 6 bekommt Match 42 → B bevorzugt (spielte auf 6).
        st.assign_scorekeeper_for_court(6, 42);
        assert_eq!(st.assigned_scorekeeper(6), Some(vec!["B".to_string()]));
        // B ist aus der Schlange raus.
        assert_eq!(st.scorekeeper_queue().len(), 2);
        // Feld 9 (niemand spielte dort) → der Älteste (A).
        st.assign_scorekeeper_for_court(9, 43);
        assert_eq!(st.assigned_scorekeeper(9), Some(vec!["A".to_string()]));
        // Idempotent: gleiche (Feld, Match) zieht nicht erneut.
        st.assign_scorekeeper_for_court(9, 43);
        assert_eq!(st.scorekeeper_queue().len(), 1); // nur noch C
                                                     // Leere Schlange: nächstes Feld bekommt niemanden — erst C ziehen.
        st.assign_scorekeeper_for_court(1, 44);
        assert_eq!(st.assigned_scorekeeper(1), Some(vec!["C".to_string()]));
        st.assign_scorekeeper_for_court(2, 45);
        assert_eq!(st.assigned_scorekeeper(2), None, "Schlange leer");
    }

    #[test]
    fn scorekeeper_assignment_is_cleared_when_court_frees() {
        let st = TabletState::default();
        st.enqueue_scorekeeper(1, vec!["A".into()], 5, 1_000);
        st.assign_scorekeeper_for_court(5, 42);
        assert!(st.assigned_scorekeeper(5).is_some());
        // Feld 5 trägt jetzt ein ANDERES Match → alte Zuweisung räumen.
        let mut active = std::collections::HashMap::new();
        active.insert(5, 99);
        st.retain_scorekeeper_assignments(&active);
        assert_eq!(st.assigned_scorekeeper(5), None);
        // Leeres active → alles geräumt.
        st.assign_scorekeeper_for_court(5, 99);
        st.retain_scorekeeper_assignments(&std::collections::HashMap::new());
        assert_eq!(st.assigned_scorekeeper(5), None);
    }

    #[test]
    fn scorekeeper_remove_and_advance_and_manual_add() {
        let st = TabletState::default();
        st.enqueue_scorekeeper(1, vec!["A".into()], 5, 1_000);
        st.enqueue_scorekeeper(2, vec!["B".into()], 6, 2_000);
        st.add_scorekeeper_manual(vec![" C ".into()], 3_000); // getrimmt
        let keys: Vec<String> = st
            .scorekeeper_queue()
            .iter()
            .map(|e| e.key.clone())
            .collect();
        assert_eq!(keys.len(), 3);
        // Manuellen Eintrag „C" nach vorne ziehen.
        let c_key = st
            .scorekeeper_queue()
            .into_iter()
            .find(|e| e.names == vec!["C".to_string()])
            .unwrap()
            .key;
        st.advance_scorekeeper(&c_key);
        assert_eq!(st.scorekeeper_queue()[0].names, vec!["C".to_string()]);
        // „A" entfernen.
        st.remove_scorekeeper(&keys[0]);
        let names: Vec<Vec<String>> = st
            .scorekeeper_queue()
            .into_iter()
            .map(|e| e.names)
            .collect();
        assert_eq!(names, vec![vec!["C".to_string()], vec!["B".to_string()]]);
    }
}
