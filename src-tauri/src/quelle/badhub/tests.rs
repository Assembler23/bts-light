//! Tests der Quelle badhub: Abbildung Kern-Stand → `BtpSnapshot`, Kopplung
//! und Abgleich gegen eine badhub-Attrappe.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use turnierkern::*;

use super::abbildung::{abbilden, spiel_id};
use super::paket::*;
use super::*;
use crate::btp::model::{Discipline, MatchResult, MatchStatus};

pub(super) const UUID: &str = "0f8b2c1e-1111-4222-8333-444455556666";
const VOR: &str = "2026-12-01T12:00:00";
const SCHLUSS: &str = "2026-12-05T23:59:59";
pub(super) const T1: &str = "2026-12-06T10:00:00";
pub(super) const T2: &str = "2026-12-06T10:25:00";

fn k(key: &str) -> Konkurrenz {
    Konkurrenz {
        key: key.into(),
        personen: 1,
        max_meldungen: None,
        altersklassen: vec![],
        gruppe: None,
        freimeldung: false,
    }
}

/// Wie badhub: jeder Befehl über JSON (Lehre aus Kern v0.9.0).
pub(super) fn befehl(
    log: &mut Vec<TurnierEreignis>,
    b: TurnierBefehl,
) -> Result<(), TurnierAblehnung> {
    let b: TurnierBefehl = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
    let neu = pruefen_turnier(&falten_turnier(log), &b)?;
    log.extend(neu);
    Ok(())
}

/// Turnier mit HE (`n` Meldungen, System `s`), ausgelost und freigegeben,
/// Zeitplan auf den Hallen `hallen` (Kennung, Feldzahl) gerechnet.
pub(super) fn turnier(n: u32, s: System, hallen: &[(&str, u32)]) -> Vec<TurnierEreignis> {
    let wege: BTreeSet<Weg> = [Weg::Veranstalter].into();
    let mut log = vec![];
    befehl(
        &mut log,
        TurnierBefehl::Eroeffnen {
            id: "auf".into(),
            meldeschluss: SCHLUSS.into(),
            wege,
            nachruecken: Nachruecken::Veranstalter,
            konkurrenzen: vec![k("HE")],
            nachmeldeschluss: None,
            eine_je_gruppe: false,
        },
    )
    .unwrap();
    for p in 1..=n {
        befehl(
            &mut log,
            TurnierBefehl::Melden {
                id: format!("m{p}"),
                konkurrenz: "HE".into(),
                personen: vec![PersonRef {
                    person: 100 + p,
                    altersklasse: None,
                }],
                weg: Weg::Veranstalter,
                akteur: Akteur::Veranstalter,
                jetzt: VOR.into(),
                partner_bestaetigung: false,
            },
        )
        .unwrap();
        befehl(
            &mut log,
            TurnierBefehl::StatusSetzen {
                id: format!("b{p}"),
                meldung: p,
                status: Status::Confirmed,
            },
        )
        .unwrap();
    }
    befehl(
        &mut log,
        TurnierBefehl::SystemFestlegen {
            id: "sys".into(),
            konkurrenz: "HE".into(),
            system: s,
            teilnehmer: None,
        },
    )
    .unwrap();
    befehl(
        &mut log,
        TurnierBefehl::ZeitplanRahmenFestlegen {
            id: "rahmen".into(),
            rahmen: ZeitplanRahmen {
                tage: vec![Spieltag {
                    datum: "2026-12-06".into(),
                    beginn: "09:00".into(),
                    ende: "20:00".into(),
                }],
                hallen: hallen
                    .iter()
                    .map(|(id, f)| Halle {
                        id: (*id).into(),
                        felder: *f,
                    })
                    .collect(),
                dauer: 30,
                dauer_je: BTreeMap::new(),
                pause: 20,
                reihenfolge: vec![],
            },
        },
    )
    .unwrap();
    befehl(
        &mut log,
        TurnierBefehl::Auslosen {
            id: "los".into(),
            konkurrenz: "HE".into(),
            einstellung: AuslosungEinstellung::default(),
            los: 3,
            merkmale: BTreeMap::new(),
        },
    )
    .unwrap();
    befehl(
        &mut log,
        TurnierBefehl::AuslosungFreigeben {
            id: "frei".into(),
            konkurrenz: "HE".into(),
        },
    )
    .unwrap();
    befehl(
        &mut log,
        TurnierBefehl::ZeitplanRechnen {
            id: "plan".into(),
            ab: None,
        },
    )
    .unwrap();
    log
}

pub(super) fn paket_fuer(log: Vec<TurnierEreignis>, n: u32, hallen: Vec<PaketHalle>) -> Paket {
    Paket {
        schema: SCHEMA,
        kern_version: KERN_VERSION.into(),
        turnier: PaketTurnier {
            id: 17,
            uuid: UUID.into(),
            name: "Wintercup".into(),
            beginn: Some("2026-12-06".into()),
            ende: None,
        },
        ereignisse: log,
        konkurrenzen: vec![PaketKonkurrenz {
            key: "HE".into(),
            name: "Herreneinzel A".into(),
            disziplin: "HE".into(),
            nr: 3,
        }],
        rahmen: PaketRahmen {
            hallen,
            mindestpause_min: Some(20),
        },
        runden: BTreeMap::new(),
        meldungen: (1..=n)
            .map(|m| PaketMeldung {
                meldung: m,
                konkurrenz: "HE".into(),
                personen: vec![PaketPerson {
                    person: 100 + m,
                    vorname: format!("V{m}"),
                    nachname: format!("N{m}"),
                    verein: Some(format!("Verein {m}")),
                    nation: Some("GER".into()),
                }],
            })
            .collect(),
        schiedsrichter: vec![PaketSchiedsrichter {
            id: 4,
            name: "Sabine Pfiff".into(),
            person: None,
        }],
        stammdaten_etag: Some("\"s1\"".into()),
    }
}

pub(super) fn eine_halle() -> Vec<PaketHalle> {
    vec![PaketHalle {
        id: "H1".into(),
        name: "Halle 1".into(),
        felder: vec!["1".into(), "2".into()],
    }]
}

pub(super) fn melden(
    log: &mut Vec<TurnierEreignis>,
    id: &str,
    spiel: u32,
    saetze: &[(u16, u16)],
    ende: Option<Spielende>,
) {
    befehl(
        log,
        TurnierBefehl::ErgebnisMelden {
            id: id.into(),
            konkurrenz: "HE".into(),
            spiel,
            saetze: saetze.to_vec(),
            ende,
            jetzt: T2.into(),
            beginn: None,
            bis: None,
        },
    )
    .unwrap();
}

pub(super) fn aufrufen(
    log: &mut Vec<TurnierEreignis>,
    id: &str,
    spiel: u32,
    feld: &str,
) -> Result<(), TurnierAblehnung> {
    befehl(
        log,
        TurnierBefehl::SpielAufrufen {
            id: id.into(),
            konkurrenz: "HE".into(),
            spiel,
            feld: Some(feld.into()),
            jetzt: T1.into(),
        },
    )
}

pub(super) const OBEN: &[(u16, u16)] = &[(21, 10), (21, 10)];

#[test]
fn ko_mit_fortschreiben_bildet_ids_kanten_und_namen_ab() {
    let mut log = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    let snap = abbilden(
        &falten_turnier(&log),
        &paket_fuer(log.clone(), 4, eine_halle()),
    );
    assert_eq!(snap.tournament_name, "Wintercup");
    assert_eq!(snap.rest_minutes, Some(20));
    assert_eq!(snap.matches.len(), 3);
    let ids: Vec<i64> = snap.matches.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![300_001, 300_002, 300_003]);
    assert_eq!(spiel_id(3, 2), 300_002);
    let fin = &snap.matches[2];
    // Finale wartet: Plätze offen, Kanten zeigen auf die Halbfinals.
    assert!(fin.team1.is_empty() && fin.team2.is_empty());
    assert_eq!((fin.entry1_id, fin.entry2_id), (0, 0));
    assert_eq!((fin.from1, fin.from2), (Some(1), Some(2)));
    assert_eq!(fin.round_name, "Finale");
    assert_eq!(snap.matches[0].round_name, "Halbfinale");
    assert_eq!(fin.draw_id, 3);
    assert_eq!(fin.discipline, Discipline::MensSingles);
    // Vorspiel-Auflösung des TL-Webs funktioniert über dieselbe Kante.
    let vor = crate::tablet::tl::vorspiel_fuer(&snap, fin, 1).unwrap();
    assert_eq!(vor.id, 300_001);
    // Spieler und Planzeit des Halbfinals.
    let hf = &snap.matches[0];
    assert_eq!(hf.team1.len(), 1);
    assert!(hf.team1[0].name.starts_with('V'));
    assert_eq!(
        hf.team1[0].club.as_deref().map(|c| c.starts_with("Verein")),
        Some(true)
    );
    assert_eq!(hf.planned_time.map(|t| t / 10_000), Some(20261206));
    assert!(hf.display_order.is_some());
    assert_eq!(hf.location_id, Some(1));
    assert_eq!(hf.scoring.best_of, 3);
    assert_eq!(hf.scoring.cap_score, 30);

    // Ergebnis im Halbfinale 1 → Sieger steht im Finale oben.
    melden(&mut log, "e1", 1, OBEN, None);
    let snap = abbilden(
        &falten_turnier(&log),
        &paket_fuer(log.clone(), 4, eine_halle()),
    );
    let hf = &snap.matches[0];
    assert_eq!(hf.status, MatchStatus::Finished);
    assert_eq!(hf.winner, Some(1));
    assert_eq!(hf.sets, vec![(21, 10), (21, 10)]);
    assert_eq!(snap.matches[2].entry1_id, hf.entry1_id);
    assert_eq!(snap.matches[2].team1, hf.team1);
}

#[test]
fn walkover_und_feld_belegt() {
    let mut log = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    aufrufen(&mut log, "a1", 1, "2").unwrap();
    // Dasselbe Feld für ein zweites Spiel lehnt der Kern ab — die Halle
    // sieht nur das erste.
    assert!(aufrufen(&mut log, "a2", 2, "2").is_err());
    melden(
        &mut log,
        "wo",
        2,
        &[],
        Some(Spielende::Kampflos {
            sieger: Seite::Unten,
        }),
    );
    let snap = abbilden(
        &falten_turnier(&log),
        &paket_fuer(log.clone(), 4, eine_halle()),
    );
    let s1 = &snap.matches[0];
    assert_eq!(s1.status, MatchStatus::OnCourt);
    assert_eq!(s1.court_id, Some(2));
    assert_eq!(s1.court.as_deref(), Some("2"));
    let s2 = &snap.matches[1];
    assert_eq!(s2.status, MatchStatus::Finished);
    assert_eq!(s2.result, MatchResult::Walkover);
    assert_eq!(s2.winner, Some(2));
    assert!(s2.sets.is_empty());
    assert_eq!(s2.court_id, None);
    // Felder samt Identität.
    assert_eq!(snap.courts, vec!["1", "2"]);
    assert_eq!(snap.court_infos[1].id, 2);
    assert!(!snap.is_multi_hall());
}

#[test]
fn gruppe_mit_eigenen_rundentexten_und_aufgabe() {
    let mut log = turnier(3, System::Gruppe { rueckspiel: false }, &[("H1", 2)]);
    let st = falten_turnier(&log);
    let spiele = &st.durchfuehrung["HE"].spiele;
    assert_eq!(spiele.len(), 3);
    let mut p = paket_fuer(log.clone(), 3, eine_halle());
    p.runden.insert(
        "HE".into(),
        [("1".to_string(), "Gruppe A, Runde 1".to_string())].into(),
    );
    let snap = abbilden(&st, &p);
    assert_eq!(snap.matches[0].round_name, "Gruppe A, Runde 1");
    // Ohne Text aus badhub: eigener Rückfall.
    assert_eq!(snap.matches[1].round_name, "Gruppe A");
    // Gruppenspiele haben keine Baumkante.
    assert!(snap
        .matches
        .iter()
        .all(|m| m.from1.is_none() && m.from2.is_none()));
    // Aufgabe → Retired.
    melden(
        &mut log,
        "aufgabe1",
        1,
        &[(21, 15), (5, 3)],
        Some(Spielende::Aufgabe {
            aufgebend: Seite::Unten,
        }),
    );
    let snap = abbilden(&falten_turnier(&log), &paket_fuer(log, 3, eine_halle()));
    assert_eq!(snap.matches[0].result, MatchResult::Retired);
    assert_eq!(snap.matches[0].winner, Some(1));
    // Meldeliste und Schiedsrichter.
    assert_eq!(snap.entries.len(), 3);
    assert_eq!(snap.entries[0].event_id, 3);
    assert_eq!(snap.events[0].name, "Herreneinzel A");
    assert_eq!(snap.officials[0].display_name(), "Sabine Pfiff");
}

/// Zählweise je Spiel (Kern 0.14.0): Turnier 3×15, KO-Phase der Konkurrenz 3×21, Finale 5×11 —
/// jedes Spiel bekommt sein eigenes Zählformat (Tablet: Satzziel, Deckel, Pause).
#[test]
fn zaehlweise_je_spiel_turnier_phase_spiel() {
    let mut log = turnier(
        6,
        System::GruppenKo {
            gruppengroesse: 3,
            aufsteiger: 2,
            platz3: false,
        },
        &[("H1", 2)],
    );
    let regeln = |z: Zaehlweise, ko: Option<Zaehlweise>| Regeln {
        zaehlweise: z,
        gruppenregeln: Gruppenregeln::untereinander(),
        zaehlweise_ko: ko,
    };
    befehl(
        &mut log,
        TurnierBefehl::TurnierRegelnFestlegen {
            id: "zt".into(),
            regeln: Some(regeln(Zaehlweise::badminton_15(), None)),
        },
    )
    .unwrap();
    let st = falten_turnier(&log);
    let ko: Vec<u32> = st.durchfuehrung["HE"]
        .spiele
        .iter()
        .filter(|s| s.phase == Phase::Ko)
        .map(|s| s.nr)
        .collect();
    let finale = *ko.last().unwrap();
    befehl(
        &mut log,
        TurnierBefehl::RegelnFestlegen {
            id: "zk".into(),
            konkurrenz: "HE".into(),
            regeln: Some(regeln(
                Zaehlweise::badminton_15(),
                Some(Zaehlweise::badminton()),
            )),
        },
    )
    .unwrap();
    befehl(
        &mut log,
        TurnierBefehl::ZaehlweiseFestlegen {
            id: "zs".into(),
            konkurrenz: "HE".into(),
            spiel: finale,
            zaehlweise: Some(Zaehlweise::badminton_11()),
        },
    )
    .unwrap();
    let st = falten_turnier(&log);
    let snap = abbilden(&st, &paket_fuer(log.clone(), 6, eine_halle()));
    let fmt = |nr: u32| {
        let m = snap
            .matches
            .iter()
            .find(|m| m.id % super::abbildung::ID_FAKTOR == i64::from(nr))
            .unwrap();
        (
            m.scoring.best_of,
            m.scoring.target_score,
            m.scoring.cap_score,
            m.scoring.interval_at,
        )
    };
    let gruppe = st.durchfuehrung["HE"]
        .spiele
        .iter()
        .find(|s| s.phase == Phase::Gruppe)
        .unwrap()
        .nr;
    assert_eq!(fmt(gruppe), (3, 15, 21, Some(8)), "Gruppe 3×15");
    assert_eq!(fmt(ko[0]), (3, 21, 30, Some(11)), "KO 3×21");
    assert_eq!(
        fmt(finale),
        (5, 11, 15, None),
        "Finale 5×11 ohne Pause im Satz"
    );
}

#[test]
fn mehrere_hallen_mit_kern_feldnamen() {
    let mut log = turnier(4, System::Ko { platz3: false }, &[("H1", 2), ("H2", 2)]);
    // Kern-Feldname wie badhub `Spielansicht::felder`: „<Halle> n“.
    aufrufen(&mut log, "a1", 1, "H2 1").unwrap();
    let hallen = vec![
        PaketHalle {
            id: "H1".into(),
            name: "Halle 1".into(),
            felder: vec!["1".into(), "2".into()],
        },
        PaketHalle {
            id: "H2".into(),
            name: "Halle 2".into(),
            felder: vec!["1".into(), "2".into()],
        },
    ];
    let snap = abbilden(
        &falten_turnier(&log),
        &paket_fuer(log.clone(), 4, hallen.clone()),
    );
    assert!(snap.is_multi_hall());
    assert_eq!(snap.court_infos.len(), 4);
    let s1 = &snap.matches[0];
    assert_eq!(s1.court_id, Some(3), "erstes Feld der zweiten Halle");
    assert_eq!(snap.court_display_label(3), "Halle 2 · 1");
    // Liefert badhub die Felder schon mit Hallenkennung, gilt dasselbe.
    let mut mit_praefix = hallen;
    mit_praefix[1].felder = vec!["H2 1".into(), "H2 2".into()];
    let snap = abbilden(&falten_turnier(&log), &paket_fuer(log, 4, mit_praefix));
    assert_eq!(snap.matches[0].court_id, Some(3));
    assert_eq!(snap.court_infos[2].name, "1");
}

#[test]
fn ohne_hallen_im_paket_gelten_die_felder_des_kerns() {
    let log = turnier(4, System::Ko { platz3: false }, &[("A", 3)]);
    let snap = abbilden(&falten_turnier(&log), &paket_fuer(log, 4, vec![]));
    assert_eq!(snap.courts, vec!["1", "2", "3"]);
}

#[test]
fn kern_version_passt_zum_cargo_tag() {
    let cargo = include_str!("../../../Cargo.toml");
    let zeile = cargo
        .lines()
        .find(|l| l.starts_with("turnierkern"))
        .expect("turnierkern in Cargo.toml");
    assert!(
        zeile.contains(&format!("tag = \"v{KERN_VERSION}\"")),
        "KERN_VERSION muss zum Tag passen: {zeile}"
    );
}

#[test]
fn api_basis_folgt_der_push_adresse() {
    assert_eq!(
        api_basis("https://test.badhub.de/api/live_update.php").unwrap(),
        "https://test.badhub.de/api/torg/v1"
    );
    assert_eq!(
        api_basis("https://badhub.de/api/live_update.php").unwrap(),
        "https://badhub.de/api/torg/v1"
    );
    assert!(api_basis("http://badhub.de/api/live_update.php").is_err());
    assert!(api_basis("http://127.0.0.1:9/x").is_ok());
}

#[test]
fn uuid_pruefung() {
    assert!(uuid_gueltig(UUID));
    assert!(!uuid_gueltig("../../etc"));
    assert!(!uuid_gueltig("abc"));
}

// ── Attrappe der badhub-API ────────────────────────────────────────────────

#[derive(Default)]
struct Attrappe {
    paket: StdMutex<Option<Paket>>,
    nachzuegler: StdMutex<Vec<TurnierEreignis>>,
    paket_abrufe: AtomicUsize,
    nicht_geaendert: AtomicUsize,
    letztes_ab: StdMutex<Option<String>>,
    widerrufen: std::sync::atomic::AtomicBool,
}

const TOKEN: &str = "tok-123";

async fn attrappe_starten(a: Arc<Attrappe>) -> String {
    use axum::extract::{Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum::{Json, Router};

    fn berechtigt(a: &Attrappe, h: &HeaderMap) -> bool {
        !a.widerrufen.load(Ordering::SeqCst)
            && h.get("authorization").and_then(|v| v.to_str().ok())
                == Some(&format!("Bearer {TOKEN}"))
    }

    async fn koppeln(Json(body): Json<serde_json::Value>) -> impl IntoResponse {
        if body["code"] == "ABCD2345" && body["install_id"] == "inst-1" {
            (
                StatusCode::OK,
                Json(serde_json::json!({"token": TOKEN, "turnier": {"id": 17, "uuid": UUID, "name": "Wintercup"}})),
            )
                .into_response()
        } else {
            StatusCode::UNPROCESSABLE_ENTITY.into_response()
        }
    }
    async fn paket(State(a): State<Arc<Attrappe>>, h: HeaderMap) -> axum::response::Response {
        if !berechtigt(&a, &h) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        a.paket_abrufe.fetch_add(1, Ordering::SeqCst);
        if h.get("if-none-match").and_then(|v| v.to_str().ok()) == Some("\"s1\"") {
            a.nicht_geaendert.fetch_add(1, Ordering::SeqCst);
            return StatusCode::NOT_MODIFIED.into_response();
        }
        let p = a.paket.lock().unwrap().clone().unwrap();
        ([("ETag", "\"s1\"")], Json(p)).into_response()
    }
    async fn ereignisse(
        State(a): State<Arc<Attrappe>>,
        h: HeaderMap,
        Query(q): Query<BTreeMap<String, String>>,
    ) -> axum::response::Response {
        if !berechtigt(&a, &h) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let ab: u64 = q.get("ab").and_then(|v| v.parse().ok()).unwrap_or(0);
        *a.letztes_ab.lock().unwrap() = q.get("ab").cloned();
        let neu: Vec<TurnierEreignis> = a
            .nachzuegler
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.n() > ab)
            .cloned()
            .collect();
        Json(serde_json::json!({ "ereignisse": neu })).into_response()
    }

    let app = Router::new()
        .route("/api/torg/v1/koppeln", post(koppeln))
        .route("/api/torg/v1/turnier/paket", get(paket))
        .route("/api/torg/v1/turnier/ereignisse", get(ereignisse))
        .with_state(a);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}/api/torg/v1")
}

/// Turnier mit einem Ergebnis, das erst später kommt (Nachzügler).
fn mit_nachzuegler() -> (Vec<TurnierEreignis>, Vec<TurnierEreignis>) {
    let basis = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    let mut voll = basis.clone();
    melden(&mut voll, "e1", 1, OBEN, None);
    let nachzuegler = voll[basis.len()..].to_vec();
    (basis, nachzuegler)
}

#[tokio::test]
async fn kopplung_paket_etag_ereignisse_und_widerruf() {
    let (basis_log, nachzuegler) = mit_nachzuegler();
    let a = Arc::new(Attrappe::default());
    *a.paket.lock().unwrap() = Some(paket_fuer(basis_log, 4, eine_halle()));
    let basis = attrappe_starten(a.clone()).await;
    let daten = tempfile::tempdir().unwrap();

    // Falscher Code → klare Ablehnung, nichts abgelegt.
    let e = koppeln(&basis, daten.path(), "FALSCH99", "inst-1", "Halle 1 PC")
        .await
        .unwrap_err();
    assert!(e.contains("Kopplungscode"), "{e}");
    assert!(!daten.path().join("turniere").exists());

    // Richtiger Code (mit Bindestrich wie in badhub angezeigt).
    let k = koppeln(&basis, daten.path(), "ABCD-2345", "inst-1", "Halle 1 PC")
        .await
        .unwrap();
    assert_eq!(k.turnier.uuid, UUID);
    assert_eq!(k.turnier.name, "Wintercup");
    assert_eq!(k.turnier.basis, basis);
    assert_eq!(k.spiele, 3);
    assert!(k.kern_hinweis.is_none());

    // Laden wie beim Start und Stand lesen.
    let mut q = KernQuelle::laden(daten.path(), &k.turnier).unwrap();
    assert_eq!(q.stand().matches.len(), 3);
    assert_eq!(q.stand().matches[0].status, MatchStatus::Scheduled);

    // Paket mit If-None-Match → 304.
    let (b, t, n, etag) = q.zugang().unwrap();
    assert_eq!(etag.as_deref(), Some("\"s1\""));
    assert_eq!(
        api::paket(&b, &t, etag.as_deref()).await.unwrap(),
        api::PaketAntwort::Unveraendert
    );
    assert_eq!(a.nicht_geaendert.load(Ordering::SeqCst), 1);

    // Ein neues Ergebnis in badhub → Ereignisse ab n.
    *a.nachzuegler.lock().unwrap() = nachzuegler;
    let neu = api::ereignisse(&b, &t, n).await.unwrap();
    assert_eq!(
        a.letztes_ab.lock().unwrap().as_deref(),
        Some(n.to_string().as_str())
    );
    assert_eq!(q.ereignisse_uebernehmen(neu.clone()), neu.len());
    assert_eq!(
        q.ereignisse_uebernehmen(neu),
        0,
        "doppelt zugestellt ändert nichts"
    );
    assert_eq!(q.stand().matches[0].status, MatchStatus::Finished);

    // Neustart: der nachgeholte Stand liegt auf der Platte.
    let mut q2 = KernQuelle::laden(daten.path(), &k.turnier).unwrap();
    assert_eq!(q2.stand().matches[0].status, MatchStatus::Finished);

    // Widerruf in badhub → 401 → Zugang abgelehnt.
    a.widerrufen.store(true, Ordering::SeqCst);
    assert_eq!(
        api::ereignisse(&b, &t, 0).await.unwrap_err(),
        api::ApiFehler::Zugang
    );
    assert_eq!(
        api::paket(&b, &t, None).await.unwrap_err(),
        api::ApiFehler::Zugang
    );
}

#[tokio::test]
async fn offline_bleibt_der_letzte_stand() {
    let (basis_log, _) = mit_nachzuegler();
    let a = Arc::new(Attrappe::default());
    *a.paket.lock().unwrap() = Some(paket_fuer(basis_log, 4, eine_halle()));
    let basis = attrappe_starten(a).await;
    let daten = tempfile::tempdir().unwrap();
    let mut k = koppeln(&basis, daten.path(), "ABCD2345", "inst-1", "")
        .await
        .unwrap();
    assert_eq!(k.turnier.geraet_name, "bts-light");
    // Netz weg: ein Port, an dem niemand lauscht.
    let tot = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tot.local_addr().unwrap().port();
    drop(tot);
    k.turnier.basis = format!("http://127.0.0.1:{port}/api/torg/v1");
    let mut q = KernQuelle::laden(daten.path(), &k.turnier).unwrap();
    let vorher = q.stand();
    let (b, t, n, _) = q.zugang().unwrap();
    assert!(matches!(
        api::ereignisse(&b, &t, n).await,
        Err(api::ApiFehler::Netz(_))
    ));
    assert_eq!(q.stand(), vorher);
}

#[test]
fn fremdes_turnier_im_paket_wird_verworfen() {
    let daten = tempfile::tempdir().unwrap();
    let log = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    let p = paket_fuer(log, 4, eine_halle());
    let ablage = Ablage::neu(daten.path(), UUID).unwrap();
    ablage.paket_speichern(&p, None).unwrap();
    let cfg = BadhubTurnierConfig {
        uuid: UUID.into(),
        id: 17,
        name: "Wintercup".into(),
        basis: "https://badhub.de/api/torg/v1".into(),
        geraet_name: "PC".into(),
    };
    let mut q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    let mut fremd = p.clone();
    fremd.turnier.uuid = "aaaaaaaa-1111-4222-8333-444455556666".into();
    assert!(q.paket_uebernehmen(fremd, None).is_err());
    let mut doppelt = p.clone();
    doppelt.konkurrenzen.push(doppelt.konkurrenzen[0].clone());
    assert!(q.paket_uebernehmen(doppelt, None).is_err());
    let mut alt = p;
    alt.schema = 2;
    assert!(q.paket_uebernehmen(alt, None).is_err());
}

#[test]
fn laden_verlangt_https_basis() {
    let daten = tempfile::tempdir().unwrap();
    let cfg = BadhubTurnierConfig {
        uuid: UUID.into(),
        basis: "http://badhub.de/api/torg/v1".into(),
        ..BadhubTurnierConfig::default()
    };
    assert!(KernQuelle::laden(daten.path(), &cfg).is_err());
}
