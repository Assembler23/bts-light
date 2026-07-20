//! Sync-Engine: ein Poll-Push-Zyklus BTP → Badhub.
//!
//! Strategie bei Push-Fehlern (Resend-on-failure): Es gibt keine
//! persistente Outbox. Schlägt ein Push fehl, wird der zuletzt gesendete
//! Stand verworfen – der nächste Zyklus sendet dann einen vollen `tset`
//! mit dem aktuellen Komplettstand. Die Turnierdaten liegen ohnehin in
//! BTP und werden bei jedem Zyklus neu abgefragt.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::badhub::diff::{diff, Update};
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

/// Stabiler Schlüssel zur Spieler-Identität für die Verfügbarkeitsprüfung der
/// Auto-Feldvergabe: bevorzugt die Lizenznummer (`member_id`), sonst der
/// normalisierte Name. So greift die Prüfung auch über Disziplinen hinweg
/// (dieselbe Person hat je Disziplin eine andere EntryID, aber dieselbe Lizenz).
/// Achtung: Ohne `member_id` (Turniere ohne Lizenzen) können zwei verschiedene
/// Spieler mit identischem Namen verschmelzen – in lizenzierten Turnieren ist
/// die `member_id` praktisch immer gesetzt, daher hier akzeptiert.
fn player_key(p: &crate::btp::model::BtpPlayer) -> String {
    match p
        .member_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id) => id.to_ascii_lowercase(),
        None => p.name.trim().to_ascii_lowercase(),
    }
}

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
}

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
    RetryAction::Write(Box::new(update))
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
            court_free_since: HashMap::new(),
            pending_auto: HashMap::new(),
            last_btp_retry_flush: None,
            seen_matches: false,
            suspect_empty_polls: 0,
            highlight_written: HashSet::new(),
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
            match prepare_btp_retry(&entry, snapshot, now) {
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
        // Zuweisung beim Feld-Aufruf (ADR 0007, Scheibe 2): jedem belegten Feld
        // einen Bediener aus der Warteschlange zuordnen (idempotent je Spiel);
        // Zuweisungen frei gewordener/gewechselter Felder räumen.
        if manage_queue {
            for (&court_id, &match_id) in &oncourt_now {
                tablet.assign_scorekeeper_for_court(court_id, match_id);
            }
            tablet.retain_scorekeeper_assignments(&oncourt_now);
        }
        self.oncourt_prev = oncourt_now;
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
        let busy: HashSet<i64> = snapshot.matches.iter().filter_map(|m| m.court_id).collect();
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

        if !config.auto_assign.enabled {
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
        let pending_courts: HashSet<i64> = self.pending_auto.keys().copied().collect();
        let pending_matches: HashSet<i64> = self.pending_auto.values().map(|(m, _)| *m).collect();
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
        // sonst den BTP-Zeitplan von oben nach unten (PlannedTime), dann
        // Spielnummer/ID als Tiebreaker. Ohne Ansetzung → ans Ende der Zeit-
        // gruppe, danach greift die Spielnummer (Verhalten wie bisher).
        ready.sort_by_key(|m| {
            (
                call_for(m.id).is_none(),
                m.planned_time.unwrap_or(i64::MAX),
                m.match_num.unwrap_or(i64::MAX),
                m.id,
            )
        });

        // ── Spieler-Verfügbarkeit ────────────────────────────────────────
        // Spieler, die GERADE spielen (nur OnCourt). Bewusst NICHT „jedes Match
        // mit court_id": ein beendetes Spiel kann seine Feld-Zuweisung in BTP
        // noch tragen, hat den Spieler aber freigegeben – der unterliegt dann
        // der Pausen-Logik, nicht dem harten Belegt-Block.
        let busy_players: HashSet<String> = snapshot
            .matches
            .iter()
            .filter(|m| m.court_id.is_some() && m.status == MatchStatus::OnCourt)
            .flat_map(|m| m.team1.iter().chain(m.team2.iter()))
            .map(player_key)
            .collect();
        // Pausen-Fenster: Override aus der Config (>0) sonst BTP-Setting 1303.
        let pause_ms: u64 = {
            let mins = if config.auto_assign.pause_minutes > 0.0 {
                config.auto_assign.pause_minutes
            } else {
                snapshot.rest_minutes.unwrap_or(0) as f64
            };
            if mins.is_finite() && mins > 0.0 {
                (mins * 60_000.0) as u64
            } else {
                0
            }
        };
        // Letztes Spielende je Spieler (max finished_at) – nur nötig bei Pause.
        let last_finish: HashMap<String, u64> = if pause_ms == 0 {
            HashMap::new()
        } else {
            let mut lf: HashMap<String, u64> = HashMap::new();
            for m in snapshot
                .matches
                .iter()
                .filter(|m| m.status == MatchStatus::Finished)
            {
                if let Some(end) = m.finished_at {
                    for p in m.team1.iter().chain(m.team2.iter()) {
                        let e = lf.entry(player_key(p)).or_insert(0);
                        *e = (*e).max(end);
                    }
                }
            }
            lf
        };

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
                // Verfügbarkeit: kein Spieler darf gerade spielen, in diesem
                // Zyklus schon vergeben sein oder noch in seiner Pause stecken.
                let player_free = m.team1.iter().chain(m.team2.iter()).all(|p| {
                    let k = player_key(p);
                    if busy_players.contains(&k) || used_players.contains(&k) {
                        return false;
                    }
                    if pause_ms > 0 {
                        if let Some(&end) = last_finish.get(&k) {
                            if now.saturating_sub(end) < pause_ms {
                                return false;
                            }
                        }
                    }
                    true
                });
                if !player_free {
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
            });
            // Feld gilt jetzt als belegt – Wartezeit zurücksetzen und die
            // Zuweisung als „unterwegs" merken, damit weder Feld noch Match bis
            // zur BTP-Rückmeldung erneut vergeben werden (keine Doppelvergabe).
            self.court_free_since.remove(&court.id);
            self.pending_auto.insert(court.id, (m.id, now));
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
        tablet.apply_tablet_scores(&mut snapshot);
        // „In Vorbereitung" gerufene Spiele in den Snapshot stempeln, damit
        // der Aufruf-Zeitstempel im nächsten Push an badhub.de mitgeht.
        tablet.apply_preparation_calls(&mut snapshot);
        // Aufrufe zusätzlich in BTP sichtbar machen (P1, Highlight-Flag) —
        // nur der Diff zum letzten Stand, nur wenn sich etwas geändert hat.
        self.reconcile_highlights(config, tablet, &snapshot).await;
        // Heartbeat: Ist regulär nichts zu senden, aber seit dem letzten
        // Push >60 s vergangen, wird ein voller `tset` als Lebenszeichen
        // erzwungen (Diff gegen `None`). badhub frischt damit `updated_at`
        // auf und erkennt das Turnier als aktiv.
        let mut update = match self.plan(&snapshot) {
            Update::None if self.heartbeat_due() => diff(None, &snapshot, self.rid),
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
        match push::push_update(http, &config.badhub.url, &config.badhub.password, &update).await {
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
    fn plan(&self, current: &BtpSnapshot) -> Update {
        diff(self.last_pushed.as_ref(), current, self.rid)
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
            matches: vec![BtpMatch {
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
                sets: vec![(5, 3)],
                winner: None,
                result: MatchResult::Normal,
                status: MatchStatus::OnCourt,
                finished_at: None,
                preparation_call_ts: None,
                preparation_hall: None,
                scoring: crate::btp::model::ScoringFormat::default(),
            }],
        }
    }

    #[test]
    fn first_plan_is_always_full() {
        let engine = SyncEngine::new();
        assert!(matches!(engine.plan(&snapshot()), Update::Full(_)));
    }

    #[test]
    fn unchanged_snapshot_after_success_plans_nothing() {
        let mut engine = SyncEngine::new();
        engine.on_success(snapshot());
        assert!(matches!(engine.plan(&snapshot()), Update::None));
    }

    #[test]
    fn after_failure_next_plan_is_full_again() {
        let mut engine = SyncEngine::new();
        engine.on_success(snapshot());
        // Ohne Fehler wäre ein unveränderter Snapshot ein No-op …
        assert!(matches!(engine.plan(&snapshot()), Update::None));
        // … nach einem Push-Fehler aber wird wieder voll gesendet.
        engine.on_failure();
        assert!(matches!(engine.plan(&snapshot()), Update::Full(_)));
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
            sets: Vec::new(),
            winner: None,
            result: MatchResult::Normal,
            status: MatchStatus::Scheduled,
            finished_at: None,
            preparation_call_ts: None,
            preparation_hall: None,
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
    fn auto_assign_skips_locked_court() {
        let mut engine = SyncEngine::new();
        let tablet = TabletState::default();
        tablet.set_court_locked(1, true);
        let snap = snap_with(vec![court(1, None)], vec![ready_match(7, 1)], Vec::new());
        let (courts, _) = engine.auto_assign(&cfg_auto(true, 0.0), &snap, &tablet);
        assert!(courts.is_empty());
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
    fn auto_assign_court_with_finished_match_stays_busy() {
        // Sicherheitsnetz (Kontext v0.9.113): Trägt ein beendetes Spiel in BTP
        // noch seine CourtID, gilt das Feld als belegt — keine Doppelvergabe.
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
            courts.is_empty(),
            "Feld mit noch nicht abgeräumtem beendeten Spiel bleibt belegt"
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
            prepare_btp_retry(&pending(7, None, 0), &snap, 1_000),
            RetryAction::Drop("BTP hat bereits ein Ergebnis")
        );
    }

    #[test]
    fn retry_is_dropped_after_max_age() {
        let snap = snap_with(Vec::new(), Vec::new(), Vec::new());
        let too_old = BTP_RETRY_MAX_AGE.as_millis() as u64 + 1;
        assert_eq!(
            prepare_btp_retry(&pending(7, None, 0), &snap, too_old),
            RetryAction::Drop("Eintrag zu alt")
        );
    }

    #[test]
    fn retry_strips_player_checkout_after_five_minutes() {
        // Tilos 5-Minuten-Guard: späte Replays dürfen Spieler nicht erneut
        // auschecken/umstempeln — Ergebnis + Sätze bleiben unverändert.
        let snap = snap_with(Vec::new(), Vec::new(), Vec::new());
        let late = PLAYER_CHECKOUT_WINDOW.as_millis() as u64 + 1;
        let RetryAction::Write(u) = prepare_btp_retry(&pending(7, None, 0), &snap, late) else {
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
        let RetryAction::Write(u) = prepare_btp_retry(&pending(7, Some(5), 0), &snap, 1_000) else {
            panic!("Write erwartet");
        };
        assert_eq!(u.free_court_id, Some(5));

        // Feld inzwischen anderweitig belegt (unser Match hat es verloren) →
        // Freigabe entfällt, sonst räumte das Replay die neue Zuweisung weg.
        let mut other = ready_match(9, 2);
        other.court_id = Some(5);
        let snap2 = snap_with(Vec::new(), vec![other], Vec::new());
        let RetryAction::Write(u2) = prepare_btp_retry(&pending(7, Some(5), 0), &snap2, 1_000)
        else {
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
        let RetryAction::Write(u) = prepare_btp_retry(&entry, &snap, 1_000) else {
            panic!("Write erwartet");
        };
        assert_eq!(*u, entry.update, "frischer Eintrag geht 1:1 raus");
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
}
