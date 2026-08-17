//! Geteilter Zustand zwischen Sync-Loop und Tablet-Server.
//!
//! Der Sync-Loop legt hier den jeweils neuesten BTP-Snapshot ab, der
//! Tablet-Server pflegt die laufenden Court-Sessions. Beide Seiten teilen
//! sich ein `Arc<TabletState>`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};

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

/// Fan-out-Deckel der Court-Monitor-Nudge-Abos je Server (A1, ADR 0016) —
/// derselbe Wert wie im Relay (`MAX_MONITOR_SUBS`). Über der Grenze lehnt
/// `subscribe_monitor` ein weiteres Abo ab; die Anzeige fällt still auf ihren
/// Poll-Fallback zurück. Schützt Speicher und Broadcast-Kosten gegen einen
/// Zuschauer-DoS (viele TVs/Tabs, die den Monitor-WS öffnen).
const MAX_MONITOR_SUBS: usize = 256;

/// Lebensdauer des Finalisiert-Merkers (A2 / ADR 0017, Regel b). Lang genug,
/// dass ein kurz abgerissenes Tablet nach seiner Rückkehr noch „finalized"
/// erfährt und das Hand-Ergebnis nicht überbügelt; kurz genug, dass der Merker
/// nicht ewig hängt, falls das Feld nie ein neues Match bekommt (Turnierende).
/// Ein neues Match auf dem Feld räumt den Merker unabhängig davon sofort.
const FINALIZED_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Wie lange eine frisch nach BTP geschriebene Feldzuweisung als belegt gilt,
/// bevor BTP sie bestätigt hat.
///
/// Lang genug, um den Abfragetakt samt einer trägen BTP-Antwort zu
/// überbrücken; kurz genug, dass ein fehlgeschlagener Schreibvorgang das Feld
/// nicht spürbar blockiert.
const RESERVATION_TTL_MS: u64 = 15_000;

/// Wie lange die Antwort auf einen Vorgang der Turnierleitungs-Oberfläche
/// aufgehoben wird, um Wiederholungen dieselbe Antwort zu geben.
///
/// Deckt Netzwackler und Doppeltipps ab. Danach ist der Vorgang vergessen —
/// die Liste soll über ein Turnier hinweg nicht unbegrenzt wachsen.
const OP_MEMORY_MS: u64 = 60_000;

/// Obergrenze für die Länge einer Vorgangskennung. Sie kommt von außen; eine
/// Kennung, die länger ist als jede sinnvolle Kennung, wird verworfen.
const MAX_OP_ID_LEN: usize = 128;

/// Wie viele Vorgänge höchstens erinnert werden. Bei acht Geräten und einer
/// Minute Gedächtnis reichlich bemessen — die Grenze existiert, damit ein
/// Gerät mit gültigem Zugang den Arbeitsspeicher nicht füllen kann.
const MAX_REMEMBERED_OPS: usize = 512;

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
/// `Default` existiert für Tests und den `paint`-Helfer — im Betrieb baut
/// ausschließlich `overview_from` die Zeilen.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
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
    /// Effektive Hallen-Farbe (Hex, Spec hallen-farben) — von
    /// `hall_colors::paint` an den Serving-Stellen gefüllt, weil dort die
    /// Config greifbar ist. `None` bei Ein-Hallen-Turnieren oder Feldern
    /// ohne Halle.
    pub hall_color: Option<String>,
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
    /// Vereinsnamen, parallel zu `team1`/`team2` (leerer String, wenn BTP
    /// keinen führt) – zuschaltbares Anzeige-Feld in TL-Web/Tablet.
    pub team1_clubs: Vec<String>,
    pub team2_clubs: Vec<String>,
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
    /// Wie oft dieses Spiel schon aufgerufen wurde, gezählt am Turnier-PC
    /// (0 = noch nie; mit „Aufrufe unbegrenzt" nach oben offen, sonst
    /// maximal 3 — Konsumenten dürfen KEIN `min(…, 3)` daraufsetzen).
    /// Damit zeigen Desktop-Übersicht und Turnierleitungs-Seite dieselbe
    /// Stufe — auch wenn die eine gerufen hat und die andere nicht.
    pub call_stage: u8,
    /// Zählformat des aktuellen Matches (Sätze/Zielpunkt/Cap), damit die
    /// Felderübersicht Satz-/Matchball berechnen kann (Plan 16). 0 = kein
    /// Match / unbekannt (dann keine Satzball-Anzeige).
    pub best_of: i64,
    pub target_score: i64,
    pub cap_score: i64,
    /// Gibt es zum laufenden Match einen Punktverlauf (Spec
    /// punktverlauf-graph)? Felderübersicht und TL-Web bieten den
    /// Graph-Klick nur dann an.
    pub has_timeline: bool,
    /// Schiedsrichter des laufenden Spiels (Spec `schiedsrichter-management`).
    /// Leer, wenn keiner zugewiesen ist oder ohne Schiedsrichter gespielt
    /// wird. Als Liste, damit die Anzeige dieselbe Form hat wie `scorekeeper`.
    pub sr: Vec<String>,
    /// Aufschlagrichter des laufenden Spiels.
    pub ar: Vec<String>,
    /// Konflikt-Kategorie („Verein"/„Person"), wenn ein zugewiesener
    /// Official nicht zu diesem Spiel passt. Bewusst nur die Kategorie —
    /// der Grund (welcher Verein, welcher Spieler) bleibt am Turnier-PC.
    pub official_warn: Option<String>,
    /// IDs der wirksamen Besetzung (0 = keiner). Die **Bedienung** braucht
    /// sie: Zwei Schiedsrichter können denselben Anzeigenamen tragen, und
    /// eine Auswahl über den Namen träfe dann den Falschen.
    pub sr_id: i64,
    pub ar_id: i64,
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

/// Ownership-Token eines Felds (A2 / ADR 0017): WER hält den Slot gerade und
/// hat er seit seiner Übernahme gezählt? `epoch` = der monotone `claim_court`-
/// Token, `device` = aktives Tablet. Autorität ist der Slot-Halter, nicht ein
/// rev-Zähler — das ist die Grundlage der Reconnect-Konfliktregel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourtOwner {
    /// Monotone Claim-Epoche (`token` aus `claim_court`).
    pub epoch: u64,
    /// Geräte-ID des aktuellen Halters (leer bei alten Tablet-Seiten).
    pub device: String,
    /// Hat dieser Halter seit seinem Claim mindestens einen Score erzeugt?
    pub scored_since_claim: bool,
}

/// Ausgang der reinen Reconnect-Entscheidung (A2 / ADR 0017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectDecision {
    /// Das zurückkehrende Tablet setzt seinen LOKALEN Stand durch.
    KeepLocal,
    /// Das zurückkehrende Tablet tritt zurück (adoptiert / zählt nicht mehr).
    StandDown,
}

/// Reine, unit-testbare Reconnect-Entscheidung (A2 / ADR 0017): Nach einem
/// Tablet-Reconnect im selben Spiel entscheidet der SLOT-HALTER, nicht ein
/// rev-Zähler, wessen Stand gilt. Deterministisch, ohne `self`/Lock.
///
/// `returning_device` = Geräte-ID des zurückkehrenden Tablets.
/// `current_owner` = aktueller Slot-Halter (`None` = Feld frei). Für die
/// Reconnect-Stelle ist das der Halter VOR dem erneuten Claim.
/// `finalized` = das Match wurde per Hand fertig eingegeben (aus dem BTP-Status
/// abgeleitet, siehe [`TabletState::recently_finalized`]) — dann darf kein
/// Tablet das Ergebnis überbügeln.
///
/// Regeln (in dieser Reihenfolge, deterministisch):
/// 1. `finalized` → `StandDown` (Hand-Ergebnis nicht überbügeln).
/// 2. Feld frei (`None`) → `KeepLocal` (niemand hat übernommen).
/// 3. Halter == Rückkehrer → `KeepLocal` (der Reclaimer/Halter ist die Wahrheit).
/// 4. Fremder Halter UND hat seit der Übernahme gezählt → `StandDown`
///    (der legitime Übernehmer gewinnt).
/// 5. Fremder Halter OHNE Score seit der Übernahme → `KeepLocal` (der
///    Rückkehrer gewinnt deterministisch; bei echter Divergenz ist der
///    stille Verlierer bewusst in Kauf genommen — siehe ADR 0017).
///
/// `epoch` des Rückkehrers ist für die Entscheidung NICHT nötig (Autorität ist
/// gerätebasiert); sie wird nur zur Diagnose mitgeführt.
pub fn reconnect_decision(
    returning_device: &str,
    current_owner: Option<CourtOwner>,
    finalized: bool,
) -> ReconnectDecision {
    // Regel b (A2 / ADR 0017): Das Finalisiert-Signal wird vom Sync-Loop aus
    // dem BTP-Status abgeleitet (`TabletState::recently_finalized`) und an den
    // Reconnect-Eintritten hereingereicht — ein finalisiertes Match darf kein
    // Tablet mehr überbügeln.
    if finalized {
        return ReconnectDecision::StandDown;
    }
    match current_owner {
        None => ReconnectDecision::KeepLocal,
        Some(owner) if owner.device == returning_device => ReconnectDecision::KeepLocal,
        Some(owner) if owner.scored_since_claim => ReconnectDecision::StandDown,
        Some(_) => ReconnectDecision::KeepLocal,
    }
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
    /// CourtID → hat der AKTUELLE Slot-Halter seit seinem `claim_court`
    /// mindestens einen Score erzeugt? (A2 / ADR 0017, Reconnect-Wahrheit.)
    /// Grundlage der Konfliktregel „legitim weitergezählt": nur wenn ein
    /// ANDERES Gerät den Slot hält UND seit der Übernahme gezählt hat, tritt
    /// ein zurückkehrendes Tablet zurück. Gesetzt in `record_score`,
    /// zurückgesetzt in `claim_court` (neuer Claim = neuer Zähl-Abschnitt).
    /// Ein Court ohne Eintrag hat seit dem letzten Claim nicht gezählt.
    scored_since_claim: RwLock<HashSet<i64>>,
    /// CourtID → (Match-ID, Zeitpunkt) des zuletzt in BTP FINALISIERTEN Matches
    /// dieses Felds (A2 / ADR 0017, Regel b). `match_for_court` liefert beendete
    /// Matches nicht mehr (Status Finished ≠ OnCourt); dieser Merker erlaubt es
    /// dem Server, dem Tablet — das noch dieselbe matchId trägt — `finalized:true`
    /// zu schicken (Rücktritt statt bloßer `MatchCleared`) und einen nachlaufenden
    /// Score fürs finalisierte Match zu verwerfen. R2 gewahrt: die Wahrheit
    /// bleibt BTP, wir spiegeln nur den Finished-Status. Kurze TTL
    /// ([`FINALIZED_TTL`]), damit der Merker nicht ewig hängt; ein Feld mit
    /// OnCourt-Match räumt ihn ohnehin bedingungslos (`clear_finalized`). Vom
    /// Sync-Loop beim Übergang OnCourt→Finished gesetzt.
    recently_finalized: RwLock<HashMap<i64, (i64, std::time::Instant)>>,
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
    /// CourtID → (Match-ID, Zeitpunkt): Zuweisungen, die schon nach BTP
    /// geschrieben, aber noch nicht zurückgelesen wurden.
    ///
    /// Der Schnappschuss hinkt BTP um bis zu einen Abfragetakt hinterher —
    /// in diesem Fenster sähe eine zweite Prüfung das Feld noch frei und
    /// ließe eine konkurrierende Zuweisung durch. Die Reservierung schließt
    /// genau dieses Fenster. Sie verfällt von selbst, damit ein
    /// fehlgeschlagener Schreibvorgang das Feld nicht dauerhaft blockiert.
    pending_assign: RwLock<HashMap<i64, (i64, u64)>>,
    /// Vorgangskennung → (Zeitpunkt, Antwort): schon ausgeführte Aktionen
    /// der Turnierleitungs-Oberfläche.
    ///
    /// Ein Doppeltipp bei träger Verbindung schickt dieselbe Aktion zweimal.
    /// Ohne dieses Gedächtnis schriebe der zweite Versuch erneut nach BTP.
    recent_ops: RwLock<HashMap<String, (u64, String, relay_proto::TlResponse)>>,
    /// Automatische Feldvergabe zur Laufzeit angehalten?
    ///
    /// Der Sync-Lauf bekommt seine Konfiguration **einmal beim Start** und
    /// liest sie nie neu — eine Änderung an der Datei bliebe also wirkungslos.
    /// Dieser Schalter wirkt sofort und ist genau dafür da: Während die
    /// Turnierleitung von Hand umsortiert, soll die Automatik nicht
    /// dazwischenfunken. Er gilt bis zum nächsten Start; danach zählt wieder
    /// die Grundeinstellung aus der Konfiguration.
    auto_assign_paused: RwLock<bool>,
    /// Erfolgte Aufrufe je Feld:
    /// `court_id → (match_id, Stufe, bereits gerufene Parteien)`.
    ///
    /// Gehört an den Turnier-PC und nicht in die Geräte: Zählte jede Seite
    /// für sich, riefe ein Helfer zum zweiten Mal, während der nächste schon
    /// beim dritten ist — und niemand wüsste, ob das Spiel gleich gestrichen
    /// wird. Die Zahl ist die Zahl der **Aufrufe**, nicht der Zeitablauf; die
    /// Fälligkeitsanzeige bleibt davon unberührt.
    ///
    /// Die Parteien-Maske ([`SIDE_TEAM1`]/[`SIDE_TEAM2`]) merkt sich, wer auf
    /// der **aktuellen** Stufe schon gerufen wurde — damit „Partei A rufen"
    /// und direkt danach „Partei B rufen" **eine** Aufruf-Runde bleiben und
    /// die Stufe nicht zweimal hochzählen (Spec tl-liste-vereinfachen E1).
    call_stages: RwLock<HashMap<i64, (i64, u8, u8)>>,
    /// Nachrufe am Meeting Point: `(match_id, Partei) → Stufe`. Getrennt nach
    /// Partei, weil in der Regel nur eine fehlt.
    prep_call_stages: RwLock<HashMap<(i64, String), u8>>,
    /// Punktverlauf-Speicher (Spec `punktverlauf-graph`, ADR 0014/0015):
    /// Ballwechsel-Verläufe je Match, dauerhaft je Turnier persistiert.
    /// Er hängt hier, weil LAN-Server, Relay-Client und Tauri-Commands
    /// denselben Stand sehen müssen — wie beim übrigen Tablet-Zustand.
    timeline: crate::tablet::timeline::TimelineStore,
    /// Schiedsrichter-Roster (Spec `schiedsrichter-management`, ADR 0022):
    /// Rotationsreihenfolge, Pausen, Sperrlisten, feldweise Schalter und
    /// lokale SR/AR-Zuweisungen — turniergebunden persistiert. Er hängt hier,
    /// weil LAN-Server, Relay-Client und Tauri-Commands denselben Stand
    /// sehen müssen; die Stammliste selbst bleibt BTPs (R2).
    officials: crate::tablet::officials::OfficialsStore,
    /// Ausnahmeliste der automatischen Feldvergabe (Spec
    /// `feldvergabe-ausnahme`, Muster ADR 0022): Match-IDs, die die
    /// Turnierleitung von `sync.rs::auto_assign` ausgenommen hat —
    /// turniergebunden persistiert, kein Personendatum. Er hängt hier aus
    /// demselben Grund wie `officials`: TL-Web-Actions und Tauri-Commands
    /// müssen denselben Stand sehen.
    auto_assign_exclusions: crate::tablet::exclusion::AutoAssignExclusionStore,
    /// Manuelle Spielreihenfolge je Halle (Spec
    /// `spielliste-manuelle-reihenfolge`, ADR 0023): Match-IDs im
    /// Präfix-Block ihrer Halle, turniergebunden persistiert. Er hängt hier
    /// aus demselben Grund wie `officials`/`auto_assign_exclusions`: TL-Web
    /// und Desktop müssen denselben Stand sehen; die BTP-Reihenfolge selbst
    /// bleibt unangetastet (R2).
    queue_order: crate::tablet::queue_order::QueueOrderStore,
    /// Spielzeiten-Messung je Match (Spec `spielzeiten-prognose`, ADR 0027):
    /// erste Feldzuweisung, erster Punkt, Spielende — turniergebunden
    /// persistiert (`match-times.json`). Er hängt hier, weil Sync-Loop,
    /// Ergebnis-Pfade (LAN/Cloud/TL-Web/Desktop) und die TL-Anzeige
    /// denselben Stand sehen müssen; `on_court_since` bleibt reiner
    /// RAM-Zubringer für den Aufruf-Timer.
    match_times: crate::tablet::match_times::MatchTimesStore,
    /// Automatisch vorverteilte Hallen (Spec `hallen-vorverteilung`,
    /// ADR 0029): turniergebunden persistiert (`auto-halls.json`). Hier,
    /// weil Sync-Loop (verteilt), TL-Web (Badge, Räumen) und die Kaskade
    /// denselben Stand sehen müssen.
    auto_halls: crate::tablet::hall_assign::AutoHallStore,
    /// Match-ID → zuletzt publizierte Startzeit-Prognose (Unix-ms) — reines
    /// Diagnose-Gedächtnis für den Prognose/Wirklichkeit-Vergleich (E12),
    /// gepflegt von `tl::build_state_limited`.
    predicted_starts: RwLock<HashMap<i64, u64>>,
    /// (Messwert-Generation, Statistik): Cache für `cached_time_stats` —
    /// neu gerechnet nur, wenn sich am Zeiten-Store etwas geändert hat.
    time_stats_cache: Mutex<Option<(u64, std::sync::Arc<crate::tablet::predict::TimeStats>)>>,
    /// Match-ID → Halle, die die Turnierleitung diesem Spiel **von Hand**
    /// gegeben hat.
    ///
    /// BTP führt an angesetzten Spielen keinen Spielort (nachgewiesen an zwei
    /// echten Mitschnitten, siehe `assign::hall_for_match`) — ohne diese
    /// Ablage gäbe es für ein Spiel ohne Disziplin-Regel keine Möglichkeit,
    /// überhaupt zu sagen, in welche Halle es gehört, bevor es aufgerufen
    /// wird.
    ///
    /// Nur zur Laufzeit, wie die Vorbereitungs-Aufrufe: Match-IDs gelten je
    /// Turnier, und ein Neustart der Übertragung ist der Moment, in dem man
    /// ohnehin neu ordnet.
    manual_halls: RwLock<HashMap<i64, String>>,
    /// Datei, in der die Spielorte liegen. `OnceLock`, weil `TabletState`
    /// `derive(Default)` benutzt und den Pfad erst beim Start erfährt.
    manual_halls_path: std::sync::OnceLock<std::path::PathBuf>,
    /// Revision des Anzeige-Zustands: `(Nummer, Fingerabdruck)`. Steigt nur
    /// bei echter Änderung — **die eine** Quelle für LAN und Cloud. Zwei
    /// getrennte Zähler wären schlimmer als keiner: Dieselbe Zahl meinte
    /// dann verschiedene Stände.
    tl_state_rev: RwLock<(u64, String)>,
    /// Freitext-Ansagen (Master legt ab; Master + Slaves pollen + sprechen die
    /// für ihre Halle bestimmten). Dedup über die fortlaufende `id`.
    freetext: RwLock<Vec<FreetextItem>>,
    freetext_seq: AtomicU64,
    /// Ansage-Aufträge der Turnierleitung, von den Ansage-Geräten abgeholt.
    announce_jobs: RwLock<Vec<AnnounceJob>>,
    announce_seq: AtomicU64,
    /// Zuletzt gesehene Ansage-Geräte: Halle (klein) → Zeitpunkt des Abrufs.
    announce_listeners: RwLock<HashMap<String, u64>>,
    /// Fehlgeschlagene BTP-Ergebnis-Writes, die der Sync-Loop nachschiebt
    /// (Nachschub-Queue, Cluster A5 — needsync-Prinzip aus Tilos BTS,
    /// robuster: periodischer Retry statt nur beim Reconnect). Je Match
    /// höchstens ein Eintrag, der neueste Stand gewinnt.
    btp_retry: RwLock<Vec<PendingBtpWrite>>,
    /// Pfad der `btp-retry.json` (persistente Nachschub-Queue, ADR 0018).
    /// `OnceLock`, weil `TabletState` `derive(Default)` nutzt und den Pfad
    /// erst beim Start erfährt. Ungesetzt = Persistenz aus (z. B. in Tests).
    btp_retry_path: OnceLock<PathBuf>,
    /// Serialisiert die Schreibvorgänge auf `btp-retry.json` (Vorbild
    /// `scores_persist_lock`): mehrere Ergebnis-Writes können gleichzeitig
    /// einreihen; ohne dieses Lock schnitten sich die Schreiber die Datei ab.
    btp_retry_persist_lock: Mutex<()>,
    /// Turnier-Guard der persistierten Queue = `snapshot.tournament_name`
    /// (immer verfügbar, ADR 0015). Beim Laden wird bei Mismatch verworfen
    /// (BTP-`match_id` sind nur pro Turnier eindeutig); leerer Name → die
    /// Queue wird nicht geschrieben (ließe sich keinem Turnier zuordnen).
    btp_retry_tournament: RwLock<String>,
    /// Ob die Queue schon einmal von Platte geladen wurde. Der erste
    /// `set_snapshot` (Turnier-Guard nun verfügbar) triggert das Laden genau
    /// einmal (CAS); spätere Snapshots aktualisieren nur den Guard.
    btp_retry_loaded: AtomicBool,
    /// Match-ID → (letzter ERFOLGREICHER Direkt-Write, Zeitpunkt Unix-ms).
    /// Schließt das Nachschub-Race: Landet ein (langsamer) Queue-Write NACH
    /// einer zwischenzeitlich erfolgreichen Korrektur, erkennt der Flush
    /// das hieran und schreibt die neuere Korrektur sofort erneut
    /// (Selbstheilung statt stillem Überschreiben).
    last_direct_btp_write: RwLock<HashMap<i64, (crate::btp::proto::MatchUpdate, u64)>>,
    /// Court-Monitor-Nudge-Abonnenten (A1, ADR 0016): CourtID → Sende-Enden der
    /// Monitor-WS-Verbindungen, die **genau dieses Feld** beobachten (ein
    /// Court-Monitor, `monitor.html`). Bei einer Änderung des Felds bekommt
    /// jeder Eintrag ein winziges „Feld geändert, seq N"-Signal; die Anzeige
    /// holt daraufhin den Vollstand über ihre bestehende Poll-Route (eine
    /// Datenquelle, kein Flackern).
    monitor_subs: RwLock<HashMap<i64, Vec<MonitorNudgeTx>>>,
    /// Nudge-Abonnenten **ohne** Feld-Filter: die Feld-Übersicht
    /// (`overview.html`) will Signale ALLER Felder. Jeder `notify_monitor`
    /// weckt zusätzlich diese Liste.
    monitor_subs_all: RwLock<Vec<MonitorNudgeTx>>,
    /// Pro-Court monoton steigende Nudge-Sequenz. Der Client verwirft anhand
    /// dieses Werts veraltete Nudges (kein Rückwärtsspringen der Anzeige).
    monitor_seq: RwLock<HashMap<i64, u64>>,
}

/// Der Monitor-Nudge auf der Wire: `{"court":<id>,"seq":<n>}` (A1, ADR 0016).
/// Geteilter Typ für Erzeuger ([`TabletState::notify_monitor`]) und
/// Verbraucher (LAN-Monitor-WS reicht den String 1:1 durch; der
/// Relay-Client parst ihn für den Score-Spiegel) — Producer und Consumer
/// können so nicht stillschweigend auseinanderlaufen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MonitorNudge {
    pub court: i64,
    pub seq: u64,
}

/// Sende-Ende eines Monitor-Nudge-Kanals (A1, ADR 0016). Trägt den fertig
/// serialisierten JSON-Nudge `{"court":<i64>,"seq":<u64>}`; der WS-Handler
/// reicht ihn 1:1 auf seinen Socket. Unbounded, weil `notify_monitor` NIE
/// blockieren darf (es läuft unter dem `record_score`-Lock).
pub type MonitorNudgeTx = tokio::sync::mpsc::UnboundedSender<String>;
/// Empfangs-Ende eines Monitor-Nudge-Kanals; der WS-Handler leert es auf
/// seinen Socket. Fällt die Verbindung weg, wird das `Rx` fallengelassen und
/// der zugehörige `Tx` beim nächsten `notify_monitor` ausgesiebt.
pub type MonitorNudgeRx = tokio::sync::mpsc::UnboundedReceiver<String>;

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

/// Parteien-Maske der Aufruf-Zählung (siehe `TabletState::call_stages`):
/// Bit 0 = erste Partei, Bit 1 = zweite Partei.
///
/// Bewusst eine Bitmaske statt eines zweiten `HashMap`-Schlüssels wie bei
/// den Vorbereitungs-Nachrufen (`prep_call_stages`): Dort ist die Stufe je
/// Partei eigenständig, hier bleibt sie **eine** Zahl je Feld (die Zusage
/// „alle Geräte zeigen dieselbe Zahl"). Die Maske sagt nur, wer auf dieser
/// Stufe schon dran war.
pub const SIDE_TEAM1: u8 = 0b01;
/// Siehe [`SIDE_TEAM1`].
pub const SIDE_TEAM2: u8 = 0b10;
/// Beide Parteien — das bisherige Verhalten eines Aufrufs.
pub const SIDE_BOTH: u8 = SIDE_TEAM1 | SIDE_TEAM2;

/// Übersetzt die Wire-Partei in die Maske der Aufruf-Zählung. `None` =
/// beide (neutralste Variante, siehe `TlAction::AnnounceCourtCall`).
pub fn side_mask(side: Option<relay_proto::PrepCallSide>) -> u8 {
    match side {
        Some(relay_proto::PrepCallSide::Team1) => SIDE_TEAM1,
        Some(relay_proto::PrepCallSide::Team2) => SIDE_TEAM2,
        Some(relay_proto::PrepCallSide::Both) | None => SIDE_BOTH,
    }
}

/// Serde-Default für [`AnnounceJobKind::CourtCall::side`].
fn both_side() -> relay_proto::PrepCallSide {
    relay_proto::PrepCallSide::Both
}

/// Worum es bei einem Ansage-Auftrag geht.
///
/// Bewusst **keine** fertigen Worte: Text, Gong, Stimme und die Aussprache
/// der Namen entstehen am Ansage-Gerät — mit demselben Code wie bei einer
/// Ansage aus der Desktop-App. Ein Freitext von der Turnierleitungs-Seite
/// klänge anders als derselbe Aufruf vom Turnier-PC, und die Namenskorrektur
/// bliebe außen vor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnnounceJobKind {
    /// Erneuter Aufruf eines Spiels, das schon auf dem Feld steht.
    CourtCall {
        #[serde(rename = "courtId")]
        court_id: i64,
        #[serde(rename = "matchId")]
        match_id: i64,
        /// Die Stufe, die der Turnier-PC gezählt hat (2 oder 3).
        stage: u8,
        /// Welche Partei gemeint ist — Vorbild [`AnnounceJobKind::PrepCall`].
        /// Das Ansage-Gerät nennt dann nur diese Partei, genau wie beim
        /// Vorbereitungs-Nachruf. Fehlt das Feld (Auftrag aus einer älteren
        /// Fassung), gilt `Both`.
        #[serde(default = "both_side")]
        side: relay_proto::PrepCallSide,
    },
    /// Nur die Besetzung eines Felds ansagen (Schiedsrichter,
    /// Aufschlagrichter) — der manuelle Knopf aus Client und TL-Web. Eine
    /// nachträgliche Zuweisung sagt nie von selbst an (Spec Nr. 8).
    Officials {
        #[serde(rename = "courtId")]
        court_id: i64,
    },
    /// Erneuter Aufruf eines in Vorbereitung gerufenen Spiels.
    PrepCall {
        #[serde(rename = "matchId")]
        match_id: i64,
        side: relay_proto::PrepCallSide,
        /// Die Stufe, die der Turnier-PC gezählt hat (2 oder 3). Ohne sie
        /// bliebe jeder Nachruf ein „Zweiter Aufruf", und die Wartenden
        /// erführen nie, dass es der letzte vor der kampflosen Wertung war.
        stage: u8,
    },
}

/// Ein Ansage-Auftrag für die Geräte einer Halle.
///
/// `hall` leer = alle Hallen. `id` ist fortlaufend, damit ein Gerät nur
/// spricht, was es noch nicht gehört hat — dieselbe Buchführung wie beim
/// Freitext.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnounceJob {
    pub id: u64,
    pub hall: String,
    /// Zeitpunkt der Erteilung — Grundlage des Verfalls.
    #[serde(rename = "createdAtMs")]
    pub created_at_ms: u64,
    #[serde(flatten)]
    pub kind: AnnounceJobKind,
}

/// Nach dieser Zeit wird ein Auftrag nicht mehr gesprochen.
///
/// Ein Gerät, das eine Minute weg war, soll beim Wiederkommen nicht die
/// Aufrufe der letzten Minute nachplärren — die Spiele laufen längst.
const ANNOUNCE_JOB_TTL_MS: u64 = 60_000;

/// So lange gilt ein Ansage-Gerät nach seinem letzten Abruf als anwesend.
///
/// Großzügig gegenüber dem Abfragetakt der Geräte: Ein einzelner
/// ausgefallener Abruf soll die Turnierleitung nicht mit „hier hört niemand
/// zu" beunruhigen.
const ANNOUNCE_LISTENER_TTL_MS: u64 = 30_000;

/// Obergrenze der Zuhörer-Liste. Ein Turnier hat eine Handvoll Hallen; alles
/// darüber ist ein Fehler oder ein Angriff, und beides darf den Turnier-PC
/// nicht zum Absturz bringen.
const MAX_ANNOUNCE_LISTENERS: usize = 64;

/// Auf Platte gesicherter Live-Stand eines Felds (für den App-Neustart).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedScore {
    match_id: i64,
    sets: Vec<(i64, i64)>,
}

/// Auf Platte gesicherte BTP-Nachschub-Queue (übersteht App-Neustart, ADR
/// 0018). **Eigenes Schema** statt `#[derive(Serialize)]` direkt auf dem
/// BTP-Wire-Typ `MatchUpdate` (Entscheidung 3): explizite, versionierbare
/// Grenze zwischen Platten-Format und Protokoll — eine Protokolländerung
/// zieht nicht stillschweigend das Disk-Schema mit. Vorbild `PersistedScore`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedBtpQueue {
    /// Turnier-Guard = `snapshot.tournament_name`. Beim Laden wird die Datei
    /// bei Mismatch verworfen (BTP-`match_id` sind nur pro Turnier eindeutig).
    tournament: String,
    entries: Vec<PersistedBtpEntry>,
}

/// Ein persistierter Nachschub-Eintrag (Platten-Spiegel von `PendingBtpWrite`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedBtpEntry {
    update: PersistedMatchUpdate,
    /// Bezugszeitpunkt (Unix-ms) — steuert nach dem Laden weiterhin das
    /// Spieler-Checkout-Fenster und die Höchst-Lebensdauer (`prepare_btp_retry`).
    enqueued_ms: u64,
}

/// Platten-Spiegel aller `MatchUpdate`-Felder (inkl. `player_ids`,
/// `end_ts_ms` — nur BTP-IDs, keine Namen/Geburtsjahr).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedMatchUpdate {
    btp_match_id: i64,
    draw_id: i64,
    planning_id: i64,
    sets: Vec<(i64, i64)>,
    team1_won: bool,
    duration_mins: i64,
    score_status: i64,
    free_court_id: Option<i64>,
    player_ids: Vec<i64>,
    end_ts_ms: Option<u64>,
    /// Schiedsrichter-Besetzung (Live-Befund 14.08.2026, siehe
    /// `MatchUpdate::officials`). `#[serde(default)]`, damit eine vor diesem
    /// Feld persistierte Queue-Datei beim App-Neustart weiter lesbar bleibt.
    #[serde(default)]
    officials: Option<(i64, i64)>,
}

impl From<&crate::btp::proto::MatchUpdate> for PersistedMatchUpdate {
    fn from(u: &crate::btp::proto::MatchUpdate) -> Self {
        PersistedMatchUpdate {
            btp_match_id: u.btp_match_id,
            draw_id: u.draw_id,
            planning_id: u.planning_id,
            sets: u.sets.clone(),
            team1_won: u.team1_won,
            duration_mins: u.duration_mins,
            score_status: u.score_status,
            free_court_id: u.free_court_id,
            player_ids: u.player_ids.clone(),
            end_ts_ms: u.end_ts_ms,
            officials: u.officials,
        }
    }
}

impl From<PersistedMatchUpdate> for crate::btp::proto::MatchUpdate {
    fn from(p: PersistedMatchUpdate) -> Self {
        crate::btp::proto::MatchUpdate {
            btp_match_id: p.btp_match_id,
            draw_id: p.draw_id,
            planning_id: p.planning_id,
            sets: p.sets,
            team1_won: p.team1_won,
            duration_mins: p.duration_mins,
            score_status: p.score_status,
            free_court_id: p.free_court_id,
            player_ids: p.player_ids,
            end_ts_ms: p.end_ts_ms,
            officials: p.officials,
        }
    }
}

impl From<&PendingBtpWrite> for PersistedBtpEntry {
    fn from(w: &PendingBtpWrite) -> Self {
        PersistedBtpEntry {
            update: (&w.update).into(),
            enqueued_ms: w.enqueued_ms,
        }
    }
}

impl From<PersistedBtpEntry> for PendingBtpWrite {
    fn from(e: PersistedBtpEntry) -> Self {
        PendingBtpWrite {
            update: e.update.into(),
            enqueued_ms: e.enqueued_ms,
        }
    }
}

impl TabletState {
    /// Den neuesten BTP-Snapshot ablegen (vom Sync-Loop aufgerufen).
    pub fn set_snapshot(&self, snapshot: BtpSnapshot) {
        // Punktverlauf folgt dem Turnier des Snapshots (öffnet/lädt bei
        // Wechsel die zugehörige Datei) — ein leerer Name ändert nichts.
        self.timeline.set_tournament(&snapshot.tournament_name);
        // Schiedsrichter-Roster ebenso (ADR 0022) — und danach die
        // BTP-Officials-Liste in die Rotationsreihenfolge aufnehmen: neue
        // hinten dran, bekannte auf ihrem Platz. Reihenfolge der beiden
        // Aufrufe zählt: erst binden/verwerfen, dann füllen.
        self.officials.set_tournament(&snapshot.tournament_name);
        let official_ids: Vec<i64> = snapshot.officials.iter().map(|o| o.id).collect();
        self.officials.sync_roster(&official_ids);
        // Ausnahmeliste der Auto-Vergabe ebenso turniergebunden (Muster
        // ADR 0022, Spec `feldvergabe-ausnahme`).
        self.auto_assign_exclusions
            .set_tournament(&snapshot.tournament_name);
        // Manuelle Spielreihenfolge ebenso turniergebunden (ADR 0023).
        self.queue_order.set_tournament(&snapshot.tournament_name);
        // Spielzeiten-Messung ebenso turniergebunden (Spec
        // `spielzeiten-prognose`, Muster ADR 0022).
        self.match_times.set_tournament(&snapshot.tournament_name);
        // Auto-Hallen ebenso turniergebunden (Spec `hallen-vorverteilung`).
        self.auto_halls.set_tournament(&snapshot.tournament_name);
        // Turnier-Guard der persistenten Nachschub-Queue mitführen (ADR 0018):
        // dieselbe Identität wie der Punktverlauf-Speicher (`tournament_name`).
        *self.btp_retry_tournament.write().unwrap() = snapshot.tournament_name.clone();
        // Beim ERSTEN Snapshot ist der Guard verfügbar → die Queue genau
        // einmal (CAS) von Platte laden (Merge, nicht Replace). Spätere
        // Snapshots aktualisieren nur den Guard.
        if self
            .btp_retry_loaded
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.load_btp_retry();
        }
        *self.snapshot.write().unwrap() = Some(snapshot);
    }

    /// Der Punktverlauf-Speicher (geteilt von LAN-Server, Relay-Client
    /// und Tauri-Commands).
    pub fn timeline_store(&self) -> &crate::tablet::timeline::TimelineStore {
        &self.timeline
    }

    /// Der Schiedsrichter-Roster (Spec `schiedsrichter-management`).
    pub fn officials_store(&self) -> &crate::tablet::officials::OfficialsStore {
        &self.officials
    }

    /// Ist dieses Match gerade von der automatischen Feldvergabe ausgenommen
    /// (Spec `feldvergabe-ausnahme`)? Aufrufer bleiben `auto_assign`
    /// (sync.rs) und beide Anzeigen (TL-Web-Warteliste,
    /// Desktop-Kandidatenliste) — nie der Store direkt, damit es nur diese
    /// eine Prüfung gibt.
    pub fn auto_assign_excluded(&self, match_id: i64) -> bool {
        self.auto_assign_exclusions.is_excluded(match_id)
    }

    /// Ausnahme setzen oder zurücknehmen — Ziel sowohl des TL-Web-Actions-
    /// Pfads (`tl.rs`) als auch des Desktop-Commands (`commands.rs`), beide
    /// auf demselben Speicher.
    pub fn set_auto_assign_excluded(&self, match_id: i64, excluded: bool) {
        self.auto_assign_exclusions.set_excluded(match_id, excluded);
    }

    /// Aufräumen bei Spielende (aus `sync.rs::reconcile_auto_assign_exclusions`):
    /// entfernt jede Ausnahme, deren Match nicht mehr in `keep` steht.
    pub fn retain_auto_assign_exclusions(&self, keep: &std::collections::HashSet<i64>) {
        self.auto_assign_exclusions.retain(keep);
    }

    /// Der Speicher der manuellen Spielreihenfolge (Spec
    /// `spielliste-manuelle-reihenfolge`) — geteilt von TL-Web-Actions,
    /// Tauri-Commands und `sync.rs`.
    pub fn queue_order_store(&self) -> &crate::tablet::queue_order::QueueOrderStore {
        &self.queue_order
    }

    /// Ein noch nicht gerufenes Spiel vor ein anderes ziehen (Spec
    /// `spielliste-manuelle-reihenfolge`) — **geteilter Einstiegspunkt**
    /// für den TL-Web-Dispatch (`tl.rs::apply_state_action`) und den
    /// Desktop-Command (`commands::queue_reorder`), damit ein Zug auf
    /// beiden Oberflächen identisch wirkt (Konsistenz-Pflicht, ADR 0023).
    /// Die Reihenfolge gilt turnierweit, nicht je Halle (ADR 0026).
    /// Liefert `false`, wenn das Match nicht (mehr) im aktuellen Snapshot
    /// steht.
    pub fn queue_reorder(
        &self,
        config: &crate::config::AppConfig,
        match_id: i64,
        before_match_id: Option<i64>,
    ) -> bool {
        let Some(snap) = self.snapshot_clone() else {
            return false;
        };
        if !snap.matches.iter().any(|m| m.id == match_id) {
            return false;
        }
        let manual = self.manual_halls();
        let auto = self.auto_halls.halls();
        let called: HashSet<i64> = self
            .preparation_calls()
            .iter()
            .map(|c| c.match_id)
            .collect();
        let effective = crate::tablet::assign::ready_queue(
            config,
            &snap,
            &manual,
            &auto,
            &called,
            &self.queue_order,
        );
        // TL-Web zeigt nur die ersten `QUEUE_LIMIT` Spiele
        // (`tl::build_state_limited`) — der neue Präfix darf serverseitig nie
        // mehr Spiele umfassen, als die ziehende Oberfläche überhaupt zeigen
        // konnte. Sonst zöge ein Zug ans (dort unsichtbare) Ende der vollen
        // Liste Spiele in den Präfix, die auf TL-Web niemand gesehen hat
        // (Code-Review-Fund 14.08.2026). Das gezogene und das Zielspiel
        // selbst bleiben immer erreichbar — auch wenn sie (nur vom
        // unbegrenzten Desktop-Weg aus möglich) jenseits der Grenze liegen.
        let visible = [
            Some(crate::tablet::tl::QUEUE_LIMIT),
            effective
                .iter()
                .position(|id| *id == match_id)
                .map(|p| p + 1),
            before_match_id
                .and_then(|b| effective.iter().position(|id| *id == b))
                .map(|p| p + 1),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(effective.len())
        .min(effective.len());
        self.queue_order
            .reorder(&effective[..visible], match_id, before_match_id);
        true
    }

    /// Die manuelle Reihenfolge auf einmal verwerfen
    /// (globaler Reset-Knopf, Spec `spielliste-manuelle-reihenfolge`).
    pub fn queue_order_reset(&self) {
        self.queue_order.reset_all();
    }

    /// Ablage-Datei der Auto-Vergabe-Ausnahmeliste setzen (beim App-Start).
    pub fn set_auto_assign_exclusions_path(&self, path: std::path::PathBuf) {
        self.auto_assign_exclusions.set_path(path);
    }

    /// Ablage-Datei der manuellen Spielreihenfolge setzen (beim App-Start).
    pub fn set_queue_order_path(&self, path: std::path::PathBuf) {
        self.queue_order.set_path(path);
    }

    /// Der Spielzeiten-Speicher (Spec `spielzeiten-prognose`) — geteilt von
    /// Sync-Loop, Ergebnis-Pfaden und TL-Web.
    pub fn match_times_store(&self) -> &crate::tablet::match_times::MatchTimesStore {
        &self.match_times
    }

    /// Ablage-Datei der Spielzeiten-Messung setzen (beim App-Start).
    pub fn set_match_times_path(&self, path: std::path::PathBuf) {
        self.match_times.set_path(path);
    }

    /// Der Speicher der automatisch vorverteilten Hallen (Spec
    /// `hallen-vorverteilung`) — geteilt von Sync-Loop, TL-Web und Kaskade.
    pub fn auto_hall_store(&self) -> &crate::tablet::hall_assign::AutoHallStore {
        &self.auto_halls
    }

    /// Ablage-Datei der Auto-Hallen setzen (beim App-Start).
    pub fn set_auto_halls_path(&self, path: std::path::PathBuf) {
        self.auto_halls.set_path(path);
    }

    /// Zuletzt publizierte Startzeit-Prognosen merken (Match-ID → Unix-ms) —
    /// nur fürs Diagnose-Log: Beim echten Aufruf vergleicht der Sync-Loop
    /// Prognose und Wirklichkeit (Erfolgsmaß E12, ±10 min / 70 %).
    ///
    /// **Gemergt statt ersetzt** (Review 2026-08-16, F6): Die Relay-
    /// Größenleiter baut denselben Zustand mit kleineren Wartelisten —
    /// ein Ersetzen ließe nur die Matches der kleinsten Stufe übrig und
    /// die Prognose-Kontrolle bliebe für alle dahinter stumm. Aufgeräumt
    /// wird über [`Self::take_predicted_start`] (geloggt) und
    /// [`Self::retain_predicted_starts`] (aus dem Turnier verschwunden).
    pub(crate) fn merge_predicted_starts(&self, map: std::collections::HashMap<i64, u64>) {
        self.predicted_starts.write().unwrap().extend(map);
    }

    /// Alle bekannten Prognosen als Kopie — für den `sched`-Versand an badhub.
    ///
    /// Bewusst eine Momentaufnahme statt eines Lock-Durchgriffs: der Aufrufer
    /// baut daraus eine Nachricht und soll dafür nicht den Schreib-Lock der
    /// Prognose-Berechnung blockieren.
    pub(crate) fn predicted_starts_snapshot(&self) -> std::collections::HashMap<i64, u64> {
        self.predicted_starts.read().unwrap().clone()
    }

    /// Zuletzt publizierte Prognose eines Matches — nur die Tests lesen
    /// hier; die Produktion konsumiert über [`Self::take_predicted_start`].
    #[cfg(test)]
    pub(crate) fn predicted_start_ms(&self, match_id: i64) -> Option<u64> {
        self.predicted_starts
            .read()
            .unwrap()
            .get(&match_id)
            .copied()
    }

    /// Prognose eines Matches herausnehmen (genau eine Log-Zeile je
    /// Aufruf — der Eintrag ist danach verbraucht).
    pub(crate) fn take_predicted_start(&self, match_id: i64) -> Option<u64> {
        self.predicted_starts.write().unwrap().remove(&match_id)
    }

    /// Prognosen von Matches vergessen, die nicht mehr warten (beendet,
    /// kampflos, aus dem Snapshot verschwunden) — sonst wüchse das
    /// Gedächtnis über das Turnier hinweg.
    pub(crate) fn retain_predicted_starts(&self, keep: &std::collections::HashSet<i64>) {
        self.predicted_starts
            .write()
            .unwrap()
            .retain(|id, _| keep.contains(id));
    }

    /// Endzeitpunkt eines Ergebnisses für die BTP-`Duration` (Spec
    /// `spielzeiten-prognose`, E3): der ursprüngliche Ende-Stempel, falls
    /// vorhanden (eine Korrektur rechnet nicht mit „jetzt"), sonst `now`.
    /// **Der** gemeinsame Weg aller Ergebnis-Pfade — Gegenstück zu
    /// [`Self::brutto_start_ms`] (Review 2026-08-16, F9).
    pub(crate) fn result_end_ms(&self, match_id: i64, now: u64) -> u64 {
        self.match_times
            .entry(match_id)
            .and_then(|e| e.finished_ms)
            .unwrap_or(now)
    }

    /// Median-Statistik der Spielzeiten — je Messwert-Generation genau
    /// einmal gerechnet (Review 2026-08-16, F8): Der TL-Zustand wird alle
    /// ~2 s je Gerät gebaut (Relay-Leiter: mehrfach je Push); ohne Cache
    /// klonte und sortierte jeder Aufruf die komplette Zeiten-Map, obwohl
    /// sich Messwerte nur beim Stempeln ändern.
    pub(crate) fn cached_time_stats(&self) -> std::sync::Arc<crate::tablet::predict::TimeStats> {
        let generation = self.match_times.generation();
        let mut cache = self.time_stats_cache.lock().unwrap();
        if let Some((g, stats)) = cache.as_ref() {
            if *g == generation {
                return stats.clone();
            }
        }
        let stats = std::sync::Arc::new(crate::tablet::predict::time_stats(
            &self.match_times.entries(),
        ));
        *cache = Some((generation, stats.clone()));
        stats
    }

    /// Bruttostart eines Matches für die BTP-`Duration` (Spec
    /// `spielzeiten-prognose`, E1): der persistierte Erst-Stempel, mit
    /// `on_court_since` (RAM) als Zubringer-Fallback, solange der Sync-Poll
    /// noch nicht gestempelt hat. **Der** gemeinsame Weg aller
    /// Ergebnis-Pfade — vier verschiedene Schreibweisen derselben Kaskade
    /// wären der sichere Weg, eine davon zu vergessen.
    pub(crate) fn brutto_start_ms(&self, match_id: i64, court_id: Option<i64>) -> Option<u64> {
        self.match_times
            .first_assigned_ms(match_id)
            .or_else(|| court_id.and_then(|cid| self.on_court_since_ms(cid, match_id)))
    }

    /// [`Self::brutto_start_ms`] plus Erster-Punkt-Stempel in EINEM
    /// Store-Zugriff — für den TL-State-Bau, der beide je belegtem Feld
    /// alle ~2 s liest (Review 2026-08-17). Die Fallback-Kaskade des
    /// Bruttostarts lebt damit weiterhin nur HIER, nicht in tl.rs
    /// nachbuchstabiert.
    pub(crate) fn court_time_stamps(
        &self,
        match_id: i64,
        court_id: Option<i64>,
    ) -> (Option<u64>, Option<u64>) {
        let (assigned, first_point) = self.match_times.stamps(match_id);
        let start =
            assigned.or_else(|| court_id.and_then(|cid| self.on_court_since_ms(cid, match_id)));
        (start, first_point)
    }

    /// Die Schiedsrichter-Besetzung, die beim Ruf aufs Feld **mit nach BTP**
    /// geschrieben werden soll (ADR 0021): `(Official1ID, Official2ID)`,
    /// `0` = kein Dienst.
    ///
    /// `None` heißt „gar nicht anfassen": ohne Schiedsrichter-Betrieb und
    /// bei einem Spiel, das in BTS Light nie eingeteilt wurde, bleibt der
    /// Request exakt wie im Bestand.
    ///
    /// Hier — und **nur** hier — schlägt die lokale Absicht den BTP-Stand:
    /// Wer von Hand umteilt oder eine Zuweisung löst, will genau das nach
    /// BTP schreiben; sonst ließe sich eine einmal geschriebene Besetzung nie
    /// wieder ändern. Die **Anzeige** folgt weiter der Spec-Regel „BTP
    /// gewinnt" (`OfficialsStore::effective`) — bestätigt ist erst, was der
    /// nächste Snapshot zeigt. Ein Dienst, den BTS Light nie angefasst hat,
    /// wird unverändert mitgeschrieben statt gelöscht.
    pub fn officials_for_write(&self, m: &BtpMatch) -> Option<(i64, i64)> {
        if !self.officials.enabled() {
            return None;
        }
        let lokal = self.officials.assignment(m.id);
        if lokal.sr.is_none() && lokal.ar.is_none() {
            return None;
        }
        Some((
            lokal.sr.or(m.official1_id).unwrap_or(0),
            lokal.ar.or(m.official2_id).unwrap_or(0),
        ))
    }

    /// Die wirksamen Official-IDs eines Spiels (0 = keiner) — für die
    /// Bedienung, die eine Person eindeutig treffen muss.
    pub fn court_official_ids(&self, m: Option<&BtpMatch>) -> (i64, i64) {
        if !self.officials.enabled() {
            return (0, 0);
        }
        let Some(m) = m else { return (0, 0) };
        let w = self
            .officials
            .effective(m.id, m.official1_id, m.official2_id);
        (w.sr.unwrap_or(0), w.ar.unwrap_or(0))
    }

    /// Nur die Namen von SR und AR eines Spiels — die Form, die ins
    /// [`MatchBrief`](relay_proto::MatchBrief) ans Tablet geht (LAN wie
    /// Cloud, ferne Halle eingeschlossen). Holt sich den Snapshot selbst,
    /// weil die Push-Pfade keinen zur Hand haben.
    pub fn match_officials(&self, m: &BtpMatch) -> (Vec<String>, Vec<String>) {
        let Some(snap) = self.snapshot_clone() else {
            return (Vec::new(), Vec::new());
        };
        let (sr, ar, _) = self.court_officials(Some(m), &snap);
        (sr, ar)
    }

    /// Namen von SR und AR eines Spiels plus Konflikt-Kategorie — die Form,
    /// die Feldübersicht, TL-State und Tablet gleichermaßen anzeigen.
    ///
    /// Ohne SR-Betrieb (`officials.enabled` aus) ist alles leer: Ein Turnier,
    /// das ohne Schiedsrichter spielt, soll auch dann keinen sehen, wenn in
    /// BTP zufällig einer am Spiel steht (Spec Nr. 1).
    pub fn court_officials(
        &self,
        m: Option<&BtpMatch>,
        snap: &BtpSnapshot,
    ) -> (Vec<String>, Vec<String>, Option<String>) {
        let leer = (Vec::new(), Vec::new(), None);
        if !self.officials.enabled() {
            return leer;
        }
        let Some(m) = m else { return leer };
        let wirksam = self
            .officials
            .effective(m.id, m.official1_id, m.official2_id);
        let name = |id: Option<i64>| -> Vec<String> {
            id.and_then(|id| snap.official(id))
                .map(|o| vec![o.display_name()])
                .unwrap_or_default()
        };
        // Konflikt-Warnung: Der Grund bleibt hier, nach außen geht nur die
        // Kategorie. Beide Dienste werden geprüft, der erste Treffer zählt.
        let spieler: Vec<crate::btp::model::BtpPlayer> =
            m.team1.iter().chain(m.team2.iter()).cloned().collect();
        let warn = [wirksam.sr, wirksam.ar]
            .into_iter()
            .flatten()
            .find_map(|id| {
                crate::tablet::officials::official_conflict(&self.officials.extra(id), &spieler)
            })
            .map(|k| k.label().to_string());
        (name(wirksam.sr), name(wirksam.ar), warn)
    }

    /// Reiht einen fehlgeschlagenen BTP-Ergebnis-Write in die
    /// Nachschub-Queue ein (Cluster A5). Existiert für das Match schon ein
    /// Eintrag, ersetzt der neuere Stand den alten — der Bezugszeitpunkt
    /// des ERSTEN Fehlschlags bleibt (er steuert das Spieler-Checkout-
    /// Fenster und die Höchst-Lebensdauer).
    pub fn queue_btp_retry(&self, update: crate::btp::proto::MatchUpdate, now: u64) {
        {
            let mut q = self.btp_retry.write().unwrap();
            if let Some(e) = q
                .iter_mut()
                .find(|e| e.update.btp_match_id == update.btp_match_id)
            {
                e.update = update;
            } else {
                if q.len() >= BTP_RETRY_CAP {
                    q.remove(0); // ältesten opfern — Queue darf nie unbegrenzt wachsen
                }
                q.push(PendingBtpWrite {
                    update,
                    enqueued_ms: now,
                });
            }
        } // Write-Lock hier freigeben — die Platten-I/O läuft NIE unter dem
          // Daten-Lock (ADR 0018, Entscheidung 4; Vorbild `persist_scores`).
        self.persist_btp_retry();
    }

    /// Entfernt den Queue-Eintrag eines Matches — nach erfolgreichem Write
    /// (egal ob durch Nachschub oder den regulären Weg gelungen).
    pub fn clear_btp_retry(&self, match_id: i64) {
        self.btp_retry
            .write()
            .unwrap()
            .retain(|e| e.update.btp_match_id != match_id);
        // Verkleinerte Queue synchron auf Platte spiegeln (der Write-Lock der
        // Zeile oben ist mit dem Statement schon wieder freigegeben).
        self.persist_btp_retry();
    }

    /// Pfad der persistenten Nachschub-Queue setzen (beim Start). Aktiviert
    /// die Persistenz; ohne Pfad sind `persist`/`load` No-ops (Tests).
    pub fn set_btp_retry_path(&self, path: PathBuf) {
        let _ = self.btp_retry_path.set(path);
    }

    /// Die aktuelle Nachschub-Queue synchron + atomar nach `btp-retry.json`
    /// schreiben (best effort: ein Schreibfehler darf die Ergebnisannahme NIE
    /// blockieren — wie `persist_scores`). No-op ohne Pfad oder ohne Turnier.
    fn persist_btp_retry(&self) {
        let Some(path) = self.btp_retry_path.get().cloned() else {
            return; // kein Pfad → Persistenz aus
        };
        // Schreiber serialisieren (Vorbild `scores_persist_lock`): sonst
        // könnten zwei gleichzeitige Ergebnis-Writes Temp- oder Zieldatei
        // gegenseitig zerlegen.
        let _guard = self.btp_retry_persist_lock.lock().unwrap();
        let tournament = self.btp_retry_tournament.read().unwrap().clone();
        if tournament.is_empty() {
            // Ohne Turnier-Guard nicht schreiben — die Datei ließe sich beim
            // Laden keinem Turnier zuordnen (und würde verworfen).
            return;
        }
        // Queue unter kurzem `read()` klonen, Read-Guard sofort fallenlassen —
        // die anschließende Datei-I/O hält KEIN Daten-Lock.
        let entries: Vec<PersistedBtpEntry> = {
            let q = self.btp_retry.read().unwrap();
            q.iter().map(PersistedBtpEntry::from).collect()
        };
        let data = PersistedBtpQueue {
            tournament,
            entries,
        };
        if let Ok(json) = serde_json::to_string(&data) {
            // Atomar: erst Temp-Datei, dann umbenennen — nie eine halb
            // geschriebene btp-retry.json (kein `fsync` → Durabilität nur
            // gegen App-Neustart, nicht Stromausfall; bewusst, ADR 0018).
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Die persistierte Nachschub-Queue beim ersten Snapshot laden (Turnier-
    /// Guard verfügbar). Fehlende/korrupte Datei → leere Queue, kein Panic.
    /// **Merge statt Replace**: nur match_ids laden, die noch nicht in der
    /// Queue stehen — frisch (nach Start) Eingereihtes gewinnt. **Keine**
    /// erneute Match-Validierung (R5): die Einträge waren vor dem Neustart
    /// validiert; Turnier-Guard + `prepare_btp_retry`-Drop-Regeln (Sieger da,
    /// zu alt, Feld neu belegt) fangen Veraltetes beim Flush ab.
    fn load_btp_retry(&self) {
        let Some(path) = self.btp_retry_path.get() else {
            return; // kein Pfad → nichts zu laden
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return; // keine Datei (erster Start) → leere Queue
        };
        let Ok(data) = serde_json::from_str::<PersistedBtpQueue>(&text) else {
            tracing::warn!("btp-retry.json unlesbar – ignoriere");
            return;
        };
        let current = self.btp_retry_tournament.read().unwrap().clone();
        if data.tournament != current {
            // Fremdes Turnier: verwerfen — ein Replay schriebe sonst in ein
            // gleichnamiges Match eines ANDEREN Turniers (match_id-Kollision).
            tracing::warn!("btp-retry.json gehört zu einem anderen Turnier – verworfen");
            return;
        }
        let mut q = self.btp_retry.write().unwrap();
        for entry in data.entries {
            let mid = entry.update.btp_match_id;
            if q.iter().any(|e| e.update.btp_match_id == mid) {
                continue; // frisch Eingereihtes gewinnt (Merge, nicht Replace)
            }
            if q.len() >= BTP_RETRY_CAP {
                break; // Kapazitäts-Deckel — geladene Reste verwerfen
            }
            q.push(entry.into());
        }
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
    /// Felder, auf denen die Bediener-Vergabe abgeschaltet ist (Spec
    /// `schiedsrichter-management` Nr. 6 — dort bedient der Schiedsrichter
    /// selbst), bleiben außen vor und verbrauchen **keinen** Eintrag aus der
    /// Warteschlange. Ohne Eintrag gilt „aktiv", das Bestandsverhalten.
    ///
    /// Der Schalter greift **nur bei eingeschaltetem Schiedsrichter-Betrieb**:
    /// Seine einzige Bedienstelle liegt in der Schiedsrichter-Oberfläche, und
    /// die ist ohne das Feature nicht erreichbar. Ohne diese Bedingung bliebe
    /// ein einmal ausgenommenes Feld nach dem Abschalten für immer ohne
    /// Bediener, ohne dass es irgendwo zurückzunehmen wäre.
    pub fn assign_scorekeeper_for_court(&self, court_id: i64, match_id: i64) {
        if self.officials.enabled() && !self.officials.court_switches(court_id).operator {
            return;
        }
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
        // Die Aufrufe gehören zur Standzeit: Verlässt ein Spiel das Feld,
        // müssen auch seine Aufrufe vergessen werden. Sonst zeigte dasselbe
        // Spiel nach einer erneuten Zuweisung sofort „3. Aufruf erfolgt" —
        // und der Aufruf-Knopf verschwände dauerhaft.
        self.call_stages
            .write()
            .unwrap()
            .retain(|court_id, (mid, _, _)| oncourt.get(court_id) == Some(mid));
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

    /// Die Hallennamen des Turniers (BTP-`Locations`, getrimmt, ohne leere).
    /// DIE kanonische Hallenliste der Hallen-Farben (Review 2026-08-16):
    /// Desktop-`paint`, Farb-Picker und TL-Web müssen aus derselben Liste
    /// auflösen — sonst trüge dieselbe Halle je Oberfläche verschiedene
    /// Auto-Farben, sobald eine Location ohne Felder existiert.
    pub fn hall_names(&self) -> Vec<String> {
        self.snapshot
            .read()
            .unwrap()
            .as_ref()
            .map(|s| {
                s.locations
                    .iter()
                    .map(|l| l.name.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect()
            })
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

    /// Die Schiedsrichter-Besetzung, die ein Ergebnis-`SENDUPDATE` für dieses
    /// Match reassertieren soll (Live-Befund 14.08.2026, siehe
    /// `MatchUpdate::officials`): `None`, wenn ohne Schiedsrichter-Betrieb
    /// gespielt wird — dann bleibt der Request unverändert zum Bestand.
    /// Sonst immer `Some((sr, ar))` (`0` = kein Dienst), auch wenn nie
    /// jemand zugewiesen war — das schreibt explizit „niemand" und ist
    /// dieselbe Werte-Reassertion wie beim Feld selbst.
    pub fn officials_for_result(&self, match_id: i64) -> Option<(i64, i64)> {
        if !self.officials.enabled() {
            return None;
        }
        let (btp_sr, btp_ar) = self
            .snapshot
            .read()
            .unwrap()
            .as_ref()
            .and_then(|s| s.matches.iter().find(|m| m.id == match_id))
            .map(|m| (m.official1_id, m.official2_id))
            .unwrap_or((None, None));
        let wirksam = self.officials.effective(match_id, btp_sr, btp_ar);
        Some((wirksam.sr.unwrap_or(0), wirksam.ar.unwrap_or(0)))
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
        // A2 / ADR 0017: Der aktuelle Slot-Halter hat seit seinem Claim gezählt —
        // das macht ihn zum „legitimen Weiterzähler". Das Flag wird BEWUSST VOR
        // dem Score-Write gesetzt (getrennte Locks): Ein gleichzeitig
        // reconnectendes Tablet, das `court_owner` liest, sieht dann nie „Score
        // schon geschrieben, Flag noch nicht" — das Fenster löst sich in die
        // SICHERE Richtung auf (im Zweifel `scored=true` ⇒ Rückkehrer tritt
        // zurück, überschreibt den Übernehmer NICHT). Review-Befund M2.
        self.scored_since_claim.write().unwrap().insert(court_id);
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
        // Niedrig-latente Anzeige (A1, ADR 0016): Court-Monitor + Feld-
        // Übersicht sofort anstoßen, statt auf ihren nächsten Poll zu warten.
        self.notify_monitor(court_id);
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
        {
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
        // Meldungs-Zustand (Pause/Verletzung) ist am Court-Monitor sichtbar →
        // Anzeige anstoßen. Wie bei `record_score` erst den `courts`-Guard
        // droppen, DANN nudgen (Lock nicht über den Broadcast halten).
        self.notify_monitor(court_id);
    }

    /// Meldet einen Court-Monitor als Nudge-Abonnenten an (A1, ADR 0016).
    /// `court = Some(id)` → nur Nudges dieses Felds (Court-Monitor
    /// `monitor.html`); `court = None` → Nudges ALLER Felder (Feld-Übersicht
    /// `overview.html`). Der Aufrufer (`monitor_socket`) besitzt den Kanal und
    /// reicht sein Sende-Ende herein — analog zum Relay-Muster, damit
    /// `unsubscribe_monitor` denselben Sender per `same_channel` wiederfindet.
    ///
    /// Liefert `true`, wenn das Abo eingetragen wurde; `false`, wenn der
    /// Fan-out-Deckel `MAX_MONITOR_SUBS` erreicht ist — dann bedient der
    /// Aufrufer die Verbindung nicht und die Anzeige fällt still auf Poll
    /// zurück (Zuschauer-DoS-Schutz, spiegelt das Relay).
    pub fn subscribe_monitor(&self, court: Option<i64>, tx: &MonitorNudgeTx) -> bool {
        // Beide Listen in fester Reihenfolge (erst feld-spezifisch, dann
        // „alle") sperren, um den Gesamtstand konsistent zu zählen. Nur diese
        // Methode hält je beide Locks gleichzeitig; `notify_monitor`/
        // `unsubscribe_monitor` fassen immer nur eines an → kein Deadlock.
        let mut subs = self.monitor_subs.write().unwrap();
        let mut subs_all = self.monitor_subs_all.write().unwrap();
        let total = subs.values().map(Vec::len).sum::<usize>() + subs_all.len();
        if total >= MAX_MONITOR_SUBS {
            return false;
        }
        match court {
            Some(c) => subs.entry(c).or_default().push(tx.clone()),
            None => subs_all.push(tx.clone()),
        }
        true
    }

    /// Trägt eine Monitor-Verbindung wieder aus (Verbindungsende, A1). Der
    /// `notify_monitor`-Pfad siebt tote Sender zwar ohnehin lazy aus, doch das
    /// greift erst beim nächsten Nudge des betroffenen Felds — ein stiller
    /// Court könnte tote Einträge beliebig lange halten. Darum trägt der
    /// WS-Handler seinen Sender beim Schließen explizit aus (per
    /// `same_channel`, damit nur der eigene verschwindet). Spiegelt das Relay.
    pub fn unsubscribe_monitor(&self, court: Option<i64>, tx: &MonitorNudgeTx) {
        match court {
            Some(c) => {
                let mut subs = self.monitor_subs.write().unwrap();
                if let Some(list) = subs.get_mut(&c) {
                    list.retain(|t| !t.same_channel(tx));
                    if list.is_empty() {
                        subs.remove(&c);
                    }
                }
            }
            None => self
                .monitor_subs_all
                .write()
                .unwrap()
                .retain(|t| !t.same_channel(tx)),
        }
    }

    /// Weckt die Monitor-Abonnenten eines Felds (A1, ADR 0016): erhöht die
    /// pro-Court-Sequenz und schickt den winzigen Nudge
    /// `{"court":<id>,"seq":<n>}` an die Abonnenten GENAU dieses Felds UND an
    /// die „alle Felder"-Abonnenten (Feld-Übersicht). Tote Sender (Anzeige
    /// weg) werden dabei ausgesiebt. Kein `.await`, kein Netz — der Kanal ist
    /// unbounded, `send` kehrt sofort zurück; das Halten des `record_score`-
    /// Locks ist damit unkritisch.
    pub fn notify_monitor(&self, court_id: i64) {
        let seq = {
            let mut seqs = self.monitor_seq.write().unwrap();
            let s = seqs.entry(court_id).or_insert(0);
            *s += 1;
            *s
        };
        let nudge = serde_json::to_string(&MonitorNudge {
            court: court_id,
            seq,
        })
        .unwrap_or_default();
        // Feld-spezifische Abonnenten: senden + tote aussieben, leere Liste
        // ganz entfernen (kein Speicher-Leck über die Turnierdauer).
        {
            let mut subs = self.monitor_subs.write().unwrap();
            if let Some(list) = subs.get_mut(&court_id) {
                list.retain(|tx| tx.send(nudge.clone()).is_ok());
                if list.is_empty() {
                    subs.remove(&court_id);
                }
            }
        }
        // „Alle Felder"-Abonnenten (Feld-Übersicht `overview.html`).
        self.monitor_subs_all
            .write()
            .unwrap()
            .retain(|tx| tx.send(nudge.clone()).is_ok());
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
        // A2 / ADR 0017: Ein neuer Claim eröffnet einen neuen Zähl-Abschnitt —
        // der neue Halter hat noch nicht gezählt. Ohne dieses Zurücksetzen
        // würde ein alter „scored"-Zustand fälschlich den frischen Übernehmer
        // als „hat schon weitergezählt" ausweisen.
        self.scored_since_claim.write().unwrap().remove(&court_id);
        token
    }

    /// Aktueller Slot-Halter eines Felds als Ownership-Token (A2 / ADR 0017):
    /// `(epoch, device, scored_since_claim)`. `None` = Feld frei. Baut den
    /// Wert aus `active` (Epoch + Gerät) und dem `scored_since_claim`-Flag.
    /// Grundlage für [`reconnect_decision`] an den Reconnect-Eintritten.
    pub fn court_owner(&self, court_id: i64) -> Option<CourtOwner> {
        let active = self.active.read().unwrap();
        let (epoch, device) = active.get(&court_id)?;
        let scored = self.scored_since_claim.read().unwrap().contains(&court_id);
        Some(CourtOwner {
            epoch: *epoch,
            device: device.clone(),
            scored_since_claim: scored,
        })
    }

    /// Ein Match per ID aus dem aktuellen Snapshot – UNABHÄNGIG vom Status
    /// (anders als [`Self::match_for_court`], das nur OnCourt liefert). Für das
    /// Finalisiert-Signal (A2 / ADR 0017): das gerade beendete Match trägt
    /// Status Finished, muss dem Tablet aber noch mit `finalized:true`
    /// nachgereicht werden. `None`, wenn das Match nicht im Snapshot steht.
    pub fn snapshot_match(&self, match_id: i64) -> Option<BtpMatch> {
        let guard = self.snapshot.read().unwrap();
        let snap = guard.as_ref()?;
        snap.matches.iter().find(|m| m.id == match_id).cloned()
    }

    /// OnCourt→Finished: das dem Feld zugewiesene Match ist in BTP finalisiert
    /// (Sieger steht, per Hand fertig eingegeben — A2 / ADR 0017, Regel b). Vom
    /// Sync-Loop beim Übergang mit der Match-ID gesetzt; TTL-frisch gehalten.
    pub fn mark_finalized(&self, court_id: i64, match_id: i64) {
        self.recently_finalized
            .write()
            .unwrap()
            .insert(court_id, (match_id, std::time::Instant::now()));
    }

    /// Match-ID des zuletzt finalisierten Matches dieses Felds, falls der Merker
    /// noch frisch ist (innerhalb [`FINALIZED_TTL`]); sonst `None`. Räumt einen
    /// abgelaufenen Eintrag nebenbei weg, damit die Karte nicht wächst.
    pub fn recently_finalized(&self, court_id: i64) -> Option<i64> {
        let mut map = self.recently_finalized.write().unwrap();
        match map.get(&court_id) {
            Some((mid, at)) if at.elapsed() < FINALIZED_TTL => Some(*mid),
            Some(_) => {
                map.remove(&court_id);
                None
            }
            None => None,
        }
    }

    /// Ist GENAU dieses Match auf dem Feld gerade finalisiert? Grundlage des
    /// Finalisiert-Gates in `handle_score` (A2 / ADR 0017): ein nachlaufender
    /// Score fürs finalisierte Match wird verworfen — das Gate ERGÄNZT den
    /// Stale-/Plausibilitäts-Filter, R5 (`process_result`) bleibt unberührt.
    pub fn is_match_finalized(&self, court_id: i64, match_id: i64) -> bool {
        self.recently_finalized(court_id) == Some(match_id)
    }

    /// Räumt den Finalisiert-Merker eines Felds **bedingungslos** — vom Sync-Loop
    /// je Zyklus für jedes Feld aufgerufen, das ein Match **OnCourt** hat. Ein
    /// OnCourt-Match ist per BTP-Definition nicht finalisiert; jeder Merker auf so
    /// einem Feld ist daher veraltet und muss weg — auch wenn dieselbe matchId
    /// zurückkehrt (TL-Ergebniskorrektur/Undo setzt ein finalisiertes Match auf
    /// demselben Feld wieder OnCourt; ohne dieses bedingungslose Räumen verwürfe
    /// `handle_score` dessen Punkte still bis zum TTL-Ablauf — Review-Befund).
    /// Felder OHNE OnCourt-Match iteriert der Aufrufer nicht → dort hält der
    /// Merker (das Tablet zeigt noch das fertige Spiel; späte Scores gegated),
    /// bis die TTL greift.
    pub fn clear_finalized(&self, court_id: i64) {
        self.recently_finalized.write().unwrap().remove(&court_id);
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
        // Aufschlag/Pause (`court_state`) ist am Court-Monitor sichtbar →
        // Anzeige anstoßen (A1, ADR 0016). Der Schreib-Guard ist mit dem
        // Semikolon oben schon gefallen; kein Lock über den Broadcast.
        self.notify_monitor(court_id);
    }

    /// Liefert den gespiegelten Spielzustand eines Felds (für die Übernahme).
    pub fn court_state(&self, court_id: i64) -> Option<String> {
        self.court_state.read().unwrap().get(&court_id).cloned()
    }

    /// Court-Session entfernen (nach übermitteltem Ergebnis).
    /// Beansprucht ein Feld für ein Spiel — **bevor** nach BTP geschrieben
    /// wird.
    ///
    /// Liefert `false`, wenn das Feld oder das Spiel bereits beansprucht ist.
    /// Genau darin liegt der Zweck: Zwei Geräte, die im selben Moment
    /// dasselbe Feld antippen, laufen sonst beide durch die Prüfung (der
    /// Schnappschuss zeigt das Feld ja noch frei) und schreiben nacheinander
    /// nach BTP — der spätere gewinnt, und die Spieler des ersten stehen vor
    /// einem fremd belegten Feld. Die Entscheidung muss fallen, bevor der
    /// Schreibvorgang beginnt, denn der dauert.
    ///
    /// Auch die Spiel-Achse zählt: Dasselbe Spiel auf zwei Feldern hinterließe
    /// eines davon dauerhaft mit einem Geisterspiel belegt.
    pub fn try_reserve_court(&self, court_id: i64, match_id: i64, now_ms: u64) -> bool {
        let mut pending = self.pending_assign.write().unwrap();
        Self::drop_stale_reservations(&mut pending, now_ms);
        let court_taken = pending.get(&court_id).is_some_and(|(m, _)| *m != match_id);
        let match_taken = pending
            .iter()
            .any(|(c, (m, _))| *m == match_id && *c != court_id);
        if court_taken || match_taken {
            return false;
        }
        pending.insert(court_id, (match_id, now_ms));
        true
    }

    /// Hält die automatische Feldvergabe an oder gibt sie wieder frei.
    ///
    /// Der Schalter lebt **nur zur Laufzeit** (die Einstellungen bleiben
    /// unangetastet) und gilt bis zum Stoppen der Übertragung — siehe
    /// [`Self::reset_runtime_switches`].
    pub fn set_auto_assign_paused(&self, paused: bool) {
        *self.auto_assign_paused.write().unwrap() = paused;
    }

    /// Ist die automatische Feldvergabe gerade angehalten?
    pub fn auto_assign_paused(&self) -> bool {
        *self.auto_assign_paused.read().unwrap()
    }

    /// Hält einen Nachruf am Meeting Point fest und liefert seine Stufe.
    ///
    /// Je Spiel **und Partei**: Die eine kann längst da sein, während die
    /// andere fehlt. Der erste Nachruf ist der zweite Aufruf, jeder weitere
    /// der dritte und letzte — dieselbe Staffelung wie in der
    /// Desktop-Oberfläche, aber an einer Stelle gezählt, damit beide Wege
    /// dieselbe Ansage erzeugen.
    pub fn note_prep_call(&self, match_id: i64, side: &str) -> u8 {
        let mut g = self.prep_call_stages.write().unwrap();
        // Beschränken: Vorbereitungs-Aufrufe werden zwar aufgeräumt, wenn der
        // Aufruf zurückgenommen wird, aber ein langes Turnier soll die Liste
        // auch dann nicht unbegrenzt wachsen lassen.
        if g.len() > 500 {
            g.clear();
        }
        let entry = g.entry((match_id, side.to_string())).or_insert(1);
        *entry = (*entry + 1).min(3);
        *entry
    }

    /// Höchste bisher gesprochene Nachruf-Stufe eines Spiels (0 = keiner).
    ///
    /// Die Turnierleitungs-Seite braucht sie, um einen Doppeltipp von einem
    /// bewussten zweiten Nachruf zu unterscheiden.
    pub fn prep_calls_made(&self, match_id: i64) -> u8 {
        self.prep_call_stages
            .read()
            .unwrap()
            .iter()
            .filter(|((mid, _), _)| *mid == match_id)
            .map(|(_, stage)| *stage)
            .max()
            .unwrap_or(0)
    }

    /// Legt die Datei fest, in der die von Hand gesetzten Spielorte liegen,
    /// und liest sie ein.
    ///
    /// Getrennt vom Konstruktor, weil `TabletState` sein Datenverzeichnis
    /// nicht kennt — es kommt beim Start der Übertragung vom Tauri-Handle.
    /// Fehlt die Datei oder ist sie unlesbar, bleibt es leer (kein Fehler);
    /// beim ersten Setzen entsteht sie neu.
    pub fn use_manual_hall_file(&self, path: &std::path::Path) {
        let geladen: HashMap<i64, String> = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        *self.manual_halls.write().unwrap() = geladen;
        let _ = self.manual_halls_path.set(path.to_path_buf());
    }

    /// Schreibt die Spielorte, falls eine Datei dafür bekannt ist.
    fn save_manual_halls(&self, halls: &HashMap<i64, String>) {
        let Some(path) = self.manual_halls_path.get() else {
            return;
        };
        if let Ok(text) = serde_json::to_string(halls) {
            // Ein Schreibfehler darf die Aktion nicht scheitern lassen: Der
            // Ort gilt zur Laufzeit, nur der Neustart verlöre ihn.
            let _ = std::fs::write(path, text);
        }
    }

    /// Gibt einem Spiel von Hand eine Halle; leerer Name nimmt sie zurück.
    ///
    /// Gibt zurück, ob sich etwas geändert hat — der Anzeige-Zustand soll nur
    /// dann eine neue Revision bekommen, wenn wirklich etwas anders ist.
    pub fn set_manual_hall(&self, match_id: i64, hall: &str) -> bool {
        let hall = hall.trim();
        let mut g = self.manual_halls.write().unwrap();
        let geaendert = if hall.is_empty() {
            g.remove(&match_id).is_some()
        } else {
            // Deckel gegen unbegrenztes Wachsen über ein langes Turnier. 2000
            // Einträge sind mehr als jedes Turnier an Spielen hat.
            if g.len() > 2000 {
                g.clear();
            }
            g.insert(match_id, hall.to_string()).as_deref() != Some(hall)
        };
        if geaendert {
            self.save_manual_halls(&g);
        }
        geaendert
    }

    /// Die von Hand gesetzte Halle eines Spiels, falls es eine gibt.
    pub fn manual_hall(&self, match_id: i64) -> Option<String> {
        self.manual_halls.read().unwrap().get(&match_id).cloned()
    }

    /// Alle Handzuweisungen — für den Aufbau des Anzeige-Zustands, damit
    /// nicht je Spiel einzeln gesperrt werden muss.
    pub fn manual_halls(&self) -> HashMap<i64, String> {
        self.manual_halls.read().unwrap().clone()
    }

    /// Vergisst die Nachruf-Stufen eines Spiels — beim Zurücknehmen des
    /// Vorbereitungs-Aufrufs, damit ein erneuter Aufruf wieder von vorn
    /// beginnt.
    pub fn forget_prep_calls(&self, match_id: i64) {
        self.prep_call_stages
            .write()
            .unwrap()
            .retain(|(mid, _), _| *mid != match_id);
    }

    /// Kennung **dieses Programmlaufs** — Teil der Fassungs-Marke des
    /// Anzeige-Zustands.
    ///
    /// Die Revision beginnt nach einem Neustart der App wieder bei 1. Ein
    /// Gerät mit gemerkter Fassung „1" bekäme sonst „unverändert" auf einen
    /// völlig anderen Turnierstand und arbeitete auf einem Plan von vorhin.
    ///
    /// Bewusst **kein Feld**: `TabletState` wird über `Default` erzeugt, und
    /// eine dort genullte Kennung wäre über Neustarts hinweg dieselbe —
    /// genau das, was sie verhindern soll.
    pub fn process_tag(&self) -> u64 {
        static TAG: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        *TAG.get_or_init(now_ms)
    }

    /// Die Revision des Anzeige-Zustands zu diesem Fingerabdruck.
    ///
    /// Zählt **nur** hoch, wenn sich der Fingerabdruck geändert hat. Damit
    /// erkennt ein abrufendes Gerät „nichts Neues" und der Turnier-PC einen
    /// Tipp, der auf einem überholten Stand beruht. Die Zählung lebt hier,
    /// weil LAN-Server und Relay-Weg dieselbe Zahl meinen müssen.
    pub fn tl_revision(&self, fingerprint: &str) -> u64 {
        let mut g = self.tl_state_rev.write().unwrap();
        if g.1 != fingerprint {
            g.0 += 1;
            g.1 = fingerprint.to_string();
        }
        g.0
    }

    /// Sind auf diesem Feld schon Punkte gefallen?
    ///
    /// Antwort auf die Frage „sind die Spieler da?" — und damit der Riegel
    /// vor dem Aufruf-Timer: Wird gespielt, ist er gegenstandslos. Der Stand
    /// des Zähltabletts zählt, weil er dem BTP-Stand voraus ist.
    pub fn points_scored(&self, court_id: i64, match_id: i64) -> bool {
        self.courts
            .read()
            .unwrap()
            .get(&court_id)
            .filter(|s| s.match_id == match_id)
            .is_some_and(|s| s.sets.iter().any(|(a, b)| *a > 0 || *b > 0))
    }

    /// Wie viele Aufrufe für dieses Spiel schon **gesprochen** wurden
    /// (0 = noch keiner; nach oben offen, siehe „Aufrufe unbegrenzt").
    ///
    /// `0` heißt: noch keiner. Das ist nicht dasselbe wie „Stufe 1" — auf dem
    /// Feld zu stehen ist noch kein gesprochener Aufruf, und der erste Druck
    /// auf „Aufrufen" in der Desktop-Übersicht ist genau dieser schlichte
    /// erste. Diese Unterscheidung ist der Grund, warum hier Aufrufe gezählt
    /// werden und keine Stufen: Sonst liefe die gemeinsame Zählung der
    /// Desktop-Oberfläche dauerhaft um eins voraus.
    ///
    /// Steht auf dem Feld inzwischen ein anderes Spiel, gilt die alte Zählung
    /// nicht mehr.
    pub fn calls_made(&self, court_id: i64, match_id: i64) -> u8 {
        match self.call_stages.read().unwrap().get(&court_id) {
            Some((known, made, _)) if *known == match_id => *made,
            _ => 0,
        }
    }

    /// Hält einen **erneuten** Aufruf fest und liefert die gesprochene Stufe.
    ///
    /// Für die Turnierleitungs-Seite, die nur „noch einmal" weiß. Sie zeigt
    /// als erfolgte Stufe `max(1, calls_made)` und bietet die nächste an —
    /// die Rechnung hier muss dazu passen. `at_least` hebt auf die zeitlich
    /// fällige Stufe an.
    ///
    /// `unlimited` (Option „Aufrufe unbegrenzt", Feldtest 17.08.2026):
    /// `true` lässt den Zähler über den dritten Aufruf hinaus ehrlich
    /// weiterlaufen (4, 5, …). `false` behält den alten Deckel bei 3 —
    /// host-seitig, damit ein Turnier ohne die Option sich nicht allein
    /// auf das Client-Gating verlassen muss (Review 17.08.2026);
    /// zurückgedreht wird dabei trotzdem nie.
    ///
    /// `sides` ist die Parteien-Maske des Aufrufs
    /// ([`SIDE_BOTH`]/[`SIDE_TEAM1`]/[`SIDE_TEAM2`]). Die Stufe steigt nur,
    /// wenn eine der gerufenen Parteien auf der aktuellen Stufe **schon
    /// einmal** gerufen wurde — „Partei A rufen, dann Partei B rufen" ist
    /// damit eine Runde und zählt einmal, „zweimal Partei A" sind zwei
    /// (Spec tl-liste-vereinfachen E1).
    pub fn note_court_call_at_least(
        &self,
        court_id: i64,
        match_id: i64,
        at_least: u8,
        sides: u8,
        unlimited: bool,
    ) -> u8 {
        let mut g = self.call_stages.write().unwrap();
        let entry = g.entry(court_id).or_insert((match_id, 0, 0));
        // Anderes Spiel auf dem Feld: von vorn, sonst erbte es die Aufrufe
        // seines Vorgängers und stünde sofort als dritter Aufruf da.
        if entry.0 != match_id {
            *entry = (match_id, 0, 0);
        }
        let vorher = entry.1;
        // Neue Runde, sobald sich die gerufenen Parteien mit den auf dieser
        // Stufe bereits gerufenen überschneiden — oder wenn überhaupt noch
        // nichts gerufen wurde (dann ist dieser Aufruf der zweite, denn auf
        // dem Feld zu stehen war der erste). Ab Stufe 4 spricht das
        // Ansage-Gerät die schlichte Feld-Ansage ohne Stufenwort
        // (`AnnounceJobPlayer`).
        if entry.1 == 0 || entry.2 & sides != 0 {
            entry.1 = entry.1.max(1).saturating_add(1);
            entry.2 = 0;
        }
        entry.1 = entry.1.max(at_least);
        if !unlimited {
            // Ohne die Option gilt der alte Deckel bei 3 — aber nie unter
            // einen Stand zurück, den ein Gerät MIT der Option schon
            // erreicht hat (zwei Geräte, verschiedene Profile).
            entry.1 = entry.1.min(vorher.max(3));
        }
        entry.2 |= sides;
        entry.1
    }

    /// Wie [`Self::note_court_call_at_least`] ohne zeitliche Untergrenze,
    /// für beide Parteien.
    pub fn note_court_call(&self, court_id: i64, match_id: i64, unlimited: bool) -> u8 {
        self.note_court_call_at_least(court_id, match_id, 0, SIDE_BOTH, unlimited)
    }

    /// Hält fest, dass eine bestimmte Stufe **gesprochen wurde**.
    ///
    /// Der Gegenpart zu [`Self::note_court_call_at_least`]: Dort weiß der
    /// Aufrufer nur „noch einmal", hier weiß er genau, was in der Halle
    /// erklang. Die Desktop-Übersicht rechnet ihre Stufe selbst aus — würde
    /// sie stattdessen hochzählen lassen, liefe die gemeinsame Zählung ihr
    /// dauerhaft um eins voraus.
    ///
    /// Der Desktop-Aufruf gilt **immer beiden Parteien** — die Runde ist
    /// damit voll, ein anschließender Partei-Aufruf von der TL-Seite
    /// eröffnet also die nächste Stufe.
    ///
    /// Zurückgedreht wird nie: Zwei Geräte können sich überholen.
    pub fn reached_court_call(&self, court_id: i64, match_id: i64, stage: u8) -> u8 {
        let mut g = self.call_stages.write().unwrap();
        let entry = g.entry(court_id).or_insert((match_id, 0, 0));
        if entry.0 != match_id {
            *entry = (match_id, 0, 0);
        }
        // Kein Deckel bei 3: Steht der geteilte Zähler durch „Aufrufe
        // unbegrenzt" schon höher, drückte `min(3)` ihn zurück — genau das
        // verbietet der Satz unten.
        entry.1 = entry.1.max(stage);
        entry.2 = SIDE_BOTH;
        entry.1
    }

    /// Setzt die Schalter zurück, die nur zur Laufzeit gelten — beim Start
    /// der Übertragung aufgerufen.
    ///
    /// Ohne das bliebe eine auf der Turnierleitungs-Seite gesetzte Pause der
    /// automatischen Vergabe hängen, sobald das Gerät nicht mehr erreichbar
    /// ist: Die Einstellungen sagen „an", die Vergabe läuft trotzdem nicht,
    /// und es gibt keinen Griff, das zu ändern. Stoppen und Starten ist
    /// dieser Griff.
    pub fn reset_runtime_switches(&self) {
        self.set_auto_assign_paused(false);
    }

    /// Gibt den Anspruch auf ein Feld sofort wieder frei.
    ///
    /// Nötig, wenn der Schreibvorgang fehlschlägt oder das Feld gleich wieder
    /// geräumt wird — sonst bliebe es bis zum Ablauf der Frist blockiert, und
    /// wer seine eigene Zuweisung zurücknimmt, käme an sein Feld nicht mehr
    /// heran.
    pub fn release_court_claim(&self, court_id: i64) {
        self.pending_assign.write().unwrap().remove(&court_id);
    }

    /// Die noch gültigen Reservierungen als `(Feld, Spiel)`.
    pub fn reserved_courts(&self, now_ms: u64) -> Vec<(i64, i64)> {
        let mut pending = self.pending_assign.write().unwrap();
        Self::drop_stale_reservations(&mut pending, now_ms);
        let mut out: Vec<(i64, i64)> = pending.iter().map(|(c, (m, _))| (*c, *m)).collect();
        out.sort_unstable();
        out
    }

    /// Wirft abgelaufene Reservierungen weg.
    ///
    /// Abgelaufen ist auch, was in der **Zukunft** liegt: Eine rückwärts
    /// gestellte Uhr (Zeitumstellung, Zeitabgleich) machte sonst aus jeder
    /// Reservierung eine ewige, weil die Differenz auf null gesättigt würde.
    fn drop_stale_reservations(pending: &mut HashMap<i64, (i64, u64)>, now_ms: u64) {
        pending.retain(|_, (_, ts)| *ts <= now_ms && now_ms - *ts < RESERVATION_TTL_MS);
    }

    /// Räumt Reservierungen weg, die der BTP-Stand inzwischen bestätigt hat.
    ///
    /// Wird im Abfrage-Takt aufgerufen: Sobald das Spiel wirklich auf dem
    /// Feld steht, hat die Reservierung ihren Zweck erfüllt.
    pub fn release_confirmed_reservations(&self) {
        let guard = self.snapshot.read().unwrap();
        let Some(snap) = guard.as_ref() else { return };
        self.pending_assign
            .write()
            .unwrap()
            .retain(|court, (m, _)| {
                !snap
                    .matches
                    .iter()
                    .any(|x| x.id == *m && x.court_id == Some(*court))
            });
    }

    /// Antwort eines schon ausgeführten Vorgangs, falls noch bekannt.
    /// `fingerprint` beschreibt die Aktion. Er wird mitgeprüft, damit eine
    /// zufällig oder böswillig wiederverwendete Kennung nicht die
    /// Erfolgsmeldung eines ganz anderen Vorgangs bekommt — die zweite
    /// Aktion würde sonst nie ausgeführt und trotzdem als erledigt gemeldet.
    pub fn remembered_result(
        &self,
        op_id: &str,
        fingerprint: &str,
        now_ms: u64,
    ) -> Option<relay_proto::TlResponse> {
        let mut ops = self.recent_ops.write().unwrap();
        Self::drop_stale_ops(&mut ops, now_ms);
        ops.get(op_id)
            .filter(|(_, fp, _)| fp == fingerprint)
            .map(|(_, _, resp)| resp.clone())
    }

    /// Hält das Ergebnis eines Vorgangs fest, damit eine Wiederholung
    /// dieselbe Antwort bekommt, statt erneut zu schreiben.
    pub fn remember_result(
        &self,
        op_id: &str,
        fingerprint: &str,
        response: relay_proto::TlResponse,
        now_ms: u64,
    ) {
        let key = op_id.trim();
        // Leere und übermäßig lange Kennungen gar nicht erst behalten: Der
        // Wert kommt von außen, und der Turnier-PC soll sich davon nicht den
        // Arbeitsspeicher füllen lassen.
        if key.is_empty() || key.len() > MAX_OP_ID_LEN {
            return;
        }
        let mut ops = self.recent_ops.write().unwrap();
        Self::drop_stale_ops(&mut ops, now_ms);
        if ops.len() >= MAX_REMEMBERED_OPS && !ops.contains_key(key) {
            // Voll: den ältesten Eintrag weichen lassen. Die Erinnerung ist
            // eine Bequemlichkeit gegen Doppeltipps, kein Gedächtnis, für das
            // es sich zu wachsen lohnte.
            if let Some(oldest) = ops
                .iter()
                .min_by_key(|(_, (ts, _, _))| *ts)
                .map(|(k, _)| k.clone())
            {
                ops.remove(&oldest);
            }
        }
        ops.insert(key.to_string(), (now_ms, fingerprint.to_string(), response));
    }

    /// Wie viele Vorgänge gerade erinnert werden (Tests und Diagnose).
    pub fn remembered_op_count(&self) -> usize {
        self.recent_ops.read().unwrap().len()
    }

    fn drop_stale_ops(
        ops: &mut HashMap<String, (u64, String, relay_proto::TlResponse)>,
        now_ms: u64,
    ) {
        // Wie bei den Reservierungen gilt auch ein Zeitstempel aus der
        // Zukunft als abgelaufen (rückwärts gestellte Uhr).
        ops.retain(|_, (ts, _, _)| *ts <= now_ms && now_ms - *ts < OP_MEMORY_MS);
    }

    /// Zieht den laufenden Spielstand eines Spiels auf ein anderes Feld um.
    ///
    /// Nötig, wenn die Turnierleitung ein **laufendes** Spiel umhängt: Der
    /// Stand hängt am Feld, nicht am Spiel. Ohne den Umzug zeigte das neue
    /// Feld 0:0 und das alte den stehengebliebenen Stand — auf dem
    /// Court-Monitor wie im Liveticker, und ein neu verbundenes Zähltablett
    /// finge bei null an.
    ///
    /// Das Tablet selbst wandert **nicht** mit: Es bleibt an seinem Feld.
    /// Übertragen wird nur, was zum Spiel gehört (Satzstand und gespiegelter
    /// Spielzustand), nicht der Gerätezustand (Verbindung, Akku, Meldungen).
    pub fn move_match_score(&self, from_court_id: i64, to_court_id: i64, match_id: i64) {
        {
            let mut courts = self.courts.write().unwrap();
            // Nur umziehen, wenn das Quellfeld wirklich dieses Spiel zählt —
            // sonst überschriebe ein verspäteter Aufruf einen fremden Stand.
            let Some(sets) = courts
                .get(&from_court_id)
                .filter(|s| s.match_id == match_id)
                .map(|s| s.sets.clone())
            else {
                return;
            };
            let target = courts.entry(to_court_id).or_insert(CourtSession {
                match_id,
                sets: Vec::new(),
                connected: false,
                battery: None,
                injury: false,
                official: false,
            });
            target.match_id = match_id;
            target.sets = sets;
            // Das Quellfeld verliert Spiel und Stand, behält aber sein
            // Tablet (Verbindung, Akku) — dort steht ja weiterhin ein Gerät.
            if let Some(src) = courts.get_mut(&from_court_id) {
                src.match_id = 0;
                src.sets = Vec::new();
                src.injury = false;
                src.official = false;
            }
        }
        // Den gespiegelten Spielzustand (Aufschlag, Pause) mitnehmen, damit
        // ein Tablett am neuen Feld dort weiterzählen kann, wo aufgehört
        // wurde.
        let mirrored = self.court_state.write().unwrap().remove(&from_court_id);
        if let Some(state) = mirrored {
            self.court_state.write().unwrap().insert(to_court_id, state);
        }
        self.persist_scores();
        // Der Stand wandert von Quell- auf Zielfeld — BEIDE Anzeigen sind
        // betroffen (Quellfeld wird leer, Zielfeld zeigt den Stand). Sonst
        // hinge jeder TV bis zu seinem nächsten Poll (A1, ADR 0016). Erst hier,
        // nachdem alle Schreib-Guards gefallen sind (wie `record_score`).
        self.notify_monitor(from_court_id);
        self.notify_monitor(to_court_id);
    }

    pub fn clear_court(&self, court_id: i64) {
        self.courts.write().unwrap().remove(&court_id);
        // Gespiegelten Spielstand löschen, sonst bekäme ein nach dem Ergebnis
        // neu/ersatzweise verbundenes Tablet via StateRestore kurz den BEENDETEN
        // Stand (Render-Blitz, im schmalen Fenster sogar Doppel-Submit). Wird nur
        // nach Ergebnis-Submit aufgerufen (nicht beim Disconnect), daher bleibt
        // der Crash-Restore eines laufenden Spiels unberührt.
        self.court_state.write().unwrap().remove(&court_id);
        // Eine noch offene Vormerkung gehört zum Spiel, das hier gerade
        // beendet wurde. Bliebe sie stehen, wies die nächste Zuweisung auf
        // dieses Feld mit „hat gerade jemand anderes belegt" ab — obwohl es
        // sichtbar leer ist.
        self.release_court_claim(court_id);
        // Entfernten Stand auch aus der Datei nehmen.
        self.persist_scores();
        // Match-Räumung ist am Monitor sichtbar (Feld wird leer) → anstoßen.
        self.notify_monitor(court_id);
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

    /// Nimmt einen Vorschlag **beanspruchend** heraus: Nur der erste Aufruf
    /// bekommt ihn, jeder weitere geht leer aus.
    ///
    /// Das ist der Anspruch für die kampflose Wertung. Zwei gleichzeitig
    /// tippende Turnierleitungs-Geräte schrieben sonst beide dieselben
    /// Wertungen nach BTP. Ging danach gar nichts durch, legt der Aufrufer
    /// ihn mit `add_walkover_proposal` zurück.
    pub fn take_walkover_proposal(&self, id: &str) -> Option<WalkoverProposal> {
        let mut list = self.walkovers.write().unwrap();
        let pos = list.iter().position(|p| p.id == id)?;
        Some(list.remove(pos))
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
        drop(calls);

        // Von Hand gesetzte Spielorte genauso einstempeln — **ohne**
        // Aufruf-Zeitstempel: Einen Ort festzulegen ist kein Aufruf, sonst
        // meldete der Liveticker „vor X Min gerufen" für ein Spiel, nach dem
        // niemand gerufen hat. Ein echter Aufruf für dieselbe Partie hat
        // Vorrang; er hat die Halle oben schon gesetzt.
        //
        // Damit erreicht die Angabe den Hallenfilter des Livetickers
        // (`display=next&halle=…`) — bis hierher blieb er leer, sobald ein
        // Turnier seine Aufrufe über BTP statt über bts-light machte.
        let manual = self.manual_halls.read().unwrap();
        for (match_id, hall) in manual.iter() {
            let Some(m) = snapshot.matches.iter_mut().find(|m| m.id == *match_id) else {
                continue;
            };
            if m.preparation_hall.is_some() || m.status != MatchStatus::Scheduled {
                continue;
            }
            // In BTPs Schreibweise, damit der Filter greift.
            let name = snapshot
                .locations
                .iter()
                .find(|l| l.name.trim().eq_ignore_ascii_case(hall.trim()))
                .map(|l| l.name.trim().to_string())
                .unwrap_or_else(|| hall.trim().to_string());
            m.preparation_hall = Some(name);
        }
        drop(manual);

        // Automatisch vorverteilte Hallen als DRITTE Stufe einstempeln
        // (Spec `hallen-vorverteilung`, E7) — gleiche Regeln wie die
        // Hand-Hallen darüber (kein Aufruf-Zeitstempel, nur Scheduled,
        // BTP-Schreibweise), und durch die Block-Reihenfolge gilt der
        // Vorrang Aufruf > Hand > Auto von selbst. Damit sehen Spieler
        // ihre Halle früh im Liveticker (`display=next&halle=…`) und auf
        // den Hallen-Monitoren — genau der Zweck der Vorverteilung.
        for (match_id, hall) in self.auto_halls.halls() {
            let Some(m) = snapshot.matches.iter_mut().find(|m| m.id == match_id) else {
                continue;
            };
            if m.preparation_hall.is_some() || m.status != MatchStatus::Scheduled {
                continue;
            }
            let name = snapshot
                .locations
                .iter()
                .find(|l| l.name.trim().eq_ignore_ascii_case(hall.trim()))
                .map(|l| l.name.trim().to_string())
                .unwrap_or_else(|| hall.trim().to_string());
            m.preparation_hall = Some(name);
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

    /// Legt einen Ansage-Auftrag ab und liefert seine laufende Nummer.
    ///
    /// Die Nummer wird — wie beim Freitext — mindestens auf die aktuelle
    /// Uhrzeit gehoben. Sonst begänne sie nach einem Neustart des Turnier-PCs
    /// wieder klein, und ein Gerät mit gemerkter Nummer verstummte, bis sie
    /// seinen Stand wieder überholt.
    pub fn publish_announce_job(&self, hall: String, kind: AnnounceJobKind, now_ms: u64) -> u64 {
        let hall: String = hall.chars().take(128).collect();
        self.announce_seq.fetch_max(now_ms, Ordering::Relaxed);
        let id = self.announce_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let mut g = self.announce_jobs.write().unwrap();
        g.push(AnnounceJob {
            id,
            hall,
            created_at_ms: now_ms,
            kind,
        });
        // Verfallene und überzählige Aufträge gleich hier wegräumen — sie
        // werden nie wieder gesprochen und müssen nicht mitwachsen.
        g.retain(|j| j.created_at_ms + ANNOUNCE_JOB_TTL_MS > now_ms);
        let len = g.len();
        if len > 50 {
            g.drain(0..len - 50);
        }
        id
    }

    /// Aufträge mit `id > since` für `hall`, die noch nicht verfallen sind.
    ///
    /// Eine leere Geräte-Halle bekommt ALLE (Einzelhallen-Betrieb); sonst die
    /// an „alle" oder an genau diese Halle gerichteten — dieselbe Regel wie
    /// beim Freitext.
    ///
    /// Nebenbei zählt der Abruf als Lebenszeichen der Halle: Wer abholt, ist
    /// ein Ansage-Gerät. Nur so kann die Turnierleitung erfahren, dass ihr
    /// Aufruf nirgends erklingt.
    pub fn announce_jobs_since(&self, hall: &str, since: u64, now_ms: u64) -> Vec<AnnounceJob> {
        let h = hall.trim();
        {
            // Die Abhol-Route steht im Hallennetz offen und der Hallenname
            // kommt ungeprüft aus der Anfrage. Deshalb hier kappen, Abgelaufenes
            // wegräumen und die Liste beschränken: Ein fehlkonfiguriertes
            // Gerät soll den Turnier-PC nicht mit erfundenen Hallennamen
            // volllaufen lassen — mit ihm stürbe auch die BTP-Übertragung.
            let key: String = h.to_lowercase().chars().take(128).collect();
            let mut g = self.announce_listeners.write().unwrap();
            g.retain(|_, seen| *seen + ANNOUNCE_LISTENER_TTL_MS > now_ms);
            if g.len() < MAX_ANNOUNCE_LISTENERS || g.contains_key(&key) {
                g.insert(key, now_ms);
            }
        }
        self.announce_jobs
            .read()
            .unwrap()
            .iter()
            .filter(|j| j.id > since)
            .filter(|j| j.created_at_ms + ANNOUNCE_JOB_TTL_MS > now_ms)
            .filter(|j| {
                let target = j.hall.trim();
                h.is_empty() || target.is_empty() || target.eq_ignore_ascii_case(h)
            })
            .cloned()
            .collect()
    }

    /// Hört in dieser Halle gerade ein Ansage-Gerät zu?
    ///
    /// **Spiegelbild der Zustellung** in [`Self::announce_jobs_since`]: Ein
    /// Gerät ohne eingestellte Halle bekommt alles, und ein Auftrag ohne
    /// Halle geht an alle. Beides muss hier gelten — sonst meldete die Seite
    /// „kein Ansage-Gerät verbunden", während der Aufruf gerade erklingt.
    ///
    /// Die Frist ist großzügig gegenüber dem Abfragetakt: Ein einzelner
    /// ausgefallener Abruf soll nicht als „niemand da" gelten.
    pub fn has_announce_listener(&self, hall: &str, now_ms: u64) -> bool {
        let h = hall.trim().to_lowercase();
        self.announce_listeners
            .read()
            .unwrap()
            .iter()
            .any(|(known, seen)| {
                (h.is_empty() || known.is_empty() || *known == h)
                    && seen + ANNOUNCE_LISTENER_TTL_MS > now_ms
            })
    }

    /// Wie viele Ansage-Geräte die Liste gerade führt (nur für Tests).
    #[cfg(test)]
    pub fn announce_listener_count(&self) -> usize {
        self.announce_listeners.read().unwrap().len()
    }

    /// Längster Hallen-Schlüssel in der Zuhörer-Liste (nur für Tests).
    #[cfg(test)]
    pub fn longest_announce_listener_key(&self) -> usize {
        self.announce_listeners
            .read()
            .unwrap()
            .keys()
            .map(|k| k.chars().count())
            .max()
            .unwrap_or(0)
    }

    pub fn overview(&self) -> Vec<CourtOverview> {
        let guard = self.snapshot.read().unwrap();
        let Some(snap) = guard.as_ref() else {
            return Vec::new();
        };
        self.overview_from(snap)
    }

    /// Wie [`Self::overview`], aber auf einem **übergebenen** Schnappschuss.
    ///
    /// Für Aufrufer, die mehrere Sichten aus demselben BTP-Stand bauen: Zwei
    /// getrennte Lesevorgänge könnten den Sync-Lauf dazwischen erwischen, und
    /// die Teilsichten beschrieben dann verschiedene Turnierstände.
    pub fn overview_from(&self, snap: &BtpSnapshot) -> Vec<CourtOverview> {
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
                let clubs = |team: &[crate::btp::model::BtpPlayer]| {
                    team.iter()
                        .map(|p| p.club.clone().unwrap_or_default())
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
                let (sr_names, ar_names, official_warn) = self.court_officials(m, snap);
                let official_ids = self.court_official_ids(m);
                CourtOverview {
                    court_id: court.id,
                    court: court.name.clone(),
                    // Hallenname nur bei Mehr-Hallen-Turnieren; sonst leer.
                    location: snap.court_location_name(court.id),
                    // Farbe füllt `hall_colors::paint` an den Serving-Stellen
                    // nach — hier fehlt die Config.
                    hall_color: None,
                    has_timeline: m.is_some_and(|mm| self.timeline.has_timeline(mm.id)),
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
                    team1_clubs: m.map(|mm| clubs(&mm.team1)).unwrap_or_default(),
                    team2_clubs: m.map(|mm| clubs(&mm.team2)).unwrap_or_default(),
                    sets,
                    tablet_connected,
                    // Aufruf-Stufe aus der gemeinsamen Zählung: Beide
                    // Oberflächen sollen dieselbe Zahl anzeigen und beim
                    // nächsten Aufruf dieselbe Stufe ansagen.
                    call_stage: self.calls_made(court.id, m.map(|mm| mm.id).unwrap_or(0)),
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
                    // Schiedsrichter/Aufschlagrichter des laufenden Spiels
                    // (Spec schiedsrichter-management Nr. 7). BTP gewinnt
                    // gegen die lokale Zuweisung; ohne SR-Betrieb bleibt
                    // alles leer.
                    sr: sr_names,
                    ar: ar_names,
                    official_warn,
                    sr_id: official_ids.0,
                    ar_id: official_ids.1,
                }
            })
            .collect()
    }

    /// Monitor-relevante Daten eines Feldes: das aktuelle Match mit
    /// effektivem Satzstand (Tablet-getrieben falls aktiv, sonst aus BTP)
    /// und der gespiegelte Tablet-Spielzustand (Aufschlag/Pause). Vom
    /// Court-Monitor-Endpunkt genutzt.
    /// Schlanker Blick fürs Score-Spiegeln zum Relay (v0.9.200) — dieselbe
    /// Auswahl wie [`Self::monitor_court`] (Tablet-Stand vor BTP-Stand, ohne
    /// `connected`-Prüfung), aber ohne das Klonen des vollen `BtpMatch`: Der
    /// Spiegel läuft bei jedem Nudge und im 2-s-Sweep über alle Felder.
    /// `None` = kein Match auf dem Feld, nichts zu spiegeln.
    pub fn score_mirror_of(&self, court_id: i64) -> Option<ScoreMirror> {
        let (match_id, btp_sets) = {
            let guard = self.snapshot.read().unwrap();
            let m = guard.as_ref().and_then(|snap| {
                snap.matches
                    .iter()
                    .find(|m| m.status == MatchStatus::OnCourt && m.court_id == Some(court_id))
            })?;
            (m.id, m.sets.clone())
        };
        let sets = {
            let courts = self.courts.read().unwrap();
            match courts.get(&court_id) {
                Some(s) if s.match_id == match_id && !s.sets.is_empty() => s.sets.clone(),
                _ => btp_sets,
            }
        };
        Some(ScoreMirror {
            match_id,
            sets,
            state: self.court_state(court_id),
        })
    }

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
/// Spiegel-Stand eines Felds fürs Relay ([`TabletState::score_mirror_of`]):
/// Match, effektiver Satzstand und roher Tablet-Spielzustand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreMirror {
    pub match_id: i64,
    pub sets: Vec<(i64, i64)>,
    /// Gespiegelter Tablet-Spielzustand (JSON-String), falls vorhanden.
    pub state: Option<String>,
}

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
            team1: vec![player("Anna")],
            team2: vec![player("Ben")],
            entry1_id: 10,
            entry2_id: 20,
            // Court-Name spielt für die Identität keine Rolle – die
            // CourtID ist maßgeblich. Wir setzen einen Platzhalter-Namen.
            court: court.map(|cid| format!("C{cid}")),
            court_id: court,
            location_id: None,
            sets: vec![(5, 3)],
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
            officials: Vec::new(),
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
    fn a_fresh_assignment_reserves_the_court_until_btp_confirms() {
        // Zwischen dem Schreiben nach BTP und der Rückmeldung sieht der
        // Schnappschuss das Feld noch leer. Ohne Reservierung ließe die
        // Prüfung eine zweite Zuweisung durch, und die Spieler der ersten
        // stünden vor einem Feld, auf dem ein fremdes Spiel läuft.
        let st = TabletState::default();
        assert!(st.try_reserve_court(3, 42, 1_000));
        assert_eq!(st.reserved_courts(1_000), vec![(3, 42)]);
        // Kurz danach gilt sie noch …
        assert_eq!(st.reserved_courts(5_000), vec![(3, 42)]);
    }

    #[test]
    fn a_second_claim_on_the_same_court_loses() {
        // Der eigentliche Zweck: Zwei Geräte tippen im selben Moment
        // dasselbe Feld an. Genau eines darf gewinnen — und die
        // Entscheidung muss VOR dem Schreiben nach BTP fallen, sonst läuft
        // sie ins Leere, solange BTP antwortet.
        let st = TabletState::default();
        assert!(st.try_reserve_court(3, 42, 1_000));
        assert!(
            !st.try_reserve_court(3, 77, 1_100),
            "das Feld ist schon vergeben"
        );
        // Derselbe Vorgang noch einmal ist dagegen in Ordnung.
        assert!(st.try_reserve_court(3, 42, 1_200));
    }

    #[test]
    fn the_same_match_cannot_be_claimed_for_two_courts() {
        // Sonst stünde dasselbe Spiel in BTP auf zwei Feldern: Eines davon
        // bliebe dauerhaft mit einem Geisterspiel belegt.
        let st = TabletState::default();
        assert!(st.try_reserve_court(3, 42, 1_000));
        assert!(
            !st.try_reserve_court(5, 42, 1_100),
            "das Spiel ist schon einem Feld zugesagt"
        );
    }

    #[test]
    fn a_failed_write_releases_its_claim_at_once() {
        // Schlägt der Schreibvorgang fehl, darf das Feld nicht bis zum
        // Ablauf der Frist blockiert bleiben — der nächste Versuch soll
        // sofort möglich sein.
        let st = TabletState::default();
        assert!(st.try_reserve_court(3, 42, 1_000));
        st.release_court_claim(3);
        assert!(st.reserved_courts(1_100).is_empty());
        assert!(st.try_reserve_court(3, 77, 1_200), "Feld ist wieder frei");
    }

    #[test]
    fn a_clock_jumping_backwards_does_not_freeze_a_reservation() {
        // Zeitumstellung oder Zeitabgleich können die Uhr zurückstellen.
        // Ein Zeitstempel aus der „Zukunft" darf keine ewige Reservierung
        // ergeben.
        let st = TabletState::default();
        st.try_reserve_court(3, 42, 10_000);
        assert!(
            st.reserved_courts(1_000).is_empty(),
            "Zeitstempel aus der Zukunft wird verworfen"
        );
    }

    #[test]
    fn a_reservation_expires_so_a_failed_write_does_not_block_the_court() {
        // Schlägt der Schreibvorgang fehl, bestätigt BTP nie — die
        // Reservierung muss von selbst verfallen, sonst wäre das Feld
        // dauerhaft blockiert.
        let st = TabletState::default();
        st.try_reserve_court(3, 42, 1_000);
        let after_ttl = 1_000 + RESERVATION_TTL_MS + 1;
        assert!(st.reserved_courts(after_ttl).is_empty());
    }

    #[test]
    fn a_reservation_is_released_once_btp_reports_the_match_on_the_court() {
        // Sobald der Schnappschuss die Zuweisung zeigt, ist die Reservierung
        // überflüssig — sie darf das Feld nicht länger blockieren, als nötig.
        let st = TabletState::default();
        st.try_reserve_court(3, 42, 1_000);
        st.set_snapshot(snapshot(
            vec![match_on(42, Some(3), MatchStatus::OnCourt)],
            vec![(3, "Feld 3")],
        ));
        st.release_confirmed_reservations();
        assert!(st.reserved_courts(1_500).is_empty());
    }

    #[test]
    fn repeating_an_operation_returns_the_stored_answer_instead_of_acting_twice() {
        // Ein Doppeltipp bei träger Verbindung darf nicht zweimal nach BTP
        // schreiben. Der Vorgangsschlüssel entscheidet: gleiche Kennung =
        // gleiche Antwort, ohne die Aktion erneut auszuführen.
        let st = TabletState::default();
        assert_eq!(st.remembered_result("op-1", "assign:42:3", 1_000), None);
        st.remember_result("op-1", "assign:42:3", relay_proto::TlResponse::ok(7), 1_000);
        let again = st
            .remembered_result("op-1", "assign:42:3", 1_200)
            .expect("gespeichert");
        assert!(again.ok);
        assert_eq!(again.state_rev, 7);
        // Ein anderer Vorgang ist davon unberührt.
        assert_eq!(st.remembered_result("op-2", "assign:42:3", 1_200), None);
    }

    #[test]
    fn a_reused_key_for_a_different_action_is_not_answered_from_memory() {
        // Sonst bekäme eine völlig andere Aktion die gespeicherte
        // Erfolgsmeldung der ersten — und würde nie ausgeführt.
        let st = TabletState::default();
        st.remember_result("op-1", "assign:42:3", relay_proto::TlResponse::ok(1), 1_000);
        assert_eq!(
            st.remembered_result("op-1", "free:5", 1_100),
            None,
            "andere Aktion, also keine gespeicherte Antwort"
        );
    }

    #[test]
    fn the_operation_memory_cannot_be_flooded() {
        // Ein Gerät mit gültigem Zugang darf den Arbeitsspeicher des
        // Turnier-PCs nicht mit erfundenen Kennungen füllen.
        let st = TabletState::default();
        for i in 0..(MAX_REMEMBERED_OPS * 3) {
            st.remember_result(
                &format!("op-{i}"),
                "a",
                relay_proto::TlResponse::ok(1),
                1_000,
            );
        }
        assert!(st.remembered_op_count() <= MAX_REMEMBERED_OPS);
        // Übermäßig lange Kennungen werden gar nicht erst behalten.
        let overlong = "x".repeat(500);
        st.remember_result(&overlong, "a", relay_proto::TlResponse::ok(1), 1_000);
        assert_eq!(st.remembered_result(&overlong, "a", 1_000), None);
    }

    #[test]
    fn a_remembered_operation_is_forgotten_after_a_while() {
        // Sonst wüchse die Liste über ein Turnier hinweg unbegrenzt, und ein
        // zufällig wiederholter Schlüssel bekäme Jahre später eine Antwort.
        let st = TabletState::default();
        st.remember_result("op-1", "a", relay_proto::TlResponse::ok(1), 1_000);
        assert!(st
            .remembered_result("op-1", "a", 1_000 + OP_MEMORY_MS + 1)
            .is_none());
    }

    #[test]
    fn moving_a_match_takes_its_score_along() {
        // Der Spielstand hängt am Feld, nicht am Spiel. Hängt die
        // Turnierleitung ein laufendes Spiel um, muss er mitwandern — sonst
        // zeigt das neue Feld 0:0 und das alte den stehengebliebenen Stand,
        // auf dem Court-Monitor wie im Liveticker.
        let st = TabletState::default();
        st.attach_tablet(1);
        st.record_score(1, 42, vec![(21, 15), (11, 8)]);
        st.set_court_state(1, r#"{"serving":{"team":"a"}}"#.to_string());

        st.move_match_score(1, 2, 42);

        // Nach dem Umzug steht das Spiel auf Feld 2 — so, wie BTP es nach
        // dem Schreibvorgang meldet.
        st.set_snapshot(snapshot(
            vec![match_on(42, Some(2), MatchStatus::OnCourt)],
            vec![(1, "Feld 1"), (2, "Feld 2")],
        ));
        let ov = st.overview();
        let neu = ov.iter().find(|c| c.court_id == 2).unwrap();
        let alt = ov.iter().find(|c| c.court_id == 1).unwrap();
        assert_eq!(
            neu.sets,
            vec![(21, 15), (11, 8)],
            "der Stand ist mitgewandert"
        );
        assert_eq!(alt.match_id, 0, "das alte Feld ist leer");
        assert_eq!(
            st.court_state(2).as_deref(),
            Some(r#"{"serving":{"team":"a"}}"#),
            "auch Aufschlag und Pause ziehen mit um"
        );
        assert_eq!(st.court_state(1), None);
    }

    #[test]
    fn moving_does_not_touch_a_court_that_counts_another_match() {
        // Ein verspäteter Umhänge-Auftrag darf keinen fremden Stand
        // überschreiben.
        let st = TabletState::default();
        st.attach_tablet(1);
        st.record_score(1, 99, vec![(5, 3)]); // ein ANDERES Spiel
        st.move_match_score(1, 2, 42);

        st.set_snapshot(snapshot(
            vec![match_on(99, Some(1), MatchStatus::OnCourt)],
            vec![(1, "Feld 1"), (2, "Feld 2")],
        ));
        let ov = st.overview();
        assert_eq!(
            ov.iter().find(|c| c.court_id == 1).unwrap().sets,
            vec![(5, 3)],
            "der fremde Stand bleibt, wo er ist"
        );
        assert_eq!(ov.iter().find(|c| c.court_id == 2).unwrap().match_id, 0);
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

    /// Schlanker Spiegel-Blick fürs Relay (v0.9.200): dieselbe effektive
    /// Satzstand-Auswahl wie `monitor_court` (Tablet-Stand vor BTP-Stand),
    /// aber ohne das Klonen des vollen `BtpMatch` — der Spiegel läuft bei
    /// jedem Nudge und im 2-s-Sweep über alle Felder.
    #[test]
    fn score_mirror_of_returns_the_effective_court_state() {
        let st = TabletState::default();
        st.set_snapshot(snapshot(
            vec![match_on(1, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1"), (102, "Court 2")],
        ));
        // Ohne Tablet: BTP-Stand (match_on setzt sets = [(5,3)]), kein State.
        assert_eq!(
            st.score_mirror_of(101),
            Some(ScoreMirror {
                match_id: 1,
                sets: vec![(5, 3)],
                state: None,
            })
        );
        // Tablet zählt und spiegelt seinen Zustand: beides kommt mit.
        st.record_score(101, 1, vec![(11, 9)]);
        st.set_court_state(101, r#"{"match":{"matchId":1}}"#.into());
        assert_eq!(
            st.score_mirror_of(101),
            Some(ScoreMirror {
                match_id: 1,
                sets: vec![(11, 9)],
                state: Some(r#"{"match":{"matchId":1}}"#.to_string()),
            })
        );
        // Feld ohne Match → nichts zu spiegeln.
        assert_eq!(st.score_mirror_of(102), None);
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
    fn a_second_recall_at_the_meeting_point_becomes_the_last_one() {
        // Wer dreimal „Nachruf" drückt und dreimal „Zweiter Aufruf" hört,
        // erfährt nie, dass es der letzte vor der kampflosen Wertung war.
        // Die Desktop-Oberfläche eskaliert seit jeher 2 → 3; die
        // Turnierleitungs-Seite muss dasselbe tun, und gezählt wird an einer
        // Stelle: hier.
        let st = TabletState::default();
        assert_eq!(st.note_prep_call(42, "team1"), 2, "der erste Nachruf");
        assert_eq!(st.note_prep_call(42, "team1"), 3);
        assert_eq!(
            st.note_prep_call(42, "team1"),
            3,
            "mehr als drei gibt es nicht"
        );
        // Die andere Partei zählt für sich — sie war ja vielleicht längst da.
        assert_eq!(st.note_prep_call(42, "team2"), 2);
    }

    #[test]
    fn a_match_returning_to_a_court_starts_its_calls_from_the_beginning() {
        // Die Standzeit wird beim Verlassen des Feldes vergessen — die
        // Aufrufe müssen es auch. Sonst zeigte ein gerade erst aufgerufenes
        // Spiel „3. Aufruf erfolgt", und der Aufruf-Knopf verschwände
        // dauerhaft: Die Turnierleitung könnte es nicht mehr rufen.
        let st = TabletState::default();
        st.note_court_call(101, 42, false);
        st.note_court_call(101, 42, false);
        assert_eq!(st.calls_made(101, 42), 3);

        // Feld geräumt (Spiel 42 steht nirgends mehr).
        st.reconcile_on_court(&HashMap::new(), 5_000);
        // Und später wieder aufgerufen.
        st.reconcile_on_court(&HashMap::from([(101, 42)]), 9_000);
        assert_eq!(
            st.calls_made(101, 42),
            0,
            "frisch auf dem Feld heißt: noch kein Aufruf gesprochen"
        );
    }

    #[test]
    fn an_announcement_device_without_a_hall_answers_for_a_job_without_one() {
        // Ein Auftrag ohne Halle geht an JEDES Gerät. Dann muss auch jedes
        // Gerät als Zuhörer dafür zählen — sonst meldete die Seite „kein
        // Ansage-Gerät verbunden", während der Aufruf gerade erklingt, und
        // jemand ruft zur Sicherheit per Funk ein zweites Mal.
        let st = TabletState::default();
        st.announce_jobs_since("Halle A", 0, 10_000);
        assert!(
            st.has_announce_listener("", 10_000),
            "das Gerät in Halle A hört auch den Aufruf ohne Halle"
        );
    }

    #[test]
    fn made_up_hall_names_cannot_grow_the_listener_list_without_end() {
        // Die Abhol-Route ist im Hallennetz offen. Ohne Grenze könnte ein
        // fehlkonfiguriertes (oder böswilliges) Gerät mit wechselnden
        // Hallennamen den Speicher des Turnier-PCs volllaufen lassen —
        // mitsamt Tablet-Server und BTP-Übertragung.
        let st = TabletState::default();
        for i in 0..500 {
            st.announce_jobs_since(&format!("Halle {i}"), 0, 10_000);
        }
        assert!(
            st.announce_listener_count() <= 64,
            "die Liste bleibt beschränkt, war aber {}",
            st.announce_listener_count()
        );
        // Ein überlanger Name wird gekappt statt in voller Länge behalten.
        st.announce_jobs_since(&"A".repeat(4096), 0, 10_000);
        assert!(st.longest_announce_listener_key() <= 128);
    }

    #[test]
    fn a_hall_counts_as_covered_while_a_device_is_picking_up_announcements() {
        // Die Turnierleitung muss erfahren, wenn ihr Aufruf nirgends
        // erklingt. Wer Aufträge abholt, ist ein Ansage-Gerät — einen
        // anderen Nachweis gibt es nicht, und einen zweiten Meldeweg
        // bräuchte es dafür auch nicht.
        let st = TabletState::default();
        assert!(!st.has_announce_listener("Halle A", 10_000));

        st.announce_jobs_since("Halle A", 0, 10_000);
        assert!(st.has_announce_listener("Halle A", 10_000));
        assert!(
            !st.has_announce_listener("Halle B", 10_000),
            "nur die eigene Halle"
        );

        // Ein Gerät ohne eingestellte Halle spricht für alle (Einzelhalle).
        st.announce_jobs_since("", 0, 10_000);
        assert!(st.has_announce_listener("Halle B", 10_000));

        // Und wer lange nicht mehr abgeholt hat, zählt nicht mehr.
        assert!(!st.has_announce_listener("Halle A", 10_000 + 60_000));
    }

    #[test]
    fn an_announcement_job_reaches_only_its_own_hall() {
        // In einer Zwei-Hallen-Veranstaltung darf der Aufruf für Halle B
        // nicht aus den Lautsprechern von Halle A kommen. Ein Gerät ohne
        // eingestellte Halle (Einzelhallen-Betrieb) hört alles.
        let st = TabletState::default();
        let now = 100_000;
        st.publish_announce_job(
            "Halle B".to_string(),
            AnnounceJobKind::CourtCall {
                court_id: 7,
                match_id: 42,
                stage: 2,
                side: relay_proto::PrepCallSide::Both,
            },
            now,
        );
        assert!(st.announce_jobs_since("Halle A", 0, now).is_empty());
        assert_eq!(st.announce_jobs_since("Halle B", 0, now).len(), 1);
        assert_eq!(
            st.announce_jobs_since("", 0, now).len(),
            1,
            "ohne Halle: alles"
        );
    }

    #[test]
    fn an_announcement_nobody_could_play_expires_instead_of_arriving_late() {
        // Ein Ansage-Gerät, das eine Minute weg war, darf beim Wiederkommen
        // nicht die Aufrufe der letzten Minute nachplärren — die Spiele
        // laufen längst.
        let st = TabletState::default();
        let job = AnnounceJobKind::PrepCall {
            match_id: 42,
            side: relay_proto::PrepCallSide::Both,
            stage: 2,
        };
        st.publish_announce_job(String::new(), job, 100_000);
        assert_eq!(
            st.announce_jobs_since("", 0, 155_000).len(),
            1,
            "55 s: noch gültig"
        );
        assert!(
            st.announce_jobs_since("", 0, 161_000).is_empty(),
            "nach 60 s verfällt der Auftrag"
        );
    }

    #[test]
    fn each_announcement_device_picks_up_only_what_it_has_not_heard() {
        // Dieselbe Buchführung wie beim Freitext: fortlaufende Nummer, das
        // Gerät merkt sich die letzte. Sonst spräche es bei jeder Abfrage
        // alles noch einmal.
        let st = TabletState::default();
        let kind = AnnounceJobKind::CourtCall {
            court_id: 1,
            match_id: 7,
            stage: 2,
            side: relay_proto::PrepCallSide::Both,
        };
        let first = st.publish_announce_job(String::new(), kind.clone(), 1_000);
        let second = st.publish_announce_job(String::new(), kind, 1_000);
        assert!(second > first, "die Nummer läuft weiter");
        let neu = st.announce_jobs_since("", first, 1_000);
        assert_eq!(neu.len(), 1);
        assert_eq!(neu[0].id, second);
        assert!(st.announce_jobs_since("", second, 1_000).is_empty());
    }

    #[test]
    fn the_call_stage_is_counted_at_the_host_not_on_the_device() {
        // Zweiter und dritter Aufruf müssen auf jedem Gerät dieselbe Zahl
        // zeigen. Zählte jeder Browser für sich, riefe die eine Turnierleitung
        // zum zweiten Mal, während die andere schon beim dritten ist — und
        // niemand wüsste, ob das Spiel gleich gestrichen wird.
        let st = TabletState::default();
        assert_eq!(st.calls_made(101, 7), 0, "noch kein Aufruf gesprochen");
        assert_eq!(
            st.note_court_call(101, 7, false),
            2,
            "der erneute Aufruf ist der 2."
        );
        assert_eq!(st.note_court_call(101, 7, false), 3);
        assert_eq!(st.calls_made(101, 7), 3, "und jedes Gerät liest dieselbe 3");
    }

    #[test]
    fn repeated_calls_keep_counting_beyond_the_third() {
        // Option „Aufrufe unbegrenzt" (Feldtest 17.08.2026): Die TL-Seite
        // darf beliebig oft rufen — der Zähler läuft dann ehrlich weiter,
        // statt bei drei festzuhängen. Ab dem vierten Aufruf spricht das
        // Ansage-Gerät die schlichte Feld-Ansage ohne Stufenwort
        // (`AnnounceJobPlayer`): „Dritter und letzter Aufruf" noch einmal
        // wäre gelogen.
        let st = TabletState::default();
        st.note_court_call(101, 7, true);
        st.note_court_call(101, 7, true);
        assert_eq!(
            st.note_court_call(101, 7, true),
            4,
            "nach dem dritten kommt der vierte, kein Deckel"
        );
        assert_eq!(st.note_court_call(101, 7, true), 5);
        assert_eq!(st.calls_made(101, 7), 5, "jedes Gerät liest dieselbe 5");
        // Eine zeitliche Untergrenze (Uhr, maximal 3) drückt den Zähler
        // dabei nie wieder herunter.
        assert_eq!(st.note_court_call_at_least(101, 7, 3, SIDE_BOTH, true), 6);
        // Ein Gerät OHNE die Option dreht den geteilten Zähler nicht
        // zurück — treibt ihn aber auch nicht weiter über seinen Deckel
        // hinaus (sein Client bietet jenseits von 3 ohnehin keinen Knopf).
        assert_eq!(st.note_court_call(101, 7, false), 6);
    }

    #[test]
    fn without_the_option_the_counter_keeps_its_cap_of_three() {
        // Sicherheitsnetz aus dem Review 17.08.2026: Ohne „Aufrufe
        // unbegrenzt" bleibt der alte Deckel host-seitig bestehen — ein
        // Turnier ohne die Option darf sich nicht allein auf das
        // Client-Gating verlassen (alte tl.html-Stände, schnelle
        // Doppel-Tipps über Geschwister-Knöpfe).
        let st = TabletState::default();
        st.note_court_call(101, 7, false);
        st.note_court_call(101, 7, false);
        assert_eq!(st.note_court_call(101, 7, false), 3, "Deckel hält");
        assert_eq!(st.calls_made(101, 7), 3);
    }

    #[test]
    fn the_court_overview_carries_the_call_stage_for_every_ui() {
        // Nur wenn die Feld-Übersicht die Stufe mitliefert, kann auch die
        // Desktop-Oberfläche sehen, dass die Turnierleitungs-Seite gerufen
        // hat — sonst bliebe sie zurück und böte erneut den zweiten Aufruf
        // an, während die Halle den dritten gehört hat.
        let st = TabletState::default();
        let snap = snapshot(
            vec![match_on(7, Some(101), MatchStatus::OnCourt)],
            vec![(101, "Court 1")],
        );
        st.set_snapshot(snap.clone());
        assert_eq!(st.overview()[0].call_stage, 0, "noch nichts gesprochen");
        st.note_court_call(101, 7, false);
        assert_eq!(st.overview()[0].call_stage, 2);
    }

    #[test]
    fn the_desktop_reports_the_stage_it_actually_spoke() {
        // Die Desktop-Übersicht sagt beim ersten Druck den schlichten Aufruf
        // an (ohne Stufenwort) — das ist Stufe 1. Zählte der Turnier-PC das
        // als „einen weiteren Aufruf", stünde er sofort auf 2: Die
        // Turnierleitungs-Seite zeigte „2. Aufruf erfolgt", obwohl die Halle
        // erst einen gehört hat, und nach dem zweiten Druck verschwände dort
        // der Aufruf-Knopf ganz. Deshalb meldet der Desktop, was er
        // gesprochen hat, statt hochzählen zu lassen.
        let st = TabletState::default();
        assert_eq!(st.reached_court_call(101, 7, 1), 1, "schlichter Aufruf");
        assert_eq!(st.calls_made(101, 7), 1);
        assert_eq!(st.reached_court_call(101, 7, 2), 2, "„Zweiter Aufruf\"");
        assert_eq!(st.calls_made(101, 7), 2);
        // Eine niedrigere Meldung dreht nicht zurück (zwei Geräte gleichzeitig).
        assert_eq!(st.reached_court_call(101, 7, 1), 2);
        // Und ein anderes Spiel auf dem Feld beginnt von vorn.
        assert_eq!(st.reached_court_call(101, 8, 1), 1);
    }

    #[test]
    fn the_clock_can_lift_the_call_stage_without_skipping_a_step() {
        // Die Desktop-Übersicht sagt mindestens die Stufe an, die ihre Uhr
        // schon als fällig zeigt — sonst stünde am Feld „3. Aufruf fällig",
        // während der Knopf den zweiten ansagt. Diese Vorgabe darf die
        // gemeinsame Zählung anheben, aber nie zurückdrehen.
        let st = TabletState::default();
        assert_eq!(
            st.note_court_call_at_least(101, 7, 3, SIDE_BOTH, false),
            3,
            "die Uhr war weiter"
        );
        assert_eq!(
            st.note_court_call_at_least(101, 7, 2, SIDE_BOTH, false),
            3,
            "und eine niedrigere Vorgabe dreht nicht zurück"
        );
    }

    #[test]
    fn calling_both_parties_one_after_the_other_is_one_call_round() {
        // Spec tl-liste-vereinfachen E1: Ein Partei-Aufruf ist ein
        // vollwertiger Aufruf und zählt die Stufe hoch — aber nur EINMAL
        // je Runde. Wer erst Partei A und dann Partei B ruft, hat einmal
        // gerufen, nicht zweimal.
        let st = TabletState::default();
        assert_eq!(
            st.note_court_call_at_least(101, 7, 0, SIDE_TEAM1, false),
            2,
            "der erste Partei-Aufruf ist der zweite Aufruf"
        );
        assert_eq!(
            st.note_court_call_at_least(101, 7, 0, SIDE_TEAM2, false),
            2,
            "die andere Partei gehört zur selben Runde"
        );
        assert_eq!(st.calls_made(101, 7), 2, "alle Geräte lesen dieselbe 2");

        // Dieselbe Partei ein zweites Mal: das ist eine neue Runde.
        assert_eq!(st.note_court_call_at_least(101, 7, 0, SIDE_TEAM1, false), 3);
        assert_eq!(st.calls_made(101, 7), 3);
    }

    #[test]
    fn a_party_call_after_a_full_call_opens_the_next_stage() {
        // Nach einem Aufruf an beide Parteien ist die Runde voll — der
        // nächste Aufruf, egal an wen, ist der dritte und letzte.
        let st = TabletState::default();
        assert_eq!(st.note_court_call(101, 7, false), 2);
        assert_eq!(st.note_court_call_at_least(101, 7, 0, SIDE_TEAM2, false), 3);
        // Und die Gegenpartei schließt dieselbe (dritte) Runde ab.
        assert_eq!(st.note_court_call_at_least(101, 7, 0, SIDE_TEAM1, false), 3);
    }

    #[test]
    fn a_desktop_call_closes_the_round_for_the_web_side_too() {
        // Der Desktop-Aufruf gilt immer beiden Parteien und meldet nur
        // seine Stufe (`reached_court_call`). Ein anschließender
        // Partei-Aufruf von der TL-Seite muss deshalb die nächste Stufe
        // eröffnen — sonst hörte die Halle zweimal „Zweiter Aufruf".
        let st = TabletState::default();
        assert_eq!(st.reached_court_call(101, 7, 2), 2);
        assert_eq!(st.note_court_call_at_least(101, 7, 0, SIDE_TEAM1, false), 3);
    }

    #[test]
    fn a_new_match_on_the_court_starts_counting_calls_from_the_beginning() {
        // Sonst erbte das nächste Spiel die Stufe seines Vorgängers und
        // stünde sofort als „dritter Aufruf" da.
        let st = TabletState::default();
        st.note_court_call(101, 7, false);
        st.note_court_call(101, 7, false);
        assert_eq!(st.calls_made(101, 8), 0, "anderes Spiel, neue Zählung");
        assert_eq!(st.note_court_call(101, 8, false), 2);
        assert_eq!(st.calls_made(101, 7), 0, "und der Vorgänger ist vergessen");
    }

    #[test]
    fn restarting_the_transfer_lets_the_automatic_assignment_run_again() {
        // Die Pause wird auf der Turnierleitungs-Seite gesetzt. Ist das Tablet
        // weg (leer, verlegt, Zugang widerrufen), gäbe es sonst keinen Weg
        // zurück — die Automatik bliebe für den Rest des Turniers aus, obwohl
        // sie in den Einstellungen eingeschaltet ist. Das Stoppen und Starten
        // der Übertragung ist der Griff, den jede Turnierleitung kennt.
        let st = TabletState::default();
        st.set_auto_assign_paused(true);
        st.reset_runtime_switches();
        assert!(!st.auto_assign_paused());
    }

    #[test]
    fn only_one_device_gets_a_walkover_proposal_to_work_on() {
        // Zwei Turnierleitungs-Geräte tippen im selben Moment „kampflos
        // werten". Ohne Anspruch schrieben beide dieselben Wertungen nach
        // BTP. Wer den Vorschlag nimmt, hat ihn — der andere sieht, dass
        // schon jemand da war.
        let st = TabletState::default();
        st.add_walkover_proposal(WalkoverProposal {
            id: "p-1".to_string(),
            entry_id: 10,
            retired_team: "Weber / Fischer".to_string(),
            draw_name: "HD B".to_string(),
            created_at_ms: 1_000,
        });
        let first = st.take_walkover_proposal("p-1");
        assert!(first.is_some(), "der erste bekommt den Vorschlag");
        assert!(
            st.take_walkover_proposal("p-1").is_none(),
            "der zweite geht leer aus"
        );
        // Ging gar nichts nach BTP, kommt er zurück und bleibt bearbeitbar.
        st.add_walkover_proposal(first.unwrap());
        assert!(st.take_walkover_proposal("p-1").is_some());
    }

    #[test]
    fn clearing_a_court_also_drops_its_pending_claim() {
        // Ein geräumtes Feld darf sofort wieder vergeben werden. Blieb die
        // Vormerkung des Schreibvorgangs stehen, wies der nächste Versuch mit
        // „hat gerade jemand anderes belegt" ab — bis zu 15 Sekunden lang,
        // obwohl das Feld sichtbar leer war.
        let st = TabletState::default();
        assert!(st.try_reserve_court(101, 7, 1_000));
        st.clear_court(101);
        assert!(
            st.reserved_courts(1_000).is_empty(),
            "die Vormerkung gehört zum Spiel, das gerade beendet wurde"
        );
        assert!(st.try_reserve_court(101, 8, 1_000), "und das Feld ist frei");
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
    fn halls_set_by_hand_survive_a_restart() {
        // Aus dem Test am Gerät (09.08.): Nach einem Neustart des Turnier-PCs
        // waren alle von Hand gesetzten Spielorte weg — 0 von 120 Spielen
        // hatten noch eine Halle. Für Vorbereitungs-Aufrufe ist das richtig
        // (die sind Minuten alt), für den Spielort nicht: Den setzt die
        // Turnierleitung einmal für den Tag, und ein Absturz mitten im
        // Turnier darf diese Arbeit nicht vernichten.
        let dir = tempfile::tempdir().unwrap();
        let datei = dir.path().join("spielorte.json");

        let st = TabletState::default();
        st.use_manual_hall_file(&datei);
        st.set_manual_hall(4711, "Halle B");
        st.set_manual_hall(4712, "Halle A");
        st.set_manual_hall(4712, ""); // wieder zurückgenommen

        // Neuer Zustand, dieselbe Datei — wie nach einem Neustart.
        let neu = TabletState::default();
        neu.use_manual_hall_file(&datei);
        assert_eq!(neu.manual_hall(4711).as_deref(), Some("Halle B"));
        assert_eq!(neu.manual_hall(4712), None, "zurückgenommen bleibt weg");
    }

    #[test]
    fn a_missing_hall_file_is_not_an_error() {
        // Erster Start, noch nichts gesetzt: leer statt Fehler.
        let dir = tempfile::tempdir().unwrap();
        let st = TabletState::default();
        st.use_manual_hall_file(&dir.path().join("gibt-es-nicht.json"));
        assert_eq!(st.manual_hall(1), None);
        // Und danach lässt sich trotzdem setzen.
        st.set_manual_hall(1, "Halle A");
        assert_eq!(st.manual_hall(1).as_deref(), Some("Halle A"));
    }

    #[test]
    fn a_hall_set_by_hand_reaches_the_liveticker() {
        // Der Hallenfilter des Livetickers (`display=next&halle=…`) liest die
        // Halle am anstehenden Spiel. Bisher füllte sie nur ein
        // Vorbereitungs-Aufruf — lief das Turnier über BTP, blieb sie leer und
        // der Monitor einer Halle zeigte gar nichts. Eine von Hand gesetzte
        // Halle muss deshalb genauso durchschlagen, **ohne** dass das Spiel
        // dadurch als „aufgerufen" gilt.
        use crate::btp::model::BtpLocation;
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(4, None, MatchStatus::Scheduled)],
            vec![(101, "Court 1")],
        );
        snap.locations = vec![BtpLocation {
            id: 7,
            name: "Halle A".to_string(),
        }];
        st.set_manual_hall(4, "halle a");

        st.apply_preparation_calls(&mut snap);

        let m = &snap.matches[0];
        assert_eq!(
            m.preparation_hall.as_deref(),
            Some("Halle A"),
            "in BTPs Schreibweise"
        );
        assert_eq!(
            m.preparation_call_ts, None,
            "einen Ort zu setzen ist kein Aufruf - sonst meldete der Monitor einen Aufruf, den es nie gab"
        );
    }

    #[test]
    fn eine_auto_verteilte_halle_erreicht_den_liveticker() {
        // Spec `hallen-vorverteilung` (E7): Die automatisch vorverteilte
        // Halle ist der Spieler-Kanal des Features — sie muss den
        // Hallenfilter des Livetickers erreichen wie eine Hand-Halle,
        // ohne als „aufgerufen" zu gelten. Hand schlägt Auto.
        use crate::btp::model::BtpLocation;
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![
                match_on(4, None, MatchStatus::Scheduled),
                match_on(5, None, MatchStatus::Scheduled),
            ],
            vec![(101, "Court 1")],
        );
        snap.locations = vec![BtpLocation {
            id: 7,
            name: "Halle A".to_string(),
        }];
        st.auto_hall_store()
            .insert_many(&[(4, "halle a".into()), (5, "halle a".into())]);
        st.set_manual_hall(5, "Halle B");

        st.apply_preparation_calls(&mut snap);

        let m4 = snap.matches.iter().find(|m| m.id == 4).unwrap();
        assert_eq!(
            m4.preparation_hall.as_deref(),
            Some("Halle A"),
            "Auto-Halle in BTPs Schreibweise"
        );
        assert_eq!(m4.preparation_call_ts, None, "kein Aufruf");
        let m5 = snap.matches.iter().find(|m| m.id == 5).unwrap();
        assert_eq!(
            m5.preparation_hall.as_deref(),
            Some("Halle B"),
            "die Hand-Halle behält Vorrang vor der Auto-Halle"
        );
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

    // ───────────── A2 / ADR 0017: Reconnect-Wahrheit „Slot-Halter gewinnt" ─────────────

    fn owner(epoch: u64, device: &str, scored: bool) -> CourtOwner {
        CourtOwner {
            epoch,
            device: device.to_string(),
            scored_since_claim: scored,
        }
    }

    /// Regel 2: Feld frei (kein Halter) → der Rückkehrer setzt lokal durch.
    #[test]
    fn reconnect_decision_slot_free_keeps_local() {
        assert_eq!(
            reconnect_decision("dev-a", None, false),
            ReconnectDecision::KeepLocal
        );
    }

    /// Regel 3: Der Rückkehrer ist selbst der Halter (eigener Reclaim) →
    /// KeepLocal, auch wenn dieser Halter schon gezählt hat.
    #[test]
    fn reconnect_decision_own_reclaim_keeps_local() {
        assert_eq!(
            reconnect_decision("dev-a", Some(owner(5, "dev-a", true)), false),
            ReconnectDecision::KeepLocal
        );
    }

    /// Regel 4: Ein FREMDES Gerät hält den Slot UND hat seit der Übernahme
    /// gezählt → der Rückkehrer tritt zurück (legitimer Übernehmer gewinnt).
    #[test]
    fn reconnect_decision_foreign_owner_scored_stands_down() {
        assert_eq!(
            reconnect_decision("dev-a", Some(owner(7, "dev-b", true)), false),
            ReconnectDecision::StandDown
        );
    }

    /// Regel 5: Fremder Halter, aber OHNE Score seit der Übernahme → der
    /// Rückkehrer gewinnt deterministisch (KeepLocal).
    #[test]
    fn reconnect_decision_foreign_owner_not_scored_keeps_local() {
        assert_eq!(
            reconnect_decision("dev-a", Some(owner(7, "dev-b", false)), false),
            ReconnectDecision::KeepLocal
        );
    }

    /// Regel 1: Ein finalisiertes Match darf nie überbügelt werden → StandDown,
    /// unabhängig vom Halter (auch beim eigenen Reclaim, auch bei freiem Feld).
    #[test]
    fn reconnect_decision_finalized_always_stands_down() {
        assert_eq!(
            reconnect_decision("dev-a", None, true),
            ReconnectDecision::StandDown
        );
        assert_eq!(
            reconnect_decision("dev-a", Some(owner(5, "dev-a", false)), true),
            ReconnectDecision::StandDown
        );
        assert_eq!(
            reconnect_decision("dev-a", Some(owner(7, "dev-b", false)), true),
            ReconnectDecision::StandDown
        );
    }

    /// `scored_since_claim`: nach dem Claim false, nach `record_score` true,
    /// nach erneutem Claim wieder false (neuer Zähl-Abschnitt).
    #[test]
    fn scored_since_claim_tracks_counting_section() {
        let st = TabletState::default();
        st.claim_court(1, "dev-a");
        assert_eq!(
            st.court_owner(1),
            Some(owner(1, "dev-a", false)),
            "frisch geclaimt: noch nicht gezählt"
        );

        st.record_score(1, 100, vec![(5, 3)]);
        assert_eq!(
            st.court_owner(1),
            Some(owner(1, "dev-a", true)),
            "nach record_score: gezählt"
        );

        // Übernahme durch ein anderes Gerät: neuer Zähl-Abschnitt, Flag zurück.
        st.claim_court(1, "dev-b");
        assert_eq!(
            st.court_owner(1),
            Some(owner(2, "dev-b", false)),
            "neuer Claim: Zähler wieder auf false"
        );
    }

    /// `court_owner` liefert `None` für ein freies Feld.
    #[test]
    fn court_owner_none_when_free() {
        let st = TabletState::default();
        assert_eq!(st.court_owner(9), None);
    }

    /// Epoch-Monotonie: aufeinanderfolgende Claims liefern strikt steigende
    /// Tokens (die Epoch der Ownership) — auch über verschiedene Felder hinweg.
    #[test]
    fn claim_court_epochs_are_strictly_increasing() {
        let st = TabletState::default();
        let e1 = st.claim_court(1, "dev-a");
        let e2 = st.claim_court(1, "dev-b");
        let e3 = st.claim_court(2, "dev-c");
        assert!(e1 < e2, "erneuter Claim desselben Felds steigt");
        assert!(e2 < e3, "Claim eines anderen Felds steigt weiter");
        assert_eq!(st.court_owner(1).map(|o| o.epoch), Some(e2));
        assert_eq!(st.court_owner(2).map(|o| o.epoch), Some(e3));
    }

    /// Die Reconnect-Kette real durchgespielt: Gerät A zählt, Gerät B
    /// übernimmt den Slot und zählt weiter — A kehrt zurück und tritt zurück.
    #[test]
    fn reconnect_end_to_end_foreign_takeover_scored() {
        let st = TabletState::default();
        st.claim_court(1, "dev-a");
        st.record_score(1, 100, vec![(11, 9)]);
        // B übernimmt und zählt.
        st.claim_court(1, "dev-b");
        st.record_score(1, 100, vec![(11, 11)]);
        // A kehrt zurück: der aktuelle Halter (B) hat gezählt → StandDown.
        assert_eq!(
            reconnect_decision("dev-a", st.court_owner(1), false),
            ReconnectDecision::StandDown
        );
    }

    /// Gegenprobe: B übernimmt, hat aber noch NICHT gezählt — A gewinnt.
    #[test]
    fn reconnect_end_to_end_foreign_takeover_not_scored() {
        let st = TabletState::default();
        st.claim_court(1, "dev-a");
        st.record_score(1, 100, vec![(11, 9)]);
        st.claim_court(1, "dev-b"); // Übernahme ohne eigenen Score
        assert_eq!(
            reconnect_decision("dev-a", st.court_owner(1), false),
            ReconnectDecision::KeepLocal
        );
    }

    // ─────────── A2 / ADR 0017, Regel b: Finalisiert-Signal ───────────

    /// Setzen und Lesen des Finalisiert-Merkers samt Match-Bindung.
    #[test]
    fn recently_finalized_set_and_lookup() {
        let st = TabletState::default();
        assert_eq!(st.recently_finalized(5), None, "frisch: kein Merker");
        st.mark_finalized(5, 42);
        assert_eq!(st.recently_finalized(5), Some(42));
        assert!(st.is_match_finalized(5, 42));
        assert!(!st.is_match_finalized(5, 43), "andere Match-ID → nein");
        assert!(!st.is_match_finalized(6, 42), "anderes Feld → nein");
    }

    /// TTL-Ablauf: ein Merker älter als [`FINALIZED_TTL`] gilt als abgelaufen
    /// und wird beim Lesen weggeräumt (der Merker darf nicht ewig hängen).
    #[test]
    fn recently_finalized_expires_after_ttl() {
        let st = TabletState::default();
        // Direkt einen überalterten Zeitstempel setzen (Test im selben Modul).
        let stale = std::time::Instant::now()
            .checked_sub(FINALIZED_TTL + std::time::Duration::from_secs(1))
            .expect("Instant weit genug in der Vergangenheit");
        st.recently_finalized
            .write()
            .unwrap()
            .insert(5, (42, stale));
        assert_eq!(st.recently_finalized(5), None, "abgelaufen → None");
        assert!(
            !st.recently_finalized.read().unwrap().contains_key(&5),
            "abgelaufener Eintrag wird beim Lesen geräumt"
        );
    }

    /// `clear_finalized`: ein Feld mit OnCourt-Match räumt den Merker
    /// bedingungslos — auch bei derselben matchId (TL-Ergebniskorrektur/Undo),
    /// sonst würden dessen Punkte still verworfen (Review-Befund M1).
    #[test]
    fn clear_finalized_removes_marker_unconditionally() {
        let st = TabletState::default();
        st.mark_finalized(5, 42);
        // Dasselbe Match kehrt OnCourt zurück (Ergebniskorrektur) → Merker weg.
        st.clear_finalized(5);
        assert_eq!(
            st.recently_finalized(5),
            None,
            "OnCourt (auch gleiche matchId) → geräumt"
        );
    }

    /// End-to-End: Ist das Feld finalisiert, tritt sogar der EIGENE Halter beim
    /// Reconnect zurück (Regel b schlägt „eigener Reclaim = KeepLocal"). So
    /// überbügelt kein Tablet das per Hand eingegebene Ergebnis.
    #[test]
    fn reconnect_end_to_end_finalized_stands_down() {
        let st = TabletState::default();
        st.claim_court(1, "dev-a");
        st.record_score(1, 100, vec![(21, 10)]);
        st.mark_finalized(1, 100);
        let finalized = st.recently_finalized(1).is_some();
        assert!(finalized);
        assert_eq!(
            reconnect_decision("dev-a", st.court_owner(1), finalized),
            ReconnectDecision::StandDown
        );
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
            officials: None,
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

    // ── Persistente Nachschub-Queue (ADR 0018, Hebel B/(b)) ────────────────

    /// Snapshot mit einem bestimmten Turnier-Namen (Turnier-Guard).
    fn snap_named(name: &str) -> BtpSnapshot {
        let mut s = snapshot(Vec::new(), Vec::new());
        s.tournament_name = name.to_string();
        s
    }

    /// Ein Official mit dieser ID (Name nur zur Unterscheidung).
    fn official(id: i64) -> crate::btp::model::BtpOfficial {
        crate::btp::model::BtpOfficial {
            id,
            name: format!("Schiri{id}"),
            first: String::new(),
            nationality: None,
        }
    }

    #[test]
    fn overview_zeigt_schiedsrichter_nur_bei_aktivem_betrieb() {
        let st = TabletState::default();
        let mut snap = snapshot(
            vec![match_on(1, Some(5), MatchStatus::OnCourt)],
            vec![(5, "Feld 1"), (6, "Feld 2")],
        );
        snap.officials = vec![official(1), official(2)];
        st.set_snapshot(snap);
        st.officials_store()
            .assign(1, crate::tablet::officials::OfficialRole::Sr, 1);

        // Feature aus (Default) ⇒ kein Wort von Schiedsrichtern.
        let c = &st.overview()[0];
        assert!(c.sr.is_empty());
        assert!(c.official_warn.is_none());

        // Feature an ⇒ Name am belegten Feld, freies Feld bleibt leer.
        st.officials_store().set_enabled(true);
        let o = st.overview();
        assert_eq!(o[0].sr, vec!["Schiri1".to_string()]);
        assert!(o[0].ar.is_empty(), "kein AR zugewiesen");
        assert!(o[1].sr.is_empty(), "Feld ohne Spiel");
    }

    #[test]
    fn das_tablet_bekommt_die_namen_von_sr_und_ar() {
        // Spec Nr. 7: Das Schiri-Tablet zeigt SR/AR des laufenden Spiels —
        // als Namen, damit es nichts auflösen muss (LAN wie Cloud).
        let st = TabletState::default();
        let m = match_on(1, Some(5), MatchStatus::OnCourt);
        let mut snap = snapshot(vec![m.clone()], vec![(5, "Feld 1")]);
        snap.officials = vec![official(1), official(2)];
        st.set_snapshot(snap);
        st.officials_store()
            .assign(1, crate::tablet::officials::OfficialRole::Sr, 1);
        st.officials_store()
            .assign(1, crate::tablet::officials::OfficialRole::Ar, 2);

        // Ohne Schiedsrichter-Betrieb bleibt der Brief leer.
        assert_eq!(st.match_officials(&m), (Vec::new(), Vec::new()));

        st.officials_store().set_enabled(true);
        assert_eq!(
            st.match_officials(&m),
            (vec!["Schiri1".to_string()], vec!["Schiri2".to_string()])
        );
    }

    #[test]
    fn overview_meldet_die_konflikt_kategorie_am_feld() {
        // Manuelle Zuweisung mit Konflikt wird ausgeführt UND gewarnt
        // (Spec Nr. 2) — die Anzeige trägt nur die Kategorie, nie den Grund.
        let st = TabletState::default();
        let mut m = match_on(1, Some(5), MatchStatus::OnCourt);
        m.team1[0].club = Some("TSV Musterstadt".into());
        let mut snap = snapshot(vec![m], vec![(5, "Feld 1")]);
        snap.officials = vec![official(1)];
        st.set_snapshot(snap);
        st.officials_store().set_enabled(true);
        st.officials_store().set_club(1, "TSV Musterstadt");
        st.officials_store()
            .assign(1, crate::tablet::officials::OfficialRole::Sr, 1);

        let c = &st.overview()[0];
        assert_eq!(c.official_warn.as_deref(), Some("Verein"));
    }

    #[test]
    fn ein_feld_ohne_bedienervergabe_verbraucht_keinen_eintrag() {
        // Spec Nr. 6: Felder, auf denen der Schiedsrichter selbst das Tablet
        // bedient, brauchen keinen Spieler als Bediener — und dürfen der
        // Warteschlange deshalb auch keinen wegnehmen.
        let st = TabletState::default();
        st.officials_store().set_enabled(true);
        st.officials_store().set_court_switches(
            5,
            crate::tablet::officials::CourtSwitches {
                sr: true,
                ar: true,
                operator: false,
            },
        );
        st.enqueue_scorekeeper(1, vec!["A".into()], 9, 1_000);

        st.assign_scorekeeper_for_court(5, 42);
        assert!(st.assigned_scorekeeper(5).is_none(), "Feld ist ausgenommen");
        assert_eq!(
            st.scorekeeper_queue().len(),
            1,
            "der Eintrag bleibt für ein anderes Feld erhalten"
        );

        // Default (kein Eintrag) bleibt aktiv — Bestandsverhalten.
        st.assign_scorekeeper_for_court(6, 43);
        assert!(st.assigned_scorekeeper(6).is_some());
        assert!(st.scorekeeper_queue().is_empty());

        // Und ohne Schiedsrichter-Betrieb greift der Schalter gar nicht:
        // Sonst bliebe ein ausgenommenes Feld nach dem Abschalten für immer
        // ohne Bediener — die Bedienstelle dafür ist dann unerreichbar.
        st.officials_store().set_enabled(false);
        st.enqueue_scorekeeper(2, vec!["B".into()], 9, 2_000);
        st.assign_scorekeeper_for_court(5, 44);
        assert_eq!(st.assigned_scorekeeper(5), Some(vec!["B".to_string()]));
    }

    #[test]
    fn snapshot_bindet_das_officials_roster_ans_turnier() {
        // Der Roster folgt dem Snapshot: Turnier binden, neue Officials in
        // die Rotationsreihenfolge aufnehmen — beim Turnierwechsel wird der
        // Stand verworfen (ADR 0022).
        let dir = tempfile::tempdir().unwrap();
        let st = TabletState::default();
        st.officials_store()
            .set_path(dir.path().join("officials-state.json"));

        let mut snap = snap_named("Cup A");
        snap.officials = vec![official(3), official(5)];
        st.set_snapshot(snap);
        assert_eq!(st.officials_store().tournament(), "Cup A");
        assert_eq!(st.officials_store().order(), vec![3, 5]);

        // Zusatzdaten des laufenden Turniers …
        st.officials_store().set_paused(3, true);
        let mut snap = snap_named("Cup A");
        snap.officials = vec![official(3), official(5), official(8)];
        st.set_snapshot(snap);
        assert_eq!(
            st.officials_store().order(),
            vec![3, 5, 8],
            "neuer kommt an"
        );
        assert!(st.officials_store().extra(3).paused, "Pause bleibt");

        // … überleben den Turnierwechsel NICHT.
        let mut snap = snap_named("Cup B");
        snap.officials = vec![official(9)];
        st.set_snapshot(snap);
        assert_eq!(st.officials_store().order(), vec![9]);
        assert!(!st.officials_store().extra(3).paused);
    }

    #[test]
    fn officials_for_result_reasserts_the_known_occupation() {
        // Live-Befund 14.08.2026: Das Ergebnis-SENDUPDATE verlor die
        // Schiedsrichter-Besetzung, wenn der Match-Knoten sie wegliess.
        // `officials_for_result` liefert deshalb immer einen konkreten
        // Wert, solange der Schiedsrichter-Betrieb läuft.
        let st = TabletState::default();

        // Ohne Schiedsrichter-Betrieb: nichts anfassen.
        let mut m = match_on(10, Some(5), MatchStatus::OnCourt);
        st.set_snapshot(snapshot(vec![m.clone()], Vec::new()));
        assert_eq!(st.officials_for_result(10), None);

        st.officials_store().set_enabled(true);

        // BTP kennt die Besetzung bereits — die gewinnt.
        m.official1_id = Some(3);
        st.set_snapshot(snapshot(vec![m.clone()], Vec::new()));
        assert_eq!(st.officials_for_result(10), Some((3, 0)));

        // BTP kennt nichts, aber lokal ist eine Zuweisung vorgemerkt.
        m.official1_id = None;
        st.set_snapshot(snapshot(vec![m.clone()], Vec::new()));
        st.officials_store()
            .assign(10, crate::tablet::officials::OfficialRole::Sr, 4);
        assert_eq!(st.officials_for_result(10), Some((4, 0)));

        // Gar nichts bekannt: explizit „niemand" (0, 0), nicht None — sonst
        // bliebe der Request unverändert und ein späterer BTP-Eintrag würde
        // nie überschrieben.
        let unbekannt = match_on(11, Some(6), MatchStatus::OnCourt);
        st.set_snapshot(snapshot(vec![unbekannt], Vec::new()));
        assert_eq!(st.officials_for_result(11), Some((0, 0)));
    }

    #[test]
    fn snapshot_bindet_die_auto_vergabe_ausnahmeliste_ans_turnier() {
        // Spec `feldvergabe-ausnahme`, Muster ADR 0022: Turnier binden, beim
        // Wechsel wird der Stand verworfen.
        let dir = tempfile::tempdir().unwrap();
        let st = TabletState::default();
        st.set_auto_assign_exclusions_path(dir.path().join("excluded-matches.json"));

        st.set_snapshot(snap_named("Cup A"));
        st.set_auto_assign_excluded(10, true);
        assert!(st.auto_assign_excluded(10));

        // Derselbe Turniername im nächsten Snapshot lässt die Ausnahme
        // stehen …
        st.set_snapshot(snap_named("Cup A"));
        assert!(st.auto_assign_excluded(10));

        // … ein Turnierwechsel verwirft sie.
        st.set_snapshot(snap_named("Cup B"));
        assert!(!st.auto_assign_excluded(10));
    }

    #[test]
    fn queue_reorder_never_backfills_matches_beyond_what_tl_web_could_show() {
        // Code-Review-Fund 14.08.2026: TL-Web zeigt nur die ersten
        // `tl::QUEUE_LIMIT` (120) Spiele. Ohne Deckel würde ein Zug
        // ans (dort unsichtbare) Ende der VOLLEN Liste auch Spiele jenseits
        // der 120 in den Präfix ziehen, die niemand gesehen hat.
        let matches: Vec<BtpMatch> = (1..=125)
            .map(|id| match_on(id, None, MatchStatus::Scheduled))
            .collect();
        let st = TabletState::default();
        st.set_snapshot(snapshot(matches, Vec::new()));

        // Match 119 liegt innerhalb der sichtbaren ersten 120 — ans Ende
        // ziehen (before=None).
        assert!(st.queue_reorder(&crate::config::AppConfig::default(), 119, None));

        assert_eq!(
            st.queue_order_store().rank(121),
            None,
            "Match 121 lag jenseits der TL-Web-Kappungsgrenze — darf nicht in den Präfix gezogen werden"
        );
        assert_eq!(st.queue_order_store().rank(125), None);
        // Das gezogene Match selbst landet weiterhin im Präfix.
        assert!(st.queue_order_store().rank(119).is_some());
    }

    #[test]
    fn btp_retry_persist_and_reload_roundtrip() {
        // App-Neustart: gefüllte Queue sichern, frische Instanz mit gleichem
        // Turnier lädt sie identisch (inkl. player_ids + enqueued_ms).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("btp-retry.json");

        let st = TabletState::default();
        st.set_btp_retry_path(path.clone());
        st.set_snapshot(snap_named("Cup A"));
        let mut u = upd(7, 42);
        u.player_ids = vec![55, 66];
        u.end_ts_ms = Some(1_700_000_000_000);
        st.queue_btp_retry(u, 4_242);
        assert!(path.exists(), "queue_btp_retry schreibt die Datei");

        let st2 = TabletState::default();
        st2.set_btp_retry_path(path.clone());
        st2.set_snapshot(snap_named("Cup A"));
        let q = st2.btp_retries();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].update.btp_match_id, 7);
        assert_eq!(q[0].update.duration_mins, 42);
        assert_eq!(q[0].update.player_ids, vec![55, 66]);
        assert_eq!(q[0].update.end_ts_ms, Some(1_700_000_000_000));
        assert_eq!(q[0].enqueued_ms, 4_242, "Bezugszeitpunkt bleibt erhalten");
    }

    #[test]
    fn btp_retry_discards_foreign_tournament() {
        // Turnier-Guard: unter „Cup A" persistiert, Load mit „Cup B" verwirft
        // (BTP-match_id kollidieren sonst über Turniere hinweg).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("btp-retry.json");

        let st = TabletState::default();
        st.set_btp_retry_path(path.clone());
        st.set_snapshot(snap_named("Cup A"));
        st.queue_btp_retry(upd(7, 30), 1_000);

        let st2 = TabletState::default();
        st2.set_btp_retry_path(path.clone());
        st2.set_snapshot(snap_named("Cup B"));
        assert!(
            st2.btp_retries().is_empty(),
            "fremdes Turnier → Queue verworfen"
        );
    }

    #[test]
    fn btp_retry_missing_file_is_empty() {
        // Erster Start: Datei fehlt → leere Queue, kein Fehler.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("btp-retry.json");
        let st = TabletState::default();
        st.set_btp_retry_path(path);
        st.set_snapshot(snap_named("Cup A"));
        assert!(st.btp_retries().is_empty());
    }

    #[test]
    fn btp_retry_corrupt_file_is_empty_no_panic() {
        // Halbe/kaputte JSON → leere Queue + warn, kein Panic.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("btp-retry.json");
        std::fs::write(&path, b"{ \"tournament\": \"Cup A\", \"entr").unwrap();
        let st = TabletState::default();
        st.set_btp_retry_path(path);
        st.set_snapshot(snap_named("Cup A"));
        assert!(st.btp_retries().is_empty());
    }

    #[test]
    fn btp_retry_clear_persists_shrunken_queue() {
        // clear_btp_retry schreibt synchron die verkleinerte Datei.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("btp-retry.json");

        let st = TabletState::default();
        st.set_btp_retry_path(path.clone());
        st.set_snapshot(snap_named("Cup A"));
        st.queue_btp_retry(upd(7, 30), 1_000);
        st.queue_btp_retry(upd(8, 20), 2_000);
        st.clear_btp_retry(7);

        let st2 = TabletState::default();
        st2.set_btp_retry_path(path.clone());
        st2.set_snapshot(snap_named("Cup A"));
        let q = st2.btp_retries();
        assert_eq!(q.len(), 1, "nach clear nur noch ein Eintrag auf Platte");
        assert_eq!(q[0].update.btp_match_id, 8);
    }

    #[test]
    fn btp_retry_load_merges_keeping_fresh_entries() {
        // Merge statt Replace: ein zwischen Start und erstem Snapshot frisch
        // eingereihter Eintrag überlebt das Laden (frisch gewinnt).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("btp-retry.json");

        // Alt-Stand auf Platte: Match 7, duration 30.
        let st = TabletState::default();
        st.set_btp_retry_path(path.clone());
        st.set_snapshot(snap_named("Cup A"));
        st.queue_btp_retry(upd(7, 30), 1_000);

        // „Neustart": Match 7 (duration 99) frisch eingereiht, BEVOR der erste
        // Snapshot das Laden auslöst.
        let st2 = TabletState::default();
        st2.set_btp_retry_path(path.clone());
        st2.queue_btp_retry(upd(7, 99), 5_000);
        st2.set_snapshot(snap_named("Cup A"));
        let q = st2.btp_retries();
        assert_eq!(q.len(), 1);
        assert_eq!(
            q.iter()
                .find(|e| e.update.btp_match_id == 7)
                .unwrap()
                .update
                .duration_mins,
            99,
            "frisch Eingereihtes wird nicht vom Alt-Stand überschrieben"
        );
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

    // ───────────── Court-Monitor-Nudge (A1, ADR 0016) ─────────────

    /// Liest den `court`/`seq` aus einem Nudge-JSON.
    fn parse_nudge(json: &str) -> (i64, u64) {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        (v["court"].as_i64().unwrap(), v["seq"].as_u64().unwrap())
    }

    /// Legt einen Monitor-Kanal an, abonniert ihn und gibt das Empfangs-Ende
    /// zurück (der State hält eine Sender-Klon-Referenz). Kapselt das seit dem
    /// Fan-out-Deckel geänderte `subscribe_monitor(court, &tx)`-Muster.
    fn sub_monitor(st: &TabletState, court: Option<i64>) -> MonitorNudgeRx {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(
            st.subscribe_monitor(court, &tx),
            "unter dem Deckel akzeptiert"
        );
        rx
    }

    #[test]
    fn notify_monitor_wakes_only_the_courts_subscribers_and_the_all_list() {
        // Broker-Routing: `notify_monitor(5)` weckt GENAU die Court-5-Anzeige
        // und die „alle Felder"-Übersicht — die Court-3-Anzeige bleibt still.
        let st = TabletState::default();
        let mut sub5 = sub_monitor(&st, Some(5));
        let mut sub3 = sub_monitor(&st, Some(3));
        let mut sub_all = sub_monitor(&st, None);

        st.notify_monitor(5);

        // Court-5-Abonnent bekommt den Nudge fürs Feld 5.
        let (court, seq) = parse_nudge(&sub5.try_recv().expect("Court-5 wird geweckt"));
        assert_eq!((court, seq), (5, 1));
        // „Alle Felder"-Abonnent ebenfalls.
        let (court_all, _) = parse_nudge(&sub_all.try_recv().expect("Übersicht wird geweckt"));
        assert_eq!(court_all, 5);
        // Court-3-Abonnent NICHT.
        assert!(sub3.try_recv().is_err(), "Feld 3 bleibt unberührt");
    }

    #[test]
    fn monitor_seq_is_monotonic_per_court() {
        // `seq` steigt je Feld streng monoton und zählt getrennt je Feld.
        let st = TabletState::default();
        let mut a = sub_monitor(&st, Some(1));
        let mut b = sub_monitor(&st, Some(2));

        st.notify_monitor(1);
        st.notify_monitor(1);
        st.notify_monitor(2);

        assert_eq!(parse_nudge(&a.try_recv().unwrap()).1, 1);
        assert_eq!(parse_nudge(&a.try_recv().unwrap()).1, 2);
        // Feld 2 hat seinen eigenen Zähler, beginnt also wieder bei 1.
        assert_eq!(parse_nudge(&b.try_recv().unwrap()).1, 1);
    }

    #[test]
    fn dead_monitor_subscriber_is_pruned_on_next_notify() {
        // Subscribe/Unsubscribe-Lebenszyklus: Fällt die Anzeige weg (Rx
        // fallengelassen), siebt der nächste Nudge den toten Sender aus —
        // die interne Liste des Felds verschwindet danach ganz.
        let st = TabletState::default();
        let sub = sub_monitor(&st, Some(7));
        assert_eq!(
            st.monitor_subs.read().unwrap().get(&7).map(|v| v.len()),
            Some(1)
        );

        drop(sub); // Anzeige verschwindet.
        st.notify_monitor(7);

        assert!(
            st.monitor_subs.read().unwrap().get(&7).is_none(),
            "toter Abonnent ausgesiebt, leere Liste entfernt"
        );
    }

    #[test]
    fn unsubscribe_monitor_removes_only_the_own_sender() {
        // Explizites Austragen (Verbindungsende) entfernt GENAU den eigenen
        // Sender; ein zweiter Abonnent desselben Felds bleibt bestehen.
        let st = TabletState::default();
        let (tx_a, _rx_a) = tokio::sync::mpsc::unbounded_channel();
        let (tx_b, _rx_b) = tokio::sync::mpsc::unbounded_channel();
        assert!(st.subscribe_monitor(Some(4), &tx_a));
        assert!(st.subscribe_monitor(Some(4), &tx_b));

        st.unsubscribe_monitor(Some(4), &tx_a);

        let subs = st.monitor_subs.read().unwrap();
        let list = subs.get(&4).expect("Feld-Liste bleibt (tx_b noch drin)");
        assert_eq!(list.len(), 1, "nur tx_a ausgetragen");
        assert!(list[0].same_channel(&tx_b), "der verbliebene ist tx_b");
    }

    #[test]
    fn monitor_fanout_cap_rejects_the_over_limit_subscription() {
        // Fan-out-Deckel (N5): Bis exakt `MAX_MONITOR_SUBS` werden Abos
        // eingetragen; das (N+1)-te wird abgelehnt (Zuschauer-DoS-Schutz).
        let st = TabletState::default();
        // Rx-Enden lebendig halten, sonst gälten die Sender als tot.
        let mut keep = Vec::new();
        for _ in 0..MAX_MONITOR_SUBS {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            keep.push(rx);
            assert!(
                st.subscribe_monitor(Some(1), &tx),
                "unter dem Deckel akzeptiert"
            );
        }
        let (tx_over, _rx_over) = tokio::sync::mpsc::unbounded_channel();
        assert!(
            !st.subscribe_monitor(Some(1), &tx_over),
            "über dem Deckel abgelehnt"
        );
        let total: usize = st.monitor_subs.read().unwrap().values().map(Vec::len).sum();
        assert_eq!(total, MAX_MONITOR_SUBS, "genau der Deckel ist eingetragen");
    }
}
