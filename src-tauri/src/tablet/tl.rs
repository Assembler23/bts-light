//! Der Anzeige-Zustand der Turnierleitungs-Oberfläche.
//!
//! Ein einziger Abruf füllt die ganze Seite: Felder, Spielliste, Zeiten,
//! Rahmendaten. Gebaut wird er **am Host** — im LAN-Betrieb liefert ihn der
//! eingebettete Server direkt aus, im Cloud-Betrieb schiebt der Host ihn als
//! opakes JSON zum Relay, der ihn unverändert weiterreicht. So bleibt die
//! Turnierlogik vollständig hier (R5), und beide Wege zeigen dasselbe.
//!
//! **Datensparsamkeit ist hier Teil der Funktion**, nicht nur eine Regel im
//! Kopf: Diese Daten laufen über eine aus dem Internet erreichbare Seite.
//! Deshalb enthält der Zustand keine Lizenznummern (die Spieler-Identität
//! bleibt am Host; nach außen geht nur das *Ergebnis* der Prüfung), keine
//! Nationalitäten (die existieren allein für die Sprachwahl der Ansage, und
//! diese Seite spricht nicht) und keine Akkustände. Ein Test wacht darüber.
//!
//! Details: `docs/features/turnierleitung-web.md`.

use crate::config::AppConfig;
use crate::tablet::assign::{self, Blocked, HallSource, PlayerAvailability};
use crate::tablet::state::TabletState;

/// Erkennt das Gerät hinter einem mitgeschickten Zugang.
///
/// Liefert `None`, sobald irgendetwas nicht stimmt — unbekannter oder leerer
/// Zugang, oder die Oberfläche ist gar nicht freigeschaltet. Der Schalter
/// wird **hier** mitgeprüft und nicht nur beim Registrieren der Routen:
/// Sonst bliebe ein früher gekoppeltes Gerät nach dem Abschalten weiter
/// berechtigt.
///
/// Der Aufrufer liest die Konfiguration frisch von der Platte, damit ein
/// Widerruf ohne Neustart greift.
pub fn authorize(config: &AppConfig, token: &str) -> Option<crate::config::TlDevice> {
    if !config.tl_web.enabled {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    config
        .tl_web
        .devices
        .iter()
        .find(|d| d.token == token)
        .cloned()
}

/// Was eine Aktion in BTP schreiben würde — getrennt vom Schreiben selbst,
/// damit die Entscheidung ohne Netz prüfbar bleibt (Muster
/// `build_manual_result_update`).
#[derive(Debug)]
pub(crate) struct CourtWrite {
    pub courts: Vec<crate::btp::proto::CourtAssignment>,
    pub match_courts: Vec<crate::btp::proto::MatchCourt>,
}

/// Prüft eine feldbezogene Aktion und übersetzt sie in den BTP-Schreibvorgang.
///
/// Rein: kein Netz, kein Zustand. Lehnt die Prüfung ab, entsteht **kein**
/// Schreibvorgang — und die Antwort trägt den maschinenlesbaren Grund samt
/// einer Meldung, die ein Mensch versteht.
pub(crate) fn plan_court_action(
    snap: &crate::btp::model::BtpSnapshot,
    config: &AppConfig,
    locked: &[i64],
    reserved: &[(i64, i64)],
    action: &relay_proto::TlAction,
) -> Result<CourtWrite, relay_proto::TlResponse> {
    use crate::btp::model::MatchStatus;
    use crate::btp::proto::{CourtAssignment, MatchCourt};
    use relay_proto::TlAction;

    // Die Feldzuordnung führt BTP doppelt: am Feld und am Spiel. Beide
    // Seiten gehören in denselben Schreibvorgang.
    let match_side = |match_id: i64, court_id: i64| -> Vec<MatchCourt> {
        snap.matches
            .iter()
            .find(|m| m.id == match_id)
            .map(|m| {
                vec![MatchCourt {
                    match_id: m.id,
                    draw_id: m.draw_id,
                    planning_id: m.planning_id,
                    court_id,
                }]
            })
            .unwrap_or_default()
    };

    match action {
        TlAction::AssignCourt {
            court_id,
            match_id,
            expect,
        } => {
            assign::check_assign(
                snap, config, locked, reserved, *match_id, *court_id, *expect,
            )
            .map_err(|e| assign_error_response(snap, e))?;
            Ok(CourtWrite {
                courts: vec![CourtAssignment {
                    court_id: *court_id,
                    match_id: Some(*match_id),
                }],
                match_courts: match_side(*match_id, *court_id),
            })
        }
        TlAction::FreeCourt { court_id, expect } => {
            assign::check_free(snap, *court_id, *expect)
                .map_err(|e| assign_error_response(snap, e))?;
            // Die Feldzuordnung am Spiel wird **nur** bei einem laufenden
            // Spiel gelöscht. Bei einem beendeten hält BTP fest, wo gespielt
            // wurde — das ist Turnier-Dokumentation, die der Ergebnispfad
            // ausdrücklich bewahrt und die Desktop-Schaltfläche ebenso.
            // Sonst hinge das Protokoll davon ab, über welche Oberfläche
            // jemand das Feld freigegeben hat.
            let running = snap
                .matches
                .iter()
                .find(|m| m.court_id == Some(*court_id) && m.status == MatchStatus::OnCourt)
                .map(|m| m.id);
            Ok(CourtWrite {
                courts: vec![CourtAssignment {
                    court_id: *court_id,
                    match_id: None,
                }],
                // 0 = Feldzuordnung am Spiel löschen.
                match_courts: running.map(|m| match_side(m, 0)).unwrap_or_default(),
            })
        }
        TlAction::MoveMatch {
            from_court_id,
            to_court_id,
            match_id,
            expect_from,
            expect_to,
        } => {
            // Beide Seiten prüfen, bevor irgendetwas geschrieben wird.
            assign::check_free(snap, *from_court_id, *expect_from)
                .map_err(|e| assign_error_response(snap, e))?;
            // Für das Zielfeld gilt dieselbe Prüfung wie beim Zuweisen —
            // nur dass das Spiel hier erlaubterweise schon auf einem Feld
            // steht (nämlich dem Quellfeld).
            assign::check_move_target(
                snap,
                config,
                locked,
                reserved,
                *match_id,
                *from_court_id,
                *to_court_id,
                *expect_to,
            )
            .map_err(|e| assign_error_response(snap, e))?;
            Ok(CourtWrite {
                courts: vec![
                    CourtAssignment {
                        court_id: *from_court_id,
                        match_id: None,
                    },
                    CourtAssignment {
                        court_id: *to_court_id,
                        match_id: Some(*match_id),
                    },
                ],
                match_courts: match_side(*match_id, *to_court_id),
            })
        }
        // Alles Übrige berührt keine Feldzuordnung und kommt in einem
        // eigenen Schritt. Bis dahin ehrlich ablehnen, statt still ins
        // Leere zu laufen.
        _ => Err(relay_proto::TlResponse::err(
            relay_proto::TlErrorCode::Unsupported,
            "Diese Aktion ist noch nicht freigeschaltet.",
        )),
    }
}

/// Führt eine Aktion aus, die **nur den Turnier-PC** betrifft und nichts nach
/// BTP schreibt: Vorbereitungs-Aufrufe, Zähltafelbediener-Warteschlange,
/// Walkover-Vorschläge.
///
/// Diese Zustände kennt BTP gar nicht — bts-light führt sie selbst, genau wie
/// bei den Aufrufen aus der Desktop-Oberfläche. Entsprechend gibt es hier
/// weder Schreibfehler noch Reservierungen; die Aktion gilt sofort.
///
/// `Ok(None)` heißt: nicht meine Zuständigkeit (die Aktion berührt Felder und
/// gehört in die Feld-Planung).
pub(crate) fn apply_state_action(
    tablet: &TabletState,
    _config: &AppConfig,
    now_ms: u64,
    action: &relay_proto::TlAction,
) -> Result<relay_proto::TlResponse, relay_proto::TlResponse> {
    use relay_proto::{TlAction as A, TlErrorCode as C, TlResponse};

    let known_match = |id: i64| -> bool {
        tablet
            .snapshot_clone()
            .is_some_and(|s| s.matches.iter().any(|m| m.id == id))
    };

    match action {
        A::CallPreparation {
            match_ids,
            location_id,
        } => {
            if match_ids.is_empty() {
                return Err(TlResponse::err(C::NotAllowed, "Kein Spiel ausgewählt."));
            }
            // Erst prüfen, dann eintragen: Ein Aufruf für ein Spiel, das es
            // im aktuellen Stand nicht gibt, erschiene nirgends und ließe
            // sich auch nicht zurücknehmen.
            if let Some(unknown) = match_ids.iter().find(|id| !known_match(**id)) {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    format!("Spiel {unknown} gibt es im aktuellen Turnierstand nicht."),
                ));
            }
            for match_id in match_ids {
                tablet.add_preparation_call(crate::tablet::state::PreparationCall {
                    match_id: *match_id,
                    location_id: *location_id,
                    called_at_ms: now_ms,
                });
            }
            Ok(TlResponse::ok(0))
        }
        A::RetractPreparation { match_id } => {
            tablet.remove_preparation_call(*match_id);
            Ok(TlResponse::ok(0))
        }
        A::ScorekeeperAdd { names } => {
            let names: Vec<String> = names
                .iter()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect();
            if names.is_empty() {
                return Err(TlResponse::err(C::NotAllowed, "Kein Name eingegeben."));
            }
            tablet.add_scorekeeper_manual(names, now_ms);
            Ok(TlResponse::ok(0))
        }
        A::ScorekeeperRemove { key } => {
            tablet.remove_scorekeeper(key);
            Ok(TlResponse::ok(0))
        }
        A::ScorekeeperAdvance { key } => {
            tablet.advance_scorekeeper(key);
            Ok(TlResponse::ok(0))
        }
        A::DismissWalkover { proposal_id } => {
            tablet.remove_walkover_proposal(proposal_id);
            Ok(TlResponse::ok(0))
        }
        A::SetAutoAssign { enabled } => {
            // Laufzeit-Schalter, nicht die Grundeinstellung: Der Sync-Lauf
            // liest die Konfiguration nach dem Start nicht neu, eine
            // Dateiänderung bliebe also wirkungslos. Nach einem Neustart
            // gilt wieder, was in den Einstellungen steht.
            tablet.set_auto_assign_paused(!*enabled);
            Ok(TlResponse::ok(0))
        }
        // Alles Weitere berührt Felder oder BTP und läuft woanders.
        _ => Err(TlResponse::err(
            C::Unsupported,
            "Diese Aktion ist noch nicht freigeschaltet.",
        )),
    }
}

/// Übersetzt eine Ergebnis-Aktion in die BTP-Schreibvorgänge.
///
/// Rein und ohne Netz. Die eigentliche Prüfung (Satz-Vollständigkeit,
/// Sieger-Ermittlung, bereits gewertet) liegt in
/// `server::build_manual_result_update` und wird hier nur benutzt — es ist
/// dieselbe, die auch am Zähltablett und in der Desktop-Oberfläche greift
/// (R5).
pub(crate) fn plan_result_action(
    snap: &crate::btp::model::BtpSnapshot,
    on_court_since: Option<u64>,
    now_ms: u64,
    action: &relay_proto::TlAction,
) -> Result<Vec<crate::btp::proto::MatchUpdate>, relay_proto::TlResponse> {
    use relay_proto::{TlAction as A, TlErrorCode as C, TlResponse};

    match action {
        A::EnterResult {
            match_id,
            sets,
            retired,
            overwrite,
            ..
        } => {
            if *retired {
                // Aufgabe und kampflose Wertung ziehen Folgen nach sich —
                // wer kommt weiter, welche Spiele der Disziplin fallen mit.
                // Diese Logik hängt am Walkover-Weg; eine halbe Umsetzung
                // hier wäre schlimmer als eine klare Ansage.
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Eine Aufgabe wird über den Walkover-Vorschlag gewertet, nicht über \
                     die Ergebnis-Eingabe.",
                ));
            }
            if *overwrite {
                // Steht bewusst aus, bis am echten BTP geprüft ist, was ein
                // Überschreiben mit dem Turnierbaum macht.
                return Err(TlResponse::err(
                    C::Unsupported,
                    "Ein bereits gewertetes Spiel lässt sich hier noch nicht überschreiben.",
                ));
            }
            let m = snap
                .matches
                .iter()
                .find(|m| m.id == *match_id)
                .ok_or_else(|| {
                    TlResponse::err(
                        C::NotAllowed,
                        "Das Spiel gibt es im aktuellen Turnierstand nicht mehr.",
                    )
                })?;
            let pairs: Vec<(i64, i64)> = sets.iter().map(|s| (s.a, s.b)).collect();
            let update =
                crate::tablet::server::build_manual_result_update(m, pairs, on_court_since, now_ms)
                    .map_err(|e| TlResponse::err(C::NotAllowed, e))?;
            Ok(vec![update])
        }
        _ => Err(TlResponse::err(
            C::Unsupported,
            "Diese Aktion ist noch nicht freigeschaltet.",
        )),
    }
}

/// Baut die kampflosen Wertungen für die ausgewählten Spiele eines
/// Walkover-Vorschlags.
///
/// Rein und ohne Netz. Die aufgebende Mannschaft verliert, der Gegner
/// gewinnt — ohne Sätze, mit dem BTP-Vermerk für „kampflos". Kampflose
/// Spiele stehen auf keinem Feld, also gibt es nichts freizugeben und keine
/// Spieler auszuchecken.
pub(crate) fn walkover_updates(
    candidates: &[crate::tablet::state::WalkoverCandidate],
    match_ids: &[i64],
) -> Vec<crate::btp::proto::MatchUpdate> {
    candidates
        .iter()
        .filter(|c| match_ids.contains(&c.match_id))
        .map(|c| crate::btp::proto::MatchUpdate {
            btp_match_id: c.match_id,
            draw_id: c.draw_id,
            planning_id: c.planning_id,
            sets: Vec::new(),
            // Sieger ist die jeweils NICHT aufgebende Mannschaft.
            team1_won: !c.retired_is_team1,
            duration_mins: 0,
            score_status: 1, // 1 = kampflos
            free_court_id: None,
            player_ids: Vec::new(),
            end_ts_ms: None,
        })
        .collect()
}

/// Prüft und baut die kampflosen Wertungen eines Walkover-Vorschlags.
///
/// Bleibt nichts zu schreiben übrig — etwa weil der letzte angekreuzte
/// Kandidat zwischen Anzeige und Antippen aufs Feld gewandert ist —, ist das
/// ein Fehler und **kein** stiller Erfolg: Sonst verschwände der Vorschlag,
/// ohne dass je etwas gewertet wurde.
pub(crate) fn plan_walkover_action(
    candidates: &[crate::tablet::state::WalkoverCandidate],
    match_ids: &[i64],
) -> Result<Vec<crate::btp::proto::MatchUpdate>, relay_proto::TlResponse> {
    let updates = walkover_updates(candidates, match_ids);
    if updates.is_empty() {
        return Err(relay_proto::TlResponse::err(
            relay_proto::TlErrorCode::AlreadyHandled,
            "Keines der gewählten Spiele lässt sich noch kampflos werten — bitte die \
             Aufgabe erneut aufrufen.",
        ));
    }
    Ok(updates)
}

/// Berührt diese Aktion eine Feldzuordnung (und damit BTP)?
fn touches_courts(action: &relay_proto::TlAction) -> bool {
    use relay_proto::TlAction as A;
    matches!(
        action,
        A::AssignCourt { .. } | A::FreeCourt { .. } | A::MoveMatch { .. }
    )
}

/// Führt eine Aktion eines Turnierleitungs-Geräts aus.
///
/// Der **einzige** Weg, auf dem ein solches Gerät etwas verändert — im
/// LAN-Betrieb vom eingebetteten Server aufgerufen, im Cloud-Betrieb vom
/// Relay-Client mit demselben Aufruf. Damit wird jede Mutation genau einmal
/// geprüft, so wie es bei den Ergebnissen der Tablets schon gehandhabt wird
/// (R5).
pub(crate) async fn execute(
    ctx: &crate::tablet::server::ServerCtx,
    device: &crate::config::TlDevice,
    op_id: &str,
    now_ms: u64,
    action: relay_proto::TlAction,
) -> relay_proto::TlResponse {
    use relay_proto::{TlErrorCode as C, TlResponse};

    // Wiederholung? Dann die gespeicherte Antwort, ohne erneut zu schreiben.
    // Ein Doppeltipp bei träger Verbindung schickt dieselbe Aktion zweimal;
    // ohne diese Prüfung landete sie zweimal in BTP. Der Fingerabdruck stellt
    // sicher, dass es wirklich dieselbe Aktion ist.
    let fingerprint = action_fingerprint(&action);
    if let Some(known) = ctx.tablet.remembered_result(op_id, &fingerprint, now_ms) {
        tracing::info!(
            "TL-Web [{}]: {} war schon erledigt (Wiederholung)",
            device.label,
            action_label(&action)
        );
        return known;
    }

    // Frisch von der Platte: So greifen Widerruf und Abschalten sofort.
    let config = ctx.app_config();
    if config.slave_mode {
        return TlResponse::err(
            C::NotAllowed,
            "Dieser Rechner ist ein Ansage-Slave — Feldvergabe läuft nur am Turnier-PC.",
        );
    }
    let Some(snap) = ctx.tablet.snapshot_clone() else {
        return TlResponse::err(C::NotAllowed, "Es ist noch kein Turnier geladen.");
    };
    // Wertungen: eigener Weg, weil sie Ergebnisse schreiben statt
    // Feldzuordnungen — mit der Nachschub-Queue, die auch der Tablet- und
    // der Desktop-Pfad benutzen. Fällt BTP kurz aus, reicht der Sync-Lauf
    // die Wertung nach, statt sie zu verlieren.
    if let Some(response) = execute_result_action(ctx, device, &config, now_ms, &action).await {
        if response.ok {
            ctx.tablet
                .remember_result(op_id, &fingerprint, response.clone(), now_ms);
        }
        return response;
    }

    // Aktionen ohne Feldbezug ändern nur den Zustand am Turnier-PC: kein
    // Schreibvorgang nach BTP, also auch keine Reservierung und kein
    // Fehlschlag von dort.
    if !touches_courts(&action) {
        let response = match apply_state_action(&ctx.tablet, &config, now_ms, &action) {
            Ok(ok) => {
                tracing::info!(
                    "TL-Web [{}]: {} ausgeführt",
                    device.label,
                    action_label(&action)
                );
                ok
            }
            Err(rejected) => {
                tracing::info!(
                    "TL-Web [{}]: {} abgelehnt ({})",
                    device.label,
                    action_label(&action),
                    rejected.error.as_deref().unwrap_or("ohne Grund")
                );
                return rejected;
            }
        };
        ctx.tablet
            .remember_result(op_id, &fingerprint, response.clone(), now_ms);
        return response;
    }

    let locked = ctx.tablet.locked_courts();
    // Zuweisungen, die schon geschrieben, aber von BTP noch nicht bestätigt
    // sind — in diesem Fenster sieht der Schnappschuss das Feld noch frei.
    let reserved = ctx.tablet.reserved_courts(now_ms);

    let plan = match plan_court_action(&snap, &config, &locked, &reserved, &action) {
        Ok(plan) => plan,
        Err(response) => {
            // Auch Ablehnungen werden festgehalten — nur so lässt sich nach
            // dem Turnier zählen, wie oft sich zwei Geräte in die Quere
            // kamen. Der Zugang des Geräts taucht dabei nie auf.
            tracing::info!(
                "TL-Web [{}]: {} abgelehnt ({})",
                device.label,
                action_label(&action),
                response.error.as_deref().unwrap_or("ohne Grund")
            );
            return response;
        }
    };

    // Felder beanspruchen, **bevor** geschrieben wird. Der Schreibvorgang
    // nach BTP dauert (Anmeldung + Aktualisierung); genau in dieser Zeit
    // würden zwei gleichzeitig tippende Geräte beide durchlaufen, weil der
    // Schnappschuss das Feld noch frei zeigt. Wer den Anspruch nicht bekommt,
    // schreibt gar nicht erst.
    let mut claimed: Vec<i64> = Vec::new();
    for c in &plan.courts {
        if let Some(match_id) = c.match_id {
            if !ctx.tablet.try_reserve_court(c.court_id, match_id, now_ms) {
                for done in &claimed {
                    ctx.tablet.release_court_claim(*done);
                }
                tracing::info!(
                    "TL-Web [{}]: {} abgelehnt (Feld gerade vergeben)",
                    device.label,
                    action_label(&action)
                );
                return TlResponse::err(
                    C::CourtTaken,
                    "Feld wurde im selben Moment von jemand anderem belegt.",
                );
            }
            claimed.push(c.court_id);
        } else {
            // Ein Feld, das geräumt wird, braucht keinen Anspruch mehr —
            // sonst bliebe es nach „belegen, dann doch freigeben" bis zum
            // Ablauf der Frist gesperrt.
            ctx.tablet.release_court_claim(c.court_id);
        }
    }

    match crate::tablet::server::write_courts_to_btp(&config, &plan.courts, &plan.match_courts)
        .await
    {
        Ok(()) => {
            // Beim Umhängen wandert der laufende Spielstand mit — er hängt
            // am Feld, nicht am Spiel. Ohne das zeigte das neue Feld 0:0 und
            // das alte den stehengebliebenen Stand.
            if let relay_proto::TlAction::MoveMatch {
                from_court_id,
                to_court_id,
                match_id,
                ..
            } = &action
            {
                ctx.tablet
                    .move_match_score(*from_court_id, *to_court_id, *match_id);
            }
            tracing::info!(
                "TL-Web [{}]: {} ausgeführt",
                device.label,
                action_label(&action)
            );
            let response = TlResponse::ok(0);
            ctx.tablet
                .remember_result(op_id, &fingerprint, response.clone(), now_ms);
            response
        }
        Err(e) => {
            // Der Schreibvorgang ist gescheitert — die Ansprüche sofort
            // zurückgeben, damit der nächste Versuch nicht an der eigenen
            // Vormerkung scheitert.
            for court_id in &claimed {
                ctx.tablet.release_court_claim(*court_id);
            }
            tracing::warn!(
                "TL-Web [{}]: {} — BTP-Schreibfehler: {e}",
                device.label,
                action_label(&action)
            );
            // Fehlgeschlagene Schreibvorgänge werden NICHT gemerkt: Ein
            // erneuter Versuch soll es wirklich noch einmal versuchen.
            TlResponse::err(C::BtpError, format!("BTP hat abgelehnt: {e}"))
        }
    }
}

/// Führt eine Wertung aus (Ergebnis eintragen, kampflos werten).
///
/// `None` heißt: keine Wertung, an anderer Stelle weiterbehandeln.
///
/// Wie im Tablet- und Desktop-Pfad landet ein fehlgeschlagener Schreibvorgang
/// in der Nachschub-Queue — der Sync-Lauf reicht ihn nach, sobald BTP wieder
/// antwortet. Eine Wertung darf nicht verlorengehen, nur weil das Netz
/// kurz weg war.
async fn execute_result_action(
    ctx: &crate::tablet::server::ServerCtx,
    device: &crate::config::TlDevice,
    config: &AppConfig,
    now_ms: u64,
    action: &relay_proto::TlAction,
) -> Option<relay_proto::TlResponse> {
    use relay_proto::{TlAction as A, TlErrorCode as C, TlResponse};

    // Der beanspruchte Walkover-Vorschlag — nur gefüllt, wenn er dieser
    // Anfrage gehört. Ging danach gar nichts nach BTP, kommt er zurück.
    let mut claimed_walkover: Option<crate::tablet::state::WalkoverProposal> = None;
    let updates = match action {
        A::EnterResult { match_id, .. } => {
            let snap = ctx.tablet.snapshot_clone()?;
            let on_court_since = snap
                .matches
                .iter()
                .find(|m| m.id == *match_id)
                .and_then(|m| m.court_id)
                .and_then(|cid| ctx.tablet.on_court_since_ms(cid, *match_id));
            match plan_result_action(&snap, on_court_since, now_ms, action) {
                Ok(u) => u,
                Err(rejected) => return Some(rejected),
            }
        }
        A::ConfirmWalkover {
            proposal_id,
            match_ids,
        } => {
            if match_ids.is_empty() {
                return Some(TlResponse::err(C::NotAllowed, "Kein Spiel ausgewählt."));
            }
            // Beanspruchend herausnehmen: Tippen zwei Geräte im selben
            // Moment, schriebe sonst jedes dieselben Wertungen nach BTP.
            let Some(proposal) = ctx.tablet.take_walkover_proposal(proposal_id) else {
                return Some(TlResponse::err(
                    C::AlreadyHandled,
                    "Der Vorschlag ist nicht mehr offen — vermutlich hat ihn jemand anderes \
                     schon bearbeitet.",
                ));
            };
            let planned = match plan_walkover_action(
                &ctx.tablet.walkover_candidates(proposal.entry_id),
                match_ids,
            ) {
                Ok(u) => u,
                Err(rejected) => {
                    // Nichts geschrieben → zurücklegen, sonst wäre die
                    // kampflose Wertung lautlos verschwunden.
                    ctx.tablet.add_walkover_proposal(proposal);
                    return Some(rejected);
                }
            };
            claimed_walkover = Some(proposal);
            planned
        }
        _ => return None,
    };

    let mut written = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for update in updates {
        // Nachschub-Eintrag und Schreibzeit erledigt der gemeinsame Weg —
        // der Zeitstempel muss von NACH dem Schreiben stammen.
        match crate::tablet::server::write_result_settled(config, &ctx.tablet, &update).await {
            Ok(()) => {
                if let Some(cid) = update.free_court_id {
                    ctx.tablet.clear_court(cid);
                }
                written += 1;
            }
            Err(e) => {
                ctx.tablet.queue_btp_retry(update, now_ms);
                errors.push(e);
            }
        }
    }

    // Kam gar nichts durch, kommt der beanspruchte Vorschlag zurück: Er ist
    // dann der einzige Hinweis darauf, dass hier noch etwas offen ist. Ging
    // ein Teil durch, bleibt er verschwunden — der Rest liegt in der
    // Nachschub-Queue und wird von dort geschrieben, ein zweiter Anlauf über
    // den Vorschlag schriebe dieselben Wertungen erneut.
    if let Some(proposal) = claimed_walkover {
        if written == 0 {
            ctx.tablet.add_walkover_proposal(proposal);
        }
    }

    tracing::info!(
        "TL-Web [{}]: {} — {written} Wertung(en) geschrieben, {} offen",
        device.label,
        action_label(action),
        errors.len()
    );
    // Der Grund von BTP gehört ins Protokoll: Die Seite bekommt nur den
    // beruhigenden Satz, aber bei der Fehlersuche nach dem Turnier ist
    // „Anmeldung abgelehnt" etwas völlig anderes als „nicht erreichbar".
    if !errors.is_empty() {
        tracing::warn!(
            "TL-Web: {} nicht geschrieben ({}) — Nachschub übernimmt",
            action_label(action),
            errors.join(" | ")
        );
    }
    Some(if errors.is_empty() {
        TlResponse::ok(0)
    } else if written > 0 {
        // Teilweise durchgekommen: Das ist kein Fehlschlag, aber die
        // Turnierleitung muss wissen, dass noch etwas unterwegs ist.
        TlResponse::ok(0).with_warning(format!(
            "{written} gewertet, {} wird automatisch nachgereicht.",
            errors.len()
        ))
    } else {
        TlResponse::err(
            C::BtpError,
            "BTP war nicht erreichbar — die Wertung wird automatisch nachgereicht.",
        )
    })
}

/// Kurzer, stabiler Fingerabdruck einer Aktion — erkennt, ob eine wiederholte
/// Vorgangskennung wirklich dieselbe Absicht trägt.
///
/// **Jede Nutzlast gehört hinein.** Die Vorgangskennung der Seite fasst ein
/// Zeitfenster zusammen; wer sich vertippt und sofort korrigiert, schickt
/// dieselbe Kennung mit anderem Inhalt. Fehlt dieser Inhalt im Fingerabdruck,
/// gilt die Korrektur als Doppeltipp und wird nie geschrieben — bei
/// gemeldetem Erfolg.
///
/// Darum **ohne** Sammelzweig: Eine neue Aktion soll den Übersetzer brechen,
/// nicht lautlos in der Idempotenz landen. Die Erwartungswerte (`expect…`)
/// bleiben draußen — sie sind die Absicherung gegen Gleichzeitigkeit, nicht
/// die Absicht; nach einer Zustands-Aktualisierung trägt derselbe Tipp
/// andere Erwartungen.
///
/// Der Fingerabdruck wird **nie** protokolliert (dafür ist `action_label` da)
/// — er darf deshalb auch Namen aus einer Zähltafelbediener-Meldung tragen.
fn action_fingerprint(action: &relay_proto::TlAction) -> String {
    use relay_proto::TlAction as A;
    let ids = |v: &[i64]| {
        v.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    match action {
        A::AssignCourt {
            court_id,
            match_id,
            expect: _,
        } => format!("assign:{match_id}:{court_id}"),
        A::FreeCourt {
            court_id,
            expect: _,
        } => format!("free:{court_id}"),
        A::MoveMatch {
            from_court_id,
            to_court_id,
            match_id,
            expect_from: _,
            expect_to: _,
        } => format!("move:{match_id}:{from_court_id}:{to_court_id}"),
        A::CallPreparation {
            match_ids,
            location_id,
        } => format!("prep:{}:{}", ids(match_ids), location_id.unwrap_or(0)),
        A::RetractPreparation { match_id } => format!("prep-retract:{match_id}"),
        A::AnnounceCourtCall { court_id, match_id } => {
            format!("call:{match_id}:{court_id}")
        }
        A::AnnouncePrepCall { match_id, side } => format!("prep-call:{match_id}:{side:?}"),
        A::EnterResult {
            match_id,
            sets,
            retired,
            winner,
            overwrite,
        } => format!(
            "result:{match_id}:{}:{retired}:{}:{overwrite}",
            sets.iter()
                .map(|s| format!("{}-{}", s.a, s.b))
                .collect::<Vec<_>>()
                .join(","),
            winner.unwrap_or(0)
        ),
        A::ConfirmWalkover {
            proposal_id,
            match_ids,
        } => format!("wo:{proposal_id}:{}", ids(match_ids)),
        A::DismissWalkover { proposal_id } => format!("wo-dismiss:{proposal_id}"),
        A::ScorekeeperAdvance { key } => format!("sk-advance:{key}"),
        A::ScorekeeperRemove { key } => format!("sk-remove:{key}"),
        A::ScorekeeperAdd { names } => format!("sk-add:{}", names.join(",")),
        A::SetAutoAssign { enabled } => format!("auto:{enabled}"),
    }
}

/// Kurzbeschreibung einer Aktion fürs Protokoll — ohne personenbezogene
/// Angaben und **nie** mit dem Zugang des Geräts.
fn action_label(action: &relay_proto::TlAction) -> String {
    use relay_proto::TlAction as A;
    match action {
        A::AssignCourt {
            court_id, match_id, ..
        } => format!("Spiel {match_id} auf Feld {court_id}"),
        A::FreeCourt { court_id, .. } => format!("Feld {court_id} freigeben"),
        A::MoveMatch {
            from_court_id,
            to_court_id,
            match_id,
            ..
        } => format!("Spiel {match_id} von Feld {from_court_id} auf Feld {to_court_id}"),
        // Bewusst **ohne** Inhalt: Ein `{:?}` der ganzen Aktion schriebe
        // Spielernamen ins Protokoll — und die Protokolle werden zur
        // Fehlersuche hochgeladen. Die Art der Aktion genügt.
        A::CallPreparation { .. } => "Vorbereitungs-Aufruf".to_string(),
        A::RetractPreparation { .. } => "Vorbereitungs-Aufruf zurücknehmen".to_string(),
        A::AnnounceCourtCall { court_id, .. } => format!("Erneuter Aufruf Feld {court_id}"),
        A::AnnouncePrepCall { .. } => "Erneuter Vorbereitungs-Aufruf".to_string(),
        A::EnterResult { match_id, .. } => format!("Ergebnis für Spiel {match_id}"),
        A::ConfirmWalkover { .. } => "Kampflose Wertung".to_string(),
        A::DismissWalkover { .. } => "Walkover-Vorschlag verwerfen".to_string(),
        A::ScorekeeperAdvance { .. } => "Zähltafelbediener vorziehen".to_string(),
        A::ScorekeeperRemove { .. } => "Zähltafelbediener entfernen".to_string(),
        A::ScorekeeperAdd { .. } => "Zähltafelbediener ergänzen".to_string(),
        A::SetAutoAssign { enabled } => {
            format!(
                "Automatische Vergabe {}",
                if *enabled { "an" } else { "aus" }
            )
        }
    }
}

/// Übersetzt eine Ablehnung der Vergabe-Prüfung in eine Antwort, die ein
/// Mensch versteht und die Seite auswerten kann.
fn assign_error_response(
    snap: &crate::btp::model::BtpSnapshot,
    err: assign::AssignError,
) -> relay_proto::TlResponse {
    use assign::AssignError as E;
    use relay_proto::{TlErrorCode as C, TlResponse};

    let name_of = |match_id: i64| -> String {
        snap.matches
            .iter()
            .find(|m| m.id == match_id)
            .map(|m| {
                let a = m.team1.first().map(|p| p.name.as_str()).unwrap_or("?");
                let b = m.team2.first().map(|p| p.name.as_str()).unwrap_or("?");
                format!("{a} / {b}")
            })
            .unwrap_or_else(|| format!("Spiel {match_id}"))
    };

    match err {
        E::CourtTaken { by_match, finished } if finished => TlResponse::err(
            C::CourtTaken,
            format!(
                "Das Feld wird noch geräumt — {} ist beendet, aber BTP hat das Feld \
                 noch nicht freigegeben.",
                name_of(by_match)
            ),
        ),
        E::CourtTaken { by_match, .. } => TlResponse::err(
            C::CourtTaken,
            format!(
                "Feld wurde gerade von jemand anderem belegt: {}.",
                name_of(by_match)
            ),
        ),
        E::CourtFree => TlResponse::err(
            C::CourtFree,
            "Auf dem Feld steht nicht mehr das Spiel, das du gesehen hast.",
        ),
        E::CourtLocked => TlResponse::err(C::CourtLocked, "Das Feld ist gesperrt."),
        E::MatchElsewhere { court_id } => TlResponse::err(
            C::MatchElsewhere,
            format!("Das Spiel steht bereits auf Feld {court_id}."),
        ),
        E::MatchNotPlayable => TlResponse::err(
            C::NotAllowed,
            "Das Spiel ist nicht spielbereit (schon beendet, schon auf dem Feld, \
             oder die Paarung steht noch nicht fest).",
        ),
        E::PlayerOnCourt { players } => TlResponse::err(
            C::NotAllowed,
            format!(
                "{} steht gerade auf einem anderen Feld.",
                players.join(" und ")
            ),
        ),
        E::HallNotAllowed => TlResponse::err(
            C::HallNotAllowed,
            "Diese Disziplin darf in dieser Halle nicht gespielt werden.",
        ),
        E::UnknownMatch => TlResponse::err(
            C::NotAllowed,
            "Das Spiel gibt es im aktuellen Turnierstand nicht mehr.",
        ),
        E::UnknownCourt => TlResponse::err(
            C::NotAllowed,
            "Das Feld gibt es im aktuellen Turnierstand nicht mehr.",
        ),
    }
}

/// Der komplette Anzeige-Zustand.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlState {
    /// Steigt nur bei echter Änderung — daran erkennt ein abrufendes Gerät,
    /// ob sich etwas getan hat, ohne den ganzen Stand zu vergleichen.
    pub rev: u64,
    /// Server-Zeit beim Bauen. Die Geräte rechnen daraus ihren Zeit-Versatz
    /// aus und zeigen dieselbe verstrichene Zeit, egal wie falsch ihre eigene
    /// Uhr geht.
    pub server_now_ms: u64,
    pub tournament: String,
    /// Mehr-Hallen-Turnier? Nur dann bietet die Seite einen Hallenfilter an.
    pub multi_hall: bool,
    /// Alle Hallen des Turniers, alphabetisch nach Namen.
    ///
    /// Mit Kennung, nicht nur mit Namen: Ein Vorbereitungs-Aufruf braucht sie,
    /// damit er auf der Meeting-Point-Anzeige **einer** Halle erscheint und
    /// nicht auf allen.
    pub halls: Vec<TlHall>,
    pub auto_assign: TlAutoAssign,
    /// Schwellen des Aufruf-Timers, damit die Seite die Aufruf-Stufe
    /// genauso einfärbt wie die Desktop-Oberfläche.
    pub call_timer: TlCallTimer,
    /// Pflichtpause zwischen zwei Spielen eines Spielers (Minuten), aus der
    /// Konfiguration oder aus BTP. `None` = keine.
    pub rest_minutes: Option<i64>,
    pub courts: Vec<TlCourt>,
    /// Die Spielliste, **bereits sortiert** — dieselbe Reihenfolge, nach der
    /// die automatische Vergabe arbeitet. Bewusst serverseitig sortiert:
    /// Sonst zeigten zwei Geräte zwei Reihenfolgen.
    pub queue: Vec<TlMatch>,
    /// Offene Walkover-Vorschläge: Nach einer Aufgabe schlägt bts-light vor,
    /// die Folgespiele derselben Mannschaft kampflos zu werten. Welche das
    /// sein sollen, entscheidet die Turnierleitung.
    pub walkovers: Vec<TlWalkover>,
    /// Hallen, deren Warteliste gekappt wurde (leerer Name = Spiele ohne
    /// Hallenzuordnung). Leer = nichts gekappt.
    ///
    /// Gekappt wird **je Halle**, nicht über das ganze Turnier: Global
    /// gekappt könnte die Sortierung eine komplette Halle verdrängen, und
    /// das Gerät dort sähe eine leere Liste, obwohl hundert Spiele warten.
    pub truncated_halls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlAutoAssign {
    /// Läuft die automatische Vergabe gerade? Berücksichtigt sowohl die
    /// Grundeinstellung als auch das Anhalten aus dieser Oberfläche.
    pub enabled: bool,
    /// Ist sie in den Einstellungen grundsätzlich eingeschaltet? Nur dann
    /// lässt sie sich hier überhaupt wieder starten.
    pub configured: bool,
    pub wait_minutes: f64,
    /// Tages-Halle; leer = alle.
    pub active_hall: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlCallTimer {
    pub enabled: bool,
    pub second_call_minutes: f64,
    pub third_call_minutes: f64,
}

/// Ein Feld mit dem, was gerade darauf läuft.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlCourt {
    pub court_id: i64,
    pub court: String,
    /// Hallenname; leer bei Ein-Hallen-Turnieren.
    pub location: String,
    /// 0 = kein Spiel auf dem Feld.
    pub match_id: i64,
    pub match_name: String,
    pub round_name: String,
    pub class_label: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    pub sets: Vec<(i64, i64)>,
    pub tablet_connected: bool,
    /// Verletzung/Behandlung läuft — die Turnierleitung will das sehen.
    pub injury: bool,
    /// Das Feld hat die Turnierleitung gerufen.
    pub official_call: bool,
    /// Laufende Pause am Feld. Bewusst **typisiert** statt roh
    /// durchgereicht: Der Block kommt vom Zähltablett, und was von dort
    /// kommt, darf nicht ungeprüft an alle Turnierleitungs-Geräte und durch
    /// den Relay wandern.
    pub pause: Option<TlPause>,
    pub scorekeeper: Vec<String>,
    pub scorekeeper_assigned: bool,
    pub locked: bool,
    /// Ein **beendetes** Spiel hält dieses Feld noch, weil BTP es nicht
    /// abgeräumt hat. `match_id` ist dann 0 (es läuft ja nichts mehr), das
    /// Feld ist aber trotzdem nicht belegbar. Ohne diese Angabe zeigte die
    /// Seite ein freies Feld, auf das keine Zuweisung möglich ist.
    pub clearing: Option<i64>,
    /// Seit wann das Spiel auf dem Feld steht (= 1. Aufruf). Grundlage der
    /// hochzählenden Uhr und der Aufruf-Stufe.
    pub on_court_since_ms: Option<u64>,
    /// Zählformat, damit die Seite Satz- und Matchball anzeigen kann.
    pub best_of: i64,
    pub target_score: i64,
    pub cap_score: i64,
}

/// Ein Spiel in der Warteliste.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlMatch {
    pub match_id: i64,
    /// Die Nummer aus dem Papierplan — danach sucht der Helfer.
    pub match_num: Option<i64>,
    /// Angesetzte Zeit als `YYYYMMDDHHMM`, wie BTP sie führt.
    pub planned_time: Option<i64>,
    pub draw_name: String,
    pub round_name: String,
    pub class_label: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// In welche Halle das Spiel gehört, und woher wir das wissen.
    pub hall: String,
    pub hall_source: HallSource,
    /// Bereits in die Vorbereitung gerufen?
    pub prep_call: Option<TlPrepCall>,
    /// Warum das Spiel gerade nicht aufs Feld kann; `None` = spielbereit.
    pub blocked: Option<TlBlocked>,
}

/// Eine Halle des Turniers.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlHall {
    /// BTP-Kennung des Standorts — nötig für den Vorbereitungs-Aufruf.
    pub id: i64,
    pub name: String,
}

/// Ein offener Walkover-Vorschlag samt der Spiele, die er beträfe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlWalkover {
    pub id: String,
    /// Die Mannschaft, die aufgegeben hat.
    pub retired_team: String,
    pub draw_name: String,
    pub created_at_ms: u64,
    /// Die Spiele, die kampflos gewertet werden könnten — die
    /// Turnierleitung wählt aus.
    pub candidates: Vec<TlWalkoverMatch>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlWalkoverMatch {
    pub match_id: i64,
    pub round_name: String,
    /// Wer davon profitieren würde.
    pub opponent: String,
}

/// Eine laufende Pause am Feld (BWF-Intervall, Satzpause, Behandlung).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlPause {
    /// Art der Pause, wie das Zähltablett sie meldet.
    pub kind: String,
    /// Ende der Pause in Server-Zeit.
    pub ends_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlPrepCall {
    /// Halle, in die gerufen wurde; leer = ohne Hallenangabe.
    pub hall: String,
    pub called_at_ms: u64,
}

/// Warum ein Spiel wartet — mit Namen, denn „gesperrt" ohne Namen ist eine
/// Blackbox, der die Turnierleitung zu Recht misstraut.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum TlBlocked {
    /// Mindestens ein Spieler steht gerade auf einem Feld.
    Playing { players: Vec<String> },
    /// Mindestens ein Spieler ist noch in seiner Pause.
    Pause {
        /// Ab wann der Letzte wieder darf — damit der Helfer planen kann,
        /// statt zu raten.
        until_ms: u64,
        players: Vec<String>,
    },
}

impl From<Blocked> for TlBlocked {
    fn from(b: Blocked) -> Self {
        match b {
            Blocked::Playing { players } => TlBlocked::Playing { players },
            Blocked::Pause { until_ms, players } => TlBlocked::Pause { until_ms, players },
        }
    }
}

/// Wie viele wartende Spiele **je Halle** höchstens ausgeliefert werden.
///
/// Bei großen Turnieren stehen mehrere hundert Spiele an; alle zu übertragen
/// kostet bei jedem Abruf und auf jedem Gerät. Die Liste ist nach
/// Dringlichkeit sortiert, die vorderen sind die, um die es geht — was
/// wegfällt, meldet `truncated_halls` ehrlich.
const QUEUE_LIMIT_PER_HALL: usize = 120;

/// Ordnungsschlüssel eines wartenden Spiels samt dem Spiel selbst und seiner
/// Halle — die Zwischenform, in der sortiert und gekappt wird, bevor die
/// teuren Zeichenketten der Anzeige entstehen.
type OrderedMatch<'a> = (
    (bool, i64, i64, i64),
    &'a crate::btp::model::BtpMatch,
    String,
);

/// Baut den Anzeige-Zustand aus dem aktuellen BTP-Stand und dem, was der Host
/// selbst verwaltet (Aufrufe, Sperren, Live-Spielstände).
///
/// `rev` gibt der Aufrufer vor — er entscheidet, ob sich gegenüber dem
/// zuletzt ausgelieferten Stand überhaupt etwas geändert hat.
pub fn build_state(tablet: &TabletState, config: &AppConfig, now_ms: u64, rev: u64) -> TlState {
    let Some(snap) = tablet.snapshot_clone() else {
        // Noch kein Turnier geladen: leerer, aber gültiger Zustand — die
        // Seite zeigt „warte auf Turnierdaten" statt eines Fehlers.
        return TlState {
            rev,
            server_now_ms: now_ms,
            tournament: String::new(),
            multi_hall: false,
            halls: Vec::new(),
            auto_assign: auto_assign_view(config, tablet.auto_assign_paused()),
            call_timer: call_timer_view(config),
            rest_minutes: None,
            courts: Vec::new(),
            queue: Vec::new(),
            walkovers: Vec::new(),
            truncated_halls: Vec::new(),
        };
    };

    // Felder und Warteliste stammen aus **demselben** Schnappschuss. Zwei
    // getrennte Lesevorgänge könnten den Sync-Lauf dazwischen erwischen —
    // dann beschrieben Felder und Liste zwei verschiedene Turnierstände.
    let courts: Vec<TlCourt> = tablet
        .overview_from(&snap)
        .into_iter()
        .map(|c| {
            let clearing = clearing_match(&snap, c.court_id, c.match_id);
            court_view(c, clearing)
        })
        .collect();

    // Aufrufe einmal auflösen: Match-ID → Halle des Aufrufs.
    let calls = tablet.preparation_calls();
    let called_hall = |match_id: i64| -> Option<(String, u64)> {
        calls.iter().find(|c| c.match_id == match_id).map(|c| {
            let hall = c
                .location_id
                .and_then(|id| snap.locations.iter().find(|l| l.id == id))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            (hall, c.called_at_ms)
        })
    };

    let availability = PlayerAvailability::from_snapshot(&snap, config);

    // Spielbereite Spiele — dieselbe Bedingung wie bei der automatischen
    // Vergabe: geplant und mit feststehender Paarung. Spiele, deren Gegner
    // noch aus einem Vorspiel kommt, könnte niemand sinnvoll vergeben.
    // Erst nur die Ordnungsschlüssel sammeln — die teuren Zeichenketten
    // entstehen später und nur für die Spiele, die auch ausgeliefert werden.
    let mut ordered: Vec<OrderedMatch> = snap
        .matches
        .iter()
        .filter(|m| {
            m.status == crate::btp::model::MatchStatus::Scheduled
                && !m.team1.is_empty()
                && !m.team2.is_empty()
        })
        .map(|m| {
            let call = called_hall(m.id);
            let (hall, _) =
                assign::hall_for_match(config, &snap, m, call.as_ref().map(|(h, _)| h.as_str()));
            (assign::sort_key(m, call.is_some()), m, hall)
        })
        .collect();
    ordered.sort_by_key(|(key, _, _)| *key);

    // Je Halle kappen, nicht über das ganze Turnier.
    let mut per_hall: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut truncated: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue: Vec<TlMatch> = Vec::new();
    for (_, m, hall) in ordered {
        let count = per_hall.entry(hall.clone()).or_insert(0);
        if *count >= QUEUE_LIMIT_PER_HALL {
            truncated.insert(hall);
            continue;
        }
        *count += 1;
        let call = called_hall(m.id);
        let (_, hall_source) =
            assign::hall_for_match(config, &snap, m, call.as_ref().map(|(h, _)| h.as_str()));
        queue.push(TlMatch {
            match_id: m.id,
            match_num: m.match_num,
            planned_time: m.planned_time,
            draw_name: m.draw_name.clone(),
            round_name: m.round_name.clone(),
            class_label: m.class_label.clone(),
            team1: m.team1.iter().map(|p| p.name.clone()).collect(),
            team2: m.team2.iter().map(|p| p.name.clone()).collect(),
            hall,
            hall_source,
            prep_call: call.map(|(hall, called_at_ms)| TlPrepCall { hall, called_at_ms }),
            blocked: availability.blocked(m, now_ms).map(TlBlocked::from),
        });
    }

    let mut halls: Vec<TlHall> = snap
        .locations
        .iter()
        .filter(|l| !l.name.trim().is_empty())
        .map(|l| TlHall {
            id: l.id,
            name: l.name.trim().to_string(),
        })
        .collect();
    halls.sort_by_key(|h| h.name.to_lowercase());
    halls.dedup_by(|a, b| a.name == b.name);

    TlState {
        rev,
        server_now_ms: now_ms,
        tournament: snap.tournament_name.clone(),
        multi_hall: snap.is_multi_hall(),
        halls,
        auto_assign: auto_assign_view(config, tablet.auto_assign_paused()),
        call_timer: call_timer_view(config),
        // Genau der Wert, nach dem auch die Blockier-Zeiten in diesem
        // Datensatz gerechnet sind: Konfiguration schlägt BTP-Einstellung.
        // Ein abweichender Anzeigewert ließe die Seite sich selbst
        // widersprechen.
        rest_minutes: effective_rest_minutes(&snap, config),
        courts,
        queue,
        // Vorschläge, deren Spiele inzwischen alle gewertet sind, fallen
        // dabei heraus — sie hätten nichts mehr anzubieten.
        walkovers: tablet
            .walkover_proposals()
            .into_iter()
            .filter_map(|p| {
                let candidates: Vec<TlWalkoverMatch> = tablet
                    .walkover_candidates(p.entry_id)
                    .into_iter()
                    .map(|c| TlWalkoverMatch {
                        match_id: c.match_id,
                        round_name: c.round_name,
                        opponent: c.opponent,
                    })
                    .collect();
                if candidates.is_empty() {
                    return None;
                }
                Some(TlWalkover {
                    id: p.id,
                    retired_team: p.retired_team,
                    draw_name: p.draw_name,
                    created_at_ms: p.created_at_ms,
                    candidates,
                })
            })
            .collect(),
        truncated_halls: truncated.into_iter().collect(),
    }
}

/// Die tatsächlich geltende Pflichtpause in Minuten — dieselbe Regel, nach
/// der [`PlayerAvailability`] rechnet: Ein gesetzter Wert in der
/// Konfiguration schlägt die BTP-Einstellung.
fn effective_rest_minutes(
    snap: &crate::btp::model::BtpSnapshot,
    config: &AppConfig,
) -> Option<i64> {
    if config.auto_assign.pause_minutes > 0.0 {
        Some(config.auto_assign.pause_minutes as i64)
    } else {
        snap.rest_minutes.filter(|m| *m > 0)
    }
}

/// Welches **beendete** Spiel hält dieses Feld noch? `None`, wenn das Feld
/// wirklich frei ist oder ein laufendes Spiel darauf steht.
fn clearing_match(
    snap: &crate::btp::model::BtpSnapshot,
    court_id: i64,
    running_match_id: i64,
) -> Option<i64> {
    if running_match_id != 0 {
        return None;
    }
    assign::court_occupied_by(snap, court_id)
}

fn auto_assign_view(config: &AppConfig, paused: bool) -> TlAutoAssign {
    TlAutoAssign {
        enabled: config.auto_assign.enabled && !paused,
        configured: config.auto_assign.enabled,
        wait_minutes: config.auto_assign.wait_minutes,
        active_hall: config.auto_assign.active_hall.clone(),
    }
}

fn call_timer_view(config: &AppConfig) -> TlCallTimer {
    TlCallTimer {
        enabled: config.call_timer.enabled,
        second_call_minutes: config.call_timer.second_call_minutes,
        third_call_minutes: config.call_timer.third_call_minutes,
    }
}

/// Beschneidet die Feld-Übersicht auf das, was die Turnierleitung braucht.
///
/// Bewusst **weggelassen**: Nationalitäten (nur für die Sprachwahl der
/// Ansage, und diese Seite spricht nicht), Akkustand (keine Geräte-Übersicht
/// in diesem Feature) und die Aufschlag-Anzeige (Zählhilfe, keine
/// Vergabehilfe).
/// Beschneidet die Feld-Übersicht auf das, was die Turnierleitung braucht.
///
/// Bewusst **weggelassen**: Nationalitäten (nur für die Sprachwahl der
/// Ansage, und diese Seite spricht nicht), Akkustand (keine Geräte-Übersicht
/// in diesem Feature) und die Aufschlag-Anzeige (Zählhilfe, keine
/// Vergabehilfe).
fn court_view(c: crate::tablet::state::CourtOverview, clearing: Option<i64>) -> TlCourt {
    // Aus dem rohen Tablet-JSON nur die zwei bekannten Angaben übernehmen.
    // Alles andere bliebe ungeprüfter Fremdinhalt auf einer aus dem Internet
    // erreichbaren Seite.
    let pause = c.pause.as_ref().and_then(|v| {
        Some(TlPause {
            kind: v.get("kind")?.as_str()?.to_string(),
            ends_at_ms: v.get("endsAt")?.as_u64()?,
        })
    });
    TlCourt {
        clearing,
        pause,
        court_id: c.court_id,
        court: c.court,
        location: c.location,
        match_id: c.match_id,
        match_name: c.match_name,
        round_name: c.round_name,
        class_label: c.class_label,
        team1: c.team1,
        team2: c.team2,
        sets: c.sets,
        tablet_connected: c.tablet_connected,
        injury: c.injury,
        official_call: c.official_call,
        scorekeeper: c.scorekeeper,
        scorekeeper_assigned: c.scorekeeper_assigned,
        locked: c.locked,
        on_court_since_ms: c.on_court_since_ms,
        best_of: c.best_of,
        target_score: c.target_score,
        cap_score: c.cap_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{
        BtpCourt, BtpLocation, BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult,
        MatchStatus,
    };
    use crate::config::DisciplineHallRule;
    use crate::tablet::state::PreparationCall;

    /// Spieler ohne Lizenznummer — die Identität läuft dann über den Namen,
    /// und verschiedene Namen sind verschiedene Personen. (Mit derselben
    /// Lizenz für alle wären sie es nicht, was Verfügbarkeitsprüfungen
    /// unbrauchbar machte.)
    fn player(name: &str) -> BtpPlayer {
        BtpPlayer {
            id: 0,
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: Some("GER".to_string()),
            club: None,
        }
    }

    /// Spieler mit Lizenznummer — nur für den Datensparsamkeits-Test, der
    /// belegen muss, dass die Nummer den Host nicht verlässt.
    fn licensed_player(name: &str, license: &str) -> BtpPlayer {
        BtpPlayer {
            member_id: Some(license.to_string()),
            ..player(name)
        }
    }

    fn a_match(id: i64) -> BtpMatch {
        BtpMatch {
            id,
            draw_id: 1,
            planning_id: id,
            draw_name: "HE A".to_string(),
            discipline: Discipline::MensSingles,
            class_label: "A".to_string(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: vec![player("Müller")],
            team2: vec![player("Schmidt")],
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

    fn snap(courts: Vec<BtpCourt>, matches: Vec<BtpMatch>, locs: Vec<BtpLocation>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "Testturnier".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: locs,
            court_infos: courts,
            matches,
            events: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn a_court(id: i64, location_id: Option<i64>) -> BtpCourt {
        BtpCourt {
            id,
            name: format!("Feld {id}"),
            location_id,
            sort_order: id,
        }
    }

    fn state_with(snapshot: BtpSnapshot, config: &AppConfig) -> TlState {
        let tablet = TabletState::default();
        tablet.set_snapshot(snapshot);
        build_state(&tablet, config, 1_000_000, 7)
    }

    #[test]
    fn without_a_tournament_the_state_is_empty_but_valid() {
        // Die Seite soll „warte auf Turnierdaten" zeigen können, statt auf
        // einen Fehler zu laufen — bts-light startet regelmäßig, bevor BTP
        // etwas geladen hat.
        let tablet = TabletState::default();
        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 3);
        assert_eq!(s.rev, 3);
        assert_eq!(s.server_now_ms, 1_000_000);
        assert!(s.courts.is_empty());
        assert!(s.queue.is_empty());
        assert!(!s.multi_hall);
    }

    #[test]
    fn the_queue_holds_only_playable_matches() {
        // Dieselbe Bedingung wie bei der automatischen Vergabe. Ein Spiel,
        // dessen Gegner noch aus einem Vorspiel kommt, könnte niemand
        // sinnvoll vergeben — es gehört nicht in die Liste.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let mut done = a_match(2);
        done.status = MatchStatus::Finished;
        let mut open = a_match(3);
        open.team2 = Vec::new();

        let s = state_with(
            snap(
                vec![a_court(1, None)],
                vec![running, done, open, a_match(4)],
                Vec::new(),
            ),
            &AppConfig::default(),
        );
        let ids: Vec<i64> = s.queue.iter().map(|m| m.match_id).collect();
        assert_eq!(ids, vec![4]);
    }

    #[test]
    fn the_queue_is_sorted_like_the_automatic_assignment() {
        // Gerufene zuerst, dann nach Ansetzung. Zeigte die Liste eine andere
        // Reihenfolge als die Automatik benutzt, verlöre die Turnierleitung
        // das Vertrauen in beide.
        let mut early = a_match(1);
        early.planned_time = Some(202_608_081_200);
        let mut late = a_match(2);
        late.planned_time = Some(202_608_081_600);
        let mut called = a_match(3);
        called.planned_time = Some(202_608_081_800);

        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![early, late, called], Vec::new()));
        tablet.add_preparation_call(PreparationCall {
            match_id: 3,
            location_id: None,
            called_at_ms: 500,
        });

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let ids: Vec<i64> = s.queue.iter().map(|m| m.match_id).collect();
        assert_eq!(ids, vec![3, 1, 2], "gerufen zuerst, dann nach Ansetzung");
    }

    #[test]
    fn a_called_match_carries_its_call_and_hall() {
        // „In Vorbereitung seit X Minuten" ist die wichtigste Wartezeit der
        // ganzen Ansicht.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            vec![a_match(7)],
            vec![
                BtpLocation {
                    id: 1,
                    name: "Halle A".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle B".to_string(),
                },
            ],
        ));
        tablet.add_preparation_call(PreparationCall {
            match_id: 7,
            location_id: Some(2),
            called_at_ms: 900_000,
        });

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let m = &s.queue[0];
        assert_eq!(
            m.prep_call,
            Some(TlPrepCall {
                hall: "Halle B".to_string(),
                called_at_ms: 900_000
            })
        );
        assert_eq!(m.hall, "Halle B");
        assert_eq!(m.hall_source, HallSource::Call);
        assert!(s.multi_hall);
        let namen: Vec<&str> = s.halls.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(namen, vec!["Halle A", "Halle B"]);
        // Die Kennung muss mit, sonst könnte ein Vorbereitungs-Aufruf keine
        // Halle benennen.
        assert_eq!(s.halls[1].id, 2);
    }

    #[test]
    fn a_blocked_match_says_who_blocks_it_and_until_when() {
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![player("Müller")];
        running.team2 = vec![player("Gegner")];
        let mut waiting = a_match(2);
        waiting.team1 = vec![player("Müller")];
        waiting.team2 = vec![player("Frei")];

        let s = state_with(
            snap(vec![a_court(1, None)], vec![running, waiting], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(
            s.queue[0].blocked,
            Some(TlBlocked::Playing {
                players: vec!["Müller".to_string()]
            })
        );
    }

    #[test]
    fn courts_carry_the_running_match_and_its_clock() {
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let s = state_with(
            snap(vec![a_court(1, None)], vec![running], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts.len(), 1);
        assert_eq!(s.courts[0].court_id, 1);
        assert_eq!(s.courts[0].match_id, 7);
        assert_eq!(s.courts[0].team1, vec!["Müller".to_string()]);
    }

    #[test]
    fn the_state_carries_the_frame_the_page_needs() {
        let mut cfg = AppConfig::default();
        cfg.auto_assign.enabled = true;
        cfg.auto_assign.wait_minutes = 2.5;
        cfg.call_timer.enabled = true;
        let mut sn = snap(vec![a_court(1, None)], vec![a_match(1)], Vec::new());
        sn.rest_minutes = Some(20);

        let s = state_with(sn, &cfg);
        assert_eq!(s.tournament, "Testturnier");
        assert!(s.auto_assign.enabled);
        assert_eq!(s.auto_assign.wait_minutes, 2.5);
        assert!(s.call_timer.enabled);
        assert_eq!(s.rest_minutes, Some(20));
    }

    #[test]
    fn a_single_hall_tournament_offers_no_hall_filter() {
        let s = state_with(
            snap(
                vec![a_court(1, Some(1))],
                vec![a_match(1)],
                vec![BtpLocation {
                    id: 1,
                    name: "Main Location".to_string(),
                }],
            ),
            &AppConfig::default(),
        );
        assert!(!s.multi_hall, "eine Halle → kein Filter");
    }

    #[test]
    fn a_very_long_queue_is_capped_and_says_so() {
        // Große Turniere haben mehrere hundert wartende Spiele. Die Liste
        // ist nach Dringlichkeit sortiert; was hinten wegfällt, wird
        // gemeldet statt still unterschlagen.
        let matches: Vec<BtpMatch> = (1..=QUEUE_LIMIT_PER_HALL as i64 + 5).map(a_match).collect();
        let s = state_with(snap(Vec::new(), matches, Vec::new()), &AppConfig::default());
        assert_eq!(s.queue.len(), QUEUE_LIMIT_PER_HALL);
        // Ohne Hallenzuordnung ist die Gruppe der leere Name — auch sie
        // meldet ihre Kappung, statt sie zu verschweigen.
        assert_eq!(s.truncated_halls, vec![String::new()]);
    }

    fn cfg_with_device(token: &str) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.tl_web.enabled = true;
        cfg.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-1".to_string(),
            token: token.to_string(),
            label: "Tablet TL".to_string(),
            created_at_ms: 1,
            hall: String::new(),
        });
        cfg
    }

    #[test]
    fn a_known_token_identifies_its_device() {
        let cfg = cfg_with_device("tok-geheim");
        let dev = authorize(&cfg, "tok-geheim").expect("Gerät erkannt");
        assert_eq!(dev.id, "dev-1");
        assert_eq!(dev.label, "Tablet TL");
    }

    #[test]
    fn an_unknown_or_empty_token_is_rejected() {
        let cfg = cfg_with_device("tok-geheim");
        assert!(authorize(&cfg, "tok-falsch").is_none());
        assert!(authorize(&cfg, "").is_none(), "leer ist kein Zugang");
        assert!(authorize(&cfg, "   ").is_none());
    }

    #[test]
    fn no_token_works_while_the_feature_is_switched_off() {
        // Der Schalter ist die Sicherung: Ohne ihn ist der schreibende Pfad
        // unerreichbar, auch mit einem gültigen Token. Sonst bliebe ein
        // früher gekoppeltes Gerät nach dem Abschalten weiter drin.
        let mut cfg = cfg_with_device("tok-geheim");
        cfg.tl_web.enabled = false;
        assert!(authorize(&cfg, "tok-geheim").is_none());
    }

    #[test]
    fn assigning_plans_both_the_court_and_the_match_side() {
        // BTP führt die Zuordnung doppelt: am Feld und am Spiel. Fehlte
        // eine Seite, zeigte BTP das Spiel ohne Feld oder das Feld ohne
        // Spiel — beides hat die Turnierleitung schon erlebt.
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        let write = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::AssignCourt {
                court_id: 1,
                match_id: 7,
                expect: relay_proto::CourtExpectation::Free,
            },
        )
        .expect("erlaubt");
        assert_eq!(write.courts.len(), 1);
        assert_eq!(write.courts[0].court_id, 1);
        assert_eq!(write.courts[0].match_id, Some(7));
        assert_eq!(write.match_courts.len(), 1);
        assert_eq!(write.match_courts[0].match_id, 7);
        assert_eq!(write.match_courts[0].court_id, 1);
    }

    #[test]
    fn freeing_plans_to_clear_both_sides_too() {
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![running], Vec::new());
        let write = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::FreeCourt {
                court_id: 1,
                expect: relay_proto::CourtExpectation::Match { match_id: 7 },
            },
        )
        .expect("erlaubt");
        assert_eq!(write.courts[0].match_id, None, "Feld wird leer");
        assert_eq!(
            write.match_courts[0].court_id, 0,
            "und das Spiel verliert seine Feldzuordnung"
        );
    }

    #[test]
    fn moving_a_match_is_planned_as_a_single_write() {
        // Als „freigeben + zuweisen" wäre zwischendurch ein Zustand
        // sichtbar, in dem das Spiel auf keinem Feld steht — und die
        // automatische Vergabe könnte das Zielfeld wegschnappen.
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![running],
            Vec::new(),
        );
        let write = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::MoveMatch {
                from_court_id: 1,
                to_court_id: 2,
                match_id: 7,
                expect_from: relay_proto::CourtExpectation::Match { match_id: 7 },
                expect_to: relay_proto::CourtExpectation::Free,
            },
        )
        .expect("erlaubt");
        assert_eq!(
            write.courts.len(),
            2,
            "beide Felder in einem Schreibvorgang"
        );
        let from = write.courts.iter().find(|c| c.court_id == 1).unwrap();
        let to = write.courts.iter().find(|c| c.court_id == 2).unwrap();
        assert_eq!(from.match_id, None, "altes Feld wird leer");
        assert_eq!(to.match_id, Some(7), "neues Feld bekommt das Spiel");
        assert_eq!(write.match_courts.len(), 1);
        assert_eq!(write.match_courts[0].court_id, 2);
    }

    #[test]
    fn freeing_a_court_held_by_a_finished_match_keeps_its_court_in_btp() {
        // BTP hält am beendeten Spiel fest, wo es gespielt wurde — das ist
        // Turnier-Dokumentation. Der Ergebnispfad bewahrt sie ausdrücklich,
        // und die Desktop-Schaltfläche auch. Der Web-Weg darf sie nicht als
        // Nebenwirkung löschen, sonst hinge das Turnierprotokoll davon ab,
        // welche Oberfläche jemand benutzt hat.
        let mut done = a_match(7);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![done], Vec::new());
        let write = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::FreeCourt {
                court_id: 1,
                expect: relay_proto::CourtExpectation::Match { match_id: 7 },
            },
        )
        .expect("erlaubt");
        assert_eq!(write.courts[0].match_id, None, "das Feld wird frei");
        assert!(
            write.match_courts.is_empty(),
            "aber das beendete Spiel behält seine Feldangabe"
        );
    }

    #[test]
    fn a_rejected_action_is_not_planned_at_all() {
        // Nichts wird nach BTP geschrieben, wenn die Prüfung ablehnt —
        // die Ablehnung trägt den maschinenlesbaren Grund.
        let mut taken = a_match(9);
        taken.status = MatchStatus::OnCourt;
        taken.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![a_match(7), taken], Vec::new());
        let err = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::AssignCourt {
                court_id: 1,
                match_id: 7,
                expect: relay_proto::CourtExpectation::Free,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::CourtTaken));
        assert!(
            err.error.as_deref().unwrap_or_default().contains("belegt"),
            "die Meldung muss für Menschen lesbar sein: {err:?}"
        );
    }

    #[test]
    fn a_court_being_cleared_is_rejected_with_its_own_wording() {
        // „Feld ist belegt" wäre hier verwirrend: Auf dem Monitor ist das
        // Feld leer, das Spiel nur noch nicht abgeräumt. Der Text muss das
        // erklären, sonst sucht die Turnierleitung den Fehler bei sich.
        let mut done = a_match(9);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![a_match(7), done], Vec::new());
        let err = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::AssignCourt {
                court_id: 1,
                match_id: 7,
                expect: relay_proto::CourtExpectation::Free,
            },
        )
        .unwrap_err();
        let text = err.error.unwrap_or_default();
        assert!(
            text.contains("geräumt") || text.contains("beendet"),
            "erwartet ein Hinweis aufs Abräumen, war: {text}"
        );
    }

    #[test]
    fn actions_that_do_not_touch_courts_are_not_planned_here() {
        // Aktionen ohne Feldbezug laufen nicht über die Feld-Planung —
        // sie ändern nur den Zustand am Turnier-PC und schreiben nichts
        // nach BTP.
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        let err = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::SetAutoAssign { enabled: true },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::Unsupported));
    }

    #[test]
    fn entering_a_result_builds_the_same_write_as_the_desktop_path() {
        // Der Hauptfall: Niemand hat gezählt, die Turnierleitung trägt den
        // Endstand nach. Dieselbe Prüfung wie am Zähltablett (R5) — sie
        // liegt in `build_manual_result_update` und wird hier nur benutzt.
        let mut m = a_match(7);
        m.status = MatchStatus::OnCourt;
        m.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![m], Vec::new());

        let updates = plan_result_action(
            &s,
            None,
            9_000,
            &relay_proto::TlAction::EnterResult {
                match_id: 7,
                sets: vec![
                    relay_proto::SetAb { a: 21, b: 15 },
                    relay_proto::SetAb { a: 21, b: 19 },
                ],
                retired: false,
                winner: None,
                overwrite: false,
            },
        )
        .expect("erlaubt");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].btp_match_id, 7);
        assert_eq!(updates[0].sets, vec![(21, 15), (21, 19)]);
        assert!(updates[0].team1_won);
        // Das Spiel stand auf einem Feld — das wird im selben Vorgang frei.
        assert_eq!(updates[0].free_court_id, Some(1));
    }

    #[test]
    fn an_incomplete_set_is_rejected_like_everywhere_else() {
        // Ein noch laufender Satz darf nicht als gewonnener gewertet werden.
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        let err = plan_result_action(
            &s,
            None,
            9_000,
            &relay_proto::TlAction::EnterResult {
                match_id: 7,
                sets: vec![relay_proto::SetAb { a: 15, b: 12 }],
                retired: false,
                winner: None,
                overwrite: false,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::NotAllowed));
    }

    #[test]
    fn a_retirement_is_pointed_to_the_path_that_can_handle_it() {
        // Aufgabe und kampflose Wertung laufen über den Walkover-Weg —
        // dort hängt die Folgelogik (wer kommt weiter, welche Spiele der
        // Disziplin fallen mit). Eine halbe Umsetzung hier wäre schlimmer
        // als eine klare Ansage.
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        let err = plan_result_action(
            &s,
            None,
            9_000,
            &relay_proto::TlAction::EnterResult {
                match_id: 7,
                sets: vec![relay_proto::SetAb { a: 21, b: 15 }],
                retired: true,
                winner: Some(1),
                overwrite: false,
            },
        )
        .unwrap_err();
        assert!(
            err.error.unwrap_or_default().contains("Aufgabe"),
            "die Meldung muss den richtigen Weg nennen"
        );
    }

    #[test]
    fn overwriting_an_existing_result_is_not_available_yet() {
        // Steht bewusst aus, bis am echten BTP geprüft ist, was ein
        // Überschreiben mit dem Turnierbaum macht (Spec, offener Punkt 1).
        let mut done = a_match(7);
        done.status = MatchStatus::Finished;
        done.winner = Some(1);
        let s = snap(vec![a_court(1, None)], vec![done], Vec::new());
        let err = plan_result_action(
            &s,
            None,
            9_000,
            &relay_proto::TlAction::EnterResult {
                match_id: 7,
                sets: vec![
                    relay_proto::SetAb { a: 21, b: 15 },
                    relay_proto::SetAb { a: 21, b: 10 },
                ],
                retired: false,
                winner: None,
                overwrite: true,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::Unsupported));
    }

    #[test]
    fn confirming_a_walkover_writes_one_result_per_selected_match() {
        // Die aufgebende Mannschaft verliert, der Gegner gewinnt — ohne
        // Sätze, mit dem BTP-Vermerk für „kampflos".
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));
        let candidates = vec![
            crate::tablet::state::WalkoverCandidate {
                match_id: 11,
                draw_id: 1,
                planning_id: 111,
                round_name: "VF".to_string(),
                opponent: "Meier".to_string(),
                retired_is_team1: true,
            },
            crate::tablet::state::WalkoverCandidate {
                match_id: 12,
                draw_id: 1,
                planning_id: 112,
                round_name: "HF".to_string(),
                opponent: "Kraus".to_string(),
                retired_is_team1: false,
            },
        ];
        let updates = walkover_updates(&candidates, &[11, 12]);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].score_status, 1, "1 = kampflos");
        assert!(updates[0].sets.is_empty(), "kampflos hat keine Sätze");
        assert!(!updates[0].team1_won, "Team 1 hat aufgegeben");
        assert!(updates[1].team1_won, "hier war es Team 2");
        // Ein nicht ausgewähltes Spiel bleibt unangetastet.
        assert!(walkover_updates(&candidates, &[11]).len() == 1);
    }

    #[test]
    fn a_corrected_score_is_not_mistaken_for_a_repeat_of_the_first_one() {
        // Die Vorgangskennung der Seite fasst ein Zeitfenster zusammen. Wer
        // sich vertippt und sofort korrigiert, schickt darum dieselbe Kennung
        // mit ANDEREN Sätzen. Trägt der Fingerabdruck die Sätze nicht, gilt
        // die Korrektur als Wiederholung: Sie wird nie geschrieben, die Seite
        // meldet trotzdem Erfolg.
        let first = relay_proto::TlAction::EnterResult {
            match_id: 7,
            sets: vec![relay_proto::SetAb { a: 21, b: 15 }],
            retired: false,
            winner: None,
            overwrite: false,
        };
        let corrected = relay_proto::TlAction::EnterResult {
            match_id: 7,
            sets: vec![relay_proto::SetAb { a: 21, b: 51 }],
            retired: false,
            winner: None,
            overwrite: false,
        };
        assert_ne!(
            action_fingerprint(&first),
            action_fingerprint(&corrected),
            "andere Sätze sind eine andere Absicht"
        );
        assert_eq!(
            action_fingerprint(&first),
            action_fingerprint(&first.clone()),
            "derselbe Doppeltipp bleibt eine Wiederholung"
        );
    }

    #[test]
    fn every_action_payload_reaches_the_fingerprint() {
        // Gegen die Klasse von Fehlern, die den Korrektur-Fall verursacht hat:
        // Aktionen, die sich in ihrer Nutzlast unterscheiden, dürfen nie
        // denselben Fingerabdruck bekommen.
        use relay_proto::TlAction as A;
        let pairs: Vec<(A, A)> = vec![
            (
                A::CallPreparation {
                    match_ids: vec![1],
                    location_id: None,
                },
                A::CallPreparation {
                    match_ids: vec![2],
                    location_id: None,
                },
            ),
            (
                A::CallPreparation {
                    match_ids: vec![1],
                    location_id: Some(1),
                },
                A::CallPreparation {
                    match_ids: vec![1],
                    location_id: Some(2),
                },
            ),
            (
                A::RetractPreparation { match_id: 1 },
                A::RetractPreparation { match_id: 2 },
            ),
            (
                A::AnnounceCourtCall {
                    court_id: 1,
                    match_id: 7,
                },
                A::AnnounceCourtCall {
                    court_id: 2,
                    match_id: 7,
                },
            ),
            (
                A::AnnouncePrepCall {
                    match_id: 7,
                    side: relay_proto::PrepCallSide::Team1,
                },
                A::AnnouncePrepCall {
                    match_id: 7,
                    side: relay_proto::PrepCallSide::Team2,
                },
            ),
            (
                A::ConfirmWalkover {
                    proposal_id: "p-1".to_string(),
                    match_ids: vec![11],
                },
                A::ConfirmWalkover {
                    proposal_id: "p-1".to_string(),
                    match_ids: vec![11, 12],
                },
            ),
            (
                A::DismissWalkover {
                    proposal_id: "p-1".to_string(),
                },
                A::DismissWalkover {
                    proposal_id: "p-2".to_string(),
                },
            ),
            (
                A::ScorekeeperAdvance {
                    key: "a".to_string(),
                },
                A::ScorekeeperAdvance {
                    key: "b".to_string(),
                },
            ),
            (
                A::ScorekeeperRemove {
                    key: "a".to_string(),
                },
                A::ScorekeeperRemove {
                    key: "b".to_string(),
                },
            ),
            (
                A::ScorekeeperAdd {
                    names: vec!["a".to_string()],
                },
                A::ScorekeeperAdd {
                    names: vec!["b".to_string()],
                },
            ),
            (
                A::SetAutoAssign { enabled: true },
                A::SetAutoAssign { enabled: false },
            ),
        ];
        for (left, right) in pairs {
            assert_ne!(
                action_fingerprint(&left),
                action_fingerprint(&right),
                "gleicher Fingerabdruck für {} und {}",
                action_label(&left),
                action_label(&right)
            );
        }
    }

    #[test]
    fn a_walkover_without_a_single_writable_match_is_refused() {
        // Zwischen Anzeige und Antippen kann der letzte Kandidat aufs Feld
        // gewandert sein. Dann gibt es nichts zu schreiben — der Vorschlag
        // darf NICHT als erledigt verschwinden, sonst ist die kampflose
        // Wertung lautlos weg und niemand kann sie nachholen.
        let candidates = vec![crate::tablet::state::WalkoverCandidate {
            match_id: 11,
            draw_id: 1,
            planning_id: 111,
            round_name: "VF".to_string(),
            opponent: "Meier".to_string(),
            retired_is_team1: true,
        }];
        let err = plan_walkover_action(&candidates, &[99]).unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::AlreadyHandled));
        assert!(!err.ok);
        // Mit einem noch vorhandenen Kandidaten geht es weiter wie bisher.
        assert_eq!(plan_walkover_action(&candidates, &[11]).unwrap().len(), 1);
    }

    #[test]
    fn calling_matches_into_preparation_records_them_with_their_hall() {
        // Der Aufruf lebt am Turnier-PC (BTP kennt keinen solchen Zustand).
        // Die Halle entscheidet, welcher Meeting-Point-Monitor ihn zeigt.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![a_match(7), a_match(8)], Vec::new()));

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            5_000,
            &relay_proto::TlAction::CallPreparation {
                match_ids: vec![7, 8],
                location_id: Some(2),
            },
        )
        .expect("erlaubt");
        assert!(done.ok);

        let calls = tablet.preparation_calls();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|c| c.location_id == Some(2)));
        assert!(calls.iter().all(|c| c.called_at_ms == 5_000));
    }

    #[test]
    fn retracting_a_call_removes_exactly_that_one() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![a_match(7), a_match(8)], Vec::new()));
        apply_state_action(
            &tablet,
            &AppConfig::default(),
            5_000,
            &relay_proto::TlAction::CallPreparation {
                match_ids: vec![7, 8],
                location_id: None,
            },
        )
        .unwrap();

        apply_state_action(
            &tablet,
            &AppConfig::default(),
            6_000,
            &relay_proto::TlAction::RetractPreparation { match_id: 7 },
        )
        .unwrap();
        let left: Vec<i64> = tablet
            .preparation_calls()
            .iter()
            .map(|c| c.match_id)
            .collect();
        assert_eq!(left, vec![8]);
    }

    #[test]
    fn calling_an_unknown_match_is_rejected() {
        // Sonst hinge ein Aufruf an einem Spiel, das es nicht gibt — er
        // erschiene nirgends und ließe sich auch nicht zurücknehmen.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![a_match(7)], Vec::new()));
        let err = apply_state_action(
            &tablet,
            &AppConfig::default(),
            5_000,
            &relay_proto::TlAction::CallPreparation {
                match_ids: vec![99],
                location_id: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::NotAllowed));
        assert!(tablet.preparation_calls().is_empty());
    }

    #[test]
    fn the_scorekeeper_queue_can_be_tended() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));
        let cfg = AppConfig::default();

        apply_state_action(
            &tablet,
            &cfg,
            1_000,
            &relay_proto::TlAction::ScorekeeperAdd {
                names: vec!["Weber".to_string()],
            },
        )
        .unwrap();
        apply_state_action(
            &tablet,
            &cfg,
            1_100,
            &relay_proto::TlAction::ScorekeeperAdd {
                names: vec!["Fischer".to_string()],
            },
        )
        .unwrap();
        assert_eq!(tablet.scorekeeper_queue().len(), 2);

        // Vorziehen: der Zweite rückt an den Anfang.
        let second = tablet.scorekeeper_queue()[1].key.clone();
        apply_state_action(
            &tablet,
            &cfg,
            1_200,
            &relay_proto::TlAction::ScorekeeperAdvance {
                key: second.clone(),
            },
        )
        .unwrap();
        assert_eq!(tablet.scorekeeper_queue()[0].key, second);

        apply_state_action(
            &tablet,
            &cfg,
            1_300,
            &relay_proto::TlAction::ScorekeeperRemove { key: second },
        )
        .unwrap();
        assert_eq!(tablet.scorekeeper_queue().len(), 1);
    }

    #[test]
    fn adding_a_scorekeeper_without_a_name_is_rejected() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));
        let err = apply_state_action(
            &tablet,
            &AppConfig::default(),
            1_000,
            &relay_proto::TlAction::ScorekeeperAdd { names: Vec::new() },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::NotAllowed));
    }

    #[test]
    fn dismissing_a_walkover_proposal_removes_it() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));
        tablet.add_walkover_proposal(crate::tablet::state::WalkoverProposal {
            id: "p-1".to_string(),
            entry_id: 1,
            retired_team: "Weber".to_string(),
            draw_name: "HE".to_string(),
            created_at_ms: 1_000,
        });
        apply_state_action(
            &tablet,
            &AppConfig::default(),
            2_000,
            &relay_proto::TlAction::DismissWalkover {
                proposal_id: "p-1".to_string(),
            },
        )
        .unwrap();
        assert!(tablet.walkover_proposals().is_empty());
    }

    #[test]
    fn open_walkover_proposals_reach_the_page_with_their_matches() {
        // Ohne die Vorschläge samt betroffener Spiele könnte die Seite
        // nicht anbieten, was gewertet werden soll — die Turnierleitung
        // wählt ja aus, welche Folgespiele kampflos gehen.
        let tablet = TabletState::default();
        // Ein noch offenes Folgespiel der aufgebenden Mannschaft — ohne
        // solche Spiele hätte der Vorschlag nichts anzubieten und fiele zu
        // Recht heraus.
        let mut folgespiel = a_match(21);
        folgespiel.entry1_id = 1;
        folgespiel.round_name = "Halbfinale".to_string();
        tablet.set_snapshot(snap(Vec::new(), vec![folgespiel], Vec::new()));
        tablet.add_walkover_proposal(crate::tablet::state::WalkoverProposal {
            id: "p-1".to_string(),
            entry_id: 1,
            retired_team: "Weber / Fischer".to_string(),
            draw_name: "HD B".to_string(),
            created_at_ms: 4_000,
        });

        let s = build_state(&tablet, &AppConfig::default(), 9_000, 1);
        assert_eq!(s.walkovers.len(), 1);
        assert_eq!(s.walkovers[0].candidates.len(), 1);
        assert_eq!(s.walkovers[0].candidates[0].match_id, 21);
        assert_eq!(s.walkovers[0].id, "p-1");
        assert_eq!(s.walkovers[0].retired_team, "Weber / Fischer");
        assert_eq!(s.walkovers[0].draw_name, "HD B");
        assert_eq!(s.walkovers[0].created_at_ms, 4_000);
    }

    #[test]
    fn a_court_still_held_by_a_finished_match_is_not_shown_as_free() {
        // Sonst zeigt die Seite ein leeres Feld an, das die Vergabe-Prüfung
        // als belegt ablehnt — der Helfer tippt gegen eine unsichtbare Wand.
        // Der Zustand ist normal: BTP räumt das Feld erst nach einigen
        // Abfragen ab.
        let mut done = a_match(9);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = state_with(
            snap(vec![a_court(1, None)], vec![done, a_match(2)], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts[0].match_id, 0, "kein LAUFENDES Spiel");
        assert_eq!(
            s.courts[0].clearing,
            Some(9),
            "aber das Feld ist noch nicht frei"
        );
    }

    #[test]
    fn a_genuinely_free_court_says_so() {
        let s = state_with(
            snap(vec![a_court(1, None)], vec![a_match(2)], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts[0].match_id, 0);
        assert_eq!(s.courts[0].clearing, None);
    }

    #[test]
    fn the_queue_cap_applies_per_hall_not_globally() {
        // Global gekappt könnte eine ganze Halle wegfallen: Die Sortierung
        // zieht die frühen Runden der ersten Halle nach vorn, und das Gerät
        // in Halle C sähe eine leere Liste, obwohl dort hundert Spiele
        // warten. Das verletzt „nie stillschweigend ausgeblendet".
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: "HE A".to_string(),
            hall: "Halle A".to_string(),
        });
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: "HE B".to_string(),
            hall: "Halle B".to_string(),
        });

        let mut matches = Vec::new();
        for i in 1..=(QUEUE_LIMIT_PER_HALL as i64 + 10) {
            let mut m = a_match(i);
            m.draw_name = "HE A".to_string();
            m.planned_time = Some(202_608_080_800 + i); // Halle A zuerst
            matches.push(m);
        }
        for i in 1..=5 {
            let mut m = a_match(1000 + i);
            m.draw_name = "HE B".to_string();
            m.planned_time = Some(202_608_081_800 + i); // Halle B später
            matches.push(m);
        }

        let s = state_with(
            snap(
                vec![a_court(1, Some(1)), a_court(2, Some(2))],
                matches,
                vec![
                    BtpLocation {
                        id: 1,
                        name: "Halle A".to_string(),
                    },
                    BtpLocation {
                        id: 2,
                        name: "Halle B".to_string(),
                    },
                ],
            ),
            &cfg,
        );

        let in_b = s.queue.iter().filter(|m| m.hall == "Halle B").count();
        assert_eq!(in_b, 5, "Halle B darf nicht von Halle A verdrängt werden");
        assert_eq!(
            s.truncated_halls,
            vec!["Halle A".to_string()],
            "nur Halle A wurde gekappt, und das steht dort"
        );
    }

    #[test]
    fn the_hall_name_is_canonicalised_to_the_btp_spelling() {
        // Die Regel wird von Hand getippt; die Vergabe vergleicht sie
        // ohne Rücksicht auf Groß-/Kleinschreibung. Gäbe die Anzeige die
        // getippte Schreibweise aus, fände der Hallenfilter das Spiel
        // nicht — und weil es eine Halle *hat*, landete es auch nicht im
        // Abschnitt „ohne Hallenzuordnung". Es verschwände lautlos.
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "halle b".to_string(), // klein getippt
        });
        let s = state_with(
            snap(
                vec![a_court(1, Some(1)), a_court(2, Some(2))],
                vec![a_match(7)],
                vec![
                    BtpLocation {
                        id: 1,
                        name: "Halle A".to_string(),
                    },
                    BtpLocation {
                        id: 2,
                        name: "Halle B".to_string(),
                    },
                ],
            ),
            &cfg,
        );
        assert_eq!(s.queue[0].hall, "Halle B", "Schreibweise aus BTP");
        assert!(s.halls.iter().any(|h| h.name == s.queue[0].hall));
    }

    #[test]
    fn the_rest_time_matches_the_one_actually_enforced() {
        // Der angezeigte Pausenwert muss der sein, nach dem auch die
        // Blockier-Zeiten im selben Datensatz gerechnet sind — sonst
        // widerspricht die Seite sich selbst.
        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 30.0;
        let mut sn = snap(Vec::new(), vec![a_match(1)], Vec::new());
        sn.rest_minutes = Some(20); // BTP sagt etwas anderes

        let s = state_with(sn, &cfg);
        assert_eq!(
            s.rest_minutes,
            Some(30),
            "die Konfiguration schlägt BTP — wie bei der Vergabe"
        );
    }

    #[test]
    fn the_pause_is_republished_as_known_fields_only() {
        // Der Pausen-Block kommt roh vom Zähltablett. Würde er unverändert
        // weitergereicht, könnte ein Tablett beliebigen Inhalt an alle
        // Turnierleitungs-Geräte und durch den Relay schicken.
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"game","endsAt":1700000000000,"heimlich":"streng geheim"}}"#
                .to_string(),
        );

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let pause = s.courts[0].pause.as_ref().expect("Pause vorhanden");
        assert_eq!(pause.kind, "game");
        assert_eq!(pause.ends_at_ms, 1_700_000_000_000);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("heimlich"),
            "unbekannte Felder des Tabletts dürfen nicht weiterwandern: {json}"
        );
    }

    /// Sammelt alle Feldnamen eines JSON-Baums.
    fn field_names(v: &serde_json::Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, val) in map {
                    out.insert(k.clone());
                    field_names(val, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|i| field_names(i, out)),
            _ => {}
        }
    }

    #[test]
    fn every_published_field_is_deliberately_allowed() {
        // Der strukturelle Wächter: Statt nach verbotenen Wörtern zu
        // suchen (was nur findet, woran jemand gedacht hat), wird JEDES
        // ausgelieferte Feld gegen eine bewusst gepflegte Liste geprüft.
        // Wer ein Feld hinzufügt, muss es hier eintragen — und dabei
        // begründen, warum es nach außen darf.
        const ERLAUBT: &[&str] = &[
            // Rahmen
            "rev",
            "server_now_ms",
            "tournament",
            "multi_hall",
            "halls",
            "rest_minutes",
            "auto_assign",
            "call_timer",
            "enabled",
            // Grundeinstellung der automatischen Vergabe: kein
            // personenbezogenes Datum, und die Seite braucht sie, um den
            // Schalter überhaupt anbieten zu dürfen.
            "configured",
            "wait_minutes",
            "active_hall",
            "second_call_minutes",
            "third_call_minutes",
            "courts",
            "queue",
            "truncated_halls",
            // Walkover-Vorschläge: Mannschaftsnamen, die auch sonst überall
            // in der Ansicht stehen, plus Runde und Gegner. Die
            // Turnierleitung muss sehen, was sie da kampflos wertet.
            "walkovers",
            "retired_team",
            "candidates",
            "opponent",
            // Feld
            "court_id",
            "court",
            "location",
            "match_id",
            "match_name",
            "round_name",
            "class_label",
            "team1",
            "team2",
            "sets",
            "tablet_connected",
            "injury",
            "official_call",
            "pause",
            "kind",
            "ends_at_ms",
            "scorekeeper",
            "scorekeeper_assigned",
            "locked",
            "clearing",
            "on_court_since_ms",
            "best_of",
            "target_score",
            "cap_score",
            // Warteliste
            "match_num",
            "planned_time",
            "draw_name",
            "hall",
            "hall_source",
            "prep_call",
            "called_at_ms",
            "blocked",
            "reason",
            "players",
            "until_ms",
        ];

        let tablet = TabletState::default();
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![running, a_match(2)],
            Vec::new(),
        ));
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"game","endsAt":1700000000000}}"#.to_string(),
        );
        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);

        let value = serde_json::to_value(&s).unwrap();
        let mut names = std::collections::BTreeSet::new();
        field_names(&value, &mut names);
        let unerlaubt: Vec<&String> = names
            .iter()
            .filter(|n| !ERLAUBT.contains(&n.as_str()))
            .collect();
        assert!(
            unerlaubt.is_empty(),
            "Nicht freigegebene Felder im Anzeige-Zustand: {unerlaubt:?} — \
             eintragen und begründen, warum sie nach außen dürfen"
        );
    }

    #[test]
    fn the_state_never_carries_personal_data_beyond_its_purpose() {
        // Diese Daten laufen über eine aus dem Internet erreichbare Seite.
        // Der Test schlägt fehl, sobald jemand ein Feld nachrüstet, das
        // Lizenznummer, Geburtsjahr oder Nationalität transportiert — er
        // macht die Datenschutzregel durchsetzbar statt nur dokumentiert.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        running.team2 = vec![licensed_player("Gegner", "08-005678")];
        let mut waiting = a_match(2);
        waiting.team1 = vec![licensed_player("Weber", "08-009999")];
        waiting.team2 = vec![licensed_player("Fischer", "08-004321")];

        let s = state_with(
            snap(vec![a_court(1, None)], vec![running, waiting], Vec::new()),
            &AppConfig::default(),
        );
        let json = serde_json::to_string(&s).unwrap().to_lowercase();

        for verboten in [
            "08-001234",  // die Lizenznummer aus dem Fixture
            "member",     // Lizenznummer-Feld
            "nationalit", // Nationalität (nur für die Sprachwahl der Ansage)
            "ger",        // deren Wert aus dem Fixture
            "birth",      // Geburtsjahr — laut Projektregel nirgends
            "geburt",
            "battery", // Akkustand: keine Geräte-Übersicht in diesem Feature
            "serving", // Aufschlag: Zählhilfe, keine Vergabehilfe
        ] {
            assert!(
                !json.contains(verboten),
                "'{verboten}' darf nicht im Anzeige-Zustand stehen: {json}"
            );
        }
        // Gegenprobe: Die Namen, die die Turnierleitung zum Arbeiten braucht,
        // sind sehr wohl da — sonst prüfte der Test nur einen leeren Zustand.
        assert!(json.contains("müller"));
    }
}
