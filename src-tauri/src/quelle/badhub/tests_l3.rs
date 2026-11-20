//! Tests Paket L3: Schreibweichen → Kernbefehle, lokales Protokoll, Lease,
//! Outbox und Kernbefehle aus badhub — gegen eine badhub-Attrappe, die die
//! Antworten von badhub B1 nachbildet (`TorgGeraetController`,
//! `Lease`, `LeaseHochladen`).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use turnierkern::*;

use super::api::{self, ApiFehler};
use super::lease::LeaseZustand;
use super::paket::*;
use super::schreiben;
use super::tests::{aufrufen, eine_halle, melden, paket_fuer, turnier, OBEN, UUID};
use super::*;
use crate::btp::proto::{CourtAssignment, MatchCourt, MatchUpdate};

const TOKEN: &str = "tok-l3";
const JETZT: &str = "2026-12-06T11:00:00";

// ── Übersetzung der Weichen ────────────────────────────────────────────────

fn ko4() -> (Vec<TurnierEreignis>, Paket) {
    let log = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    let p = paket_fuer(log.clone(), 4, eine_halle());
    (log, p)
}

fn update(id: i64, sets: Vec<(i64, i64)>, team1_won: bool, status: i64) -> MatchUpdate {
    MatchUpdate {
        btp_match_id: id,
        draw_id: 3,
        planning_id: id % 100_000,
        sets,
        team1_won,
        duration_mins: 0,
        score_status: status,
        free_court_id: None,
        player_ids: vec![],
        end_ts_ms: None,
        officials: None,
        shuttles: 0,
    }
}

#[test]
fn feld_zuweisen_umhaengen_und_raeumen_werden_aufruf_und_ruecknahme() {
    let (mut log, p) = ko4();
    let st = falten_turnier(&log);
    // Zuweisen: Spiel 300001 auf Feld 2 → spiel_aufrufen mit Feldname „2“.
    let b = schreiben::aufrufe(
        &st,
        &p,
        &[CourtAssignment {
            court_id: 2,
            match_id: Some(300_001),
        }],
        JETZT,
    )
    .unwrap();
    assert_eq!(
        b,
        vec![TurnierBefehl::SpielAufrufen {
            id: String::new(),
            konkurrenz: "HE".into(),
            spiel: 1,
            feld: Some("2".into()),
            jetzt: JETZT.into(),
        }]
    );
    aufrufen(&mut log, "a1", 1, "2").unwrap();
    let st = falten_turnier(&log);
    // Derselbe Aufruf noch einmal: kein Befehl.
    let b = schreiben::aufrufe(
        &st,
        &p,
        &[CourtAssignment {
            court_id: 2,
            match_id: Some(300_001),
        }],
        JETZT,
    )
    .unwrap();
    assert!(b.is_empty());
    // Umhängen 2 → 1: nur ein neuer Aufruf, keine Rücknahme.
    let b = schreiben::aufrufe(
        &st,
        &p,
        &[
            CourtAssignment {
                court_id: 2,
                match_id: None,
            },
            CourtAssignment {
                court_id: 1,
                match_id: Some(300_001),
            },
        ],
        JETZT,
    )
    .unwrap();
    assert_eq!(b.len(), 1);
    assert!(matches!(&b[0], TurnierBefehl::SpielAufrufen { feld: Some(f), .. } if f == "1"));
    // Räumen ohne Ergebnis → aufruf_zuruecknehmen.
    let b = schreiben::aufrufe(
        &st,
        &p,
        &[CourtAssignment {
            court_id: 2,
            match_id: None,
        }],
        JETZT,
    )
    .unwrap();
    assert_eq!(
        b,
        vec![TurnierBefehl::AufrufZuruecknehmen {
            id: String::new(),
            konkurrenz: "HE".into(),
            spiel: 1
        }]
    );
    // Unbekanntes Spiel / Feld → Fehler.
    assert!(schreiben::aufrufe(
        &st,
        &p,
        &[CourtAssignment {
            court_id: 2,
            match_id: Some(900_001)
        }],
        JETZT
    )
    .is_err());
    assert!(schreiben::aufrufe(
        &st,
        &p,
        &[CourtAssignment {
            court_id: 9,
            match_id: Some(300_002)
        }],
        JETZT
    )
    .is_err());
}

#[test]
fn ergebnis_walkover_aufgabe_und_disqualifikation() {
    let (log, p) = ko4();
    let st = falten_turnier(&log);
    let e = |u: MatchUpdate| schreiben::ergebnis(&st, &p, &u, JETZT).unwrap().unwrap();
    // Regulär mit Zeiten aus bts-light (Ende + gemessene Dauer).
    let mut u = update(300_001, vec![(21, 15), (21, 19)], true, 0);
    u.end_ts_ms = Some(1_765_015_800_000);
    u.duration_mins = 30;
    let TurnierBefehl::ErgebnisMelden {
        saetze,
        ende,
        beginn,
        bis,
        jetzt,
        ..
    } = e(u)
    else {
        panic!()
    };
    assert_eq!(saetze, vec![(21, 15), (21, 19)]);
    assert_eq!(ende, None);
    assert_eq!(jetzt, JETZT);
    let (b, z) = (beginn.unwrap(), bis.unwrap());
    assert_eq!((b.len(), z.len()), (19, 19));
    assert!(b < z, "{b} < {z}");
    // Kampflos: keine Sätze, Sieger.
    let TurnierBefehl::ErgebnisMelden { saetze, ende, .. } =
        e(update(300_001, vec![(3, 1)], false, 1))
    else {
        panic!()
    };
    assert!(saetze.is_empty());
    assert_eq!(
        ende,
        Some(Spielende::Kampflos {
            sieger: Seite::Unten
        })
    );
    // Aufgabe: Sätze bleiben, aufgebend = Verlierer.
    let TurnierBefehl::ErgebnisMelden { saetze, ende, .. } =
        e(update(300_001, vec![(21, 15), (5, 3)], true, 2))
    else {
        panic!()
    };
    assert_eq!(saetze.len(), 2);
    assert_eq!(
        ende,
        Some(Spielende::Aufgabe {
            aufgebend: Seite::Unten
        })
    );
    // Disqualifikation: mit Sätzen als Aufgabe, ohne als kampflos.
    let TurnierBefehl::ErgebnisMelden { ende, .. } = e(update(300_001, vec![(4, 2)], false, 3))
    else {
        panic!()
    };
    assert_eq!(
        ende,
        Some(Spielende::Aufgabe {
            aufgebend: Seite::Oben
        })
    );
    let TurnierBefehl::ErgebnisMelden { ende, .. } = e(update(300_001, vec![], false, 3)) else {
        panic!()
    };
    assert_eq!(
        ende,
        Some(Spielende::Kampflos {
            sieger: Seite::Unten
        })
    );
}

#[test]
fn dasselbe_ergebnis_zweimal_erzeugt_keinen_zweiten_befehl() {
    let (mut log, p) = ko4();
    melden(&mut log, "e1", 1, OBEN, None);
    let st = falten_turnier(&log);
    let u = update(300_001, vec![(21, 10), (21, 10)], true, 0);
    assert_eq!(schreiben::ergebnis(&st, &p, &u, JETZT).unwrap(), None);
    // Ein anderes Ergebnis ist eine Korrektur.
    let u = update(300_001, vec![(21, 10), (21, 12)], true, 0);
    assert!(schreiben::ergebnis(&st, &p, &u, JETZT).unwrap().is_some());
}

#[test]
fn schiedsrichter_mit_person_aus_dem_paket() {
    let (log, mut p) = ko4();
    p.schiedsrichter.push(PaketSchiedsrichter {
        id: 7,
        name: "AR".into(),
        person: Some(104),
    });
    let st = falten_turnier(&log);
    let mc = |o: Option<(i64, i64)>| MatchCourt {
        match_id: 300_001,
        draw_id: 3,
        planning_id: 1,
        court_id: 0,
        officials: o,
        highlight: None,
        shuttles: 0,
    };
    let r = schreiben::offizielle(&st, &p, &[mc(Some((4, 7))), mc(None)]);
    assert_eq!(r.len(), 1, "ohne officials kein Befehl");
    let Ok(Some(TurnierBefehl::OffizielleSetzen {
        schiedsrichter,
        aufschlagrichter,
        spiel,
        ..
    })) = &r[0].1
    else {
        panic!("{r:?}")
    };
    assert_eq!(*spiel, 1);
    assert_eq!(
        *schiedsrichter,
        Some(Offizieller {
            id: 4,
            person: None
        })
    );
    assert_eq!(
        *aufschlagrichter,
        Some(Offizieller {
            id: 7,
            person: Some(104)
        })
    );
    // 0 = kein Dienst; unverändert leer → kein Befehl.
    let r = schreiben::offizielle(&st, &p, &[mc(Some((0, 0)))]);
    assert_eq!(r[0].1, Ok(None));
}

#[test]
fn befehls_ids_passen_zu_badhub() {
    assert!(schreiben::id_gueltig("bts-0a1b2c3d-18c5f-12"));
    assert!(!schreiben::id_gueltig("bts_1"));
    assert!(!schreiben::id_gueltig(""));
    assert!(!schreiben::id_gueltig(&"a".repeat(65)));
}

// ── Attrappe von badhub B1 ────────────────────────────────────────────────

#[derive(Default)]
struct Badhub {
    ereignisse: StdMutex<Vec<TurnierEreignis>>,
    ids: StdMutex<BTreeSet<String>>,
    /// Gerät hält die Lease, mit Epoche.
    lease: StdMutex<Option<u64>>,
    epoche: StdMutex<u64>,
    relay: StdMutex<Option<serde_json::Value>>,
    /// Nächste n Hochladen-Aufrufe antworten 503.
    fehler_503: AtomicUsize,
    /// Nächster Aufruf antwortet `luecke` (Antwort ging verloren: badhub
    /// hat schon übernommen).
    antwort_verloren: AtomicBool,
    abweichung: AtomicBool,
    hochladen_aufrufe: AtomicUsize,
    paket: StdMutex<Option<Paket>>,
    letzter_body: StdMutex<Option<serde_json::Value>>,
}

impl Badhub {
    fn n(&self) -> u64 {
        self.ereignisse
            .lock()
            .unwrap()
            .iter()
            .map(TurnierEreignis::n)
            .max()
            .unwrap_or(0)
    }
    fn zurueckholen(&self) {
        *self.lease.lock().unwrap() = None;
        *self.epoche.lock().unwrap() += 1;
    }
}

async fn badhub_starten(b: Arc<Badhub>) -> String {
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};

    fn ok(h: &HeaderMap) -> bool {
        h.get("authorization").and_then(|v| v.to_str().ok()) == Some(&format!("Bearer {TOKEN}"))
    }
    async fn lease_nehmen(
        State(b): State<Arc<Badhub>>,
        h: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if !ok(&h) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let n = body["n"].as_u64().unwrap();
        if n != b.n() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"fehler": "n_veraltet", "n": b.n()})),
            )
                .into_response();
        }
        let mut lease = b.lease.lock().unwrap();
        let epoche = match *lease {
            Some(e) => e, // selbes Gerät: Epoche bleibt
            None => {
                let mut e = b.epoche.lock().unwrap();
                *e += 1;
                *e
            }
        };
        *lease = Some(epoche);
        *b.relay.lock().unwrap() = body.get("relay").cloned();
        Json(serde_json::json!({"epoche": epoche, "n": n})).into_response()
    }
    async fn lease_zurueck(
        State(b): State<Arc<Badhub>>,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        let mut lease = b.lease.lock().unwrap();
        if *lease != body["epoche"].as_u64() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"fehler": "epoche_veraltet"})),
            )
                .into_response();
        }
        if body["n"].as_u64() != Some(b.n()) {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"fehler": "n_offen", "n": b.n()})),
            )
                .into_response();
        }
        *lease = None;
        Json(serde_json::json!({"epoche": body["epoche"], "n": b.n()})).into_response()
    }
    async fn hochladen(
        State(b): State<Arc<Badhub>>,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        b.hochladen_aufrufe.fetch_add(1, Ordering::SeqCst);
        *b.letzter_body.lock().unwrap() = Some(body.clone());
        if b.fehler_503.load(Ordering::SeqCst) > 0 {
            b.fehler_503.fetch_sub(1, Ordering::SeqCst);
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if *b.lease.lock().unwrap() != body["epoche"].as_u64() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"fehler": "epoche_veraltet", "epoche": *b.epoche.lock().unwrap()})),
            )
                .into_response();
        }
        if b.abweichung.load(Ordering::SeqCst) {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(
                    serde_json::json!({"fehler": "abweichung", "posten": 0, "grund": "abgelehnt"}),
                ),
            )
                .into_response();
        }
        let posten = body["posten"].as_array().unwrap().clone();
        assert!(posten.len() <= 200, "höchstens 200 Posten je Aufruf");
        // Bekannte führende Befehle überspringen, Lücke prüfen.
        let mut basis = body["basis_n"].as_u64().unwrap();
        let mut neu = Vec::new();
        for p in &posten {
            let id = p["befehl"]["id"].as_str().unwrap().to_string();
            assert!(schreiben::id_gueltig(&id), "{id}");
            let es: Vec<TurnierEreignis> = serde_json::from_value(p["ereignisse"].clone()).unwrap();
            if b.ids.lock().unwrap().contains(&id) && neu.is_empty() {
                basis += es.len() as u64;
                continue;
            }
            neu.push((id, es));
        }
        if basis != b.n() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({"fehler": "luecke", "erwartet_n": b.n()})),
            )
                .into_response();
        }
        for (id, es) in neu {
            b.ids.lock().unwrap().insert(id);
            b.ereignisse.lock().unwrap().extend(es);
        }
        if b.antwort_verloren.swap(false, Ordering::SeqCst) {
            return StatusCode::GATEWAY_TIMEOUT.into_response();
        }
        Json(serde_json::json!({"bis_n": b.n()})).into_response()
    }
    async fn paket(State(b): State<Arc<Badhub>>) -> axum::response::Response {
        let mut p = b.paket.lock().unwrap().clone().unwrap();
        p.ereignisse = b.ereignisse.lock().unwrap().clone();
        Json(p).into_response()
    }
    let app = Router::new()
        .route("/api/torg/v1/turnier/paket", axum::routing::get(paket))
        .route(
            "/api/torg/v1/turnier/lease",
            post(lease_nehmen).delete(lease_zurueck),
        )
        .route("/api/torg/v1/turnier/ereignisse", post(hochladen))
        .with_state(b);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}/api/torg/v1")
}

/// Gekoppeltes Turnier auf der Platte (wie nach `koppeln`) samt
/// badhub-Attrappe mit demselben Stand.
async fn gekoppelt() -> (
    tempfile::TempDir,
    Arc<Badhub>,
    BadhubTurnierConfig,
    StdMutex<Option<KernQuelle>>,
) {
    let (log, p) = ko4();
    let b = Arc::new(Badhub::default());
    *b.ereignisse.lock().unwrap() = log;
    *b.paket.lock().unwrap() = Some(p.clone());
    let basis = badhub_starten(b.clone()).await;
    let daten = tempfile::tempdir().unwrap();
    let a = Ablage::neu(daten.path(), UUID).unwrap();
    a.paket_speichern(&p, Some("\"s1\"")).unwrap();
    a.token_speichern(TOKEN).unwrap();
    let cfg = BadhubTurnierConfig {
        uuid: UUID.into(),
        id: 17,
        name: "Wintercup".into(),
        basis,
        geraet_name: "Halle 1 PC".into(),
    };
    let q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    (daten, b, cfg, StdMutex::new(Some(q)))
}

/// Ein Lease-Schritt wie im Abgleich (Netz außerhalb der Sperre).
async fn lease_schritt(z: &StdMutex<Option<KernQuelle>>, relay_ns: Option<&str>) {
    let (basis, token, bedarf) = {
        let mut g = z.lock().unwrap();
        let q = g.as_mut().unwrap();
        (
            q.basis.clone(),
            q.token.clone().unwrap(),
            q.lease_bedarf(relay_ns),
        )
    };
    if let Some((n, relay)) = bedarf {
        let antwort = api::lease_nehmen(&basis, &token, n, relay.as_ref()).await;
        z.lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .lease_antwort(antwort, relay);
    }
}

fn mit<T>(z: &StdMutex<Option<KernQuelle>>, f: impl FnOnce(&mut KernQuelle) -> T) -> T {
    f(z.lock().unwrap().as_mut().unwrap())
}

fn aufruf(spiel: u32, feld: &str) -> TurnierBefehl {
    TurnierBefehl::SpielAufrufen {
        id: String::new(),
        konkurrenz: "HE".into(),
        spiel,
        feld: Some(feld.into()),
        jetzt: JETZT.into(),
    }
}

#[tokio::test]
async fn ohne_lease_keine_eingabe_mit_lease_lokal_gueltig_und_nach_netzrueckkehr_in_badhub() {
    let (daten, b, cfg, z) = gekoppelt().await;
    // Ohne Lease: gesperrt.
    let e = mit(&z, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap_err();
    assert!(matches!(e, SchreibFehler::Gesperrt(_)), "{e:?}");

    // Lease nehmen, mit Relay-Angaben.
    lease_schritt(&z, Some("ns-halle")).await;
    let relay = b.relay.lock().unwrap().clone().unwrap();
    assert_eq!(relay["ns"], "ns-halle");
    let tl_token = relay["tl_token"].as_str().unwrap().to_string();
    assert_eq!(tl_token.len(), 64);
    mit(&z, |q| {
        assert_eq!(q.lease.as_ref().unwrap().epoche, 1);
        assert_eq!(q.tl_zugang.as_deref(), Some(tl_token.as_str()));
    });

    // Eingabe offline: lokal gültig (Kern-Stand + Protokoll), Outbox wartet.
    let n0 = b.n();
    mit(&z, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    mit(&z, |q| {
        assert_eq!(
            q.stand().matches[0].status,
            crate::btp::model::MatchStatus::OnCourt
        );
        assert_eq!(q.wartende_ereignisse(), 1);
        assert_eq!(q.abgleich().zustand, "wartet");
    });
    // Ergebnis vom Tablet (Weiche) — ohne Netz.
    mit(&z, |q| {
        let st = q.kern_stand();
        let b = schreiben::ergebnis(
            &st,
            &q.paket,
            &update(300_001, vec![(21, 10), (21, 10)], true, 0),
            JETZT,
        )
        .unwrap()
        .unwrap();
        q.ausfuehren(vec![b], AKTEUR_HALLE).unwrap();
    });
    // Absturz: neu laden — Protokoll, Lease und Zugang sind da.
    let z2 = StdMutex::new(Some(KernQuelle::laden(daten.path(), &cfg).unwrap()));
    mit(&z2, |q| {
        assert_eq!(q.wartende_ereignisse(), 2);
        assert_eq!(
            q.stand().matches[0].status,
            crate::btp::model::MatchStatus::Finished
        );
        assert_eq!(q.tl_zugang.as_deref(), Some(tl_token.as_str()));
        // Nach dem Neustart keine neue Anmeldung, solange die Outbox nicht leer ist.
        assert!(q.lease_bedarf(Some("ns-halle")).is_none());
    });
    // Netz zurück: Outbox lädt hoch, badhub hat beide Ereignisse.
    assert_eq!(outbox_schritt(&z2).await, OutboxSchritt::Weiter);
    assert_eq!(b.n(), n0 + 2);
    assert_eq!(outbox_schritt(&z2).await, OutboxSchritt::Ruhe);
    mit(&z2, |q| {
        assert_eq!(q.wartende_ereignisse(), 0);
        assert_eq!(q.abgleich().text, "badhub: aktuell");
        assert_eq!(
            q.paket.n(),
            n0 + 2,
            "bestätigte Ereignisse gehören zum Paket"
        );
    });
    // Und der Stand nach einem weiteren Neustart stimmt.
    let mut q3 = KernQuelle::laden(daten.path(), &cfg).unwrap();
    assert_eq!(q3.n_bestaetigt(), n0 + 2);
    assert_eq!(
        q3.stand().matches[0].status,
        crate::btp::model::MatchStatus::Finished
    );
}

#[tokio::test]
async fn feld_belegt_kommt_als_ablehnung_des_kerns_an_und_nichts_wird_geschrieben() {
    let (_d, _b, _c, z) = gekoppelt().await;
    lease_schritt(&z, None).await;
    mit(&z, |q| q.ausfuehren(vec![aufruf(1, "2")], AKTEUR_HALLE)).unwrap();
    let vorher = mit(&z, |q| q.protokoll.posten().len());
    let e = mit(&z, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap_err();
    let SchreibFehler::Kern { text, grund } = e else {
        panic!("{e:?}")
    };
    assert_eq!(grund["grund"], "feld_belegt");
    assert!(text.contains("Feld 2"), "{text}");
    assert_eq!(mit(&z, |q| q.protokoll.posten().len()), vorher);
    // Alle oder keiner: ein gültiger Befehl vor dem abgelehnten zählt nicht.
    let e = mit(&z, |q| {
        q.ausfuehren(
            vec![
                TurnierBefehl::AufrufZuruecknehmen {
                    id: String::new(),
                    konkurrenz: "HE".into(),
                    spiel: 1,
                },
                aufruf(2, "9999"),
                aufruf(2, &"x".repeat(200)),
            ],
            AKTEUR_HALLE,
        )
    });
    assert!(e.is_err());
    assert_eq!(mit(&z, |q| q.protokoll.posten().len()), vorher);
}

#[tokio::test]
async fn kernbefehl_aus_badhub_idempotent_jetzt_vom_knoten_und_nur_spielbefehle() {
    let (_d, _b, _c, z) = gekoppelt().await;
    lease_schritt(&z, Some("ns")).await;
    let befehl = serde_json::json!({
        "art": "spiel_aufrufen", "id": "bh-77", "konkurrenz": "HE", "spiel": 1, "feld": "1"
    });
    // `jetzt` fehlt: setzt der Knoten.
    assert_eq!(mit(&z, |q| q.befehl_von_badhub(befehl.clone())), Ok(1));
    // Dieselbe Kennung noch einmal (badhub wiederholt): wirkungslos.
    assert_eq!(mit(&z, |q| q.befehl_von_badhub(befehl.clone())), Ok(0));
    mit(&z, |q| {
        let p = q.protokoll.posten().last().unwrap();
        assert_eq!(p.akteur, AKTEUR_BADHUB);
        assert_eq!(p.befehl.id(), "bh-77");
    });
    // Turnierweite Befehle sind unter der Lease gesperrt.
    let e = mit(&z, |q| {
        q.befehl_von_badhub(serde_json::json!({"art": "zeitplan_rechnen", "id": "bh-78"}))
    });
    assert!(matches!(e, Err(SchreibFehler::Ungueltig(_))), "{e:?}");
    // Kennung, die badhub nicht annähme.
    let e = mit(&z, |q| {
        q.befehl_von_badhub(serde_json::json!({"art": "aufruf_zuruecknehmen", "id": "bh_79", "konkurrenz": "HE", "spiel": 1}))
    });
    assert!(matches!(e, Err(SchreibFehler::Ungueltig(_))), "{e:?}");
    // Kern-Ablehnung mit Grund.
    let e = mit(&z, |q| {
        q.befehl_von_badhub(serde_json::json!({"art": "spiel_aufrufen", "id": "bh-80", "konkurrenz": "HE", "spiel": 2, "feld": "1"}))
    });
    assert!(
        matches!(&e, Err(SchreibFehler::Kern { grund, .. }) if grund["grund"] == "feld_belegt"),
        "{e:?}"
    );
}

#[tokio::test]
async fn outbox_luecke_backoff_idempotenz_und_abweichung() {
    let (_d, b, _c, z) = gekoppelt().await;
    lease_schritt(&z, None).await;
    mit(&z, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    // Server-Fehler → später (Backoff 5, 10, 20, 40, 60, 60 …).
    b.fehler_503.store(1, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Spaeter);
    let w: Vec<u64> = (1..=7).map(|i| outbox_wartezeit(i).as_secs()).collect();
    assert_eq!(w, vec![5, 10, 20, 40, 60, 60, 60]);
    // Antwort geht verloren: badhub hat übernommen, bts-light weiß es nicht.
    b.antwort_verloren.store(true, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Spaeter);
    let nach_verlust = b.n();
    // Wiederholung desselben Postens: badhub überspringt die bekannte ID.
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Weiter);
    assert_eq!(b.n(), nach_verlust, "nichts doppelt");
    assert_eq!(mit(&z, |q| q.wartende_ereignisse()), 0);

    // Lücke: bts-light glaubt sich weiter hinten als badhub (Antwort verloren
    // UND inzwischen ein neuer Posten) → badhub nennt erwartet_n.
    mit(&z, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap();
    b.antwort_verloren.store(true, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Spaeter);
    mit(&z, |q| {
        q.ausfuehren(
            vec![TurnierBefehl::AufrufZuruecknehmen {
                id: String::new(),
                konkurrenz: "HE".into(),
                spiel: 2,
            }],
            AKTEUR_HALLE,
        )
    })
    .unwrap();
    // Jetzt mit basis_n vor dem verlorenen Posten: badhub überspringt ihn.
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Weiter);
    assert_eq!(mit(&z, |q| q.wartende_ereignisse()), 0);
    assert_eq!(mit(&z, |q| q.n_lokal()), b.n());
    // Echte Lücke: bts-light meldet einen zu kleinen basis_n.
    mit(&z, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap();
    let server = b.n();
    let r = api::hochladen(
        &mit(&z, |q| q.basis.clone()),
        TOKEN,
        1,
        server - 1,
        &mit(&z, |q| {
            q.protokoll
                .posten()
                .last()
                .map(|p| vec![p.fuer_badhub()])
                .unwrap()
        }),
    )
    .await
    .unwrap();
    assert_eq!(r, api::HochladenAntwort::Luecke { erwartet_n: server });
    assert_eq!(mit(&z, |q| q.outbox_antwort(Ok(r))), OutboxSchritt::Weiter);
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Weiter);
    assert_eq!(mit(&z, |q| q.n_lokal()), b.n());

    // Abweichung (422): Outbox steht, rot.
    mit(&z, |q| {
        q.ausfuehren(
            vec![TurnierBefehl::AufrufZuruecknehmen {
                id: String::new(),
                konkurrenz: "HE".into(),
                spiel: 2,
            }],
            AKTEUR_HALLE,
        )
    })
    .unwrap();
    b.abweichung.store(true, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Angehalten);
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Ruhe);
    let lage = mit(&z, |q| q.abgleich());
    assert_eq!(lage.zustand, "angehalten");
    assert!(lage.rot && !lage.kann_beenden);
}

#[tokio::test]
async fn offline_wartet_die_outbox_und_die_gespeicherte_lease_gilt_weiter() {
    let (daten, _b, mut cfg, z) = gekoppelt().await;
    lease_schritt(&z, None).await;
    mit(&z, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    // Netz weg.
    let tot = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tot.local_addr().unwrap().port();
    drop(tot);
    cfg.basis = format!("http://127.0.0.1:{port}/api/torg/v1");
    let z2 = StdMutex::new(Some(KernQuelle::laden(daten.path(), &cfg).unwrap()));
    assert_eq!(outbox_schritt(&z2).await, OutboxSchritt::Spaeter);
    // Eingaben gehen weiter (Lease gilt ohne Netz).
    mit(&z2, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap();
    assert_eq!(mit(&z2, |q| q.wartende_ereignisse()), 2);
    // Ohne gespeicherte Lease und ohne Netz: keine Eingabe möglich.
    let (_d3, _b3, mut cfg3, z3) = gekoppelt().await;
    cfg3.basis = cfg.basis.clone();
    let _ = cfg3;
    mit(&z3, |q| q.basis = cfg.basis.clone());
    lease_schritt(&z3, None).await;
    assert!(matches!(
        mit(&z3, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)),
        Err(SchreibFehler::Gesperrt(_))
    ));
    assert_eq!(mit(&z3, |q| q.abgleich().zustand), "keine_lease");
}

#[tokio::test]
async fn zurueckgeholt_sperrt_eingaben_und_uebernehmen_verwirft_das_nicht_angekommene() {
    let (_d, b, _c, z) = gekoppelt().await;
    lease_schritt(&z, Some("ns")).await;
    mit(&z, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    b.zurueckholen();
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Angehalten);
    let lage = mit(&z, |q| q.abgleich());
    assert_eq!(lage.zustand, "entzogen");
    assert!(lage.rot && lage.kann_uebernehmen);
    assert_eq!(lage.text, TEXT_ENTZOGEN);
    assert!(matches!(
        mit(&z, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)),
        Err(SchreibFehler::Gesperrt(t)) if t == TEXT_ENTZOGEN
    ));
    // Kein TL-Zugang mehr für badhub.
    assert!(mit(&z, |q| q.tl_zugang.is_none()));
    // Keine automatische Neuanmeldung.
    assert!(mit(&z, |q| q.lease_bedarf(Some("ns"))).is_none());

    // In badhub wird inzwischen weitergespielt (anderes Ergebnis als lokal).
    {
        let mut log = b.ereignisse.lock().unwrap().clone();
        melden(&mut log, "badhub-1", 2, OBEN, None);
        *b.ereignisse.lock().unwrap() = log;
    }
    // Übernehmen: das Nicht-Angekommene wird verworfen, Stand aus badhub.
    let t = uebernehmen_in(&z).await.unwrap();
    assert!(t.contains("1 Eingaben"), "{t}");
    mit(&z, |q| {
        assert_eq!(q.n_lokal(), b.n());
        assert!(q.lease.is_none());
        assert_eq!(
            q.stand().matches[1].status,
            crate::btp::model::MatchStatus::Finished
        );
        assert_eq!(
            q.stand().matches[0].status,
            crate::btp::model::MatchStatus::Scheduled
        );
    });
    lease_schritt(&z, Some("ns")).await;
    mit(&z, |q| {
        assert_eq!(
            q.lease.as_ref().unwrap().epoche,
            3,
            "Rückholung und Neuvergabe erhöhen je um 1 (wie badhub B1)"
        );
        assert!(q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE).is_ok());
    });
}

#[tokio::test]
async fn lease_zurueckgeben_nur_mit_leerer_outbox_danach_ohne_personenauszug() {
    let (daten, b, cfg, z) = gekoppelt().await;
    lease_schritt(&z, None).await;
    mit(&z, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    let e = zurueckgeben_in(&z).await.unwrap_err();
    assert!(e.contains("warten noch 1"), "{e}");
    assert!(b.lease.lock().unwrap().is_some());
    assert_eq!(outbox_schritt(&z).await, OutboxSchritt::Weiter);
    let ok = zurueckgeben_in(&z).await.unwrap();
    assert!(ok.contains("zurückgegeben"), "{ok}");
    assert!(b.lease.lock().unwrap().is_none());
    assert!(!daten
        .path()
        .join("turniere")
        .join(UUID)
        .join("personen.bin")
        .exists());
    mit(&z, |q| {
        assert!(q.paket.meldungen.is_empty());
        assert_eq!(q.abgleich().zustand, "zurueckgegeben");
        assert!(matches!(
            q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE),
            Err(SchreibFehler::Gesperrt(_))
        ));
    });
    // Nach einem Neustart bleibt es zurückgegeben — keine stille Neuanmeldung.
    let mut q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    assert!(q.lease_bedarf(None).is_none());
    assert!(q.stand().matches[0].team1.iter().all(|p| p.name.is_empty()));
}

#[test]
fn personenauszug_nach_frist_und_alte_kopplungen_werden_aufgeraeumt() {
    let daten = tempfile::tempdir().unwrap();
    let (_, mut p) = ko4();
    p.turnier.ende = Some("2026-12-06".into());
    let a = Ablage::neu(daten.path(), UUID).unwrap();
    a.paket_speichern(&p, None).unwrap();
    let alt_uuid = "aaaaaaaa-1111-4222-8333-444455556666";
    let mut alt = p.clone();
    alt.turnier.uuid = alt_uuid.into();
    Ablage::neu(daten.path(), alt_uuid)
        .unwrap()
        .paket_speichern(&alt, None)
        .unwrap();
    let pers = daten
        .path()
        .join("turniere")
        .join(UUID)
        .join("personen.bin");
    // Innerhalb der Frist: aktuelles bleibt vollständig, altes wird gelöscht.
    let tag = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    aufraeumen(daten.path(), Some(UUID), tag("2026-12-20"));
    assert!(pers.exists());
    assert!(!daten.path().join("turniere").join(alt_uuid).exists());
    // Nach 14 Tagen: Personenauszug weg, Paket bleibt.
    aufraeumen(daten.path(), Some(UUID), tag("2026-12-21"));
    assert!(!pers.exists());
    assert!(daten
        .path()
        .join("turniere")
        .join(UUID)
        .join("paket.json")
        .exists());
}

#[test]
fn abgeschnittene_protokollzeile_nach_absturz_kostet_nur_die_letzte_eingabe() {
    let daten = tempfile::tempdir().unwrap();
    let (_, p) = ko4();
    let a = Ablage::neu(daten.path(), UUID).unwrap();
    a.paket_speichern(&p, None).unwrap();
    lease::speichern(
        a.dir(),
        &lease::Lease {
            epoche: 1,
            bis_n: p.n(),
            zustand: LeaseZustand::Aktiv,
            ..Default::default()
        },
    )
    .unwrap();
    let cfg = BadhubTurnierConfig {
        uuid: UUID.into(),
        basis: "https://badhub.de/api/torg/v1".into(),
        ..Default::default()
    };
    let mut q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE).unwrap();
    q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE).unwrap();
    let datei = a.dir().join(protokoll::DATEI);
    let roh = std::fs::read(&datei).unwrap();
    std::fs::write(&datei, &roh[..roh.len() - 20]).unwrap();
    let mut q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    assert_eq!(q.wartende_ereignisse(), 1);
    let snap = q.stand();
    assert_eq!(
        snap.matches[0].status,
        crate::btp::model::MatchStatus::OnCourt
    );
    assert_eq!(
        snap.matches[1].status,
        crate::btp::model::MatchStatus::Scheduled
    );
    // Und es geht nahtlos weiter.
    q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE).unwrap();
    assert_eq!(
        KernQuelle::laden(daten.path(), &cfg)
            .unwrap()
            .wartende_ereignisse(),
        2
    );
}

#[test]
fn kern_version_muss_zum_schreiben_gleich_sein() {
    let daten = tempfile::tempdir().unwrap();
    let (_, mut p) = ko4();
    p.kern_version = "0.11.0".into();
    let a = Ablage::neu(daten.path(), UUID).unwrap();
    a.paket_speichern(&p, None).unwrap();
    lease::speichern(
        a.dir(),
        &lease::Lease {
            epoche: 1,
            bis_n: p.n(),
            ..Default::default()
        },
    )
    .unwrap();
    let cfg = BadhubTurnierConfig {
        uuid: UUID.into(),
        basis: "https://badhub.de/api/torg/v1".into(),
        ..Default::default()
    };
    let mut q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    let e = q
        .ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)
        .unwrap_err();
    assert!(e.to_string().contains("0.11.0"), "{e}");
}

#[test]
fn turnier_schluessel_ist_die_uuid() {
    let daten = tempfile::tempdir().unwrap();
    let (_, p) = ko4();
    Ablage::neu(daten.path(), UUID)
        .unwrap()
        .paket_speichern(&p, None)
        .unwrap();
    let cfg = BadhubTurnierConfig {
        uuid: UUID.into(),
        basis: "https://badhub.de/api/torg/v1".into(),
        ..Default::default()
    };
    let q = KernQuelle::laden(daten.path(), &cfg).unwrap();
    assert_eq!(q.turnier_schluessel(), format!("badhub:{UUID}"));
    let _ = ApiFehler::Zugang;
}
