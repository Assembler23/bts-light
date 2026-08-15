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

/// Das Gerät zu einer **Kennung** — der Weg über den Relay.
///
/// Dort prüft der Relay den Zugang und schickt nur die Kennung weiter (ein
/// Zugang hat in Protokollen nichts verloren). Der Turnier-PC schaut
/// trotzdem in seiner eigenen Liste nach: Sonst hinge die
/// Nachvollziehbarkeit allein am Relay, und ein Protokolleintrag benennte
/// womöglich ein Gerät, das dieser Rechner nie gekoppelt hat.
pub(crate) fn device_by_id(config: &AppConfig, device_id: &str) -> Option<crate::config::TlDevice> {
    if !config.tl_web.enabled {
        return None;
    }
    let id = device_id.trim();
    if id.is_empty() {
        return None;
    }
    config.tl_web.devices.iter().find(|d| d.id == id).cloned()
}

/// Die Geräte, die der Relay kennen muss: Kennung und Zugang, **kein Name**.
///
/// Das Etikett bleibt am Turnier-PC — es kann einen Personennamen enthalten,
/// und der hat auf einem fremden Server nichts zu suchen. Ist die
/// Oberfläche abgeschaltet, ist die Liste leer, und das heißt beim Relay
/// ausdrücklich „kein Gerät zugelassen".
pub(crate) fn auth_devices(config: &AppConfig) -> Vec<relay_proto::TlAuthDevice> {
    if !config.tl_web.enabled {
        return Vec::new();
    }
    config
        .tl_web
        .devices
        .iter()
        .filter(|d| !d.token.trim().is_empty())
        .map(|d| relay_proto::TlAuthDevice {
            id: d.id.clone(),
            token: d.token.clone(),
            // Panel-Profil-Zuordnung (Spec tl-web-panelsystem, ADR 0025):
            // reitet auf demselben TlAuth-Spiegel wie Kennung/Zugang, damit
            // der Relay sie in `X-Tl-Active-Profile` beantworten kann.
            profile_id: d.profile_id.clone(),
        })
        .collect()
}

/// Erkennungsmerkmal der Geräteliste — **inklusive der Zugänge**, aber als
/// Hash.
///
/// Der Abgleich darf nicht nur die Kennungen sehen: Wird der Zugang eines
/// verlorenen Tablets ersetzt, bleibt seine Kennung dieselbe (sie ist die
/// Geräte-Identität). Ein Abgleich über Kennungen allein hielte den alten
/// Zugang beim Relay am Leben — das verlorene Gerät schriebe weiter mit.
///
/// Gehasht statt im Klartext, weil dieser Wert in einer Variablen lebt, die
/// beim Suchen nach Fehlern schnell ausgegeben ist. Dasselbe FNV-1a wie beim
/// Ansage-Cache: klein, stabil, ohne Abhängigkeit.
///
/// Trägt auch `profile_id` mit (Spec tl-web-panelsystem, ADR 0025): Ändert
/// sich nur die Profilwahl eines Geräts (Kennung/Zugang bleiben gleich),
/// muss `push_tl_auth` das trotzdem als Änderung erkennen — sonst bliebe
/// der Relay auf der alten Zuordnung stehen und der `X-Tl-Active-Profile`-
/// Header zeigte ein Profil, das am Gerät längst nicht mehr gilt.
pub(crate) fn auth_fingerprint(devices: &[relay_proto::TlAuthDevice]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for d in devices {
        for b in
            d.id.bytes()
                .chain(b":".iter().copied())
                .chain(d.token.bytes())
                .chain(b":".iter().copied())
                .chain(d.profile_id.bytes())
        {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{}:{hash:016x}", devices.len())
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
                    // Bleibt hier leer: Diese Funktion ist rein (kein
                    // Zustand). Die Besetzung trägt der Aufrufer nach, der
                    // den Roster kennt (ADR 0021).
                    officials: None,
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
    config: &AppConfig,
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
            // Mit dem Aufruf verfallen auch seine Nachrufe: Ein später erneut
            // gerufenes Spiel beginnt wieder beim zweiten Aufruf, nicht beim
            // letzten.
            tablet.forget_prep_calls(*match_id);
            Ok(TlResponse::ok(0))
        }
        A::SetHall { match_id, hall } => {
            // Nur Hallen, die das Turnier wirklich hat — sonst stünde das
            // Spiel in einem Ort, den kein Filter zeigt und kein Feld
            // bedient. Leerer Name nimmt die Zuweisung zurück.
            let hall = hall.trim();
            if !hall.is_empty() {
                let snap = tablet.snapshot_clone();
                let bekannt = snap.as_ref().is_some_and(|s| {
                    s.locations
                        .iter()
                        .any(|l| l.name.trim().eq_ignore_ascii_case(hall))
                });
                if !bekannt {
                    return Err(TlResponse::err(
                        relay_proto::TlErrorCode::HallNotAllowed,
                        format!("Die Halle {hall} gibt es in diesem Turnier nicht."),
                    ));
                }
            }
            tablet.set_manual_hall(*match_id, hall);
            Ok(TlResponse::ok(0))
        }
        A::ExcludeFromAutoAssign { match_id, excluded } => {
            // Wie bei CallPreparation: ein unbekanntes Match erschiene
            // nirgends und ließe sich auch nicht zurücknehmen.
            if !known_match(*match_id) {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    format!("Spiel {match_id} gibt es im aktuellen Turnierstand nicht."),
                ));
            }
            tablet.set_auto_assign_excluded(*match_id, *excluded);
            Ok(TlResponse::ok(0))
        }
        A::QueueReorder {
            match_id,
            before_match_id,
        } => {
            // Wie ExcludeFromAutoAssign: ein unbekanntes Match erschiene
            // nirgends und ließe sich auch nicht sinnvoll einsortieren.
            if !known_match(*match_id) {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    format!("Spiel {match_id} gibt es im aktuellen Turnierstand nicht."),
                ));
            }
            tablet.queue_reorder(config, *match_id, *before_match_id);
            Ok(TlResponse::ok(0))
        }
        A::QueueOrderReset => {
            tablet.queue_order_reset();
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
        A::AnnounceCourtCall { court_id, match_id } => {
            let Some(snap) = tablet.snapshot_clone() else {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Es ist noch kein Turnier geladen.",
                ));
            };
            // Steht das Spiel wirklich auf diesem Feld? Sonst ginge eine
            // Ansage für eine Begegnung hinaus, die dort gar nicht spielt.
            // BTP ist die Wahrheit (R2), nicht die Kachel im Browser.
            let on_court = snap
                .matches
                .iter()
                .any(|m| m.id == *match_id && m.court_id == Some(*court_id));
            if !on_court {
                return Err(TlResponse::err(
                    C::CourtFree,
                    "Dieses Spiel steht nicht mehr auf dem Feld — bitte neu laden.",
                ));
            }
            let hall = snap
                .court_infos
                .iter()
                .find(|c| c.id == *court_id)
                .and_then(|c| c.location_id)
                .and_then(|id| snap.locations.iter().find(|l| l.id == id))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            // Die Uhr am Feld darf nicht weiter sein als der Aufruf: Steht
            // dort schon „Letzter Aufruf", wäre ein zweiter ein Rückschritt.
            let faellig = due_call_stage(tablet, config, *court_id, *match_id, now_ms);
            let stage = tablet.note_court_call_at_least(*court_id, *match_id, faellig);
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::CourtCall {
                    court_id: *court_id,
                    match_id: *match_id,
                    stage,
                },
                now_ms,
            );
            Ok(announcement_response(tablet, &hall, now_ms))
        }
        A::AnnouncePrepCall { match_id, side } => {
            // Nur, was auch gerufen wurde: Ein „erneuter" Aufruf ohne ersten
            // wäre für die Wartenden nicht nachvollziehbar, und die Halle des
            // Aufrufs ist die einzige Angabe, wohin er gehört.
            let Some(call) = tablet
                .preparation_calls()
                .into_iter()
                .find(|c| c.match_id == *match_id)
            else {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Dieses Spiel ist nicht in Vorbereitung gerufen.",
                ));
            };
            let hall = call
                .location_id
                .and_then(|id| {
                    tablet
                        .snapshot_clone()?
                        .locations
                        .iter()
                        .find(|l| l.id == id)
                        .map(|l| l.name.clone())
                })
                .unwrap_or_default();
            // Die Staffelung 2 → 3 zählt der Turnier-PC, damit der Nachruf
            // aus der Seite und der aus der Desktop-Oberfläche dieselbe
            // Ansage erzeugen.
            let stage = tablet.note_prep_call(*match_id, side_key(side));
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::PrepCall {
                    match_id: *match_id,
                    side: *side,
                    stage,
                },
                now_ms,
            );
            Ok(announcement_response(tablet, &hall, now_ms))
        }
        // ── Schiedsrichter (Spec schiedsrichter-management) ─────────
        A::OfficialAssign {
            court_id: _,
            match_id,
            official_id,
            role,
        } => {
            officials_an(tablet)?;
            tablet
                .officials_store()
                .assign(*match_id, tl_role(*role), *official_id);
            Ok(TlResponse::ok(0))
        }
        A::OfficialClear {
            court_id: _,
            match_id,
            role,
        } => {
            officials_an(tablet)?;
            tablet
                .officials_store()
                .clear_assignment(*match_id, tl_role(*role));
            Ok(TlResponse::ok(0))
        }
        A::OfficialPause {
            official_id,
            paused,
        } => {
            officials_an(tablet)?;
            tablet.officials_store().set_paused(*official_id, *paused);
            Ok(TlResponse::ok(0))
        }
        A::OfficialReorder {
            official_id,
            before_official_id,
        } => {
            officials_an(tablet)?;
            tablet
                .officials_store()
                .reorder(*official_id, *before_official_id);
            Ok(TlResponse::ok(0))
        }
        A::OfficialSetClub { official_id, club } => {
            officials_an(tablet)?;
            tablet.officials_store().set_club(*official_id, club);
            Ok(TlResponse::ok(0))
        }
        A::OfficialBlocklistSet {
            official_id,
            clubs,
            players,
        } => {
            // Sperrlisten sind Personendaten — sie sollen gar nicht erst in
            // der Turnierdatei eines Turniers landen, das ohne
            // Schiedsrichter läuft.
            officials_an(tablet)?;
            tablet
                .officials_store()
                .set_blocklists(*official_id, clubs.clone(), players.clone());
            Ok(TlResponse::ok(0))
        }
        A::OfficialsCourtToggle {
            court_id,
            sr,
            ar,
            operator,
        } => {
            officials_an(tablet)?;
            tablet.officials_store().set_court_switches(
                *court_id,
                crate::tablet::officials::CourtSwitches {
                    sr: *sr,
                    ar: *ar,
                    operator: *operator,
                },
            );
            Ok(TlResponse::ok(0))
        }
        A::AnnounceOfficials { court_id } => {
            officials_an(tablet)?;
            let Some(snap) = tablet.snapshot_clone() else {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Es ist noch kein Turnier geladen.",
                ));
            };
            // Nur ansagen, was es zu sagen gibt — sonst ginge ein Gong ohne
            // Inhalt in die Halle.
            let m = snap.matches.iter().find(|m| {
                m.court_id == Some(*court_id) && m.status == crate::btp::model::MatchStatus::OnCourt
            });
            let (sr, ar, _) = tablet.court_officials(m, &snap);
            if sr.is_empty() && ar.is_empty() {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Diesem Feld ist niemand zugewiesen.",
                ));
            }
            let hall = snap
                .court_infos
                .iter()
                .find(|c| c.id == *court_id)
                .and_then(|c| c.location_id)
                .and_then(|id| snap.locations.iter().find(|l| l.id == id))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::Officials {
                    court_id: *court_id,
                },
                now_ms,
            );
            Ok(announcement_response(tablet, &hall, now_ms))
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

/// Baut den Anzeige-Zustand **mit** seiner Revision.
///
/// Die eine Stelle, an der die Revision entsteht — für den LAN-Server und
/// den Relay-Weg gleichermaßen. Zwei getrennte Zähler wären schlimmer als
/// keiner: Ein Gerät im Hallennetz und eines aus dem Internet meinten mit
/// derselben Zahl verschiedene Stände, und die Altersprüfung träfe zufällige
/// Entscheidungen.
pub(crate) fn build_state_with_rev(
    tablet: &TabletState,
    config: &AppConfig,
    now_ms: u64,
) -> TlState {
    let mut state = build_state(tablet, config, now_ms, 0);
    state.rev = tablet.tl_revision(&state_fingerprint(&mut state));
    state
}

/// Der Anzeige-Zustand als JSON für den Relay — **garantiert klein genug**.
///
/// Der Relay legt einen zu großen Zustand nicht ab und sagt es dem Host
/// nicht. Ohne eigene Kürzung wäre die Cloud-Oberfläche also ausgerechnet in
/// großen Turnieren tot, und niemand wüsste warum. Gekürzt wird die
/// Warteliste — sie ist der große Teil und nach Dringlichkeit sortiert, die
/// vorderen Spiele sind die, um die es geht. `truncated_halls` meldet es.
///
/// Liefert `(json, rev)`. Dass gekürzt wurde, steht im Zustand selbst
/// (`truncated_halls`) — dort sieht es auch die Turnierleitung.
pub(crate) fn state_for_relay(
    tablet: &TabletState,
    config: &AppConfig,
    now_ms: u64,
) -> (String, u64) {
    // Stufen statt Feinsuche: wenige, vorhersagbare Größen sind im Betrieb
    // leichter zu erklären als eine Zahl, die bei jedem Abruf anders ausfällt.
    //
    // Die erste Stufe ist **kürzer als im Hallennetz** (40 statt 120), und
    // das ist Absicht: Der Zustand geht bei jedem Ballwechsel neu über die
    // Leitung — die Live-Punktestände gehören dazu, sie sind der Grund, warum
    // man hinsieht. Bei voller Liste wäre das ein Dauerstrom von zehner
    // Kilobyte alle zwei Sekunden, auf einem Turnier-PC womöglich über
    // Mobilfunk, neben dem Ergebnisweg nach BTP. Vierzig wartende Spiele je
    // Halle sind mehr, als eine Turnierleitung unterwegs überblickt; was
    // fehlt, meldet `truncated_halls` ehrlich, und im Hallennetz steht
    // weiterhin die volle Liste.
    const STUFEN: [usize; 4] = [40, 20, 10, 5];
    let mut letzte = String::new();
    let mut letzte_rev = 0;
    for limit in STUFEN {
        let mut state = build_state_limited(tablet, config, now_ms, 0, limit);
        state.rev = tablet.tl_revision(&state_fingerprint(&mut state));
        letzte_rev = state.rev;
        letzte = serde_json::to_string(&state).unwrap_or_default();
        if letzte.len() <= relay_proto::MAX_TL_STATE_LEN {
            return (letzte, letzte_rev);
        }
    }
    // Selbst die kürzeste Stufe passt nicht — dann liegt es nicht an der
    // Warteliste. Lieber den zu großen Stand schicken und den Relay
    // entscheiden lassen (er verwirft ihn und die Seite meldet sich als
    // nicht verbunden), als hier stillschweigend gar nichts zu tun.
    tracing::warn!(
        "TL-Zustand bleibt mit {} Bytes über der Relay-Grenze ({}) — \
         die Cloud-Oberfläche wird nichts anzeigen",
        letzte.len(),
        relay_proto::MAX_TL_STATE_LEN
    );
    (letzte, letzte_rev)
}

/// Fingerabdruck des Zustands **ohne** Uhrzeit.
///
/// Ohne diese Ausnahme zählte die Revision im Sekundentakt hoch, obwohl sich
/// nichts geändert hat: Die Übertragungsersparnis wäre dahin, und jeder Tipp
/// käme als „auf überholtem Stand" zurück.
///
/// Nimmt den Zustand **veränderlich** und stellt die Uhrzeit danach wieder
/// her, statt ihn zu kopieren: Das hier ist der heißeste Pfad des Features
/// — jedes Gerät fragt alle zwei Sekunden, und der Zustand kann zehner
/// Kilobyte groß sein. Eine Kopie nur zum Wegwerfen wäre auf demselben
/// Rechner spürbar, der nebenher BTP und die Tablets bedient. (`rev` ist
/// beim Bau ohnehin 0.)
fn state_fingerprint(state: &mut TlState) -> String {
    let zeit = state.server_now_ms;
    state.server_now_ms = 0;
    let fp = serde_json::to_string(&state).unwrap_or_default();
    state.server_now_ms = zeit;
    fp
}

/// Kurzform einer Partei als Schlüssel der Nachruf-Zählung.
fn side_key(side: &relay_proto::PrepCallSide) -> &'static str {
    match side {
        relay_proto::PrepCallSide::Both => "both",
        relay_proto::PrepCallSide::Team1 => "team1",
        relay_proto::PrepCallSide::Team2 => "team2",
    }
}

/// Welche Aufruf-Stufe ist nach der **Uhr** fällig?
///
/// Dieselbe Rechnung, die das Aufruf-Abzeichen in der Desktop-Übersicht und
/// die Uhr auf der Turnierleitungs-Seite anstellen. Sie gehört hierher,
/// damit alle Geräte dieselbe Zahl bekommen — rechnete jede Oberfläche für
/// sich, hinge die eine bei „2. Aufruf", während die andere längst den
/// letzten anzeigt.
///
/// `0`, wenn es nichts zu sagen gibt: kein Timer, keine Standzeit.
fn due_call_stage(
    tablet: &TabletState,
    config: &AppConfig,
    court_id: i64,
    match_id: i64,
    now_ms: u64,
) -> u8 {
    if !config.call_timer.enabled {
        return 0;
    }
    // Sind Punkte gefallen, sind die Spieler da — dann hebt die Uhr nichts
    // mehr an. Ohne diesen Riegel schallte „Dritter und letzter Aufruf"
    // durch die Halle, während längst gespielt wird. Beide Oberflächen
    // halten sich an dieselbe Regel.
    if tablet.points_scored(court_id, match_id) {
        return 0;
    }
    let Some(since) = tablet.on_court_since_ms(court_id, match_id) else {
        return 0;
    };
    let minuten = now_ms.saturating_sub(since) as f64 / 60_000.0;
    if minuten >= config.call_timer.third_call_minutes {
        3
    } else if minuten >= config.call_timer.second_call_minutes {
        2
    } else {
        0
    }
}

/// Die Antwort auf einen Ansage-Auftrag.
///
/// Der Auftrag ist abgelegt und die Stufe gezählt — das gilt auch dann, wenn
/// in der Halle gerade kein Gerät zuhört. Die Turnierleitung soll aber nicht
/// glauben, es sei etwas erklungen: Sie steht im Zweifel im Büro und hört die
/// Anlage gar nicht. Die Stufe trotzdem hochzuzählen ist die ehrlichere
/// Variante — sonst stünde sie später auf einem anderen Stand als das, was
/// die Halle gehört hat.
fn announcement_response(tablet: &TabletState, hall: &str, now_ms: u64) -> relay_proto::TlResponse {
    let ok = relay_proto::TlResponse::ok(0);
    if tablet.has_announce_listener(hall, now_ms) {
        return ok;
    }
    let wo = if hall.is_empty() {
        "Es ist kein Ansage-Gerät verbunden".to_string()
    } else {
        format!("In {hall} ist kein Ansage-Gerät verbunden")
    };
    ok.with_warning(format!("{wo} — der Aufruf wurde nicht gesprochen."))
}

/// Warum eine Ergebnis-Korrektur gerade nicht geht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CorrectionBlocker {
    /// Der Sieger spielt sein Folgespiel bereits.
    Running,
    /// Das Folgespiel ist schon gewertet.
    Decided,
    /// Das Folgespiel steht bereit, aber es ist ungeprüft, was BTP beim
    /// Überschreiben mit dem Baum macht.
    Untested,
}

/// Steht der Korrektur eines Ergebnisses etwas im Weg?
///
/// `None` heißt: nichts, was der geänderte Sieger durcheinanderbringen
/// könnte — ein Finale, ein Gruppenspiel, ein Draw ohne weitere Runde.
///
/// Die Kante im Turnierbaum liegt in `from1`/`from2`: Ein Folgespiel
/// verweist über sie auf die Planungsposition seines Vorgängers. Die
/// Positionen sind **nur je Draw eindeutig** — BTP vergibt in jedem Draw
/// dieselben —, deshalb zählt nur ein Treffer im selben Draw.
///
/// Der Fall `FollowUpUntested` ist der ehrliche Rest: Ein beendetes KO-Spiel
/// wird selbst zum Feeder-Slot, der Sieger steht also **sofort** im nächsten
/// Spiel. Ob BTP den Baum beim Überschreiben neu rechnet, weiß bis zum
/// Experiment niemand (docs/btp_protocol.md) — und wer das nicht weiß, darf
/// den Turnierbaum nicht anfassen.
pub(crate) fn correction_blocker(
    snap: &crate::btp::model::BtpSnapshot,
    match_id: i64,
) -> Option<CorrectionBlocker> {
    use crate::btp::model::MatchStatus;
    let m = snap.matches.iter().find(|m| m.id == match_id)?;
    let folge = snap.matches.iter().find(|o| {
        o.draw_id == m.draw_id
            && o.id != m.id
            && (o.from1 == Some(m.planning_id) || o.from2 == Some(m.planning_id))
    })?;
    if folge.winner.is_some() || folge.status == MatchStatus::Finished {
        return Some(CorrectionBlocker::Decided);
    }
    if folge.status == MatchStatus::OnCourt {
        return Some(CorrectionBlocker::Running);
    }
    Some(CorrectionBlocker::Untested)
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
    officials: Option<(i64, i64)>,
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
                // Überschreiben nur, wo es nichts umzurechnen gibt. Was
                // dahinter liegt, entscheidet der Turnierbaum.
                match correction_blocker(snap, *match_id) {
                    None => {}
                    Some(CorrectionBlocker::Running) => {
                        return Err(TlResponse::err(
                            C::CorrectionBlocked,
                            "Der Sieger spielt bereits sein nächstes Spiel — eine Korrektur \
                             hier zöge ein laufendes Feld mit. Bitte in BTP von Hand.",
                        ))
                    }
                    Some(CorrectionBlocker::Decided) => {
                        return Err(TlResponse::err(
                            C::CorrectionBlocked,
                            "Das Folgespiel ist bereits gewertet — eine Korrektur hier machte \
                             aus einem gültigen Ergebnis ein Rätsel. Bitte in BTP von Hand.",
                        ))
                    }
                    Some(CorrectionBlocker::Untested) => {
                        // Der offene Punkt aus der Spec: Ein beendetes
                        // KO-Spiel wird selbst zum Feeder-Slot, der Sieger
                        // steht also sofort im nächsten Spiel. Ob BTP den
                        // Baum beim Überschreiben neu rechnet, hat noch
                        // niemand ausprobiert — und wer das nicht weiß, darf
                        // ihn nicht anfassen (docs/btp_protocol.md).
                        return Err(TlResponse::err(
                            C::CorrectionBlocked,
                            "Der Sieger steht schon im nächsten Spiel. Ob sich das hier \
                             gefahrlos ändern lässt, ist noch nicht geprüft — bitte in BTP \
                             von Hand.",
                        ));
                    }
                }
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
            let update = crate::tablet::server::build_manual_result_update_opt(
                m,
                pairs,
                on_court_since,
                now_ms,
                *overwrite,
                officials,
            )
            .map_err(|e| {
                // „Bereits gewertet" ohne Überschreib-Wunsch ist kein
                // Verbot, sondern ein Hinweis: Die Seite kann daraufhin
                // ausdrücklich fragen.
                let code = if m.winner.is_some() && !*overwrite {
                    C::AlreadyScored
                } else {
                    C::NotAllowed
                };
                TlResponse::err(code, e)
            })?;
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
    officials: &std::collections::HashMap<i64, (i64, i64)>,
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
            // Kampflose Spiele standen selten schon auf einem Feld, aber
            // falls doch (z. B. Aufgabe statt Disqualifikation gemeldet),
            // reasserted dieselbe Regel wie bei jedem Ergebnis-Write die
            // bekannte Besetzung mit (Live-Befund 14.08.2026).
            officials: officials.get(&c.match_id).copied(),
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
    officials: &std::collections::HashMap<i64, (i64, i64)>,
) -> Result<Vec<crate::btp::proto::MatchUpdate>, relay_proto::TlResponse> {
    let updates = walkover_updates(candidates, match_ids, officials);
    if updates.is_empty() {
        return Err(relay_proto::TlResponse::err(
            relay_proto::TlErrorCode::AlreadyHandled,
            "Keines der gewählten Spiele lässt sich noch kampflos werten — bitte die \
             Aufgabe erneut aufrufen.",
        ));
    }
    Ok(updates)
}

/// Läuft dieses Turnier überhaupt mit Schiedsrichtern? Jede
/// Officials-Aktion beginnt damit — sonst schriebe ein Gerät Zusatzdaten
/// (darunter Sperrlisten, also Personendaten) in die Turnierdatei eines
/// Turniers, das gar keine Schiedsrichter führt.
fn officials_an(tablet: &TabletState) -> Result<(), relay_proto::TlResponse> {
    if tablet.officials_store().enabled() {
        return Ok(());
    }
    Err(relay_proto::TlResponse::err(
        relay_proto::TlErrorCode::NotAllowed,
        "Dieses Turnier läuft ohne Schiedsrichter.",
    ))
}

/// Wire-Rolle in die Rolle des Roster-Speichers übersetzen.
fn tl_role(role: relay_proto::TlOfficialRole) -> crate::tablet::officials::OfficialRole {
    match role {
        relay_proto::TlOfficialRole::Sr => crate::tablet::officials::OfficialRole::Sr,
        relay_proto::TlOfficialRole::Ar => crate::tablet::officials::OfficialRole::Ar,
    }
}

/// Kurzform der Rolle für Fingerabdrücke.
fn role_key(role: relay_proto::TlOfficialRole) -> &'static str {
    match role {
        relay_proto::TlOfficialRole::Sr => "sr",
        relay_proto::TlOfficialRole::Ar => "ar",
    }
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
    view_rev: u64,
    action: relay_proto::TlAction,
) -> relay_proto::TlResponse {
    use relay_proto::{TlErrorCode as C, TlResponse};

    // Ins Protokoll gehört die **Kennung** des Geräts, nie sein Etikett: Das
    // kann „Tablet von Anna Meier" heißen, und die Protokolle werden zur
    // Fehlersuche hochgeladen. Dazu der Stand, auf dem die Entscheidung
    // beruhte — nach einer strittigen Aktion ist genau das die Frage.
    //
    // **Bewusst keine Schwellenprüfung auf `view_rev`.** Eine Grenze in
    // Revisionen wäre willkürlich: Sie steigt bei jeder Änderung, in einem
    // vollen Turnier also im Sekundentakt, in einer ruhigen Phase minutenlang
    // gar nicht — dieselbe Zahl bedeutete mal Sekunden, mal eine
    // Viertelstunde. Was ein veralteter Blick wirklich anrichten kann, fangen
    // die **fachlichen** Prüfungen ab, und die sind genauer: `expect` beim
    // Feld (das Spiel steht nicht mehr dort), der beanspruchte
    // Walkover-Vorschlag (ist weg → „schon bearbeitet"), die
    // Ergebnisprüfung (bereits gewertet).
    tracing::info!(
        "TL-Web [{}]: {} (Ansicht {view_rev})",
        device.id,
        action_label(&action)
    );

    // Wiederholung? Dann die gespeicherte Antwort, ohne erneut zu schreiben.
    // Ein Doppeltipp bei träger Verbindung schickt dieselbe Aktion zweimal;
    // ohne diese Prüfung landete sie zweimal in BTP. Der Fingerabdruck stellt
    // sicher, dass es wirklich dieselbe Aktion ist.
    let fingerprint = action_fingerprint(&action);
    if let Some(known) = ctx.tablet.remembered_result(op_id, &fingerprint, now_ms) {
        tracing::info!(
            "TL-Web [{}]: {} war schon erledigt (Wiederholung)",
            device.id,
            action_label(&action)
        );
        return known;
    }

    // Panel-Profile: eigener Weg wie Wertungen unten, weil sie `AppConfig`/
    // `config.json` ändern statt Turnier-Zustand — `apply_state_action`
    // bleibt auf reine `TabletState`-Änderungen ohne Datei-I/O beschränkt
    // (Spec tl-web-panelsystem, ADR 0025). **Bewusst VOR dem Snapshot-Gate
    // unten:** Profile sind turnierunabhängige, reine Layout-Einstellungen
    // (siehe `build_state_limited`, das `profiles`/`default_profile_id`
    // bereits ohne geladenes Turnier liefert) — ein TL, der vor dem ersten
    // BTP-Import schon einen Wandmonitor einrichtet, darf dafür nicht
    // fälschlich „kein Turnier geladen" sehen.
    if let Some(response) = execute_profile_action(ctx, device, now_ms, &action) {
        if response.ok {
            ctx.tablet
                .remember_result(op_id, &fingerprint, response.clone(), now_ms);
        }
        return response;
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
                    device.id,
                    action_label(&action)
                );
                ok
            }
            Err(rejected) => {
                tracing::info!(
                    "TL-Web [{}]: {} abgelehnt ({})",
                    device.id,
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
        Ok(mut plan) => {
            // Beim Ruf aufs Feld die Schiedsrichter-Besetzung mitschreiben
            // (ADR 0021). `plan_court_action` bleibt rein und kennt den
            // Roster nicht — deshalb hier, wo der Zustand vorliegt.
            for mc in &mut plan.match_courts {
                if mc.court_id == 0 {
                    continue; // Freigeben lässt die Besetzung unangetastet
                }
                mc.officials = snap
                    .matches
                    .iter()
                    .find(|m| m.id == mc.match_id)
                    .and_then(|m| ctx.tablet.officials_for_write(m));
            }
            plan
        }
        Err(response) => {
            // Auch Ablehnungen werden festgehalten — nur so lässt sich nach
            // dem Turnier zählen, wie oft sich zwei Geräte in die Quere
            // kamen. Der Zugang des Geräts taucht dabei nie auf.
            tracing::info!(
                "TL-Web [{}]: {} abgelehnt ({})",
                device.id,
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
                    device.id,
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
                device.id,
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
                device.id,
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
            let officials = ctx.tablet.officials_for_result(*match_id);
            match plan_result_action(&snap, on_court_since, now_ms, action, officials) {
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
            let officials: std::collections::HashMap<i64, (i64, i64)> = match_ids
                .iter()
                .filter_map(|id| ctx.tablet.officials_for_result(*id).map(|o| (*id, o)))
                .collect();
            let planned = match plan_walkover_action(
                &ctx.tablet.walkover_candidates(proposal.entry_id),
                match_ids,
                &officials,
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
                // Punktverlauf abschließen — auch der TL-Wertungsweg
                // beendet ein evtl. tablet-gezähltes Spiel; ohne das
                // bliebe `finished=false` und der Abweichungs-Hinweis
                // (AK-8) könnte gerade dort nie erscheinen, wo Verlauf
                // und Wertung auseinanderliegen (Review 2026-08-11).
                // ScoreStatus 2 = Aufgabe; ohne Aufzeichnung No-op.
                ctx.tablet
                    .timeline_store()
                    .finalize(update.btp_match_id, update.score_status == 2);
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
        device.id,
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
        A::SetHall { match_id, hall } => format!("hall:{match_id}:{hall}"),
        A::ExcludeFromAutoAssign { match_id, excluded } => format!("excl:{match_id}:{excluded}"),
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
        A::OfficialAssign {
            match_id,
            official_id,
            role,
            ..
        } => format!("off-assign:{match_id}:{}:{official_id}", role_key(*role)),
        A::OfficialClear { match_id, role, .. } => {
            format!("off-clear:{match_id}:{}", role_key(*role))
        }
        A::OfficialPause {
            official_id,
            paused,
        } => format!("off-pause:{official_id}:{paused}"),
        A::OfficialReorder {
            official_id,
            before_official_id,
        } => format!(
            "off-order:{official_id}:{}",
            before_official_id.unwrap_or(0)
        ),
        // Ohne Inhalt: Der Fingerabdruck landet im Protokoll, und
        // Vereinsnamen bzw. Sperrlisten haben dort nichts zu suchen.
        A::OfficialSetClub { official_id, .. } => format!("off-club:{official_id}"),
        A::OfficialBlocklistSet { official_id, .. } => format!("off-block:{official_id}"),
        A::OfficialsCourtToggle {
            court_id,
            sr,
            ar,
            operator,
        } => format!("off-court:{court_id}:{sr}:{ar}:{operator}"),
        A::AnnounceOfficials { court_id } => format!("off-announce:{court_id}"),
        A::QueueReorder {
            match_id,
            before_match_id,
        } => format!("queue-order:{match_id}:{}", before_match_id.unwrap_or(0)),
        A::QueueOrderReset => "queue-order-reset".to_string(),
        // Panel-Profile (Spec tl-web-panelsystem): der Fingerabdruck
        // beschreibt den ganzen Inhalt (wie `EnterResult`), damit ein
        // wiederverwendetes `op_id` zwei inhaltlich verschiedene Speicher-
        // vorgänge nicht fälschlich als „schon erledigt" behandelt.
        A::ProfileSave { profile } => format!(
            "profile-save:{}:{}:{}:{}{}{}{}{}{}{}:{:?}",
            profile.id,
            profile.name,
            profile
                .panels
                .iter()
                .map(|p| format!("{}:{}:{}", p.key, p.visible, p.height_fr))
                .collect::<Vec<_>>()
                .join(","),
            profile.display.show_numbers,
            profile.display.show_nations,
            profile.display.show_club_names,
            profile.display.show_club_logos,
            profile.display.show_discipline,
            profile.display.show_round,
            profile.display.show_group,
            profile.display.list_position,
        ),
        A::ProfileDelete { profile_id } => format!("profile-delete:{profile_id}"),
        A::ProfileSelect { profile_id } => format!("profile-select:{profile_id}"),
        A::ProfileSetDefault { profile_id } => format!("profile-default:{profile_id}"),
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
        A::SetHall { match_id, hall } => {
            if hall.trim().is_empty() {
                format!("Halle von Spiel {match_id} zurückgenommen")
            } else {
                format!("Spiel {match_id} nach {hall}")
            }
        }
        A::ExcludeFromAutoAssign { match_id, excluded } => format!(
            "Spiel {match_id} {} Auto-Vergabe",
            if *excluded { "aus" } else { "wieder in" }
        ),
        A::AnnounceCourtCall { court_id, .. } => format!("Erneuter Aufruf Feld {court_id}"),
        A::AnnouncePrepCall { .. } => "Erneuter Vorbereitungs-Aufruf".to_string(),
        A::EnterResult { match_id, .. } => format!("Ergebnis für Spiel {match_id}"),
        A::ConfirmWalkover { .. } => "Kampflose Wertung".to_string(),
        A::DismissWalkover { .. } => "Walkover-Vorschlag verwerfen".to_string(),
        A::ScorekeeperAdvance { .. } => "Zähltafelbediener vorziehen".to_string(),
        A::ScorekeeperRemove { .. } => "Zähltafelbediener entfernen".to_string(),
        A::ScorekeeperAdd { .. } => "Zähltafelbediener ergänzen".to_string(),
        A::OfficialAssign {
            court_id, match_id, ..
        } => format!("Schiedsrichter für Spiel {match_id} (Feld {court_id})"),
        A::OfficialClear { court_id, .. } => {
            format!("Schiedsrichter von Feld {court_id} gelöst")
        }
        A::OfficialPause { paused, .. } => {
            if *paused {
                "Schiedsrichter pausiert".to_string()
            } else {
                "Schiedsrichter wieder eingeteilt".to_string()
            }
        }
        A::OfficialReorder { .. } => "Schiedsrichter-Reihenfolge geändert".to_string(),
        // Bewusst ohne Inhalt (siehe `action_fingerprint`).
        A::OfficialSetClub { .. } => "Schiedsrichter-Verein gepflegt".to_string(),
        A::OfficialBlocklistSet { .. } => "Schiedsrichter-Sperren gepflegt".to_string(),
        A::OfficialsCourtToggle { court_id, .. } => {
            format!("Feld-Schalter von Feld {court_id}")
        }
        A::AnnounceOfficials { court_id } => format!("Schiedsrichter-Ansage Feld {court_id}"),
        A::QueueReorder { match_id, .. } => format!("Spielliste umsortiert (Spiel {match_id})"),
        A::QueueOrderReset => "Manuelle Spielreihenfolge zurückgesetzt".to_string(),
        A::SetAutoAssign { enabled } => {
            format!(
                "Automatische Vergabe {}",
                if *enabled { "an" } else { "aus" }
            )
        }
        A::ProfileSave { profile } => format!("Profil „{}“ gespeichert", profile.name),
        A::ProfileDelete { profile_id } => format!("Profil {profile_id} gelöscht"),
        A::ProfileSelect { profile_id } => format!("Profil {profile_id} gewählt"),
        A::ProfileSetDefault { profile_id } => {
            format!("Profil {profile_id} als Standard gesetzt")
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
    /// Verwaltet dieser Turnier-PC Zähltafelbediener? Nur dann zeigt die
    /// Seite den Warteschlangen-Abschnitt.
    pub scorekeeper_managed: bool,
    /// Die Warteschlange, in Reihenfolge. Der `key` ist die stabile
    /// Kennung für Vorziehen/Entfernen — dieselbe wie am Turnier-PC.
    pub scorekeepers: Vec<TlScorekeeper>,
    /// Zuletzt beendete Spiele, neueste zuerst — für die
    /// Ergebnis-Übersicht der Turnierleitung. Dieselbe Filterung, Sortierung
    /// und `result`-Abbildung wie `finished_matches` in commands.rs (die
    /// Desktop-Tabelle), damit beide Ansichten dasselbe erzählen.
    pub finished: Vec<TlFinished>,
    /// Raster-Anordnung je Halle (Host-Einstellung, `AppConfig.hall_layouts`).
    /// Hallen ohne Eintrag bekommen kein Element hier — die Seite zeigt sie
    /// dann in der bisherigen Fließ-Darstellung.
    pub layouts: Vec<TlHallLayout>,
    /// Turnierweite Anzeige-Optionen (`config.display`): Vereinsnamen
    /// und/oder -logos an den Spielernamen zeigen. Zentral gesetzt, damit
    /// alle TL-Bildschirme dasselbe Bild zeigen.
    pub show_club_names: bool,
    pub show_club_logos: bool,
    /// Läuft dieses Turnier mit Schiedsrichtern? Nur dann zeigt die Seite
    /// SR/AR-Elemente (Spec schiedsrichter-management Nr. 1).
    #[serde(default)]
    pub officials_managed: bool,
    /// Die Schiedsrichter in Rotationsreihenfolge.
    ///
    /// Bewusst **reduziert** wie `TlScorekeeper`: Name, Pause, Dienst,
    /// Einsatz-Zähler. Sperrlisten und Vereins-Angaben stehen **nicht**
    /// hier — sie sind Personendaten und kommen nur auf gezielte, per
    /// Geräte-Token authentifizierte Anfrage (`/tl/officials`).
    #[serde(default)]
    pub officials: Vec<TlOfficial>,
    /// Der Panel-Profil-Katalog (Spec tl-web-panelsystem, ADR 0025) —
    /// geteilt, klein, unkritisch, Muster `layouts_view`/`layouts`.
    /// Wiederverwendet direkt den `relay-proto`-Wire-Typ statt eines
    /// eigenen tl.rs-lokalen Structs: Derselbe `TlPanelProfileWire` reist
    /// auch als [`relay_proto::TlAction::ProfileSave`]-Payload, das
    /// Anlegen eines Duplikats brächte hier keinen Gewinn.
    #[serde(default)]
    pub profiles: Vec<relay_proto::TlPanelProfileWire>,
    /// Turnierweiter Standard, wenn ein Gerät kein eigenes Profil gewählt
    /// hat. Leer = eingebautes Standardprofil (tl.html kennt es).
    #[serde(default)]
    pub default_profile_id: String,
}

/// Ein Schiedsrichter im Turnierleitungs-Zustand.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlOfficial {
    pub id: i64,
    /// Anzeigename aus BTP — zweckgebunden wie die Spielernamen.
    pub name: String,
    pub paused: bool,
    /// Feld, auf dem er gerade Dienst tut (0 = frei).
    pub on_duty_court_id: i64,
    /// Zahl der bisherigen Einsätze, aus den beendeten Spielen abgeleitet.
    pub appearances: usize,
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
    /// Ab wann ein aufgerufenes Spiel ohne einen einzigen Punkt als
    /// überfällig gilt — die Schwelle für die auffällige Feldfarbe. Kommt
    /// vom Turnier-PC, damit alle Geräte dasselbe Feld rot sehen.
    pub not_started_minutes: f64,
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
    /// Disziplin als snake_case-Schlüssel (`mens_singles` …), wie überall
    /// sonst auf der Wire-Ebene. Die Seite setzt daraus mit `class_label`
    /// die gewohnte Klassenbezeichnung zusammen („HE-C") — ohne sie sagt
    /// eine Auslosung namens „Gruppe 6" nicht, worum es geht.
    pub discipline: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Nationen als ISO-Kürzel („GER"), **parallel** zu `team1`/`team2`;
    /// leerer String, wo BTP nichts führt.
    ///
    /// Standardmäßig blendet die Seite sie aus — eingeschaltet helfen sie
    /// bei internationalen Turnieren, die richtige Paarung ans Feld zu
    /// holen. Dieselbe Angabe zeigt der Court-Monitor als Flagge, dort
    /// sogar öffentlich; hier steht sie hinter dem Gerätezugang.
    pub team1_nat: Vec<String>,
    pub team2_nat: Vec<String>,
    /// Vereinsnamen, **parallel** zu `team1`/`team2`; leerer String, wo BTP
    /// keinen führt. Zuschaltbares Anzeige-Feld (turnierweit,
    /// `config.display`); dient zugleich als Schlüssel fürs Vereinslogo
    /// (`/info/club-logo?name=`). Datenschutz: bewusst freigegeben wie die
    /// Nation (Entscheidung 12.08.2026), Default aus.
    pub team1_club: Vec<String>,
    pub team2_club: Vec<String>,
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
    /// hochzählenden Uhr und der Fälligkeitsanzeige.
    pub on_court_since_ms: Option<u64>,
    /// Wie oft dieses Spiel schon aufgerufen wurde (1–3), **gezählt am
    /// Turnier-PC**. Nicht zu verwechseln mit der Fälligkeit aus der Uhr:
    /// Die sagt, wann der nächste Aufruf dran wäre, diese Zahl, wie viele
    /// erfolgt sind. Nur so zeigen zwei Turnierleitungen dieselbe Stufe.
    pub call_stage: u8,
    /// Zählformat, damit die Seite Satz- und Matchball anzeigen kann.
    pub best_of: i64,
    pub target_score: i64,
    pub cap_score: i64,
    /// Gibt es zu diesem Spiel einen Punktverlauf zum Anzeigen? Nur dann
    /// bietet die Seite den Graph-Klick an — kein „Klick ins Leere" bei
    /// Spielen ohne Tablet-Zählung. `default` hält ältere Gegenstellen
    /// kompatibel.
    #[serde(default)]
    pub has_timeline: bool,
    /// Schiedsrichter/Aufschlagrichter des laufenden Spiels; leer ohne
    /// Zuweisung oder ohne Schiedsrichter-Betrieb.
    #[serde(default)]
    pub sr: Vec<String>,
    #[serde(default)]
    pub ar: Vec<String>,
    /// Konflikt-Kategorie („Verein"/„Person"). **Nur die Kategorie** — der
    /// Grund (welcher Verein, welcher Spieler) verlässt den Turnier-PC nie.
    #[serde(default)]
    pub official_warn: Option<String>,
    /// IDs der wirksamen Besetzung (0 = keiner) — die Auswahl auf der Seite
    /// trifft damit die Person, nicht den Namen (Namensgleichheit kommt in
    /// großen Listen vor).
    #[serde(default)]
    pub sr_id: i64,
    #[serde(default)]
    pub ar_id: i64,
    /// Die drei Feld-Schalter, damit die Seite sie zeigen und setzen kann.
    #[serde(default)]
    pub rotate_sr: bool,
    #[serde(default)]
    pub rotate_ar: bool,
    #[serde(default)]
    pub assign_operator: bool,
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
    /// Siehe [`TlCourt::discipline`].
    pub discipline: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Nationen als ISO-Kürzel, **parallel** zu den Namen (leerer String =
    /// keine Angabe). Siehe [`TlCourt::team1_nat`].
    pub team1_nat: Vec<String>,
    pub team2_nat: Vec<String>,
    /// Vereinsnamen, **parallel** zu den Namen. Siehe [`TlCourt::team1_club`].
    pub team1_club: Vec<String>,
    pub team2_club: Vec<String>,
    /// In welche Halle das Spiel gehört, und woher wir das wissen.
    pub hall: String,
    pub hall_source: HallSource,
    /// Bereits in die Vorbereitung gerufen?
    pub prep_call: Option<TlPrepCall>,
    /// Warum das Spiel gerade nicht aufs Feld kann; `None` = spielbereit.
    pub blocked: Option<TlBlocked>,
    /// Von der Turnierleitung von der automatischen Feldvergabe ausgenommen
    /// (Spec `feldvergabe-ausnahme`)? Manuelles Zuweisen bleibt davon
    /// unberührt — reine Anzeige-Information für das Badge in der Liste.
    #[serde(default)]
    pub excluded_from_auto_assign: bool,
    /// Steht dieses Spiel gerade im manuellen Präfix seiner Halle (Spec
    /// `spielliste-manuelle-reihenfolge`)? Reine Anzeige-Information fürs
    /// Badge in der Liste — die tatsächliche Sortierung liegt bereits in
    /// der Reihenfolge dieser Liste selbst.
    #[serde(default)]
    pub manual: bool,
}

/// Eine Halle des Turniers.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlHall {
    /// BTP-Kennung des Standorts — nötig für den Vorbereitungs-Aufruf.
    pub id: i64,
    pub name: String,
}

/// Raster-Anordnung der Felder einer Halle, wie sie am Turnier-PC hinterlegt
/// ist ([`crate::config::HallLayoutConfig`]). Reiner Datentransport — keine
/// Personendaten, deshalb ohne weitere Prüfung erlaubt.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlHallLayout {
    pub hall: String,
    pub columns: u8,
    /// snake_case-String derselben vier Werte wie
    /// [`crate::config::LayoutOrigin`] — die Seite kennt keine Rust-Enums.
    pub origin: String,
    pub serpentine: bool,
    /// Spaltenweise statt reihenweise nummerieren.
    pub vertical: bool,
}

/// Ein Wartender in der Zähltafelbediener-Warteschlange, wie er auf der
/// Turnierleitungs-Seite erscheint. Siehe [`crate::tablet::state::ScorekeeperEntry`]
/// für die Quelle — hier fehlt bewusst `from_court_id`, die Seite braucht
/// nur, wer wartet und seit wann.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlScorekeeper {
    pub key: String,
    pub names: Vec<String>,
    pub enqueued_ms: u64,
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
    /// Höchste bisher gesprochene Nachruf-Stufe (0 = noch keiner). Die Seite
    /// unterscheidet damit einen Doppeltipp von einem bewussten zweiten
    /// Nachruf.
    pub recalls: u8,
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
pub(crate) const QUEUE_LIMIT_PER_HALL: usize = 120;

/// Höchstzahl beendeter Spiele im Zustand. Die Seite ist ein
/// Arbeits-Werkzeug, kein Archiv — wer mehr braucht, schaut in BTP.
const FINISHED_LIMIT: usize = 30;

/// Ein beendetes Spiel, wie es die Ergebnis-Übersicht braucht. Felder und
/// `result`-Werte spiegeln [`crate::commands::FinishedMatchRow`] (die
/// Desktop-Tabelle) — bewusst dieselbe Bedeutung, damit Turnierleitung am
/// Tablet und am Turnier-PC dasselbe Ergebnis lesen.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlFinished {
    pub match_id: i64,
    pub match_num: i64,
    pub draw_name: String,
    pub round_name: String,
    pub class_label: String,
    /// Siehe [`TlCourt::discipline`].
    pub discipline: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// 1 oder 2 — wer gewonnen hat.
    pub winner: u8,
    pub sets: Vec<(i64, i64)>,
    /// `normal` | `walkover` | `retired` | `disqualified` — die Seite
    /// kennzeichnet alles außer `normal` mit einem Abzeichen, sonst sähe
    /// ein Teil-Spielstand (14:16, 15:10) wie ein Fehler aus.
    pub result: String,
    /// Feld, auf dem es lief; leer, wenn direkt in BTP gewertet.
    pub court: String,
    /// Nur zur Laufzeit gestempelt — Spiele, die vor dem App-Start
    /// beendet waren, haben keinen Zeitstempel und stehen am Ende.
    pub finished_at_ms: Option<u64>,
    /// Siehe [`TlCourt::has_timeline`] — Papier-Ergebnisse haben keinen
    /// Verlauf, die Beendet-Zeile bietet den Klick dann nicht an.
    #[serde(default)]
    pub has_timeline: bool,
}

/// Ordnungsschlüssel eines wartenden Spiels samt dem Spiel selbst und seiner
/// Halle — die Zwischenform, in der sortiert und gekappt wird, bevor die
/// teuren Zeichenketten der Anzeige entstehen.
type OrderedMatch<'a> = (
    assign::ManualOrderSortKey,
    &'a crate::btp::model::BtpMatch,
    String,
    HallSource,
);

/// Baut den Anzeige-Zustand aus dem aktuellen BTP-Stand und dem, was der Host
/// selbst verwaltet (Aufrufe, Sperren, Live-Spielstände).
///
/// `rev` gibt der Aufrufer vor — er entscheidet, ob sich gegenüber dem
/// zuletzt ausgelieferten Stand überhaupt etwas geändert hat.
pub fn build_state(tablet: &TabletState, config: &AppConfig, now_ms: u64, rev: u64) -> TlState {
    build_state_limited(tablet, config, now_ms, rev, QUEUE_LIMIT_PER_HALL)
}

/// Wie [`build_state`], aber mit vorgegebener Wartelisten-Länge je Halle.
///
/// Für den Weg über den Relay: Der legt einen zu großen Zustand gar nicht
/// erst ab, und der Host erfährt davon nichts. Er muss also selbst kürzen —
/// was wegfällt, meldet `truncated_halls` wie immer.
pub(crate) fn build_state_limited(
    tablet: &TabletState,
    config: &AppConfig,
    now_ms: u64,
    rev: u64,
    queue_limit: usize,
) -> TlState {
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
            scorekeeper_managed: config.scorekeeper.enabled,
            scorekeepers: Vec::new(),
            finished: Vec::new(),
            // Die Raster-Einstellung ist Host-Konfiguration, kein
            // Turnierstand — sie gilt auch, solange BTP noch nichts
            // geliefert hat.
            layouts: layouts_view(config),
            show_club_names: config.display.show_club_names,
            show_club_logos: config.display.show_club_logos,
            officials_managed: tablet.officials_store().enabled(),
            officials: Vec::new(),
            // Der Profil-Katalog ist Host-Konfiguration, kein Turnierstand
            // — er gilt auch, solange BTP noch nichts geliefert hat
            // (dieselbe Begründung wie bei `layouts` oben).
            profiles: profiles_view(config),
            default_profile_id: config.tl_web.default_profile_id.clone(),
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
            let schalter = tablet.officials_store().court_switches(c.court_id);
            court_view(c, clearing, schalter)
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

    // Die von Hand gesetzten Hallen einmal holen, nicht je Spiel — sonst
    // sperrte der Aufbau der Liste hundertfach.
    let manual = tablet.manual_halls();

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
            let manual_hall = manual.get(&m.id).map(String::as_str);
            let called_hall_str = call.as_ref().map(|(h, _)| h.as_str());
            let (hall, hall_source, key) = assign::resolve_and_sort_key(
                config,
                &snap,
                m,
                manual_hall,
                called_hall_str,
                call.is_some(),
                tablet.queue_order_store(),
            );
            (key, m, hall, hall_source)
        })
        .collect();
    ordered.sort_by_key(|(key, _, _, _)| *key);

    // Je Halle kappen, nicht über das ganze Turnier.
    let mut per_hall: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut truncated: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue: Vec<TlMatch> = Vec::new();
    for (_, m, hall, hall_source) in ordered {
        let count = per_hall.entry(hall.clone()).or_insert(0);
        if *count >= queue_limit {
            truncated.insert(hall);
            continue;
        }
        *count += 1;
        let call = called_hall(m.id);
        let manually_ordered = tablet.queue_order_store().rank(&hall, m.id).is_some();
        queue.push(TlMatch {
            match_id: m.id,
            match_num: m.match_num,
            planned_time: m.planned_time,
            draw_name: m.draw_name.clone(),
            round_name: m.round_name.clone(),
            class_label: m.class_label.clone(),
            discipline: m.discipline.as_str().to_string(),
            team1: m.team1.iter().map(|p| p.name.clone()).collect(),
            team2: m.team2.iter().map(|p| p.name.clone()).collect(),
            team1_nat: m
                .team1
                .iter()
                .map(|p| p.nationality.clone().unwrap_or_default())
                .collect(),
            team2_nat: m
                .team2
                .iter()
                .map(|p| p.nationality.clone().unwrap_or_default())
                .collect(),
            team1_club: m
                .team1
                .iter()
                .map(|p| p.club.clone().unwrap_or_default())
                .collect(),
            team2_club: m
                .team2
                .iter()
                .map(|p| p.club.clone().unwrap_or_default())
                .collect(),
            hall,
            hall_source,
            prep_call: call.map(|(hall, called_at_ms)| TlPrepCall {
                hall,
                called_at_ms,
                recalls: tablet.prep_calls_made(m.id),
            }),
            blocked: availability.blocked(m, now_ms).map(TlBlocked::from),
            excluded_from_auto_assign: tablet.auto_assign_excluded(m.id),
            manual: manually_ordered,
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

    // Nur wenn diese Installation Zähltafelbediener verwaltet, geht die
    // Warteschlange überhaupt raus — sonst zeigte ein Gerät einen
    // Abschnitt, für den es gar keine Bedienhandlung gibt.
    let scorekeeper_managed = config.scorekeeper.enabled;
    let scorekeepers = if scorekeeper_managed {
        tablet
            .scorekeeper_queue()
            .into_iter()
            .map(|e| TlScorekeeper {
                key: e.key,
                names: e.names,
                enqueued_ms: e.enqueued_ms,
            })
            .collect()
    } else {
        Vec::new()
    };

    // Schiedsrichter: nur bei eingeschaltetem Betrieb, in
    // Rotationsreihenfolge, ohne Sperrlisten (Personendaten — die kommen
    // über die gezielte Leseroute).
    let officials_managed = tablet.officials_store().enabled();
    let officials = if officials_managed {
        officials_view(tablet, &snap)
    } else {
        Vec::new()
    };

    // Beendete Spiele: Filter, Sortierung und `result`-Abbildung wie
    // `finished_matches` in commands.rs (Desktop-Tabelle) — dort erprobt,
    // hier übernommen statt neu erfunden.
    //
    // Das Limit folgt dem Warteschlangen-Limit nach unten (Relay-Stufen
    // 40/20/10/5): Ein kleines `queue_limit` heißt, der Zustand muss sowieso
    // klein bleiben — dann darf die Ergebnisliste nicht der größte Brocken
    // sein. `max(5)` verhindert, dass die unterste Stufe die Liste ganz
    // leerräumt.
    let finished_limit = FINISHED_LIMIT.min(queue_limit.max(5));
    let mut finished_matches: Vec<&crate::btp::model::BtpMatch> = snap
        .matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::Finished && m.winner.is_some())
        .collect();
    // Neueste zuerst. `unwrap_or(0)` zieht Spiele ohne Zeitstempel (vor
    // App-Start beendet) explizit ans Ende statt an den Anfang — wie in
    // `finished_matches`.
    finished_matches.sort_by(|a, b| {
        b.finished_at
            .unwrap_or(0)
            .cmp(&a.finished_at.unwrap_or(0))
            .then(b.match_num.unwrap_or(0).cmp(&a.match_num.unwrap_or(0)))
            .then(b.id.cmp(&a.id))
    });
    let finished: Vec<TlFinished> = finished_matches
        .into_iter()
        .take(finished_limit)
        .map(|m| TlFinished {
            match_id: m.id,
            match_num: m.match_num.unwrap_or(0),
            draw_name: m.draw_name.clone(),
            round_name: m.round_name.clone(),
            class_label: m.class_label.clone(),
            discipline: m.discipline.as_str().to_string(),
            team1: m.team1.iter().map(|p| p.name.clone()).collect(),
            team2: m.team2.iter().map(|p| p.name.clone()).collect(),
            winner: m.winner.unwrap_or(0),
            sets: m.sets.clone(),
            result: match m.result {
                crate::btp::model::MatchResult::Normal => "normal",
                crate::btp::model::MatchResult::Walkover => "walkover",
                crate::btp::model::MatchResult::Retired => "retired",
                crate::btp::model::MatchResult::Disqualified => "disqualified",
            }
            .to_string(),
            court: m.court.clone().unwrap_or_default(),
            finished_at_ms: m.finished_at,
            has_timeline: tablet.timeline_store().has_timeline(m.id),
        })
        .collect();

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
        officials_managed,
        officials,
        scorekeeper_managed,
        scorekeepers,
        finished,
        layouts: layouts_view(config),
        show_club_names: config.display.show_club_names,
        show_club_logos: config.display.show_club_logos,
        profiles: profiles_view(config),
        default_profile_id: config.tl_web.default_profile_id.clone(),
    }
}

/// Übersetzt die Host-Konfiguration je Halle in die Wire-Form — `origin` als
/// snake_case-String statt Rust-Enum, damit tl.html es ohne Zusatzwissen
/// lesen kann.
fn layouts_view(config: &AppConfig) -> Vec<TlHallLayout> {
    config
        .hall_layouts
        .iter()
        .map(|l| TlHallLayout {
            hall: l.hall.clone(),
            columns: l.columns,
            origin: match l.origin {
                crate::config::LayoutOrigin::BottomLeft => "bottom_left",
                crate::config::LayoutOrigin::BottomRight => "bottom_right",
                crate::config::LayoutOrigin::TopLeft => "top_left",
                crate::config::LayoutOrigin::TopRight => "top_right",
            }
            .to_string(),
            serpentine: l.serpentine,
            vertical: l.vertical,
        })
        .collect()
}

/// Übersetzt den Profil-Katalog aus der Host-Konfiguration in die Wire-Form
/// (Spec tl-web-panelsystem, ADR 0025) — Muster `layouts_view`.
fn profiles_view(config: &AppConfig) -> Vec<relay_proto::TlPanelProfileWire> {
    config.tl_web.profiles.iter().map(profile_to_wire).collect()
}

/// Ein einzelnes Profil aus der Host-Konfiguration in die Wire-Form.
fn profile_to_wire(p: &crate::config::TlPanelProfile) -> relay_proto::TlPanelProfileWire {
    relay_proto::TlPanelProfileWire {
        id: p.id.clone(),
        name: p.name.clone(),
        panels: p
            .panels
            .iter()
            .map(|s| relay_proto::TlPanelSettingWire {
                key: s.key.clone(),
                visible: s.visible,
                height_fr: s.height_fr,
            })
            .collect(),
        display: relay_proto::TlDisplaySettingsWire {
            show_numbers: p.display.show_numbers,
            show_nations: p.display.show_nations,
            show_club_names: p.display.show_club_names,
            show_club_logos: p.display.show_club_logos,
            show_discipline: p.display.show_discipline,
            show_round: p.display.show_round,
            show_group: p.display.show_group,
            list_position: match p.display.list_position {
                crate::config::TlListPosition::Right => relay_proto::TlListPositionWire::Right,
                crate::config::TlListPosition::Bottom => relay_proto::TlListPositionWire::Bottom,
            },
        },
        updated_at_ms: p.updated_at_ms,
    }
}

/// Die Umkehrung von [`profile_to_wire`]: ein von `tl.html` gesendetes
/// [`relay_proto::TlPanelProfileWire`] (`TlAction::ProfileSave`-Payload) in
/// die Host-Konfiguration übersetzen. `id`/`updated_at_ms` werden bewusst
/// NICHT übernommen — die Kennung entscheidet `profile_save` (Upsert/neu
/// vergeben), der Zeitstempel kommt immer vom Host (Last-Write-Wins-Marker).
fn panels_from_wire(
    panels: &[relay_proto::TlPanelSettingWire],
) -> Vec<crate::config::TlPanelSetting> {
    panels
        .iter()
        .map(|s| crate::config::TlPanelSetting {
            key: s.key.clone(),
            visible: s.visible,
            height_fr: s.height_fr,
        })
        .collect()
}

/// Siehe [`panels_from_wire`] — dasselbe für die Anzeige-Optionen.
fn display_settings_from_wire(
    d: &relay_proto::TlDisplaySettingsWire,
) -> crate::config::TlDisplaySettings {
    crate::config::TlDisplaySettings {
        show_numbers: d.show_numbers,
        show_nations: d.show_nations,
        show_club_names: d.show_club_names,
        show_club_logos: d.show_club_logos,
        show_discipline: d.show_discipline,
        show_round: d.show_round,
        show_group: d.show_group,
        list_position: match d.list_position {
            relay_proto::TlListPositionWire::Right => crate::config::TlListPosition::Right,
            relay_proto::TlListPositionWire::Bottom => crate::config::TlListPosition::Bottom,
        },
    }
}

/// Höchstlänge eines Profilnamens. Wie beim Geräte-Label (`tl_device_add`)
/// gekappt statt abgelehnt — ein Anzeigefeld soll nicht mit einem Fehler
/// antworten, nur weil jemand sehr viel tippt.
const MAX_TL_PROFILE_NAME_LEN: usize = 60;

/// Höchstzahl der Panel-Einträge in EINEM Profil. Das Frontend kennt neun
/// feste Schlüssel; die Reserve fängt künftige Panels ab, ohne dass ein
/// einzelner Aufruf den `TlState` sprengen kann (R4).
const MAX_TL_PROFILE_PANELS: usize = 32;

/// Legt ein Panel-Profil an oder überschreibt es (Upsert nach `id`; Spec
/// tl-web-panelsystem). Last-Write-Wins ohne Konfliktprüfung — die Spec
/// verlangt ausdrücklich keine Fehlermeldung bei gleichzeitiger Bearbeitung
/// durch zwei Geräte, deshalb wird hier NICHT gegen ein zuvor gesehenes
/// `updated_at_ms` geprüft. `updated_at_ms` stempelt immer der Host (`now_ms`),
/// nie der Client — sonst könnte eine falsch gehende Client-Uhr eine neuere
/// Änderung verdrängen.
///
/// Rein & testbar: kein Netz, kein `ServerCtx` — die Persistenz übernimmt
/// der Aufrufer (`execute_profile_action`).
fn profile_save(
    config: &mut AppConfig,
    profile: &relay_proto::TlPanelProfileWire,
    now_ms: u64,
) -> Result<(), relay_proto::TlResponse> {
    use relay_proto::{TlErrorCode as C, TlResponse};

    let name = profile.name.trim();
    if name.is_empty() {
        return Err(TlResponse::err(C::NotAllowed, "Profilname fehlt."));
    }
    // Der Katalog reist VOLLSTÄNDIG in jedem `TlState` mit (R4,
    // `MAX_TL_STATE_LEN`). Ohne serverseitige Grenzen könnte ein einzelnes
    // Gerät den Zustand über das Limit treiben und damit die Oberfläche
    // ALLER Geräte lahmlegen — das `maxlength` im Browser ist über einen
    // direkten Aufruf der Kommando-Route umgehbar.
    // Name: kappen statt ablehnen (Muster `tl_device_add`-Label — bei einem
    // Anzeigefeld ist stilles Kürzen weniger überraschend als ein Fehler).
    let name: String = name.chars().take(MAX_TL_PROFILE_NAME_LEN).collect();
    let name = name.as_str();
    // Panels: ablehnen. Die Panel-Liste ist keine Freitext-Eingabe, sondern
    // eine feste, im Frontend bekannte Menge (neun Schlüssel) — eine
    // überlange Liste ist ein Protokollfehler, kein Bedienfall.
    if profile.panels.len() > MAX_TL_PROFILE_PANELS {
        return Err(TlResponse::err(
            C::NotAllowed,
            format!("Ein Profil kann höchstens {MAX_TL_PROFILE_PANELS} Panels führen."),
        ));
    }
    let incoming_id = profile.id.trim();
    let exists =
        !incoming_id.is_empty() && config.tl_web.profiles.iter().any(|p| p.id == incoming_id);
    // Kappung nur für ECHT neue Profile — ein Update eines bestehenden darf
    // nicht scheitern, nur weil der Katalog voll ist (sonst könnte ein
    // Profil, das schon existiert, plötzlich nicht mehr gespeichert werden).
    if !exists && config.tl_web.profiles.len() >= relay_proto::MAX_TL_PROFILES {
        return Err(TlResponse::err(
            C::NotAllowed,
            format!(
                "Mehr als {} Profile sind nicht möglich — bitte zuerst eines löschen.",
                relay_proto::MAX_TL_PROFILES
            ),
        ));
    }
    // Neu UND ohne mitgeschickte Kennung: Der Host vergibt eine — Muster
    // `TabletState::add_scorekeeper_manual` (Zeit + laufender Index statt
    // einer neuen Abhängigkeit).
    let id = if incoming_id.is_empty() {
        format!("profile-{now_ms}-{}", config.tl_web.profiles.len())
    } else {
        incoming_id.to_string()
    };
    let saved = crate::config::TlPanelProfile {
        id: id.clone(),
        name: name.to_string(),
        panels: panels_from_wire(&profile.panels),
        display: display_settings_from_wire(&profile.display),
        updated_at_ms: now_ms,
    };
    if let Some(slot) = config.tl_web.profiles.iter_mut().find(|p| p.id == id) {
        *slot = saved;
    } else {
        config.tl_web.profiles.push(saved);
    }
    Ok(())
}

/// Entfernt ein Panel-Profil. Geräte, die es trugen, fallen auf das
/// Standardprofil zurück (leere `profile_id`) statt in einen Fehlerzustand
/// zu laufen (Spec tl-web-panelsystem, Grill-Punkt 7). Das Löschen eines
/// bereits verschwundenen Profils ist ein No-Op — Löschen ist idempotent,
/// kein Fehler.
fn profile_delete(config: &mut AppConfig, profile_id: &str) {
    config.tl_web.profiles.retain(|p| p.id != profile_id);
    for d in &mut config.tl_web.devices {
        if d.profile_id == profile_id {
            d.profile_id.clear();
        }
    }
    if config.tl_web.default_profile_id == profile_id {
        config.tl_web.default_profile_id.clear();
    }
}

/// Wählt für das AUFRUFENDE Gerät ein Profil. `device_id` kommt aus der
/// Bearer-Token-Authentifizierung, NIE aus einem Client-Feld — das ist die
/// Sicherheitsgrenze: Ein Gerät darf nur sich selbst binden, nie ein
/// anderes umbiegen. Leere `profile_id` ("Standard") ist immer gültig; jede
/// andere muss im Katalog existieren.
fn profile_select(
    config: &mut AppConfig,
    device_id: &str,
    profile_id: &str,
) -> Result<(), relay_proto::TlResponse> {
    use relay_proto::{TlErrorCode as C, TlResponse};
    if !profile_id.is_empty() && !config.tl_web.profiles.iter().any(|p| p.id == profile_id) {
        return Err(TlResponse::err(
            C::NotAllowed,
            "Dieses Profil gibt es nicht (mehr).",
        ));
    }
    if let Some(d) = config.tl_web.devices.iter_mut().find(|d| d.id == device_id) {
        d.profile_id = profile_id.to_string();
    }
    Ok(())
}

/// Setzt das turnierweite Standardprofil. Leer ("eingebautes
/// Standardprofil") ist immer gültig; jede andere Kennung muss im Katalog
/// existieren.
fn profile_set_default(
    config: &mut AppConfig,
    profile_id: &str,
) -> Result<(), relay_proto::TlResponse> {
    use relay_proto::{TlErrorCode as C, TlResponse};
    if !profile_id.is_empty() && !config.tl_web.profiles.iter().any(|p| p.id == profile_id) {
        return Err(TlResponse::err(
            C::NotAllowed,
            "Dieses Profil gibt es nicht (mehr).",
        ));
    }
    config.tl_web.default_profile_id = profile_id.to_string();
    Ok(())
}

/// Panel-Profile pflegen (Spec tl-web-panelsystem, ADR 0025): Diese vier
/// Aktionen ändern `AppConfig`/`config.json`, nicht den Turnier-Zustand in
/// `TabletState` — deshalb ein eigener Zweig wie `execute_result_action`,
/// statt sie durch `apply_state_action` laufen zu lassen (die bleibt auf
/// reine `TabletState`-Änderungen ohne Datei-I/O beschränkt).
///
/// `None` heißt: keine Profil-Aktion, an anderer Stelle weiterbehandeln.
fn execute_profile_action(
    ctx: &crate::tablet::server::ServerCtx,
    device: &crate::config::TlDevice,
    now_ms: u64,
    action: &relay_proto::TlAction,
) -> Option<relay_proto::TlResponse> {
    use relay_proto::{TlAction as A, TlResponse};

    let ok_or = |result: Result<(), TlResponse>| match result {
        Ok(()) => TlResponse::ok(0),
        Err(response) => response,
    };

    match action {
        A::ProfileSave { profile } => {
            Some(ok_or(ctx.mutate_app_config(|config| {
                profile_save(config, profile, now_ms)
            })))
        }
        A::ProfileDelete { profile_id } => Some(ok_or(ctx.mutate_app_config(|config| {
            profile_delete(config, profile_id);
            Ok(())
        }))),
        A::ProfileSelect { profile_id } => {
            Some(ok_or(ctx.mutate_app_config(|config| {
                profile_select(config, &device.id, profile_id)
            })))
        }
        A::ProfileSetDefault { profile_id } => {
            Some(ok_or(ctx.mutate_app_config(|config| {
                profile_set_default(config, profile_id)
            })))
        }
        _ => None,
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
        not_started_minutes: config.call_timer.not_started_minutes,
    }
}

/// Sperrlisten, Stammverein und Einsatz-Liste **eines** Schiedsrichters als
/// JSON — die Antwort der gezielten Leseroute (`/tl/api/officials/{id}`).
///
/// Bewusst getrennt vom Broadcast-Zustand: Diese Angaben kodieren
/// persönliche Beziehungen und sollen nur dort landen, wo gerade jemand sie
/// pflegt — nicht auf jedem gekoppelten Gerät. Ein unbekannter Official
/// liefert leere Listen statt eines Fehlers, damit die Seite den Dialog
/// trotzdem öffnen kann.
pub(crate) fn official_detail_json(
    tablet: &crate::tablet::state::TabletState,
    official_id: i64,
) -> String {
    use crate::btp::model::MatchStatus;
    let store = tablet.officials_store();
    let extra = store.extra(official_id);
    let snap = tablet.snapshot_clone();
    let einsaetze = snap
        .as_ref()
        .map(|snap| {
            let beendet: Vec<crate::tablet::officials::FinishedMatch> = snap
                .matches
                .iter()
                .filter(|m| m.status == MatchStatus::Finished)
                .map(|m| crate::tablet::officials::FinishedMatch {
                    match_id: m.id,
                    btp_sr: m.official1_id,
                    btp_ar: m.official2_id,
                    court_id: m.court_id,
                    finished_at: m.finished_at,
                })
                .collect();
            store
                .appearances(&beendet)
                .remove(&official_id)
                .unwrap_or_default()
                .into_iter()
                .map(|a| {
                    let m = snap.matches.iter().find(|m| m.id == a.match_id);
                    serde_json::json!({
                        "match_id": a.match_id,
                        "role": match a.role {
                            crate::tablet::officials::OfficialRole::Sr => "sr",
                            crate::tablet::officials::OfficialRole::Ar => "ar",
                        },
                        "match_name": m
                            .map(|m| format!("{} {}", m.draw_name, m.round_name).trim().to_string())
                            .unwrap_or_default(),
                        "court": a
                            .court_id
                            .and_then(|c| snap.court_infos.iter().find(|ci| ci.id == c))
                            .map(|ci| ci.name.clone())
                            .unwrap_or_default(),
                        "finished_at": a.finished_at,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // Auswahllisten für die Pflege (Spieler + Vereine des Turniers). Sie
    // reisen mit dieser gezielten Antwort, nicht im Broadcast-Zustand: Die
    // Meldeliste ist deutlich größer als alles, was die Seite sonst
    // bekommt, und sie wird nur beim bewussten Öffnen des Dialogs
    // gebraucht.
    let (pick_players, pick_clubs) = snap
        .as_ref()
        .map(|snap| crate::tablet::officials::pick_lists(&snap.entries))
        .unwrap_or_default();
    serde_json::json!({
        "official_id": official_id,
        "club": extra.club,
        "blocked_clubs": extra.blocked_clubs,
        "blocked_players": extra.blocked_players,
        "appearances": einsaetze,
        "pick_players": pick_players,
        "pick_clubs": pick_clubs,
    })
    .to_string()
}

/// Die Schiedsrichter in Rotationsreihenfolge — reduziert auf das, was die
/// Turnierleitungs-Seite zum Einteilen braucht. Sperrlisten und Verein
/// bleiben bewusst draußen (Personendaten, Wächter-Test).
fn officials_view(
    tablet: &crate::tablet::state::TabletState,
    snap: &crate::btp::model::BtpSnapshot,
) -> Vec<TlOfficial> {
    use crate::btp::model::MatchStatus;
    let store = tablet.officials_store();
    let einsaetze = store.appearances(
        &snap
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::Finished)
            .map(|m| crate::tablet::officials::FinishedMatch {
                match_id: m.id,
                btp_sr: m.official1_id,
                btp_ar: m.official2_id,
                court_id: m.court_id,
                finished_at: m.finished_at,
            })
            .collect::<Vec<_>>(),
    );
    let mut dienst: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for m in snap
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::OnCourt)
    {
        let Some(court_id) = m.court_id else { continue };
        let w = store.effective(m.id, m.official1_id, m.official2_id);
        for id in [w.sr, w.ar].into_iter().flatten() {
            dienst.insert(id, court_id);
        }
    }
    let order = store.order();
    let mut out: Vec<(usize, TlOfficial)> = snap
        .officials
        .iter()
        .map(|o| {
            let pos = order
                .iter()
                .position(|id| *id == o.id)
                .unwrap_or(usize::MAX);
            (
                pos,
                TlOfficial {
                    id: o.id,
                    name: o.display_name(),
                    paused: store.extra(o.id).paused,
                    on_duty_court_id: dienst.get(&o.id).copied().unwrap_or(0),
                    appearances: einsaetze.get(&o.id).map(Vec::len).unwrap_or(0),
                },
            )
        })
        .collect();
    out.sort_by_key(|(pos, o)| (*pos, o.id));
    out.into_iter().map(|(_, o)| o).collect()
}

/// Beschneidet die Feld-Übersicht auf das, was die Turnierleitung braucht.
///
/// Bewusst **weggelassen**: Nationalitäten (nur für die Sprachwahl der
/// Ansage, und diese Seite spricht nicht), Akkustand (keine Geräte-Übersicht
/// in diesem Feature) und die Aufschlag-Anzeige (Zählhilfe, keine
/// Vergabehilfe).
fn court_view(
    c: crate::tablet::state::CourtOverview,
    clearing: Option<i64>,
    schalter: crate::tablet::officials::CourtSwitches,
) -> TlCourt {
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
        call_stage: c.call_stage,
        court_id: c.court_id,
        court: c.court,
        location: c.location,
        match_id: c.match_id,
        match_name: c.match_name,
        round_name: c.round_name,
        class_label: c.class_label,
        discipline: c.discipline.as_str().to_string(),
        team1: c.team1,
        team2: c.team2,
        team1_nat: c.team1_nationalities,
        team2_nat: c.team2_nationalities,
        team1_club: c.team1_clubs,
        team2_club: c.team2_clubs,
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
        has_timeline: c.has_timeline,
        sr: c.sr,
        ar: c.ar,
        official_warn: c.official_warn,
        sr_id: c.sr_id,
        ar_id: c.ar_id,
        rotate_sr: schalter.sr,
        rotate_ar: schalter.ar,
        assign_operator: schalter.operator,
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
            display_order: None,
            from1: None,
            from2: None,
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
            officials: Vec::new(),
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
    fn the_state_carries_the_hall_layouts() {
        // Host-Einstellung je Halle (Task 9) muss unverändert in den
        // Anzeige-Zustand durchgereicht werden — sonst könnte die Seite kein
        // Raster zeigen, obwohl eines konfiguriert ist.
        let mut config = AppConfig::default();
        config.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "Halle 1".into(),
            columns: 3,
            origin: crate::config::LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: true,
        });
        let s = state_with(snap(Vec::new(), Vec::new(), Vec::new()), &config);
        assert_eq!(s.layouts.len(), 1);
        assert_eq!(s.layouts[0].hall, "Halle 1");
        assert_eq!(s.layouts[0].columns, 3);
        assert_eq!(s.layouts[0].origin, "bottom_left");
        assert!(!s.layouts[0].serpentine);
        // Nummerierungsrichtung (vertikal) muss ebenso durchgereicht werden.
        assert!(s.layouts[0].vertical);
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
    fn finished_matches_appear_newest_first_and_are_capped() {
        // Sechs beendete Spiele mit fallendem Zeitstempel (600 .. 100), eines
        // ganz ohne — beendet, bevor bts-light lief. Wie bei der
        // Desktop-Liste (`finished_matches` in commands.rs) sollen die mit
        // Zeitstempel neueste zuerst kommen und das ohne ans Ende rutschen.
        let mut m1 = a_match(1);
        m1.status = MatchStatus::Finished;
        m1.winner = Some(1);
        m1.sets = vec![(21, 15), (21, 18)];
        m1.finished_at = Some(600);

        let mut m2 = a_match(2);
        m2.status = MatchStatus::Finished;
        m2.winner = Some(2);
        m2.finished_at = Some(500);

        let mut m3 = a_match(3);
        m3.status = MatchStatus::Finished;
        m3.winner = Some(1);
        m3.finished_at = Some(400);

        let mut m4 = a_match(4);
        m4.status = MatchStatus::Finished;
        m4.winner = Some(2);
        m4.finished_at = Some(300);

        let mut m5 = a_match(5);
        m5.status = MatchStatus::Finished;
        m5.winner = Some(1);
        m5.finished_at = Some(200);

        let mut m6 = a_match(6);
        m6.status = MatchStatus::Finished;
        m6.winner = Some(2);
        m6.finished_at = Some(100);

        let mut m_ohne = a_match(7);
        m_ohne.status = MatchStatus::Finished;
        m_ohne.winner = Some(1);
        m_ohne.result = MatchResult::Retired;
        m_ohne.finished_at = None;

        // Ein nicht beendetes Spiel darf nicht auftauchen.
        let offen = a_match(8);

        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            Vec::new(),
            vec![m1, m2, m3, m4, m5, m6, m_ohne, offen],
            Vec::new(),
        ));
        let config = AppConfig::default();

        // Limit 40 (Hallennetz): alle sieben beendeten Spiele kommen durch,
        // sortiert wie oben beschrieben.
        let state = build_state_limited(&tablet, &config, 1_000, 1, 40);
        let ids: Vec<i64> = state.finished.iter().map(|f| f.match_id).collect();
        assert_eq!(
            ids,
            vec![1, 2, 3, 4, 5, 6, 7],
            "neueste zuerst, ohne Zeitstempel ans Ende"
        );
        assert_eq!(state.finished[6].result, "retired");

        // Der Relay-Weg kürzt wirklich: Limit 5 heißt genau die fünf
        // jüngsten Spiele — die beiden ältesten (6, ohne Zeitstempel: 7)
        // fallen weg, nicht nur irgendwelche.
        let eng = build_state_limited(&tablet, &config, 1_000, 2, 5);
        let capped_ids: Vec<i64> = eng.finished.iter().map(|f| f.match_id).collect();
        assert_eq!(eng.finished.len(), 5, "Limit 5 kappt auf genau fünf");
        assert_eq!(
            capped_ids,
            vec![1, 2, 3, 4, 5],
            "gekappt wird am Ende der sortierten Liste, nicht wahllos"
        );
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
                called_at_ms: 900_000,
                recalls: 0,
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
            profile_id: String::new(),
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
    fn a_finished_match_needs_no_freeing_anymore() {
        // Seit ein beendetes Spiel sein Feld nicht mehr hält, gibt es hier
        // nichts freizugeben: Wer trotzdem „vom Feld nehmen" schickt (etwa
        // von einer Ansicht, die noch den alten Stand zeigt), bekommt eine
        // Ablehnung statt eines Schreibvorgangs, der die Feldangabe des
        // beendeten Spiels löschen könnte — die ist Turnier-Dokumentation
        // („wo wurde gespielt") und bleibt unangetastet.
        let mut done = a_match(7);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![done], Vec::new());
        let err = plan_court_action(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            &relay_proto::TlAction::FreeCourt {
                court_id: 1,
                expect: relay_proto::CourtExpectation::Match { match_id: 7 },
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::CourtFree));
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
    fn a_court_whose_last_match_is_finished_accepts_the_next_one() {
        // Früher wurde das mit einem eigenen Wortlaut abgelehnt („wird noch
        // geräumt"). Da BTP die Feldangabe am beendeten Spiel nie entfernt,
        // war diese Ablehnung endgültig statt vorübergehend — das Feld war
        // für den Rest des Turniers verloren.
        let mut done = a_match(9);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![a_match(7), done], Vec::new());
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
        .expect("das Feld ist wieder vergebbar");
        assert_eq!(write.courts[0].match_id, Some(7));
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
            None,
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
    fn a_result_can_be_entered_for_a_match_that_never_saw_a_court() {
        // Spiel im Status Scheduled, keinem Feld zugewiesen — die
        // Turnierleitung trägt den Endstand ein (niemand hat gezählt).
        // Erwartung: gleiche Schreib-Nutzlast wie am Desktop-Pfad
        // (enter_result, commands.rs:1349), Status-Feld gesetzt (ScoreStatus
        // 0 = regulär), KEINE Feldfreigabe (es gibt kein Feld freizugeben).
        let m = a_match(7); // Status::Scheduled, court_id: None (siehe a_match)
        let s = snap(Vec::new(), vec![m], Vec::new());

        let updates = plan_result_action(
            &s,
            None, // kein Aufruf-Stempel — das Spiel stand nie auf einem Feld
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
            None,
        )
        .expect("erlaubt — auch ohne Feld muss sich ein Endstand eintragen lassen");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].btp_match_id, 7);
        assert_eq!(updates[0].sets, vec![(21, 15), (21, 19)]);
        assert!(updates[0].team1_won);
        // Status-Feld (ScoreStatus): 0 = regulär ausgespielt, wie am Desktop.
        assert_eq!(updates[0].score_status, 0);
        // Kein Feld zum Freigeben — der Nachbar-Test mit Feld erwartet hier
        // Some(1), dieser hier None.
        assert_eq!(
            updates[0].free_court_id, None,
            "kein Feld zugewiesen → nichts freizugeben"
        );
        assert!(
            updates[0].player_ids.is_empty(),
            "kein Aufruf → kein Checkout"
        );
        assert_eq!(updates[0].end_ts_ms, None);
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
            None,
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
            None,
        )
        .unwrap_err();
        assert!(
            err.error.unwrap_or_default().contains("Aufgabe"),
            "die Meldung muss den richtigen Weg nennen"
        );
    }

    #[test]
    fn a_result_with_nothing_behind_it_can_be_corrected() {
        // Ein gewertetes Spiel ohne Folgespiel: Hier gibt es nichts, was der
        // geänderte Sieger durcheinanderbringen könnte — ein Finale, ein
        // Gruppenspiel, ein Draw ohne weitere Runde.
        let mut done = a_match(7);
        done.status = MatchStatus::Finished;
        done.winner = Some(1);
        let s = snap(vec![a_court(1, None)], vec![done], Vec::new());
        let aktion = |overwrite| relay_proto::TlAction::EnterResult {
            match_id: 7,
            sets: vec![
                relay_proto::SetAb { a: 21, b: 15 },
                relay_proto::SetAb { a: 21, b: 10 },
            ],
            retired: false,
            winner: None,
            overwrite,
        };

        // Ohne ausdrücklichen Wunsch bleibt es bei „schon gewertet" — so
        // ersetzt niemand versehentlich ein Ergebnis.
        let err = plan_result_action(&s, None, 9_000, &aktion(false), None).unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::AlreadyScored));

        // Mit ausdrücklichem Wunsch geht es durch.
        let updates = plan_result_action(&s, None, 9_000, &aktion(true), None).unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].team1_won);
    }

    #[test]
    fn a_correction_that_would_touch_the_bracket_is_still_refused() {
        // Sobald ein Folgespiel dranhängt, bleibt es gesperrt — bis am
        // echten BTP geprüft ist, was ein Überschreiben mit dem Turnierbaum
        // macht (Spec, offener Punkt 1; docs/btp_protocol.md).
        let (mut vorher, folge) = ko_paar(7, 8);
        vorher.status = MatchStatus::Finished;
        vorher.winner = Some(1);
        let s = snap(Vec::new(), vec![vorher, folge], Vec::new());
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
            None,
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::CorrectionBlocked));
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
        let updates = walkover_updates(&candidates, &[11, 12], &std::collections::HashMap::new());
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].score_status, 1, "1 = kampflos");
        assert!(updates[0].sets.is_empty(), "kampflos hat keine Sätze");
        assert!(!updates[0].team1_won, "Team 1 hat aufgegeben");
        assert!(updates[1].team1_won, "hier war es Team 2");
        // Ein nicht ausgewähltes Spiel bleibt unangetastet.
        assert!(walkover_updates(&candidates, &[11], &std::collections::HashMap::new()).len() == 1);
    }

    /// Ein Spiel, das auf einem Feld steht — Grundlage der Aufruf-Tests.
    fn match_on_court(id: i64, court_id: i64) -> BtpMatch {
        let mut m = a_match(id);
        m.court_id = Some(court_id);
        m.court = Some(format!("Feld {court_id}"));
        m.status = crate::btp::model::MatchStatus::OnCourt;
        m
    }

    #[test]
    fn a_device_the_tournament_pc_does_not_know_is_refused_even_from_the_relay() {
        // Über den Relay kommt nur eine Kennung, kein Zugang — den hat der
        // Relay geprüft. Der Turnier-PC schaut trotzdem in seiner eigenen
        // Liste nach: Sonst hinge die Nachvollziehbarkeit („wer hat was
        // ausgelöst") allein am Relay, und ein Protokolleintrag benennte ein
        // Gerät, das dieser Rechner nie gekoppelt hat.
        let mut cfg = AppConfig::default();
        cfg.tl_web.enabled = true;
        cfg.tl_web.devices = vec![crate::config::TlDevice {
            id: "tl-3f2a".to_string(),
            token: "geheim".to_string(),
            label: "Tablet Meeting Point".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        }];
        assert_eq!(
            device_by_id(&cfg, "tl-3f2a").map(|d| d.label),
            Some("Tablet Meeting Point".to_string())
        );
        assert!(device_by_id(&cfg, "tl-fremd").is_none());
        assert!(device_by_id(&cfg, "").is_none());

        // Und bei abgeschalteter Oberfläche gilt kein Gerät — derselbe
        // Riegel wie im Hallennetz.
        cfg.tl_web.enabled = false;
        assert!(device_by_id(&cfg, "tl-3f2a").is_none());
    }

    /// Ein KO-Spiel samt Folgespiel: `vorher` speist über seine
    /// Planungsposition das `folge`-Spiel.
    fn ko_paar(vorher_id: i64, folge_id: i64) -> (BtpMatch, BtpMatch) {
        let mut vorher = a_match(vorher_id);
        vorher.planning_id = 2000;
        vorher.from1 = Some(1000);
        vorher.from2 = Some(1001);
        let mut folge = a_match(folge_id);
        folge.planning_id = 3000;
        folge.from1 = Some(2000); // kommt aus dem Vorgänger
        folge.from2 = Some(2001);
        (vorher, folge)
    }

    #[test]
    fn a_result_without_any_follow_up_match_may_be_corrected() {
        // Der unstrittige Fall: Es gibt nichts, was der geänderte Sieger
        // durcheinanderbringen könnte — ein Finale, ein Gruppenspiel, ein
        // Draw ohne weitere Runde.
        let mut m = a_match(7);
        m.planning_id = 2000;
        let snap = snap(Vec::new(), vec![m], Vec::new());
        assert_eq!(correction_blocker(&snap, 7), None);
    }

    #[test]
    fn a_correction_is_refused_while_the_follow_up_is_being_played() {
        // Hier ist der Sieger längst weitergerückt und spielt bereits. Ein
        // Überschreiben zöge einen laufenden Court mit — im schlimmsten Fall
        // stünden zwei Paare auf dem Feld und keiner wüsste, wer gemeint ist.
        let (vorher, mut folge) = ko_paar(7, 8);
        folge.status = crate::btp::model::MatchStatus::OnCourt;
        folge.court_id = Some(3);
        let snap = snap(Vec::new(), vec![vorher, folge], Vec::new());
        assert_eq!(
            correction_blocker(&snap, 7),
            Some(CorrectionBlocker::Running)
        );
    }

    #[test]
    fn a_correction_is_refused_once_the_follow_up_is_decided() {
        // Noch weiter: Das Folgespiel ist gewertet. Wer jetzt den Sieger der
        // Vorrunde ändert, macht aus einem gültigen Ergebnis ein Rätsel.
        let (vorher, mut folge) = ko_paar(7, 8);
        folge.winner = Some(1);
        folge.status = crate::btp::model::MatchStatus::Finished;
        let snap = snap(Vec::new(), vec![vorher, folge], Vec::new());
        assert_eq!(
            correction_blocker(&snap, 7),
            Some(CorrectionBlocker::Decided)
        );
    }

    #[test]
    fn a_follow_up_that_has_not_started_is_the_open_question() {
        // Der Fall, den erst das BTP-Experiment klärt: Das Folgespiel steht
        // da, der Sieger ist eingesetzt, gespielt wird noch nicht. Ob BTP
        // den Baum beim Überschreiben neu rechnet, weiß niemand — bis es
        // jemand ausprobiert hat, bleibt der Fall gesperrt.
        let (vorher, folge) = ko_paar(7, 8);
        let snap = snap(Vec::new(), vec![vorher, folge], Vec::new());
        assert_eq!(
            correction_blocker(&snap, 7),
            Some(CorrectionBlocker::Untested)
        );
    }

    #[test]
    fn a_follow_up_in_another_draw_does_not_count() {
        // Planungspositionen sind nur je Draw eindeutig — BTP vergibt in
        // jedem Draw dieselben. Ohne die Draw-Prüfung blockierte ein
        // fremdes Turnierfeld die Korrektur.
        let (vorher, mut folge) = ko_paar(7, 8);
        folge.draw_id = 99;
        folge.status = crate::btp::model::MatchStatus::OnCourt;
        let snap = snap(Vec::new(), vec![vorher, folge], Vec::new());
        assert_eq!(correction_blocker(&snap, 7), None);
    }

    #[test]
    fn a_board_too_big_for_the_relay_is_shortened_instead_of_lost() {
        // Der Relay legt einen zu großen Zustand nicht ab — und der Host
        // erfährt davon nichts. Ohne eigene Kürzung wäre die
        // Cloud-Oberfläche in genau den Turnieren tot, in denen sie am
        // meisten hülfe: je größer das Turnier, desto sicherer.
        // Ein volles Zwei-Hallen-Turnier: Die Warteliste wird **je Halle**
        // gekappt, also stehen bis zu 240 Spiele im Zustand — mit
        // Doppelpaarungen und Namen, wie sie im Badminton vorkommen.
        let mut cfg = AppConfig::default();
        for (draw, halle) in [("HE A", "Halle A"), ("HE B", "Halle B")] {
            cfg.discipline_hall_rules.push(DisciplineHallRule {
                discipline: "mens_singles".to_string(),
                draw_name: draw.to_string(),
                hall: halle.to_string(),
            });
        }
        let tablet = TabletState::default();
        let mut matches = Vec::new();
        for id in 1..=400 {
            let mut m = a_match(id);
            m.team1 = vec![
                player("Maximiliane Charlotte von Hohenlohe-Waldenburg"),
                player("Friederike Alexandra Schmidt-Blumenthal"),
            ];
            m.team2 = vec![
                player("Konstantin Ferdinand Oppermann-Lindenau"),
                player("Sebastian Aurelius Wittgenstein-Berleburg"),
            ];
            m.draw_name = if id % 2 == 0 { "HE A" } else { "HE B" }.to_string();
            m.round_name = "Achtelfinale der Trostrunde".to_string();
            matches.push(m);
        }
        tablet.set_snapshot(snap(Vec::new(), matches, Vec::new()));

        let (json, _rev) = state_for_relay(&tablet, &cfg, 1_000_000);
        assert!(
            json.len() <= relay_proto::MAX_TL_STATE_LEN,
            "passt nicht: {} Bytes",
            json.len()
        );
        // Und die Kürzung wird gemeldet, statt Spiele stillschweigend
        // verschwinden zu lassen.
        let state: TlState = serde_json::from_str(&json).unwrap();
        assert!(
            !state.truncated_halls.is_empty(),
            "gekürzt, aber nicht gesagt"
        );

        // Ein kleines Turnier verliert nichts.
        let klein = TabletState::default();
        klein.set_snapshot(snap(Vec::new(), vec![a_match(1)], Vec::new()));
        let (json, _rev) = state_for_relay(&klein, &AppConfig::default(), 1_000_000);
        let state: TlState = serde_json::from_str(&json).unwrap();
        assert!(state.truncated_halls.is_empty());
        assert_eq!(state.queue.len(), 1);
    }

    #[test]
    fn replacing_a_lost_devices_access_reaches_the_relay() {
        // Ein Tablet geht verloren, sein Zugang wird ersetzt — die Kennung
        // bleibt (sie ist die Geräte-Identität). Erkennt der Abgleich nur
        // Kennungen, gilt der ALTE Zugang beim Relay weiter: Das verlorene
        // Tablet schriebe bis Turnierende aus dem Internet mit, und das neu
        // gekoppelte käme nicht durch.
        let mut cfg = AppConfig::default();
        cfg.tl_web.enabled = true;
        cfg.tl_web.devices = vec![crate::config::TlDevice {
            id: "tl-1".to_string(),
            token: "alt".to_string(),
            label: "Tablet".to_string(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        }];
        let vorher = auth_fingerprint(&auth_devices(&cfg));

        cfg.tl_web.devices[0].token = "neu".to_string();
        let nachher = auth_fingerprint(&auth_devices(&cfg));
        assert_ne!(vorher, nachher, "der Zugangswechsel muss auffallen");

        // Der Fingerabdruck selbst trägt keinen Zugang — er lebt in einer
        // Variablen, die beim Suchen nach Fehlern schnell ausgegeben ist.
        assert!(
            !nachher.contains("neu"),
            "kein Zugang im Klartext: {nachher}"
        );
    }

    #[test]
    fn the_devices_pushed_to_the_relay_carry_no_names() {
        // Der Relay braucht Kennung und Zugang — mehr nicht. Das Etikett
        // („Tablet Meeting Point") bleibt am Turnier-PC: Es kann einen
        // Personennamen enthalten, und der hat auf einem fremden Server
        // nichts zu suchen.
        let mut cfg = AppConfig::default();
        cfg.tl_web.enabled = true;
        cfg.tl_web.devices = vec![crate::config::TlDevice {
            id: "tl-1".to_string(),
            token: "tok".to_string(),
            label: "Tablet von Anna Meier".to_string(),
            created_at_ms: 1,
            hall: "Halle A".to_string(),
            profile_id: String::new(),
        }];
        let devices = auth_devices(&cfg);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "tl-1");
        assert_eq!(devices[0].token, "tok");
        let json = serde_json::to_string(&devices).unwrap();
        assert!(!json.contains("Anna"), "kein Name im Frame: {json}");
        assert!(!json.contains("Halle"), "auch keine Halle: {json}");

        // Abgeschaltet heißt: kein einziges Gerät zugelassen.
        cfg.tl_web.enabled = false;
        assert!(auth_devices(&cfg).is_empty());
    }

    #[test]
    fn the_revision_only_moves_when_the_board_really_changed() {
        // Die Revision ist die Grundlage von zweierlei: Der Relay spart mit
        // ihr die Übertragung (gleiche Fassung → „unverändert"), und der
        // Turnier-PC erkennt an ihr, ob ein Tipp auf einem überholten Plan
        // beruhte. Beides bricht, wenn sie im Sekundentakt hochzählt, nur
        // weil die Uhr weitergelaufen ist.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new()));
        let cfg = AppConfig::default();

        let erste = build_state_with_rev(&tablet, &cfg, 1_000_000);
        assert!(erste.rev > 0, "eine Revision beginnt bei 1, nicht bei 0");
        let spaeter = build_state_with_rev(&tablet, &cfg, 1_060_000);
        assert_eq!(
            spaeter.rev, erste.rev,
            "eine Minute später, aber nichts passiert: dieselbe Fassung"
        );
        assert!(spaeter.server_now_ms > erste.server_now_ms, "die Uhr läuft");

        // Jetzt eine echte Änderung.
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![a_match(7), a_match(8)],
            Vec::new(),
        ));
        let danach = build_state_with_rev(&tablet, &cfg, 1_060_000);
        assert!(
            danach.rev > erste.rev,
            "ein neues Spiel in der Liste ist eine neue Fassung"
        );
    }

    #[test]
    fn the_state_carries_the_call_stage_so_every_device_shows_the_same_number() {
        // Ohne diese Zahl im Zustand rechnete jede Seite selbst — und zwei
        // Turnierleitungen sähen verschiedene Stufen für dasselbe Spiel.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let cfg = AppConfig::default();
        let before = build_state(&tablet, &cfg, 1_000_000, 1);
        let court = before.courts.iter().find(|c| c.court_id == 3).unwrap();
        assert_eq!(
            court.call_stage, 0,
            "auf dem Feld stehen ist noch kein gesprochener Aufruf"
        );

        tablet.note_court_call(3, 7);
        let after = build_state(&tablet, &cfg, 1_000_000, 2);
        let court = after.courts.iter().find(|c| c.court_id == 3).unwrap();
        assert_eq!(court.call_stage, 2);
    }

    #[test]
    fn exclude_from_auto_assign_sets_and_clears_the_exclusion() {
        // Spec `feldvergabe-ausnahme`: TL-Web setzt/nimmt die Ausnahme
        // zurück, betrifft ausschließlich den lokalen Store — kein
        // BTP-Write, kein Rückgabewert außer Erfolg.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![a_match(7)], Vec::new()));

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::ExcludeFromAutoAssign {
                match_id: 7,
                excluded: true,
            },
        )
        .unwrap();
        assert!(done.ok);
        assert!(tablet.auto_assign_excluded(7));

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::ExcludeFromAutoAssign {
                match_id: 7,
                excluded: false,
            },
        )
        .unwrap();
        assert!(done.ok);
        assert!(!tablet.auto_assign_excluded(7));
    }

    #[test]
    fn exclude_from_auto_assign_rejects_an_unknown_match() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));

        let err = apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::ExcludeFromAutoAssign {
                match_id: 999,
                excluded: true,
            },
        )
        .unwrap_err();
        assert!(!err.ok);
        assert!(!tablet.auto_assign_excluded(999));
    }

    #[test]
    fn queue_reorder_moves_a_match_within_its_hall_and_marks_it_manual() {
        // Spec `spielliste-manuelle-reihenfolge`: Match 3 (BTP-Reihenfolge
        // zuletzt, da Spielnummer 3) vor Match 1 ziehen — der Präfix
        // enthält danach nur das gezogene Match, Match 1 folgt weiter über
        // die normale BTP-Reihenfolge unmittelbar dahinter.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            Vec::new(),
            vec![a_match(1), a_match(2), a_match(3)],
            Vec::new(),
        ));

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::QueueReorder {
                match_id: 3,
                before_match_id: Some(1),
            },
        )
        .unwrap();
        assert!(done.ok);

        let state = build_state(&tablet, &AppConfig::default(), 0, 1);
        let ids: Vec<i64> = state.queue.iter().map(|m| m.match_id).collect();
        assert_eq!(
            ids,
            vec![3, 1, 2],
            "3 vorgezogen, Rest folgt BTP-Reihenfolge"
        );
        let manual_flags: std::collections::HashMap<i64, bool> =
            state.queue.iter().map(|m| (m.match_id, m.manual)).collect();
        assert!(manual_flags[&3], "gezogenes Match ist markiert");
        assert!(
            !manual_flags[&1],
            "Zielmatch braucht keinen eigenen Präfix-Rang"
        );
    }

    #[test]
    fn queue_reorder_rejects_an_unknown_match() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));

        let err = apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::QueueReorder {
                match_id: 999,
                before_match_id: None,
            },
        )
        .unwrap_err();
        assert!(!err.ok);
    }

    #[test]
    fn queue_order_reset_clears_every_halls_prefix() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            Vec::new(),
            vec![a_match(1), a_match(2), a_match(3)],
            Vec::new(),
        ));
        apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::QueueReorder {
                match_id: 3,
                before_match_id: Some(1),
            },
        )
        .unwrap();

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            0,
            &relay_proto::TlAction::QueueOrderReset,
        )
        .unwrap();
        assert!(done.ok);

        let state = build_state(&tablet, &AppConfig::default(), 0, 1);
        let ids: Vec<i64> = state.queue.iter().map(|m| m.match_id).collect();
        assert_eq!(ids, vec![1, 2, 3], "wieder reine BTP-Reihenfolge");
        assert!(state.queue.iter().all(|m| !m.manual));
    }

    // ───────────── Panel-Profile (Spec tl-web-panelsystem, ADR 0025) ────────

    /// Minimales, gültiges Wire-Profil für Tests.
    fn wire_profile(id: &str, name: &str) -> relay_proto::TlPanelProfileWire {
        relay_proto::TlPanelProfileWire {
            id: id.to_string(),
            name: name.to_string(),
            panels: vec![relay_proto::TlPanelSettingWire {
                key: "courts".to_string(),
                visible: true,
                height_fr: 2.0,
            }],
            display: relay_proto::TlDisplaySettingsWire {
                show_numbers: true,
                list_position: relay_proto::TlListPositionWire::Bottom,
                ..Default::default()
            },
            // Wird von `profile_save` ohnehin verworfen und durch `now_ms`
            // ersetzt — hier absichtlich ein Fantasiewert, um genau das zu
            // belegen.
            updated_at_ms: 999,
        }
    }

    #[test]
    fn profiles_view_maps_config_to_wire_profiles() {
        let mut config = AppConfig::default();
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "profil-1".into(),
            name: "Wandmonitor".into(),
            panels: vec![crate::config::TlPanelSetting {
                key: "officials".into(),
                visible: false,
                height_fr: 1.5,
            }],
            display: crate::config::TlDisplaySettings {
                show_club_names: true,
                list_position: crate::config::TlListPosition::Bottom,
                ..Default::default()
            },
            updated_at_ms: 42,
        });
        let view = profiles_view(&config);
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].id, "profil-1");
        assert_eq!(view[0].name, "Wandmonitor");
        assert_eq!(view[0].panels.len(), 1);
        assert_eq!(view[0].panels[0].key, "officials");
        assert!(!view[0].panels[0].visible);
        assert_eq!(view[0].panels[0].height_fr, 1.5);
        assert!(view[0].display.show_club_names);
        assert_eq!(
            view[0].display.list_position,
            relay_proto::TlListPositionWire::Bottom
        );
        assert_eq!(view[0].updated_at_ms, 42);
    }

    #[test]
    fn execute_profile_save_upserts_by_id() {
        let mut config = AppConfig::default();
        profile_save(&mut config, &wire_profile("profil-1", "Erst"), 1_000).unwrap();
        assert_eq!(config.tl_web.profiles.len(), 1);
        assert_eq!(config.tl_web.profiles[0].name, "Erst");

        // Zweiter Save mit derselben id + neuem Namen: überschreibt, statt
        // ein zweites Element anzulegen.
        profile_save(&mut config, &wire_profile("profil-1", "Geändert"), 2_000).unwrap();
        assert_eq!(config.tl_web.profiles.len(), 1, "Upsert, kein Duplikat");
        assert_eq!(config.tl_web.profiles[0].name, "Geändert");
        assert_eq!(config.tl_web.profiles[0].updated_at_ms, 2_000);
    }

    #[test]
    fn execute_profile_save_generates_id_when_empty() {
        let mut config = AppConfig::default();
        profile_save(&mut config, &wire_profile("", "Neu"), 5_000).unwrap();
        assert_eq!(config.tl_web.profiles.len(), 1);
        assert!(
            !config.tl_web.profiles[0].id.is_empty(),
            "der Host vergibt eine Kennung"
        );
        assert_eq!(config.tl_web.profiles[0].updated_at_ms, 5_000);
    }

    #[test]
    fn execute_profile_save_rejects_empty_name() {
        let mut config = AppConfig::default();
        let err = profile_save(&mut config, &wire_profile("profil-1", "  "), 1_000).unwrap_err();
        assert!(!err.ok);
        assert!(config.tl_web.profiles.is_empty());
    }

    #[test]
    fn profile_save_truncates_long_names() {
        // Das `maxlength` im Browser ist über einen direkten Aufruf der
        // Kommando-Route umgehbar — der Katalog reist vollständig in jedem
        // `TlState` mit, ein überlanger Name dürfte ihn nicht sprengen.
        let mut config = AppConfig::default();
        let lang = "ä".repeat(500);
        profile_save(&mut config, &wire_profile("profil-1", &lang), 1_000).unwrap();
        assert_eq!(
            config.tl_web.profiles[0].name.chars().count(),
            MAX_TL_PROFILE_NAME_LEN,
            "gekappt, nicht abgelehnt — und nach ZEICHEN, nicht nach Bytes"
        );
    }

    #[test]
    fn profile_save_rejects_oversized_panel_list() {
        let mut config = AppConfig::default();
        let mut profil = wire_profile("profil-1", "Zu viele Panels");
        profil.panels = (0..MAX_TL_PROFILE_PANELS + 1)
            .map(|i| relay_proto::TlPanelSettingWire {
                key: format!("panel-{i}"),
                visible: true,
                height_fr: 1.0,
            })
            .collect();
        let err = profile_save(&mut config, &profil, 1_000).unwrap_err();
        assert!(!err.ok);
        assert!(
            config.tl_web.profiles.is_empty(),
            "abgelehnt — nichts gespeichert"
        );
    }

    #[test]
    fn profiles_capped_at_max_tl_profiles() {
        let mut config = AppConfig::default();
        for i in 0..relay_proto::MAX_TL_PROFILES {
            profile_save(
                &mut config,
                &wire_profile(&format!("profil-{i}"), &format!("P{i}")),
                1_000,
            )
            .unwrap();
        }
        assert_eq!(config.tl_web.profiles.len(), relay_proto::MAX_TL_PROFILES);

        // Ein NEUES Profil scheitert an der Kappung ...
        let err =
            profile_save(&mut config, &wire_profile("profil-neu", "Zu viel"), 1_000).unwrap_err();
        assert!(!err.ok);
        assert_eq!(config.tl_web.profiles.len(), relay_proto::MAX_TL_PROFILES);

        // ... aber ein Update eines BESTEHENDEN Profils bleibt möglich —
        // die Kappung darf ein Update nicht blockieren.
        profile_save(
            &mut config,
            &wire_profile("profil-0", "Aktualisiert"),
            2_000,
        )
        .unwrap();
        assert_eq!(config.tl_web.profiles.len(), relay_proto::MAX_TL_PROFILES);
        assert_eq!(config.tl_web.profiles[0].name, "Aktualisiert");
    }

    #[test]
    fn last_write_wins_by_updated_at_ms() {
        // Die Spec verlangt ausdrücklich KEINE Konfliktprüfung: Der zuletzt
        // gespeicherte Stand gewinnt einfach, unabhängig von der Reihenfolge
        // der `updated_at_ms`-Werte im Wire-Payload (die ohnehin verworfen
        // werden — der Host stempelt selbst).
        let mut config = AppConfig::default();
        profile_save(&mut config, &wire_profile("profil-1", "Von Gerät A"), 5_000).unwrap();
        profile_save(&mut config, &wire_profile("profil-1", "Von Gerät B"), 1_000).unwrap();
        assert_eq!(
            config.tl_web.profiles.len(),
            1,
            "kein Konflikt, kein Duplikat"
        );
        assert_eq!(
            config.tl_web.profiles[0].name, "Von Gerät B",
            "die zuletzt ausgeführte Aktion gewinnt"
        );
        assert_eq!(config.tl_web.profiles[0].updated_at_ms, 1_000);
    }

    #[test]
    fn execute_profile_delete_falls_back_devices_to_default() {
        let mut config = AppConfig::default();
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "profil-1".into(),
            name: "Wandmonitor".into(),
            panels: Vec::new(),
            display: crate::config::TlDisplaySettings::default(),
            updated_at_ms: 1,
        });
        config.tl_web.default_profile_id = "profil-1".into();
        config.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-a".into(),
            token: "tok-a".into(),
            label: "Tablet A".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: "profil-1".into(),
        });
        config.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-b".into(),
            token: "tok-b".into(),
            label: "Tablet B".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: "profil-anderes".into(),
        });

        profile_delete(&mut config, "profil-1");

        assert!(config.tl_web.profiles.is_empty());
        assert!(
            config.tl_web.default_profile_id.is_empty(),
            "auch der turnierweite Standard fällt zurück"
        );
        assert!(
            config.tl_web.devices[0].profile_id.is_empty(),
            "Gerät A trug das gelöschte Profil → Standard"
        );
        assert_eq!(
            config.tl_web.devices[1].profile_id, "profil-anderes",
            "Gerät B trug ein anderes Profil → unberührt"
        );

        // Löschen eines bereits verschwundenen Profils ist ein No-Op, kein
        // Fehler.
        profile_delete(&mut config, "profil-1");
        assert!(config.tl_web.devices[0].profile_id.is_empty());
    }

    #[test]
    fn execute_profile_select_sets_calling_devices_profile_id_not_target() {
        // Sicherheitstest: `profile_select` bekommt die Geräte-Kennung aus
        // der Auth (hier: `device_id`-Parameter), NIE aus einem Client-Feld
        // — ein Gerät darf nur sich selbst binden.
        let mut config = AppConfig::default();
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "profil-1".into(),
            name: "Wandmonitor".into(),
            panels: Vec::new(),
            display: crate::config::TlDisplaySettings::default(),
            updated_at_ms: 1,
        });
        config.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-a".into(),
            token: "tok-a".into(),
            label: "Tablet A".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        });
        config.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-b".into(),
            token: "tok-b".into(),
            label: "Tablet B".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: String::new(),
        });

        // Gerät A wählt ein Profil ...
        profile_select(&mut config, "dev-a", "profil-1").unwrap();

        assert_eq!(
            config.tl_web.devices[0].profile_id, "profil-1",
            "das aufrufende Gerät bekommt die Wahl"
        );
        assert!(
            config.tl_web.devices[1].profile_id.is_empty(),
            "ein fremdes Gerät bleibt unberührt, obwohl es in der Liste steht"
        );
    }

    #[test]
    fn execute_profile_select_accepts_empty_as_default_and_rejects_unknown() {
        let mut config = AppConfig::default();
        config.tl_web.devices.push(crate::config::TlDevice {
            id: "dev-a".into(),
            token: "tok-a".into(),
            label: "Tablet A".into(),
            created_at_ms: 1,
            hall: String::new(),
            profile_id: "profil-1".into(),
        });
        // Leer ("Standard") ist immer gültig, auch ohne Katalog.
        profile_select(&mut config, "dev-a", "").unwrap();
        assert!(config.tl_web.devices[0].profile_id.is_empty());

        // Eine unbekannte Kennung wird abgelehnt.
        let err = profile_select(&mut config, "dev-a", "spukt-nicht").unwrap_err();
        assert!(!err.ok);
    }

    #[test]
    fn execute_profile_set_default_updates_the_tournament_wide_default() {
        let mut config = AppConfig::default();
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "profil-1".into(),
            name: "Wandmonitor".into(),
            panels: Vec::new(),
            display: crate::config::TlDisplaySettings::default(),
            updated_at_ms: 1,
        });
        profile_set_default(&mut config, "profil-1").unwrap();
        assert_eq!(config.tl_web.default_profile_id, "profil-1");

        let err = profile_set_default(&mut config, "unbekannt").unwrap_err();
        assert!(!err.ok);
        assert_eq!(
            config.tl_web.default_profile_id, "profil-1",
            "eine abgelehnte Änderung darf den bisherigen Stand nicht anrühren"
        );

        // Leer (eingebautes Standardprofil) ist immer gültig.
        profile_set_default(&mut config, "").unwrap();
        assert!(config.tl_web.default_profile_id.is_empty());
    }

    #[test]
    fn touches_courts_false_for_profile_actions() {
        for action in [
            relay_proto::TlAction::ProfileSave {
                profile: wire_profile("profil-1", "X"),
            },
            relay_proto::TlAction::ProfileDelete {
                profile_id: "profil-1".to_string(),
            },
            relay_proto::TlAction::ProfileSelect {
                profile_id: "profil-1".to_string(),
            },
            relay_proto::TlAction::ProfileSetDefault {
                profile_id: "profil-1".to_string(),
            },
        ] {
            assert!(
                !touches_courts(&action),
                "Panel-Profile sind reine Konfiguration, keine Feld-Aktion"
            );
        }
    }

    #[test]
    fn a_second_call_counts_up_at_the_host_and_orders_the_announcement() {
        // Der erneute Aufruf tut zweierlei: Er zählt die Stufe hoch (damit
        // jedes Gerät dieselbe Zahl sieht) und beauftragt die Ansage in der
        // Halle des Feldes. Sprechen tut die Seite nie selbst — sie steht im
        // Zweifel in einem Büro und nicht an der Anlage.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, Some(2))],
            vec![match_on_court(7, 3)],
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
        ));
        // Ein Gerät in Halle B hört zu — sonst käme eine Warnung dazu.
        tablet.announce_jobs_since("Halle B", 0, 50_000);

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
            },
        )
        .unwrap();
        assert!(done.ok);
        assert!(done.warning.is_none(), "in Halle B hört jemand zu");
        assert_eq!(tablet.calls_made(3, 7), 2, "der erneute Aufruf ist der 2.");

        let jobs = tablet.announce_jobs_since("Halle B", 0, 50_000);
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].kind,
            crate::tablet::state::AnnounceJobKind::CourtCall {
                court_id: 3,
                match_id: 7,
                stage: 2,
            }
        );
        assert!(
            tablet.announce_jobs_since("Halle A", 0, 50_000).is_empty(),
            "Halle A geht der Aufruf nichts an"
        );
    }

    #[test]
    fn a_call_never_lags_behind_what_the_clock_already_shows_as_due() {
        // Am Feld steht „Letzter Aufruf", weil die Uhr abgelaufen ist. Sagte
        // der Knopf daneben dann den zweiten an, widersprächen sich zwei
        // Anzeigen auf demselben Bildschirm — und die Halle hörte einen
        // Aufruf, der eine Stufe zurückliegt. Dieselbe Regel wendet die
        // Desktop-Übersicht seit jeher an; sie gehört an den Turnier-PC,
        // damit sie für jedes Gerät gilt.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let mut cfg = AppConfig::default();
        cfg.call_timer.enabled = true;
        cfg.call_timer.second_call_minutes = 2.0;
        cfg.call_timer.third_call_minutes = 4.0;
        // Das Spiel steht seit fünf Minuten auf dem Feld, ohne dass ein Punkt
        // gefallen ist: Die Uhr ist bei der dritten Stufe.
        let seit = 1_000_000;
        tablet.reconcile_on_court(&std::collections::HashMap::from([(3, 7)]), seit);

        let done = apply_state_action(
            &tablet,
            &cfg,
            seit + 5 * 60_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
            },
        )
        .unwrap();
        assert!(done.ok);
        assert_eq!(
            tablet.calls_made(3, 7),
            3,
            "die Uhr war beim letzten Aufruf, also ist es der letzte"
        );
    }

    #[test]
    fn a_running_match_is_never_escalated_to_the_last_call() {
        // Der Aufruf-Timer zählt die Zeit, bis die Spieler ans Feld kommen.
        // Sind Punkte gefallen, sind sie da — dann ist er gegenstandslos.
        // Beide Oberflächen halten sich daran; der Turnier-PC muss es auch,
        // sonst schallt „Dritter und letzter Aufruf" durch die Halle,
        // während längst gespielt wird.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let mut cfg = AppConfig::default();
        cfg.call_timer.enabled = true;
        cfg.call_timer.second_call_minutes = 2.0;
        cfg.call_timer.third_call_minutes = 4.0;
        let seit = 1_000_000;
        tablet.reconcile_on_court(&std::collections::HashMap::from([(3, 7)]), seit);
        // Es wird gespielt: Der erste Punkt ist gefallen.
        tablet.record_score(3, 7, vec![(1, 0)]);

        apply_state_action(
            &tablet,
            &cfg,
            seit + 6 * 60_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
            },
        )
        .unwrap();
        assert_eq!(
            tablet.calls_made(3, 7),
            2,
            "die Uhr hebt nichts an, solange gespielt wird"
        );
    }

    #[test]
    fn calling_a_match_that_is_not_on_that_court_is_refused() {
        // Sonst ginge eine Ansage für eine Begegnung hinaus, die dort gar
        // nicht spielt — und die Stufe zählte für ein fremdes Spiel hoch.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let err = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 99,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::CourtFree));
        assert_eq!(tablet.calls_made(3, 99), 0, "nichts hochgezählt");
    }

    #[test]
    fn an_announcement_without_a_device_in_the_hall_still_counts_but_says_so() {
        // Die Turnierleitung darf nicht glauben, der Aufruf sei erklungen.
        // Trotzdem gilt er als erfolgt: Sonst stünde die Stufe auf einem
        // anderen Stand als das, was die Halle gehört hat, sobald das Gerät
        // zurückkommt.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
            },
        )
        .unwrap();
        assert!(done.ok, "die Aktion gilt als ausgeführt");
        assert!(
            done.warning
                .as_deref()
                .is_some_and(|w| w.contains("Ansage")),
            "aber die Seite erfährt es: {:?}",
            done.warning
        );
        assert_eq!(tablet.calls_made(3, 7), 2, "und die Stufe zählt trotzdem");
    }

    #[test]
    fn a_repeated_preparation_call_is_announced_in_the_hall_it_was_called_to() {
        // Der Vorbereitungs-Aufruf hat seine eigene Halle (der Meeting Point
        // kann in einer anderen Halle stehen als das spätere Feld).
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            Vec::new(),
            vec![a_match(7)],
            vec![crate::btp::model::BtpLocation {
                id: 2,
                name: "Halle B".to_string(),
            }],
        ));
        tablet.add_preparation_call(crate::tablet::state::PreparationCall {
            match_id: 7,
            location_id: Some(2),
            called_at_ms: 10_000,
        });
        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnouncePrepCall {
                match_id: 7,
                side: relay_proto::PrepCallSide::Team1,
            },
        )
        .unwrap();
        assert!(done.ok);
        let jobs = tablet.announce_jobs_since("Halle B", 0, 50_000);
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].kind,
            crate::tablet::state::AnnounceJobKind::PrepCall {
                match_id: 7,
                side: relay_proto::PrepCallSide::Team1,
                stage: 2,
            },
            "der erste Nachruf ist der zweite Aufruf"
        );
    }

    #[test]
    fn a_preparation_call_that_was_never_made_cannot_be_repeated() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), vec![a_match(7)], Vec::new()));
        let err = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnouncePrepCall {
                match_id: 7,
                side: relay_proto::PrepCallSide::Both,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::NotAllowed));
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
        let err = plan_walkover_action(&candidates, &[99], &std::collections::HashMap::new())
            .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::AlreadyHandled));
        assert!(!err.ok);
        // Mit einem noch vorhandenen Kandidaten geht es weiter wie bisher.
        assert_eq!(
            plan_walkover_action(&candidates, &[11], &std::collections::HashMap::new())
                .unwrap()
                .len(),
            1
        );
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
    fn the_state_shows_the_scorekeeper_queue_only_when_managed() {
        // Aufbau wie in `the_scorekeeper_queue_can_be_tended`: TabletState mit
        // Snapshot, ein manueller Eintrag in der Warteschlange.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));
        tablet.add_scorekeeper_manual(vec!["Anna Alt".to_string()], 1_000);

        let mut cfg = AppConfig::default();
        cfg.scorekeeper.enabled = true;
        let state = build_state(&tablet, &cfg, 1_000, 1);
        assert!(state.scorekeeper_managed);
        assert_eq!(state.scorekeepers.len(), 1);
        assert_eq!(state.scorekeepers[0].names, vec!["Anna Alt".to_string()]);
        assert!(!state.scorekeepers[0].key.is_empty());

        // Verwaltung aus: Die Liste bleibt leer, das Gerät blendet den
        // Abschnitt aus — niemand bedient eine Warteschlange, die es nicht gibt.
        cfg.scorekeeper.enabled = false;
        let state = build_state(&tablet, &cfg, 1_000, 2);
        assert!(!state.scorekeeper_managed);
        assert!(state.scorekeepers.is_empty());
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
    fn the_page_learns_when_a_called_match_counts_as_overdue() {
        // Die Turnierleitung faerbt ihre Feldkacheln danach ein: aufgerufen,
        // zu lange nicht angefangen, im Spiel, beendet. Ab wann „zu lange"
        // gilt, entscheidet das Turnier — und die Schwelle muss auf jedem
        // Gerät dieselbe sein, sonst leuchtet eine Halle rot und die andere
        // nicht.
        let mut cfg = AppConfig::default();
        cfg.call_timer.not_started_minutes = 7.5;
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(Vec::new(), Vec::new(), Vec::new()));

        let s = build_state(&tablet, &cfg, 1_000, 1);
        assert_eq!(s.call_timer.not_started_minutes, 7.5);
    }

    #[test]
    fn nationalities_travel_next_to_the_names() {
        // Für die zuschaltbare Nationen-Anzeige. Parallel zu den Namen statt
        // verschachtelt: Die Seite geht beide Listen im Gleichschritt durch,
        // und ein fehlender Eintrag (nicht jeder Spieler hat eine Angabe)
        // bleibt ein leerer Platz statt einer Lücke, die alles verschiebt.
        let tablet = TabletState::default();
        let mut wartend = a_match(2);
        wartend.team1[0].nationality = Some("GER".to_string());
        wartend.team2[0].nationality = None;
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![wartend], Vec::new()));

        let s = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        assert_eq!(s.queue[0].team1_nat, vec!["GER"]);
        assert_eq!(
            s.queue[0].team2_nat,
            vec![""],
            "ohne Angabe ein leerer Platz — die Listen bleiben gleich lang"
        );
    }

    #[test]
    fn every_match_carries_its_discipline() {
        // Turniere benennen ihre Auslosungen frei („Gruppe 6"). Steht dort
        // nicht zufällig „HE" drin, ist am Bildschirm nicht zu erkennen, ob
        // ein Einzel oder ein Doppel aufs Feld soll — der Helfer sieht es
        // erst an der Zahl der Namen. Die Disziplin gehört deshalb an jedes
        // Spiel, auf dem Feld wie in der Warteliste.
        let tablet = TabletState::default();
        let mut auf_dem_feld = a_match(1);
        auf_dem_feld.status = MatchStatus::OnCourt;
        auf_dem_feld.court_id = Some(1);
        auf_dem_feld.draw_name = "Gruppe 6".to_string();
        auf_dem_feld.discipline = Discipline::MensDoubles;
        let mut wartend = a_match(2);
        wartend.draw_name = "Gruppe 6".to_string();
        wartend.discipline = Discipline::Mixed;
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![auf_dem_feld, wartend],
            Vec::new(),
        ));

        let s = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        assert_eq!(s.courts[0].discipline, "mens_doubles");
        assert_eq!(s.queue[0].discipline, "mixed");
    }

    #[test]
    fn has_timeline_only_for_recorded_matches() {
        // Punktverlauf (AK-4): Der Graph-Klick erscheint nur, wo es
        // wirklich einen Verlauf gibt — Papier-Spiele bieten ihn nicht an.
        let tablet = TabletState::default();
        let mut auf_dem_feld = a_match(1);
        auf_dem_feld.status = MatchStatus::OnCourt;
        auf_dem_feld.court_id = Some(1);
        let mut fertig_mit_verlauf = a_match(2);
        fertig_mit_verlauf.status = MatchStatus::Finished;
        fertig_mit_verlauf.winner = Some(1);
        let mut fertig_papier = a_match(3);
        fertig_papier.status = MatchStatus::Finished;
        fertig_papier.winner = Some(2);
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![auf_dem_feld, fertig_mit_verlauf, fertig_papier],
            Vec::new(),
        ));
        // Verläufe: fürs Feld-Spiel und EIN beendetes; das Papier-Spiel
        // bekommt keinen.
        assert!(tablet.timeline_store().apply_rally(1, 1, 1, "A", 1, 0));
        assert!(tablet.timeline_store().apply_rally(2, 1, 1, "B", 0, 1));

        let s = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        assert!(s.courts[0].has_timeline);
        let by_id = |id: i64| s.finished.iter().find(|f| f.match_id == id).unwrap();
        assert!(by_id(2).has_timeline);
        assert!(!by_id(3).has_timeline, "Papier-Spiel ohne Verlauf");
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
    fn a_court_of_a_finished_match_is_shown_as_free() {
        // Bis 09.08.2026 zeigte die Seite hier „wird geräumt" — in der
        // Annahme, BTP räume das Feld gleich ab. Es räumt nie ab: Die
        // CourtID bleibt als Doku am beendeten Match stehen. Damit stand
        // jedes Feld nach seinem ersten Ergebnis dauerhaft auf „wird
        // geräumt" und nahm nichts mehr an (im Turniertest aufgetreten).
        let mut done = a_match(9);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = state_with(
            snap(vec![a_court(1, None)], vec![done, a_match(2)], Vec::new()),
            &AppConfig::default(),
        );
        assert_eq!(s.courts[0].match_id, 0, "kein laufendes Spiel");
        assert_eq!(s.courts[0].clearing, None, "und das Feld ist wieder frei");
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
            // Punktverlauf (Spec punktverlauf-graph): reines Bool-Flag „es
            // gibt einen Graphen" — der Verlauf selbst geht NIE über den
            // Zustands-Push, sondern nur on-demand über die eigene Route.
            "has_timeline",
            // Grundeinstellung der automatischen Vergabe: kein
            // personenbezogenes Datum, und die Seite braucht sie, um den
            // Schalter überhaupt anbieten zu dürfen.
            "configured",
            "wait_minutes",
            "active_hall",
            "second_call_minutes",
            "third_call_minutes",
            "not_started_minutes",
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
            // Disziplin als Schlüssel („mens_singles"). Sagt etwas über das
            // Spiel, nichts über die Personen — und steht ohnehin auf jedem
            // Aushang.
            "discipline",
            "team1",
            "team2",
            // Nation als ISO-Kürzel, zuschaltbar und standardmäßig aus.
            // Dieselbe Angabe zeigt der Court-Monitor öffentlich als Flagge;
            // hier steht sie hinter dem Gerätezugang. Kein Geburtsjahr, keine
            // Lizenznummer — die bleiben draußen.
            "team1_nat",
            "team2_nat",
            // Vereinsname, ebenso zuschaltbar und standardmäßig aus (Nutzer-
            // Entscheidung 12.08.2026 — bewusste Datenschutz-Freigabe wie bei
            // der Nationalität). Dieselbe Angabe steht auf jeder Meldeliste und
            // jedem Aushang; hier hinter dem Gerätezugang.
            "team1_club",
            "team2_club",
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
            // Zahl der gesprochenen Aufrufe (0–3) — keine Angabe zu Personen.
            "call_stage",
            "recalls",
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
            // Spec `feldvergabe-ausnahme`: reines Bool-Flag „Auto-Vergabe
            // übergeht dieses Spiel gerade" — keine Angabe zu Personen.
            "excluded_from_auto_assign",
            // Spec `spielliste-manuelle-reihenfolge`: reines Bool-Flag „steht
            // im manuellen Präfix seiner Halle" — keine Angabe zu Personen.
            "manual",
            // Warteschlange der Zähltafelbediener: Namen stehen ohnehin je Feld im
            // Zustand (`scorekeeper`); der `key` ist eine zufällige Kennung ohne
            // Personenbezug, `enqueued_ms` eine Uhrzeit.
            "scorekeeper_managed",
            "scorekeepers",
            "key",
            "names",
            "enqueued_ms",
            // Schiedsrichter (Spec schiedsrichter-management): Der Name ist
            // zweckgebunden freigegeben wie die Spielernamen — ohne ihn ließe
            // sich niemand einteilen. `paused`, `on_duty_court_id` und
            // `appearances` sind Betriebsangaben ohne Personenbezug über den
            // Namen hinaus. Sperrlisten, Verein, Lizenz und Geburtsjahr
            // stehen bewusst NICHT hier: Sperrlisten kodieren persönliche
            // Beziehungen und kommen nur über die gezielte, authentifizierte
            // Leseroute.
            "officials_managed",
            "officials",
            "id",
            "name",
            "paused",
            "on_duty_court_id",
            "appearances",
            // SR/AR je Feld + Konflikt-KATEGORIE (nie der Grund) und die
            // drei Feld-Schalter, die die Seite auch setzen kann.
            "sr",
            "ar",
            "official_warn",
            "sr_id",
            "ar_id",
            "rotate_sr",
            "rotate_ar",
            "assign_operator",
            // Ergebnis-Übersicht: keine Personendaten über die ohnehin
            // gezeigten Namen hinaus (`team1`/`team2`, `draw_name`,
            // `round_name`, `class_label`, `discipline`, `sets`, `court`,
            // `match_num` sind bereits durch Warteliste/Feld erlaubt).
            "finished",
            "winner",
            "result",
            "finished_at_ms",
            // Raster-Anordnung je Halle: reine Geometrie-Konfiguration vom
            // Turnier-PC, keine Personendaten.
            "layouts",
            "columns",
            "origin",
            "serpentine",
            // Nummerierungsrichtung (spaltenweise statt reihenweise) — ebenso
            // reine Geometrie.
            "vertical",
            // Turnierweite Anzeige-Schalter (nur boolesche Flags, keine
            // Personendaten): ob tl.html Vereinsname/-logo einblenden darf.
            "show_club_names",
            "show_club_logos",
            // Panel-Profile (Spec tl-web-panelsystem, ADR 0025): reine
            // Layout-/Sichtbarkeits-Konfiguration ohne jeden Personenbezug.
            // `id`/`name`/`key` sind bereits oben erlaubt (Schiedsrichter/
            // Zähltafelbediener) — hier neu: der Profil-Katalog selbst, die
            // Panel-Höhe/-Sichtbarkeit je Eintrag und die Anzeige-Schalter.
            // Die Wire-Struct (`relay_proto::TlPanelProfileWire` & Co.)
            // serialisiert camelCase, deshalb stehen hier die camelCase-
            // Formen, nicht die Rust-Feldnamen.
            "profiles",
            "default_profile_id",
            "panels",
            "visible",
            "heightFr",
            "display",
            "showNumbers",
            "showNations",
            "showClubNames",
            "showClubLogos",
            "showDiscipline",
            "showRound",
            "showGroup",
            "listPosition",
            "updatedAtMs",
        ];

        let tablet = TabletState::default();
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        // Ein beendetes Spiel gehört in dieses Fixture: Sonst serialisiert
        // `finished` als leeres Array, und der Wächter unten sähe nie eines
        // der 13 `TlFinished`-Felder — ein künftiges Feld dort könnte am
        // Test vorbeirutschen.
        let mut finished = a_match(3);
        finished.status = MatchStatus::Finished;
        finished.winner = Some(1);
        finished.finished_at = Some(500_000);
        finished.team1 = vec![player("Winter")];
        finished.team2 = vec![player("Sommer")];
        let mut schnappschuss = snap(
            vec![a_court(1, None)],
            vec![running, a_match(2), finished],
            Vec::new(),
        );
        schnappschuss.officials = vec![crate::btp::model::BtpOfficial {
            id: 1,
            name: "Schiedsmann".to_string(),
            first: "Sabine".to_string(),
            nationality: None,
        }];
        tablet.set_snapshot(schnappschuss);
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"game","endsAt":1700000000000}}"#.to_string(),
        );
        // Ebenso die Zähltafelbediener-Warteschlange: Ohne Eintrag bliebe
        // `scorekeepers` leer und der Wächter sähe auch deren Felder nie.
        tablet.add_scorekeeper_manual(vec!["Anna Alt".to_string()], 1_000);
        // Ebenso ein Schiedsrichter samt Zusatzdaten: Ohne ihn bliebe
        // `officials` leer und der Wächter sähe dessen Felder nie.
        tablet.officials_store().set_enabled(true);
        tablet.officials_store().set_club(1, "TSV Musterstadt");
        tablet
            .officials_store()
            .set_blocklists(1, vec!["SC Gesperrt".into()], vec![4242]);
        let mut config = AppConfig::default();
        config.scorekeeper.enabled = true;
        // Ebenso ein Raster-Eintrag: Ohne ihn bliebe `layouts` leer und der
        // Wächter sähe `columns`/`origin`/`serpentine` nie.
        config.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "Halle 1".into(),
            columns: 3,
            origin: crate::config::LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: false,
        });
        // Ebenso ein Panel-Profil: Ohne Eintrag bliebe `profiles` leer und
        // der Wächter sähe die `TlPanelProfileWire`-Felder (Panel-Liste,
        // Anzeige-Optionen) nie.
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "profil-1".into(),
            name: "Wandmonitor".into(),
            panels: vec![crate::config::TlPanelSetting {
                key: "courts".into(),
                visible: true,
                height_fr: 2.0,
            }],
            display: crate::config::TlDisplaySettings {
                show_numbers: true,
                show_nations: true,
                show_club_names: true,
                show_club_logos: true,
                show_discipline: true,
                show_round: true,
                show_group: true,
                list_position: crate::config::TlListPosition::Bottom,
            },
            updated_at_ms: 1_000,
        });
        config.tl_web.default_profile_id = "profil-1".into();
        let s = build_state(&tablet, &config, 1_000_000, 1);
        assert!(
            !s.officials.is_empty(),
            "Fixture-Fehler: das Fixture muss einen Schiedsrichter enthalten, \
             sonst prüft dieser Test die `TlOfficial`-Felder gar nicht"
        );
        assert!(
            !s.finished.is_empty(),
            "Fixture-Fehler: das Fixture muss ein beendetes Spiel enthalten, \
             sonst prüft dieser Test die `TlFinished`-Felder gar nicht"
        );
        assert!(
            !s.layouts.is_empty(),
            "Fixture-Fehler: das Fixture muss ein Raster enthalten, \
             sonst prüft dieser Test die `TlHallLayout`-Felder gar nicht"
        );
        assert!(
            !s.scorekeepers.is_empty(),
            "Fixture-Fehler: das Fixture muss einen Zähltafelbediener enthalten, \
             sonst prüft dieser Test die `TlScorekeeper`-Felder gar nicht"
        );
        assert!(
            !s.profiles.is_empty(),
            "Fixture-Fehler: das Fixture muss ein Panel-Profil enthalten, \
             sonst prüft dieser Test die `TlPanelProfileWire`-Felder gar nicht"
        );

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
    fn die_detail_route_liefert_sperren_und_einsaetze_nur_gezielt() {
        // Spec: Sperrlisten und Einsatz-Liste kommen NUR auf gezielte
        // Anfrage — hier der Inhalt, den die Route ausliefert.
        let tablet = TabletState::default();
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let mut finished = a_match(3);
        finished.status = MatchStatus::Finished;
        finished.winner = Some(1);
        finished.finished_at = Some(500_000);
        let mut schnappschuss = snap(vec![a_court(1, None)], vec![running, finished], Vec::new());
        schnappschuss.officials = vec![crate::btp::model::BtpOfficial {
            id: 1,
            name: "Schiedsmann".to_string(),
            first: "Sabine".to_string(),
            nationality: None,
        }];
        tablet.set_snapshot(schnappschuss);
        tablet.officials_store().set_enabled(true);
        tablet.officials_store().set_club(1, "TSV Musterstadt");
        tablet
            .officials_store()
            .set_blocklists(1, vec!["SC Gesperrt".into()], vec![4242]);
        tablet
            .officials_store()
            .assign(3, crate::tablet::officials::OfficialRole::Sr, 1);

        let json = official_detail_json(&tablet, 1);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["club"], "TSV Musterstadt");
        assert_eq!(v["blocked_clubs"][0], "SC Gesperrt");
        assert_eq!(v["blocked_players"][0], 4242);
        // Einsätze: nur das beendete Spiel, mit Rolle und Endezeit.
        assert_eq!(v["appearances"].as_array().unwrap().len(), 1);
        assert_eq!(v["appearances"][0]["match_id"], 3);
        assert_eq!(v["appearances"][0]["role"], "sr");
        assert_eq!(v["appearances"][0]["finished_at"], 500_000);

        // Ein unbekannter Official liefert leere Listen statt eines Fehlers —
        // die Seite soll den Dialog trotzdem öffnen können.
        let leer: serde_json::Value =
            serde_json::from_str(&official_detail_json(&tablet, 99)).unwrap();
        assert_eq!(leer["blocked_clubs"].as_array().unwrap().len(), 0);
        assert_eq!(leer["appearances"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn the_state_never_carries_personal_data_beyond_its_purpose() {
        // Diese Daten laufen über eine aus dem Internet erreichbare Seite.
        // Der Test schlägt fehl, sobald jemand ein Feld nachrüstet, das
        // Lizenznummer oder Geburtsjahr transportiert — er macht die
        // Datenschutzregel durchsetzbar statt nur dokumentiert.
        //
        // **Die Nation ist seit 09.08.2026 erlaubt**, **der Verein seit
        // 12.08.2026** — beide standen vorher hier auf der Verbotsliste.
        // Bewusst geändert: Die Turnierleitung braucht sie, um die richtige
        // Paarung ans Feld zu holen und Vereinskollegen auseinanderzuhalten.
        // Es bleibt beim ISO-Kürzel bzw. Vereinsnamen, die Anzeige ist
        // turnierweit zuschaltbar und standardmäßig aus — und dieselbe Angabe
        // steht ohnehin auf jeder Meldeliste und jedem Aushang, während sie
        // hier hinter dem Gerätezugang steht.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        // Ein Verein am Fixture-Spieler, damit die Gegenprobe unten belegt,
        // dass der zuschaltbare Vereinsname tatsächlich transportiert wird.
        running.team1[0].club = Some("SC Musterstadt".to_string());
        running.team2 = vec![licensed_player("Gegner", "08-005678")];
        let mut waiting = a_match(2);
        waiting.team1 = vec![licensed_player("Weber", "08-009999")];
        waiting.team2 = vec![licensed_player("Fischer", "08-004321")];
        // Ein beendetes Spiel gehört auch hier ins Fixture — sonst prüft der
        // Test die Personendaten in `TlFinished` (Team-Namen, Satzstände)
        // nie, obwohl genau die über eine aus dem Internet erreichbare Seite
        // laufen.
        let mut finished = a_match(3);
        finished.status = MatchStatus::Finished;
        finished.winner = Some(1);
        finished.finished_at = Some(500_000);
        finished.team1 = vec![licensed_player("Winter", "08-003333")];
        finished.team2 = vec![licensed_player("Sommer", "08-004444")];

        // Wie im Allowlist-Wächter: Warteschlange der Zähltafelbediener nicht
        // leer lassen, sonst prüft dieser Test auch deren Felder nie.
        // `state_with` reicht dafür nicht (der Tablet-Zustand bleibt darin
        // gekapselt) — deshalb hier wie dort von Hand aufgebaut.
        let tablet = TabletState::default();
        let mut schnappschuss = snap(
            vec![a_court(1, None)],
            vec![running, waiting, finished],
            Vec::new(),
        );
        schnappschuss.officials = vec![crate::btp::model::BtpOfficial {
            id: 1,
            name: "Schiedsmann".to_string(),
            first: "Sabine".to_string(),
            nationality: None,
        }];
        tablet.set_snapshot(schnappschuss);
        tablet.add_scorekeeper_manual(vec!["Anna Alt".to_string()], 1_000);
        // Schiedsrichter mit ALLEN Zusatzdaten: Sein Name darf hinaus (wie
        // die Spielernamen, zweckgebunden), seine Sperrlisten und sein
        // Stammverein nicht — genau das prüft die Verbotsliste unten.
        tablet.officials_store().set_enabled(true);
        tablet.officials_store().set_club(1, "TSV Sperrverein");
        tablet
            .officials_store()
            .set_blocklists(1, vec!["SC Gesperrt".into()], vec![4242]);
        let mut config = AppConfig::default();
        config.scorekeeper.enabled = true;
        // Auch hier ein Raster-Eintrag, damit `layouts` nicht leer bleibt —
        // Halle als reines Geometrie-Datum, kein Personenbezug.
        config.hall_layouts.push(crate::config::HallLayoutConfig {
            hall: "Halle 1".into(),
            columns: 3,
            origin: crate::config::LayoutOrigin::BottomLeft,
            serpentine: false,
            vertical: false,
        });
        // Auch hier ein Panel-Profil (Spec tl-web-panelsystem): reine
        // Layout-/Sichtbarkeits-Konfiguration, damit dieser Test
        // strukturell mitprüft, dass `TlPanelProfileWire` keine
        // Personendaten trägt — genau wie bei Raster/Zähltafelbediener/
        // Schiedsrichtern oben.
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "profil-1".into(),
            name: "Wandmonitor".into(),
            panels: vec![crate::config::TlPanelSetting {
                key: "courts".into(),
                visible: true,
                height_fr: 2.0,
            }],
            display: crate::config::TlDisplaySettings {
                show_numbers: true,
                ..Default::default()
            },
            updated_at_ms: 1_000,
        });
        let s = build_state(&tablet, &config, 1_000_000, 7);
        assert!(
            !s.finished.is_empty(),
            "Fixture-Fehler: das Fixture muss ein beendetes Spiel enthalten"
        );
        assert!(
            !s.scorekeepers.is_empty(),
            "Fixture-Fehler: das Fixture muss einen Zähltafelbediener enthalten"
        );
        assert!(
            !s.officials.is_empty(),
            "Fixture-Fehler: das Fixture muss einen Schiedsrichter enthalten"
        );
        assert!(
            !s.profiles.is_empty(),
            "Fixture-Fehler: das Fixture muss ein Panel-Profil enthalten"
        );
        let json = serde_json::to_string(&s).unwrap().to_lowercase();

        for verboten in [
            "08-001234", // die Lizenznummer aus dem Fixture
            "08-003333", // Lizenznummer des beendeten Spiels
            "08-004444", // Lizenznummer des beendeten Spiels (Gegenseite)
            "member",    // Lizenznummer-Feld
            "birth",     // Geburtsjahr — laut Projektregel nirgends
            "geburt",
            "battery", // Akkustand: keine Geräte-Übersicht in diesem Feature
            "serving", // Aufschlag: Zählhilfe, keine Vergabehilfe
            // Die Sperrlisten eines Schiedsrichters kodieren persönliche
            // Beziehungen (wen er nicht pfeifen soll) — sie gehen NIE in den
            // Zustand, den alle Geräte bekommen, sondern nur auf gezielte,
            // per Geräte-Token authentifizierte Anfrage. Der Stammverein
            // gehört zur selben Pflege-Ansicht.
            "sc gesperrt",     // gesperrter Verein aus dem Fixture
            "4242",            // gesperrter Spieler aus dem Fixture
            "blocked_clubs",   // die Felder selbst
            "blocked_players", // (schlicht `blocked` gibt es in der
            "tsv sperrverein", // Warteliste bereits — anderer Zweck)
        ] {
            assert!(
                !json.contains(verboten),
                "'{verboten}' darf nicht im Anzeige-Zustand stehen: {json}"
            );
        }
        // Die Nation darf — aber nur als Kürzel neben dem Namen, nicht als
        // ganzer Spieler-Datensatz.
        assert!(
            json.contains("team1_nat"),
            "die zuschaltbare Nationen-Anzeige braucht das Kürzel"
        );
        // Der Verein darf ebenso — als Name neben dem Namen, zuschaltbar.
        assert!(
            json.contains("sc musterstadt"),
            "der zuschaltbare Vereinsname muss transportiert werden"
        );
        // Gegenprobe: Die Namen, die die Turnierleitung zum Arbeiten braucht,
        // sind sehr wohl da — sonst prüfte der Test nur einen leeren Zustand.
        assert!(json.contains("müller"));
        // Der **Name** des Schiedsrichters ist bewusst freigegeben (wie die
        // Spielernamen, zweckgebunden): Ohne ihn ließe sich niemand
        // einteilen, und er steht ohnehin auf dem Aushang. Seine Sperrlisten
        // und sein Stammverein bleiben draußen (Verbotsliste oben).
        assert!(
            json.contains("sabine schiedsmann"),
            "der Schiedsrichter-Name muss transportiert werden"
        );
    }
}
