//! Sync-Engine: ein Poll-Push-Zyklus BTP → Badhub.
//!
//! Strategie bei Push-Fehlern (Resend-on-failure): Es gibt keine
//! persistente Outbox. Schlägt ein Push fehl, wird der zuletzt gesendete
//! Stand verworfen – der nächste Zyklus sendet dann einen vollen `tset`
//! mit dem aktuellen Komplettstand. Die Turnierdaten liegen ohnehin in
//! BTP und werden bei jedem Zyklus neu abgefragt.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::badhub::diff::{diff, roster_update, Update};
use crate::badhub::payload::{build_checkin_roster, CheckinRosterMessage};
use crate::badhub::push;
use crate::btp::client;
use crate::btp::model::{BtpSnapshot, MatchResult, MatchStatus};
use crate::config::AppConfig;
use crate::tablet::state::TabletState;

/// Abstand der Heartbeats: Hat sich am Turnierstand >60 s nichts geändert,
/// sendet die Sync-Engine trotzdem einen vollen `tset` als Lebenszeichen.
/// So erkennt badhub.de ein laufendes Turnier als „live", auch wenn gerade
/// keine Punkte fallen — und meldet es als beendet, sobald bts-light
/// schließt und die Heartbeats ausbleiben.
const HEARTBEAT_AFTER: Duration = Duration::from_secs(60);

/// Aktuelle Zeit in Unix-Millisekunden.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Die Spieler-Identität für die Verfügbarkeitsprüfung liegt in
// `tablet::assign` — geteilt mit der Turnierleitungs-Anzeige, damit beide
// dieselbe Person meinen.
use crate::tablet::assign::player_key;

/// Ergebnis eines Sync-Zyklus.
#[derive(Debug)]
pub enum SyncOutcome {
    /// Ein vollständiger `tset` wurde gesendet.
    PushedFull,
    /// Ein kleines `tupdate_match` wurde gesendet.
    PushedUpdate,
    /// Keine Änderung – nichts gesendet.
    Idle,
    /// Ansage-Slave-Modus: BTP gelesen, nur lokal angesagt (kein Push/Vergabe).
    SlaveActive,
    /// BTP nicht erreichbar oder Antwort unbrauchbar.
    BtpError(String),
    /// Verdächtig leerer BTP-Snapshot verworfen (Leer-Snapshot-Guard);
    /// der Zyklus hat keinerlei Zustand verändert.
    SnapshotDiscarded,
    /// Push an Badhub fehlgeschlagen.
    PushError(String),
}

/// Hält den Zustand zwischen den Zyklen.
pub struct SyncEngine {
    /// Zuletzt erfolgreich gesendeter Stand; `None` erzwingt einen vollen
    /// `tset` (erster Lauf oder nach einem Push-Fehler).
    last_pushed: Option<BtpSnapshot>,
    /// Fortlaufende Request-ID für Badhub.
    rid: u64,
    /// Match-ID → Zeitpunkt, zu dem das Match erstmals als beendet
    /// erkannt wurde. BTP liefert keinen End-Zeitstempel, deshalb wird er
    /// hier über die Zyklen hinweg gemerkt.
    finished_at: HashMap<i64, u64>,
    /// Zeitpunkt des letzten tatsächlich gesendeten Pushes (echtes Update
    /// oder Heartbeat). Steuert, wann das nächste Lebenszeichen fällig ist.
    last_push_at: Option<Instant>,
    /// Zuletzt geloggte Turnier-Topologie (Hallen, Felder, Matches) –
    /// das Diagnose-Log nennt sie nur bei Änderung, nicht jeden Zyklus.
    last_topology: Option<(usize, usize, usize)>,
    /// CourtID → Match-ID des im letzten Zyklus dort OnCourt gewesenen
    /// Spiels. Wechselt das (Spiel verlässt das Feld) und ist es beendet,
    /// merkt sich der State den Verlierer als Zähltafelbediener fürs Feld.
    oncourt_prev: HashMap<i64, i64>,
    /// Derselbe Vorher-Stand, aber für die Schiedsrichter-Rotation: Sie
    /// bestückt nur **neu** belegte Felder, und ihr Hook läuft an anderer
    /// Stelle im Zyklus als der der Zähltafelbediener. Ein gemeinsamer
    /// Merker würde beide aneinanderketten.
    officials_oncourt_prev: HashMap<i64, i64>,
    /// Zuletzt nach BTP geschriebene Besetzung je Match (ADR 0021).
    /// BTP übernimmt asynchron; ohne diesen Merker schriebe jeder Zyklus
    /// denselben Wert erneut, bis der Snapshot nachzieht.
    officials_written: HashMap<i64, (i64, i64)>,
    /// CourtID → Zeitpunkt (Unix-ms), seit dem ein Feld frei ist (kein Match
    /// referenziert es). Grundlage der Wartezeit der automatischen Feldvergabe.
    court_free_since: HashMap<i64, u64>,
    /// Schon automatisch vergebene, aber von BTP noch nicht bestätigte
    /// Zuweisungen: CourtID → (Match-ID, Versand-Zeitpunkt). Verhindert, dass
    /// dasselbe Match/Feld erneut vergeben wird, bevor der BTP-Write im
    /// nächsten Poll sichtbar ist (sonst Doppelvergabe bei langsamem BTP).
    /// Einträge fallen weg, sobald das Feld belegt erscheint oder nach
    /// [`PENDING_AUTO_TTL`] (fehlgeschlagener Write → erneuter Versuch).
    pending_auto: HashMap<i64, (i64, u64)>,
    /// Zeitpunkt des letzten Nachschub-Flushs (A5) — drosselt die
    /// Wiederholversuche auf [`BTP_RETRY_FLUSH_EVERY`].
    last_btp_retry_flush: Option<Instant>,
    /// Leer-Snapshot-Guard: Hat diese Sitzung schon einen Snapshot MIT
    /// Matches gesehen? Nur dann ist ein plötzlich leerer Stand verdächtig.
    seen_matches: bool,
    /// Anzahl der direkt aufeinanderfolgenden leeren Snapshots, die der
    /// Guard bereits verworfen hat.
    suspect_empty_polls: u32,
    /// Match-IDs, für die in BTP zuletzt `Highlight:1` geschrieben wurde
    /// (P1). Grundlage der Highlight-Reconciliation: nur der Diff zum
    /// aktuellen Aufruf-Stand wird nach BTP geschrieben.
    highlight_written: HashSet<i64>,
    /// Zuletzt erfolgreich gesendete Meldeliste (Hallen-Check-In, ADR 0009).
    /// `None` = in dieser Sitzung noch nichts gesendet → der nächste Zyklus
    /// schickt sie vollständig. Getrennt von `last_pushed`, weil Meldeliste
    /// und Liveticker unabhängig voneinander scheitern dürfen.
    last_roster: Option<CheckinRosterMessage>,
    /// Wann hat badhub den Check-In-Endpunkt zuletzt mit 404/400 abgelehnt?
    /// Dann läuft dort eine ältere Version ohne das Feature. Statt jeden
    /// Zyklus erneut anzuklopfen, wird für [`CHECKIN_UNSUPPORTED_RETRY`]
    /// pausiert — aber nicht dauerhaft aufgegeben: derselbe Status kann von
    /// einem kurzen Aussetzer während eines badhub-Deploys stammen, und ein
    /// Turnier läuft über mehrere Tage.
    checkin_unsupported_since: Option<Instant>,
}

/// Wie lange nach einem 404/400 des Check-In-Endpunkts pausiert wird, bevor
/// erneut angeklopft wird. Lang genug, dass ein altes badhub kein Log
/// vollschreibt; kurz genug, dass ein badhub-Deploy während eines
/// mehrtägigen Turniers noch am selben Tag greift.
const CHECKIN_UNSUPPORTED_RETRY: Duration = Duration::from_secs(30 * 60);

/// Wie lange eine offene Auto-Zuweisung als „unterwegs" gilt, bevor sie als
/// fehlgeschlagen verworfen und neu versucht wird (BTP-Write nicht sichtbar).
const PENDING_AUTO_TTL: Duration = Duration::from_secs(30);

/// Leer-Snapshot-Guard: Beim wievielten aufeinanderfolgenden leeren Abruf
/// der leere Stand als echt übernommen wird. 2 = ein einzelner leerer
/// Abruf wird verworfen, die Bestätigung im Folge-Poll übernommen
/// (Turnier-Befund 19.07.: BTP lieferte 2× je EINEN Abruf lang
/// „0 Hallen/Felder/Matches" → Massen-Freigabe aller Felder).
const EMPTY_CONFIRM_POLLS: u32 = 2;

/// Nachschub-Queue (A5): frühestens alle 30 s einen Flush versuchen —
/// der Poll-Zyklus läuft alle ~2 s, ein strauchelndes BTP soll nicht im
/// Sekundentakt mit Login+SENDUPDATE beharkt werden.
const BTP_RETRY_FLUSH_EVERY: Duration = Duration::from_secs(30);

/// Spieler-Checkout-Fenster (Tilos 5-Minuten-Guard): Wird ein Ergebnis
/// später als 5 min nach Spielende nachgeschoben, bleibt der
/// Players-Block weg — sonst würde ein Replay die Spieler erneut
/// auschecken/umstempeln, obwohl sie längst im nächsten Spiel stecken.
const PLAYER_CHECKOUT_WINDOW: Duration = Duration::from_secs(5 * 60);

/// Höchst-Lebensdauer eines Queue-Eintrags — danach ist das Turnier
/// vorbei bzw. der Fall manuell geklärt; ein Uralt-Replay wäre nur
/// noch riskant.
const BTP_RETRY_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Entscheidung der Nachschub-Queue für EINEN Eintrag (rein, testbar).
#[derive(Debug, PartialEq)]
enum RetryAction {
    /// Eintrag verwerfen (Grund fürs Log).
    Drop(&'static str),
    /// Diesen (ggf. entschärften) Stand nach BTP schreiben.
    Write(Box<crate::btp::proto::MatchUpdate>),
}

/// Bereitet einen Nachschub-Write vor — oder verwirft ihn:
/// - BTP kennt für das Match bereits ein Ergebnis (z. B. von der
///   Turnierleitung manuell nachgetragen) → NIE überschreiben.
/// - Älter als [`BTP_RETRY_MAX_AGE`] → verwerfen.
/// - Außerhalb des [`PLAYER_CHECKOUT_WINDOW`] → Players-Block entfernen
///   (Tilos 5-Minuten-Guard gegen späte Spieler-Replays).
/// - Feld-Freigabe nur, wenn das Feld laut Snapshot noch UNSER Match
///   trägt — sonst würde das Replay einem inzwischen neu belegten Feld
///   die frische Zuweisung wegräumen.
fn prepare_btp_retry(
    entry: &crate::tablet::state::PendingBtpWrite,
    snapshot: &BtpSnapshot,
    now: u64,
    tablet: &TabletState,
) -> RetryAction {
    // winner.is_some() impliziert im Modell Finished (model.rs setzt den
    // Status genau dann) — der Sieger allein ist das Kriterium.
    let already_decided = snapshot
        .matches
        .iter()
        .any(|m| m.id == entry.update.btp_match_id && m.winner.is_some());
    if already_decided {
        return RetryAction::Drop("BTP hat bereits ein Ergebnis");
    }
    let age = now.saturating_sub(entry.enqueued_ms);
    if age > BTP_RETRY_MAX_AGE.as_millis() as u64 {
        return RetryAction::Drop("Eintrag zu alt");
    }
    let mut update = entry.update.clone();
    if age > PLAYER_CHECKOUT_WINDOW.as_millis() as u64 {
        update.player_ids.clear();
        update.end_ts_ms = None;
    }
    if let Some(fc) = update.free_court_id {
        let still_ours = snapshot
            .matches
            .iter()
            .any(|m| m.id == update.btp_match_id && m.court_id == Some(fc));
        if !still_ours {
            update.free_court_id = None;
        }
    }
    // Besetzung neu abfragen statt die beim Einreihen eingefrorene zu
    // wiederholen (Code-Review-Fund 14.08.2026): Zwischen Einreihen und
    // Nachschub kann die Turnierleitung die Zuweisung korrigiert haben —
    // der veraltete Wert würde die Korrektur sonst stillschweigend
    // überschreiben.
    if update.officials.is_some() {
        update.officials = tablet.officials_for_result(update.btp_match_id);
    }
    RetryAction::Write(Box::new(update))
}

/// Die Schiedsrichter-Änderungen, die noch nach BTP müssen (ADR 0021).
///
/// Rein und damit testbar: Soll-Stand ist die **wirksame** Besetzung
/// (BTP gewinnt, sonst die lokale Zuweisung), Ist-Stand das, was der
/// Snapshot am Match trägt. Geschrieben wird nur der Unterschied — und
/// nicht erneut, was schon geschrieben und von BTP noch nicht
/// zurückgemeldet wurde (`geschrieben`); BTP übernimmt asynchron (≤1 s,
/// Messung 13.08.2026), sonst liefe jeder Zyklus in denselben Write.
///
/// **Schreibt immer die aktuelle `CourtID` mit** (Live-Befund 14.08.2026,
/// siehe [`MatchCourt::court_id`]): Ein früherer, das Feld weglassender
/// Write liess BTP beobachtbar die gerade erst angekommene Feldzuweisung
/// wieder verlieren, wenn er kurz nach ihr auf demselben Match landete —
/// unabhängig davon, wie kurz „kurz" war (eine testweise Karenzzeit von
/// 10 s reichte am laufenden Turnier nicht). Reasserted der Write
/// stattdessen dieselbe `CourtID`, die der Snapshot gerade zeigt, ist die
/// Reihenfolge der beiden Requests folgenlos — eine Wartezeit erübrigt
/// sich.
fn officials_entries(
    tablet: &TabletState,
    snapshot: &BtpSnapshot,
    geschrieben: &HashMap<i64, (i64, i64)>,
) -> Vec<crate::btp::proto::MatchCourt> {
    let store = tablet.officials_store();
    if !store.enabled() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for m in &snapshot.matches {
        let Some((sr, ar)) = tablet.officials_for_write(m) else {
            continue;
        };
        let ist = (m.official1_id.unwrap_or(0), m.official2_id.unwrap_or(0));
        if ist == (sr, ar) || geschrieben.get(&m.id) == Some(&(sr, ar)) {
            continue;
        }
        out.push(crate::btp::proto::MatchCourt {
            match_id: m.id,
            draw_id: m.draw_id,
            planning_id: m.planning_id,
            court_id: m.court_id.unwrap_or(0),
            officials: Some((sr, ar)),
        });
    }
    out
}

/// Gewünschter Highlight-Stand (P1): Match-IDs, die gerufen sind UND im
/// Snapshot noch ruf-bar (Scheduled, beide Mannschaften stehen). Aufs Feld
/// gerufene/beendete Spiele fallen so automatisch heraus → Highlight:0. Rein.
fn highlight_desired(
    calls: &[crate::tablet::state::PreparationCall],
    snapshot: &BtpSnapshot,
) -> HashSet<i64> {
    calls
        .iter()
        .filter_map(|c| snapshot.matches.iter().find(|m| m.id == c.match_id))
        .filter(|m| {
            m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
        })
        .map(|m| m.id)
        .collect()
}

/// Diff `desired` gegen `written` → nur die geänderten Matches als
/// `HighlightEntry` (Identität aus dem Snapshot). Matches, die nicht mehr im
/// Snapshot stehen, werden ausgelassen (kein Knoten baubar). Rein & testbar.
fn highlight_entries(
    desired: &HashSet<i64>,
    written: &HashSet<i64>,
    snapshot: &BtpSnapshot,
) -> Vec<crate::btp::proto::HighlightEntry> {
    snapshot
        .matches
        .iter()
        .filter_map(|m| {
            let want = desired.contains(&m.id);
            if want == written.contains(&m.id) {
                return None;
            }
            Some(crate::btp::proto::HighlightEntry {
                match_id: m.id,
                draw_id: m.draw_id,
                planning_id: m.planning_id,
                on: want,
            })
        })
        .collect()
}

impl Default for SyncEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            last_pushed: None,
            rid: 1,
            finished_at: HashMap::new(),
            last_push_at: None,
            last_topology: None,
            oncourt_prev: HashMap::new(),
            officials_oncourt_prev: HashMap::new(),
            officials_written: HashMap::new(),
            court_free_since: HashMap::new(),
            pending_auto: HashMap::new(),
            last_btp_retry_flush: None,
            seen_matches: false,
            suspect_empty_polls: 0,
            highlight_written: HashSet::new(),
            last_roster: None,
            checkin_unsupported_since: None,
        }
    }

    /// Entscheidet, ob in diesem Zyklus eine Meldeliste zu senden ist, und
    /// baut sie — ohne Netzwerkzugriff.
    ///
    /// Getrennt vom Senden, weil der Snapshot dafür gebraucht wird, der
    /// eigentliche Versand aber erst **nach** dem Liveticker-Push laufen darf.
    fn plan_checkin_roster(
        &self,
        config: &AppConfig,
        snapshot: &BtpSnapshot,
    ) -> Option<CheckinRosterMessage> {
        if !config.checkin.is_ready() || self.checkin_retry_pending() {
            return None;
        }
        let roster = build_checkin_roster(snapshot, &config.checkin.tournament_uuid, self.rid);
        // Nur bei echter Änderung senden — die Meldeliste sind Stammdaten,
        // kein Live-Stand (kein Heartbeat).
        roster_update(self.last_roster.as_ref(), roster)
    }

    /// Hat badhub den Check-In abgelehnt und die Sperrfrist läuft noch?
    fn checkin_retry_pending(&self) -> bool {
        self.checkin_unsupported_since
            .is_some_and(|t| t.elapsed() < CHECKIN_UNSUPPORTED_RETRY)
    }

    /// Sendet die vorbereitete Meldeliste (ADR 0009).
    ///
    /// Bewusst mit **eigenem Fehlerpfad**: Der Check-In ist additiv — geht er
    /// schief, muss der Liveticker trotzdem laufen. Fehler werden deshalb nur
    /// geloggt und ändern den [`SyncOutcome`] nicht.
    async fn send_checkin_roster(
        &mut self,
        config: &AppConfig,
        http: &reqwest::Client,
        roster: CheckinRosterMessage,
    ) {
        match push::push_checkin_roster(http, &config.badhub.url, &config.badhub.password, &roster)
            .await
        {
            Ok(()) => {
                tracing::debug!(
                    klassen = roster.classes.len(),
                    meldungen = roster.entries.len(),
                    "Meldeliste an badhub gesendet"
                );
                self.last_roster = Some(roster);
                self.checkin_unsupported_since = None;
            }
            // badhub kennt den Nachrichtentyp nicht → dort läuft eine ältere
            // Version (bts-light kommt per Auto-Update auf alle
            // Installationen, badhub deployt unabhängig). Nicht jeden Zyklus
            // erneut anklopfen — aber auch nicht für immer aufgeben: derselbe
            // Status kann von einem kurzen Aussetzer während eines
            // badhub-Deploys stammen, und ein Turnier läuft über Tage.
            Err(push::PushError::Status(404)) | Err(push::PushError::Status(400)) => {
                if self.checkin_unsupported_since.is_none() {
                    tracing::info!(
                        "badhub kennt den Hallen-Check-In noch nicht — naechster Versuch in {} Minuten",
                        CHECKIN_UNSUPPORTED_RETRY.as_secs() / 60
                    );
                }
                self.checkin_unsupported_since = Some(Instant::now());
            }
            Err(e) => {
                // last_roster bleibt unverändert → der nächste Zyklus versucht
                // es erneut mit der vollständigen Liste.
                tracing::warn!("Meldeliste konnte nicht gesendet werden: {e}");
            }
        }
    }

    /// Highlight-Reconciliation (P1): macht „in Vorbereitung"-Aufrufe in BTP
    /// sichtbar. Vergleicht die Menge aktuell gerufener, noch ruf-barer Spiele
    /// (Scheduled, Paarung steht) mit dem zuletzt geschriebenen Stand und
    /// schreibt NUR den Diff — `Highlight:1` für neu gerufene, `Highlight:0`
    /// für nicht mehr gerufene (zurückgenommen / aufs Feld gerufen / beendet).
    /// Läuft im Master-Zyklus, wenn BTP nachweislich erreichbar ist; kein
    /// Schreiben, solange sich nichts geändert hat. Fehler sind nicht fatal —
    /// der Stand wird dann NICHT übernommen, sodass der nächste Zyklus es
    /// erneut versucht.
    async fn reconcile_highlights(
        &mut self,
        config: &AppConfig,
        tablet: &TabletState,
        snapshot: &BtpSnapshot,
    ) {
        // Gewünschter Stand: gerufene Matches, die im Snapshot noch ruf-bar sind.
        let desired = highlight_desired(&tablet.preparation_calls(), snapshot);
        if desired == self.highlight_written {
            return; // nichts zu tun – kein BTP-Write
        }
        // Diff → HighlightEntry (Identität aus dem Snapshot).
        let entries = highlight_entries(&desired, &self.highlight_written, snapshot);
        if entries.is_empty() {
            // Alle Diffs betrafen Matches, die nicht mehr im Snapshot stehen
            // (z. B. gelöscht) — Stand trotzdem übernehmen, um erneute Versuche
            // zu vermeiden.
            self.highlight_written = desired;
            return;
        }
        match crate::tablet::server::write_highlight_to_btp(config, &entries).await {
            Ok(()) => {
                tracing::info!(
                    "BTP-Highlight aktualisiert: {} Änderung(en) ({} gerufen)",
                    entries.len(),
                    desired.len()
                );
                self.highlight_written = desired;
            }
            // Nicht übernehmen → nächster Zyklus versucht es erneut.
            Err(e) => tracing::warn!("BTP-Highlight-Update fehlgeschlagen: {e}"),
        }
    }

    /// Schiedsrichter-Zuweisungen nach BTP zurückschreiben (ADR 0021).
    ///
    /// Muster [`reconcile_highlights`]: nur der Unterschied geht raus, der
    /// Stand wird **nur bei `Ok`** übernommen — ein fehlgeschlagener Write
    /// wird im nächsten Zyklus wiederholt. BTP übernimmt asynchron (≤ 1 s,
    /// Messung 13.08.2026), deshalb der Merker: Ohne ihn schriebe jeder
    /// Zyklus dasselbe erneut, bis der Snapshot nachzieht.
    async fn reconcile_officials(
        &mut self,
        config: &AppConfig,
        tablet: &TabletState,
        snapshot: &BtpSnapshot,
    ) {
        // Erst loslassen, was BTP inzwischen zeigt (R2) — sonst schriebe der
        // Diff unten eine spätere Änderung IN BTP wieder zurück.
        let store = tablet.officials_store();
        for m in &snapshot.matches {
            store.confirm(m.id, m.official1_id, m.official2_id);
        }
        let entries = officials_entries(tablet, snapshot, &self.officials_written);
        if entries.is_empty() {
            // Aufräumen: Was BTP inzwischen trägt, muss nicht länger als
            // „geschrieben" gemerkt werden (sonst wüchse die Karte über das
            // Turnier).
            self.officials_written
                .retain(|id, _| snapshot.matches.iter().any(|m| m.id == *id));
            return;
        }
        match crate::tablet::server::write_officials_to_btp(config, &entries).await {
            Ok(()) => {
                tracing::info!(
                    "BTP-Schiedsrichter aktualisiert: {} Änderung(en): {}",
                    entries.len(),
                    entries
                        .iter()
                        .map(|e| format!(
                            "Spiel {} (Feld {})→{:?}",
                            e.match_id, e.court_id, e.officials
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                for e in &entries {
                    if let Some(officials) = e.officials {
                        self.officials_written.insert(e.match_id, officials);
                    }
                }
            }
            // Nicht übernehmen → nächster Zyklus versucht es erneut.
            Err(e) => tracing::warn!("BTP-Schiedsrichter-Update fehlgeschlagen: {e}"),
        }
    }

    /// Aufräumen der Auto-Vergabe-Ausnahmeliste (Spec
    /// `feldvergabe-ausnahme`): Eine Ausnahme wird entfernt, sobald das Match
    /// `Finished` ist (deckt auch Walkover/Retired ab, die in BTP ebenfalls
    /// über `Finished` + `Winner` laufen, keine eigene `MatchStatus`-Variante)
    /// oder nicht mehr im Snapshot vorkommt. Kein BTP-Write, rein lokal —
    /// anders als die übrigen `reconcile_*` deshalb nicht `async`.
    fn reconcile_auto_assign_exclusions(&self, tablet: &TabletState, snapshot: &BtpSnapshot) {
        let keep: HashSet<i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status != MatchStatus::Finished)
            .map(|m| m.id)
            .collect();
        tablet.retain_auto_assign_exclusions(&keep);
    }

    /// Spielzeiten-Messung je Poll abgleichen (Spec `spielzeiten-prognose`,
    /// E4): Alle OnCourt-Matches werden — nur wenn noch kein Stempel steht —
    /// mit ihrer ersten Feldzuweisung gestempelt; Matches, die BTP wieder als
    /// `Scheduled` ohne Feld führt, zählen Richtung bestätigter Abnahme
    /// (Reset nach [`match_times::DEASSIGN_CONFIRM_POLLS`] Polls). Finished
    /// steht in keinem der beiden Sets und setzt den Zähler nur zurück.
    /// Kein BTP-Write, rein lokal; die Reset-Logik selbst ist im Store
    /// getestet.
    fn reconcile_match_times(&self, tablet: &TabletState, snapshot: &BtpSnapshot, now: u64) {
        let assigned: Vec<(i64, &str, &str)> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::OnCourt && m.court_id.is_some())
            .map(|m| (m.id, m.class_label.as_str(), m.discipline.as_str()))
            .collect();
        let deassigned: HashSet<i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::Scheduled && m.court_id.is_none())
            .map(|m| m.id)
            .collect();
        tablet
            .match_times_store()
            .reconcile(&assigned, &deassigned, now);
    }

    /// Aufräumen der manuellen Spielreihenfolge (Spec
    /// `spielliste-manuelle-reihenfolge`, ADR 0026): Ein Match bleibt in
    /// der Reihenfolge, solange es spielbereit und noch nicht zugewiesen
    /// ist. Wird es zugewiesen/beendet oder verschwindet es aus dem
    /// Snapshot, steht es nicht mehr im `keep`-Set und fällt heraus.
    ///
    /// Ein **Hallenwechsel** räumt seit ADR 0026 nichts mehr auf — die
    /// Reihenfolge ist global, ein Wechsel der Halle ändert an der Abfolge
    /// nichts. Deshalb braucht diese Stelle weder Konfiguration noch
    /// Hallen-Auflösung. Kein BTP-Write, rein lokal.
    fn reconcile_queue_order(&self, tablet: &TabletState, snapshot: &BtpSnapshot) {
        let keep: HashSet<i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::Scheduled)
            .filter(|m| !m.team1.is_empty() && !m.team2.is_empty())
            .map(|m| m.id)
            .collect();
        tablet.queue_order_store().retain(&keep);
    }

    /// Nachschub-Queue flushen (A5): fehlgeschlagene Ergebnis-Writes
    /// erneut nach BTP schreiben. Läuft nur im Master-Modus, frühestens
    /// alle [`BTP_RETRY_FLUSH_EVERY`], und nur wenn der aktuelle Poll BTP
    /// erreicht hat (der Snapshot dieses Zyklus liegt vor) — das ist
    /// Tilos needsync-Prinzip, nur periodisch statt nur beim Reconnect.
    async fn flush_btp_retries(
        &mut self,
        config: &AppConfig,
        tablet: &TabletState,
        snapshot: &BtpSnapshot,
    ) {
        let entries = tablet.btp_retries();
        if entries.is_empty() {
            return;
        }
        // Bestätigt leerer Snapshot (Turnier in BTP geschlossen/entladen):
        // ein Nachschub in ein nicht (mehr) geladenes Turnier ergibt keinen
        // Sinn — Einträge bleiben liegen, bis wieder Matches da sind oder
        // die Höchst-Lebensdauer greift.
        if snapshot.matches.is_empty() {
            return;
        }
        if self
            .last_btp_retry_flush
            .is_some_and(|t| t.elapsed() < BTP_RETRY_FLUSH_EVERY)
        {
            return;
        }
        self.last_btp_retry_flush = Some(Instant::now());
        let now = now_ms();
        let mut still_failing = 0usize;
        for entry in entries {
            let match_id = entry.update.btp_match_id;
            // Direkt vor dem Write erneut prüfen: Hat ein zwischenzeitlich
            // erfolgreicher Direkt-Write (Tablet-Retry) den Eintrag schon
            // geräumt, entfällt der Nachschub.
            if !tablet.btp_retry_pending(match_id) {
                continue;
            }
            match prepare_btp_retry(&entry, snapshot, now, tablet) {
                RetryAction::Drop(reason) => {
                    tablet.clear_btp_retry(match_id);
                    tracing::warn!("Nachschub für Match {match_id} verworfen: {reason}");
                }
                RetryAction::Write(update) => {
                    let write_started = now_ms();
                    match crate::tablet::server::write_result_to_btp(config, &update).await {
                        Ok(()) => {
                            tablet.clear_btp_retry(match_id);
                            tracing::info!("Nachschub OK: Match {match_id} nach BTP geschrieben");
                            // Race-Selbstheilung: Ist WÄHREND unseres Writes
                            // eine Korrektur direkt durchgegangen, hat unser
                            // (älterer) Stand sie gerade überschrieben —
                            // die neuere Korrektur sofort erneut schreiben.
                            if let Some(newer) =
                                tablet.direct_btp_write_since(match_id, write_started)
                            {
                                tracing::warn!(
                                    "Nachschub für Match {match_id} hat eine \
                                     zwischenzeitliche Korrektur überholt — schreibe \
                                     die Korrektur erneut"
                                );
                                if let Err(e) =
                                    crate::tablet::server::write_result_to_btp(config, &newer).await
                                {
                                    // Korrektur erneut einreihen — der nächste
                                    // Flush versucht es wieder.
                                    tablet.queue_btp_retry(newer, now);
                                    tracing::warn!(
                                        "Korrektur-Rewrite für Match {match_id} \
                                         fehlgeschlagen ({e}) — wieder eingereiht"
                                    );
                                }
                            }
                        }
                        Err(_) => {
                            // Eintrag bleibt — nächster Versuch in ≥30 s.
                            // Sammel-Log statt einer Zeile je Match (Queue
                            // kann viele Einträge halten).
                            still_failing += 1;
                        }
                    }
                }
            }
        }
        if still_failing > 0 {
            tracing::info!("Nachschub-Queue: {still_failing} Eintrag/Einträge weiter erfolglos");
        }
    }

    /// Leer-Snapshot-Guard (Turnier-Befund 19.07.2026): BTP lieferte
    /// vereinzelt einen Abruf lang einen leeren Turnier-Stand — ungefiltert
    /// gab das alle Felder frei (samt Auto-Neuvergabe Sekunden später) und
    /// leerte den Liveticker. Ein leerer Snapshot direkt nach gefüllten
    /// Daten wird deshalb verworfen und erst übernommen, wenn BTP ihn im
    /// Folge-Poll bestätigt (echte Leerung, z. B. Turnier in BTP
    /// geschlossen). R2 bleibt gewahrt: BTP ist die Wahrheit — nur eben
    /// erst, wenn es zweimal dasselbe sagt.
    ///
    /// Bewusste Grenzen: (a) Nach einem App-Neustart ist `seen_matches`
    /// leer — trifft der Aussetzer exakt den allerersten Poll, greift der
    /// Guard nicht (Neustart mitten im Turnier + Aussetzer im selben
    /// Moment: akzeptiertes Restrisiko). (b) `BtpError`-Zyklen dazwischen
    /// setzen den Zähler NICHT zurück — zwei leere Abrufe, getrennt nur
    /// durch technische Fehl-Polls, gelten weiter als Bestätigung.
    ///
    /// Liefert `true`, wenn der Snapshot verdächtig ist und der Zyklus
    /// ohne jede Zustandsänderung abgebrochen werden soll.
    fn empty_snapshot_is_suspect(&mut self, snapshot: &BtpSnapshot) -> bool {
        if !snapshot.matches.is_empty() {
            self.seen_matches = true;
            self.suspect_empty_polls = 0;
            return false;
        }
        // Noch nie Matches gesehen (Start vor Turnier-Aufbau) → leer ist
        // der normale Zustand, nichts zu schützen.
        if !self.seen_matches {
            return false;
        }
        self.suspect_empty_polls += 1;
        if self.suspect_empty_polls >= EMPTY_CONFIRM_POLLS {
            // BTP bleibt dabei → leeren Stand als echt übernehmen. Guard
            // zurücksetzen: leer ist ab jetzt der bekannte Zustand, bis
            // wieder Matches auftauchen.
            tracing::info!(
                "BTP bestätigt den leeren Turnier-Stand ({}. Abruf in Folge) — übernommen",
                self.suspect_empty_polls
            );
            self.seen_matches = false;
            self.suspect_empty_polls = 0;
            return false;
        }
        true
    }

    /// Ist ein Heartbeat fällig? `true`, wenn noch nie gepusht wurde oder
    /// der letzte Push länger als [`HEARTBEAT_AFTER`] zurückliegt.
    fn heartbeat_due(&self) -> bool {
        self.last_push_at
            .is_none_or(|t| t.elapsed() >= HEARTBEAT_AFTER)
    }

    /// Stempelt beendete Matches: Beim ersten Erkennen eines Siegers wird
    /// der aktuelle Zeitpunkt gemerkt und in jedes beendete Match
    /// zurückgeschrieben (stabil über alle folgenden Zyklen).
    fn stamp_finished(&mut self, snapshot: &mut BtpSnapshot) {
        let now = now_ms();
        for m in &mut snapshot.matches {
            if m.status == MatchStatus::Finished {
                m.finished_at = Some(*self.finished_at.entry(m.id).or_insert(now));
            }
        }
    }

    /// Verfolgt den Zähltafelbediener je Feld: Verlässt das im letzten
    /// Zyklus auf einem Feld OnCourt gewesene Spiel das Feld und ist es
    /// beendet, merkt sich der TabletState den Verlierer als
    /// Zähltafelbediener fürs nächste Spiel auf diesem Feld. BTP behält die
    /// Feld-Zuordnung beendeter Spiele nicht zuverlässig — daher tracken
    /// wir den Übergang selbst über die Zyklen.
    fn track_scorekeepers(
        &mut self,
        snapshot: &BtpSnapshot,
        tablet: &TabletState,
        manage_queue: bool,
    ) {
        let oncourt_now: HashMap<i64, i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::OnCourt)
            .filter_map(|m| m.court_id.map(|c| (c, m.id)))
            .collect();
        for (&court_id, &prev_match_id) in &self.oncourt_prev {
            // Steht auf dem Feld jetzt ein anderes (oder kein) Spiel?
            if oncourt_now.get(&court_id) == Some(&prev_match_id) {
                continue;
            }
            // Das vorige Spiel hat das Feld verlassen — beendet + mit Sieger?
            if let Some(fm) = snapshot.matches.iter().find(|m| m.id == prev_match_id) {
                if fm.status == MatchStatus::Finished {
                    // A2 / ADR 0017, Regel b: Der Übergang OnCourt→Finished ist
                    // genau das Finalisiert-Signal — das Feld hat sein Match in
                    // BTP fertig (Sieger steht). Merken (mit der Match-ID), damit
                    // der Server dem Tablet, das noch dieselbe matchId trägt,
                    // `finalized:true` schickt und ein nachlaufender Score
                    // verworfen wird. R2 gewahrt: die Wahrheit bleibt BTP.
                    tablet.mark_finalized(court_id, prev_match_id);
                    if let Some(w) = fm.winner {
                        let loser = if w == 1 { &fm.team2 } else { &fm.team1 };
                        let names: Vec<String> = loser.iter().map(|p| p.name.clone()).collect();
                        if !names.is_empty() {
                            tablet.set_scorekeeper(court_id, names.clone());
                            // Zähltafelbediener-Warteschlange (ADR 0007): nur
                            // bei REGULÄR ausgespieltem Ergebnis einreihen —
                            // Walkover/Aufgabe/DQ erzeugen keinen Bediener.
                            if manage_queue && fm.result == MatchResult::Normal {
                                tablet.enqueue_scorekeeper(fm.id, names, court_id, now_ms());
                            }
                        }
                    }
                }
            }
        }
        // A2 / ADR 0017, Regel b: Jedes Feld mit einem Match OnCourt ist per
        // BTP-Definition nicht finalisiert — den Finalisiert-Merker dort
        // BEDINGUNGSLOS räumen. Das deckt sowohl ein neues Match als auch die
        // TL-Ergebniskorrektur ab (finalisiertes Match kehrt mit DERSELBEN
        // matchId auf dasselbe Feld zurück); sonst verwürfe `handle_score` dessen
        // Punkte still bis zum TTL-Ablauf. Die TTL fängt nur Felder ab, die kein
        // neues Match bekommen (Tablet zeigt noch das fertige Spiel).
        for &court_id in oncourt_now.keys() {
            tablet.clear_finalized(court_id);
        }
        // Zuweisung beim Feld-Aufruf (ADR 0007, Scheibe 2): jedem belegten Feld
        // einen Bediener aus der Warteschlange zuordnen (idempotent je Spiel);
        // Zuweisungen frei gewordener/gewechselter Felder räumen. Ist die
        // Verwaltung aus, alle Zuweisungen löschen — sonst bliebe eine alte
        // Zuweisung in der Anzeige hängen (Review-Befund).
        if manage_queue {
            // Nach CourtID sortiert zuweisen → deterministische, faire
            // FIFO-Verteilung bei mehreren gleichzeitig neu belegten Feldern
            // (HashMap-Iteration wäre zufällig).
            let mut courts: Vec<(i64, i64)> = oncourt_now.iter().map(|(&c, &m)| (c, m)).collect();
            courts.sort_by_key(|&(c, _)| c);
            for (court_id, match_id) in courts {
                tablet.assign_scorekeeper_for_court(court_id, match_id);
            }
            tablet.retain_scorekeeper_assignments(&oncourt_now);
        } else {
            tablet.clear_scorekeeper_assignments();
        }
        self.oncourt_prev = oncourt_now;
    }

    /// Schiedsrichter-Rotation (Spec `schiedsrichter-management` Nr. 4).
    ///
    /// Master-only und bewusst **nur beim Neu-Belegen**: Bestückt wird ein
    /// Feld in dem Zyklus, in dem ein anderes Spiel darauf kommt. Wer eine
    /// Zuweisung von Hand löscht, bekäme sie sonst im nächsten Poll zurück.
    /// Nach Spielende rücken die Officials ans Ende der Reihenfolge — ihre
    /// Zuweisung bleibt am Match stehen (Grundlage der Einsatz-Ableitung).
    /// Die globalen Schalter kommen aus dem Roster-Speicher, nicht aus der
    /// Sync-Konfiguration: Diese wird einmal beim Start gelesen, jene folgt
    /// den Einstellungen sofort.
    fn track_officials(&mut self, snapshot: &BtpSnapshot, tablet: &TabletState) {
        let store = tablet.officials_store();
        let (rotation_sr, rotation_ar) = store.rotation();
        if !store.enabled() {
            // Abschalten mitten im Turnier räumt alles (Spec Nr. 1) — sonst
            // bliebe ein Name in einer Anzeige hängen.
            store.clear_assignments();
            self.officials_oncourt_prev.clear();
            return;
        }
        let oncourt_now: HashMap<i64, i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::OnCourt)
            .filter_map(|m| m.court_id.map(|c| (c, m.id)))
            .collect();

        // Verlassene Felder: War das vorige Spiel beendet, rücken seine
        // Officials ans Ende der Reihenfolge — nach CourtID sortiert, damit
        // bei mehreren gleichzeitig verlassenen Feldern dieselbe
        // Deterministik gilt wie bei der Zuteilung unten (sonst entschiede
        // die zufällige HashMap-Iterationsreihenfolge, wer zuerst ans Ende
        // rückt).
        let mut verlassen: Vec<(i64, i64)> = self
            .officials_oncourt_prev
            .iter()
            .map(|(&c, &m)| (c, m))
            .collect();
        verlassen.sort_by_key(|&(c, _)| c);
        for (court_id, prev_match_id) in verlassen {
            if oncourt_now.get(&court_id) == Some(&prev_match_id) {
                continue;
            }
            let Some(fm) = snapshot.matches.iter().find(|m| m.id == prev_match_id) else {
                continue;
            };
            if fm.status != MatchStatus::Finished {
                continue;
            }
            let wirksam = store.effective(fm.id, fm.official1_id, fm.official2_id);
            let fertig: Vec<i64> = wirksam.sr.into_iter().chain(wirksam.ar).collect();
            if fertig.is_empty() {
                tracing::info!(
                    "Feld {court_id}: Spiel {} beendet, aber keine Schiedsrichter-Besetzung \
                     bekannt (weder BTP noch lokal) — nichts rückt ans Ende",
                    fm.id
                );
            } else {
                tracing::info!(
                    "Feld {court_id}: Spiel {} beendet — {:?} rücken ans Ende der Rotation",
                    fm.id,
                    fertig
                );
            }
            store.move_to_end(&fertig);
        }

        // Wer tut gerade irgendwo Dienst? Aus den laufenden Spielen, damit
        // niemand zwei Felder gleichzeitig bekommt.
        let bekannt: Vec<i64> = snapshot.officials.iter().map(|o| o.id).collect();
        let mut im_dienst: HashSet<i64> = HashSet::new();
        for m in snapshot.matches.iter().filter(|m| {
            m.status == MatchStatus::OnCourt
                && m.court_id.is_some_and(|c| oncourt_now.contains_key(&c))
        }) {
            let w = store.effective(m.id, m.official1_id, m.official2_id);
            im_dienst.extend(w.sr);
            im_dienst.extend(w.ar);
        }

        // Neu belegte Felder bestücken — nach CourtID sortiert, damit die
        // Verteilung bei mehreren gleichzeitig deterministisch ist.
        let mut courts: Vec<(i64, i64)> = oncourt_now.iter().map(|(&c, &m)| (c, m)).collect();
        courts.sort_by_key(|&(c, _)| c);
        for (court_id, match_id) in courts {
            if self.officials_oncourt_prev.get(&court_id) == Some(&match_id) {
                continue; // unverändert belegt ⇒ nichts nachfüllen
            }
            let Some(m) = snapshot.matches.iter().find(|m| m.id == match_id) else {
                continue;
            };
            let schalter = store.court_switches(court_id);
            let players: Vec<crate::btp::model::BtpPlayer> =
                m.team1.iter().chain(m.team2.iter()).cloned().collect();
            let vorher = store.effective(match_id, m.official1_id, m.official2_id);
            store.rotate_court(crate::tablet::officials::RotationInput {
                match_id,
                players: &players,
                btp_sr: m.official1_id,
                btp_ar: m.official2_id,
                bekannt: &bekannt,
                im_dienst: &im_dienst,
                sr: rotation_sr && schalter.sr,
                ar: rotation_ar && schalter.ar,
            });
            // Frisch Zugewiesene zählen sofort als im Dienst — sonst bekäme
            // das nächste Feld im selben Zyklus dieselbe Person.
            let nachher = store.effective(match_id, m.official1_id, m.official2_id);
            if vorher.sr != nachher.sr {
                im_dienst.extend(nachher.sr);
            }
            if vorher.ar != nachher.ar {
                im_dienst.extend(nachher.ar);
            }
        }
        self.officials_oncourt_prev = oncourt_now;
    }

    /// Bestimmt die automatischen Feldvergaben dieses Zyklus und pflegt dabei
    /// `court_free_since`. Liefert die nach BTP zu schreibenden (Court-,
    /// Match-)Updates; leer, wenn die Funktion aus ist oder nichts ansteht.
    ///
    /// Regeln (bewusst konservativ – schreibt live nach BTP):
    /// - Nur Felder, die im Snapshot frei (kein Match referenziert sie) und
    ///   nicht gesperrt sind und seit ≥ `wait_minutes` frei stehen.
    /// - Nächstes spielbereites Match (Scheduled, beide Teams bekannt);
    ///   Reihenfolge gerufen-zuerst → Spielnummer → ID (wie die Vorbereitung).
    /// - Mehr-Hallen-Turnier: nur Matches, die für DIESE Halle in die
    ///   Vorbereitung gerufen wurden (kein Risiko, ein Spiel in die falsche
    ///   Halle zu legen). Ein-Hallen: das nächste bereite Match.
    /// - Kein Match doppelt in einem Zyklus.
    fn auto_assign(
        &mut self,
        config: &AppConfig,
        snapshot: &BtpSnapshot,
        tablet: &TabletState,
    ) -> (
        Vec<crate::btp::proto::CourtAssignment>,
        Vec<crate::btp::proto::MatchCourt>,
    ) {
        let now = now_ms();
        // Belegt = irgendein Match referenziert das Feld (OnCourt ODER noch
        // nicht abgeräumtes beendetes Spiel) → solche Felder sind nicht frei.
        // Die Definition liegt in `tablet::assign` und wird mit der manuellen
        // Vergabe geteilt — sonst hielte der eine Pfad ein Feld für frei, das
        // der andere als belegt sieht.
        let busy: HashSet<i64> = crate::tablet::assign::occupied_courts(snapshot);
        // Frei-seit pflegen: belegte vergessen, freie stempeln, unbekannte raus.
        let known: HashSet<i64> = snapshot.court_infos.iter().map(|c| c.id).collect();
        self.court_free_since.retain(|id, _| known.contains(id));
        for court in &snapshot.court_infos {
            if busy.contains(&court.id) {
                self.court_free_since.remove(&court.id);
            } else {
                self.court_free_since.entry(court.id).or_insert(now);
            }
        }

        // Offene (von BTP noch nicht bestätigte) Auto-Zuweisungen abgleichen:
        // belegt sichtbar gewordene Felder bzw. abgelaufene Einträge fallen
        // weg (Letzteres = Write vermutlich fehlgeschlagen → erneut versuchen).
        self.pending_auto.retain(|court_id, (_, ts)| {
            !busy.contains(court_id)
                && now.saturating_sub(*ts) < PENDING_AUTO_TTL.as_millis() as u64
        });

        // Ausgeschaltet in der Konfiguration ODER zur Laufzeit angehalten
        // (Turnierleitungs-Oberfläche). Die Vormerkungen oben werden trotzdem
        // gepflegt, damit ein Wiedereinschalten nicht auf altem Stand
        // aufsetzt.
        if !config.auto_assign.enabled || tablet.auto_assign_paused() {
            return (Vec::new(), Vec::new());
        }
        // Wartezeit robust: NaN/Inf/negativ → 0 (sofort).
        let wm = config.auto_assign.wait_minutes;
        let wait_ms = if wm.is_finite() && wm > 0.0 {
            (wm * 60_000.0) as u64
        } else {
            0
        };
        let locked: HashSet<i64> = tablet.locked_courts().into_iter().collect();
        // Felder/Matches mit offener (unbestätigter) Auto-Zuweisung sperren.
        let mut pending_courts: HashSet<i64> = self.pending_auto.keys().copied().collect();
        let mut pending_matches: HashSet<i64> =
            self.pending_auto.values().map(|(m, _)| *m).collect();
        // Dasselbe gilt für Zuweisungen, die von HAND geschrieben wurden und
        // auf die Bestätigung durch BTP warten (Turnierleitungs-Oberfläche).
        // Ohne das legte die Automatik im selben Takt ein zweites Spiel auf
        // ein Feld, das gerade jemand belegt hat.
        let reserved = tablet.reserved_courts(now);
        for (court_id, match_id) in &reserved {
            pending_courts.insert(*court_id);
            pending_matches.insert(*match_id);
        }
        // Und ihre Spieler gelten als belegt: Sonst ruft die Automatik
        // jemanden auf ein zweites Feld, den die Turnierleitung gerade
        // woanders hingestellt hat — im Schnappschuss steht er ja noch
        // nirgends.
        let reserved_players: HashSet<String> = reserved
            .iter()
            .filter_map(|(_, m)| snapshot.matches.iter().find(|x| x.id == *m))
            .flat_map(|m| m.team1.iter().chain(m.team2.iter()))
            .map(player_key)
            .collect();
        let multi_hall = snapshot.is_multi_hall();
        // Aktive Halle (Tages-Halle) aus der Config → LocationID auflösen.
        // Ist sie gesetzt, vergeben wir NUR auf Felder dieser Halle und brauchen
        // KEINEN manuellen „in Vorbereitung"-Aufruf (Mehr-Hallen-Turnier, an dem
        // Tag wird nur eine Halle bespielt). Unbekannter Name → wie nicht gesetzt.
        // Nur im Mehr-Hallen-Fall relevant: bei Ein-Hallen-Turnieren (auch wenn
        // versehentlich gesetzt) wird die aktive Halle ignoriert — sonst würden
        // Felder ohne `location_id` (häufig bei Ein-Hallen-BTP) alle gefiltert
        // und es würde nichts vergeben.
        let active_loc: Option<i64> = if !multi_hall {
            None
        } else {
            let name = config.auto_assign.active_hall.trim();
            if name.is_empty() {
                None
            } else {
                let found = snapshot
                    .locations
                    .iter()
                    .find(|l| l.name.trim().eq_ignore_ascii_case(name))
                    .map(|l| l.id);
                if found.is_none() {
                    tracing::warn!(
                        "Aktive Halle '{name}' nicht gefunden – Auto-Vergabe fällt auf \
                         Aufruf-Pflicht zurück. Verfügbar: {:?}",
                        snapshot
                            .locations
                            .iter()
                            .map(|l| &l.name)
                            .collect::<Vec<_>>()
                    );
                }
                found
            }
        };
        // Aufruf-Pflicht nur im Mehr-Hallen-Fall OHNE gesetzte aktive Halle.
        let require_call = multi_hall && active_loc.is_none();
        let calls = tablet.preparation_calls();
        let call_for = |mid: i64| calls.iter().find(|c| c.match_id == mid);

        // Spielbereite Matches in Vorbereitungs-Reihenfolge.
        let mut ready: Vec<&crate::btp::model::BtpMatch> = snapshot
            .matches
            .iter()
            .filter(|m| {
                m.status == MatchStatus::Scheduled && !m.team1.is_empty() && !m.team2.is_empty()
            })
            .collect();
        // Reihenfolge: manuell „in Vorbereitung" gerufene zuerst (Override),
        // dann der manuelle Präfix je Halle (Spec
        // `spielliste-manuelle-reihenfolge`), sonst den BTP-Zeitplan von
        // oben nach unten (PlannedTime), dann Spielnummer/ID als
        // Tiebreaker. Ohne Ansetzung → ans Ende der Zeitgruppe, danach
        // greift die Spielnummer (Verhalten wie bisher).
        // Die Reihenfolge liegt in `tablet::assign` und wird mit der Anzeige
        // geteilt — zeigte die Liste eine andere als die Automatik benutzt,
        // verlöre die Turnierleitung das Vertrauen in beide.
        let manual_halls = tablet.manual_halls();
        ready.sort_by_key(|m| {
            let call = call_for(m.id);
            let manual_hall = manual_halls.get(&m.id).map(String::as_str);
            let called_hall = call.and_then(|c| c.location_id).and_then(|lid| {
                snapshot
                    .locations
                    .iter()
                    .find(|l| l.id == lid)
                    .map(|l| l.name.as_str())
            });
            let (_, _, key) = crate::tablet::assign::resolve_and_sort_key(
                config,
                snapshot,
                m,
                manual_hall,
                called_hall,
                call.is_some(),
                tablet.queue_order_store(),
            );
            key
        });

        // ── Spieler-Verfügbarkeit ────────────────────────────────────────
        // Wer gerade spielt und wer noch pausiert, liegt in `tablet::assign`
        // — dieselbe Auskunft, die die Turnierleitungs-Anzeige gibt. Die
        // zyklusinterne Regel „kein Spieler auf zwei gleichzeitig frei
        // werdende Felder" bleibt hier, sie gilt nur innerhalb eines Laufs.
        let availability =
            crate::tablet::assign::PlayerAvailability::from_snapshot(snapshot, config);

        let mut courts = Vec::new();
        let mut match_courts = Vec::new();
        let mut used: HashSet<i64> = HashSet::new();
        // Spieler, die in DIESEM Zyklus schon ein Feld bekommen haben – kein
        // Spieler darf auf zwei gleichzeitig frei werdende Felder kommen.
        let mut used_players: HashSet<String> = HashSet::new();

        for court in &snapshot.court_infos {
            if busy.contains(&court.id)
                || locked.contains(&court.id)
                || pending_courts.contains(&court.id)
            {
                continue;
            }
            // Aktive Halle gesetzt → nur deren Felder bespielen.
            if let Some(loc) = active_loc {
                if court.location_id != Some(loc) {
                    continue;
                }
            }
            // Hallenname dieses Felds (für die Disziplin→Halle-Regeln).
            let court_hall = snapshot.court_location_name(court.id);
            let free_since = self.court_free_since.get(&court.id).copied().unwrap_or(now);
            if now.saturating_sub(free_since) < wait_ms {
                continue;
            }
            let pick = ready.iter().find(|m| {
                if used.contains(&m.id) || pending_matches.contains(&m.id) {
                    return false;
                }
                // Von der Turnierleitung ausgenommen (Spec
                // `feldvergabe-ausnahme`) — manuelles Zuweisen bleibt davon
                // unberührt, nur die Automatik überspringt es.
                if tablet.auto_assign_excluded(m.id) {
                    return false;
                }
                // Verfügbarkeit: kein Spieler darf gerade spielen oder noch in
                // seiner Pause stecken (geteilte Regel) …
                if availability.blocked(m, now).is_some() {
                    return false;
                }
                // … und keiner darf in DIESEM Zyklus schon ein Feld bekommen
                // haben (rein zykluslokal, deshalb hier) oder gerade von Hand
                // auf ein Feld gestellt worden sein, das BTP noch nicht
                // zurückmeldet.
                if m.team1.iter().chain(m.team2.iter()).any(|p| {
                    let k = player_key(p);
                    used_players.contains(&k) || reserved_players.contains(&k)
                }) {
                    return false;
                }
                // Disziplin/Klasse→Halle-Regel: Match darf nur in seine erlaubte
                // Halle (manuell wie automatisch). Ohne Regel: keine Einschränkung.
                if !config.hall_allows_match(m.discipline.as_str(), &m.draw_name, &court_hall) {
                    return false;
                }
                if require_call {
                    // Mehr-Hallen ohne aktive Halle: nur für diese Halle
                    // gerufene Matches.
                    call_for(m.id)
                        .and_then(|c| c.location_id)
                        .zip(court.location_id)
                        .map(|(a, b)| a == b)
                        .unwrap_or(false)
                } else {
                    // Ein-Hallen oder aktive Halle gesetzt: jedes spielbereite
                    // Match (Reihenfolge regelt die Zeit-Sortierung).
                    true
                }
            });
            let Some(m) = pick else { continue };
            used.insert(m.id);
            // Spieler dieses Matches für den Rest des Zyklus als belegt merken.
            for p in m.team1.iter().chain(m.team2.iter()) {
                used_players.insert(player_key(p));
            }
            courts.push(crate::btp::proto::CourtAssignment {
                court_id: court.id,
                match_id: Some(m.id),
            });
            match_courts.push(crate::btp::proto::MatchCourt {
                match_id: m.id,
                draw_id: m.draw_id,
                planning_id: m.planning_id,
                court_id: court.id,
                // Die Besetzung wandert mit ins Zuweisungs-Update
                // (ADR 0021) — ein Request statt zwei.
                officials: tablet.officials_for_write(m),
            });
            // Feld gilt jetzt als belegt – Wartezeit zurücksetzen und die
            // Zuweisung als „unterwegs" merken, damit weder Feld noch Match bis
            // zur BTP-Rückmeldung erneut vergeben werden (keine Doppelvergabe).
            self.court_free_since.remove(&court.id);
            self.pending_auto.insert(court.id, (m.id, now));
            // Dieselbe Vormerkung auch im geteilten Zustand: Sonst wüsste die
            // Turnierleitungs-Oberfläche nichts davon und könnte im selben
            // Zeitfenster ein zweites Spiel auf dasselbe Feld legen. Der
            // Schutz muss in BEIDE Richtungen wirken, sonst ist er keiner.
            tablet.try_reserve_court(court.id, m.id, now);
        }
        (courts, match_courts)
    }

    /// Führt einen vollständigen Poll-Push-Zyklus aus.
    ///
    /// `tablet` bekommt den frischen BTP-Snapshot (Court→Match-Auflösung für
    /// den Tablet-Server); Courts mit aktivem Tablet treiben anschließend
    /// ihren Live-Score selbst – ihr Satzstand überschreibt den BTP-Poll.
    pub async fn run_once(
        &mut self,
        config: &AppConfig,
        http: &reqwest::Client,
        tablet: &TabletState,
    ) -> SyncOutcome {
        let mut snapshot = match client::fetch_snapshot(
            &config.btp.host,
            config.btp.port,
            config.btp.password.as_deref(),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(e) => return SyncOutcome::BtpError(e.to_string()),
        };

        // Leer-Snapshot-Guard: verdächtig leere Stände verwerfen, BEVOR
        // irgendetwas davon abgeleitet wird (Feld-Freigaben, Auto-Vergabe,
        // Tablet-Snapshot, Liveticker-Push). MUSS der erste Schritt nach
        // fetch_snapshot bleiben — jeder Schritt davor würde bei einem
        // Aussetzer bereits Zustand aus dem leeren Stand ableiten.
        if self.empty_snapshot_is_suspect(&snapshot) {
            tracing::warn!(
                "BTP-Snapshot ohne Matches direkt nach gefülltem Stand — verworfen \
                 (Abruf {}/{}), warte auf Bestätigung im nächsten Abruf",
                self.suspect_empty_polls,
                EMPTY_CONFIRM_POLLS
            );
            return SyncOutcome::SnapshotDiscarded;
        }

        // Turnier-Topologie ins Diagnose-Log – nur bei Änderung, damit es
        // den Log nicht jeden Poll-Zyklus flutet. Zeigt u. a., ob ein
        // Mehr-Hallen-Turnier korrekt erkannt wurde.
        let topology = (
            snapshot.locations.len(),
            snapshot.court_infos.len(),
            snapshot.matches.len(),
        );
        if self.last_topology != Some(topology) {
            tracing::info!(
                "BTP-Snapshot: {} Hallen, {} Felder, {} Matches",
                topology.0,
                topology.1,
                topology.2
            );
            // Diagnose: welche Zählweisen liefert BTP? Distinkte Formate
            // (Sätze/Ende/Cap/Intervall) ohne Spielernamen – zur Kontrolle,
            // ob z. B. „3×15 (21)" korrekt als 15/21/Intervall-8 ankommt.
            let mut formats: Vec<(i64, i64, i64, Option<i64>)> = snapshot
                .matches
                .iter()
                .map(|m| {
                    (
                        m.scoring.best_of,
                        m.scoring.target_score,
                        m.scoring.cap_score,
                        m.scoring.interval_at,
                    )
                })
                .collect();
            formats.sort_unstable();
            formats.dedup();
            tracing::info!("BTP-Zählweisen (best_of, Ende, Cap, Intervall): {formats:?}");
            self.last_topology = Some(topology);
        }

        self.stamp_finished(&mut snapshot);
        self.track_scorekeepers(&snapshot, tablet, config.scorekeeper.enabled);
        // Aufruf-Timer: je Feld festhalten, seit wann das aktuelle Spiel dort
        // steht (1. Aufruf). Aus demselben OnCourt-Stand wie die Scorekeeper.
        // Bewusst VOR set_snapshot: so ist der Zeitstempel spätestens da, wenn
        // overview() das neue OnCourt-Match sieht (sonst fehlte der Chip einen
        // Poll lang). Reihenfolge nicht umdrehen.
        let oncourt_now: HashMap<i64, i64> = snapshot
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::OnCourt)
            .filter_map(|m| m.court_id.map(|c| (c, m.id)))
            .collect();
        tablet.reconcile_on_court(&oncourt_now, now_ms());
        // Spielzeiten-Messung (Spec `spielzeiten-prognose`, E4): der
        // persistente Erst-Stempel je Match — bewusst NACH
        // reconcile_on_court aus demselben Snapshot, damit beide Uhren
        // dieselbe Zuweisung sehen.
        self.reconcile_match_times(tablet, &snapshot, now_ms());
        // Automatische Feldvergabe: freie, lange genug freie, nicht gesperrte
        // Felder mit dem nächsten spielbereiten Match belegen (schreibt nach
        // BTP). Aus dem aktuellen Snapshot bestimmt – kollidiert so nicht mit
        // einer BTP-seitigen Zuweisung; der nächste Poll liest beides gleich.
        // Auto-Feldvergabe nur im Normalbetrieb – ein Ansage-Slave schreibt nie
        // nach BTP (nur der Master vergibt Felder).
        if !config.slave_mode {
            // Nachschub-Queue (A5): liegengebliebene Ergebnis-Writes
            // nachreichen — BTP ist in diesem Zyklus nachweislich erreichbar.
            // Kollidiert nicht mit der Auto-Vergabe direkt darunter: beide
            // arbeiten auf DEMSELBEN (vor dem Flush geladenen) Snapshot, ein
            // hier frisch freigegebenes Feld erscheint dort noch belegt und
            // wird frühestens im nächsten Poll neu vergeben.
            self.flush_btp_retries(config, tablet, &snapshot).await;
            let (auto_courts, auto_matches) = self.auto_assign(config, &snapshot, tablet);
            if !auto_courts.is_empty() {
                match crate::tablet::server::write_courts_to_btp(
                    config,
                    &auto_courts,
                    &auto_matches,
                )
                .await
                {
                    Ok(()) => {
                        tracing::info!("Auto-Feldvergabe: {} Feld(er) belegt", auto_courts.len())
                    }
                    Err(e) => tracing::warn!("Auto-Feldvergabe fehlgeschlagen: {e}"),
                }
            }
        }
        // Rohen BTP-Stand dem Tablet-Server geben, dann die Sätze
        // tablet-getriebener Courts überschreiben.
        tablet.set_snapshot(snapshot.clone());
        // Ansage-Slave: nur lesen + lokal ansagen (MatchAnnouncer liest den
        // Snapshot). KEIN Liveticker-Push (würde mit dem Master kollidieren).
        if config.slave_mode {
            return SyncOutcome::SlaveActive;
        }
        // Schiedsrichter-Rotation (Spec schiedsrichter-management Nr. 4):
        // bewusst NACH `set_snapshot` — der Roster ist dann ans Turnier
        // gebunden und um neue BTP-Officials ergänzt. Master-only: der
        // Ansage-Slave ist oben schon zurückgekehrt.
        self.track_officials(&snapshot, tablet);
        tablet.apply_tablet_scores(&mut snapshot);
        // Von Hand geschriebene Feldzuweisungen, die BTP inzwischen
        // zurückmeldet, brauchen keine Vormerkung mehr — sie sollen das Feld
        // nicht länger blockieren als nötig.
        tablet.release_confirmed_reservations();
        // „In Vorbereitung" gerufene Spiele in den Snapshot stempeln, damit
        // der Aufruf-Zeitstempel im nächsten Push an badhub.de mitgeht.
        tablet.apply_preparation_calls(&mut snapshot);
        // Aufrufe zusätzlich in BTP sichtbar machen (P1, Highlight-Flag) —
        // nur der Diff zum letzten Stand, nur wenn sich etwas geändert hat.
        self.reconcile_highlights(config, tablet, &snapshot).await;
        // Schiedsrichter-Besetzung zurückschreiben (ADR 0021) — direkt nach
        // den Highlights, aus demselben Snapshot.
        self.reconcile_officials(config, tablet, &snapshot).await;
        // Ausnahmeliste der Auto-Vergabe aufräumen (Spec
        // `feldvergabe-ausnahme`) — braucht kein Slave, der macht ohnehin
        // keine Auto-Vergabe (oben schon zurückgekehrt).
        self.reconcile_auto_assign_exclusions(tablet, &snapshot);
        // Manuelle Spielreihenfolge ebenso aufräumen (Spec
        // `spielliste-manuelle-reihenfolge`) — direkt danach, gleiche
        // Bedingungen (lokal, kein Slave nötig).
        self.reconcile_queue_order(tablet, &snapshot);
        // Meldeliste für den Hallen-Check-In (ADR 0009) vorbereiten — gesendet
        // wird sie erst NACH dem Liveticker-Push (siehe unten).
        let roster = self.plan_checkin_roster(config, &snapshot);
        // Kontext der manuellen Spielreihenfolge (Spec
        // `spielliste-manuelle-reihenfolge`) — lebt außerhalb des Snapshots,
        // deshalb hier einmal je Zyklus frisch gebaut.
        let queue_ctx = crate::badhub::payload::LivetickerContext::new(
            config,
            tablet.manual_halls(),
            tablet.queue_order_store(),
        );
        // Heartbeat: Ist regulär nichts zu senden, aber seit dem letzten
        // Push >60 s vergangen, wird ein voller `tset` als Lebenszeichen
        // erzwungen (Diff gegen `None`). badhub frischt damit `updated_at`
        // auf und erkennt das Turnier als aktiv.
        let mut update = match self.plan(&snapshot, &queue_ctx) {
            Update::None if self.heartbeat_due() => diff(None, &snapshot, self.rid, &queue_ctx),
            other => other,
        };
        // Turnierlogo aus der Config in den vollen `tset`-Event injizieren –
        // badhubs `#live-logo` zeigt es dann an (gleiche Felder wie Original-BTS).
        // Nur `tset` trägt den Event-Block; ein `tupdate_match` braucht es nicht,
        // da badhub den Logo-Stand aus dem zuletzt gemergten Snapshot behält.
        // Bei leerem Logo bleiben die Felder leer und werden nicht serialisiert.
        if let Update::Full(msg) = &mut update {
            let logo = &config.tournament_logo;
            if !logo.data.is_empty() {
                msg.event.tournament_logo = logo.data.clone();
                msg.event.tournament_logo_mime = logo.mime.clone();
                msg.event
                    .tournament_logo_background_color
                    .clone_from(&logo.background_color);
            }
        }
        let sent_something = !matches!(update, Update::None);
        let push_result =
            push::push_update(http, &config.badhub.url, &config.badhub.password, &update).await;
        // Erst jetzt die Meldeliste: der Liveticker ist die zeitkritische
        // Funktion, der Check-In die additive. Stünde der Roster-Push davor,
        // könnte ein hängender Check-In-Endpunkt die Ergebnis-Übertragung um
        // seinen ganzen Timeout verzögern — bei einem 5-Sekunden-Poll-Takt
        // wäre das ein spürbarer Aussetzer auf dem Liveticker.
        if let Some(roster) = roster {
            self.send_checkin_roster(config, http, roster).await;
        }
        match push_result {
            Ok(()) => {
                let outcome = match update {
                    Update::Full(_) => SyncOutcome::PushedFull,
                    Update::Single(_) => SyncOutcome::PushedUpdate,
                    Update::None => SyncOutcome::Idle,
                };
                if sent_something {
                    self.last_push_at = Some(Instant::now());
                }
                self.on_success(snapshot);
                outcome
            }
            Err(e) => {
                self.on_failure();
                SyncOutcome::PushError(e.to_string())
            }
        }
    }

    /// Plant das nächste Update gegen den zuletzt gesendeten Stand.
    fn plan(
        &self,
        current: &BtpSnapshot,
        ctx: &crate::badhub::payload::LivetickerContext,
    ) -> Update {
        diff(self.last_pushed.as_ref(), current, self.rid, ctx)
    }

    /// Nach erfolgreichem Push: Stand merken, Request-ID erhöhen.
    fn on_success(&mut self, pushed: BtpSnapshot) {
        self.last_pushed = Some(pushed);
        self.rid += 1;
    }

    /// Nach fehlgeschlagenem Push: gemerkten Stand verwerfen, damit der
    /// nächste Zyklus einen vollen `tset` sendet.
    fn on_failure(&mut self) {
        self.last_pushed = None;
        self.rid += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{BtpMatch, BtpPlayer, Discipline, MatchResult, MatchStatus};

    fn snapshot() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![BtpMatch {
                display_order: None,
                from1: None,
                from2: None,
                id: 1,
                draw_id: 1,
                planning_id: 1001,
                draw_name: "HE".to_string(),
                discipline: Discipline::MensSingles,
                class_label: String::new(),
                round_name: "G1".to_string(),
                match_num: Some(1),
                planned_time: None,
                team1: vec![BtpPlayer {
                    id: 0,
                    name: "A".to_string(),
                    first: String::new(),
                    last: "A".to_string(),
                    member_id: None,
                    nationality: None,
                    club: None,
                }],
                team2: vec![BtpPlayer {
                    id: 0,
                    name: "B".to_string(),
                    first: String::new(),
                    last: "B".to_string(),
                    member_id: None,
                    nationality: None,
                    club: None,
                }],
                entry1_id: 0,
                entry2_id: 0,
                court: Some("1".to_string()),
                court_id: None,
                location_id: None,
                sets: vec![(5, 3)],
                winner: None,
                result: MatchResult::Normal,
                status: MatchStatus::OnCourt,
                finished_at: None,
                preparation_call_ts: None,
                preparation_hall: None,
                official1_id: None,
                official2_id: None,
                scoring: crate::btp::model::ScoringFormat::default(),
            }],
        }
    }

    #[test]
    fn first_plan_is_always_full() {
        let engine = SyncEngine::new();
        assert!(matches!(
            engine.plan(
                &snapshot(),
                &crate::badhub::payload::LivetickerContext::bare(&AppConfig::default())
            ),
            Update::Full(_)
        ));
    }

    #[test]
    fn unchanged_snapshot_after_success_plans_nothing() {
        let mut engine = SyncEngine::new();
        engine.on_success(snapshot());
        assert!(matches!(
            engine.plan(
                &snapshot(),
                &crate::badhub::payload::LivetickerContext::bare(&AppConfig::default())
            ),
            Update::None
        ));
    }

    #[test]
    fn after_failure_next_plan_is_full_again() {
        let mut engine = SyncEngine::new();
        engine.on_success(snapshot());
        // Ohne Fehler wäre ein unveränderter Snapshot ein No-op …
        assert!(matches!(
            engine.plan(
                &snapshot(),
                &crate::badhub::payload::LivetickerContext::bare(&AppConfig::default())
            ),
            Update::None
        ));
        // … nach einem Push-Fehler aber wird wieder voll gesendet.
        engine.on_failure();
        assert!(matches!(
            engine.plan(
                &snapshot(),
                &crate::badhub::payload::LivetickerContext::bare(&AppConfig::default())
            ),
            Update::Full(_)
        ));
    }

    #[test]
    fn heartbeat_due_until_a_push_happened() {
        let mut engine = SyncEngine::new();
        // Noch nie gepusht → Heartbeat fällig.
        assert!(engine.heartbeat_due());
        // Direkt nach einem Push → noch kein Heartbeat fällig.
        engine.last_push_at = Some(Instant::now());
        assert!(!engine.heartbeat_due());
    }

    // ───────────────────────── Auto-Feldvergabe ─────────────────────────

    use crate::btp::model::{BtpCourt, BtpLocation};
    use crate::config::AutoAssignConfig;
    use crate::tablet::state::{PreparationCall, TabletState};

    fn player(n: &str) -> BtpPlayer {
        BtpPlayer {
            id: 0,
            name: n.to_string(),
            first: String::new(),
            last: n.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        }
    }

    /// Match mit Status/Feld/Halle-unabhängig; Scheduled = spielbereit.
    fn ready_match(id: i64, num: i64) -> BtpMatch {
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
            match_num: Some(num),
            planned_time: None,
            team1: vec![player("A")],
            team2: vec![player("B")],
            entry1_id: 0,
            entry2_id: 0,
            court: None,
            court_id: None,
            location_id: None,
            sets: Vec::new(),
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::Scheduled,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
            official1_id: None,
            official2_id: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    fn court(id: i64, location_id: Option<i64>) -> BtpCourt {
        BtpCourt {
            id,
            name: id.to_string(),
            location_id,
            sort_order: id,
        }
    }

    fn snap_with(
        courts: Vec<BtpCourt>,
        matches: Vec<BtpMatch>,
        locs: Vec<BtpLocation>,
    ) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: locs,
            court_infos: courts,
            matches,
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        }
    }

    fn cfg_auto(enabled: bool, wait_minutes: f64) -> AppConfig {
        AppConfig {
            auto_assign: AutoAssignConfig {
                enabled,
                wait_minutes,
                pause_minutes: 0.0,
                active_hall: String::new(),
            },
            ..AppConfig::default()
        }
    }

    #[test]
    fn auto_assign_fills_free_court_with_ready_match() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        // wait=0 → sofort belegen.
        let (courts, matches) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].court_id, 1);
        assert_eq!(courts[0].match_id, Some(7));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].match_id, 7);
        assert_eq!(matches[0].court_id, 1);
    }

    #[test]
    fn highlight_desired_only_callable_scheduled_matches() {
        // P1: nur gerufene Spiele, die noch Scheduled sind + Paarung steht.
        let mut on_court = ready_match(8, 2);
        on_court.status = MatchStatus::OnCourt;
        on_court.court_id = Some(1);
        let snap = snap_with(Vec::new(), vec![ready_match(7, 1), on_court], Vec::new());
        let calls = vec![
            PreparationCall {
                match_id: 7,
                location_id: None,
                called_at_ms: 0,
            }, // Scheduled → dabei
            PreparationCall {
                match_id: 8,
                location_id: None,
                called_at_ms: 0,
            }, // aufs Feld gerufen → raus (Highlight:0)
            PreparationCall {
                match_id: 99,
                location_id: None,
                called_at_ms: 0,
            }, // nicht im Snapshot → raus
        ];
        assert_eq!(highlight_desired(&calls, &snap), HashSet::from([7]));
    }

    #[test]
    fn highlight_entries_only_the_diff() {
        let snap = snap_with(
            Vec::new(),
            vec![ready_match(7, 1), ready_match(8, 2), ready_match(9, 3)],
            Vec::new(),
        );
        // 7 neu gerufen (→ on), 9 nicht mehr gerufen (→ off), 8 unverändert.
        let desired = HashSet::from([7, 8]);
        let written = HashSet::from([8, 9]);
        let entries = highlight_entries(&desired, &written, &snap);
        let mut got: Vec<(i64, bool)> = entries.iter().map(|e| (e.match_id, e.on)).collect();
        got.sort();
        assert_eq!(got, vec![(7, true), (9, false)]);
        // Identität (Draw/Planning) aus dem Snapshot mitgegeben.
        let e7 = entries.iter().find(|e| e.match_id == 7).unwrap();
        assert_eq!((e7.draw_id, e7.planning_id), (1, 1007));
    }

    #[test]
    fn auto_assign_disabled_assigns_nothing_but_tracks_free() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(false, 0.0), &snap, &tablet);
        assert!(courts.is_empty());
        // Frei-seit wird trotzdem gepflegt (für den Wartezeit-Start).
        assert!(engine.court_free_since.contains_key(&1));
    }

    #[test]
    fn the_tournament_desk_can_pause_the_automatic_assignment_at_once() {
        // Während die Turnierleitung von Hand umsortiert, soll die Automatik
        // nicht dazwischenfunken. Der Schalter muss SOFORT wirken: Der
        // Sync-Lauf bekommt seine Konfiguration einmal beim Start und liest
        // sie nie neu — eine Änderung an der Datei bliebe wirkungslos.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());

        tablet.set_auto_assign_paused(true);
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(courts.is_empty(), "pausiert vergibt die Automatik nichts");

        tablet.set_auto_assign_paused(false);
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(courts.len(), 1, "und danach wieder");
    }

    #[test]
    fn auto_assign_skips_a_court_someone_just_claimed_by_hand() {
        // Die Turnierleitung hat gerade ein Spiel auf Feld 1 geschrieben,
        // BTP hat es aber noch nicht zurückgemeldet. Ohne Rücksicht auf die
        // Reservierung legte die Automatik im selben Takt ein zweites Spiel
        // auf dasselbe Feld.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.try_reserve_court(1, 99, now_ms());
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(
            courts.is_empty(),
            "das von Hand belegte Feld bleibt der Automatik verschlossen"
        );
    }

    #[test]
    fn auto_assign_skips_locked_court() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.set_court_locked(1, true);
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(courts.is_empty());
    }

    #[test]
    fn exclusion_wird_bei_spielende_automatisch_entfernt() {
        // Spec `feldvergabe-ausnahme`: Sobald ein ausgenommenes Match
        // `Finished` ist, verschwindet die Ausnahme von selbst — sonst
        // bliebe die Datei über das Turnierende hinaus voll.
        let engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.set_auto_assign_excluded(7, true);
        tablet.set_auto_assign_excluded(8, true);

        // Match 7 ist beendet, Match 8 läuft noch.
        let snap = snap_with(
            Vec::new(),
            vec![
                finished_named(7, 0, "A", "B"),
                oncourt_named(8, 1, "C", "D"),
            ],
            Vec::new(),
        );
        engine.reconcile_auto_assign_exclusions(&tablet, &snap);
        assert!(
            !tablet.auto_assign_excluded(7),
            "beendetes Match aufgeräumt"
        );
        assert!(
            tablet.auto_assign_excluded(8),
            "laufendes Match bleibt ausgenommen"
        );

        // Match 8 verschwindet ganz aus dem Snapshot (z. B. gelöscht) —
        // auch das räumt auf.
        let snap = snap_with(Vec::new(), vec![finished_named(7, 0, "A", "B")], Vec::new());
        engine.reconcile_auto_assign_exclusions(&tablet, &snap);
        assert!(!tablet.auto_assign_excluded(8));
    }

    #[test]
    fn queue_order_wird_bei_spielende_automatisch_entfernt() {
        // Spec `spielliste-manuelle-reihenfolge`, Blocker 3: ein Match
        // verlässt den Präfix automatisch, sobald es beendet ist oder aus
        // dem Snapshot verschwindet.
        let engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.queue_order_store().reorder(&[7, 8], 7, Some(8));
        assert_eq!(tablet.queue_order_store().rank(7), Some(0));

        let snap = snap_with(
            Vec::new(),
            vec![
                finished_named(7, 0, "A", "B"),
                ready_named(8, None, "C", "D"),
            ],
            Vec::new(),
        );
        engine.reconcile_queue_order(&tablet, &snap);
        assert_eq!(
            tablet.queue_order_store().rank(7),
            None,
            "beendetes Match aufgeräumt"
        );

        // Match 8 verschwindet ganz aus dem Snapshot.
        let snap = snap_with(Vec::new(), Vec::new(), Vec::new());
        engine.reconcile_queue_order(&tablet, &snap);
        assert_eq!(tablet.queue_order_store().rank(8), None);
    }

    #[test]
    fn spielzeiten_stempel_folgen_dem_snapshot() {
        // Spec `spielzeiten-prognose` (E4): OnCourt stempelt genau einmal;
        // Finished zählt nie als Feldabnahme; erst drei aufeinanderfolgende
        // Snapshots „Scheduled ohne Feld" verwerfen den Stempel.
        let engine = SyncEngine::new();
        let tablet = TabletState::default();

        let snap = snap_with(
            Vec::new(),
            vec![
                oncourt_named(7, 1, "A", "B"),
                ready_named(8, None, "C", "D"),
            ],
            Vec::new(),
        );
        engine.reconcile_match_times(&tablet, &snap, 1_000);
        assert_eq!(tablet.match_times_store().first_assigned_ms(7), Some(1_000));
        assert_eq!(
            tablet.match_times_store().first_assigned_ms(8),
            None,
            "wartendes Spiel bekommt keinen Stempel"
        );

        // Finished zählt nie als Abnahme — auch nach drei Polls.
        let snap = snap_with(Vec::new(), vec![finished_named(7, 9_000, "A", "B")], Vec::new());
        for t in [2_000, 3_000, 4_000] {
            engine.reconcile_match_times(&tablet, &snap, t);
        }
        assert_eq!(tablet.match_times_store().first_assigned_ms(7), Some(1_000));

        // Zurück auf „Scheduled ohne Feld": nach drei Polls ist der
        // Stempel weg (bestätigte Abnahme, Spiel wird neu angesetzt).
        let snap = snap_with(Vec::new(), vec![ready_named(7, None, "A", "B")], Vec::new());
        for t in [5_000, 6_000, 7_000] {
            engine.reconcile_match_times(&tablet, &snap, t);
        }
        assert_eq!(tablet.match_times_store().first_assigned_ms(7), None);
    }

    #[test]
    fn ein_hallenwechsel_laesst_die_reihenfolge_unangetastet() {
        // ADR 0026: Die Reihenfolge ist global — ein Hallenwechsel räumt
        // seit dem NICHTS mehr auf (früher fiel der Eintrag aus der alten
        // Halle heraus). Das Spiel behält seinen Platz.
        let engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.queue_order_store().reorder(&[7, 8], 7, Some(8));
        assert_eq!(tablet.queue_order_store().rank(7), Some(0));

        // Match 7 bekommt jetzt von Hand die Halle "Halle B".
        tablet.set_manual_hall(7, "Halle B");
        let snap = snap_with(
            Vec::new(),
            vec![
                ready_named(7, None, "A", "B"),
                ready_named(8, None, "C", "D"),
            ],
            vec![
                crate::btp::model::BtpLocation {
                    id: 1,
                    name: "Halle A".to_string(),
                },
                crate::btp::model::BtpLocation {
                    id: 2,
                    name: "Halle B".to_string(),
                },
            ],
        );
        engine.reconcile_queue_order(&tablet, &snap);
        assert_eq!(tablet.queue_order_store().rank(7), Some(0));
    }

    #[test]
    fn auto_assign_skips_excluded_match() {
        // Spec `feldvergabe-ausnahme`: ein ausgenommenes Match wird
        // übersprungen, auch wenn ein Feld frei ist und es sonst spielbereit
        // wäre.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.set_auto_assign_excluded(7, true);
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(
            courts.is_empty(),
            "ausgenommenes Match bleibt unberücksichtigt"
        );

        // Reaktiviert: wird beim nächsten Zyklus wieder berücksichtigt.
        tablet.set_auto_assign_excluded(7, false);
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].match_id, Some(7));
    }

    #[test]
    fn auto_assign_waits_until_court_free_long_enough() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        // Erste Runde mit Wartezeit 5 min: gerade erst frei → noch nichts.
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 5.0), &snap, &tablet);
        assert!(courts.is_empty());
        // Frei-seit künstlich 6 min in die Vergangenheit → jetzt belegen.
        let old = now_ms().saturating_sub(6 * 60_000);
        engine.court_free_since.insert(1, old);
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 5.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].match_id, Some(7));
    }

    #[test]
    fn auto_assign_no_double_assign_one_match_two_courts() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(
            vec![court(1, None), court(2, None)],
            vec![ready_match(7, 1)],
            Vec::new(),
        );
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        // Nur ein Feld belegt – das eine Match nicht doppelt.
        assert_eq!(courts.len(), 1);
    }

    #[test]
    fn auto_assign_does_not_rebook_match_until_btp_confirms() {
        // Regression (HIGH): schreibt der BTP-Write langsam, zeigt der nächste
        // Poll das Match noch als Scheduled und das Feld noch frei. Die
        // Zuweisung darf NICHT ein zweites Mal (auf ein anderes Feld) erfolgen.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(
            vec![court(1, None), court(2, None)],
            vec![ready_match(7, 1)],
            Vec::new(),
        );
        let cfg = cfg_auto(true, 0.0);
        let (first, _) = engine.auto_assign(&cfg, &snap, &tablet);
        assert_eq!(first.len(), 1, "erste Runde: ein Feld belegt");
        // Zweite Runde mit UNVERÄNDERTEM Snapshot (BTP noch nicht bestätigt):
        // Match 7 ist „unterwegs" → keine erneute Vergabe.
        let (second, _) = engine.auto_assign(&cfg, &snap, &tablet);
        assert!(
            second.is_empty(),
            "kein erneutes Buchen vor BTP-Bestätigung"
        );
    }

    #[test]
    fn auto_assign_multi_hall_only_matches_called_for_that_hall() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        // Match 7 wurde für Halle 2 in die Vorbereitung gerufen.
        tablet.add_preparation_call(PreparationCall {
            match_id: 7,
            location_id: Some(2),
            called_at_ms: 0,
        });
        let snap = snap_with(
            vec![court(1, Some(1)), court(2, Some(2))],
            vec![ready_match(7, 1)],
            vec![
                BtpLocation {
                    id: 1,
                    name: "Halle 1".into(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle 2".into(),
                },
            ],
        );
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        // Nur das Feld in Halle 2 (location_id=2) bekommt das Match.
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].court_id, 2);
    }

    // ── Zeit-Reihenfolge + Spieler-Verfügbarkeit ─────────────────────────
    fn ready_named(id: i64, planned: Option<i64>, p1: &str, p2: &str) -> BtpMatch {
        let mut m = ready_match(id, id);
        m.planned_time = planned;
        m.team1 = vec![player(p1)];
        m.team2 = vec![player(p2)];
        m
    }
    fn oncourt_named(id: i64, court_id: i64, p1: &str, p2: &str) -> BtpMatch {
        let mut m = ready_named(id, None, p1, p2);
        m.status = MatchStatus::OnCourt;
        m.court = Some(court_id.to_string());
        m.court_id = Some(court_id);
        m
    }
    fn finished_named(id: i64, end_ms: u64, p1: &str, p2: &str) -> BtpMatch {
        let mut m = ready_named(id, None, p1, p2);
        m.status = MatchStatus::Finished;
        m.winner = Some(1);
        m.finished_at = Some(end_ms);
        m
    }
    fn cfg_auto_pause(wait: f64, pause: f64) -> AppConfig {
        let mut c = cfg_auto(true, wait);
        c.auto_assign.pause_minutes = pause;
        c
    }

    #[test]
    fn player_key_prefers_member_id_then_name() {
        // Die Funktion ist mit der Turnierleitungs-Anzeige geteilt und wird
        // in `tablet::assign` wortgleich geprüft. Hier bleibt sie stehen,
        // weil die Auto-Vergabe auf ihr aufbaut: Ginge die Spieler-Identität
        // kaputt, käme derselbe Spieler auf zwei Felder gleichzeitig.
        let mut p = player("Müller");
        assert_eq!(player_key(&p), "müller");
        p.member_id = Some("  08-001234 ".to_string());
        assert_eq!(player_key(&p), "08-001234");
    }

    #[test]
    fn auto_assign_orders_by_planned_time() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        // Ein freies Feld, zwei spielbereite Spiele – das früher angesetzte gewinnt.
        let snap = snap_with(
            vec![court(1, None)],
            vec![
                ready_named(7, Some(202506141400), "A", "B"),
                ready_named(8, Some(202506141000), "C", "D"),
            ],
            Vec::new(),
        );
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].match_id, Some(8));
    }

    #[test]
    fn auto_assign_prefers_a_manually_advanced_match_over_the_earlier_schedule() {
        // Spec `spielliste-manuelle-reihenfolge`: ein manuell vorgezogenes
        // Match bekommt bevorzugt ein frei werdendes Feld — auch wenn ein
        // anderes Match früher angesetzt ist.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(
            vec![court(1, None)],
            vec![
                ready_named(7, Some(202506141400), "A", "B"), // später angesetzt
                ready_named(8, Some(202506141000), "C", "D"), // früher angesetzt
            ],
            Vec::new(),
        );
        // 7 vor 8 ziehen.
        tablet.queue_order_store().reorder(&[7, 8], 7, Some(8));

        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(
            courts[0].match_id,
            Some(7),
            "manueller Vorrang schlägt PlannedTime"
        );
    }

    #[test]
    fn a_manually_advanced_but_excluded_match_is_still_skipped_by_auto_assign() {
        // Zusammenspiel mit der Feldvergabe-Ausnahme (Constraint aus dem
        // Brief `spielliste-manuelle-reihenfolge`): ein ausgenommenes Spiel
        // bleibt ausgenommen, unabhängig von seiner Präfix-Position — es
        // steht zwar ganz vorn in der Anzeige, wird aber nie automatisch
        // zugewiesen.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(
            vec![court(1, None)],
            vec![
                ready_named(7, Some(202506141400), "A", "B"),
                ready_named(8, Some(202506141000), "C", "D"),
            ],
            Vec::new(),
        );
        tablet.queue_order_store().reorder(&[7, 8], 7, Some(8));
        tablet.set_auto_assign_excluded(7, true);

        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(
            courts.len(),
            1,
            "das Feld bleibt nicht leer, nur weil das vorgezogene Match ausgenommen ist"
        );
        assert_eq!(
            courts[0].match_id,
            Some(8),
            "ausgenommenes Match 7 wird trotz Präfix-Vorrang übersprungen"
        );
    }

    #[test]
    fn auto_assign_skips_player_on_other_court() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(
            vec![court(9, None)],
            vec![
                oncourt_named(1, 5, "A", "B"),     // A spielt gerade
                ready_named(7, Some(1), "A", "X"), // teilt A → überspringen
                ready_named(8, Some(2), "C", "D"), // frei → bekommt das Feld
            ],
            Vec::new(),
        );
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].match_id, Some(8));
    }

    #[test]
    fn auto_assign_same_player_not_on_two_courts() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap = snap_with(
            vec![court(9, None), court(10, None)],
            vec![
                ready_named(7, Some(1), "A", "B"),
                ready_named(8, Some(2), "A", "C"), // teilt A mit 7
            ],
            Vec::new(),
        );
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        // Nur ein Spiel vergeben – A kann nicht auf zwei Felder gleichzeitig.
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].match_id, Some(7));
    }

    #[test]
    fn auto_assign_respects_pause_after_finish() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let now = now_ms();
        let snap = snap_with(
            vec![court(9, None)],
            vec![
                finished_named(1, now - 120_000, "A", "B"), // A vor 2 Min fertig
                ready_named(7, Some(1), "A", "X"),          // A noch in Pause
                ready_named(8, Some(2), "C", "D"),          // C frei
            ],
            Vec::new(),
        );
        // 5-Min-Pause → A (vor 2 Min fertig) übersprungen, C kommt dran.
        let (courts, _) = engine.auto_assign(&cfg_auto_pause(0.0, 5.0), &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].match_id, Some(8));
    }

    #[test]
    fn auto_assign_pause_falls_back_to_btp_setting() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let now = now_ms();
        let mut snap = snap_with(
            vec![court(9, None)],
            vec![
                finished_named(1, now - 120_000, "A", "B"),
                ready_named(7, Some(1), "A", "X"),
            ],
            Vec::new(),
        );
        snap.rest_minutes = Some(5); // BTP-Setting 1303 = 5 Min
                                     // pause_minutes=0 → BTP-Wert greift → A noch in Pause → nichts vergeben.
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(courts.is_empty());
    }

    #[test]
    fn auto_assign_active_hall_assigns_without_call() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let locs = vec![
            BtpLocation {
                id: 1,
                name: "Halle A".into(),
            },
            BtpLocation {
                id: 2,
                name: "Halle B".into(),
            },
        ];
        // Feld 10 in Halle A, Feld 20 in Halle B; ein spielbereites Match, NICHT
        // „in Vorbereitung" gerufen.
        let snap = snap_with(
            vec![court(10, Some(1)), court(20, Some(2))],
            vec![ready_match(7, 1)],
            locs,
        );
        let mut cfg = cfg_auto(true, 0.0);
        cfg.auto_assign.active_hall = "Halle A".to_string();
        let (courts, _) = engine.auto_assign(&cfg, &snap, &tablet);
        // Aktive Halle A → Match landet ohne Aufruf auf Feld 10 (Halle A),
        // nicht auf Feld 20 (Halle B). (Ohne aktive Halle bräuchte es im
        // Mehr-Hallen-Fall einen Aufruf → nichts.)
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].court_id, 10);
    }

    #[test]
    fn auto_assign_active_hall_ignored_in_single_hall() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        // Ein-Hallen-Turnier (keine Locations), Feld ohne location_id, aber
        // active_hall gesetzt → muss ignoriert werden, sonst würde der Hall-
        // Filter das Feld überspringen und nichts vergeben.
        let snap = snap_with(vec![court(9, None)], vec![ready_match(7, 1)], Vec::new());
        let mut cfg = cfg_auto(true, 0.0);
        cfg.auto_assign.active_hall = "Halle A".to_string();
        let (courts, _) = engine.auto_assign(&cfg, &snap, &tablet);
        assert_eq!(courts.len(), 1);
        assert_eq!(courts[0].court_id, 9);
    }

    #[test]
    fn auto_assign_skips_match_with_unknown_opponent() {
        // Scheduled, aber Gegner noch offen (team2 leer) → nicht spielbereit,
        // darf NICHT auf ein Feld gelegt werden.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let mut m = ready_match(7, 1);
        m.team2 = Vec::new();
        let snap = snap_with(vec![court(1, None)], vec![m], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(courts.is_empty(), "unvollständige Paarung nicht vergeben");
    }

    #[test]
    fn auto_assign_reuses_a_court_whose_match_is_over() {
        // **Umgekehrt seit 09.08.2026.** Bis dahin galt: Trägt ein beendetes
        // Spiel noch seine CourtID, bleibt das Feld belegt — ein
        // Sicherheitsnetz gegen Doppelvergabe (v0.9.113).
        //
        // Dieses Netz wurde zur Falle, als der Ergebnis-Pfad im Juli anfing,
        // die CourtID am beendeten Match **absichtlich** stehen zu lassen
        // (Turnier-Doku „wo wurde gespielt", `proto.rs`). Seither räumt sie
        // niemand mehr ab — und das Feld blieb bis zum Turnierende besetzt.
        // Im Test am 09.08. genau so aufgetreten: Feld 03 nahm nach dem
        // ersten Ergebnis kein Spiel mehr an.
        //
        // Vor Doppelvergabe schützt weiterhin die Wartezeit der Automatik
        // (`wait_minutes` auf `court_free_since`) — hier bewusst 0.0, damit
        // der Test die Belegung prüft und nicht die Uhr.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let mut fin = ready_match(5, 1);
        fin.status = MatchStatus::Finished;
        fin.court_id = Some(1);
        fin.winner = Some(1);
        let ready = ready_match(7, 2);
        let snap = snap_with(vec![court(1, None)], vec![fin, ready], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(
            !courts.is_empty(),
            "das Feld des beendeten Spiels steht wieder zur Verfügung"
        );
    }

    // ───────────────────── Leer-Snapshot-Guard ─────────────────────

    #[test]
    fn empty_snapshot_before_any_data_is_not_suspect() {
        // App startet vor dem Turnier-Aufbau: BTP liefert (noch) keine
        // Matches — das ist der Normalzustand, kein Verdachtsfall.
        let mut engine = SyncEngine::new();
        let empty = snap_with(Vec::new(), Vec::new(), Vec::new());
        assert!(!engine.empty_snapshot_is_suspect(&empty));
        assert!(!engine.empty_snapshot_is_suspect(&empty));
    }

    #[test]
    fn single_empty_snapshot_after_data_is_discarded() {
        // Turnier-Befund 19.07.: BTP lieferte EINEN Abruf lang 0 Matches →
        // ohne Guard Massen-Freigabe aller Felder. Der erste leere Abruf
        // nach gefüllten Daten wird verworfen.
        let mut engine = SyncEngine::new();
        let full = snap_with(Vec::new(), vec![ready_match(1, 1)], Vec::new());
        let empty = snap_with(Vec::new(), Vec::new(), Vec::new());
        assert!(!engine.empty_snapshot_is_suspect(&full));
        assert!(
            engine.empty_snapshot_is_suspect(&empty),
            "1. leerer Abruf → verwerfen"
        );
    }

    #[test]
    fn second_consecutive_empty_snapshot_is_accepted() {
        // Bestätigt BTP den leeren Stand im Folge-Poll, ist er die Wahrheit
        // (R2) — z. B. Turnier in BTP geschlossen. Danach ist leer der
        // bekannte Zustand: keine weiteren Verwerfungen.
        let mut engine = SyncEngine::new();
        let full = snap_with(Vec::new(), vec![ready_match(1, 1)], Vec::new());
        let empty = snap_with(Vec::new(), Vec::new(), Vec::new());
        assert!(!engine.empty_snapshot_is_suspect(&full));
        assert!(engine.empty_snapshot_is_suspect(&empty));
        assert!(
            !engine.empty_snapshot_is_suspect(&empty),
            "2. leerer Abruf → übernehmen"
        );
        assert!(
            !engine.empty_snapshot_is_suspect(&empty),
            "leer bleibt akzeptiert"
        );
    }

    #[test]
    fn returning_matches_rearm_the_guard() {
        // Kommen nach einem verworfenen leeren Abruf wieder Matches, war es
        // ein Aussetzer: Zähler zurück, der NÄCHSTE leere Abruf wird wieder
        // als erster (verdächtiger) gewertet.
        let mut engine = SyncEngine::new();
        let full = snap_with(Vec::new(), vec![ready_match(1, 1)], Vec::new());
        let empty = snap_with(Vec::new(), Vec::new(), Vec::new());
        assert!(!engine.empty_snapshot_is_suspect(&full));
        assert!(engine.empty_snapshot_is_suspect(&empty));
        assert!(
            !engine.empty_snapshot_is_suspect(&full),
            "Daten zurück → alles normal"
        );
        assert!(
            engine.empty_snapshot_is_suspect(&empty),
            "neuer Aussetzer → wieder verwerfen"
        );
    }

    // ───────────────────── Nachschub-Queue (A5) ─────────────────────

    use crate::tablet::state::PendingBtpWrite;

    fn upd(match_id: i64, free_court: Option<i64>) -> crate::btp::proto::MatchUpdate {
        crate::btp::proto::MatchUpdate {
            btp_match_id: match_id,
            draw_id: 1,
            planning_id: 1000 + match_id,
            sets: vec![(21, 15), (21, 17)],
            team1_won: true,
            duration_mins: 33,
            score_status: 0,
            free_court_id: free_court,
            player_ids: vec![11, 12],
            end_ts_ms: Some(500_000),
            officials: None,
        }
    }

    fn pending(match_id: i64, free_court: Option<i64>, enqueued_ms: u64) -> PendingBtpWrite {
        PendingBtpWrite {
            update: upd(match_id, free_court),
            enqueued_ms,
        }
    }

    #[test]
    fn retry_is_dropped_when_btp_already_has_a_result() {
        // Turnierleitung hat das Ergebnis inzwischen manuell nachgetragen →
        // der Nachschub darf es NIE überschreiben.
        let snap = snap_with(Vec::new(), vec![finished_named(7, 1, "A", "B")], Vec::new());
        assert_eq!(
            prepare_btp_retry(&pending(7, None, 0), &snap, 1_000, &TabletState::default()),
            RetryAction::Drop("BTP hat bereits ein Ergebnis")
        );
    }

    #[test]
    fn retry_is_dropped_after_max_age() {
        let snap = snap_with(Vec::new(), Vec::new(), Vec::new());
        let too_old = BTP_RETRY_MAX_AGE.as_millis() as u64 + 1;
        assert_eq!(
            prepare_btp_retry(
                &pending(7, None, 0),
                &snap,
                too_old,
                &TabletState::default()
            ),
            RetryAction::Drop("Eintrag zu alt")
        );
    }

    #[test]
    fn retry_strips_player_checkout_after_five_minutes() {
        // Tilos 5-Minuten-Guard: späte Replays dürfen Spieler nicht erneut
        // auschecken/umstempeln — Ergebnis + Sätze bleiben unverändert.
        let snap = snap_with(Vec::new(), Vec::new(), Vec::new());
        let late = PLAYER_CHECKOUT_WINDOW.as_millis() as u64 + 1;
        let RetryAction::Write(u) =
            prepare_btp_retry(&pending(7, None, 0), &snap, late, &TabletState::default())
        else {
            panic!("Write erwartet");
        };
        assert!(u.player_ids.is_empty());
        assert_eq!(u.end_ts_ms, None);
        assert_eq!(u.sets, vec![(21, 15), (21, 17)], "Ergebnis unangetastet");
    }

    #[test]
    fn retry_keeps_court_release_only_while_court_is_still_ours() {
        // Feld trägt laut Snapshot noch UNSER Match → Freigabe bleibt.
        let mut ours = ready_match(7, 1);
        ours.court_id = Some(5);
        let snap = snap_with(Vec::new(), vec![ours], Vec::new());
        let RetryAction::Write(u) = prepare_btp_retry(
            &pending(7, Some(5), 0),
            &snap,
            1_000,
            &TabletState::default(),
        ) else {
            panic!("Write erwartet");
        };
        assert_eq!(u.free_court_id, Some(5));

        // Feld inzwischen anderweitig belegt (unser Match hat es verloren) →
        // Freigabe entfällt, sonst räumte das Replay die neue Zuweisung weg.
        let mut other = ready_match(9, 2);
        other.court_id = Some(5);
        let snap2 = snap_with(Vec::new(), vec![other], Vec::new());
        let RetryAction::Write(u2) = prepare_btp_retry(
            &pending(7, Some(5), 0),
            &snap2,
            1_000,
            &TabletState::default(),
        ) else {
            panic!("Write erwartet");
        };
        assert_eq!(u2.free_court_id, None);
    }

    #[test]
    fn fresh_retry_is_written_unchanged() {
        let mut ours = ready_match(7, 1);
        ours.court_id = Some(5);
        let snap = snap_with(Vec::new(), vec![ours], Vec::new());
        let entry = pending(7, Some(5), 0);
        let RetryAction::Write(u) =
            prepare_btp_retry(&entry, &snap, 1_000, &TabletState::default())
        else {
            panic!("Write erwartet");
        };
        assert_eq!(*u, entry.update, "frischer Eintrag geht 1:1 raus");
    }

    #[test]
    fn retry_refreshes_a_stale_officials_value_instead_of_replaying_it() {
        // Code-Review-Fund 14.08.2026: Zwischen Einreihen und Nachschub kann
        // die Turnierleitung die Besetzung korrigiert haben — der beim
        // Einreihen eingefrorene Wert darf die Korrektur nicht überschreiben.
        let tablet = TabletState::default();
        tablet.officials_store().set_enabled(true);
        let mut ours = ready_match(7, 1);
        ours.court_id = Some(5);
        let snap = snap_with(Vec::new(), vec![ours], Vec::new());
        // Nach dem Einreihen (mit Besetzung 5,0) korrigiert die
        // Turnierleitung auf Official 7.
        tablet
            .officials_store()
            .assign(7, crate::tablet::officials::OfficialRole::Sr, 7);

        let mut entry = pending(7, Some(5), 0);
        entry.update.officials = Some((5, 0));
        let RetryAction::Write(u) = prepare_btp_retry(&entry, &snap, 1_000, &tablet) else {
            panic!("Write erwartet");
        };
        assert_eq!(
            u.officials,
            Some((7, 0)),
            "der Nachschub schreibt die aktuelle Besetzung, nicht die beim \
             Einreihen eingefrorene"
        );
    }

    // ──────────────── Spielende-Stempel & Zähltafelbediener ────────────────

    #[test]
    fn stamp_finished_stamps_once_and_keeps_timestamp() {
        // BTP liefert kein Endezeitpunkt-Feld — wir stempeln beim ERSTEN
        // Poll, der das Spiel als beendet sieht, und der Stempel bleibt über
        // alle folgenden Zyklen stabil (Pausen-Logik + Ticker hängen daran).
        let mut engine = SyncEngine::new();
        let mut snap = snap_with(
            Vec::new(),
            vec![finished_named(1, 0, "A", "B"), ready_match(2, 2)],
            Vec::new(),
        );
        snap.matches[0].finished_at = None;
        engine.stamp_finished(&mut snap);
        let first = snap.matches[0].finished_at.expect("beendet → gestempelt");
        assert!(
            snap.matches[1].finished_at.is_none(),
            "laufend/geplant bleibt ungestempelt"
        );

        // Nächster Poll-Zyklus: frischer Snapshot, gleicher Stempel.
        let mut snap2 = snap_with(Vec::new(), vec![finished_named(1, 0, "A", "B")], Vec::new());
        snap2.matches[0].finished_at = None;
        engine.stamp_finished(&mut snap2);
        assert_eq!(snap2.matches[0].finished_at, Some(first));
    }

    #[test]
    fn officials_writes_only_the_difference_to_btp() {
        // ADR 0021: geschrieben wird, was BTP noch nicht trägt — und nur das.
        let tablet = TabletState::default();
        tablet.officials_store().set_enabled(true);
        let mut m1 = oncourt_named(10, 5, "A", "B");
        m1.draw_id = 3;
        m1.planning_id = 1002;
        let mut m2 = oncourt_named(11, 6, "C", "D");
        m2.draw_id = 3;
        m2.planning_id = 1003;
        // BTP trägt an Match 11 schon denselben Schiedsrichter.
        m2.official1_id = Some(2);
        let snap = snap_officials(vec![m1, m2], &[1, 2]);
        tablet.set_snapshot(snap.clone());
        tablet
            .officials_store()
            .assign(10, crate::tablet::officials::OfficialRole::Sr, 1);
        tablet
            .officials_store()
            .assign(11, crate::tablet::officials::OfficialRole::Sr, 2);

        let offen = officials_entries(&tablet, &snap, &HashMap::new());
        assert_eq!(offen.len(), 1, "nur Match 10 weicht ab");
        assert_eq!(offen[0].match_id, 10);
        assert_eq!(offen[0].draw_id, 3);
        assert_eq!(offen[0].planning_id, 1002);
        assert_eq!(offen[0].officials, Some((1, 0)), "kein AR ⇒ 0");

        // Schon geschrieben (und von BTP noch nicht zurückgemeldet) ⇒ nicht
        // erneut schreiben, sonst liefe jeder Zyklus in denselben Write.
        let mut geschrieben = HashMap::new();
        geschrieben.insert(10i64, (1i64, 0i64));
        assert!(officials_entries(&tablet, &snap, &geschrieben).is_empty());

        // Ohne Schiedsrichter-Betrieb wird gar nichts geschrieben.
        tablet.officials_store().set_enabled(false);
        assert!(officials_entries(&tablet, &snap, &HashMap::new()).is_empty());
    }

    #[test]
    fn officials_write_clears_a_removed_assignment_with_zero() {
        // Löschen ist die 0 — sonst bliebe ein abgezogener Schiedsrichter in
        // BTP stehen.
        let tablet = TabletState::default();
        tablet.officials_store().set_enabled(true);
        let mut m = oncourt_named(10, 5, "A", "B");
        m.draw_id = 3;
        m.planning_id = 1002;
        m.official1_id = Some(1); // BTP kennt ihn noch
        let snap = snap_officials(vec![m], &[1]);
        tablet.set_snapshot(snap.clone());
        // Lokal ausdrücklich entfernt.
        tablet
            .officials_store()
            .clear_assignment(10, crate::tablet::officials::OfficialRole::Sr);

        let offen = officials_entries(&tablet, &snap, &HashMap::new());
        assert_eq!(offen.len(), 1);
        assert_eq!(
            offen[0].officials,
            Some((0, 0)),
            "das ausdrückliche Lösen geht als 0 nach BTP — sonst bliebe der \
             Schiedsrichter dort für immer stehen"
        );
    }

    #[test]
    fn officials_entries_always_reasserts_the_current_court_id() {
        // Live-Befund 14.08.2026 (Zwei-Hallen-Turnier, Match 1216/Feld 8,
        // erneut beobachtet nach einer testweise eingeführten 10s-Karenzzeit
        // — der Abstand zum zweiten Write war real z. T. 11–18s, eine feste
        // Wartezeit reicht also nicht): Ein eigenständiges
        // Schiedsrichter-SENDUPDATE, das die CourtID wegliess, ließ BTP die
        // gerade erst angekommene Feldzuweisung verlieren. Der Abgleich
        // schreibt die aktuelle CourtID deshalb IMMER mit (siehe
        // `MatchCourt::court_id`) — dann ist die Reihenfolge zweier Writes
        // zum selben Match folgenlos, unabhängig vom zeitlichen Abstand.
        let tablet = TabletState::default();
        tablet.officials_store().set_enabled(true);
        let mut m = oncourt_named(10, 8, "A", "B"); // Feld 8
        m.draw_id = 3;
        m.planning_id = 1002;
        let snap = snap_officials(vec![m], &[1, 2]);
        tablet.set_snapshot(snap.clone());
        tablet
            .officials_store()
            .assign(10, crate::tablet::officials::OfficialRole::Sr, 1);

        let offen = officials_entries(&tablet, &snap, &HashMap::new());
        assert_eq!(offen.len(), 1);
        assert_eq!(
            offen[0].court_id, 8,
            "die aktuell bekannte CourtID reist immer mit, egal wie kurz nach der \
             Feldzuweisung der Abgleich schreibt"
        );
        assert_eq!(offen[0].officials, Some((1, 0)));

        // Ein Match, das noch nie auf einem Feld stand, schreibt 0 — das ist
        // dieselbe „nicht gepflegt"-Bedeutung, die BTP dort ohnehin zeigt,
        // keine Löschung einer echten Zuweisung.
        let mut ohne_feld = ready_named(20, None, "C", "D");
        ohne_feld.draw_id = 3;
        ohne_feld.planning_id = 2001;
        let snap2 = snap_officials(vec![ohne_feld], &[1, 2]);
        tablet.set_snapshot(snap2.clone());
        tablet
            .officials_store()
            .assign(20, crate::tablet::officials::OfficialRole::Sr, 2);
        let offen2 = officials_entries(&tablet, &snap2, &HashMap::new());
        assert_eq!(offen2.len(), 1);
        assert_eq!(offen2[0].court_id, 0);
    }

    // --- Schiedsrichter-Rotation (Spec schiedsrichter-management, Nr. 4) ---

    fn official_named(id: i64) -> crate::btp::model::BtpOfficial {
        crate::btp::model::BtpOfficial {
            id,
            name: format!("Schiri{id}"),
            first: String::new(),
            nationality: None,
        }
    }

    /// Snapshot mit Officials-Liste.
    fn snap_officials(matches: Vec<BtpMatch>, ids: &[i64]) -> BtpSnapshot {
        let mut s = snap_with(Vec::new(), matches, Vec::new());
        s.officials = ids.iter().copied().map(official_named).collect();
        s
    }

    /// Engine + Tablet mit eingeschaltetem Schiedsrichter-Betrieb.
    fn officials_setup(rot_sr: bool, rot_ar: bool) -> (SyncEngine, TabletState) {
        let tablet = TabletState::default();
        tablet.officials_store().set_enabled(true);
        tablet.officials_store().set_rotation(rot_sr, rot_ar);
        (SyncEngine::new(), tablet)
    }

    #[test]
    fn track_officials_bestueckt_ein_neu_belegtes_feld() {
        let (mut engine, tablet) = officials_setup(true, false);
        let snap = snap_officials(vec![oncourt_named(10, 5, "A", "B")], &[1, 2, 3]);
        tablet.set_snapshot(snap.clone());

        engine.track_officials(&snap, &tablet);
        let store = tablet.officials_store();
        assert_eq!(store.assignment(10).sr, Some(1));
        assert_eq!(store.assignment(10).ar, None, "AR-Rotation ist aus");
    }

    #[test]
    fn track_officials_fuellt_eine_entfernte_zuweisung_nicht_wieder_auf() {
        // Spec Nr. 4: Bestückt wird beim NEU-Belegen. Wer bewusst ohne SR
        // spielen lässt, darf ihn nicht im nächsten Poll zurückbekommen.
        let (mut engine, tablet) = officials_setup(true, false);
        let snap = snap_officials(vec![oncourt_named(10, 5, "A", "B")], &[1, 2]);
        tablet.set_snapshot(snap.clone());
        engine.track_officials(&snap, &tablet);
        assert_eq!(tablet.officials_store().assignment(10).sr, Some(1));

        tablet
            .officials_store()
            .clear_assignment(10, crate::tablet::officials::OfficialRole::Sr);
        engine.track_officials(&snap, &tablet);
        assert_eq!(
            tablet.officials_store().assignment(10).sr,
            Some(0),
            "unverändertes Feld wird nicht neu bestückt; die Löschung bleibt \
             als ausdrückliches „keiner“ stehen — so geht sie nach BTP"
        );
    }

    #[test]
    fn track_officials_rueckt_nach_spielende_ans_ende_und_behaelt_die_zuweisung() {
        // Nach dem Spiel ans Ende der Reihenfolge (Spec Nr. 4) — die
        // Zuweisung selbst bleibt am Match stehen (Spec Nr. 11,
        // Einsatz-Ableitung).
        let (mut engine, tablet) = officials_setup(true, true);
        let snap1 = snap_officials(vec![oncourt_named(10, 5, "A", "B")], &[1, 2, 3]);
        tablet.set_snapshot(snap1.clone());
        engine.track_officials(&snap1, &tablet);
        assert_eq!(tablet.officials_store().order(), vec![1, 2, 3]);

        let snap2 = snap_officials(vec![finished_named(10, 42, "A", "B")], &[1, 2, 3]);
        tablet.set_snapshot(snap2.clone());
        engine.track_officials(&snap2, &tablet);
        assert_eq!(
            tablet.officials_store().order(),
            vec![3, 1, 2],
            "SR und AR des beendeten Spiels rücken ans Ende"
        );
        assert_eq!(
            tablet.officials_store().assignment(10).sr,
            Some(1),
            "Zuweisung bleibt dem beendeten Spiel erhalten"
        );
    }

    #[test]
    fn track_officials_ans_ende_ruecken_ist_bei_mehreren_feldern_nach_courtid_sortiert() {
        // Zwei Felder werden im selben Zyklus fertig — welches zuerst ans
        // Ende der Reihenfolge rückt, muss wie bei der Zuteilung nach
        // CourtID sortiert sein, nicht von der HashMap-Iterationsreihenfolge
        // abhängen (sonst wäre das Ergebnis von Poll-Zyklus zu Poll-Zyklus
        // unvorhersehbar).
        let (mut engine, tablet) = officials_setup(true, true);
        let snap1 = snap_officials(
            vec![
                oncourt_named(10, 7, "A", "B"), // höhere CourtID
                oncourt_named(11, 3, "C", "D"), // niedrigere CourtID
            ],
            &[1, 2, 3, 4],
        );
        tablet.set_snapshot(snap1.clone());
        engine.track_officials(&snap1, &tablet);
        assert_eq!(tablet.officials_store().assignment(11).sr, Some(1));
        assert_eq!(tablet.officials_store().assignment(10).sr, Some(3));

        let snap2 = snap_officials(
            vec![
                finished_named(10, 1, "A", "B"),
                finished_named(11, 2, "C", "D"),
            ],
            &[1, 2, 3, 4],
        );
        tablet.set_snapshot(snap2.clone());
        engine.track_officials(&snap2, &tablet);
        assert_eq!(
            tablet.officials_store().order(),
            vec![1, 2, 3, 4],
            "Feld 3 (kleinere CourtID) rückt zuerst ans Ende (1,2), danach Feld 7 (3,4) — \
             deterministisch nach CourtID, nicht nach HashMap-Reihenfolge"
        );
    }

    #[test]
    fn track_officials_vergibt_niemanden_doppelt_ueber_zwei_felder() {
        let (mut engine, tablet) = officials_setup(true, true);
        let snap = snap_officials(
            vec![
                oncourt_named(10, 5, "A", "B"),
                oncourt_named(11, 6, "C", "D"),
            ],
            &[1, 2, 3, 4],
        );
        tablet.set_snapshot(snap.clone());
        engine.track_officials(&snap, &tablet);
        let store = tablet.officials_store();
        // Feld 5 (kleinere CourtID) zuerst: 1+2, dann Feld 6: 3+4.
        assert_eq!(
            store.assignment(10),
            crate::tablet::officials::MatchOfficials {
                sr: Some(1),
                ar: Some(2)
            }
        );
        assert_eq!(
            store.assignment(11),
            crate::tablet::officials::MatchOfficials {
                sr: Some(3),
                ar: Some(4)
            }
        );
    }

    #[test]
    fn track_officials_respektiert_den_feldschalter() {
        let (mut engine, tablet) = officials_setup(true, true);
        let snap = snap_officials(vec![oncourt_named(10, 5, "A", "B")], &[1, 2]);
        tablet.set_snapshot(snap.clone());
        tablet.officials_store().set_court_switches(
            5,
            crate::tablet::officials::CourtSwitches {
                sr: false,
                ar: true,
                operator: true,
            },
        );
        engine.track_officials(&snap, &tablet);
        let a = tablet.officials_store().assignment(10);
        assert_eq!(a.sr, None, "SR-Rotation ist für dieses Feld aus");
        assert_eq!(a.ar, Some(1), "AR-Rotation läuft weiter");
    }

    #[test]
    fn track_officials_raeumt_alles_wenn_der_globale_schalter_aus_ist() {
        // Spec Nr. 1: Abschalten mitten im Turnier räumt die Zuweisungen —
        // sonst bliebe ein Name in einer Anzeige hängen.
        let (mut engine, tablet) = officials_setup(true, true);
        let snap = snap_officials(vec![oncourt_named(10, 5, "A", "B")], &[1, 2]);
        tablet.set_snapshot(snap.clone());
        engine.track_officials(&snap, &tablet);
        assert!(!tablet.officials_store().assignments().is_empty());

        tablet.officials_store().set_enabled(false);
        engine.track_officials(&snap, &tablet);
        assert!(tablet.officials_store().assignments().is_empty());
    }

    #[test]
    fn track_scorekeepers_remembers_loser_after_finish() {
        // Turnier-Regel: Der Verlierer zählt das nächste Spiel auf dem Feld.
        // Zyklus 1: Match 1 läuft auf Feld 5 — Zyklus 2: beendet, Sieger
        // Team 1 → Verlierer „B" wird als Zähltafelbediener von Feld 5 gemerkt.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, false);
        assert!(
            tablet.scorekeeper(5).is_empty(),
            "läuft noch → kein Bediener"
        );

        let snap2 = snap_with(
            Vec::new(),
            vec![finished_named(1, 42, "A", "B")],
            Vec::new(),
        );
        engine.track_scorekeepers(&snap2, &tablet, false);
        assert_eq!(tablet.scorekeeper(5), vec!["B".to_string()]);
    }

    #[test]
    fn track_scorekeepers_marks_finalized_on_court() {
        // A2 / ADR 0017, Regel b: Der Übergang OnCourt→Finished setzt den
        // Finalisiert-Merker (Feld + Match-ID) — Grundlage dafür, dem Tablet
        // `finalized:true` zu schicken und einen nachlaufenden Score zu
        // verwerfen. Zyklus 1: Match 1 läuft auf Feld 5; Zyklus 2: beendet.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, false);
        assert_eq!(
            tablet.recently_finalized(5),
            None,
            "läuft noch → nicht finalisiert"
        );
        let snap2 = snap_with(
            Vec::new(),
            vec![finished_named(1, 42, "A", "B")],
            Vec::new(),
        );
        engine.track_scorekeepers(&snap2, &tablet, false);
        assert_eq!(
            tablet.recently_finalized(5),
            Some(1),
            "beendet → Merker trägt die Match-ID"
        );
        assert!(tablet.is_match_finalized(5, 1));
        assert!(
            !tablet.is_match_finalized(5, 999),
            "andere Match-ID → nicht finalisiert"
        );
    }

    #[test]
    fn track_scorekeepers_clears_finalized_on_new_match() {
        // A2 / ADR 0017, Regel b (Ablauf des Merkers): Bekommt das Feld ein
        // NEUES Match, ist der alte Finalisiert-Merker Geschichte — sonst
        // gälte ein Score des neuen Spiels fälschlich als finalisiert.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, false);
        let snap2 = snap_with(
            Vec::new(),
            vec![finished_named(1, 42, "A", "B")],
            Vec::new(),
        );
        engine.track_scorekeepers(&snap2, &tablet, false);
        assert_eq!(tablet.recently_finalized(5), Some(1));
        // Neues Match 2 läuft nun auf Feld 5 → Merker geräumt.
        let snap3 = snap_with(Vec::new(), vec![oncourt_named(2, 5, "C", "D")], Vec::new());
        engine.track_scorekeepers(&snap3, &tablet, false);
        assert_eq!(
            tablet.recently_finalized(5),
            None,
            "neues Match räumt den Merker"
        );
    }

    #[test]
    fn scorekeeper_queue_enqueues_loser_of_regular_finish() {
        // ADR 0007: bei regulär beendetem Spiel wird der Verlierer in die
        // globale Warteschlange eingereiht (manage_queue = true).
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, true);
        assert!(tablet.scorekeeper_queue().is_empty());
        let snap2 = snap_with(
            Vec::new(),
            vec![finished_named(1, 42, "A", "B")],
            Vec::new(),
        );
        engine.track_scorekeepers(&snap2, &tablet, true);
        let q = tablet.scorekeeper_queue();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].names, vec!["B".to_string()]);
        assert_eq!(q[0].from_court_id, 5);
    }

    #[test]
    fn scorekeeper_queue_skips_walkover_finish() {
        // Walkover erzeugt keinen Zähltafelbediener (Tilo: nur reguläre Spiele).
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, true);
        let mut wo = finished_named(1, 42, "A", "B");
        wo.result = MatchResult::Walkover;
        let snap2 = snap_with(Vec::new(), vec![wo], Vec::new());
        engine.track_scorekeepers(&snap2, &tablet, true);
        assert!(tablet.scorekeeper_queue().is_empty());
    }

    #[test]
    fn scorekeeper_assignment_cleared_when_management_disabled() {
        // Review-Befund (HIGH): wird die Verwaltung mitten im Turnier aus-
        // geschaltet, darf keine alte Zuweisung in der Anzeige hängen bleiben.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.enqueue_scorekeeper(1, vec!["A".into()], 5, 1_000);
        let snap = snap_with(Vec::new(), vec![oncourt_named(9, 5, "X", "Y")], Vec::new());
        engine.track_scorekeepers(&snap, &tablet, true);
        assert!(
            tablet.assigned_scorekeeper(5).is_some(),
            "manage=on → zugewiesen"
        );
        engine.track_scorekeepers(&snap, &tablet, false);
        assert!(
            tablet.assigned_scorekeeper(5).is_none(),
            "manage=off → geräumt"
        );
    }

    #[test]
    fn scorekeeper_assignment_deterministic_by_court_order() {
        // Zwei Felder gleichzeitig neu belegt, ein Wartender ohne „eigenes Feld"
        // → das Feld mit der KLEINEREN CourtID bekommt ihn (sortierte Zuweisung,
        // nicht die zufällige HashMap-Reihenfolge).
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.enqueue_scorekeeper(1, vec!["A".into()], 99, 1_000);
        let snap = snap_with(
            Vec::new(),
            vec![
                oncourt_named(10, 3, "X", "Y"),
                oncourt_named(11, 7, "P", "Q"),
            ],
            Vec::new(),
        );
        engine.track_scorekeepers(&snap, &tablet, true);
        assert_eq!(
            tablet.assigned_scorekeeper(3),
            Some(vec!["A".to_string()]),
            "kleinere CourtID zuerst"
        );
        assert_eq!(tablet.assigned_scorekeeper(7), None, "Schlange danach leer");
    }

    #[test]
    fn scorekeeper_queue_off_when_disabled() {
        // manage_queue = false → keine Warteschlange (per-Feld-Hinweis bleibt).
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, false);
        let snap2 = snap_with(
            Vec::new(),
            vec![finished_named(1, 42, "A", "B")],
            Vec::new(),
        );
        engine.track_scorekeepers(&snap2, &tablet, false);
        assert!(tablet.scorekeeper_queue().is_empty());
        assert_eq!(
            tablet.scorekeeper(5),
            vec!["B".to_string()],
            "Hinweis bleibt"
        );
    }

    #[test]
    fn track_scorekeepers_ignores_match_leaving_court_unfinished() {
        // Verlässt ein Spiel das Feld OHNE beendet zu sein (z. B. Zuweisung in
        // BTP zurückgenommen), gibt es keinen Verlierer → kein Bediener-Eintrag.
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        let snap1 = snap_with(Vec::new(), vec![oncourt_named(1, 5, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap1, &tablet, false);

        let snap2 = snap_with(Vec::new(), vec![ready_named(1, None, "A", "B")], Vec::new());
        engine.track_scorekeepers(&snap2, &tablet, false);
        assert!(tablet.scorekeeper(5).is_empty());
    }

    // --- Meldeliste fuer den Hallen-Check-In (A-5) -------------------------

    /// Mini-HTTP-Mock wie in `badhub::push`: nimmt eine Anfrage entgegen,
    /// antwortet mit der vorgegebenen Statuszeile und meldet den empfangenen
    /// Rumpf zurueck.
    async fn spawn_checkin_mock(
        status_line: &'static str,
    ) -> (String, std::sync::Arc<tokio::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 65536];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                sink.lock()
                    .await
                    .push(String::from_utf8_lossy(&buf[..n]).to_string());
                let body = r#"{"status":"ok"}"#;
                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{addr}/api/live_update.php"), seen)
    }

    fn roster_snapshot() -> BtpSnapshot {
        let mut s = snapshot();
        s.events = vec![crate::btp::model::BtpEvent {
            id: 1,
            name: "HE A".to_string(),
            discipline: crate::btp::model::Discipline::MensSingles,
        }];
        s.entries = vec![crate::btp::model::BtpEntry {
            id: 10,
            event_id: 1,
            players: vec![crate::btp::model::BtpPlayer {
                id: 1,
                name: "Anna Beispiel".to_string(),
                first: "Anna".to_string(),
                last: "Beispiel".to_string(),
                member_id: None,
                nationality: None,
                club: None,
            }],
        }];
        s
    }

    impl SyncEngine {
        /// Plant und sendet die Meldeliste in einem Schritt — im Produktivcode
        /// liegt der Liveticker-Push dazwischen.
        async fn push_roster_for_test(
            &mut self,
            config: &AppConfig,
            http: &reqwest::Client,
            snapshot: &BtpSnapshot,
        ) {
            if let Some(roster) = self.plan_checkin_roster(config, snapshot) {
                self.send_checkin_roster(config, http, roster).await;
            }
        }
    }

    fn checkin_config(url: String, uuid: &str, enabled: bool) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.badhub.url = url;
        cfg.badhub.password = "pw".to_string();
        cfg.checkin.enabled = enabled;
        cfg.checkin.tournament_uuid = uuid.to_string();
        cfg
    }

    #[tokio::test]
    async fn roster_is_pushed_once_and_not_repeated_while_unchanged() {
        let (url, seen) = spawn_checkin_mock("200 OK").await;
        let cfg = checkin_config(url, "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA", true);
        let http = crate::badhub::push::build_client();
        let mut engine = SyncEngine::new();
        let snap = roster_snapshot();

        engine.push_roster_for_test(&cfg, &http, &snap).await;
        engine.push_roster_for_test(&cfg, &http, &snap).await;

        let requests = seen.lock().await;
        assert_eq!(
            requests.len(),
            1,
            "unveraenderte Meldeliste nur einmal senden"
        );
        assert!(requests[0].contains("centry_list"));
        assert!(requests[0].contains("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"));
    }

    #[tokio::test]
    async fn roster_is_not_pushed_without_a_tournament_guid() {
        // Eingeschaltet, aber nicht eingerichtet: badhub wuesste nicht, zu
        // welchem Turnier die Liste gehoert.
        let (url, seen) = spawn_checkin_mock("200 OK").await;
        let cfg = checkin_config(url, "", true);
        let http = crate::badhub::push::build_client();
        let mut engine = SyncEngine::new();

        engine
            .push_roster_for_test(&cfg, &http, &roster_snapshot())
            .await;
        assert!(seen.lock().await.is_empty());
    }

    #[tokio::test]
    async fn roster_is_not_pushed_when_checkin_is_off() {
        let (url, seen) = spawn_checkin_mock("200 OK").await;
        let cfg = checkin_config(url, "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA", false);
        let http = crate::badhub::push::build_client();
        let mut engine = SyncEngine::new();

        engine
            .push_roster_for_test(&cfg, &http, &roster_snapshot())
            .await;
        assert!(seen.lock().await.is_empty());
    }

    #[tokio::test]
    async fn an_old_badhub_pauses_the_roster_push_but_is_retried_later() {
        // 404 heisst hier nicht "kaputt", sondern "badhub kennt den Check-In
        // noch nicht" — dann nicht jeden Zyklus erneut anklopfen, aber auch
        // nicht fuer immer aufgeben.
        let (url, seen) = spawn_checkin_mock("404 Not Found").await;
        let cfg = checkin_config(url, "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA", true);
        let http = crate::badhub::push::build_client();
        let mut engine = SyncEngine::new();
        let snap = roster_snapshot();

        engine.push_roster_for_test(&cfg, &http, &snap).await;
        assert!(engine.checkin_unsupported_since.is_some());
        engine.push_roster_for_test(&cfg, &http, &snap).await;
        assert_eq!(
            seen.lock().await.len(),
            1,
            "innerhalb der Sperrfrist nicht erneut anklopfen"
        );

        // Nach Ablauf der Sperrfrist wird erneut versucht — ein badhub-Deploy
        // waehrend eines mehrtaegigen Turniers soll noch am selben Tag greifen.
        engine.checkin_unsupported_since =
            Some(Instant::now() - CHECKIN_UNSUPPORTED_RETRY - Duration::from_secs(1));
        engine.push_roster_for_test(&cfg, &http, &snap).await;
        assert_eq!(
            seen.lock().await.len(),
            2,
            "nach der Sperrfrist erneut versuchen"
        );
    }

    #[tokio::test]
    async fn a_failed_roster_push_is_retried_with_the_full_list() {
        // 500: last_roster bleibt leer, der naechste Zyklus sendet erneut.
        let (url, seen) = spawn_checkin_mock("500 Internal Server Error").await;
        let cfg = checkin_config(url, "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA", true);
        let http = crate::badhub::push::build_client();
        let mut engine = SyncEngine::new();
        let snap = roster_snapshot();

        engine.push_roster_for_test(&cfg, &http, &snap).await;
        assert!(engine.last_roster.is_none());
        assert!(
            engine.checkin_unsupported_since.is_none(),
            "500 pausiert das Feature nicht"
        );
        engine.push_roster_for_test(&cfg, &http, &snap).await;
        assert_eq!(seen.lock().await.len(), 2);
    }
}
