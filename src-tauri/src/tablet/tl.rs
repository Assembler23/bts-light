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
use crate::tablet::predict;
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
            // Spiele ohne feststehende Paarung lassen sich nicht rufen (Spec
            // `tl-offene-paarungen`, Nicht-Ziel G2): Es gibt niemanden
            // anzusagen. Ausdrücklich ablehnen statt durchlaufen zu lassen —
            // sonst nähme der Aufruf den Weg bis in den Zustand und
            // verschwände erst beim nächsten BTP-Schnappschuss wieder
            // (`apply_preparation_calls`), ohne dass jemand erführe, warum.
            if let Some(offen) = match_ids.iter().find(|id| ist_offene_paarung(tablet, **id)) {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    format!(
                        "Spiel {offen} hat noch keine feststehende Paarung — \
                         es gibt niemanden aufzurufen."
                    ),
                ));
            }
            for match_id in match_ids {
                tablet.add_preparation_call(crate::tablet::state::PreparationCall {
                    match_id: *match_id,
                    location_id: *location_id,
                    called_at_ms: now_ms,
                });
                // Der Aufruf ist die frischere Entscheidung als die
                // automatische Vorverteilung — er räumt deren Eintrag (E3,
                // Spec `hallen-vorverteilung`); der Sync-Reconcile ist nur
                // das Sicherheitsnetz.
                tablet.auto_hall_store().remove(*match_id);
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
            // Jeder Hand-Eingriff (auch die Rücknahme, leerer Name) räumt
            // eine Auto-Zuordnung — sonst käme nach der Rücknahme die alte
            // Auto-Halle aus der Kaskade zurück, statt dass frisch verteilt
            // wird (B2: die TL entscheidet, die Automatik füllt nur nach).
            tablet.auto_hall_store().remove(*match_id);
            Ok(TlResponse::ok(0))
        }
        A::SetWishCourt { match_id, court_id } => {
            if !known_match(*match_id) {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    format!("Spiel {match_id} gibt es im aktuellen Turnierstand nicht."),
                ));
            }
            if let Some(court_id) = court_id {
                let snap = tablet.snapshot_clone();
                let Some(snap) = snap else {
                    return Err(TlResponse::err(
                        C::NotAllowed,
                        "Es ist noch kein Turnier geladen.",
                    ));
                };
                // Nur Felder, die dieses Turnier hat — sonst wartete das
                // Spiel auf ein Feld, das es nicht gibt, und in der Liste
                // stünde nur „wartet" ohne Grund.
                let Some(feld) = snap.court_infos.iter().find(|c| c.id == *court_id) else {
                    return Err(TlResponse::err(
                        C::NotAllowed,
                        "Dieses Feld gibt es in diesem Turnier nicht.",
                    ));
                };
                // Ein gesperrtes Feld bekommt von der Automatik ohnehin kein
                // Spiel — der Wunsch liefe ins Leere und das Spiel wartete
                // stumm. Und man braucht die Sperre für diesen Zweck nicht
                // mehr: Der Wunsch hält das Feld selbst frei.
                if tablet.is_court_locked(*court_id) {
                    return Err(TlResponse::err(
                        C::NotAllowed,
                        format!(
                            "Feld {} ist gesperrt. Ein Wunschfeld hält das Feld ohnehin \
                             frei — dafür braucht es die Sperre nicht.",
                            feld.name
                        ),
                    ));
                }
                // Widerspricht das Feld der Hallen-Regel des Spiels, bekäme
                // das Spiel dort nie ein Feld (`hall_allows_match`). Lieber
                // jetzt ablehnen als später stumm warten lassen.
                let feld_halle = snap.court_location_name(*court_id);
                let Some(m) = snap.matches.iter().find(|m| m.id == *match_id) else {
                    return Err(TlResponse::err(
                        C::NotAllowed,
                        format!("Spiel {match_id} gibt es im aktuellen Turnierstand nicht."),
                    ));
                };
                if !config.hall_allows_match(m.discipline.as_str(), &m.draw_name, &feld_halle) {
                    return Err(TlResponse::err(
                        C::HallNotAllowed,
                        "Dieses Spiel darf nach den Hallen-Regeln nicht in diese Halle.",
                    ));
                }
            }
            tablet.set_wish_court(*match_id, *court_id);
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
        A::AnnounceCourtCall {
            court_id,
            match_id,
            side,
        } => {
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
            let hall = hall_of_court(&snap, *court_id);
            // Die Uhr am Feld darf nicht weiter sein als der Aufruf: Steht
            // dort schon „Letzter Aufruf", wäre ein zweiter ein Rückschritt.
            let faellig = due_call_stage(tablet, config, *court_id, *match_id, now_ms);
            // Ein Partei-Aufruf ist ein vollwertiger Aufruf, zählt die Stufe
            // aber nur EINMAL je Runde hoch: Wer erst Partei A und dann
            // Partei B ruft, hat einmal gerufen, nicht zweimal (Spec
            // tl-liste-vereinfachen E1 — die Regel steckt in
            // `note_court_call_at_least`, damit Desktop und TL-Web sie
            // teilen).
            let stage = tablet.note_court_call_at_least(
                *court_id,
                *match_id,
                faellig,
                crate::tablet::state::side_mask(*side),
                unlimited_court_calls(config),
            );
            // Läuft das Spiel schon (Punkte gefallen), wäre „Zweiter/
            // Dritter und letzter Aufruf" absurd — der Auftrag wird auf
            // die Ab-4-Ansage gehoben, die das Ansage-Gerät ohne
            // Stufenwort spricht (`AnnounceJobPlayer`). Der Zähler selbst
            // bleibt ehrlich bei seiner Zahl.
            let job_stage = if tablet.points_scored(*court_id, *match_id) {
                stage.max(4)
            } else {
                stage
            };
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::CourtCall {
                    court_id: *court_id,
                    match_id: *match_id,
                    stage: job_stage,
                    side: side.unwrap_or(relay_proto::PrepCallSide::Both),
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
            let hall = hall_of_court(&snap, *court_id);
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::Officials {
                    court_id: *court_id,
                },
                now_ms,
            );
            Ok(announcement_response(tablet, &hall, now_ms))
        }
        A::AnnounceStartPlay { court_id, match_id } => {
            let Some(snap) = tablet.snapshot_clone() else {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Es ist noch kein Turnier geladen.",
                ));
            };
            // Nur ein Feld, auf dem GENAU dieses Spiel steht. Zwischen dem
            // Antippen am Tablet und dem Ankommen hier kann das Spiel vom
            // Feld genommen oder getauscht worden sein — dann wäre die
            // Aufforderung an die Falschen gerichtet.
            let steht_dort = snap.matches.iter().any(|m| {
                m.id == *match_id
                    && m.court_id == Some(*court_id)
                    && m.status == crate::btp::model::MatchStatus::OnCourt
            });
            if !steht_dort {
                return Err(TlResponse::err(
                    C::StaleView,
                    "Auf diesem Feld steht dieses Spiel nicht (mehr).",
                ));
            }
            let hall = hall_of_court(&snap, *court_id);
            // **Kein** `due_call_stage`, **kein** `note_court_call_at_least`:
            // Die Aufforderung ist kein Aufruf (Spec A3.3). Wer hier
            // „konsistenzhalber" eine Stufe hochzählte, ließe das
            // Aufruf-Abzeichen springen und brächte die Turnierleitung um
            // die Zählung, an der die kampflose Wertung hängt.
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::StartPlay {
                    court_id: *court_id,
                    match_id: *match_id,
                },
                now_ms,
            );
            Ok(announcement_response(tablet, &hall, now_ms))
        }
        A::AnnounceScorekeeper { court_id } => {
            if !config.scorekeeper.enabled {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Die Zähltafelbediener-Verwaltung ist ausgeschaltet.",
                ));
            }
            let Some(snap) = tablet.snapshot_clone() else {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Es ist noch kein Turnier geladen.",
                ));
            };
            // Das Spiel, das gerade auf dem Feld steht — an ihm hängt die
            // Bediener-Zuweisung, und es begrenzt den Zähler.
            let Some(m) = snap.matches.iter().find(|m| {
                m.court_id == Some(*court_id) && m.status == crate::btp::model::MatchStatus::OnCourt
            }) else {
                return Err(TlResponse::err(
                    C::StaleView,
                    "Auf diesem Feld läuft gerade kein Spiel.",
                ));
            };
            // Ohne zugewiesenen Bediener gäbe es niemanden zu nennen — ein
            // Gong ohne Inhalt. Die eine Prüfung deckt alle drei Fälle ab
            // (A2.4): leere Warteschlange, Feld mit abgeschalteter Vergabe
            // (`CourtSwitches::operator`) und global ausgeschaltete
            // Verwaltung — in allen dreien weist der Sync-Lauf gar nicht
            // erst zu.
            let namen = tablet.assigned_scorekeeper(*court_id).unwrap_or_default();
            if namen.is_empty() {
                return Err(TlResponse::err(
                    C::NotAllowed,
                    "Diesem Feld ist keine Tabletbedienung zugewiesen.",
                ));
            }
            let hall = hall_of_court(&snap, *court_id);
            // Eigener Zähler — `call_stages`/`prep_call_stages` bleiben
            // unberührt (A2.5). Ein Nachruf an die Bedienung ist kein
            // Spieler-Aufruf; zöge er die Aufruf-Zahl hoch, glaubte die
            // Turnierleitung, sie hätte schon zweimal gerufen.
            let stage = tablet.note_scorekeeper_call(*court_id, m.id);
            tablet.publish_announce_job(
                hall.clone(),
                crate::tablet::state::AnnounceJobKind::ScorekeeperCall {
                    court_id: *court_id,
                    match_id: m.id,
                    stage,
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
/// vorderen Spiele sind die, um die es geht. `queue_truncated` meldet es.
///
/// Liefert `(json, rev)`. Dass gekürzt wurde, steht im Zustand selbst
/// (`queue_truncated`) — dort sieht es auch die Turnierleitung.
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
    // Mobilfunk, neben dem Ergebnisweg nach BTP. Vierzig wartende Spiele
    // sind mehr, als eine Turnierleitung unterwegs überblickt; was fehlt,
    // meldet `queue_truncated` ehrlich, und im Hallennetz steht weiterhin
    // die volle Liste.
    //
    // Die Stufen gelten seit ADR 0026 turnierweit statt je Halle — bei
    // zwei Hallen liefert die erste Stufe damit rund halb so viele Spiele
    // wie zuvor. Das ist die gewollte Folge der einen gemeinsamen Liste:
    // Vierzig Spiele der GEMEINSAMEN Abfolge sind genau das, was oben
    // steht, egal in welcher Halle sie laufen.
    const STUFEN: [usize; 4] = [40, 20, 10, 5];
    // EINMAL kanonisch bauen (volle Länge), dann nur noch zuschneiden.
    //
    // Zwei Gründe, beide wichtig (Review 18.08.2026):
    //
    // 1. **Die Revision darf nicht an der Transportgrenze hängen.** Sie
    //    entsteht aus dem Fingerabdruck des gebauten Zustands; baute der
    //    Relay-Weg mit 40 und der LAN-Weg mit 120 Einträgen, hätten
    //    dieselben Turnierdaten zwei verschiedene Fingerabdrücke. Bei mehr
    //    als 40 wartenden Spielen kippte der geteilte Zähler dann bei JEDEM
    //    Takt hin und her — das Rev-Gate des Relays hätte nie gegriffen,
    //    der volle Zustand wäre alle zwei Sekunden über Mobilfunk gegangen
    //    und jede Seite im Sekundentakt zum Neuladen angestoßen worden.
    // 2. **Kosten.** Vorher bis zu vier volle Neubauten je Tick (jeder mit
    //    Snapshot-Kopie und Serialisierung); jetzt einer, danach nur noch
    //    Zuschneiden und Serialisieren.
    let mut state = build_state_with_rev(tablet, config, now_ms);
    let rev = state.rev;
    // Wie viele Spiele die volle Liste hatte — die Kürzungsmeldung muss
    // beim Zuschneiden mitwachsen, sonst zählte sie nur die Kappung des
    // LAN-Limits (120) und verschwiege die des Relay-Limits.
    let voll = state.queue.len() + state.queue_truncated;
    // Die offenen Spiele werden ZULETZT aufgefüllt und damit ZUERST geopfert
    // (Spec `tl-offene-paarungen`, ADR 0051). Sie kommen deshalb hier heraus:
    // Die Leiter darunter entscheidet unverändert über die Arbeitsliste, und
    // erst danach kommt zurück, was noch ins Fenster passt.
    //
    // Ein fester Zahlenwert ginge hier fehl. Im dokumentierten Worst Case
    // (26 belegte Felder, 30 Ergebnisse) liegt der Zustand schon ohne sie bei
    // gut 55 KiB — dort passt keine feste Zahl —, während ein mittleres
    // Turnier mühelos hundert offene Einträge trüge.
    let offene = std::mem::take(&mut state.open_queue);
    let offene_gesamt = offene.len() + state.open_queue_truncated;
    state.open_queue_truncated = offene_gesamt;
    let json = serde_json::to_string(&state).unwrap_or_default();
    if json.len() <= relay_proto::MAX_TL_STATE_LEN && state.queue.len() <= STUFEN[0] {
        return offene_auffuellen(state, offene, offene_gesamt, rev, json);
    }
    for limit in STUFEN {
        if state.queue.len() > limit {
            state.queue.truncate(limit);
            state.queue_truncated = voll.saturating_sub(limit);
        }
        let json = serde_json::to_string(&state).unwrap_or_default();
        if json.len() <= relay_proto::MAX_TL_STATE_LEN {
            return offene_auffuellen(state, offene, offene_gesamt, rev, json);
        }
    }
    // Vorletzte Rettung: das additive Anfangszeiten-Panel opfern —
    // Warteliste und Felder sind der Kern, der Check-In-Zeitplan ist
    // Beiwerk. Neue Listen müssen mitgekürzt werden (Plan
    // tl-web-ausbau; Review 17.08.2026), sonst kippte ein rein
    // additives Panel den gesamten Cloud-Zustand über die Relay-Grenze.
    state.checkin_times = None;
    let json = serde_json::to_string(&state).unwrap_or_default();
    if json.len() <= relay_proto::MAX_TL_STATE_LEN {
        return offene_auffuellen(state, offene, offene_gesamt, rev, json);
    }
    // Vorletzte Stufe: die Spielzeiten-Auswertung. Sie trägt seit der
    // Achsen-Erweiterung (Spec `tl-sicht-feinschliff`, Punkt 1) VIER
    // Zeilensätze statt einem und ist damit der größte rein informative
    // Brocken im Zustand. Das Panel verschwindet dann ehrlich, statt den
    // ganzen Stand über die Grenze zu kippen: Reißt sie, verwirft der Relay
    // das komplette Frame samt Vorgänger, und die Cloud-Turnierleitung
    // sähe GAR NICHTS mehr — auch keine Felder.
    state.time_stats = None;
    let json = serde_json::to_string(&state).unwrap_or_default();
    if json.len() <= relay_proto::MAX_TL_STATE_LEN {
        return offene_auffuellen(state, offene, offene_gesamt, rev, json);
    }
    // Letzte Rettung vor der Aufgabe: die Ergebnisliste stutzen. Ebenfalls
    // reine Rückschau — die Felder dagegen sind das Bedienelement der Seite
    // und bleiben in JEDER Stufe unangetastet. Nötig geworden, seit die
    // Liste Lizenznummern trägt (Punkt 4 derselben Spec).
    //
    // Reihenfolge nach Betriebswert (A0.1): queue → checkin_times →
    // time_stats → finished. Die Auswertung fällt vor der Ergebnisliste,
    // weil ein einzelnes Ergebnis am Feld öfter gebraucht wird als der
    // Median einer Klasse.
    for limit in [10usize, 3] {
        if state.finished.len() > limit {
            state.finished.truncate(limit);
            let json = serde_json::to_string(&state).unwrap_or_default();
            if json.len() <= relay_proto::MAX_TL_STATE_LEN {
                return offene_auffuellen(state, offene, offene_gesamt, rev, json);
            }
        }
    }
    let letzte_rev = rev;
    let letzte = serde_json::to_string(&state).unwrap_or_default();
    if letzte.len() <= relay_proto::MAX_TL_STATE_LEN {
        return offene_auffuellen(state, offene, offene_gesamt, letzte_rev, letzte);
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

/// Legt so viele offene Spiele in den fertig zugeschnittenen Zustand zurück,
/// wie das Relay-Fenster noch hergibt (Spec `tl-offene-paarungen`, ADR 0051).
///
/// `ohne_offene` ist der bereits serialisierte Zustand ohne sie — passt keine
/// einzige Zeile mehr dazu, geht genau der hinaus, und es kostet keine
/// zusätzliche Serialisierung. Ein Turnier ohne offene Spiele merkt von
/// dieser Stufe deshalb gar nichts.
fn offene_auffuellen(
    mut state: TlState,
    offene: Vec<TlOpenMatch>,
    gesamt: usize,
    rev: u64,
    ohne_offene: String,
) -> (String, u64) {
    if offene.is_empty() {
        return (ohne_offene, rev);
    }
    // Dieselbe Staffelung wie bei der Warteliste: wenige, vorhersagbare
    // Größen sind im Betrieb leichter zu erklären als eine Zahl, die bei
    // jedem Abruf anders ausfällt.
    const OFFEN_STUFEN: [usize; 4] = [40, 20, 10, 5];
    for stufe in OFFEN_STUFEN {
        let nehmen = stufe.min(offene.len());
        state.open_queue = offene[..nehmen].to_vec();
        state.open_queue_truncated = gesamt.saturating_sub(nehmen);
        let json = serde_json::to_string(&state).unwrap_or_default();
        if json.len() <= relay_proto::MAX_TL_STATE_LEN {
            return (json, rev);
        }
    }
    // Nicht einmal fünf passen: Dann geht der Stand ohne offene Spiele
    // hinaus. Die Arbeitsliste ist wichtiger als die Vorschau.
    (ohne_offene, rev)
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
    // Die Startzeit-Prognose ist zeitabgeleitet (ein freies Feld „startet
    // ab jetzt") — sie darf die Revision nicht bewegen, sonst zählte rev
    // im Minutentakt hoch, obwohl sich am Brett nichts geändert hat, und
    // jede TL-Aktion liefe auf „überholtem Stand". Die Seite klemmt eine
    // dadurch ältere Prognose selbst auf „jetzt" (max(Prognose, Uhr));
    // frisch gerechnet wird sie bei jeder echten Änderung ohnehin.
    let predicted: Vec<Option<u64>> = state.queue.iter().map(|m| m.predicted_start_ms).collect();
    for m in state.queue.iter_mut() {
        m.predicted_start_ms = None;
    }
    // Die Restzeit belegter Felder (Etappe D) ist genauso zeitabgeleitet —
    // sie schrumpft im Minutentakt und darf die Revision nicht bewegen
    // (Review 2026-08-17). Echte Änderungen (neuer Stand, neues Spiel)
    // bewegen den Fingerprint über Satzstand/Match-ID ohnehin.
    let restzeiten: Vec<Option<u64>> = state.courts.iter().map(|c| c.remaining_min).collect();
    for c in state.courts.iter_mut() {
        c.remaining_min = None;
    }
    let fp = serde_json::to_string(&state).unwrap_or_default();
    for (c, r) in state.courts.iter_mut().zip(restzeiten) {
        c.remaining_min = r;
    }
    for (m, p) in state.queue.iter_mut().zip(predicted) {
        m.predicted_start_ms = p;
    }
    state.server_now_ms = zeit;
    fp
}

/// Eine Zeile des „Anfangszeiten"-Panels: eine Check-In-Klasse des
/// heutigen Tages. Bewusst ohne Spielerlisten (Datensparsamkeit) — die
/// TL-Seite zeigt Zeitplan und Zähler, nie Namen.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TlCheckinTime {
    /// Klassenname, wie badhub ihn führt (z. B. „U15 HE-A").
    pub name: String,
    pub discipline: String,
    /// Anfangszeit als „HH:MM" (Berlin-Wandzeit von badhub).
    pub starts_hm: String,
    /// Anmeldeschluss als „HH:MM". Ohne gepflegten Anmeldeschluss gilt die
    /// Anfangszeit — dieselbe Semantik wie der Ansage-Countdown
    /// (`checkin_state::deadline_text`), sonst widersprächen sich Panel
    /// und Lautsprecher in derselben Halle (Review 17.08.2026).
    pub closes_hm: String,
    /// badhub-Fensterzustand (`unscheduled|pending|open|closed|live`) —
    /// serverseitig in Europe/Berlin berechnet, die Seite färbt nur.
    /// Bewusst NICHT `state` genannt: Die Feldnamen-Whitelist des
    /// Wächter-Tests ist flach, ein generischer Name würde dort künftige
    /// gleichnamige Felder ungeprüft passieren lassen.
    pub window_state: String,
    /// Gemeldete Spieler der Klasse (ohne Abgemeldete —
    /// `checkin_state::tl_ablage`). Spezifischer Name aus demselben
    /// Whitelist-Grund wie `window_state`.
    pub entry_count: i64,
    /// Davon eingecheckt.
    pub checked_in_count: i64,
}

/// Die Zeilen des „Anfangszeiten"-Panels: nur Klassen des übergebenen
/// Tages mit gepflegter Anfangszeit, nach Anfangszeit sortiert (Feldtest
/// 17.08.2026 — die Kachel ist ein Zeitplan; Klassen ohne Zeit sagen ihm
/// nichts, durchgelaufene Schlüsse bleiben sichtbar und werden nur
/// clientseitig ausgegraut). badhub liefert Berlin-Wandzeit ohne
/// Zonen-Anhang, geparst wie `checkin_state::deadline_text` (beide
/// Formate).
fn checkin_times_heute(
    classes: &[crate::badhub::checkin_state::CheckinClass],
    heute: chrono::NaiveDate,
) -> Vec<TlCheckinTime> {
    use crate::badhub::checkin_state::parse_badhub_zeit;
    let mut zeilen: Vec<(chrono::NaiveDateTime, TlCheckinTime)> = classes
        .iter()
        .filter_map(|c| {
            let start = parse_badhub_zeit(c.starts_at.as_deref()?)?;
            if start.date() != heute {
                return None;
            }
            // Ohne gepflegten Anmeldeschluss gilt die Anfangszeit — wie
            // beim Ansage-Countdown (`deadline_text`).
            let closes = c
                .closes_at
                .as_deref()
                .and_then(parse_badhub_zeit)
                .unwrap_or(start);
            Some((
                start,
                TlCheckinTime {
                    name: c.name.clone(),
                    discipline: c.discipline.clone(),
                    starts_hm: start.format("%H:%M").to_string(),
                    closes_hm: closes.format("%H:%M").to_string(),
                    window_state: c.state.clone(),
                    entry_count: c.gemeldet,
                    checked_in_count: c.eingecheckt,
                },
            ))
        })
        .collect();
    zeilen.sort_by_key(|(start, _)| *start);
    zeilen.into_iter().map(|(_, zeile)| zeile).collect()
}

/// Das „heute" des Anfangszeiten-Panels, abgeleitet aus dem injizierten
/// `now_ms` statt aus einem eigenen Uhr-Aufruf — `build_state` bleibt so
/// aus (tablet, config, now_ms) reproduzierbar (Review 17.08.2026).
/// Umgerechnet in die lokale Zeitzone des Turnier-PCs: badhub liefert
/// Berlin-Wandzeit, und der Rechner steht in derselben Halle (dieselbe
/// Abwägung wie `checkin_state::deadline_text`).
fn heutiges_datum(now_ms: u64) -> chrono::NaiveDate {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(now_ms as i64)
        .single()
        .map(|t| t.date_naive())
        .unwrap_or_else(|| chrono::Local::now().date_naive())
}

/// Führt irgendein Panel-Profil „Aufrufe unbegrenzt"?
///
/// Der Aufruf-Zähler ist turnier-global (ein Feld, eine Zahl) — die Option
/// wird deshalb bewusst turnier-weit gelesen statt je handelndem Gerät:
/// Sobald EIN Profil sie führt, will diese Turnierleitung über den dritten
/// Aufruf hinauszählen; ohne sie hält der Host den alten 3er-Deckel selbst
/// (Review 17.08.2026 — Client-Gating allein genügt nicht).
fn unlimited_court_calls(config: &AppConfig) -> bool {
    config
        .tl_web
        .profiles
        .iter()
        .any(|p| p.display.unlimited_court_calls)
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

/// Halle eines Felds als Name; leer, wenn das Feld keiner zugeordnet ist
/// oder das Turnier nur eine Halle hat.
///
/// Eine Stelle statt vormals drei wörtlicher Kopien (Spec
/// `tl-sicht-feinschliff`, Punkt 2 sah das Zusammenfassen vor): Jede
/// Ansage-Aktion muss ihre Zielhalle auflösen, und ein Ansage-Auftrag in
/// der falschen Halle ist ein Fehler, den niemand am Bildschirm sieht —
/// er fällt erst auf, wenn die andere Halle etwas hört.
fn hall_of_court(snap: &crate::btp::model::BtpSnapshot, court_id: i64) -> String {
    snap.court_infos
        .iter()
        .find(|c| c.id == court_id)
        .and_then(|c| c.location_id)
        .and_then(|id| snap.locations.iter().find(|l| l.id == id))
        .map(|l| l.name.clone())
        .unwrap_or_default()
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
    // „die Ansage", nicht „der Aufruf": Über diese Funktion laufen auch die
    // Schiedsrichter- und die Spielbeginn-Ansage, und die sind ausdrücklich
    // KEIN Aufruf (sie zählen keine Stufe hoch). Stünde dort „der Aufruf",
    // widerspräche der Bildschirm der Bedienanleitung, und die
    // Turnierleitung schlösse daraus, die Zählung sei bewegt worden.
    ok.with_warning(format!("{wo} — die Ansage wurde nicht gesprochen."))
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

/// Steht die Paarung dieses Spiels noch nicht (vollständig) fest?
///
/// Dieselbe Bedingung, nach der [`build_state_limited`] die offene Liste
/// füllt — an einer Stelle, damit Anzeige und Ablehnung nicht auseinander
/// laufen können.
fn ist_offene_paarung(tablet: &TabletState, match_id: i64) -> bool {
    tablet.snapshot_clone().is_some_and(|s| {
        s.matches
            .iter()
            .any(|m| m.id == match_id && (m.team1.is_empty() || m.team2.is_empty()))
    })
}

/// Wie ein noch offener Platz eines Spiels beschriftet wird — Spec
/// `tl-offene-paarungen`, ADR 0052.
///
/// `seite` ist 1 oder 2 und meint die Mannschaft, deren Platz offen ist. Die
/// Funktion wird **nur** für tatsächlich offene Plätze aufgerufen; für einen
/// besetzten Platz stehen die echten Namen im Match.
///
/// Dreistufige Kaskade:
/// 1. die Teilnehmer des direkten Vorspiels („Müller oder Schmidt"),
/// 2. `aus Spiel 42`, wenn das Vorspiel selbst noch offen ist,
/// 3. `noch offen`, wenn es kein auffindbares Vorspiel gibt.
///
/// Die Kante liegt in `from1`/`from2` und zeigt auf eine **Slot-PlanningID**,
/// nicht auf ein Spiel: Das Vorspiel ist das Match mit ebendieser
/// `planning_id` **im selben Draw** — ohne die Draw-Bindung träfe man Spiele
/// fremder Auslosungen (docs/btp_protocol.md). Gegenrichtung derselben Kante:
/// [`correction_blocker`].
///
/// Bewusst **nie** „Sieger aus": Bei Platzierungsspielen speist der Verlierer
/// den Platz, und welche Seite es ist, sagt BTP nicht.
///
/// Genau **eine** Ebene tief — sonst stünden in einer frühen Runde acht Namen
/// in einer Zeile, und die Aussage wäre wertlos.
pub(crate) fn offener_platz_text(
    snap: &crate::btp::model::BtpSnapshot,
    m: &crate::btp::model::BtpMatch,
    seite: u8,
) -> String {
    let from = if seite == 1 { m.from1 } else { m.from2 };
    let Some(slot) = from else {
        return OFFEN.to_string();
    };
    // Die Kante zeigt auf eine Slot-PlanningID. Ein Vorspiel gibt es nur,
    // wenn ein Match im SELBEN Draw genau diese Position belegt; sonst
    // speist ein Setzplatz, ein Freilos oder eine andere Auslosung.
    let Some(vorspiel) = snap
        .matches
        .iter()
        .find(|o| o.draw_id == m.draw_id && o.planning_id == slot && o.id != m.id)
    else {
        return OFFEN.to_string();
    };
    if !vorspiel.team1.is_empty() && !vorspiel.team2.is_empty() {
        return format!(
            "{} oder {}",
            mannschaft_text(&vorspiel.team1),
            mannschaft_text(&vorspiel.team2)
        );
    }
    // Das Vorspiel ist selbst noch offen — hier endet die Auflösung bewusst.
    match vorspiel.match_num {
        Some(nr) => format!("aus Spiel {nr}"),
        None => OFFEN.to_string(),
    }
}

/// Beschriftung eines Platzes, über den nichts bekannt ist.
const OFFEN: &str = "noch offen";

/// Die Spieler einer Mannschaft als ein Name — beim Doppel mit Schrägstrich
/// verbunden, damit das Paar als Einheit lesbar bleibt.
fn mannschaft_text(spieler: &[crate::btp::model::BtpPlayer]) -> String {
    spieler
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join("/")
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
            // Bewusst 0 (Spec `spielzeiten-prognose`, E1): kampflos wurde
            // nicht gespielt — hier keine Dauer aus dem Zeiten-Store füllen.
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

    // Fernbefehl an die Tablets: braucht **kein** geladenes Turnier und
    // ändert keine Konfiguration — nur einen Zähler, an dem die
    // Tablet-Verbindungen hängen. Deshalb ganz vorn, noch vor den Profilen.
    // Protokoll und Wiederholungserkennung sind oben schon gelaufen.
    if let relay_proto::TlAction::ReloadTablets = &action {
        ctx.tablet.request_tablet_reload();
        let response = relay_proto::TlResponse::ok(0);
        ctx.tablet
            .remember_result(op_id, &fingerprint, response.clone(), now_ms);
        return response;
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
    // Hallen-Vorverteilung: config-ändernde Aktion wie die Profile
    // (`mutate_app_config`), aber turnierabhängig — der E2-Guard (aktive
    // Halle) braucht den Snapshot, deshalb HINTER dem Snapshot-Gate.
    // Feldsperre: config-ändernd wie die Vorverteilung, und ebenfalls hinter
    // dem Snapshot-Gate — die Prüfung „gibt es dieses Feld?" braucht ihn
    // (Spec `tl-web-felder-sperren`, E6/E7).
    if let Some(response) = execute_lock_court_action(ctx, &snap, &action) {
        if response.ok {
            ctx.tablet
                .remember_result(op_id, &fingerprint, response.clone(), now_ms);
        }
        return response;
    }
    if let Some(response) = execute_hall_prefill_action(ctx, &config, &snap, &action) {
        if response.ok {
            ctx.tablet
                .remember_result(op_id, &fingerprint, response.clone(), now_ms);
        }
        return response;
    }

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

/// Sperrt ein Feld oder gibt es frei (Spec `tl-web-felder-sperren`).
/// `None` heißt: keine Sperr-Aktion, an anderer Stelle weiterbehandeln.
///
/// Ein gesperrtes Feld bekommt von der automatischen Vergabe kein Spiel mehr;
/// ein bereits laufendes bleibt unangetastet und zählt zu Ende. BTP kennt die
/// Sperre nicht (R2) — deshalb steht `LockCourt` bewusst **nicht** in
/// `touches_courts`, sonst liefe die Aktion in die Feld-Beanspruchung und in
/// `write_courts_to_btp`.
fn execute_lock_court_action(
    ctx: &crate::tablet::server::ServerCtx,
    snap: &crate::btp::model::BtpSnapshot,
    action: &relay_proto::TlAction,
) -> Option<relay_proto::TlResponse> {
    use relay_proto::{TlAction as A, TlErrorCode as C, TlResponse};
    let A::LockCourt { court_id, locked } = action else {
        return None;
    };
    let (court_id, locked) = (*court_id, *locked);

    // E6: Nur Felder, die dieses Turnier kennt. Ohne diese Prüfung nähme der
    // Host jede Zahl an; die Sperrliste wüchse mit Einträgen, die in keiner
    // Oberfläche auftauchen — und die dort deshalb auch niemand wieder
    // loswird.
    if !snap.court_infos.iter().any(|c| c.id == court_id) {
        return Some(TlResponse::err(
            C::NotAllowed,
            "Dieses Feld gibt es in diesem Turnier nicht.",
        ));
    }

    // Zielmenge aus dem Laufzeit-Stand ableiten (er ist die Wahrheit, aus der
    // auch die Vergabe liest).
    let mut ziel: Vec<i64> = ctx.tablet.locked_courts();
    if locked {
        if !ziel.contains(&court_id) {
            ziel.push(court_id);
            ziel.sort_unstable();
        }
    } else {
        ziel.retain(|&c| c != court_id);
    }

    // E4/E14: Erst die Datei, dann der Arbeitsspeicher. Scheitert das
    // Schreiben, bleibt BEIDES unverändert und das Gerät bekommt einen
    // Fehler — nie ein Zustand, der nur im RAM steht und beim nächsten Start
    // verschwindet. Der Zyklus läuft unter dem Config-Guard (E9, „der letzte
    // gewinnt", nie ein verlorener Schreibvorgang).
    let tournament = snap.tournament_name.clone();
    let geschrieben = ziel.clone();
    if let Err(rejected) = ctx.mutate_app_config(move |cfg| {
        cfg.locked_courts = geschrieben;
        // Turnierbezug mitschreiben (ADR 0044) — sonst gälte die Sperre als
        // „Turnier unbekannt" und überlebte den nächsten Wechsel.
        cfg.locked_courts_tournament = tournament;
        Ok(())
    }) {
        return Some(rejected);
    }
    ctx.tablet.set_court_locked(court_id, locked);
    // Der Antwortcache der Feld-Übersicht trägt den Sperr-Zustand und
    // verfällt nur über die Revision (Spec monitor-livestand-push, S1).
    ctx.tablet.bump_overview_rev();

    // E11: Hat eine Halle durch diese Sperre kein offenes Feld mehr, werden
    // ihre AUTOMATISCH verteilten Zuordnungen geräumt. Ohne das behielten die
    // dorthin vorverteilten Spiele ihre Hallenbindung (ADR 0030) und bekämen
    // gar kein Feld mehr, obwohl nebenan welche frei sind — die Halle stünde
    // still, ohne dass jemand den Grund sähe.
    //
    // Nur beim Sperren und nur im Mehr-Hallen-Turnier: Im Ein-Hallen-Fall
    // trägt kein Feld einen Hallennamen, die Menge der „offenen" Hallen wäre
    // leer und es würde ALLES geräumt.
    if locked && snap.is_multi_hall() {
        let gesperrt: std::collections::HashSet<i64> = ziel.iter().copied().collect();
        let offen: std::collections::HashSet<String> = snap
            .court_infos
            .iter()
            .filter(|c| !gesperrt.contains(&c.id))
            .map(|c| snap.court_location_name(c.id))
            .filter(|h| !h.trim().is_empty())
            .collect();
        let geraeumt = ctx
            .tablet
            .auto_hall_store()
            .remove_where_hall_not_in(&offen);
        if !geraeumt.is_empty() {
            tracing::info!(
                "Feld {court_id} gesperrt — {} Spiel(e) haben ihre automatische Hallen-\
                 Zuordnung verloren, weil deren Halle kein offenes Feld mehr hat",
                geraeumt.len()
            );
        }
    }

    Some(TlResponse::ok(0))
}

/// Führt die Hallen-Vorverteilungs-Aktionen aus (Spec
/// `hallen-vorverteilung`): `SetHallPrefill` ändert die Config (Weg über
/// `mutate_app_config`, wie die Panel-Profile), `ClearAutoHalls` räumt den
/// Auto-Store (E10). `None` heißt: keine Vorverteilungs-Aktion, an anderer
/// Stelle weiterbehandeln.
fn execute_hall_prefill_action(
    ctx: &crate::tablet::server::ServerCtx,
    config: &AppConfig,
    snap: &crate::btp::model::BtpSnapshot,
    action: &relay_proto::TlAction,
) -> Option<relay_proto::TlResponse> {
    use relay_proto::{TlAction as A, TlErrorCode as C, TlResponse};
    match action {
        A::SetHallPrefill { enabled, window } => {
            // E2-Guard host-seitig (die UI-Ausgrauung ist nur Komfort):
            // Tages-Halle und Vorverteilung schließen sich aus. Wie
            // überall zählt die aktive Halle nur im Mehr-Hallen-Fall —
            // sonst widerspräche der Guard der UI-Anzeige (Review
            // 2026-08-16) und der Vergabe, die sie dann ebenso ignoriert.
            if *enabled {
                let active = config.auto_assign.active_hall.trim();
                let aktiv = snap.is_multi_hall()
                    && !active.is_empty()
                    && snap
                        .locations
                        .iter()
                        .any(|l| l.name.trim().eq_ignore_ascii_case(active));
                if aktiv {
                    return Some(TlResponse::err(
                        C::NotAllowed,
                        "Die aktive Halle der Feldvergabe ist gesetzt — Vorverteilung und \
                         Tages-Halle schließen sich aus. Erst die aktive Halle zurücknehmen.",
                    ));
                }
            }
            // Klemme (Security-Gate für die neue Zahleneingabe): 0 bleibt
            // der „automatisch"-Sentinel, alles andere 1..=120
            // (Wartelisten-Limit).
            let window = (*window).min(QUEUE_LIMIT as u32);
            let enabled = *enabled;
            Some(
                match ctx.mutate_app_config(|cfg| {
                    cfg.hall_prefill.enabled = enabled;
                    cfg.hall_prefill.window = window;
                    Ok(())
                }) {
                    Ok(()) => TlResponse::ok(0),
                    Err(rejected) => rejected,
                },
            )
        }
        A::ClearAutoHalls => {
            // E10: räumt NUR die Auto-Zuordnungen — Hand, Regel und Aufruf
            // bleiben. Der nächste Sync-Lauf verteilt (bei aktivem
            // Schalter) frisch.
            ctx.tablet.auto_hall_store().clear_all();
            Some(relay_proto::TlResponse::ok(0))
        }
        _ => None,
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
            // Bruttostart aus dem Zeiten-Store (Spec `spielzeiten-prognose`,
            // E1): neustartfest; on_court_since bleibt Fallback. Damit
            // sendet auch die TL-Web-Wertung eine echte Duration statt 0.
            // Als Ende zählt bei einer Korrektur der ursprüngliche
            // E3-Stempel, nicht „jetzt" — sonst überschriebe die Korrektur
            // eine korrekte Duration mit Stunden.
            let court_id = snap
                .matches
                .iter()
                .find(|m| m.id == *match_id)
                .and_then(|m| m.court_id);
            let on_court_since = ctx.tablet.brutto_start_ms(*match_id, court_id);
            let btp_end_ms = ctx.tablet.result_end_ms(*match_id, now_ms);
            let officials = ctx.tablet.officials_for_result(*match_id);
            match plan_result_action(&snap, on_court_since, btp_end_ms, action, officials) {
                Ok(u) => {
                    // Spielende stempeln (E3): Eingangszeitpunkt der
                    // TL-Web-Wertung — NICHT-regulär (E11): tablet-lose
                    // Ergebnisse liefern keinen Statistik-Messwert.
                    ctx.tablet
                        .match_times_store()
                        .stamp_finished(*match_id, false, now_ms);
                    u
                }
                Err(rejected) => {
                    return Some(rejected);
                }
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
        A::AnnounceCourtCall {
            court_id,
            match_id,
            side,
        } => {
            // Die Partei gehört in den Fingerabdruck: „Partei A rufen" und
            // „Partei B rufen" unter derselben Vorgangskennung sind zwei
            // verschiedene Absichten, kein Doppeltipp.
            format!("call:{match_id}:{court_id}:{side:?}")
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
        A::SetHallPrefill { enabled, window } => format!("hall-prefill:{enabled}:{window}"),
        A::ClearAutoHalls => "hall-prefill-clear".to_string(),
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
        A::AnnounceScorekeeper { court_id } => format!("sk-announce:{court_id}"),
        A::AnnounceStartPlay { court_id, match_id } => {
            format!("start-play:{court_id}:{match_id}")
        }
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
        // Der Zielzustand gehört in den Fingerabdruck: Sperren und Freigeben
        // desselben Felds sind zwei verschiedene Absichten und dürfen sich
        // nicht gegenseitig als „schon erledigt" gelten.
        A::LockCourt { court_id, locked } => format!("lock:{court_id}:{locked}"),
        // Ohne veränderlichen Teil: Der Befehl trägt keine Nutzlast. Die
        // Idempotenz hängt an der Vorgangs-Kennung — ein zweiter, bewusster
        // Druck bringt eine neue mit und wird deshalb ausgeführt.
        A::ReloadTablets => "reload-tablets".to_string(),
        // Das Ziel gehört hinein: „auf Feld 3" und „aufheben" sind zwei
        // verschiedene Absichten und dürfen einander nicht als „schon
        // erledigt" gelten.
        A::SetWishCourt { match_id, court_id } => {
            format!("wish:{match_id}:{}", court_id.unwrap_or(0))
        }
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
        A::AnnounceCourtCall { court_id, side, .. } => match side {
            Some(relay_proto::PrepCallSide::Team1) => {
                format!("Erneuter Aufruf Feld {court_id}, Partei A")
            }
            Some(relay_proto::PrepCallSide::Team2) => {
                format!("Erneuter Aufruf Feld {court_id}, Partei B")
            }
            _ => format!("Erneuter Aufruf Feld {court_id}"),
        },
        A::AnnouncePrepCall { .. } => "Erneuter Vorbereitungs-Aufruf".to_string(),
        A::SetHallPrefill { enabled, window } => format!(
            "Hallen-Vorverteilung {} (x={window})",
            if *enabled { "an" } else { "aus" }
        ),
        A::ClearAutoHalls => "Auto-Hallen räumen".to_string(),
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
        A::AnnounceScorekeeper { court_id } => format!("Bediener-Nachruf Feld {court_id}"),
        A::AnnounceStartPlay { court_id, .. } => {
            format!("Spielbeginn-Ansage Feld {court_id}")
        }
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
        A::ReloadTablets => "Alle Tablets laden neu".to_string(),
        A::LockCourt { court_id, locked } => {
            if *locked {
                format!("Feld {court_id} gesperrt")
            } else {
                format!("Feld {court_id} freigegeben")
            }
        }
        A::SetWishCourt { match_id, court_id } => match court_id {
            Some(c) => format!("Spiel {match_id} wünscht Feld {c}"),
            None => format!("Spiel {match_id} ohne Wunschfeld"),
        },
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
    /// Kennt dieser Turnier-PC das Sperren von Feldern (Spec
    /// `tl-web-felder-sperren`, E13)?
    ///
    /// **Warum das nötig ist:** Der Relay bettet `assets/tl.html` ein und wird
    /// bei jedem main-Merge deployt; die App kommt erst über einen
    /// Release-Tag. Zwischen beidem sieht ein Gerät die neue Oberfläche, spricht
    /// aber mit einem älteren Host — und der verwirft eine unbekannte
    /// `TlAction` **still** (`relay_client.rs`), ohne Fehlermeldung. Die
    /// Oberfläche zeigt den Menüeintrag deshalb nur, wenn dieses Feld ankommt.
    ///
    /// `#[serde(default)]` = `false`: Genau das liefert ein alter Host.
    #[serde(default)]
    pub can_lock_courts: bool,
    /// Kennt dieser Turnier-PC das Wunschfeld (Spec `tl-wunschfeld`)?
    /// Dieselbe Begründung wie bei [`Self::can_lock_courts`]: Ein älterer
    /// Host verwirft die unbekannte Aktion still, und die Turnierleitung
    /// glaubte, das Endspiel sei gesteuert.
    #[serde(default)]
    pub can_set_wish_court: bool,
    /// Kennt dieser Turnier-PC den Fernbefehl „alle Tablets neu laden"
    /// (Spec `tablet-version-abgleich`)? Gleiche Begründung wie bei
    /// [`Self::can_lock_courts`]: Ein älterer Host verwirft die unbekannte
    /// Aktion still — die Turnierleitung drückte auf einen toten Knopf und
    /// hielte die Tablets für aufgefrischt.
    #[serde(default)]
    pub can_reload_tablets: bool,
    /// Nach wie vielen Sekunden ein „fertig aussehendes" Spiel gemeldet wird
    /// (`0` = Warnung aus). Kommt aus der Konfiguration; die Seite rechnet
    /// damit gegen [`TlCourt::decided_since_ms`].
    #[serde(default)]
    pub finished_warning_seconds: u32,
    /// Alle Hallen des Turniers, alphabetisch nach Namen.
    ///
    /// Mit Kennung, nicht nur mit Namen: Ein Vorbereitungs-Aufruf braucht sie,
    /// damit er auf der Meeting-Point-Anzeige **einer** Halle erscheint und
    /// nicht auf allen.
    pub halls: Vec<TlHall>,
    pub auto_assign: TlAutoAssign,
    /// Automatische Hallen-Vorverteilung (Spec `hallen-vorverteilung`).
    /// `#[serde(default)]` hält ältere Gegenstellen kompatibel; fehlt das
    /// Feld (alter Host), zeigt die Seite die Bedienung nicht.
    #[serde(default)]
    pub hall_prefill: Option<TlHallPrefill>,
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
    /// Wie viele wartende Spiele die Kappung weggelassen hat. `0` = die
    /// Liste ist vollständig.
    ///
    /// **Ersetzt `truncated_halls: Vec<String>`** (ADR 0026): Gekappt wird
    /// seit der einen globalen Reihenfolge turnierweit, eine Liste
    /// betroffener Hallen wäre damit keine ehrliche Auskunft mehr — sie
    /// beschriebe eine Grenze, die es nicht mehr gibt. Die Zahl ist das,
    /// was die Turnierleitung wirklich wissen muss: wie viel am Ende
    /// fehlt. (Der frühere Grund für die Trennung je Halle — „eine Halle
    /// darf nicht komplett verdrängt werden" — ist mit der bewusst
    /// hallenübergreifenden Sortierung aus ADR 0026 aufgegeben worden.)
    #[serde(default)]
    pub queue_truncated: usize,
    /// Spiele, deren Paarung noch nicht feststeht (Spec
    /// `tl-offene-paarungen`), in derselben Abfolge wie [`Self::queue`] —
    /// die Seite mischt sie über `queue_index` ein.
    ///
    /// `#[serde(default)]` = leer: Genau das liefert ein alter Host, und
    /// dann zeigt die Seite schlicht keine offenen Spiele.
    #[serde(default)]
    pub open_queue: Vec<TlOpenMatch>,
    /// Wie viele offene Spiele die Kappung weggelassen hat.
    #[serde(default)]
    pub open_queue_truncated: usize,
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
    /// Auswertung der gemessenen Spielzeiten (Spec `spielzeiten-prognose`).
    /// `None`, solange die Prognose ausgeschaltet ist — die Seite zeigt das
    /// Panel dann gar nicht.
    #[serde(default)]
    pub time_stats: Option<TlTimeStats>,
    /// Check-In-Anfangszeiten des heutigen Tages fürs Panel „Anfangszeiten"
    /// (Feldtest 17.08.2026): je Klasse Anfangszeit, Anmeldeschluss,
    /// badhub-Fensterzustand und die Zähler eingecheckt/gemeldet — bewusst
    /// OHNE Spielernamen (Datensparsamkeit; wer Namen braucht, hat die
    /// Desktop-Check-In-Seite). `None`, solange kein Check-In eingerichtet
    /// ist oder badhub den Zugang ablehnt — die Seite zeigt das Panel dann
    /// gar nicht. `#[serde(default)]` wie `time_stats` für alte
    /// Gegenstellen.
    #[serde(default)]
    pub checkin_times: Option<Vec<TlCheckinTime>>,
    /// Stale-Marke zum Anfangszeiten-Panel: `true`, wenn der letzte
    /// erfolgreiche badhub-Abruf länger als fünf Minuten her ist (ein
    /// Offline-Aussetzer lässt den Stand bewusst stehen — aber die Seite
    /// soll ihn nicht als live verkaufen, Review 17.08.2026).
    #[serde(default)]
    pub checkin_stale: bool,
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

/// Zustand der automatischen Hallen-Vorverteilung (Spec
/// `hallen-vorverteilung`) — reine Konfigurations-/Betriebsangaben, keine
/// Personendaten. Alte Hosts liefern das Feld nicht → die Seite blendet
/// die Bedienelemente dann gar nicht ein (Feature-Detection).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlHallPrefill {
    pub enabled: bool,
    /// Konfiguriertes x (0 = automatisch).
    pub window: u32,
    /// Tatsächlich wirksames Fenster (aufgelöster 0-Sentinel + Klemme) —
    /// erspart der Seite die Rechnung.
    pub effective_window: u32,
    /// Tages-Halle gesetzt (E2)? Dann ist die Vorverteilung blockiert und
    /// die Seite graut die Bedienung mit Hinweis aus.
    pub blocked_by_active_hall: bool,
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
    /// Lizenznummern (BTP `MemberID`), **parallel** zu `team1`/`team2`;
    /// leerer String, wo BTP keine führt. Siehe [`TlMatch::team1_ids`] —
    /// seit 18.08.2026 (Spec `tl-sicht-feinschliff`) auch am laufenden
    /// Spiel, damit die Feldkachel auf dieselbe badhub-Spielerseite
    /// verlinkt wie die Warteliste.
    #[serde(default)]
    pub team1_ids: Vec<String>,
    #[serde(default)]
    pub team2_ids: Vec<String>,
    pub sets: Vec<(i64, i64)>,
    /// Seit wann sieht dieses Spiel nach seinen Sätzen fertig aus, ohne dass
    /// ein Ergebnis angekommen wäre (Spec `tl-warnung-fertiges-spiel`)?
    /// `None` = alles normal.
    ///
    /// Bewusst ein **Zeitstempel** und kein fertig ausgewertetes „warnen ja/
    /// nein": Ein Bool, der nach einer Minute umspringt, hinge an der Uhr —
    /// und `state_fingerprint` nullt zeitabgeleitete Felder, damit die
    /// Revision nicht im Sekundentakt hochzählt. Die Warnung käme per Push
    /// nie an. Der Zeitstempel dagegen ist stabil; die Seite rechnet die
    /// Frist selbst, sie hat ohnehin einen Sekundentakt für ihre Uhren.
    #[serde(default)]
    pub decided_since_ms: Option<u64>,
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
    /// Geschätzte Restzeit des laufenden Spiels in Minuten (Spec
    /// `spielzeiten-prognose`, Etappe D): aus dem Live-Stand, wenn das Feld
    /// zählt, sonst Gruppenwert minus verstrichene Zeit. `None` bei freiem
    /// Feld, abgeschalteter Prognose oder altem Host (Serde-Default hält
    /// alte Zustände lesbar). Anzeige hinter dem Schalter
    /// `display.show_court_remaining`.
    #[serde(default)]
    pub remaining_min: Option<u64>,
    /// Wie oft dieses Spiel schon aufgerufen wurde, **gezählt am
    /// Turnier-PC** (0 = noch nie; mit „Aufrufe unbegrenzt" nach oben
    /// offen, sonst maximal 3 — kein `min(…, 3)` daraufsetzen). Nicht zu
    /// verwechseln mit der Fälligkeit aus der Uhr: Die sagt, wann der
    /// nächste Aufruf dran wäre, diese Zahl, wie viele erfolgt sind. Nur
    /// so zeigen zwei Turnierleitungen dieselbe Stufe.
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
    /// Lizenznummern (BTP `MemberID`, z. B. „08-017991"), **parallel** zu
    /// den Namen; leerer String, wo BTP keine führt. Link-Ziel der
    /// badhub-Spielerseite (`badhub.de/spieler/<Nr>/live`). Datenschutz:
    /// bewusst freigegeben (Nutzer-Entscheidung 17.08.2026) — die Nummer
    /// ist der öffentliche URL-Schlüssel genau dieser Seite und steht hier
    /// hinter dem Gerätezugang. Nur die Warteliste; laufende/beendete
    /// Spiele tragen keinen Link und deshalb auch keine Nummer.
    #[serde(default)]
    pub team1_ids: Vec<String>,
    #[serde(default)]
    pub team2_ids: Vec<String>,
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
    /// Gewünschtes Feld (CourtID) für die automatische Vergabe (Spec
    /// `tl-wunschfeld`); `None` = keins. Die Seite löst den Feldnamen selbst
    /// über `courts` auf und sagt damit auch, **warum** ein Spiel wartet —
    /// sonst sähe ein wartendes Wunschspiel in der Liste aus wie jedes
    /// andere spielbereite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wish_court: Option<i64>,
    /// Steht dieses Spiel gerade im manuellen Präfix seiner Halle (Spec
    /// `spielliste-manuelle-reihenfolge`)? Reine Anzeige-Information fürs
    /// Badge in der Liste — die tatsächliche Sortierung liegt bereits in
    /// der Reihenfolge dieser Liste selbst.
    #[serde(default)]
    pub manual: bool,
    /// Voraussichtlicher Aufruf (Spec `spielzeiten-prognose`, E8), Unix-ms,
    /// **minutengerundet** (Rev-Churn-Wächter). `None` = keine Prognose
    /// (Prognose aus, ausgenommenes Spiel oder kein erlaubtes Feld).
    #[serde(default)]
    pub predicted_start_ms: Option<u64>,
    /// Steht hinter der Prognose nur der Config-Default (keine Messwerte)?
    /// Die Seite zeigt dann „~hh:mm" statt „hh:mm" (E7).
    #[serde(default)]
    pub predicted_uncertain: bool,
}

/// Ein Spiel, dessen Paarung noch nicht (vollständig) feststeht — Spec
/// `tl-offene-paarungen`.
///
/// Bewusst **schlanker** als [`TlMatch`]: Ein offenes Spiel kann nicht aufs
/// Feld, nicht gerufen werden und bekommt keine Prognose, also trägt es weder
/// `blocked` noch `prep_call` noch `predicted_start_ms`. Ebenso fehlen
/// **Lizenznummern** — die Nummer ist das Link-Ziel der badhub-Seite eines
/// feststehenden Teilnehmers, ein Kandidat bekommt sie nicht (Datenschutz).
///
/// Die Liste reist getrennt von [`TlState::queue`], damit sie im
/// 64-KiB-Fenster der Cloud keine echten Wartespiele verdrängt (ADR 0051).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlOpenMatch {
    pub match_id: i64,
    pub match_num: Option<i64>,
    pub planned_time: Option<i64>,
    pub draw_name: String,
    pub round_name: String,
    pub class_label: String,
    /// Siehe [`TlCourt::discipline`].
    pub discipline: String,
    /// Die Namen einer bereits feststehenden Seite; leer, wenn der Platz
    /// offen ist. Ein Spiel kann halb offen sein — dann steht hier eine
    /// Seite und im Label die andere.
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Beschriftung des offenen Platzes 1 — Kandidaten des Vorspiels,
    /// „aus Spiel 42" oder „noch offen" ([`offener_platz_text`], ADR 0052).
    /// Leer, wenn die Seite besetzt ist.
    pub open_slot1_label: String,
    pub open_slot2_label: String,
    /// In welche Halle das Spiel gehört, und woher wir das wissen.
    pub hall: String,
    pub hall_source: HallSource,
    /// Wie viele Spiele der **echten** Warteliste vor diesem Eintrag stehen.
    ///
    /// Damit mischt die Seite die beiden Listen zusammen, ohne selbst zu
    /// sortieren — die Reihenfolge bleibt serverseitig verbindlich, so wie
    /// bei [`TlState::queue`] zugesagt.
    pub queue_index: u32,
    /// Steht dieses Spiel im manuellen Präfix (ADR 0053)?
    #[serde(default)]
    pub manual: bool,
    /// Von der automatischen Feldvergabe ausgenommen? Die Angabe greift
    /// erst, wenn die Paarung feststeht — setzen darf man sie schon.
    #[serde(default)]
    pub excluded_from_auto_assign: bool,
    /// Gewünschtes Feld; wie [`Self::excluded_from_auto_assign`] eine
    /// Vorab-Angabe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wish_court: Option<i64>,
}

/// Eine Halle des Turniers.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlHall {
    /// BTP-Kennung des Standorts — nötig für den Vorbereitungs-Aufruf.
    pub id: i64,
    pub name: String,
    /// Effektive Hallen-Farbe (Hex, Spec hallen-farben) — `None` bei
    /// Ein-Hallen-Turnieren und an alten Hosts (Serde-Default hält alte
    /// Zustände lesbar).
    #[serde(default)]
    pub color: Option<String>,
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
    /// Ende der Pause in Server-Zeit. `None` bei der Behandlungspause
    /// (kein Countdown) — die fiel vor Spec `spielzeiten-prognose` (E10)
    /// beim Parse komplett raus und war unsichtbar. Nach Ablauf hält das
    /// Tablet die Pause (E9, ADR 0028) — die Seite zeigt dann „überzogen".
    #[serde(default)]
    pub ends_at_ms: Option<u64>,
    /// Beginn der Pause in Server-Zeit — fürs „seit …" der Behandlungs-
    /// pause. `None` bei alten Tablet-Ständen (Auto-Update-Fenster).
    #[serde(default)]
    pub started_at_ms: Option<u64>,
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
    Playing {
        players: Vec<String>,
        /// [`assign::player_key`]-Schlüssel **parallel** zu `players` —
        /// die Seite färbt damit exakt den betroffenen Namen ein, statt
        /// über den Namen zu raten (zwei gleichnamige Spieler einer
        /// Paarung verschmölzen sonst; Review-Fund 17.08.2026). Alte
        /// Hosts senden das Feld nicht, die Seite fällt dann auf den
        /// Namensvergleich zurück.
        #[serde(default)]
        player_keys: Vec<String>,
    },
    /// Mindestens ein Spieler ist noch in seiner Pause.
    Pause {
        /// Ab wann der Letzte wieder darf — damit der Helfer planen kann,
        /// statt zu raten.
        until_ms: u64,
        players: Vec<String>,
        /// Siehe [`TlBlocked::Playing::player_keys`].
        #[serde(default)]
        player_keys: Vec<String>,
    },
}

impl From<Blocked> for TlBlocked {
    fn from(b: Blocked) -> Self {
        match b {
            Blocked::Playing {
                players,
                player_keys,
            } => TlBlocked::Playing {
                players,
                player_keys,
            },
            Blocked::Pause {
                until_ms,
                players,
                player_keys,
            } => TlBlocked::Pause {
                until_ms,
                players,
                player_keys,
            },
        }
    }
}

/// Wie viele wartende Spiele höchstens ausgeliefert werden — **turnierweit**
/// (ADR 0026; bis dahin galt der Deckel je Halle).
///
/// Bei großen Turnieren stehen mehrere hundert Spiele an; alle zu übertragen
/// kostet bei jedem Abruf und auf jedem Gerät. Die Liste ist nach
/// Dringlichkeit sortiert, die vorderen sind die, um die es geht — was
/// wegfällt, meldet `queue_truncated` ehrlich.
pub(crate) const QUEUE_LIMIT: usize = 120;

/// Wie viele Spiele mit noch offener Paarung höchstens ausgeliefert werden
/// (Spec `tl-offene-paarungen`, ADR 0051).
///
/// Eigener Deckel neben [`QUEUE_LIMIT`], damit offene Spiele die echte
/// Arbeitsliste nicht verdrängen können. Was wegfällt, meldet
/// `open_queue_truncated`. Über die Cloud kommt zusätzlich die adaptive
/// Füllung in [`state_for_relay`] hinzu — dort ist der Platz knapper als im
/// Hallennetz.
pub(crate) const OPEN_QUEUE_LIMIT: usize = 120;

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
    /// Lizenznummern, **parallel** zu `team1`/`team2`. Siehe
    /// [`TlCourt::team1_ids`] — die Beendet-Liste ist die zweite Stelle, an
    /// der die Turnierleitung während des Turniers nachschlägt.
    #[serde(default)]
    pub team1_ids: Vec<String>,
    #[serde(default)]
    pub team2_ids: Vec<String>,
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
    /// Gemessene Bruttozeit (Feldzuweisung → Ergebnis) in ganzen Minuten
    /// (Spec `spielzeiten-prognose`); `None`, wenn nicht gemessen.
    #[serde(default)]
    pub brutto_mins: Option<i64>,
    /// Gemessene Nettozeit (erster Punkt → Ergebnis) in ganzen Minuten.
    #[serde(default)]
    pub netto_mins: Option<i64>,
    /// Halle, in der es lief (Spec hallen-farben: Kürzel + Marke an der
    /// Beendet-Zeile); leer bei Papier-Ergebnissen ohne Feld und bei
    /// Ein-Hallen-Turnieren. Serde-Default hält alte Hosts lesbar.
    #[serde(default)]
    pub hall: String,
}

/// Auswertung der gemessenen Spielzeiten (Spec `spielzeiten-prognose`):
/// Mediane je Klasse × Disziplin. Nur Zahlen und Kürzel — keine
/// Personendaten (Datenschutz-Wächter prüft mit).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlTimeStats {
    /// Auswertung je Klasse × Disziplin — die ursprüngliche und weiterhin
    /// voreingestellte Achse.
    pub rows: Vec<TlTimeStatsRow>,
    /// Dieselben Messwerte nach Klasse, nach Disziplin und nach Halle
    /// geschnitten (Spec `tl-sicht-feinschliff`, Punkt 1). Alle vier reisen
    /// **gemeinsam** mit: Der Zustand entsteht seit ADR 0034 einmal zentral
    /// für alle Geräte, die Achsen-Wahl liegt aber im Profil je Gerät — der
    /// Host kann also gar nicht nur die gewählte liefern. Das Umschalten
    /// ist damit ein reiner Client-Vorgang ohne Rückfrage.
    ///
    /// `by_hall` ist bei Ein-Hallen-Turnieren **leer**; die Seite bietet
    /// die Achse dann nicht an (A1.6).
    #[serde(default)]
    pub by_class: Vec<TlTimeStatsRow>,
    #[serde(default)]
    pub by_discipline: Vec<TlTimeStatsRow>,
    #[serde(default)]
    pub by_hall: Vec<TlTimeStatsRow>,
    /// Turnierweiter Brutto-Median (ab 3 Messwerten), Minuten.
    pub tournament_brutto_mins: Option<i64>,
    /// Konfigurierter Startwert (Minuten) — die Seite erklärt damit die
    /// „~"-Kennzeichnung.
    pub default_mins: i64,
}

/// Eine Zeile der Spielzeiten-Auswertung. Alle Werte Mediane in Minuten.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlTimeStatsRow {
    pub class_label: String,
    pub discipline: String,
    /// Nur auf der Hallen-Achse gefüllt; leer heißt dort „ohne Halle"
    /// (Messwerte von vor der Umstellung). Auf den anderen Achsen immer
    /// leer — und dort wird das Feld deshalb **weggelassen**: Bei drei von
    /// vier Achsen wäre es toter Ballast, und der Zustand ringt in großen
    /// Turnieren um jedes Kilobyte (siehe Kürzungskaskade in
    /// `state_for_relay`). Das geht nur, weil das Feld NEU ist — bei
    /// `class_label`/`discipline` würde es alte Seiten brechen, die sie
    /// unbedingt lesen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hall: String,
    pub count: usize,
    pub brutto_mins: i64,
    pub netto_mins: i64,
    pub diff_mins: i64,
}

/// Ist-Zeiten eines beendeten Spiels für die Beendet-Zeile:
/// `(brutto, netto)` in Minuten. Dieselbe Plausibilitätsregel wie
/// BTP-`Duration` und Statistik (Review 2026-08-16, F7) — ein über Nacht
/// geparktes Spiel zeigt sonst genau hier den Absurdwert, den der Deckel
/// überall sonst unterdrückt. Netto ist auf Brutto geklemmt: Erreicht der
/// erste Score den Host vor dem ersten Sync-Poll, läge der Punktstempel
/// sonst VOR dem Zuweisungsstempel („40 min (netto 43)").
fn finished_times(tablet: &TabletState, match_id: i64) -> Option<(i64, Option<i64>)> {
    let e = tablet.match_times_store().entry(match_id)?;
    let finished = e.finished_ms?;
    let brutto =
        crate::tablet::match_times::plausible_duration_mins(e.first_assigned_ms?, finished)?;
    let netto = e
        .first_point_ms
        .and_then(|fp| crate::tablet::match_times::plausible_duration_mins(fp, finished))
        .map(|n| n.min(brutto));
    Some((brutto, netto))
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
    build_state_limited(tablet, config, now_ms, rev, QUEUE_LIMIT)
}

/// Wie [`build_state`], aber mit vorgegebener Wartelisten-Länge
/// (turnierweit).
///
/// Für den Weg über den Relay: Der legt einen zu großen Zustand gar nicht
/// erst ab, und der Host erfährt davon nichts. Er muss also selbst kürzen —
/// was wegfällt, meldet `queue_truncated` wie immer.
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
            can_set_wish_court: true,
            finished_warning_seconds: config.finished_warning_seconds,
            // Der Host kann es — auch wenn hier noch kein Turnier steht. Das
            // Merkmal beschreibt die Fähigkeit, nicht die Lage; ohne Turnier
            // lehnt die Aktion ohnehin ab (E7).
            can_lock_courts: true,
            can_reload_tablets: true,
            halls: Vec::new(),
            auto_assign: auto_assign_view(config, tablet.auto_assign_paused()),
            hall_prefill: Some(hall_prefill_view(config, 0, false)),
            call_timer: call_timer_view(config),
            rest_minutes: None,
            courts: Vec::new(),
            queue: Vec::new(),
            walkovers: Vec::new(),
            queue_truncated: 0,
            open_queue: Vec::new(),
            open_queue_truncated: 0,
            scorekeeper_managed: config.scorekeeper.enabled,
            scorekeepers: Vec::new(),
            finished: Vec::new(),
            time_stats: None,
            checkin_times: None,
            checkin_stale: false,
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
    // Match-ID → Spiel, einmal für den ganzen Bau. Zwei Nutzer: Die
    // Feldkachel braucht den Spieler-Datensatz für die Lizenznummern
    // (`CourtOverview` führt sie bewusst nicht mit — sie geht auch an
    // Tablet und Court-Monitor), und der Prognose-Block weiter unten
    // suchte bisher je belegtem Feld linear über alle Spiele. Bei einem
    // großen Turnier sind das ein Aufbau gegen bis zu 26 volle Scans.
    let match_by_id: std::collections::HashMap<i64, &crate::btp::model::BtpMatch> =
        snap.matches.iter().map(|m| (m.id, m)).collect();

    let mut courts: Vec<TlCourt> = tablet
        .overview_from(&snap)
        .into_iter()
        .map(|c| {
            let clearing = clearing_match(&snap, c.court_id, c.match_id);
            let schalter = tablet.officials_store().court_switches(c.court_id);
            // `match_id == 0` heißt „Feld frei" und ist keine Kennung —
            // ohne die Wache bekäme jedes freie Feld die Nummern eines
            // (theoretischen) Spiels mit der ID 0, bei leerer Namensliste.
            let spiel = (c.match_id != 0)
                .then(|| match_by_id.get(&c.match_id).copied())
                .flatten();
            // Nur für belegte Felder nachschlagen — ein freies Feld hat
            // keinen Spielstand, der fertig aussehen könnte.
            let fertig_seit = (c.match_id != 0)
                .then(|| tablet.match_times_store().decided_seen_ms(c.match_id))
                .flatten();
            court_view(c, clearing, schalter, spiel, fertig_seit)
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

    // Die von Hand gesetzten und automatisch vorverteilten Hallen einmal
    // holen, nicht je Spiel — sonst sperrte der Aufbau der Liste
    // hundertfach.
    let manual = tablet.manual_halls();
    let auto = tablet.auto_hall_store().halls();

    let availability = PlayerAvailability::from_snapshot(&snap, config);

    // Alle geplanten Spiele — auch die, deren Paarung noch aus einem Vorspiel
    // kommt (Spec `tl-offene-paarungen`). Sie werden gleich unten von den
    // spielbereiten getrennt: Vergeben kann man sie nicht, anzeigen schon.
    // Erst nur die Ordnungsschlüssel sammeln — die teuren Zeichenketten
    // entstehen später und nur für die Spiele, die auch ausgeliefert werden.
    let mut ordered: Vec<OrderedMatch> = snap
        .matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::Scheduled)
        .map(|m| {
            let call = called_hall(m.id);
            let manual_hall = manual.get(&m.id).map(String::as_str);
            let called_hall_str = call.as_ref().map(|(h, _)| h.as_str());
            // Dieselbe Auflösung wie in der Vergabe (Spec `tl-wunschfeld`):
            // Zeigte die Liste eine andere Halle als die Automatik benutzt,
            // verlöre die Turnierleitung das Vertrauen in beide.
            let manual_hall =
                assign::manual_hall_from_wish(&snap, tablet.wish_court(m.id), manual_hall);
            let (hall, hall_source, key) = assign::resolve_and_sort_key(
                config,
                &snap,
                m,
                manual_hall,
                called_hall_str,
                auto.get(&m.id).map(String::as_str),
                call.is_some(),
                tablet.queue_order_store(),
            );
            (key, m, hall, hall_source)
        })
        .collect();
    ordered.sort_by_key(|(key, _, _, _)| *key);

    // Offene Paarungen aus der Arbeitsliste heraustrennen (ADR 0051). Sie
    // reisen in einer eigenen, eigens gedeckelten Liste, damit sie im
    // 64-KiB-Fenster der Cloud kein echtes Wartespiel verdrängen. Der
    // mitgezählte Index hält fest, an welcher Stelle der GEMEINSAMEN
    // Reihenfolge sie stehen — danach mischt die Seite sie wieder ein.
    //
    // Alles unterhalb dieser Zeile rechnet weiter nur mit den spielbereiten
    // Spielen: Feldvergabe, Prognose und Blockade-Marken ergeben für ein
    // Spiel ohne Teilnehmer keinen Sinn.
    let mut offene: Vec<(usize, OrderedMatch)> = Vec::new();
    let mut spielbereit: Vec<OrderedMatch> = Vec::new();
    for eintrag in ordered {
        if eintrag.1.team1.is_empty() || eintrag.1.team2.is_empty() {
            offene.push((spielbereit.len(), eintrag));
        } else {
            spielbereit.push(eintrag);
        }
    }
    let ordered = spielbereit;

    // Startzeit-Prognose (Spec `spielzeiten-prognose`, E5–E8): Statistik
    // aus dem Zeiten-Store + deterministische Simulation der Warteliste —
    // hier und nicht im Sync-Loop, damit Prognose, Felder und Liste aus
    // DEMSELBEN Snapshot stammen und LAN wie Cloud identisch anzeigen (R3).
    // Alles minutengranular, damit der Fingerprint unten nicht jede
    // Sekunde kippt (Rev-Churn-Wächter).
    let stats = config
        .prediction
        .enabled
        .then(|| tablet.cached_time_stats());
    // Einmal aus dem Store holen, beide Felder (Zeilen + Stale-Marke)
    // bedienen — nicht zweimal klonen (Review 17.08.2026).
    let checkin = tablet.checkin_classes();
    let predictions: std::collections::HashMap<i64, predict::Prediction> = match &stats {
        Some(stats) => {
            let default_mins = config.prediction.default_duration_mins;
            let now_min = now_ms / 60_000;
            let rest_min = effective_rest_minutes(&snap, config).unwrap_or(0).max(0) as u64;
            let buffer_min = predict::effective_buffer_min(
                config.auto_assign.enabled,
                config.auto_assign.wait_minutes,
            );
            // Felder: gesperrte kommen nicht in die Vergabe-Rotation —
            // ihre SPIELER sind aber trotzdem gebunden (Review 2026-08-16,
            // F5: wer auf einem gesperrten Feld mitten im Spiel steht, ist
            // nicht „gleich dran"). Belegte Felder werden frei nach
            // max(0, Gruppenwert − verstrichen); die Spieler laufender
            // Spiele sind ab dann (+ Mindestpause) wieder einsatzbereit.
            let mut sim_courts: Vec<predict::PredictCourt> = Vec::new();
            let mut player_ready: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for c in courts.iter_mut() {
                let free_at_min = if c.match_id != 0 {
                    let times = stats.group_times(&c.class_label, &c.discipline, default_mins);
                    let (since, first_point) =
                        tablet.court_time_stamps(c.match_id, Some(c.court_id));
                    // Live-Restzeit (Etappe D), sobald das Feld wirklich
                    // zählt (Tablet/Zähltafel verbunden oder es kamen schon
                    // Punkte an) — sonst wie bisher Gruppenwert minus
                    // verstrichene Zeit. BTP-Satzstände laufender Spiele
                    // gibt es nicht, das Gate schützt also vor einem
                    // Modell ohne Datengrundlage.
                    let remaining = if c.tablet_connected || first_point.is_some() {
                        predict::live_remaining_min(&predict::LiveRemainInput {
                            now_ms,
                            sets: c.sets.clone(),
                            best_of: c.best_of,
                            target: c.target_score,
                            cap: c.cap_score,
                            first_assigned_ms: since,
                            first_point_ms: first_point,
                            netto_median_min: times.netto_min,
                            brutto_median_min: times.brutto_min,
                        })
                    } else {
                        // „~0 min Rest" wäre eine verwirrende Anzeige —
                        // Untergrenze 1 wie im Live-Modell.
                        let elapsed = since
                            .map(|s| now_min.saturating_sub(s / 60_000))
                            .unwrap_or(0);
                        times.brutto_min.saturating_sub(elapsed).max(1)
                    };
                    c.remaining_min = Some(remaining);
                    now_min + remaining
                } else {
                    now_min
                };
                if c.match_id != 0 {
                    // Über dieselbe Map wie die Feldkacheln oben: vorher
                    // war das ein linearer Scan über alle Spiele JE
                    // belegtem Feld (Review 18.08.2026).
                    if let Some(m) = match_by_id.get(&c.match_id) {
                        for p in m.team1.iter().chain(m.team2.iter()) {
                            player_ready.insert(assign::player_key(p), free_at_min + rest_min);
                        }
                    }
                }
                if !c.locked {
                    sim_courts.push(predict::PredictCourt {
                        hall: c.location.clone(),
                        free_at_min,
                    });
                }
            }
            // Bestehende Mindestpausen-Blocker (Spieler ruht noch nach
            // seinem letzten Spiel) als Bereitschafts-Untergrenze.
            let mut sim_queue: Vec<predict::PredictMatch> = Vec::new();
            for (_, m, hall, _) in ordered.iter().take(queue_limit) {
                if let Some(Blocked::Pause { until_ms, .. }) = availability.blocked(m, now_ms) {
                    let until_min = until_ms / 60_000;
                    for p in m.team1.iter().chain(m.team2.iter()) {
                        let e = player_ready.entry(assign::player_key(p)).or_insert(0);
                        *e = (*e).max(until_min);
                    }
                }
                // Ausgenommene Spiele überspringt die Vergabe wirklich —
                // sie belegen kein Feld und bekommen keine Prognose.
                if tablet.auto_assign_excluded(m.id) {
                    continue;
                }
                // Ebenso Spiele mit Wunschfeld (Spec tl-wunschfeld): Die
                // Simulation kennt nur Hallen, keine Felder. Sie gäbe dem
                // Spiel die Startzeit des ERSTEN freien Felds — also
                // systematisch zu früh — und belegte in der Rechnung ein
                // Feld, das es real nicht nimmt; alle nachfolgenden
                // Startzeiten verschöben sich mit. Lieber keine Prognose als
                // eine falsche, die auch die der anderen verdirbt.
                if tablet.wish_court(m.id).is_some() {
                    continue;
                }
                let (duration_min, uncertain) =
                    stats.group_duration(&m.class_label, m.discipline.as_str(), default_mins);
                sim_queue.push(predict::PredictMatch {
                    match_id: m.id,
                    hall: hall.clone(),
                    duration_min,
                    uncertain,
                    players: m
                        .team1
                        .iter()
                        .chain(m.team2.iter())
                        .map(assign::player_key)
                        .collect(),
                });
            }
            let predictions = predict::predict_starts(&predict::PredictInput {
                now_min,
                buffer_min,
                rest_min,
                courts: sim_courts,
                player_ready_min: player_ready,
                queue: sim_queue,
            });
            // Fürs Diagnose-Log merken (Prognose-Kontrolle, E12): Beim
            // echten Aufruf vergleicht der Sync-Loop Prognose und
            // Wirklichkeit. Gemergt, nicht ersetzt — die Relay-Leiter baut
            // denselben Zustand mit kleineren Wartelisten (F6).
            tablet.merge_predicted_starts(
                predictions
                    .iter()
                    .map(|(id, p)| (*id, p.start_min * 60_000))
                    .collect(),
            );
            predictions
        }
        None => std::collections::HashMap::new(),
    };

    // Turnierweit kappen, nicht je Halle (ADR 0026) — die Liste ist eine
    // einzige Abfolge, also gibt es auch nur eine Grenze.
    let queue_truncated = ordered.len().saturating_sub(queue_limit);
    let mut queue: Vec<TlMatch> = Vec::new();
    for (_, m, hall, hall_source) in ordered.into_iter().take(queue_limit) {
        let call = called_hall(m.id);
        let manually_ordered = tablet.queue_order_store().rank(m.id).is_some();
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
            team1_ids: license_ids(&m.team1),
            team2_ids: license_ids(&m.team2),
            hall,
            hall_source,
            prep_call: call.map(|(hall, called_at_ms)| TlPrepCall {
                hall,
                called_at_ms,
                recalls: tablet.prep_calls_made(m.id),
            }),
            blocked: availability.blocked(m, now_ms).map(TlBlocked::from),
            excluded_from_auto_assign: tablet.auto_assign_excluded(m.id),
            wish_court: tablet.wish_court(m.id),
            manual: manually_ordered,
            predicted_start_ms: predictions.get(&m.id).map(|p| p.start_min * 60_000),
            predicted_uncertain: predictions.get(&m.id).is_some_and(|p| p.uncertain),
        });
    }

    // Die offenen Spiele — schlank, ohne Vergabe-, Aufruf- und
    // Prognose-Felder (Spec `tl-offene-paarungen`).
    let open_queue_truncated = offene.len().saturating_sub(OPEN_QUEUE_LIMIT);
    let mut open_queue: Vec<TlOpenMatch> = Vec::new();
    for (queue_index, (_, m, hall, hall_source)) in offene.into_iter().take(OPEN_QUEUE_LIMIT) {
        open_queue.push(TlOpenMatch {
            match_id: m.id,
            match_num: m.match_num,
            planned_time: m.planned_time,
            draw_name: m.draw_name.clone(),
            round_name: m.round_name.clone(),
            class_label: m.class_label.clone(),
            discipline: m.discipline.as_str().to_string(),
            team1: m.team1.iter().map(|p| p.name.clone()).collect(),
            team2: m.team2.iter().map(|p| p.name.clone()).collect(),
            // Nur der offene Platz bekommt ein Label; eine feststehende Seite
            // steht mit ihren echten Namen da.
            open_slot1_label: if m.team1.is_empty() {
                offener_platz_text(&snap, m, 1)
            } else {
                String::new()
            },
            open_slot2_label: if m.team2.is_empty() {
                offener_platz_text(&snap, m, 2)
            } else {
                String::new()
            },
            hall,
            hall_source,
            queue_index: u32::try_from(queue_index).unwrap_or(u32::MAX),
            manual: tablet.queue_order_store().rank(m.id).is_some(),
            excluded_from_auto_assign: tablet.auto_assign_excluded(m.id),
            wish_court: tablet.wish_court(m.id),
        });
    }

    let mut halls: Vec<TlHall> = snap
        .locations
        .iter()
        .filter(|l| !l.name.trim().is_empty())
        .map(|l| TlHall {
            id: l.id,
            name: l.name.trim().to_string(),
            color: None,
        })
        .collect();
    halls.sort_by_key(|h| h.name.to_lowercase());
    halls.dedup_by(|a, b| a.name == b.name);
    // Hallen-Farben (Spec hallen-farben): der Resolver liefert bei < 2
    // Hallen nichts — Ein-Hallen-Turniere bleiben farblos.
    let hall_names: Vec<String> = halls.iter().map(|h| h.name.clone()).collect();
    let hallen_farben = crate::hall_colors::effective_hall_colors(config, &hall_names);
    for h in &mut halls {
        h.color = crate::hall_colors::farbe_fuer(&hallen_farben, &h.name);
    }

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
        .map(|m| {
            let zeiten = finished_times(tablet, m.id);
            TlFinished {
                match_id: m.id,
                match_num: m.match_num.unwrap_or(0),
                draw_name: m.draw_name.clone(),
                round_name: m.round_name.clone(),
                class_label: m.class_label.clone(),
                discipline: m.discipline.as_str().to_string(),
                team1: m.team1.iter().map(|p| p.name.clone()).collect(),
                team2: m.team2.iter().map(|p| p.name.clone()).collect(),
                team1_ids: license_ids(&m.team1),
                team2_ids: license_ids(&m.team2),
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
                brutto_mins: zeiten.map(|(b, _)| b),
                netto_mins: zeiten.and_then(|(_, n)| n),
                // Leer bei Ein-Hallen-Turnieren und ohne Feld — genau wie
                // `location` in der Felder-Übersicht.
                hall: m
                    .court_id
                    .map(|cid| snap.court_location_name(cid))
                    .unwrap_or_default(),
            }
        })
        .collect();

    TlState {
        // Dieser Host kann es — die Oberfläche darf den Eintrag zeigen.
        can_lock_courts: true,
        can_set_wish_court: true,
        can_reload_tablets: true,
        finished_warning_seconds: config.finished_warning_seconds,
        rev,
        server_now_ms: now_ms,
        tournament: snap.tournament_name.clone(),
        multi_hall: snap.is_multi_hall(),
        halls,
        auto_assign: auto_assign_view(config, tablet.auto_assign_paused()),
        hall_prefill: Some(hall_prefill_view(config, snap.court_infos.len(), {
            let active = config.auto_assign.active_hall.trim();
            !active.is_empty()
                && snap.is_multi_hall()
                && snap
                    .locations
                    .iter()
                    .any(|l| l.name.trim().eq_ignore_ascii_case(active))
        })),
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
        queue_truncated,
        open_queue,
        open_queue_truncated,
        officials_managed,
        officials,
        scorekeeper_managed,
        scorekeepers,
        finished,
        time_stats: stats.as_ref().map(|stats| TlTimeStats {
            rows: stats_zeilen(stats.rows()),
            by_class: stats_zeilen(stats.rows_class()),
            by_discipline: stats_zeilen(stats.rows_discipline()),
            // Ein-Hallen-Turniere bekommen die Achse gar nicht erst — dort
            // gäbe es genau eine Zeile „ohne Halle", und die sagt nichts.
            by_hall: if snap.is_multi_hall() {
                stats_zeilen(stats.rows_hall())
            } else {
                Vec::new()
            },
            tournament_brutto_mins: stats.tournament_brutto_min().map(|v| v as i64),
            default_mins: config.prediction.default_duration_mins.round() as i64,
        }),
        // „Heute" aus dem injizierten now_ms (`heutiges_datum`) — der
        // Tageswechsel bewegt den Fingerprint genau einmal um Mitternacht.
        checkin_times: checkin
            .as_ref()
            .map(|(_, classes)| checkin_times_heute(classes, heutiges_datum(now_ms))),
        // Stale ab fünf Minuten ohne erfolgreichen Abruf: deutlich über
        // dem Minuten-Lese-Takt, also nie im Normalbetrieb — und die
        // Marke kippt genau einmal, kein Rev-Churn.
        checkin_stale: checkin
            .as_ref()
            .is_some_and(|(geholt, _)| geholt.elapsed() > std::time::Duration::from_secs(5 * 60)),
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
                collapsed: s.collapsed,
                column: s.column,
            })
            .collect(),
        columns: p.columns,
        column_widths: p.column_widths.clone(),
        display: relay_proto::TlDisplaySettingsWire {
            show_numbers: p.display.show_numbers,
            show_nations: p.display.show_nations,
            show_club_names: p.display.show_club_names,
            show_club_logos: p.display.show_club_logos,
            show_discipline: p.display.show_discipline,
            show_round: p.display.show_round,
            show_group: p.display.show_group,
            show_court_remaining: p.display.show_court_remaining,
            hide_open_matches: p.display.hide_open_matches,
            unlimited_court_calls: p.display.unlimited_court_calls,
            list_position: match p.display.list_position {
                crate::config::TlListPosition::Right => relay_proto::TlListPositionWire::Right,
                crate::config::TlListPosition::Bottom => relay_proto::TlListPositionWire::Bottom,
            },
            time_stats_axis: match p.display.time_stats_axis {
                crate::config::TlTimeStatsAxis::Group => relay_proto::TlTimeStatsAxisWire::Group,
                crate::config::TlTimeStatsAxis::Class => relay_proto::TlTimeStatsAxisWire::Class,
                crate::config::TlTimeStatsAxis::Discipline => {
                    relay_proto::TlTimeStatsAxisWire::Discipline
                }
                crate::config::TlTimeStatsAxis::Hall => relay_proto::TlTimeStatsAxisWire::Hall,
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
            collapsed: s.collapsed,
            column: s.column,
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
        show_court_remaining: d.show_court_remaining,
        hide_open_matches: d.hide_open_matches,
        unlimited_court_calls: d.unlimited_court_calls,
        list_position: match d.list_position {
            relay_proto::TlListPositionWire::Right => crate::config::TlListPosition::Right,
            relay_proto::TlListPositionWire::Bottom => crate::config::TlListPosition::Bottom,
        },
        time_stats_axis: match d.time_stats_axis {
            relay_proto::TlTimeStatsAxisWire::Group => crate::config::TlTimeStatsAxis::Group,
            relay_proto::TlTimeStatsAxisWire::Class => crate::config::TlTimeStatsAxis::Class,
            relay_proto::TlTimeStatsAxisWire::Discipline => {
                crate::config::TlTimeStatsAxis::Discipline
            }
            relay_proto::TlTimeStatsAxisWire::Hall => crate::config::TlTimeStatsAxis::Hall,
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

/// Höchstzahl der Spaltenbreiten in EINEM Profil. Das Frontend kennt drei
/// feste Presets (1…3 Spalten); die Reserve fängt eine künftige vierte
/// Spalte ab, ohne dass eine beliebig lange Zahlenliste den `TlState`
/// sprengen kann (R4) — dieselbe Überlegung wie bei
/// [`MAX_TL_PROFILE_PANELS`], nur ist hier die Liste noch kürzer.
const MAX_TL_PROFILE_COLUMN_WIDTHS: usize = 8;

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
    // Spaltenbreiten: dieselbe Überlegung wie bei den Panels — eine feste,
    // im Frontend bekannte Menge (höchstens drei Spalten), also ist eine
    // lange Liste ein Protokollfehler und kein Bedienfall.
    if profile.column_widths.len() > MAX_TL_PROFILE_COLUMN_WIDTHS {
        return Err(TlResponse::err(
            C::NotAllowed,
            format!(
                "Ein Profil kann höchstens {MAX_TL_PROFILE_COLUMN_WIDTHS} Spaltenbreiten führen."
            ),
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
        // Spaltenzahl/-breiten reist der Host nur durch (wie `collapsed`):
        // Was 1…3 bedeutet und wie `0` aus `list_position` abgeleitet wird,
        // weiß ausschließlich `tl.html`.
        columns: profile.columns,
        column_widths: profile.column_widths.clone(),
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
    match assign::pflichtpause_ms(snap, config) {
        0 => None,
        ms => Some((ms / 60_000) as i64),
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

/// Zustand der Hallen-Vorverteilung für die Anzeige (Spec
/// `hallen-vorverteilung`). `total_courts`/`active_hall_known` kommen aus
/// dem Snapshot — ohne Turnier gilt das Fenster als unaufgelöst (0 Felder
/// ⇒ Klemme auf 1) und keine Tages-Halle als auflösbar.
fn hall_prefill_view(
    config: &AppConfig,
    total_courts: usize,
    active_hall_known: bool,
) -> TlHallPrefill {
    TlHallPrefill {
        enabled: config.hall_prefill.enabled,
        window: config.hall_prefill.window,
        effective_window: crate::tablet::hall_assign::effective_window(
            config.hall_prefill.window,
            total_courts,
        ) as u32,
        blocked_by_active_hall: active_hall_known,
    }
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

/// Lizenznummern einer Mannschaft, **stellungsgleich** zu den Namen: Wo BTP
/// keine Nummer führt, steht ein leerer String statt gar nichts — sonst
/// rutschte im Doppel der Link auf den falschen Partner. Einzige Ableitung
/// dieser Art im Zustand; Warteliste, Feldkachel und Beendet-Liste teilen
/// sie sich, damit die Stellungsgleichheit nicht an drei Orten gepflegt
/// werden muss.
fn license_ids(players: &[crate::btp::model::BtpPlayer]) -> Vec<String> {
    players
        .iter()
        .map(|p| p.member_id.clone().unwrap_or_default())
        .collect()
}

/// Statistik-Zeilen in die Wire-Form. Eine Stelle für alle vier Achsen —
/// sie unterscheiden sich nur darin, welche Schlüsselfelder gefüllt sind.
fn stats_zeilen(rows: &[crate::tablet::predict::StatsRow]) -> Vec<TlTimeStatsRow> {
    rows.iter()
        .map(|r| TlTimeStatsRow {
            class_label: r.class_label.clone(),
            discipline: r.discipline.clone(),
            hall: r.hall.clone(),
            count: r.count,
            brutto_mins: r.brutto_min as i64,
            netto_mins: r.netto_min as i64,
            diff_mins: r.diff_min as i64,
        })
        .collect()
}

/// Beschneidet die Feld-Übersicht auf das, was die Turnierleitung braucht.
///
/// Bewusst **weggelassen**: Akkustand (keine Geräte-Übersicht in diesem
/// Feature) und die Aufschlag-Anzeige (Zählhilfe, keine Vergabehilfe).
///
/// Mitgegeben, weil die Turnierleitung sie braucht: **Nationalitäten** und
/// **Vereinsnamen** (zuschaltbare Anzeige, Freigaben 09./12.08.2026) sowie
/// die **Lizenznummern** als badhub-Link-Ziel (Freigabe 17.08.2026,
/// ausgeweitet 18.08.2026). Letztere kommen nicht aus `CourtOverview` —
/// die geht auch an Tablet und Court-Monitor, wo die Nummer nichts zu
/// suchen hat —, sondern aus dem BTP-Spiel des Felds.
fn court_view(
    c: crate::tablet::state::CourtOverview,
    clearing: Option<i64>,
    schalter: crate::tablet::officials::CourtSwitches,
    // Das Spiel auf dem Feld, nur für die Lizenznummern — `CourtOverview`
    // führt sie bewusst nicht mit. `None` bei freiem Feld.
    spiel: Option<&crate::btp::model::BtpMatch>,
    // Seit wann sieht das Spiel fertig aus (Spec `tl-warnung-fertiges-spiel`)?
    // Kommt aus dem persistenten Zeitspeicher, den der Sync-Lauf stempelt.
    decided_since_ms: Option<u64>,
) -> TlCourt {
    // Aus dem rohen Tablet-JSON nur die bekannten Angaben übernehmen.
    // Alles andere bliebe ungeprüfter Fremdinhalt auf einer aus dem Internet
    // erreichbaren Seite. `endsAt` ist bei der Behandlungspause null (E10)
    // und `startedAt` bei alten Tablets nicht vorhanden — beides optional,
    // die Pause selbst kommt trotzdem an.
    let pause = c.pause.as_ref().and_then(|v| {
        Some(TlPause {
            kind: v.get("kind")?.as_str()?.to_string(),
            ends_at_ms: v.get("endsAt").and_then(serde_json::Value::as_u64),
            started_at_ms: v.get("startedAt").and_then(serde_json::Value::as_u64),
        })
    });
    TlCourt {
        clearing,
        decided_since_ms,
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
        team1_ids: spiel.map(|m| license_ids(&m.team1)).unwrap_or_default(),
        team2_ids: spiel.map(|m| license_ids(&m.team2)).unwrap_or_default(),
        sets: c.sets,
        tablet_connected: c.tablet_connected,
        injury: c.injury,
        official_call: c.official_call,
        scorekeeper: c.scorekeeper,
        scorekeeper_assigned: c.scorekeeper_assigned,
        locked: c.locked,
        on_court_since_ms: c.on_court_since_ms,
        // Wird — falls die Prognose an ist — beim Simulation-Bau gefüllt.
        remaining_min: None,
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

    /// Spieler mit Lizenznummer. Für den Datensparsamkeits-Test (die Nummer
    /// reist als badhub-Link-Ziel in der Warteliste — Freigabe 17.08.2026 —
    /// sowie an laufenden und beendeten Spielen, Ausweitung 18.08.2026)
    /// sowie überall dort, wo ein realistisches Fixture die Nummern
    /// mitwiegen muss (Relay-Größen-Wächter).
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
            pause_ms: None,
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

    /// Ein Spiel, dessen Platz aus einem Vorspiel gespeist wird: `from1`
    /// zeigt auf die PlanningID des Vorspiels im selben Draw.
    fn folgespiel(id: i64, feeder1: Option<i64>, feeder2: Option<i64>) -> BtpMatch {
        BtpMatch {
            from1: feeder1,
            from2: feeder2,
            team1: Vec::new(),
            team2: Vec::new(),
            round_name: "HF".to_string(),
            ..a_match(id)
        }
    }

    #[test]
    fn ein_offener_platz_nennt_die_kandidaten_des_vorspiels() {
        // Der eigentliche Nutzen: Die Turnierleitung sieht, WER gleich
        // gebraucht wird, bevor die Paarung feststeht.
        let mut vorspiel = a_match(42);
        vorspiel.planning_id = 1001;
        vorspiel.team1 = vec![player("Müller")];
        vorspiel.team2 = vec![player("Schmidt")];
        let folge = folgespiel(80, Some(1001), None);
        let s = snap(Vec::new(), vec![vorspiel, folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "Müller oder Schmidt");
    }

    #[test]
    fn kandidaten_werden_mit_oder_getrennt_das_doppel_mit_schraegstrich() {
        // Der Schrägstrich gehört dem Doppelpaar — sonst wäre nicht
        // erkennbar, ob vier Namen zwei Paare oder vier Kandidaten sind.
        let mut vorspiel = a_match(42);
        vorspiel.planning_id = 1001;
        vorspiel.team1 = vec![player("Müller"), player("Meier")];
        vorspiel.team2 = vec![player("Schmidt"), player("Klein")];
        let folge = folgespiel(80, Some(1001), None);
        let s = snap(Vec::new(), vec![vorspiel, folge.clone()], Vec::new());

        assert_eq!(
            offener_platz_text(&s, &folge, 1),
            "Müller/Meier oder Schmidt/Klein"
        );
    }

    #[test]
    fn ein_noch_offenes_vorspiel_faellt_auf_aus_spiel_nummer_zurueck() {
        // Zwei Runden vor Schluss steht noch niemand fest — dann hilft
        // wenigstens die Spielnummer, nach der die Turnierleitung sucht.
        let mut vorspiel = folgespiel(42, None, None);
        vorspiel.planning_id = 1001;
        vorspiel.match_num = Some(42);
        let folge = folgespiel(80, Some(1001), None);
        let s = snap(Vec::new(), vec![vorspiel, folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "aus Spiel 42");
    }

    #[test]
    fn die_beschriftung_sagt_nie_sieger_denn_auch_der_verlierer_speist() {
        // Bei Platzierungsspielen („3/4") füllt der VERLIERER den Platz.
        // Welche Seite es ist, sagt BTP nicht — also behaupten wir es nicht.
        let mut vorspiel = folgespiel(42, None, None);
        vorspiel.planning_id = 1001;
        vorspiel.match_num = Some(42);
        let mut folge = folgespiel(80, Some(1001), None);
        folge.round_name = "3/4".to_string();
        let s = snap(Vec::new(), vec![vorspiel, folge.clone()], Vec::new());

        let text = offener_platz_text(&s, &folge, 1);
        assert!(
            !text.contains("Sieger") && !text.contains("Verlierer"),
            "die Herkunft wird neutral benannt, war aber: {text}"
        );
    }

    #[test]
    fn ein_vorspiel_ohne_spielnummer_heisst_noch_offen() {
        // Ohne Nummer trägt „aus Spiel" keine Information mehr.
        let mut vorspiel = folgespiel(42, None, None);
        vorspiel.planning_id = 1001;
        vorspiel.match_num = None;
        let folge = folgespiel(80, Some(1001), None);
        let s = snap(Vec::new(), vec![vorspiel, folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "noch offen");
    }

    #[test]
    fn ohne_auffindbares_vorspiel_heisst_der_platz_noch_offen() {
        // Setzplatz, Freilos oder Speisung über Draw-Grenzen: `from1` zeigt
        // auf einen Slot, zu dem es kein Spiel gibt. Am Mitschnitt ist das
        // der HÄUFIGSTE Fall (34 von 42 offenen Plätzen).
        let folge = folgespiel(80, Some(9999), None);
        let s = snap(Vec::new(), vec![folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "noch offen");
    }

    #[test]
    fn ein_platz_ohne_from_kante_heisst_noch_offen() {
        let folge = folgespiel(80, None, None);
        let s = snap(Vec::new(), vec![folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "noch offen");
    }

    #[test]
    fn ein_slot_mit_gleicher_planning_id_aus_einem_fremden_draw_zaehlt_nicht() {
        // PlanningIDs sind nur je Draw eindeutig. Ohne Draw-Bindung löste die
        // Anzeige zu wildfremden Spielern auf — derselbe Fehler, der einmal
        // 95 % aller Teilnehmer eines 116-Draw-Turniers verdreht hat.
        let mut fremd = a_match(42);
        fremd.draw_id = 7;
        fremd.planning_id = 1001;
        fremd.team1 = vec![player("Fremd")];
        fremd.team2 = vec![player("Falsch")];
        let folge = folgespiel(80, Some(1001), None);
        let s = snap(Vec::new(), vec![fremd, folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "noch offen");
    }

    #[test]
    fn ein_halb_aufgeloester_platz_wird_neutral_beschriftet() {
        // Die Mannschaft steht (EntryID gesetzt), nur die Namen sind nicht
        // auflösbar. `from1` zeigt dann auf einen Teilnehmer-Slot, nicht auf
        // ein Spiel — es gibt nichts zu nennen.
        let mut folge = folgespiel(80, Some(1000), None);
        folge.entry1_id = 7;
        let s = snap(Vec::new(), vec![folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 1), "noch offen");
    }

    #[test]
    fn die_zweite_seite_liest_ihre_eigene_kante() {
        let mut vorspiel = a_match(43);
        vorspiel.planning_id = 1002;
        vorspiel.team1 = vec![player("Weber")];
        vorspiel.team2 = vec![player("Fischer")];
        let folge = folgespiel(80, None, Some(1002));
        let s = snap(Vec::new(), vec![vorspiel, folge.clone()], Vec::new());

        assert_eq!(offener_platz_text(&s, &folge, 2), "Weber oder Fischer");
        assert_eq!(offener_platz_text(&s, &folge, 1), "noch offen");
    }

    #[test]
    fn die_aufloesung_geht_genau_eine_ebene_tief_und_rekursiert_nie() {
        // Das Vorspiel ist selbst offen, SEIN Vorspiel hätte Namen. Die
        // dürfen nicht durchschlagen: In der Runde davor stünden vier, eine
        // weitere davor acht Namen in einer Zeile.
        let mut grossvater = a_match(10);
        grossvater.planning_id = 1000;
        grossvater.team1 = vec![player("Tief")];
        grossvater.team2 = vec![player("Tiefer")];
        let mut vater = folgespiel(42, Some(1000), None);
        vater.planning_id = 1001;
        vater.match_num = Some(42);
        let folge = folgespiel(80, Some(1001), None);
        let s = snap(
            Vec::new(),
            vec![grossvater, vater, folge.clone()],
            Vec::new(),
        );

        let text = offener_platz_text(&s, &folge, 1);
        assert_eq!(text, "aus Spiel 42");
        assert!(!text.contains("Tief"), "keine Enkel-Kandidaten: {text}");
    }

    /// Ein wartendes Spiel mit vollstaendiger Paarung, angesetzt zur
    /// angegebenen Zeit.
    fn wartendes(id: i64, zeit: i64) -> BtpMatch {
        let mut m = a_match(id);
        m.planned_time = Some(zeit);
        m
    }

    /// Ein Spiel mit zwei offenen Plaetzen, angesetzt zur angegebenen Zeit.
    fn offenes(id: i64, zeit: i64) -> BtpMatch {
        let mut m = folgespiel(id, None, None);
        m.planned_time = Some(zeit);
        m
    }

    #[test]
    fn der_schalter_offene_spiele_ausblenden_reist_in_beide_richtungen() {
        // Der Schalter wirkt clientseitig — aber er muss den Weg zum Gerät
        // und zurück überstehen, sonst stünde das Häkchen beim nächsten
        // Laden wieder anders.
        let mut cfg = AppConfig::default();
        cfg.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "p1".into(),
            name: "Wandmonitor".into(),
            display: crate::config::TlDisplaySettings {
                hide_open_matches: true,
                ..Default::default()
            },
            ..Default::default()
        });

        let sicht = profiles_view(&cfg);
        let wire = sicht.iter().find(|p| p.id == "p1").expect("das Profil");
        assert!(
            wire.display.hide_open_matches,
            "der Schalter muss zum Gerät reisen"
        );

        let zurueck = display_settings_from_wire(&wire.display);
        assert!(
            zurueck.hide_open_matches,
            "und aus dem Gerät zurück in die Konfiguration"
        );
    }

    #[test]
    fn eine_bestandskonfiguration_ohne_das_feld_zeigt_offene_spiele() {
        // G7: Nach dem Auto-Update steht jedes bestehende Profil auf
        // „anzeigen" — ohne dass jemand etwas tun muss.
        let alt = r#"{"show_numbers":true,"show_nations":false,"list_position":"bottom"}"#;
        let d: crate::config::TlDisplaySettings = serde_json::from_str(alt).expect("lesbar");
        assert!(!d.hide_open_matches);
    }

    #[test]
    fn ein_zustand_ohne_turnier_traegt_eine_leere_liste_offener_spiele() {
        let tablet = TabletState::default();
        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 3);
        assert!(s.open_queue.is_empty());
        assert_eq!(s.open_queue_truncated, 0);
    }

    #[test]
    fn ein_alter_zustand_ohne_die_neuen_felder_bleibt_lesbar() {
        // Der Relay bettet tl.html ein und wird bei jedem Merge deployt, die
        // App kommt erst ueber einen Release-Tag: Eine neue Seite trifft
        // regelmaessig einen alten Host. Fehlendes Feld muss "keine offenen
        // Spiele" heissen, nicht "Fehler".
        let s = state_with(
            snap(Vec::new(), vec![wartendes(1, 202_608_301_200)], Vec::new()),
            &AppConfig::default(),
        );
        let mut wert = serde_json::to_value(&s).expect("serialisierbar");
        let obj = wert.as_object_mut().expect("Objekt");
        obj.remove("open_queue");
        obj.remove("open_queue_truncated");

        let alt: TlState = serde_json::from_value(wert).expect("ohne die neuen Felder lesbar");
        assert!(alt.open_queue.is_empty());
        assert_eq!(alt.open_queue_truncated, 0);
    }

    #[test]
    fn die_spielliste_fuehrt_offene_paarungen_in_einer_zweiten_liste() {
        let s = state_with(
            snap(
                Vec::new(),
                vec![wartendes(1, 202_608_301_200), offenes(2, 202_608_301_300)],
                Vec::new(),
            ),
            &AppConfig::default(),
        );

        assert_eq!(
            s.queue.iter().map(|m| m.match_id).collect::<Vec<_>>(),
            vec![1],
            "die Arbeitsliste bleibt, was sie war"
        );
        assert_eq!(
            s.open_queue.iter().map(|m| m.match_id).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn ein_halb_offenes_spiel_behaelt_die_bekannten_namen() {
        // Steht eine Seite fest, muss sie mit Namen dastehen - sonst
        // verschenkt die Anzeige die halbe Information.
        let mut halb = offenes(2, 202_608_301_300);
        halb.team1 = vec![player("Müller")];
        let s = state_with(
            snap(Vec::new(), vec![halb], Vec::new()),
            &AppConfig::default(),
        );

        let eintrag = &s.open_queue[0];
        assert_eq!(eintrag.team1, vec!["Müller".to_string()]);
        assert!(eintrag.open_slot1_label.is_empty(), "besetzte Seite ohne Label");
        assert_eq!(eintrag.open_slot2_label, "noch offen");
    }

    #[test]
    fn ein_offenes_spiel_kennt_seinen_platz_zwischen_den_echten_spielen() {
        // Die Seite mischt nach dieser Zahl ein und sortiert NICHT selbst -
        // die Reihenfolge bleibt serverseitig verbindlich.
        let s = state_with(
            snap(
                Vec::new(),
                vec![
                    wartendes(1, 202_608_301_200),
                    offenes(2, 202_608_301_300),
                    wartendes(3, 202_608_301_400),
                    offenes(4, 202_608_301_500),
                ],
                Vec::new(),
            ),
            &AppConfig::default(),
        );

        assert_eq!(
            s.open_queue
                .iter()
                .map(|m| (m.match_id, m.queue_index))
                .collect::<Vec<_>>(),
            vec![(2, 1), (4, 2)],
            "Spiel 2 steht hinter einem, Spiel 4 hinter zwei echten Spielen"
        );
    }

    #[test]
    fn offene_spiele_verdraengen_kein_einziges_echtes_wartespiel() {
        // Das Erfolgskriterium der Spec: Nach dem Update darf kein Geraet
        // WENIGER echte Spiele sehen als vorher.
        let mut mit_offenen = Vec::new();
        let mut nur_echte = Vec::new();
        for i in 0..QUEUE_LIMIT as i64 {
            let m = wartendes(i + 1, 202_608_301_200 + i);
            nur_echte.push(m.clone());
            mit_offenen.push(m);
        }
        for i in 0..50i64 {
            mit_offenen.push(offenes(10_000 + i, 202_608_301_100));
        }

        let ohne = state_with(snap(Vec::new(), nur_echte, Vec::new()), &AppConfig::default());
        let mit = state_with(
            snap(Vec::new(), mit_offenen, Vec::new()),
            &AppConfig::default(),
        );

        assert_eq!(mit.queue.len(), ohne.queue.len());
        assert_eq!(mit.queue_truncated, ohne.queue_truncated);
        assert_eq!(
            mit.queue.iter().map(|m| m.match_id).collect::<Vec<_>>(),
            ohne.queue.iter().map(|m| m.match_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn die_offene_liste_hat_ihren_eigenen_deckel_und_meldet_die_kappung() {
        let mut matches = Vec::new();
        for i in 0..(OPEN_QUEUE_LIMIT as i64 + 5) {
            matches.push(offenes(i + 1, 202_608_301_200 + i));
        }
        let s = state_with(snap(Vec::new(), matches, Vec::new()), &AppConfig::default());

        assert_eq!(s.open_queue.len(), OPEN_QUEUE_LIMIT);
        assert_eq!(s.open_queue_truncated, 5);
    }

    #[test]
    fn ein_offenes_spiel_traegt_weder_lizenznummer_noch_blocked_marke() {
        // Datenschutz (G6): Die Lizenznummer ist das Link-Ziel der
        // badhub-Spielerseite eines FESTSTEHENDEN Teilnehmers - ein
        // Kandidat bekommt sie nicht.
        let mut halb = offenes(2, 202_608_301_300);
        halb.team1 = vec![licensed_player("Müller", "08-017991")];
        let s = state_with(
            snap(Vec::new(), vec![halb], Vec::new()),
            &AppConfig::default(),
        );

        let roh = serde_json::to_string(&s.open_queue).expect("serialisierbar");
        assert!(
            !roh.contains("08-017991"),
            "keine Lizenznummer an offenen Spielen: {roh}"
        );
        assert!(!roh.contains("blocked"), "kein blocked-Feld: {roh}");
        assert!(
            !roh.contains("predicted"),
            "keine Prognose an offenen Spielen: {roh}"
        );
    }

    #[test]
    fn ein_vorbereitungs_aufruf_fuer_ein_offenes_spiel_wird_abgelehnt() {
        // Nicht-Ziel G2: Ohne feststehende Paarung gibt es niemanden
        // anzusagen. Der Aufruf käme sonst bis in den Zustand und
        // verschwände erst beim nächsten Schnappschuss wieder — die
        // Turnierleitung hielte das Spiel für gerufen.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            Vec::new(),
            vec![a_match(1), offenes(2, 202_608_301_300)],
            Vec::new(),
        ));

        let antwort = apply_state_action(
            &tablet,
            &AppConfig::default(),
            1_000,
            &relay_proto::TlAction::CallPreparation {
                match_ids: vec![2],
                location_id: None,
            },
        );
        assert!(antwort.is_err(), "der Aufruf muss abgelehnt werden");
        assert!(
            tablet.preparation_calls().is_empty(),
            "und nichts hinterlassen haben"
        );

        // Gegenprobe: Das spielbereite Spiel lässt sich weiterhin rufen.
        assert!(apply_state_action(
            &tablet,
            &AppConfig::default(),
            1_000,
            &relay_proto::TlAction::CallPreparation {
                match_ids: vec![1],
                location_id: None,
            },
        )
        .is_ok());
    }

    #[test]
    fn halle_und_wunschfeld_gehen_auch_fuer_ein_offenes_spiel() {
        // A6: Vorbereitende Angaben sind erlaubt — sie greifen erst, wenn die
        // Paarung feststeht. Bestandsverhalten, hier festgenagelt.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            Vec::new(),
            vec![offenes(2, 202_608_301_300)],
            vec![crate::btp::model::BtpLocation {
                id: 1,
                name: "Halle 1".to_string(),
            }],
        ));
        let cfg = AppConfig::default();

        assert!(apply_state_action(
            &tablet,
            &cfg,
            1_000,
            &relay_proto::TlAction::SetHall {
                match_id: 2,
                hall: "Halle 1".to_string(),
            },
        )
        .is_ok());
        assert_eq!(
            tablet.manual_halls().get(&2).map(String::as_str),
            Some("Halle 1")
        );

        assert!(apply_state_action(
            &tablet,
            &cfg,
            1_000,
            &relay_proto::TlAction::ExcludeFromAutoAssign {
                match_id: 2,
                excluded: true,
            },
        )
        .is_ok());
        assert!(tablet.auto_assign_excluded(2));
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
    fn the_state_carries_hall_colors_only_for_multi_hall() {
        // Spec hallen-farben: Die Seite bekommt je Halle die effektive
        // Farbe — Auto-Palette folgt der alphabetischen Hallen-Sortierung.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            Vec::new(),
            vec![
                BtpLocation {
                    id: 1,
                    name: "Halle B".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle A".to_string(),
                },
            ],
        ));
        let s = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        assert_eq!(s.halls[0].name, "Halle A");
        assert_eq!(
            s.halls[0].color.as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[0])
        );
        assert_eq!(
            s.halls[1].color.as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[1])
        );

        // Ein-Hallen-Turnier: die Halle wird gelistet, aber ohne Farbe.
        let einzel = TabletState::default();
        einzel.set_snapshot(snap(
            vec![a_court(1, Some(1))],
            Vec::new(),
            vec![BtpLocation {
                id: 1,
                name: "Einzige".to_string(),
            }],
        ));
        let s1 = build_state(&einzel, &AppConfig::default(), 1_000, 1);
        assert_eq!(s1.halls.len(), 1);
        assert_eq!(s1.halls[0].color, None);
    }

    #[test]
    fn an_overridden_hall_color_reaches_the_page() {
        let mut cfg = AppConfig::default();
        cfg.upsert_hall_color("Halle A", crate::hall_colors::HALL_PALETTE[8])
            .unwrap();
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            Vec::new(),
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
        let s = build_state(&tablet, &cfg, 1_000, 1);
        assert_eq!(
            s.halls[0].color.as_deref(),
            Some(crate::hall_colors::HALL_PALETTE[8]),
            "die Übersteuerung gewinnt"
        );
    }

    #[test]
    fn tl_hall_color_defaults_to_none_for_old_hosts() {
        // Ein alter Host kennt das Feld nicht — die Seite muss den Eintrag
        // trotzdem parsen (Serde-Default) und bleibt dann farblos.
        let h: TlHall = serde_json::from_str(r#"{"id":1,"name":"Halle A"}"#).unwrap();
        assert_eq!(h.color, None);
    }

    #[test]
    fn finished_rows_carry_their_hall_name() {
        // Die Beendet-Zeile soll Hallen-Kürzel + Marke tragen können —
        // Papier-Ergebnisse ohne Feld bleiben ohne Halle.
        let mut gespielt = a_match(1);
        gespielt.status = MatchStatus::Finished;
        gespielt.winner = Some(1);
        gespielt.court_id = Some(2);
        gespielt.finished_at = Some(100);
        let mut papier = a_match(2);
        papier.status = MatchStatus::Finished;
        papier.winner = Some(2);
        papier.finished_at = Some(200);

        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            vec![gespielt, papier],
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
        let s = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        let von_feld = s.finished.iter().find(|f| f.match_id == 1).unwrap();
        assert_eq!(von_feld.hall, "Halle B");
        let ohne_feld = s.finished.iter().find(|f| f.match_id == 2).unwrap();
        assert_eq!(ohne_feld.hall, "", "Papier-Ergebnis bleibt ohne Halle");
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
                players: vec!["Müller".to_string()],
                // Ohne Lizenznummer fällt der Schlüssel auf den
                // normalisierten Namen zurück (`assign::player_key`).
                player_keys: vec!["müller".to_string()],
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
    fn die_prognose_haengt_minutengerundet_an_den_wartenden_spielen() {
        // Spec `spielzeiten-prognose` (E7/E8): ohne Messwerte gilt der
        // Config-Default (25 min) als unsicher; ein freies Feld + 2 min
        // Puffer ⇒ Spiel 1 „dran" bei now+2, Spiel 2 wartet aufs Feld.
        let tablet = TabletState::default();
        let mut m2 = a_match(2);
        m2.team1 = vec![player("Weber")];
        m2.team2 = vec![player("Fischer")];
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![a_match(1), m2],
            Vec::new(),
        ));
        let s = build_state(&tablet, &AppConfig::default(), 3_600_000, 7);
        assert_eq!(s.queue[0].predicted_start_ms, Some((60 + 2) * 60_000));
        assert!(
            s.queue[0].predicted_uncertain,
            "ohne Messwerte steht nur der Default dahinter"
        );
        assert_eq!(
            s.queue[1].predicted_start_ms,
            Some((60 + 2 + 25 + 2) * 60_000),
            "Spiel 2 wartet, bis das Feld frei wird"
        );
        assert_eq!(
            s.queue[0].predicted_start_ms.unwrap() % 60_000,
            0,
            "minutengerundet (Rev-Churn-Wächter)"
        );
    }

    #[test]
    fn die_live_restzeit_haelt_ein_zaehlendes_feld_die_volle_dauer() {
        // Etappe D: Match 7 wird live gezählt (Erster-Punkt-Stempel), steht
        // aber noch bei 0:0. Das Feld bleibt die volle Netto-Dauer belegt,
        // statt mit „Median − verstrichen" langsam freigerechnet zu werden.
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![player("Läufer")];
        running.team2 = vec![player("Renner")];
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![running, a_match(1)],
            Vec::new(),
        ));
        // Drei Messwerte: Brutto-Median 30, Netto 25, Differenz 5.
        for (id, brutto_min) in [(101, 20), (102, 30), (103, 40)] {
            let start = 1_000_000;
            tablet.match_times_store().reconcile(
                &[(id, "A", "mens_singles", "")],
                &std::collections::HashSet::new(),
                start,
            );
            tablet
                .match_times_store()
                .stamp_first_point(id, start + 5 * 60_000);
            tablet
                .match_times_store()
                .stamp_finished(id, true, start + brutto_min * 60_000);
        }
        // Match 7: vor 5 min zugewiesen (Anlauf-Median genau verbraucht),
        // erster Punkt gestempelt ⇒ Live-Modell greift.
        tablet.match_times_store().reconcile(
            &[(7, "A", "mens_singles", "")],
            &std::collections::HashSet::new(),
            55 * 60_000,
        );
        tablet.match_times_store().stamp_first_point(7, 56 * 60_000);

        let s = build_state(&tablet, &AppConfig::default(), 60 * 60_000, 7);

        assert_eq!(
            s.courts[0].remaining_min,
            Some(25),
            "0:0 ⇒ volle Nettodauer, Anlauf schon verbraucht"
        );
        assert_eq!(
            s.queue[0].predicted_start_ms,
            Some((60 + 25 + 2) * 60_000),
            "die Warteliste rechnet mit der Live-Restzeit"
        );
    }

    #[test]
    fn ohne_live_zaehlung_bleibt_das_alte_restzeit_modell() {
        // Kein Tablet verbunden, kein erster Punkt: Restzeit = Gruppenwert
        // minus verstrichene Zeit — wie vor Etappe D.
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        for (id, brutto_min) in [(101, 20), (102, 30), (103, 40)] {
            let start = 1_000_000;
            tablet.match_times_store().reconcile(
                &[(id, "A", "mens_singles", "")],
                &std::collections::HashSet::new(),
                start,
            );
            tablet
                .match_times_store()
                .stamp_first_point(id, start + 5 * 60_000);
            tablet
                .match_times_store()
                .stamp_finished(id, true, start + brutto_min * 60_000);
        }
        // Vor 10 min zugewiesen, nie ein Punkt gemeldet.
        tablet.match_times_store().reconcile(
            &[(7, "A", "mens_singles", "")],
            &std::collections::HashSet::new(),
            50 * 60_000,
        );

        let s = build_state(&tablet, &AppConfig::default(), 60 * 60_000, 7);

        assert_eq!(
            s.courts[0].remaining_min,
            Some(20),
            "Brutto-Median 30 − 10 min verstrichen"
        );
    }

    #[test]
    fn ueberfaellige_spiele_zeigen_mindestens_eine_restminute() {
        // Alt-Modell, Median längst überschritten: „~0 min Rest" wäre eine
        // verwirrende Anzeige — die Untergrenze ist 1 Minute („gleich").
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        for (id, brutto_min) in [(101, 20), (102, 30), (103, 40)] {
            let start = 1_000_000;
            tablet.match_times_store().reconcile(
                &[(id, "A", "mens_singles", "")],
                &std::collections::HashSet::new(),
                start,
            );
            tablet
                .match_times_store()
                .stamp_first_point(id, start + 5 * 60_000);
            tablet
                .match_times_store()
                .stamp_finished(id, true, start + brutto_min * 60_000);
        }
        // Vor 40 min zugewiesen (Median 30 längst vorbei), nie ein Punkt.
        tablet.match_times_store().reconcile(
            &[(7, "A", "mens_singles", "")],
            &std::collections::HashSet::new(),
            20 * 60_000,
        );

        let s = build_state(&tablet, &AppConfig::default(), 60 * 60_000, 7);

        assert_eq!(s.courts[0].remaining_min, Some(1));
    }

    #[test]
    fn ausgeschaltete_prognose_liefert_weder_zeiten_noch_statistik() {
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![a_match(1)], Vec::new()));
        let mut cfg = AppConfig::default();
        cfg.prediction.enabled = false;
        let s = build_state(&tablet, &cfg, 3_600_000, 7);
        assert_eq!(s.queue[0].predicted_start_ms, None);
        assert!(s.time_stats.is_none());
    }

    #[test]
    fn statistik_und_ist_zeiten_kommen_aus_dem_zeiten_store() {
        // Drei regulär gemessene A-Herreneinzel: Brutto 20/30/40 (Median
        // 30), Netto je 5 min kürzer (Median 25). Match 3 ist zugleich das
        // beendete Spiel im Snapshot → seine Beendet-Zeile trägt die
        // Ist-Zeiten.
        let tablet = TabletState::default();
        // Snapshot ZUERST: er bindet den Zeiten-Store ans Turnier — Stempel
        // vor der Bindung würden beim (Turnier-)Wechsel verworfen.
        let mut done = a_match(3);
        done.status = MatchStatus::Finished;
        done.winner = Some(1);
        done.finished_at = Some(2_000_000);
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![a_match(1), done],
            Vec::new(),
        ));
        let store_seed = |id: i64, brutto_min: u64| {
            let start = 1_000_000;
            tablet.match_times_store().reconcile(
                &[(id, "A", "mens_singles", "")],
                &std::collections::HashSet::new(),
                start,
            );
            tablet
                .match_times_store()
                .stamp_first_point(id, start + 5 * 60_000);
            tablet
                .match_times_store()
                .stamp_finished(id, true, start + brutto_min * 60_000);
        };
        store_seed(101, 20);
        store_seed(102, 30);
        store_seed(3, 40);

        let s = build_state(&tablet, &AppConfig::default(), 3_600_000, 7);

        let stats = s.time_stats.expect("Prognose an ⇒ Statistik da");
        assert_eq!(stats.default_mins, 25);
        assert_eq!(stats.rows.len(), 1);
        assert_eq!(stats.rows[0].class_label, "A");
        assert_eq!(stats.rows[0].discipline, "mens_singles");
        assert_eq!(stats.rows[0].count, 3);
        assert_eq!(stats.rows[0].brutto_mins, 30);
        assert_eq!(stats.rows[0].netto_mins, 25);
        assert_eq!(stats.rows[0].diff_mins, 5);
        assert_eq!(stats.tournament_brutto_mins, Some(30));

        assert_eq!(s.finished[0].match_id, 3);
        assert_eq!(s.finished[0].brutto_mins, Some(40));
        assert_eq!(s.finished[0].netto_mins, Some(35));

        // Und mit Messwerten ist die Prognose nicht mehr unsicher: das
        // wartende A-Herreneinzel bekommt den Gruppen-Median (30).
        assert!(!s.queue[0].predicted_uncertain);
        assert_eq!(s.queue[0].predicted_start_ms, Some((60 + 2) * 60_000));
    }

    #[test]
    fn spieler_auf_gesperrtem_feld_sind_fuer_die_prognose_gebunden() {
        // Review 2026-08-16 (F5): Ein gesperrtes Feld kommt nicht in die
        // Vergabe-Rotation — aber wer dort mitten im Spiel steht, ist
        // trotzdem gebunden. Sonst hieße es „gleich dran", obwohl das
        // laufende Spiel erst enden muss.
        let tablet = TabletState::default();
        let mut running = a_match(7); // Müller vs. Schmidt …
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        let waiting = a_match(2); // … die auch hier spielen (Fixture-Namen)
        tablet.set_snapshot(snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![running, waiting],
            Vec::new(),
        ));
        tablet.set_court_locked(1, true);
        let s = build_state(&tablet, &AppConfig::default(), 3_600_000, 7);
        // Feld 2 wäre ab now+2 frei — aber die Spieler stehen auf dem
        // gesperrten Feld 1 (Restzeit = Default 25 min, keine Messwerte).
        assert_eq!(s.queue[0].predicted_start_ms, Some((60 + 25) * 60_000));
    }

    #[test]
    fn das_prognose_gedaechtnis_mergt_statt_zu_ersetzen() {
        // Review 2026-08-16 (F6): Die Relay-Größenleiter baut denselben
        // Zustand mit kleineren Wartelisten — ein Ersetzen ließe nur die
        // Matches der kleinsten Stufe übrig und die E12-Kontrolle bliebe
        // für alle dahinter stumm.
        let tablet = TabletState::default();
        tablet.merge_predicted_starts([(1, 100), (2, 200)].into_iter().collect());
        tablet.merge_predicted_starts([(2, 250)].into_iter().collect()); // 5er-Stufe
        assert_eq!(tablet.predicted_start_ms(1), Some(100));
        assert_eq!(tablet.predicted_start_ms(2), Some(250));
        assert_eq!(tablet.take_predicted_start(1), Some(100));
        assert_eq!(
            tablet.predicted_start_ms(1),
            None,
            "genau eine Log-Zeile je Aufruf"
        );
        tablet.retain_predicted_starts(&std::collections::HashSet::new());
        assert_eq!(tablet.predicted_start_ms(2), None);
    }

    #[test]
    fn die_statistik_wird_je_messwert_generation_nur_einmal_gerechnet() {
        // Review 2026-08-16 (F8): Der TL-Zustand entsteht alle ~2 s je
        // Gerät — die Statistik darf nur neu rechnen, wenn sich am
        // Zeiten-Store wirklich etwas geändert hat.
        let tablet = TabletState::default();
        let a = tablet.cached_time_stats();
        let b = tablet.cached_time_stats();
        assert!(
            std::sync::Arc::ptr_eq(&a, &b),
            "unverändert → derselbe Cache"
        );
        tablet.match_times_store().reconcile(
            &[(7, "A", "HE", "")],
            &std::collections::HashSet::new(),
            1_000,
        );
        let c = tablet.cached_time_stats();
        assert!(
            !std::sync::Arc::ptr_eq(&a, &c),
            "neue Messwert-Generation → neu gerechnet"
        );
    }

    #[test]
    fn der_fingerprint_bleibt_innerhalb_einer_minute_stabil() {
        // Rev-Churn-Wächter: Zwei Bauten in derselben Minute dürfen sich
        // nur in `server_now_ms` unterscheiden — sonst zählte die Revision
        // jeden Poll hoch und jede TL-Aktion liefe auf „überholter Stand".
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, None)],
            vec![a_match(1), a_match(2)],
            Vec::new(),
        ));
        let mut a = build_state(&tablet, &AppConfig::default(), 3_600_000, 7);
        let mut b = build_state(&tablet, &AppConfig::default(), 3_650_000, 7);
        assert_eq!(state_fingerprint(&mut a), state_fingerprint(&mut b));
    }

    #[test]
    fn die_restzeit_bewegt_den_fingerprint_nicht() {
        // Rev-Churn-Wächter für Etappe D (Review 2026-08-17): `remaining_min`
        // ist zeitabgeleitet und schrumpft im Minutentakt — unmaskiert
        // bekäme jedes Gerät je belegtem Feld einmal pro Minute den vollen
        // Zustand statt eines 304, und der Relay pushte im selben Takt.
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        tablet.match_times_store().reconcile(
            &[(7, "A", "mens_singles", "")],
            &std::collections::HashSet::new(),
            50 * 60_000,
        );
        let mut a = build_state(&tablet, &AppConfig::default(), 60 * 60_000, 7);
        let mut b = build_state(&tablet, &AppConfig::default(), 62 * 60_000, 7);
        assert_ne!(
            a.courts[0].remaining_min, b.courts[0].remaining_min,
            "Fixture-Check: die Restzeit schrumpft über die zwei Minuten wirklich"
        );
        assert_eq!(state_fingerprint(&mut a), state_fingerprint(&mut b));
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
        let matches: Vec<BtpMatch> = (1..=QUEUE_LIMIT as i64 + 5).map(a_match).collect();
        let s = state_with(snap(Vec::new(), matches, Vec::new()), &AppConfig::default());
        assert_eq!(s.queue.len(), QUEUE_LIMIT);
        // Wie viele Spiele fehlen, steht im Zustand — statt sie
        // stillschweigend zu unterschlagen.
        assert_eq!(s.queue_truncated, 5);
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
    fn das_zeiten_panel_liefert_alle_vier_achsen() {
        // A1.1: Die Seite bekommt alle vier Schnitte gemeinsam — der
        // Zustand entsteht zentral für alle Geräte, die Achsen-Wahl liegt
        // aber im Profil je Gerät (ADR 0034).
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            vec![a_match(1)],
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
        let seed = |id: i64, klasse: &str, disc: &str, halle: &str, brutto: u64| {
            let start = 1_000_000;
            tablet.match_times_store().reconcile(
                &[(id, klasse, disc, halle)],
                &std::collections::HashSet::new(),
                start,
            );
            tablet
                .match_times_store()
                .stamp_first_point(id, start + 5 * 60_000);
            tablet
                .match_times_store()
                .stamp_finished(id, true, start + brutto * 60_000);
        };
        seed(101, "A", "mens_singles", "Halle A", 20);
        seed(102, "A", "womens_doubles", "Halle B", 30);
        seed(103, "B", "mens_singles", "Halle B", 40);

        let s = build_state(&tablet, &AppConfig::default(), 3_600_000, 7);
        let stats = s.time_stats.expect("Prognose an ⇒ Statistik da");

        assert_eq!(stats.rows.len(), 3, "drei Klasse-x-Disziplin-Gruppen");
        assert_eq!(stats.by_class.len(), 2, "A und B");
        assert_eq!(stats.by_discipline.len(), 2, "HE und DD");
        assert_eq!(stats.by_hall.len(), 2, "Halle A und Halle B");
        // A1.5: Jede Achse zerlegt dieselben drei Messwerte.
        for zeilen in [
            &stats.rows,
            &stats.by_class,
            &stats.by_discipline,
            &stats.by_hall,
        ] {
            assert_eq!(zeilen.iter().map(|z| z.count).sum::<usize>(), 3);
        }
        // Der Hallenname steht nur auf der Hallen-Achse.
        assert!(stats.by_hall.iter().all(|z| !z.hall.is_empty()));
        assert!(stats.rows.iter().all(|z| z.hall.is_empty()));
    }

    #[test]
    fn ein_ein_hallen_turnier_liefert_keine_hallen_achse() {
        // A1.6: Dort gäbe es genau eine Zeile „ohne Halle" — die sagt
        // nichts, also wird die Achse gar nicht erst angeboten.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(1, Some(1))],
            vec![a_match(1)],
            vec![crate::btp::model::BtpLocation {
                id: 1,
                name: "Einzige Halle".to_string(),
            }],
        ));
        let start = 1_000_000;
        tablet.match_times_store().reconcile(
            &[(101, "A", "mens_singles", "")],
            &std::collections::HashSet::new(),
            start,
        );
        tablet
            .match_times_store()
            .stamp_first_point(101, start + 5 * 60_000);
        tablet
            .match_times_store()
            .stamp_finished(101, true, start + 20 * 60_000);

        let s = build_state(&tablet, &AppConfig::default(), 3_600_000, 7);
        let stats = s.time_stats.expect("Statistik da");
        assert!(stats.by_hall.is_empty(), "keine Hallen-Achse");
        assert_eq!(stats.rows.len(), 1, "die übrigen Achsen bleiben");
        assert_eq!(stats.by_class.len(), 1);
    }

    #[test]
    fn die_achse_reist_durch_profile_to_wire_und_zurueck() {
        // A1.3: Die Wahl liegt im Profil und muss beide Grenzen überstehen.
        for achse in [
            crate::config::TlTimeStatsAxis::Group,
            crate::config::TlTimeStatsAxis::Class,
            crate::config::TlTimeStatsAxis::Discipline,
            crate::config::TlTimeStatsAxis::Hall,
        ] {
            let profil = crate::config::TlPanelProfile {
                id: "p1".to_string(),
                name: "Test".to_string(),
                display: crate::config::TlDisplaySettings {
                    time_stats_axis: achse,
                    ..Default::default()
                },
                ..Default::default()
            };
            let wire = profile_to_wire(&profil);
            let zurueck = display_settings_from_wire(&wire.display);
            assert_eq!(
                zurueck.time_stats_axis, achse,
                "die Achse muss den Rundweg überstehen"
            );
        }
    }

    #[test]
    fn ein_profil_ohne_achsen_feld_liest_sich_als_gruppe() {
        // A1.2: Jedes vor v0.9.231 gespeicherte Profil kennt das Feld nicht
        // — und muss auf der bisherigen Ansicht landen, nicht auf einer
        // zufälligen.
        let alt = r#"{"showNumbers":true,"listPosition":"right"}"#;
        let d: relay_proto::TlDisplaySettingsWire =
            serde_json::from_str(alt).expect("altes Profil bleibt lesbar");
        assert_eq!(d.time_stats_axis, relay_proto::TlTimeStatsAxisWire::Group);
        assert_eq!(
            display_settings_from_wire(&d).time_stats_axis,
            crate::config::TlTimeStatsAxis::Group
        );
    }

    #[test]
    fn die_spielzeiten_auswertung_faellt_vor_dem_ganzen_zustand() {
        // Spec `tl-sicht-feinschliff` A0.1: Die Auswertung trägt seit der
        // Achsen-Erweiterung VIER Zeilensätze. Passt der Zustand damit nicht
        // mehr durch, wird SIE geopfert — nicht der ganze Stand. Ohne die
        // Stufe verwürfe der Relay das komplette Frame samt Vorgänger, und
        // die Cloud-Turnierleitung sähe gar nichts mehr, auch keine Felder.
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
                licensed_player(
                    "Maximiliane Charlotte von Hohenlohe-Waldenburg",
                    "08-100001",
                ),
                licensed_player("Friederike Alexandra Schmidt-Blumenthal", "08-100002"),
            ];
            m.team2 = vec![
                licensed_player("Konstantin Ferdinand Oppermann-Lindenau", "08-100003"),
                licensed_player("Sebastian Aurelius Wittgenstein-Berleburg", "08-100004"),
            ];
            m.draw_name = if id % 2 == 0 { "HE A" } else { "HE B" }.to_string();
            m.round_name = "Achtelfinale der Trostrunde".to_string();
            matches.push(m);
        }
        // 40 belegte Felder und 30 Ergebnisse — ein großes Zwei-Hallen-
        // Turnier. Ohne diese Grundlast bliebe die Auswertung der einzige
        // Brocken, und der Test bewiese nur, dass sie allein nicht reicht.
        let mut courts = Vec::new();
        for court_id in 1..=40 {
            courts.push(a_court(court_id, Some(if court_id <= 20 { 1 } else { 2 })));
            let mut m = a_match(50_000 + court_id);
            m.team1 = vec![
                player("Maximiliane Charlotte von Hohenlohe-Waldenburg"),
                player("Friederike Alexandra Schmidt-Blumenthal"),
            ];
            m.team2 = vec![
                player("Konstantin Ferdinand Oppermann-Lindenau"),
                player("Sebastian Aurelius Wittgenstein-Berleburg"),
            ];
            m.round_name = "Achtelfinale der Trostrunde".to_string();
            m.status = MatchStatus::OnCourt;
            m.court_id = Some(court_id);
            m.court = Some(format!("Feld {court_id}"));
            matches.push(m);
        }
        for n in 1..=30 {
            let mut m = a_match(60_000 + n);
            m.team1 = vec![
                player("Maximiliane Charlotte von Hohenlohe-Waldenburg"),
                player("Friederike Alexandra Schmidt-Blumenthal"),
            ];
            m.team2 = vec![
                player("Konstantin Ferdinand Oppermann-Lindenau"),
                player("Sebastian Aurelius Wittgenstein-Berleburg"),
            ];
            m.status = MatchStatus::Finished;
            m.winner = Some(1);
            m.finished_at = Some(3_000_000 + n as u64);
            m.sets = vec![(21, 19), (19, 21), (21, 18)];
            matches.push(m);
        }
        tablet.set_snapshot(snap(
            courts,
            matches,
            vec![
                crate::btp::model::BtpLocation {
                    id: 1,
                    name: "Sporthalle Nordwest".to_string(),
                },
                crate::btp::model::BtpLocation {
                    id: 2,
                    name: "Sporthalle Suedost".to_string(),
                },
            ],
        ));

        // Viele Messwerte über viele Gruppen — ein großes Turnier hat
        // Dutzende Klassen-/Disziplin-/Hallen-Kombinationen, und jede wird
        // in bis zu vier Achsen zu einer Zeile.
        let mut id = 100_000;
        for klasse in [
            "A", "B", "C", "D", "E", "F", "G", "H", "U11", "U13", "U15", "U17", "U19", "O30",
            "O40", "O50", "O60", "AK1", "AK2", "AK3", "AK4", "AK5",
        ] {
            for disc in [
                "mens_singles",
                "womens_singles",
                "mens_doubles",
                "womens_doubles",
                "mixed_doubles",
            ] {
                for (n, halle) in ["Halle Nordwest", "Halle Suedost"].iter().enumerate() {
                    id += 1;
                    let start = 1_000_000;
                    tablet.match_times_store().reconcile(
                        &[(id, klasse, disc, halle)],
                        &std::collections::HashSet::new(),
                        start,
                    );
                    tablet
                        .match_times_store()
                        .stamp_first_point(id, start + 5 * 60_000);
                    tablet.match_times_store().stamp_finished(
                        id,
                        true,
                        start + (20 + n as u64) * 60_000,
                    );
                }
            }
        }
        let voll = build_state(&tablet, &cfg, 3_600_000, 7);
        let stats = voll.time_stats.as_ref().expect("Statistik ist da");
        assert!(
            stats.rows.len() + stats.by_class.len() + stats.by_discipline.len() >= 70,
            "Fixture-Fehler: zu wenige Statistik-Zeilen ({} + {} + {})",
            stats.rows.len(),
            stats.by_class.len(),
            stats.by_discipline.len()
        );

        let (json, _rev) = state_for_relay(&tablet, &cfg, 3_600_000);
        // Gemessen am 18.08.2026 mit genau diesem Fixture: Die Auswertung
        // wiegt 14 760 Bytes (110/22/5/2 Zeilen). Ohne die Kürzungsstufe
        // ginge der Zustand mit gut 70 000 von erlaubten 65 536 Bytes
        // hinaus — der Relay verwürfe ihn samt Vorgänger. Mit ihr sind es
        // 55 286.
        assert!(
            json.len() <= relay_proto::MAX_TL_STATE_LEN,
            "passt nicht: {} Bytes",
            json.len()
        );
        let state: TlState = serde_json::from_str(&json).unwrap();
        assert!(
            state.time_stats.is_none(),
            "die Auswertung muss geopfert worden sein"
        );
        // Und der Rest steht: Die Warteliste ist gekürzt, aber da.
        assert!(!state.queue.is_empty(), "die Bedienung bleibt");
        assert!(state.queue_truncated > 0, "gekürzt, aber gesagt");
    }

    /// Ein volles Turnier als Fixture für die Relay-Größen-Wächter.
    ///
    /// Der Zuschnitt ist über die Jahre gewachsen, weil jeder zu kleine
    /// Worst Case den Cloud-Zustand zu klein maß:
    /// - **Doppelpaarungen mit langen Namen und Lizenznummern** — seit die
    ///   Warteliste `team1_ids`/`player_keys` trägt (Review 17.08.2026).
    /// - **Belegte Felder und beendete Spiele** — seit auch sie die Nummern
    ///   tragen (Spec `tl-sicht-feinschliff` Punkt 4); vorher maß der
    ///   Wächter allein die Warteliste und hätte ein Wachstum in genau
    ///   diesen beiden Listen nie bemerkt.
    /// - **Vereinsnamen** — sie reisen **immer** mit, unabhängig von
    ///   `display.show_club_names` (die Einstellung steuert nur die
    ///   Anzeige). Ohne sie fehlten mehrere Kilobyte, also mehr als die
    ///   gesamte Reserve (Security-Review 18.08.2026).
    /// - **Schiedsrichter und Zähltafelbediener** — zwei weitere Listen im
    ///   selben Zustand, die **nicht** in der Kürzungskaskade stehen.
    fn volles_turnier(felder: i64, beendete: i64, wartende: i64) -> (TabletState, AppConfig) {
        let mut cfg = AppConfig::default();
        // Die Warteliste wird **je Halle** gekappt — mit zwei Hallen stehen
        // entsprechend mehr Spiele im Zustand.
        for (draw, halle) in [("HE A", "Halle A"), ("HE B", "Halle B")] {
            cfg.discipline_hall_rules.push(DisciplineHallRule {
                discipline: "mens_singles".to_string(),
                draw_name: draw.to_string(),
                hall: halle.to_string(),
            });
        }
        cfg.scorekeeper.enabled = true;
        let im_verein = |name: &str, lizenz: &str, verein: &str| {
            let mut p = licensed_player(name, lizenz);
            p.club = Some(verein.to_string());
            p
        };
        let besetze = |m: &mut crate::btp::model::BtpMatch, n: i64| {
            m.team1 = vec![
                im_verein(
                    "Maximiliane Charlotte von Hohenlohe-Waldenburg",
                    &format!("08-1{n:05}"),
                    "TSV Musterhausen-Oberdorf 1899 e.V.",
                ),
                im_verein(
                    "Friederike Alexandra Schmidt-Blumenthal",
                    &format!("08-2{n:05}"),
                    "SG Niederkirchen-Waldrand von 1911",
                ),
            ];
            m.team2 = vec![
                im_verein(
                    "Konstantin Ferdinand Oppermann-Lindenau",
                    &format!("08-3{n:05}"),
                    "Badminton-Club Seeblick-Hinterberg e.V.",
                ),
                im_verein(
                    "Sebastian Aurelius Wittgenstein-Berleburg",
                    &format!("08-4{n:05}"),
                    "1. BV Grün-Weiß Talheim-Sonnenberg",
                ),
            ];
            m.round_name = "Achtelfinale der Trostrunde".to_string();
        };
        let mut matches = Vec::new();
        for id in 1..=wartende {
            let mut m = a_match(id);
            besetze(&mut m, id);
            m.draw_name = if id % 2 == 0 { "HE A" } else { "HE B" }.to_string();
            matches.push(m);
        }
        let mut courts = Vec::new();
        for court_id in 1..=felder {
            courts.push(a_court(court_id, None));
            let mut m = a_match(10_000 + court_id);
            besetze(&mut m, 10_000 + court_id);
            m.status = crate::btp::model::MatchStatus::OnCourt;
            m.court_id = Some(court_id);
            m.court = Some(format!("Feld {court_id}"));
            matches.push(m);
        }
        for n in 1..=beendete {
            let mut m = a_match(20_000 + n);
            besetze(&mut m, 20_000 + n);
            m.status = crate::btp::model::MatchStatus::Finished;
            m.winner = Some(1);
            m.finished_at = Some(900_000 + n as u64);
            m.sets = vec![(21, 19), (19, 21), (21, 18)];
            matches.push(m);
        }
        let mut schnappschuss = snap(courts, matches, Vec::new());
        schnappschuss.officials = (1..=40)
            .map(|id| crate::btp::model::BtpOfficial {
                id,
                name: format!("Wolfgang-Dietrich Oberschiedsrichter {id}"),
                first: format!("Wolfgang-Dietrich {id}"),
                nationality: Some("GER".to_string()),
            })
            .collect();
        let tablet = TabletState::default();
        tablet.set_snapshot(schnappschuss);
        tablet.officials_store().set_enabled(true);
        for n in 1..=20 {
            tablet.add_scorekeeper_manual(
                vec![format!("Bernadette Zähltafelbedienerin {n}")],
                1_000 + n,
            );
        }
        (tablet, cfg)
    }

    #[test]
    fn a_board_too_big_for_the_relay_is_shortened_instead_of_lost() {
        // Der Relay legt einen zu großen Zustand nicht ab — und der Host
        // erfährt davon nichts. Ohne eigene Kürzung wäre die
        // Cloud-Oberfläche in genau den Turnieren tot, in denen sie am
        // meisten hülfe: je größer das Turnier, desto sicherer.
        let (tablet, cfg) = volles_turnier(26, 30, 400);

        let (json, _rev) = state_for_relay(&tablet, &cfg, 1_000_000);
        assert!(
            json.len() <= relay_proto::MAX_TL_STATE_LEN,
            "passt nicht: {} Bytes",
            json.len()
        );
        // Gemessen am 18.08.2026 mit genau diesem Fixture: 62 467 von
        // 65 536 Bytes, also nur noch rund 5 % Reserve — die Warteliste ist
        // dabei schon auf ihre unterste Stufe gekürzt. Wer den Zustand um
        // eine weitere Liste erweitert (nächster Kandidat: die vierachsige
        // Spielzeiten-Statistik aus Punkt 1), misst hier nach und nimmt sie
        // in die Kürzungskaskade auf — die Reserve trägt keine zweite
        // Erweiterung mehr.
        //
        // Und die Kürzung wird gemeldet, statt Spiele stillschweigend
        // verschwinden zu lassen.
        let state: TlState = serde_json::from_str(&json).unwrap();
        assert!(state.queue_truncated > 0, "gekürzt, aber nicht gesagt");
        // Die Felder überleben JEDE Kürzungsstufe: Sie sind das
        // Bedienelement der Seite — eine Turnierleitung ohne Feldkacheln
        // kann nichts mehr zuweisen, während sie ohne Statistik oder
        // Ergebnisliste weiterarbeitet.
        assert_eq!(
            state.courts.len(),
            26,
            "die Feldkacheln dürfen nie der Kürzung zum Opfer fallen"
        );
        // Und sie sind wirklich BELEGT: Ohne diese Prüfung könnte die
        // Fixture-Bindung (Status, `court_id`) brechen, ohne dass ein Test
        // rot wird — der Wächter mäße dann 26 leere Kacheln und die
        // dokumentierte Reserve wäre falsch (Review 18.08.2026).
        assert!(
            state.courts.iter().all(|c| c.team1_ids.len() == 2),
            "die Felder müssen mit Doppelpaarungen belegt sein, sonst misst der Wächter zu wenig"
        );

        // Ein kleines Turnier verliert nichts.
        let klein = TabletState::default();
        klein.set_snapshot(snap(Vec::new(), vec![a_match(1)], Vec::new()));
        let (json, _rev) = state_for_relay(&klein, &AppConfig::default(), 1_000_000);
        let state: TlState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.queue_truncated, 0);
        assert_eq!(state.queue.len(), 1);
    }

    #[test]
    fn eine_ueberquellende_ergebnisliste_wird_gestutzt_statt_alles_zu_verlieren() {
        // Letzte Stufe der Kürzungskaskade: Reicht selbst die kürzeste
        // Warteliste nicht (mehr Felder als im Wächter oben), wird die
        // Ergebnisliste gestutzt. Sie ist reine Rückschau — wer ein älteres
        // Ergebnis sucht, schaut in BTP. Ohne diese Stufe ginge der Zustand
        // über die Grenze, der Relay verwürfe das ganze Frame samt
        // Vorgänger, und die Cloud-Turnierleitung sähe GAR NICHTS mehr:
        // keine Felder, keine Liste, keine Bedienung.
        let (tablet, cfg) = volles_turnier(40, 30, 400);

        let (json, _rev) = state_for_relay(&tablet, &cfg, 1_000_000);
        assert!(
            json.len() <= relay_proto::MAX_TL_STATE_LEN,
            "auch ein 40-Felder-Turnier muss durchpassen: {} Bytes",
            json.len()
        );
        let state: TlState = serde_json::from_str(&json).unwrap();
        assert!(
            state.finished.len() < 30,
            "die Ergebnisliste muss gestutzt worden sein, war {}",
            state.finished.len()
        );
        // Aber nicht leergeräumt — die jüngsten Ergebnisse sind die, nach
        // denen am Feld gefragt wird.
        assert!(
            !state.finished.is_empty(),
            "die Ergebnisliste darf nicht ganz verschwinden"
        );
        // Und die Felder stehen auch hier vollständig.
        assert_eq!(state.courts.len(), 40);
        assert!(state.courts.iter().all(|c| c.team1_ids.len() == 2));
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
    fn the_revision_does_not_depend_on_the_transport_limit() {
        // Kernregression (Review 18.08.2026): Der LAN-Weg baut mit 120,
        // der Relay-Weg kürzt auf 40. Hinge die Revision am gebauten
        // Umfang, kippte der geteilte Zähler bei mehr als 40 wartenden
        // Spielen mit JEDEM Takt hin und her — das Rev-Gate des Relays
        // griffe nie, der volle Zustand ginge alle zwei Sekunden über
        // Mobilfunk, und jede Seite würde im Sekundentakt zum Neuladen
        // angestoßen. Also: gleicher Turnierstand ⇒ gleiche Revision,
        // egal über welchen Weg.
        let tablet = TabletState::default();
        let mut matches = vec![match_on_court(1, 3)];
        // Deutlich mehr als die Relay-Stufe (40) an wartenden Spielen.
        for id in 100..=200 {
            matches.push(a_match(id));
        }
        tablet.set_snapshot(snap(vec![a_court(3, None)], matches, Vec::new()));
        let cfg = AppConfig::default();

        let lan = build_state_with_rev(&tablet, &cfg, 1_000_000);
        let (_json, relay_rev) = state_for_relay(&tablet, &cfg, 1_000_000);
        assert_eq!(
            lan.rev, relay_rev,
            "derselbe Stand muss über beide Wege dieselbe Revision haben"
        );
        // Und ohne Änderung bleibt sie stehen — sonst wäre jedes Rev-Gate
        // wirkungslos.
        let (_json2, relay_rev2) = state_for_relay(&tablet, &cfg, 1_060_000);
        assert_eq!(relay_rev, relay_rev2, "ohne Änderung keine neue Revision");
        let lan2 = build_state_with_rev(&tablet, &cfg, 1_120_000);
        assert_eq!(lan.rev, lan2.rev);
    }

    /// Ein Turnier mit `echte` spielbereiten und `offen` offenen Spielen,
    /// alle mit Lizenznummern — so wiegt das Fixture so viel wie ein echter
    /// Zustand.
    fn turnier_mit_offenen(echte: i64, offen: i64) -> TabletState {
        let tablet = TabletState::default();
        let mut matches = Vec::new();
        for i in 0..echte {
            let mut m = a_match(100 + i);
            m.planned_time = Some(202_608_301_200 + i);
            m.team1 = vec![licensed_player("Langername Eins", "08-001234")];
            m.team2 = vec![licensed_player("Langername Zwei", "08-005678")];
            matches.push(m);
        }
        for i in 0..offen {
            let mut m = folgespiel(5_000 + i, None, None);
            m.planned_time = Some(202_608_301_100 + i);
            matches.push(m);
        }
        tablet.set_snapshot(snap(vec![a_court(3, None)], matches, Vec::new()));
        tablet
    }

    #[test]
    fn die_warteliste_wird_durch_offene_spiele_keine_stufe_kuerzer() {
        // Das Erfolgskriterium der Spec, als Beweis: Derselbe Stand einmal
        // mit und einmal ohne offene Spiele muss über die Cloud dieselbe
        // Arbeitsliste liefern. Sonst sähe ein Cloud-Gerät nach dem Update
        // WENIGER echte Spiele als vorher.
        let cfg = AppConfig::default();
        let (ohne, _) = state_for_relay(&turnier_mit_offenen(200, 0), &cfg, 1_000_000);
        let (mit, _) = state_for_relay(&turnier_mit_offenen(200, 150), &cfg, 1_000_000);
        let ohne: serde_json::Value = serde_json::from_str(&ohne).unwrap();
        let mit: serde_json::Value = serde_json::from_str(&mit).unwrap();

        assert_eq!(
            mit["queue"].as_array().unwrap().len(),
            ohne["queue"].as_array().unwrap().len(),
            "offene Spiele dürfen die Arbeitsliste keine Stufe kosten"
        );
        assert_eq!(mit["queue_truncated"], ohne["queue_truncated"]);
    }

    #[test]
    fn der_cloud_zustand_opfert_die_offenen_spiele_vor_der_warteliste() {
        // Offene Spiele sind ein Anzeige-Extra: Wird der Platz knapp,
        // verschwinden sie ZUERST — und melden das ehrlich.
        let cfg = AppConfig::default();
        let (json, _) = state_for_relay(&turnier_mit_offenen(200, 150), &cfg, 1_000_000);
        let state: serde_json::Value = serde_json::from_str(&json).unwrap();

        let gezeigt = state["open_queue"].as_array().unwrap().len();
        let gekappt = state["open_queue_truncated"].as_u64().unwrap() as usize;
        assert_eq!(
            gezeigt + gekappt,
            150,
            "gezeigt + gemeldet muss die volle offene Liste ergeben"
        );
        assert!(
            gezeigt < 150,
            "bei 200 echten Spielen kann nicht auch noch alles Offene passen"
        );
    }

    #[test]
    fn ein_grosses_turnier_bleibt_auch_mit_offenen_spielen_unter_der_relay_grenze() {
        // Reißt die Grenze, verwirft der Relay das ganze Frame samt
        // Vorgänger — die Cloud-Turnierleitung sähe GAR NICHTS mehr.
        let cfg = AppConfig::default();
        let (json, _) = state_for_relay(&turnier_mit_offenen(400, 400), &cfg, 1_000_000);
        assert!(
            json.len() <= relay_proto::MAX_TL_STATE_LEN,
            "Zustand zu groß: {} von {}",
            json.len(),
            relay_proto::MAX_TL_STATE_LEN
        );
    }

    #[test]
    fn die_revision_bleibt_an_der_vollen_fassung_haengen_auch_mit_offener_liste() {
        // Rev-Churn-Wächter: Die Revision entsteht aus dem VOLLEN Zustand.
        // Käme sie aus der zugeschnittenen Fassung, kippte der geteilte
        // Zähler bei jedem Takt hin und her und jede Seite lüde im
        // Sekundentakt neu.
        let tablet = turnier_mit_offenen(200, 150);
        let cfg = AppConfig::default();
        let lan = build_state_with_rev(&tablet, &cfg, 1_000_000);
        let (_json, relay_rev) = state_for_relay(&tablet, &cfg, 1_000_000);
        assert_eq!(lan.rev, relay_rev);

        let (_json2, relay_rev2) = state_for_relay(&tablet, &cfg, 1_060_000);
        assert_eq!(relay_rev, relay_rev2, "ohne Änderung keine neue Revision");
    }

    #[test]
    fn ein_turnier_ohne_offene_spiele_wird_genauso_zugeschnitten_wie_bisher() {
        // Gegenprobe: Die zusätzliche Stufe darf am Bestand nichts ändern.
        let cfg = AppConfig::default();
        let (json, _) = state_for_relay(&turnier_mit_offenen(200, 0), &cfg, 1_000_000);
        let state: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(state["open_queue"].as_array().unwrap().is_empty());
        assert_eq!(state["open_queue_truncated"].as_u64().unwrap(), 0);
    }

    #[test]
    fn the_relay_state_reports_everything_it_left_out() {
        // Die Kürzungsmeldung muss BEIDE Kappungen zusammenzählen (LAN-
        // Limit und Relay-Stufe) — sonst verschwiege sie der Turnierleitung
        // genau die Spiele, die der Cloud-Weg zusätzlich weglässt.
        let tablet = TabletState::default();
        let mut matches = Vec::new();
        for id in 100..=200 {
            matches.push(a_match(id));
        }
        let gesamt = matches.len();
        tablet.set_snapshot(snap(vec![a_court(3, None)], matches, Vec::new()));
        let (json, _rev) = state_for_relay(&tablet, &AppConfig::default(), 1_000_000);
        let state: serde_json::Value = serde_json::from_str(&json).unwrap();
        let gezeigt = state["queue"].as_array().unwrap().len();
        let gekappt = state["queue_truncated"].as_u64().unwrap() as usize;
        assert_eq!(gezeigt, 40, "der Relay-Weg zeigt höchstens 40");
        assert_eq!(
            gezeigt + gekappt,
            gesamt,
            "gezeigt + gemeldet muss die volle Liste ergeben"
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

        tablet.note_court_call(3, 7, false);
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
                collapsed: false,
                column: 1,
            }],
            display: relay_proto::TlDisplaySettingsWire {
                show_numbers: true,
                list_position: relay_proto::TlListPositionWire::Bottom,
                ..Default::default()
            },
            columns: 1,
            column_widths: Vec::new(),
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
                collapsed: true,
                column: 2,
            }],
            display: crate::config::TlDisplaySettings {
                show_club_names: true,
                list_position: crate::config::TlListPosition::Bottom,
                time_stats_axis: Default::default(),
                ..Default::default()
            },
            columns: 2,
            column_widths: vec![3.0, 1.0],
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
        assert!(
            view[0].panels[0].collapsed,
            "der Zuklapp-Zustand geht mit raus"
        );
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
    fn profile_save_and_view_pass_the_collapsed_flag_through_both_ways() {
        // Spec tl-liste-vereinfachen (D): Der Host reicht `collapsed` nur
        // durch — hin (Browser → `config.json`) und zurück (`TlState`).
        // Eine Anzeige-Logik gibt es serverseitig bewusst nicht.
        let mut config = AppConfig::default();
        let mut profil = wire_profile("profil-1", "Wandmonitor");
        profil.panels.push(relay_proto::TlPanelSettingWire {
            key: "queue".to_string(),
            visible: true,
            height_fr: 4.0,
            collapsed: true,
            column: 1,
        });
        profile_save(&mut config, &profil, 1_000).unwrap();

        let gespeichert = &config.tl_web.profiles[0].panels;
        assert!(!gespeichert[0].collapsed, "courts bleibt aufgeklappt");
        assert!(gespeichert[1].collapsed, "queue kommt zugeklappt an");

        let zurueck = &profiles_view(&config)[0].panels;
        assert!(!zurueck[0].collapsed);
        assert!(zurueck[1].collapsed);
    }

    #[test]
    fn profile_save_and_view_pass_the_column_layout_through_both_ways() {
        // Plan tl-liste-vereinfachen (F): Wie `collapsed` reicht der Host
        // Spaltenzahl, Spaltenbreiten und die Spalte je Panel nur DURCH.
        // Die Ableitung „columns == 0 ⇒ aus listPosition" sitzt allein in
        // `tl.html`; serverseitig darf sich an den Zahlen nichts ändern.
        let mut config = AppConfig::default();
        let mut profil = wire_profile("profil-1", "Wandmonitor");
        profil.columns = 3;
        profil.column_widths = vec![2.0, 1.0, 1.5];
        profil.panels.push(relay_proto::TlPanelSettingWire {
            key: "queue".to_string(),
            visible: true,
            height_fr: 1.0,
            collapsed: false,
            column: 3,
        });
        profile_save(&mut config, &profil, 1_000).unwrap();

        let gespeichert = &config.tl_web.profiles[0];
        assert_eq!(gespeichert.columns, 3);
        assert_eq!(gespeichert.column_widths, vec![2.0, 1.0, 1.5]);
        assert_eq!(gespeichert.panels[0].column, 1, "courts bleibt Spalte 1");
        assert_eq!(gespeichert.panels[1].column, 3);

        let zurueck = &profiles_view(&config)[0];
        assert_eq!(zurueck.columns, 3);
        assert_eq!(zurueck.column_widths, vec![2.0, 1.0, 1.5]);
        assert_eq!(zurueck.panels[1].column, 3);
    }

    #[test]
    fn profile_save_keeps_an_old_profile_without_column_fields_unchanged() {
        // Ein Browser von vor dem Mehrspalten-Layout schickt weder
        // `columns`/`columnWidths` noch `column` — der Host speichert dann
        // die Nullwerte, und `tl.html` liest daraus „aus listPosition
        // ableiten". Kein serverseitiges Vorbelegen: Sonst wäre die
        // Ableitung an ZWEI Stellen, die auseinanderlaufen können.
        let mut config = AppConfig::default();
        let mut profil = wire_profile("profil-1", "Alt");
        profil.columns = 0;
        profil.column_widths = Vec::new();
        profil.panels[0].column = 0;
        profile_save(&mut config, &profil, 1_000).unwrap();

        let gespeichert = &config.tl_web.profiles[0];
        assert_eq!(gespeichert.columns, 0);
        assert!(gespeichert.column_widths.is_empty());
        assert_eq!(gespeichert.panels[0].column, 0);
    }

    #[test]
    fn profile_save_rejects_oversized_column_width_list() {
        let mut config = AppConfig::default();
        let mut profil = wire_profile("profil-1", "Zu viele Breiten");
        profil.column_widths = vec![1.0; MAX_TL_PROFILE_COLUMN_WIDTHS + 1];
        let err = profile_save(&mut config, &profil, 1_000).unwrap_err();
        assert!(!err.ok);
        assert!(
            config.tl_web.profiles.is_empty(),
            "abgelehnt — nichts gespeichert"
        );
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
                collapsed: false,
                column: 1,
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
                side: None,
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
                side: relay_proto::PrepCallSide::Both,
            }
        );
        assert!(
            tablet.announce_jobs_since("Halle A", 0, 50_000).is_empty(),
            "Halle A geht der Aufruf nichts an"
        );
    }

    #[test]
    fn the_checkin_panel_lists_only_todays_scheduled_classes_sorted() {
        // Panel „Anfangszeiten" (Feldtest 17.08.2026, Empfehlungen
        // bestätigt): nur Klassen des Tages MIT gepflegter Anfangszeit,
        // nach Anfangszeit sortiert; Zeiten als „HH:MM"; Zähler reisen
        // mit, Spielernamen nie.
        use crate::badhub::checkin_state::CheckinClass;
        let heute = chrono::NaiveDate::from_ymd_opt(2026, 8, 17).unwrap();
        let klasse = |name: &str, starts: Option<&str>, closes: Option<&str>| CheckinClass {
            event_id: 1,
            name: name.into(),
            discipline: "HE".into(),
            starts_at: starts.map(str::to_string),
            closes_at: closes.map(str::to_string),
            opens_at: None,
            state: "open".into(),
            is_live: false,
            gemeldet: 16,
            eingecheckt: 12,
            players: Vec::new(),
        };
        let liste = vec![
            klasse("Nachmittag", Some("2026-08-17 13:30:00"), None),
            klasse(
                "Morgen",
                Some("2026-08-18 09:00:00"),
                Some("2026-08-18 08:30:00"),
            ),
            klasse("Ohne Zeit", None, None),
            // T-Trennzeichen: das Fallback-Format aus `deadline_text`.
            klasse(
                "Frueh",
                Some("2026-08-17T09:00:00"),
                Some("2026-08-17T08:30:00"),
            ),
        ];
        let zeilen = checkin_times_heute(&liste, heute);
        assert_eq!(zeilen.len(), 2, "nur heutige Klassen mit Anfangszeit");
        assert_eq!(zeilen[0].name, "Frueh", "nach Anfangszeit sortiert");
        assert_eq!(zeilen[0].starts_hm, "09:00");
        assert_eq!(zeilen[0].closes_hm, "08:30");
        assert_eq!(zeilen[1].name, "Nachmittag");
        assert_eq!(zeilen[1].starts_hm, "13:30");
        assert_eq!(
            zeilen[1].closes_hm, "13:30",
            "ohne Anmeldeschluss gilt die Anfangszeit — wie beim \
             Ansage-Countdown (deadline_text)"
        );
        assert_eq!(zeilen[0].entry_count, 16);
        assert_eq!(zeilen[0].checked_in_count, 12);
        assert_eq!(zeilen[0].window_state, "open");
    }

    /// Konfiguration, in der ein Profil „Aufrufe unbegrenzt" führt.
    fn config_mit_unbegrenzten_aufrufen() -> AppConfig {
        let mut config = AppConfig::default();
        config.tl_web.profiles.push(crate::config::TlPanelProfile {
            id: "p1".into(),
            name: "TL".into(),
            display: crate::config::TlDisplaySettings {
                unlimited_court_calls: true,
                ..Default::default()
            },
            ..Default::default()
        });
        config
    }

    #[test]
    fn without_the_option_the_court_call_keeps_its_cap_of_three() {
        // Review 17.08.2026: Die 3er-Klemme darf nur fallen, wenn ein
        // Profil „Aufrufe unbegrenzt" führt — sonst hinge die Invariante
        // eines Turniers ohne die Option allein am Client-Gating.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        tablet.announce_jobs_since("", 0, 50_000);
        for _ in 0..4 {
            apply_state_action(
                &tablet,
                &AppConfig::default(),
                50_000,
                &relay_proto::TlAction::AnnounceCourtCall {
                    court_id: 3,
                    match_id: 7,
                    side: None,
                },
            )
            .unwrap();
        }
        assert_eq!(tablet.calls_made(3, 7), 3, "ohne Option hält der Deckel");
    }

    #[test]
    fn with_the_option_the_fourth_call_counts_and_goes_out() {
        // Option „Aufrufe unbegrenzt": Der vierte Aufruf zählt ehrlich
        // weiter und geht als Stufe 4 hinaus — das Ansage-Gerät spricht
        // ihn ohne Stufenwort (`AnnounceJobPlayer`, Stufe >= 4).
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        tablet.announce_jobs_since("", 0, 50_000);
        let config = config_mit_unbegrenzten_aufrufen();
        let mut letzte = 0;
        for _ in 0..3 {
            apply_state_action(
                &tablet,
                &config,
                50_000,
                &relay_proto::TlAction::AnnounceCourtCall {
                    court_id: 3,
                    match_id: 7,
                    side: None,
                },
            )
            .unwrap();
            let jobs = tablet.announce_jobs_since("", letzte, 50_000);
            letzte = jobs.last().unwrap().id;
        }
        assert_eq!(tablet.calls_made(3, 7), 4, "der Zähler bleibt ehrlich");
        let jobs = tablet.announce_jobs_since("", 0, 50_000);
        assert_eq!(
            jobs.last().unwrap().kind,
            crate::tablet::state::AnnounceJobKind::CourtCall {
                court_id: 3,
                match_id: 7,
                stage: 4,
                side: relay_proto::PrepCallSide::Both,
            }
        );
    }

    #[test]
    fn a_call_to_a_running_match_is_spoken_without_a_stage_word() {
        // Sind Punkte gefallen, wäre „Zweiter Aufruf" oder gar „Dritter
        // und letzter Aufruf" (Walkover-Drohung!) mitten ins Spiel
        // absurd. Der Auftrag wird deshalb auf Stufe >= 4 gehoben — die
        // schlichte Feld-Ansage ohne Stufenwort. Der Zähler selbst zählt
        // ehrlich weiter.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        tablet.record_score(3, 7, vec![(11, 7)]);
        tablet.announce_jobs_since("", 0, 50_000);
        apply_state_action(
            &tablet,
            &config_mit_unbegrenzten_aufrufen(),
            50_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
                side: None,
            },
        )
        .unwrap();
        assert_eq!(tablet.calls_made(3, 7), 2, "gezählt wird ehrlich");
        let jobs = tablet.announce_jobs_since("", 0, 50_000);
        assert_eq!(
            jobs.last().unwrap().kind,
            crate::tablet::state::AnnounceJobKind::CourtCall {
                court_id: 3,
                match_id: 7,
                stage: 4,
                side: relay_proto::PrepCallSide::Both,
            }
        );
    }

    #[test]
    fn a_court_call_can_target_a_single_party() {
        // Spec tl-liste-vereinfachen E1: „2. Aufruf für Partei A/B" am
        // Feld — Vorbild ist der Vorbereitungs-Nachruf je Partei. Der
        // Auftrag trägt die Partei mit, damit das Ansage-Gerät nur diese
        // nennt; die Stufe zählt weiterhin der Host, einmal je Runde.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        tablet.announce_jobs_since("", 0, 50_000);

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
                side: Some(relay_proto::PrepCallSide::Team1),
            },
        )
        .unwrap();
        assert!(done.ok);
        assert_eq!(tablet.calls_made(3, 7), 2);

        let jobs = tablet.announce_jobs_since("", 0, 50_000);
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].kind,
            crate::tablet::state::AnnounceJobKind::CourtCall {
                court_id: 3,
                match_id: 7,
                stage: 2,
                side: relay_proto::PrepCallSide::Team1,
            }
        );

        // Die andere Partei gehört zur selben Runde — Stufe bleibt 2.
        apply_state_action(
            &tablet,
            &AppConfig::default(),
            51_000,
            &relay_proto::TlAction::AnnounceCourtCall {
                court_id: 3,
                match_id: 7,
                side: Some(relay_proto::PrepCallSide::Team2),
            },
        )
        .unwrap();
        assert_eq!(
            tablet.calls_made(3, 7),
            2,
            "Partei A und danach Partei B sind EIN Aufruf"
        );
        let jobs = tablet.announce_jobs_since("", jobs[0].id, 51_000);
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].kind,
            crate::tablet::state::AnnounceJobKind::CourtCall {
                court_id: 3,
                match_id: 7,
                stage: 2,
                side: relay_proto::PrepCallSide::Team2,
            }
        );
    }

    #[test]
    fn a_court_call_without_a_party_still_means_both() {
        // Rückwärtskompatibilität: Ein älterer Browser schickt kein `side`
        // — das muss weiterhin genau der bisherige Aufruf an beide sein.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let action: relay_proto::TlAction =
            serde_json::from_str(r#"{"action":"announce_court_call","courtId":3,"matchId":7}"#)
                .unwrap();
        apply_state_action(&tablet, &AppConfig::default(), 50_000, &action).unwrap();
        let jobs = tablet.announce_jobs_since("", 0, 50_000);
        assert_eq!(
            jobs[0].kind,
            crate::tablet::state::AnnounceJobKind::CourtCall {
                court_id: 3,
                match_id: 7,
                stage: 2,
                side: relay_proto::PrepCallSide::Both,
            }
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
                side: None,
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
                side: None,
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
                side: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Some(relay_proto::TlErrorCode::CourtFree));
        assert_eq!(tablet.calls_made(3, 99), 0, "nichts hochgezählt");
    }

    #[test]
    fn die_spielbeginn_ansage_laesst_call_stages_unberuehrt() {
        // Der Kern von A3.3: Die Ansage ist KEIN Aufruf. Zählte sie mit,
        // spränge das Aufruf-Abzeichen an der Kachel und die Turnierleitung
        // glaubte, sie hätte schon zweimal gerufen — bis hin zur kampflosen
        // Wertung, die am dritten Aufruf hängt.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        assert_eq!(tablet.calls_made(3, 7), 0, "Ausgangslage: nie gerufen");

        let done = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceStartPlay {
                court_id: 3,
                match_id: 7,
            },
        )
        .unwrap();

        assert!(done.ok);
        assert_eq!(
            tablet.calls_made(3, 7),
            0,
            "die Spielbeginn-Ansage darf keine Aufruf-Stufe verbrauchen"
        );
    }

    #[test]
    fn die_spielbeginn_ansage_braucht_ein_spiel_auf_dem_feld() {
        // Ohne Spiel gäbe es niemanden, der anfangen soll — ein Gong ins
        // Leere. Und ein Spielwechsel zwischen Antippen und Ankommen darf
        // nicht die falsche Paarung antreiben, deshalb muss auch die
        // Match-ID passen.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None), a_court(4, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));

        let leer = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceStartPlay {
                court_id: 4,
                match_id: 7,
            },
        );
        assert!(leer.is_err(), "freies Feld: nichts anzusagen");

        let falsches_spiel = apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceStartPlay {
                court_id: 3,
                match_id: 99,
            },
        );
        assert!(
            falsches_spiel.is_err(),
            "inzwischen steht ein anderes Spiel auf dem Feld"
        );
    }

    #[test]
    fn die_spielbeginn_ansage_ist_beliebig_oft_ausloesbar() {
        // Anders als der Aufruf kennt sie keine Obergrenze: Wer nach fünf
        // Minuten immer noch nicht spielt, wird eben noch einmal gebeten.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let aktion = relay_proto::TlAction::AnnounceStartPlay {
            court_id: 3,
            match_id: 7,
        };

        for durchgang in 1..=4 {
            let done = apply_state_action(&tablet, &AppConfig::default(), 50_000, &aktion);
            assert!(
                done.is_ok(),
                "Durchgang {durchgang} muss durchgehen — die Ansage kennt keine Stufe"
            );
        }
        assert_eq!(
            tablet.announce_jobs_since("", 0, 50_000).len(),
            4,
            "jede Auslösung erzeugt einen eigenen Auftrag"
        );
    }

    #[test]
    fn die_spielbeginn_ansage_geht_nur_in_die_halle_des_felds() {
        // In einem Zwei-Hallen-Turnier darf die andere Halle nicht mithören
        // — dort steht ein ganz anderes Feld 3.
        let tablet = TabletState::default();
        let mut schnappschuss = snap(
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
        );
        schnappschuss.court_infos.push(a_court(9, Some(1)));
        tablet.set_snapshot(schnappschuss);

        apply_state_action(
            &tablet,
            &AppConfig::default(),
            50_000,
            &relay_proto::TlAction::AnnounceStartPlay {
                court_id: 3,
                match_id: 7,
            },
        )
        .unwrap();

        let jobs = tablet.announce_jobs_since("Halle B", 0, 50_000);
        assert_eq!(jobs.len(), 1, "die Halle des Felds hört die Ansage");
        // Und es ist wirklich DIESE Ansage. Ohne die Typ-Prüfung wären alle
        // Tests hier auch dann grün, wenn der Arm versehentlich einen
        // Schiedsrichter-Auftrag ablegte — die Halle hörte im Turnier die
        // falsche Ansage (Review 18.08.2026).
        assert!(
            matches!(
                jobs[0].kind,
                crate::tablet::state::AnnounceJobKind::StartPlay {
                    court_id: 3,
                    match_id: 7
                }
            ),
            "erwartet wurde ein StartPlay-Auftrag für Feld 3/Spiel 7, war: {:?}",
            jobs[0].kind
        );
        assert!(
            tablet.announce_jobs_since("Halle A", 0, 50_000).is_empty(),
            "die andere Halle nicht"
        );
    }

    #[test]
    fn der_bediener_nachruf_zaehlt_getrennt_von_den_spieler_aufrufen() {
        // A2.5 und der ganze Grund für die eigene Ansageart: Zöge ein
        // Nachruf an die Bedienung die Spieler-Aufrufzahl hoch, glaubte die
        // Turnierleitung, sie hätte schon zweimal gerufen — und an der
        // dritten Stufe hängt die kampflose Wertung.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        tablet.officials_store().set_enabled(false);
        tablet.add_scorekeeper_manual(vec!["Anna Alt".to_string()], 1_000);
        tablet.assign_scorekeeper_for_court(3, 7);
        let mut cfg = AppConfig::default();
        cfg.scorekeeper.enabled = true;

        let done = apply_state_action(
            &tablet,
            &cfg,
            50_000,
            &relay_proto::TlAction::AnnounceScorekeeper { court_id: 3 },
        )
        .unwrap();

        assert!(done.ok);
        assert_eq!(tablet.scorekeeper_calls_made(3, 7), 1, "eigener Zähler");
        assert_eq!(
            tablet.calls_made(3, 7),
            0,
            "die Spieler-Aufrufzahl darf sich NICHT bewegen"
        );
        assert_eq!(
            tablet.prep_calls_made(7),
            0,
            "und der Vorbereitungs-Nachruf auch nicht"
        );
    }

    #[test]
    fn ein_neues_spiel_setzt_den_bediener_zaehler_zurueck() {
        // A2.6: Sonst erbte die neue Paarung die Nachrufe ihres Vorgängers
        // und stünde sofort beim „Dritten und letzten".
        let tablet = TabletState::default();
        assert_eq!(tablet.note_scorekeeper_call(3, 7), 1);
        assert_eq!(tablet.note_scorekeeper_call(3, 7), 2);
        assert_eq!(tablet.note_scorekeeper_call(3, 7), 3);
        assert_eq!(tablet.note_scorekeeper_call(3, 7), 3, "gedeckelt bei 3");
        assert_eq!(
            tablet.note_scorekeeper_call(3, 99),
            1,
            "anderes Spiel auf dem Feld: von vorn"
        );
    }

    #[test]
    fn ein_bediener_nachruf_ohne_zugewiesenen_bediener_wird_abgelehnt() {
        // A2.4: Ohne Zuweisung gäbe es niemanden zu nennen — ein Gong ohne
        // Inhalt. Die Seite zeigt den Knopf dann gar nicht erst; der Host
        // lehnt trotzdem ab, weil er sich nie auf die Seite verlässt.
        let tablet = TabletState::default();
        tablet.set_snapshot(snap(
            vec![a_court(3, None)],
            vec![match_on_court(7, 3)],
            Vec::new(),
        ));
        let mut cfg = AppConfig::default();
        cfg.scorekeeper.enabled = true;

        let abgelehnt = apply_state_action(
            &tablet,
            &cfg,
            50_000,
            &relay_proto::TlAction::AnnounceScorekeeper { court_id: 3 },
        );
        assert!(abgelehnt.is_err(), "kein Bediener ⇒ keine Ansage");
    }

    #[test]
    fn der_bediener_nachruf_legt_einen_auftrag_in_der_halle_des_felds_ab() {
        // A2.2/A2.3: Die Ansage geht nur in die Halle des Felds, trägt die
        // Stufe und nennt genau dieses Feld und Spiel.
        let tablet = TabletState::default();
        let mut schnappschuss = snap(
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
        );
        schnappschuss.court_infos.push(a_court(9, Some(1)));
        tablet.set_snapshot(schnappschuss);
        tablet.add_scorekeeper_manual(vec!["Anna Alt".to_string()], 1_000);
        tablet.assign_scorekeeper_for_court(3, 7);
        let mut cfg = AppConfig::default();
        cfg.scorekeeper.enabled = true;

        apply_state_action(
            &tablet,
            &cfg,
            50_000,
            &relay_proto::TlAction::AnnounceScorekeeper { court_id: 3 },
        )
        .unwrap();

        let jobs = tablet.announce_jobs_since("Halle B", 0, 50_000);
        assert_eq!(jobs.len(), 1, "die Halle des Felds hört den Nachruf");
        assert!(
            matches!(
                jobs[0].kind,
                crate::tablet::state::AnnounceJobKind::ScorekeeperCall {
                    court_id: 3,
                    match_id: 7,
                    stage: 1
                }
            ),
            "erwartet wurde ein ScorekeeperCall Feld 3/Spiel 7/Stufe 1, war: {:?}",
            jobs[0].kind
        );
        assert!(
            tablet.announce_jobs_since("Halle A", 0, 50_000).is_empty(),
            "die andere Halle nicht"
        );
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
                side: None,
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
                    side: None,
                },
                A::AnnounceCourtCall {
                    court_id: 2,
                    match_id: 7,
                    side: None,
                },
            ),
            (
                A::AnnounceCourtCall {
                    court_id: 1,
                    match_id: 7,
                    side: Some(relay_proto::PrepCallSide::Team1),
                },
                A::AnnounceCourtCall {
                    court_id: 1,
                    match_id: 7,
                    side: Some(relay_proto::PrepCallSide::Team2),
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
    fn member_ids_travel_next_to_the_names_in_the_queue() {
        // Link-Ziel der badhub-Spielerseite (`badhub.de/spieler/<Nr>/live`) —
        // die Seite verlinkt jeden Namen der Spielliste dorthin. Bewusste
        // Datenschutz-Freigabe (Nutzer-Entscheidung 17.08.2026, wie Nation
        // 09.08. und Verein 12.08.): Die Nummer ist der ÖFFENTLICHE
        // URL-Schlüssel genau dieser badhub-Seite und steht hier hinter dem
        // Gerätezugang. Parallel zu den Namen wie Nation/Verein; ohne
        // Nummer bleibt ein leerer Platz (die Seite verlinkt dann nicht).
        let tablet = TabletState::default();
        let mut wartend = a_match(2);
        wartend.team1 = vec![licensed_player("Weber", "08-017991")];
        wartend.team2 = vec![player("Fischer")];
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![wartend], Vec::new()));

        let s = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        assert_eq!(s.queue[0].team1_ids, vec!["08-017991"]);
        assert_eq!(
            s.queue[0].team2_ids,
            vec![""],
            "ohne Nummer ein leerer Platz — die Listen bleiben gleich lang"
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
    fn the_queue_cap_applies_globally_not_per_hall() {
        // ADR 0026: Die Spielliste ist EINE Abfolge, also gibt es auch nur
        // EINE Grenze. Die bis dahin geltende Kappung je Halle ist bewusst
        // aufgegeben — mit ihr hätte die Liste zwei Deckel, obwohl sie nur
        // noch eine Reihenfolge kennt. Dass dabei eine Halle ganz aus der
        // ausgelieferten Liste fallen kann, wird stattdessen ehrlich
        // gemeldet (`queue_truncated`) statt strukturell verhindert.
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
        for i in 1..=(QUEUE_LIMIT as i64 + 10) {
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

        assert_eq!(s.queue.len(), QUEUE_LIMIT, "genau ein globaler Deckel");
        let in_b = s.queue.iter().filter(|m| m.hall == "Halle B").count();
        assert_eq!(
            in_b, 0,
            "die später angesetzten Spiele der Halle B fallen hinten heraus"
        );
        assert_eq!(
            s.queue_truncated, 15,
            "was fehlt, wird gezählt und gemeldet"
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
            r#"{"pause":{"kind":"game","endsAt":1700000000000,"startedAt":1699999880000,"heimlich":"streng geheim"}}"#
                .to_string(),
        );

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let pause = s.courts[0].pause.as_ref().expect("Pause vorhanden");
        assert_eq!(pause.kind, "game");
        assert_eq!(pause.ends_at_ms, Some(1_700_000_000_000));
        assert_eq!(pause.started_at_ms, Some(1_699_999_880_000));
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("heimlich"),
            "unbekannte Felder des Tabletts dürfen nicht weiterwandern: {json}"
        );
    }

    /// Spec `spielzeiten-prognose` (E10): Die Behandlungspause hat kein
    /// `endsAt` — sie fiel beim typisierten Parse bisher KOMPLETT raus und
    /// war für die Turnierleitung unsichtbar. Jetzt kommt sie an (ohne
    /// Countdown, mit Beginn für die „seit …"-Anzeige).
    #[test]
    fn eine_behandlungspause_ohne_endzeit_erreicht_die_turnierleitung() {
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"injury","endsAt":null,"startedAt":900000}}"#.to_string(),
        );

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let pause = s.courts[0]
            .pause
            .as_ref()
            .expect("Behandlungspause sichtbar");
        assert_eq!(pause.kind, "injury");
        assert_eq!(pause.ends_at_ms, None);
        assert_eq!(pause.started_at_ms, Some(900_000));
    }

    /// Altes Tablet (ohne `startedAt`): die Pause kommt weiter an — nur
    /// eben ohne Beginn (Auto-Update-Fenster, sanfte Degradation).
    #[test]
    fn eine_pause_ohne_startzeit_kommt_weiter_an() {
        let tablet = TabletState::default();
        let mut running = a_match(7);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        tablet.set_snapshot(snap(vec![a_court(1, None)], vec![running], Vec::new()));
        tablet.attach_tablet(1);
        tablet.set_court_state(
            1,
            r#"{"pause":{"kind":"eleven","endsAt":1700000000000}}"#.to_string(),
        );

        let s = build_state(&tablet, &AppConfig::default(), 1_000_000, 1);
        let pause = s.courts[0].pause.as_ref().expect("Pause vorhanden");
        assert_eq!(pause.ends_at_ms, Some(1_700_000_000_000));
        assert_eq!(pause.started_at_ms, None);
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
            // Seit wann sieht ein Spiel fertig aus (Spec
            // tl-warnung-fertiges-spiel)? Ein Zeitstempel je Feld — sagt
            // nichts über Personen, nur über den Spielstand, der ohnehin in
            // "sets" steht.
            "decided_since_ms",
            // Frist derselben Warnung in Sekunden, aus der Konfiguration.
            "finished_warning_seconds",
            // Fähigkeitsmerkmal (Spec tl-wunschfeld): reines Bool „dieser
            // Turnier-PC kennt das Wunschfeld".
            "can_set_wish_court",
            // Fähigkeitsmerkmal (Spec tl-web-felder-sperren, E13): reines
            // Bool „dieser Turnier-PC kennt das Sperren". Sagt nichts über
            // Personen, nur über die Programmversion — und ohne es zeigte
            // die Seite einen Knopf, den ein älterer Host still verwirft.
            "can_lock_courts",
            // Fähigkeitsmerkmal (Spec tablet-version-abgleich): reines Bool
            // „dieser Turnier-PC kennt den Fernbefehl".
            "can_reload_tablets",
            // Hallen-Farbe (Spec hallen-farben): reiner Hex-Anzeigewert je
            // Halle — kein Personenbezug. Der Beendet-Hallenname reist als
            // ohnehin erlaubtes "hall".
            "color",
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
            // Hallen-Vorverteilung (Spec hallen-vorverteilung): reine
            // Betriebs-/Konfigurationsangaben ohne Personenbezug.
            "hall_prefill",
            "window",
            "effective_window",
            "blocked_by_active_hall",
            "second_call_minutes",
            "third_call_minutes",
            "not_started_minutes",
            "courts",
            "queue",
            "queue_truncated",
            // Spiele mit noch offener Paarung (Spec `tl-offene-paarungen`).
            // Die Liste trägt dieselben Angaben wie die Warteliste, nur
            // schlanker — und ausdrücklich OHNE Lizenznummern: Die Nummer
            // ist das Link-Ziel der badhub-Seite eines feststehenden
            // Teilnehmers, ein Kandidat bekommt sie nicht. Der
            // `_ids`-Pfad-Wächter weiter unten hält das strukturell fest.
            "open_queue",
            "open_queue_truncated",
            // Beschriftung eines offenen Platzes: entweder die Namen der
            // Kandidaten aus dem Vorspiel — dieselben Spielernamen, die
            // ohnehin in der Warteliste stehen — oder „aus Spiel 42" bzw.
            // „noch offen". Bewusst spezifisch benannt statt „candidates":
            // Dieser Name ist hier schon für die Walkover-Vorschläge
            // vergeben, und ein doppelt belegter Feldname macht die
            // Whitelist stumpf.
            "open_slot1_label",
            "open_slot2_label",
            // Position eines offenen Spiels in der gemeinsamen Reihenfolge —
            // eine reine Zahl, damit die Seite mischen kann, ohne selbst zu
            // sortieren.
            "queue_index",
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
            // Lizenznummer als Link-Ziel der badhub-Spielerseite
            // (`/spieler/<Nr>/live`; Nutzer-Entscheidung 17.08.2026,
            // ausgeweitet 18.08.2026 — bewusste Freigabe wie Nation/Verein):
            // Die Nummer ist der öffentliche URL-Schlüssel genau dieser
            // Seite und steht hier hinter dem Gerätezugang. Geburtsjahr
            // bleibt draußen.
            //
            // **Diese Liste ist flach** — sie erlaubt einen Feldnamen für
            // JEDE Struktur des Zustands. Als die Nummern am 18.08.2026 von
            // der Warteliste auf laufende und beendete Spiele ausgeweitet
            // wurden, hat dieser Wächter deshalb **nicht** angeschlagen; die
            // Ausweitung fing allein
            // `the_state_never_carries_personal_data_beyond_its_purpose`.
            // Wer hier eine Struktur-genaue Prüfung braucht, muss sie dort
            // führen, nicht hier.
            "team1_ids",
            "team2_ids",
            "sets",
            "tablet_connected",
            "injury",
            "official_call",
            "pause",
            "kind",
            "ends_at_ms",
            // Pausen-Beginn (Spec spielzeiten-prognose, E10): reine
            // Uhrzeit fürs „Behandlung seit …" — kein Personenbezug.
            "started_at_ms",
            "scorekeeper",
            "scorekeeper_assigned",
            "locked",
            "clearing",
            "on_court_since_ms",
            // Geschätzte Restminuten des laufenden Spiels (Etappe D) — eine
            // aus Satzstand und Medianen gerechnete Zahl, kein Personenbezug.
            "remaining_min",
            // Panel „Anfangszeiten" (Feldtest 17.08.2026): Check-In-Zeitplan
            // des Tages je Klasse — Klassenname, Zeiten, badhub-Fensterzustand
            // und die Zähler eingecheckt/gemeldet. Bewusst OHNE Spielerlisten
            // (die streift `checkin_state::tl_ablage` vor dem Ablegen ab),
            // also kein Personenbezug. Die Feldnamen sind absichtlich
            // spezifisch (`window_state` statt `state`, `entry_count` statt
            // `entries`): Diese Whitelist ist flach — generische Namen
            // ließen künftige gleichnamige Felder ungeprüft passieren.
            "checkin_times",
            "checkin_stale",
            "starts_hm",
            "closes_hm",
            "window_state",
            "entry_count",
            "checked_in_count",
            // Zahl der gesprochenen Aufrufe (mit „Aufrufe unbegrenzt"
            // nach oben offen) — keine Angabe zu Personen.
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
            // `player_key`-Schlüssel der blockierten Spieler (Lizenznummer
            // bzw. normalisierter Name — beide für die Warteliste ohnehin
            // freigegeben, siehe `team1_ids`): Die Seite färbt damit exakt
            // den betroffenen Namen, statt gleichnamige zu verschmelzen.
            "player_keys",
            "until_ms",
            // Spec `feldvergabe-ausnahme`: reines Bool-Flag „Auto-Vergabe
            // übergeht dieses Spiel gerade" — keine Angabe zu Personen.
            "excluded_from_auto_assign",
            // Spec `tl-wunschfeld`: eine CourtID, kein Personendatum. Die
            // Seite braucht sie, um zu zeigen, worauf ein Spiel wartet.
            "wish_court",
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
            // Spielzeiten & Prognose (Spec spielzeiten-prognose): alles
            // reine Zeiten/Zähler je MATCH — Uhrzeiten, Minuten-Mediane,
            // Anzahl Messungen, Klassen-/Disziplin-Kürzel (oben erlaubt).
            // Kein Personenbezug über die ohnehin gezeigten Namen hinaus.
            "predicted_start_ms",
            "predicted_uncertain",
            "brutto_mins",
            "netto_mins",
            "time_stats",
            "rows",
            // Die drei zusätzlichen Achsen der Spielzeiten-Auswertung
            // (Spec `tl-sicht-feinschliff`, Punkt 1). Sie tragen genau
            // dieselben Zahlen wie `rows`, nur anders gruppiert: Kürzel,
            // Zähler und Minuten-Mediane — keine Personendaten. Die
            // Hallen-Achse führt zusätzlich den BTP-Hallennamen, der in
            // diesem Zustand ohnehin an jedem Feld und jedem
            // Wartelisten-Eintrag steht.
            "by_class",
            "by_discipline",
            "by_hall",
            "count",
            "diff_mins",
            "tournament_brutto_mins",
            "default_mins",
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
            // Auf-/Zuklapp-Zustand eines Panels — reine Layout-Angabe wie
            // `visible`/`heightFr`, kein Personendatum.
            "collapsed",
            // Mehrspalten-Layout (Plan tl-liste-vereinfachen F): Spaltenzahl,
            // Spaltenbreiten und die Spalte je Panel — genauso reine
            // Layout-Zahlen wie `heightFr`, kein Personenbezug.
            "columns",
            "columnWidths",
            "column",
            "display",
            "showNumbers",
            "showNations",
            "showClubNames",
            "showClubLogos",
            "showDiscipline",
            "showRound",
            "showGroup",
            // Profil-Schalter für die Restzeit-Anzeige (Etappe D) — ein
            // Anzeige-Häkchen, kein Personenbezug.
            "showCourtRemaining",
            // Profil-Schalter „Aufrufe unbegrenzt" (Feldtest 17.08.2026) —
            // ebenfalls ein Anzeige-Häkchen, kein Personenbezug.
            "unlimitedCourtCalls",
            // Profil-Schalter „offene Paarungen ausblenden" (Spec
            // `tl-offene-paarungen`) — ebenfalls nur ein Häkchen. Invertiert
            // benannt, damit ein Profil ohne das Feld auf „anzeigen" steht.
            "hideOpenMatches",
            "listPosition",
            // Achse des Panels „Spielzeiten" (Spec `tl-sicht-feinschliff`)
            // — reine Anzeige-Präferenz wie die Häkchen daneben, ein Wort
            // aus vier festen Werten.
            "timeStatsAxis",
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
        // Ein Spiel mit noch offener Paarung samt seinem Vorspiel: Sonst
        // bliebe `open_queue` leer und der Wächter sähe die
        // `TlOpenMatch`-Felder nie — dieselbe Falle wie bei `finished`,
        // `layouts` und `checkin_times` oben.
        let mut vorspiel = a_match(4);
        vorspiel.planning_id = 1001;
        vorspiel.team1 = vec![player("Kandidat")];
        vorspiel.team2 = vec![player("Gegenkandidat")];
        let mut offen = a_match(5);
        offen.from1 = Some(1001);
        offen.team1 = Vec::new();
        offen.team2 = Vec::new();
        let mut schnappschuss = snap(
            vec![a_court(1, None)],
            vec![running, a_match(2), finished, vorspiel, offen],
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
        // Ebenso ein Check-In-Klassenstand mit Anfangszeit am „heute" des
        // Test-now (1_000_000 ms — `build_state` unten bekommt genau
        // diesen Wert, `heutiges_datum` leitet das Datum daraus ab; kein
        // Uhr-Aufruf, kein Mitternachts-Flakern): Ohne den Eintrag bliebe
        // `checkin_times` `null`, und der Wächter sähe die
        // `TlCheckinTime`-Felder nie.
        tablet.set_checkin_classes(Some(vec![crate::badhub::checkin_state::CheckinClass {
            event_id: 7,
            name: "U15 HE-A".into(),
            discipline: "HE".into(),
            starts_at: Some(format!(
                "{} 09:00:00",
                heutiges_datum(1_000_000).format("%Y-%m-%d")
            )),
            closes_at: Some(format!(
                "{} 08:30:00",
                heutiges_datum(1_000_000).format("%Y-%m-%d")
            )),
            opens_at: None,
            state: "open".into(),
            is_live: false,
            gemeldet: 16,
            eingecheckt: 12,
            players: Vec::new(),
        }]));
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
                collapsed: false,
                column: 1,
            }],
            display: crate::config::TlDisplaySettings {
                show_numbers: true,
                show_nations: true,
                show_club_names: true,
                show_club_logos: true,
                show_discipline: true,
                show_round: true,
                show_group: true,
                show_court_remaining: true,
                unlimited_court_calls: true,
                hide_open_matches: false,
                list_position: crate::config::TlListPosition::Bottom,
                time_stats_axis: Default::default(),
            },
            updated_at_ms: 1_000,
            ..Default::default()
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
            !s.open_queue.is_empty(),
            "Fixture-Fehler: das Fixture muss ein Spiel mit offener Paarung \
             enthalten, sonst prüft dieser Test die `TlOpenMatch`-Felder gar nicht"
        );
        assert!(
            !s.profiles.is_empty(),
            "Fixture-Fehler: das Fixture muss ein Panel-Profil enthalten, \
             sonst prüft dieser Test die `TlPanelProfileWire`-Felder gar nicht"
        );
        assert!(
            s.checkin_times.as_ref().is_some_and(|z| !z.is_empty()),
            "Fixture-Fehler: das Fixture muss eine heutige Check-In-Klasse \
             enthalten, sonst prüft dieser Test die `TlCheckinTime`-Felder \
             gar nicht"
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
    fn laufende_und_beendete_spiele_tragen_die_lizenznummer_als_linkziel() {
        // Spec `tl-sicht-feinschliff` A4.1/A4.2: Die Turnierleitung schlägt
        // Spieler während des Turniers auf der öffentlichen badhub-Seite
        // nach — an der Feldkachel und in der Beendet-Liste genauso wie in
        // der Warteliste, wo es das seit 17.08.2026 gibt. Ohne die Nummer im
        // Zustand kann die Seite dort keinen Link bauen.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![licensed_player("Müller", "08-001234")];
        running.team2 = vec![licensed_player("Schmidt", "08-005678")];
        let mut finished = a_match(2);
        finished.status = MatchStatus::Finished;
        finished.winner = Some(1);
        finished.finished_at = Some(500_000);
        finished.team1 = vec![licensed_player("Winter", "08-003333")];
        finished.team2 = vec![licensed_player("Sommer", "08-004444")];

        let s = state_with(
            snap(vec![a_court(1, None)], vec![running, finished], Vec::new()),
            &AppConfig::default(),
        );

        let feld = s.courts.iter().find(|c| c.court_id == 1).expect("Feld 1");
        assert_eq!(feld.team1_ids, vec!["08-001234".to_string()]);
        assert_eq!(feld.team2_ids, vec!["08-005678".to_string()]);

        let beendet = s.finished.first().expect("ein beendetes Spiel");
        assert_eq!(beendet.team1_ids, vec!["08-003333".to_string()]);
        assert_eq!(beendet.team2_ids, vec!["08-004444".to_string()]);
    }

    #[test]
    fn ein_spieler_ohne_lizenznummer_bleibt_ohne_id() {
        // Spec `tl-sicht-feinschliff` A4.3: Nicht jeder Spieler hat eine
        // Lizenznummer (Gastspieler, Papier-Meldung). Die Liste bleibt
        // **parallel** zu den Namen — sonst rutschte der Link eines Doppels
        // auf den falschen Partner. Der Namenlose bekommt einen leeren
        // Eintrag, keinen fehlenden.
        let mut running = a_match(1);
        running.status = MatchStatus::OnCourt;
        running.court_id = Some(1);
        running.team1 = vec![
            player("Ohne Nummer"),
            licensed_player("Mit Nummer", "08-007777"),
        ];
        let mut finished = a_match(2);
        finished.status = MatchStatus::Finished;
        finished.winner = Some(1);
        finished.finished_at = Some(500_000);
        finished.team1 = vec![player("Papier")];
        finished.team2 = vec![player("Kampflos")];

        let s = state_with(
            snap(vec![a_court(1, None)], vec![running, finished], Vec::new()),
            &AppConfig::default(),
        );

        let feld = s.courts.iter().find(|c| c.court_id == 1).expect("Feld 1");
        assert_eq!(
            feld.team1_ids,
            vec![String::new(), "08-007777".to_string()],
            "die Nummern bleiben Stellung für Stellung parallel zu den Namen"
        );
        assert_eq!(feld.team1.len(), feld.team1_ids.len());

        let beendet = s.finished.first().expect("ein beendetes Spiel");
        assert_eq!(beendet.team1_ids, vec![String::new()]);
        assert_eq!(beendet.team2_ids, vec![String::new()]);
    }

    #[test]
    fn auch_die_beendet_liste_haelt_gemischte_nummern_stellungsgleich() {
        // Derselbe Mischfall wie am laufenden Feld, aber in der
        // Beendet-Liste: ein Doppel, bei dem nur einer eine Lizenznummer
        // hat. Beide Wege bauen die Liste getrennt — ohne eigenen Test
        // fiele eine Abweichung in genau einem von beiden nicht auf.
        let mut finished = a_match(1);
        finished.status = MatchStatus::Finished;
        finished.winner = Some(2);
        finished.finished_at = Some(500_000);
        finished.team1 = vec![
            licensed_player("Mit Nummer", "08-001111"),
            player("Ohne Nummer"),
        ];
        finished.team2 = vec![player("Auch ohne"), licensed_player("Und mit", "08-002222")];

        let s = state_with(
            snap(Vec::new(), vec![finished], Vec::new()),
            &AppConfig::default(),
        );

        let b = s.finished.first().expect("ein beendetes Spiel");
        assert_eq!(b.team1_ids, vec!["08-001111".to_string(), String::new()]);
        assert_eq!(b.team2_ids, vec![String::new(), "08-002222".to_string()]);
        assert_eq!(b.team1.len(), b.team1_ids.len());
        assert_eq!(b.team2.len(), b.team2_ids.len());
    }

    #[test]
    fn ein_freies_und_ein_abzuraeumendes_feld_tragen_keine_nummern() {
        // Zwei Zustände ohne laufendes Spiel, die trotzdem eine Kachel
        // erzeugen: das schlicht freie Feld und das Feld, das ein bereits
        // beendetes Spiel noch hält, weil BTP es nicht abgeräumt hat
        // (`clearing`). Beide dürfen keine Lizenznummern tragen — sonst
        // stünde an einer Kachel ohne Namen ein Link auf eine fremde
        // Spielerseite.
        // Das „abzuräumende" Feld: BTP führt das Spiel noch am Feld
        // (`court_id` gesetzt, kein Sieger, nicht `Finished`), aber es
        // steht nicht mehr `OnCourt` — genau die Lücke, die `clearing`
        // beschreibt. Wäre es als beendet markiert, gälte das Feld schlicht
        // als frei.
        let mut abgeraeumt = a_match(1);
        abgeraeumt.status = MatchStatus::Scheduled;
        abgeraeumt.court_id = Some(1);
        abgeraeumt.team1 = vec![licensed_player("Winter", "08-003333")];
        abgeraeumt.team2 = vec![licensed_player("Sommer", "08-004444")];

        let s = state_with(
            snap(
                vec![a_court(1, None), a_court(2, None)],
                vec![abgeraeumt],
                Vec::new(),
            ),
            &AppConfig::default(),
        );

        for court_id in [1, 2] {
            let c = s
                .courts
                .iter()
                .find(|c| c.court_id == court_id)
                .unwrap_or_else(|| panic!("Feld {court_id}"));
            assert_eq!(c.match_id, 0, "auf Feld {court_id} läuft nichts");
            assert!(
                c.team1_ids.is_empty() && c.team2_ids.is_empty(),
                "Feld {court_id} ohne laufendes Spiel darf keine Nummern tragen"
            );
        }
        // Gegenprobe, dass das Fixture wirkt: Feld 1 ist als „abzuräumen"
        // erkannt, nicht einfach frei.
        let eins = s.courts.iter().find(|c| c.court_id == 1).expect("Feld 1");
        assert_eq!(eins.clearing, Some(1), "Feld 1 hält das beendete Spiel");
    }

    /// Sanktionsdaten (Karten, Disqualifikationen) gehören **ausschließlich**
    /// auf den Zettel — nie in den Anzeige-Zustand, den jedes gekoppelte
    /// TL-Gerät über eine aus dem Internet erreichbare Seite bekommt
    /// (Spec `schiedsrichterzettel-druck`, ADR 0037).
    ///
    /// Der Wächter prüft **strukturell**, nicht per Textregel: erst der
    /// Positiv-Nachweis, dass der Fixture wirklich Karten trägt, dann die
    /// Gegenprobe gegen Text **und** Struktur des `TlState`. Ohne den
    /// ersten Schritt bewiese der zweite nichts.
    #[test]
    fn sanktionsdaten_erreichen_den_anzeige_zustand_nie() {
        let tablet = TabletState::default();
        let mut laufend = a_match(42);
        laufend.status = MatchStatus::OnCourt;
        laufend.court_id = Some(1);
        let mut schnappschuss = snap(Vec::new(), vec![laufend], Vec::new());
        schnappschuss.tournament_name = "Test-Cup".into();
        tablet.set_snapshot(schnappschuss);

        // Fixture bewusst mit Sanktionsdaten bestücken.
        let karte = relay_proto::MatchEvent {
            id: "ff01".into(),
            seq: 1,
            set: 1,
            after_n: 3,
            score_a: 2,
            score_b: 1,
            ts_ms: 1_755_600_000_000,
            kind: relay_proto::EventKind::CardRed,
            team: 1,
            player: 0,
            receiver_team: 0,
            receiver_player: 0,
            phase: relay_proto::Phase::Play,
            retracts: String::new(),
        };
        let schwarz = relay_proto::MatchEvent {
            id: "ff02".into(),
            seq: 2,
            kind: relay_proto::EventKind::CardBlack,
            ..karte.clone()
        };
        assert!(tablet.sheet_store().apply_event(42, karte));
        assert!(tablet.sheet_store().apply_event(42, schwarz));

        // Positiv-Nachweis: Die Karten sind wirklich im Store — sonst
        // prüfte die Gegenprobe unten nichts.
        let sheet = tablet.sheet_store().sheet(42).expect("Zettel-Stand");
        assert_eq!(sheet.events.len(), 2, "Fixture trägt keine Karten");
        assert!(
            sheet.events.iter().any(|e| e.kind.is_sanction()),
            "Fixture trägt keine Sanktionsdaten"
        );

        // Gegenprobe: weder im Text …
        let state = build_state(&tablet, &AppConfig::default(), 1_000, 1);
        let json = serde_json::to_string(&state).unwrap();
        for verboten in [
            "card_red",
            "card_black",
            "card_yellow",
            "disqualified",
            "ff01",
            "ff02",
            "sanktion",
            "Sanktion",
        ] {
            assert!(
                !json.contains(verboten),
                "Sanktionsdatum '{verboten}' im Anzeige-Zustand: {json}"
            );
        }

        // … noch in der Struktur: kein Feld heißt nach Ereignissen oder
        // Karten. Ein nachgerüstetes Feld bricht hier, nicht erst im
        // Turnier.
        let wert: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut felder = Vec::new();
        fn sammle(v: &serde_json::Value, out: &mut Vec<String>) {
            match v {
                serde_json::Value::Object(map) => {
                    for (k, inner) in map {
                        out.push(k.clone());
                        sammle(inner, out);
                    }
                }
                serde_json::Value::Array(items) => {
                    for i in items {
                        sammle(i, out);
                    }
                }
                _ => {}
            }
        }
        sammle(&wert, &mut felder);
        for feld in &felder {
            let klein = feld.to_lowercase();
            assert!(
                !klein.contains("card") && !klein.contains("karte") && !klein.contains("sanktion"),
                "Feld '{feld}' im Anzeige-Zustand deutet auf Sanktionsdaten"
            );
        }
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
        //
        // **Die Lizenznummer war ab 17.08.2026 nur in der Warteliste
        // erlaubt und ist seit 18.08.2026 überall im Zustand erlaubt**
        // (Nutzer-Entscheidung, Spec `tl-sicht-feinschliff` Punkt 4). Der
        // Zweck ist derselbe geblieben und gilt an allen drei Stellen:
        // **Nachschlagen der Spielerhistorie auf der öffentlichen
        // badhub-Seite während des Turniers.** Die Nummer IST der
        // öffentliche URL-Schlüssel genau dieser Seite
        // (`/spieler/<Nr>/live`) — sie preiszugeben heißt hier, einen
        // ohnehin öffentlichen Schlüssel hinter dem Gerätezugang zu
        // wiederholen. Die frühere Beschränkung auf die Warteliste war
        // keine Datenschutz-Grenze, sondern schlicht die Stelle, an der
        // zuerst verlinkt wurde.
        //
        // **Unverändert draußen bleiben** (Verbotsliste unten): Geburtsjahr
        // überall, Check-In-Spielernamen, Sperrlisten und Stammverein der
        // Schiedsrichter. Der Feldname `member` bleibt ebenfalls verboten —
        // die Nummern reisen als `team1_ids`/`team2_ids`, nicht als roher
        // BTP-Spieler-Datensatz.
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
        // Ein Spiel mit offener Paarung, dessen Vorspiel LIZENZIERTE Spieler
        // hat (Spec `tl-offene-paarungen`): Ihre Namen dürfen als Kandidaten
        // mitreisen, ihre Lizenznummern nicht. Ohne diesen Eintrag bliebe
        // `open_queue` leer, und der `_ids`-Pfad-Wächter unten könnte gar
        // nicht belegen, dass die offene Liste keine Nummern trägt.
        let mut vorspiel = a_match(4);
        vorspiel.planning_id = 1001;
        vorspiel.team1 = vec![licensed_player("Kandidat", "08-007070")];
        vorspiel.team2 = vec![licensed_player("Gegenkandidat", "08-008080")];
        let mut offen = a_match(5);
        offen.from1 = Some(1001);
        offen.team1 = Vec::new();
        offen.team2 = Vec::new();

        // Wie im Allowlist-Wächter: Warteschlange der Zähltafelbediener nicht
        // leer lassen, sonst prüft dieser Test auch deren Felder nie.
        // `state_with` reicht dafür nicht (der Tablet-Zustand bleibt darin
        // gekapselt) — deshalb hier wie dort von Hand aufgebaut.
        let tablet = TabletState::default();
        let mut schnappschuss = snap(
            vec![a_court(1, None)],
            vec![running, waiting, finished, vorspiel, offen],
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
                collapsed: false,
                column: 1,
            }],
            display: crate::config::TlDisplaySettings {
                show_numbers: true,
                ..Default::default()
            },
            updated_at_ms: 1_000,
            ..Default::default()
        });
        // Auch hier ein Check-In-Klassenstand (Panel „Anfangszeiten"):
        // Der Wert-Wächter unten soll strukturell mitprüfen, dass die
        // Ablage wirklich nur Zeitplan und Zähler trägt — die
        // Spielerlisten streift `checkin_state::tl_ablage` ab, und genau
        // dieses Fixture merkte es, wenn das jemand entfernte.
        tablet.set_checkin_classes(Some(crate::badhub::checkin_state::tl_ablage(vec![
            crate::badhub::checkin_state::CheckinClass {
                event_id: 7,
                name: "U15 HE-A".into(),
                discipline: "HE".into(),
                starts_at: Some(format!(
                    "{} 09:00:00",
                    heutiges_datum(1_000_000).format("%Y-%m-%d")
                )),
                closes_at: None,
                opens_at: None,
                state: "open".into(),
                is_live: false,
                gemeldet: 16,
                eingecheckt: 12,
                players: vec![crate::badhub::checkin_state::CheckinPlayer {
                    player_id: 1,
                    entry_id: 0,
                    first: "Geheim".into(),
                    last: "Bleibtdrin".into(),
                    club: None,
                    nationality: None,
                    state: "open".into(),
                    source: None,
                    locked: false,
                    checked_in_at: None,
                }],
            },
        ])));
        let s = build_state(&tablet, &config, 1_000_000, 7);
        assert!(
            !s.finished.is_empty(),
            "Fixture-Fehler: das Fixture muss ein beendetes Spiel enthalten"
        );
        assert!(
            s.checkin_times.as_ref().is_some_and(|z| !z.is_empty()),
            "Fixture-Fehler: das Fixture muss eine heutige Check-In-Klasse enthalten"
        );
        // Der Fixture-Spielername darf NIRGENDS im Zustand auftauchen —
        // die Ablage trägt nur Zeitplan und Zähler.
        let roh = serde_json::to_string(&s).unwrap();
        assert!(
            !roh.contains("Bleibtdrin"),
            "Check-In-Spielernamen dürfen den TL-Zustand nie erreichen"
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
            "member", // roher BTP-Spieler-Datensatz (die Nummern reisen
            // als `team1_ids`/`team2_ids`, siehe Kopfkommentar)
            "birth", // Geburtsjahr — laut Projektregel nirgends
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
        // Die Lizenznummer darf an allen drei Stellen — sie ist das
        // Link-Ziel der badhub-Spielerseite (Freigabe 18.08.2026, siehe
        // Kopfkommentar). Positiv geprüft statt nur geduldet: Fiele eine der
        // drei Stellen still weg, wäre der Link dort tot, und niemand
        // merkte es.
        for (nummer, stelle) in [
            ("08-009999", "die Warteliste"),
            ("08-001234", "das laufende Spiel"),
            ("08-003333", "das beendete Spiel"),
            ("08-004444", "das beendete Spiel (Gegenseite)"),
        ] {
            assert!(
                json.contains(nummer),
                "{stelle} braucht die Lizenznummer als badhub-Link-Ziel"
            );
        }
        // …und NUR an diesen drei Stellen. Das ist der Ersatz für die
        // frühere Verbotsliste: Solange die Nummern verboten waren, fing
        // ein simpler Textvergleich jede neue Struktur, die sie mitnahm.
        // Seit sie erlaubt sind, muss die Prüfung strukturbezogen sein —
        // sonst könnten sie unbemerkt auch in Vorbereitungs-Aufrufen,
        // Walkover-Listen, Schiedsrichter-Einträgen oder im
        // Anfangszeiten-Panel auftauchen (Security-Review 18.08.2026).
        let baum: serde_json::Value = serde_json::to_value(&s).unwrap();
        let mut fundorte: Vec<String> = Vec::new();
        fn suche_ids(wert: &serde_json::Value, pfad: &str, fundorte: &mut Vec<String>) {
            match wert {
                serde_json::Value::Object(map) => {
                    for (schluessel, unterwert) in map {
                        if schluessel.ends_with("_ids") {
                            fundorte.push(format!("{pfad}.{schluessel}"));
                        }
                        suche_ids(unterwert, &format!("{pfad}.{schluessel}"), fundorte);
                    }
                }
                // Listenindex bewusst weglassen: Der Pfad soll die
                // STRUKTUR benennen, nicht den zufälligen Eintrag.
                serde_json::Value::Array(werte) => {
                    for unterwert in werte {
                        suche_ids(unterwert, &format!("{pfad}[]"), fundorte);
                    }
                }
                _ => {}
            }
        }
        suche_ids(&baum, "", &mut fundorte);
        fundorte.sort();
        fundorte.dedup();
        let erlaubt = [
            ".queue[].team1_ids",
            ".queue[].team2_ids",
            ".courts[].team1_ids",
            ".courts[].team2_ids",
            ".finished[].team1_ids",
            ".finished[].team2_ids",
        ];
        for ort in &fundorte {
            assert!(
                erlaubt.contains(&ort.as_str()),
                "Lizenznummern an einer nicht freigegebenen Stelle: {ort} \
                 — Zweck (badhub-Link) prüfen und hier bewusst eintragen"
            );
        }
        assert_eq!(
            fundorte.len(),
            erlaubt.len(),
            "Fixture-Fehler: es müssen alle drei Stellen belegt sein, gefunden: {fundorte:?}"
        );
        // Die offene Liste ist bewusst NICHT unter den erlaubten Stellen:
        // Der Zähler oben schlägt an, sobald jemand ihr `team1_ids`
        // nachrüstet. Damit das etwas beweist, muss sie belegt sein — und
        // der Kandidatenname muss ankommen, sonst wäre die Anzeige nutzlos.
        let offener = s
            .open_queue
            .first()
            .expect("Fixture-Fehler: die offene Liste muss belegt sein");
        assert!(
            offener.open_slot1_label.contains("Kandidat"),
            "der Kandidatenname aus dem Vorspiel gehört in die Anzeige, war: {}",
            offener.open_slot1_label
        );
        let offene_liste = serde_json::to_string(&s.open_queue).unwrap();
        for nummer in ["08-007070", "08-008080"] {
            assert!(
                !offene_liste.contains(nummer),
                "ein Kandidat darf keine Lizenznummer mitbringen: {offene_liste}"
            );
        }
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
