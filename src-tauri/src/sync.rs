//! Sync-Engine: ein Poll-Push-Zyklus BTP → Badhub.
//!
//! Strategie bei Push-Fehlern (Resend-on-failure): Es gibt keine
//! persistente Outbox. Schlägt ein Push fehl, wird der zuletzt gesendete
//! Stand verworfen – der nächste Zyklus sendet dann einen vollen `tset`
//! mit dem aktuellen Komplettstand. Die Turnierdaten liegen ohnehin in
//! BTP und werden bei jedem Zyklus neu abgefragt.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::badhub::diff::{diff, Update};
use crate::badhub::push;
use crate::btp::client;
use crate::btp::model::{BtpSnapshot, MatchStatus};
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
    /// BTP nicht erreichbar oder Antwort unbrauchbar.
    BtpError(String),
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
}

/// Wie lange eine offene Auto-Zuweisung als „unterwegs" gilt, bevor sie als
/// fehlgeschlagen verworfen und neu versucht wird (BTP-Write nicht sichtbar).
const PENDING_AUTO_TTL: Duration = Duration::from_secs(30);

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
        }
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
    fn track_scorekeepers(&mut self, snapshot: &BtpSnapshot, tablet: &TabletState) {
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
                            tablet.set_scorekeeper(court_id, names);
                        }
                    }
                }
            }
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
        use std::collections::HashSet;
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
        self.track_scorekeepers(&snapshot, tablet);
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
        let (auto_courts, auto_matches) = self.auto_assign(config, &snapshot, tablet);
        if !auto_courts.is_empty() {
            match crate::tablet::server::write_courts_to_btp(config, &auto_courts, &auto_matches)
                .await
            {
                Ok(()) => tracing::info!("Auto-Feldvergabe: {} Feld(er) belegt", auto_courts.len()),
                Err(e) => tracing::warn!("Auto-Feldvergabe fehlgeschlagen: {e}"),
            }
        }
        // Rohen BTP-Stand dem Tablet-Server geben, dann die Sätze
        // tablet-getriebener Courts überschreiben.
        tablet.set_snapshot(snapshot.clone());
        tablet.apply_tablet_scores(&mut snapshot);
        // „In Vorbereitung" gerufene Spiele in den Snapshot stempeln, damit
        // der Aufruf-Zeitstempel im nächsten Push an badhub.de mitgeht.
        tablet.apply_preparation_calls(&mut snapshot);
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
                round_name: "G1".to_string(),
                match_num: Some(1),
                planned_time: None,
                team1: vec![BtpPlayer {
                    name: "A".to_string(),
                    first: String::new(),
                    last: "A".to_string(),
                    member_id: None,
                    nationality: None,
                }],
                team2: vec![BtpPlayer {
                    name: "B".to_string(),
                    first: String::new(),
                    last: "B".to_string(),
                    member_id: None,
                    nationality: None,
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
            name: n.to_string(),
            first: String::new(),
            last: n.to_string(),
            member_id: None,
            nationality: None,
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
}
