//! Tests der Quelle „Punktspiel“: Abbildung, Meldungen und der Weg über eine
//! Attrappe der Geräte-API (axum auf 127.0.0.1, wie bei den badhub-Turnieren).

use super::*;
use std::sync::Arc;

fn person(id: i64, vor: &str, nach: &str, lizenz: Option<&str>) -> PaketPerson {
    PaketPerson {
        id,
        name: format!("{vor} {nach}").trim().to_string(),
        vorname: vor.into(),
        nachname: nach.into(),
        lizenz: lizenz.map(str::to_string),
    }
}

fn spiel(nr: i64, disziplin: &str, art: &str) -> PaketSpiel {
    PaketSpiel {
        nr,
        disziplin: disziplin.into(),
        art: art.into(),
        heim: Vec::new(),
        gast: Vec::new(),
        feld: None,
        status: "offen".into(),
        saetze: Vec::new(),
        sieger: None,
        ausgang: "normal".into(),
    }
}

const UUID: &str = "0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f";

fn paket() -> Paket {
    let mut hd = spiel(1, "1.HD", "double");
    hd.heim = vec![
        person(11, "Hans", "Heim", Some("01-000011")),
        person(12, "Hugo", "Haus", None),
    ];
    hd.gast = vec![person(21, "Gerd", "Gast", None)];
    let mut he = spiel(2, "1.HE", "single");
    he.heim = vec![person(11, "Hans", "Heim", Some("01-000011"))];
    he.gast = vec![person(21, "Gerd", "Gast", None)];
    Paket {
        schema: SCHEMA,
        begegnung: PaketBegegnung {
            uuid: UUID.into(),
            name: "TV Heim 1 – SV Gast 1".into(),
            heim: "TV Heim 1".into(),
            gast: "SV Gast 1".into(),
            staffel: "Landesliga".into(),
            heim_verein_id: Some(17),
            gast_verein_id: None,
        },
        felder: 2,
        zaehlweise: "3x21".into(),
        spiele: vec![hd, he, spiel(3, "OD", "double"), spiel(4, "DE", "single")],
    }
}

fn zuweisung(court_id: i64, match_id: Option<i64>) -> proto::CourtAssignment {
    proto::CourtAssignment { court_id, match_id }
}

#[test]
fn paket_aus_badhub_json() {
    let json = r#"{"schema":1,"begegnung":{"uuid":"0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f","name":"A – B","heim":"A","gast":"B",
        "staffel":"LL","verband":"bvbb","datum":"2026-11-29","zeit":"10:00","halle":null,"status":"laeuft"},
        "felder":3,"zaehlweise":"3x21",
        "spiele":[{"nr":1,"disziplin":"1.HD","art":"double","heim":[{"id":12,"name":"V N","vorname":"V","nachname":"N","lizenz":"01-1"}],
          "gast":[],"feld":2,"status":"laeuft","saetze":[[21,15],[3,4]],"sieger":null,"ausgang":"normal"}],
        "etag":"abc"}"#;
    let p = paket_lesen(json.as_bytes()).expect("Paket lesbar");
    assert_eq!(p.felder, 3);
    assert_eq!(p.spiele[0].saetze, vec![(21, 15), (3, 4)]);
    assert_eq!(p.spiele[0].heim[0].lizenz.as_deref(), Some("01-1"));
    // unbekannte Felder (verband, datum …) stören nicht
    assert_eq!(p.begegnung.staffel, "LL");
}

#[test]
fn fremdes_schema_und_unsinnige_felderzahl_werden_abgelehnt() {
    let mut p = paket();
    p.schema = 2;
    assert!(p.pruefen().unwrap_err().contains("aktualisieren"));
    let mut p = paket();
    p.felder = 0;
    assert!(p.pruefen().is_err());
}

#[test]
fn abbildung_felder_und_spiele() {
    let s = abbilden(&paket());
    assert_eq!(s.tournament_name, "TV Heim 1 – SV Gast 1");
    assert_eq!(s.courts, vec!["1", "2"]);
    assert_eq!(
        s.court_infos.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(s.locations.is_empty() && !s.is_multi_hall());
    assert_eq!(s.matches.len(), 4);
    let hd = &s.matches[0];
    assert_eq!(
        (hd.id, hd.draw_name.as_str(), hd.class_label.as_str()),
        (1, "1.HD", "1.HD")
    );
    assert_eq!(hd.discipline, Discipline::MensDoubles);
    assert_eq!(hd.round_name, "TV Heim 1 – SV Gast 1");
    assert_eq!(hd.status, MatchStatus::Scheduled);
    assert_eq!(hd.court, None);
    assert_eq!(hd.scoring, ScoringFormat::default());
    assert_eq!(s.matches[2].discipline, Discipline::Unknown, "OD ist offen");
    assert_eq!(s.matches[3].discipline, Discipline::WomensSingles);
}

#[test]
fn abbildung_spieler_verein_lizenz_und_offene_plaetze() {
    let s = abbilden(&paket());
    let hd = &s.matches[0];
    assert_eq!(hd.team1.len(), 2);
    assert_eq!(hd.team1[0].name, "Hans Heim");
    assert_eq!(
        (hd.team1[0].first.as_str(), hd.team1[0].last.as_str()),
        ("Hans", "Heim")
    );
    assert_eq!(hd.team1[0].member_id.as_deref(), Some("01-000011"));
    assert_eq!(hd.team1[0].club.as_deref(), Some("TV Heim 1"));
    // der zweite Gast fehlt in der Aufstellung: die Seite bleibt leer, das Spiel ist nicht spielbereit
    assert!(hd.team2.is_empty());
    // Einzel: genau ein Platz je Seite
    assert_eq!((s.matches[1].team1.len(), s.matches[1].team2.len()), (1, 1));
    // Meldeliste nur von vollständigen Seiten
    assert_eq!(s.entries.iter().filter(|e| e.event_id == 1).count(), 1);
    assert_eq!(s.entries.iter().filter(|e| e.event_id == 3).count(), 0);
    // zwei Spiele ohne Aufstellung teilen keinen Platzhalter-Spieler
    assert!(s.matches[2].team1.is_empty() && s.matches[3].team1.is_empty());
    assert!(s
        .matches
        .iter()
        .all(|m| m.team1.iter().chain(&m.team2).all(|p| p.id != 0)));
}

#[test]
fn abbildung_laufendes_und_beendetes_spiel() {
    let mut p = paket();
    p.spiele[0].status = "laeuft".into();
    p.spiele[0].feld = Some(2);
    p.spiele[0].saetze = vec![(11, 7)];
    p.spiele[1].status = "beendet".into();
    p.spiele[1].feld = Some(1);
    p.spiele[1].saetze = vec![(21, 10), (21, 12)];
    p.spiele[1].sieger = Some("gast".into());
    p.spiele[1].ausgang = "aufgabe".into();
    // läuft, aber auf einem Feld, das es nicht gibt: nicht auf einem Feld zeigen
    p.spiele[2].status = "laeuft".into();
    p.spiele[2].feld = Some(9);
    let s = abbilden(&p);
    assert_eq!(s.matches[0].status, MatchStatus::OnCourt);
    assert_eq!(
        (s.matches[0].court.as_deref(), s.matches[0].court_id),
        (Some("2"), Some(2))
    );
    assert_eq!(s.matches[0].sets, vec![(11, 7)]);
    assert_eq!(s.matches[0].winner, None);
    assert_eq!(s.matches[1].status, MatchStatus::Finished);
    assert_eq!(s.matches[1].winner, Some(2));
    assert_eq!(s.matches[1].result, MatchResult::Retired);
    assert_eq!(
        s.matches[1].court, None,
        "ein beendetes Spiel belegt kein Feld"
    );
    assert_eq!(s.matches[2].status, MatchStatus::Scheduled);
    assert_eq!(s.matches[2].court_id, None);
}

#[test]
fn zaehlweisen() {
    assert_eq!(zaehlformat("3x21"), ScoringFormat::default());
    assert_eq!(zaehlformat("5x11").target_score, 11);
    assert_eq!(zaehlformat("5x11").best_of, 5);
    assert_eq!(zaehlformat("3x15").cap_score, 21);
    assert_eq!(zaehlformat("unsinn"), ScoringFormat::default());
    assert_eq!(zaehlformat(""), ScoringFormat::default());
}

#[test]
fn feldvergabe_aufruf_freigabe_und_wechsel() {
    let mut p = paket();
    // Aufruf
    let m = feld_meldungen(&p, &[zuweisung(1, Some(2))]);
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].nr, m[0].status, m[0].feld), (2, "laeuft", Some(1)));
    // schon dort: nichts zu melden
    p.spiele[1].status = "laeuft".into();
    p.spiele[1].feld = Some(1);
    assert!(feld_meldungen(&p, &[zuweisung(1, Some(2))]).is_empty());
    // Freigabe: wieder offen
    let m = feld_meldungen(&p, &[zuweisung(1, None)]);
    assert_eq!((m[0].nr, m[0].status, m[0].feld), (2, "offen", None));
    // Feldwechsel 1 → 2 in einem Zug: nur der Aufruf, kein „offen“ dazwischen
    let m = feld_meldungen(&p, &[zuweisung(1, None), zuweisung(2, Some(2))]);
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].status, m[0].feld), ("laeuft", Some(2)));
    // Freigabe eines Felds ohne laufendes Spiel und Aufruf eines beendeten oder unbekannten Spiels: nichts
    p.spiele[1].status = "beendet".into();
    assert!(feld_meldungen(
        &p,
        &[
            zuweisung(1, None),
            zuweisung(2, Some(2)),
            zuweisung(2, Some(99))
        ]
    )
    .is_empty());
}

fn update(id: i64, team1_won: bool, score_status: i64) -> proto::MatchUpdate {
    proto::MatchUpdate {
        btp_match_id: id,
        draw_id: id,
        planning_id: id,
        sets: vec![(21, 15), (19, 21), (21, 18)],
        team1_won,
        duration_mins: 30,
        netto_mins: None,
        score_status,
        free_court_id: Some(1),
        player_ids: vec![],
        end_ts_ms: None,
        officials: None,
        shuttles: 0,
    }
}

#[test]
fn ergebnis_als_meldung() {
    let m = ergebnis_meldung(&update(2, true, 0));
    assert_eq!(
        (m.nr, m.status, m.feld, m.sieger, m.ausgang),
        (2, "beendet", None, Some("heim"), "normal")
    );
    assert_eq!(m.saetze, vec![[21, 15], [19, 21], [21, 18]]);
    assert_eq!(ergebnis_meldung(&update(2, false, 2)).sieger, Some("gast"));
    assert_eq!(ergebnis_meldung(&update(2, false, 2)).ausgang, "aufgabe");
    assert_eq!(ergebnis_meldung(&update(2, true, 1)).ausgang, "kampflos");
    assert_eq!(
        ergebnis_meldung(&update(2, true, 3)).ausgang,
        "disqualifikation"
    );
    assert_eq!(ergebnis_meldung(&update(2, true, 9)).ausgang, "normal");
    let json = serde_json::to_value(ergebnis_meldung(&update(2, true, 0))).unwrap();
    assert_eq!(json["saetze"][1], serde_json::json!([19, 21]));
    assert_eq!(json["feld"], serde_json::Value::Null);
}

#[test]
fn punktestand_nur_bei_abweichung_und_nur_fuer_laufende_spiele() {
    let mut p = paket();
    p.spiele[0].status = "laeuft".into();
    p.spiele[0].feld = Some(1);
    p.spiele[0].saetze = vec![(3, 2)];
    p.spiele[1].status = "laeuft".into();
    p.spiele[1].feld = Some(2);
    // Feld 1: neuer Stand; Feld 2: Tablet zeigt ein anderes Spiel (Feld gerade neu belegt)
    let m = punktestand_meldungen(&p, |feld| match feld {
        1 => Some((1, vec![(21, 15), (4, 4)])),
        2 => Some((9, vec![(1, 0)])),
        _ => None,
    });
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].nr, m[0].status, m[0].feld), (1, "laeuft", Some(1)));
    assert_eq!(m[0].saetze, vec![[21, 15], [4, 4]]);
    // gleicher Stand, noch kein Punkt, kein Tablet: nichts zu melden
    assert!(punktestand_meldungen(&p, |f| (f == 1).then(|| (1, vec![(3, 2)]))).is_empty());
    assert!(punktestand_meldungen(&p, |f| (f == 1).then(|| (1, Vec::new()))).is_empty());
    assert!(punktestand_meldungen(&p, |_| None).is_empty());
    // beendetes Spiel: der Punktestand überschreibt kein Ergebnis
    p.spiele[0].status = "beendet".into();
    assert!(punktestand_meldungen(&p, |_| Some((1, vec![(9, 9)]))).is_empty());
}

#[test]
fn api_basis_folgt_der_push_adresse() {
    assert_eq!(
        api_basis("https://test.badhub.de/api/live_update.php").unwrap(),
        "https://test.badhub.de/api/ls/v1"
    );
    assert!(
        api_basis("http://badhub.de/api/live_update.php").is_err(),
        "Klartext nur lokal"
    );
    assert_eq!(
        api_basis("http://127.0.0.1:8080/x").unwrap(),
        "http://127.0.0.1:8080/api/ls/v1"
    );
}

// ── Attrappe der Geräte-API ─────────────────────────────────────────────────

const TOKEN: &str = "ls_0000000000000000000000000000000000000000000000000000000000000000";

struct Attrappe {
    paket: Mutex<serde_json::Value>,
    meldungen: Mutex<Vec<serde_json::Value>>,
    /// 0 = normal, sonst dieser Status für alle Aufrufe mit Token
    status: Mutex<u16>,
}

fn paket_json() -> serde_json::Value {
    serde_json::json!({
        "schema": 1,
        "begegnung": {"uuid": UUID, "name": "TV Heim 1 – SV Gast 1", "heim": "TV Heim 1", "gast": "SV Gast 1", "staffel": "LL"},
        "felder": 2, "zaehlweise": "3x21",
        "spiele": [
            {"nr": 1, "disziplin": "1.HD", "art": "double", "heim": [], "gast": [], "feld": null, "status": "offen", "saetze": [], "sieger": null, "ausgang": "normal"},
            {"nr": 2, "disziplin": "1.HE", "art": "single", "heim": [{"id": 11, "name": "Hans Heim", "vorname": "Hans", "nachname": "Heim", "lizenz": null}],
             "gast": [{"id": 21, "name": "Gerd Gast", "vorname": "Gerd", "nachname": "Gast", "lizenz": null}],
             "feld": null, "status": "offen", "saetze": [], "sieger": null, "ausgang": "normal"}
        ]
    })
}

async fn attrappe_starten(a: Arc<Attrappe>) -> String {
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    fn gesperrt(a: &Attrappe, h: &HeaderMap) -> Option<StatusCode> {
        let ok = h.get("authorization").and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {TOKEN}"));
        if !ok {
            return Some(StatusCode::UNAUTHORIZED);
        }
        let s = *a.status.lock().unwrap();
        (s != 0).then(|| StatusCode::from_u16(s).unwrap())
    }
    fn etag(p: &serde_json::Value) -> String {
        format!("\"{}\"", p.to_string().len())
    }
    async fn koppeln(Json(body): Json<serde_json::Value>) -> axum::response::Response {
        // Passwortschutz vor badhub (Testsystem): 401 mit Basic-Aufforderung
        if body["code"] == "WANDWAND" {
            return (
                StatusCode::UNAUTHORIZED,
                [("WWW-Authenticate", "Basic realm=\"test\"")],
            )
                .into_response();
        }
        if body["code"] == "ABCD2345" && body["install_id"] == "inst-1" {
            Json(serde_json::json!({"token": TOKEN, "begegnung": {"uuid": UUID.to_uppercase(), "name": "TV Heim 1 – SV Gast 1"}})).into_response()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
    async fn begegnung(State(a): State<Arc<Attrappe>>, h: HeaderMap) -> axum::response::Response {
        if let Some(s) = gesperrt(&a, &h) {
            return s.into_response();
        }
        let p = a.paket.lock().unwrap().clone();
        if h.get("if-none-match").and_then(|v| v.to_str().ok()) == Some(etag(&p).as_str()) {
            return StatusCode::NOT_MODIFIED.into_response();
        }
        ([("ETag", etag(&p))], Json(p)).into_response()
    }
    async fn spiel(
        State(a): State<Arc<Attrappe>>,
        h: HeaderMap,
        Json(m): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if let Some(s) = gesperrt(&a, &h) {
            return s.into_response();
        }
        if m["nr"] == 99 {
            return (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({"fehler": "meldung_ungueltig", "text": "Dieses Spiel gibt es in der Begegnung nicht."}))).into_response();
        }
        a.meldungen.lock().unwrap().push(m.clone());
        let mut p = a.paket.lock().unwrap();
        for s in p["spiele"].as_array_mut().unwrap() {
            if s["nr"] == m["nr"] {
                for k in ["status", "feld", "saetze", "sieger", "ausgang"] {
                    s[k] = m[k].clone();
                }
            }
        }
        ([("ETag", etag(&p))], Json(p.clone())).into_response()
    }
    let app = Router::new()
        .route("/api/ls/v1/koppeln", post(koppeln))
        .route("/api/ls/v1/begegnung", get(begegnung))
        .route("/api/ls/v1/begegnung/spiel", post(spiel))
        .with_state(a);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{port}/api/ls/v1")
}

/// Der ganze Weg in EINEM Test: der Zustand ist prozessweit, zwei Tests
/// nebeneinander träten sich auf die Füße.
#[tokio::test]
async fn koppeln_lesen_melden_und_fehler() {
    let a = Arc::new(Attrappe {
        paket: Mutex::new(paket_json()),
        meldungen: Mutex::new(Vec::new()),
        status: Mutex::new(0),
    });
    let basis = attrappe_starten(a.clone()).await;
    let daten = tempfile::tempdir().unwrap();

    // Koppeln
    let fehler = koppeln(&basis, daten.path(), "FALSCH99", "inst-1", "Halle")
        .await
        .unwrap_err();
    assert!(fehler.contains("Kopplungscode"), "{fehler}");
    assert!(koppeln(&basis, daten.path(), "  ", "inst-1", "Halle")
        .await
        .is_err());
    let wand = koppeln(&basis, daten.path(), "WANDWAND", "inst-1", "Halle")
        .await
        .unwrap_err();
    assert!(
        wand.contains("Passwortschutz") && !wand.contains("falsch"),
        "{wand}"
    );
    let k = koppeln(
        &basis,
        daten.path(),
        "abcd-2345".to_uppercase().as_str(),
        "inst-1",
        " Halle 1 ",
    )
    .await
    .expect("gekoppelt");
    assert_eq!(k.punktspiel.uuid, UUID, "Kennung klein geschrieben");
    assert_eq!((k.spiele, k.felder, k.offen), (2, 2, 1));
    assert_eq!(k.punktspiel.geraet_name, "Halle 1");
    assert!(k.punktspiel.gekoppelt());
    // Token liegt verschlüsselt auf der Platte, nie im Klartext
    let datei = daten
        .path()
        .join("punktspiele")
        .join(UUID)
        .join("geraet.bin");
    let roh = std::fs::read(&datei).expect("Token-Datei");
    assert!(!String::from_utf8_lossy(&roh).contains("ls_0000"));
    assert_eq!(token_laden(daten.path(), UUID).as_deref(), Some(TOKEN));
    assert!(
        token_datei(daten.path(), "../../etc").is_err(),
        "Kennung benennt nie einen fremden Pfad"
    );

    // Starten und lesen
    let mut config = AppConfig::default();
    assert!(!starten(&config, daten.path()), "nichts gekoppelt");
    assert!(stand().await.is_err());
    config.punktspiel = k.punktspiel.clone();
    assert!(starten(&config, daten.path()));
    assert!(schreiben_moeglich());
    assert_eq!(
        turnier_schluessel().as_deref(),
        Some(format!("punktspiel:{UUID}").as_str())
    );
    let s = stand().await.expect("Stand");
    assert_eq!(s.matches.len(), 2);
    assert_eq!(s.courts.len(), 2);
    assert!(
        stand().await.is_ok(),
        "zweiter Abruf (304) liefert denselben Stand"
    );

    // Feldvergabe und Ergebnis
    feld_vergabe(&[zuweisung(2, Some(2))])
        .await
        .expect("Feld vergeben");
    let s = stand().await.unwrap();
    assert_eq!(s.matches[1].status, MatchStatus::OnCourt);
    assert_eq!(s.matches[1].court_id, Some(2));
    ergebnis(&update(2, true, 0))
        .await
        .expect("Ergebnis gemeldet");
    let s = stand().await.unwrap();
    assert_eq!(s.matches[1].status, MatchStatus::Finished);
    assert_eq!(s.matches[1].winner, Some(1));
    assert_eq!(s.matches[1].sets.len(), 3);
    {
        let m = a.meldungen.lock().unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0],
            serde_json::json!({"nr": 2, "status": "laeuft", "feld": 2, "saetze": [], "sieger": null, "ausgang": "normal"})
        );
        assert_eq!(m[1]["sieger"], "heim");
    }
    // Ein Punktestand, der nach dem Ergebnis an die Reihe kommt, wird aus dem
    // dann aktuellen Paket berechnet: das Spiel ist beendet, es geht nichts hinaus
    // (sonst höbe „läuft“ das Ergebnis wieder auf).
    melden_aus(|p| punktestand_meldungen(p, |_| Some((2, vec![(9, 9)]))))
        .await
        .expect("nichts zu senden");
    assert_eq!(
        a.meldungen.lock().unwrap().len(),
        2,
        "kein Punktestand nach dem Ergebnis"
    );
    assert_eq!(
        stand().await.unwrap().matches[1].status,
        MatchStatus::Finished
    );
    // badhub lehnt ab: der Text von badhub kommt bis in die Halle
    let fehler = ergebnis(&update(99, true, 0)).await.unwrap_err();
    assert!(fehler.contains("Dieses Spiel gibt es"), "{fehler}");

    // Netz weg / Serverfehler: der letzte Stand gilt weiter, schreiben scheitert
    *a.status.lock().unwrap() = 500;
    assert_eq!(stand().await.expect("letzter Stand").matches.len(), 2);
    assert!(feld_vergabe(&[zuweisung(1, Some(1))]).await.is_err());
    // Zugang entzogen: das ist immer ein Fehler, auch mit altem Stand
    *a.status.lock().unwrap() = 401;
    assert!(stand().await.unwrap_err().contains("neu koppeln"));
    *a.status.lock().unwrap() = 403;
    assert!(stand().await.unwrap_err().contains("nuLiga"));

    // Verspäteter Abruf: ging vor einer Meldung los und kommt danach an — sein älterer Stand zählt nicht
    *a.status.lock().unwrap() = 0;
    let (_, _, _, von) = zugang().unwrap();
    let alt = paket_lesen(paket_json().to_string().as_bytes()).unwrap();
    feld_vergabe(&[zuweisung(1, Some(1))])
        .await
        .expect("Meldung dazwischen");
    uebernehmen(alt.clone(), Some("\"alt\"".into()), Some(von));
    assert_eq!(
        paket_kopie().unwrap().spiele[0].status,
        "laeuft",
        "der neuere Stand aus der Meldung bleibt"
    );
    // ein Paket einer anderen Begegnung wird nie übernommen
    let mut fremd = alt;
    fremd.begegnung.uuid = "ffffffff-ffff-4fff-8fff-ffffffffffff".into();
    fremd.begegnung.name = "Fremd".into();
    uebernehmen(fremd, None, None);
    assert_eq!(
        paket_kopie().unwrap().begegnung.name,
        "TV Heim 1 – SV Gast 1"
    );

    // Kopplung mit Klartext-Adresse in der Konfiguration (von Hand geändert): nicht laden
    let mut unsicher = config.clone();
    unsicher.punktspiel.basis = "http://badhub.de/api/ls/v1".into();
    assert!(!starten(&unsicher, daten.path()));
    assert!(starten(&config, daten.path()));

    // Neustart ohne Token-Datei: nicht geladen
    std::fs::remove_file(&datei).unwrap();
    assert!(!starten(&config, daten.path()));
    assert!(!schreiben_moeglich());
}

#[test]
fn verlauf_meldung_traegt_die_eintraege_unter_matches() {
    // Form, die badhub erwartet: {"matches": [...]} — ohne Typ, ohne Turnierkennung.
    let json = serde_json::to_value(VerlaufMeldung { matches: &[] }).unwrap();
    assert_eq!(json, serde_json::json!({ "matches": [] }));
    let antwort: VerlaufAntwort =
        serde_json::from_str(r#"{"ok":true,"gespeichert":1,"verworfen":2}"#).unwrap();
    assert_eq!(antwort.verworfen, 2);
    const { assert!(VERLAUF_MAX_JE_MELDUNG <= crate::badhub::verlauf::MAX_SPIELE_JE_NACHRICHT) };
}

// ── Monitoransicht „Begegnung“ (Spec punktspiel-monitoransicht) ─────────────

#[test]
fn sicht_zeigt_alle_spiele_in_spielreihenfolge_mit_teilaufstellung() {
    let mut p = paket();
    // Durcheinander geliefert: Die Sicht ordnet nicht um, das Paket kommt
    // von badhub in Spielreihenfolge. Hier zählt, dass nichts verloren geht.
    p.spiele[1].status = "beendet".into();
    p.spiele[1].saetze = vec![(21, 15), (21, 12)];
    p.spiele[1].sieger = Some("heim".into());
    p.spiele[1].ausgang = "kampflos".into();
    let s = sicht_aus(&p);
    assert_eq!(
        (s.heim.as_str(), s.gast.as_str()),
        ("TV Heim 1", "SV Gast 1")
    );
    assert_eq!(s.staffel, "Landesliga");
    assert_eq!((s.heim_verein_id, s.gast_verein_id), (Some(17), None));
    assert_eq!((s.best_of, s.ziel, s.kappe), (3, 21, 30));
    assert_eq!(
        s.spiele.iter().map(|z| z.nr).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    // Doppel mit nur einem Gast: der eine Name bleibt sichtbar (anders als
    // im Snapshot, wo die Seite leer wird).
    let hd = &s.spiele[0];
    assert!(hd.doppel);
    assert_eq!(hd.heim, vec!["Hans Heim", "Hugo Haus"]);
    assert_eq!(hd.gast, vec!["Gerd Gast"]);
    assert_eq!(hd.status, relay_proto::BegegnungStatus::Offen);
    let he = &s.spiele[1];
    assert!(!he.doppel);
    assert_eq!(he.status, relay_proto::BegegnungStatus::Beendet);
    assert_eq!(he.sieger, Some(relay_proto::BegegnungSeite::Heim));
    assert_eq!(he.ausgang, "kampflos");
    // Ganz ohne Aufstellung: leere Seiten, die Seite schreibt „noch nicht
    // aufgestellt“.
    assert!(s.spiele[2].heim.is_empty() && s.spiele[2].gast.is_empty());
}

#[test]
fn sicht_status_und_sieger_wie_im_snapshot() {
    let mut p = paket();
    // „läuft“ auf einem Feld, das es nicht gibt → offen (wie `abbilden`).
    p.spiele[0].status = "laeuft".into();
    p.spiele[0].feld = Some(7);
    p.spiele[0].saetze = vec![(5, 3)];
    // „läuft“ auf Feld 2 mit Sieger-Angabe → Sieger erst mit „beendet“.
    p.spiele[1].status = "laeuft".into();
    p.spiele[1].feld = Some(2);
    p.spiele[1].saetze = vec![(11, 9)];
    p.spiele[1].sieger = Some("gast".into());
    p.spiele[1].ausgang = "aufgabe".into();
    let s = sicht_aus(&p);
    assert_eq!(s.spiele[0].status, relay_proto::BegegnungStatus::Offen);
    assert!(s.spiele[0].saetze.is_empty() && s.spiele[0].feld.is_none());
    assert_eq!(s.spiele[1].status, relay_proto::BegegnungStatus::Laeuft);
    assert_eq!(s.spiele[1].feld, Some(2));
    assert_eq!(s.spiele[1].saetze, vec![relay_proto::SetAb { a: 11, b: 9 }]);
    assert_eq!(s.spiele[1].sieger, None);
    assert_eq!(s.spiele[1].ausgang, "");
}

/// Datenschutz-Wächter: Lizenznummer und Personen-ID aus dem Paket gelangen
/// nicht in die Sicht — sie geht an den Relay und steht auf einem Hallen-TV.
#[test]
fn sicht_traegt_weder_lizenz_noch_personen_id() {
    let mut p = paket();
    p.spiele[0].heim[0].id = 987_654_321;
    let text = serde_json::to_string(&sicht_aus(&p)).unwrap();
    assert!(text.contains("Hans Heim"));
    for verboten in [
        "01-000011",
        "987654321",
        "lizenz",
        "\"id\"",
        "vorname",
        "nachname",
    ] {
        assert!(!text.contains(verboten), "{verboten} in der Sicht");
    }
    // Anonymisierte Personen reisen unverändert durch.
    p.spiele[1].heim = vec![person(5, "", "***", None)];
    p.spiele[1].heim[0].name = "***".into();
    assert_eq!(sicht_aus(&p).spiele[1].heim, vec!["***"]);
}

#[test]
fn sicht_deckelt_wie_der_relay() {
    let mut p = paket();
    p.spiele = (1..=30).map(|nr| spiel(nr, "1.HE", "single")).collect();
    p.begegnung.heim = "x".repeat(500);
    let s = sicht_aus(&p);
    assert_eq!(s.spiele.len(), relay_proto::MAX_BEGEGNUNG_SPIELE);
    assert_eq!(s.heim.chars().count(), 80);
    assert!(s.passt());
}

#[test]
fn paket_mit_und_ohne_vereins_ids() {
    let ohne = r#"{"schema":1,"begegnung":{"uuid":"0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f","name":"A – B","heim":"A","gast":"B"},
        "felder":2,"zaehlweise":"3x21","spiele":[]}"#;
    let p = paket_lesen(ohne.as_bytes()).expect("altes Paket bleibt lesbar");
    assert_eq!(
        (p.begegnung.heim_verein_id, p.begegnung.gast_verein_id),
        (None, None)
    );
    let mit = r#"{"schema":1,"begegnung":{"uuid":"0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f","name":"A – B","heim":"A","gast":"B",
        "heim_verein_id":17,"gast_verein_id":null},"felder":2,"zaehlweise":"3x21","spiele":[]}"#;
    let p = paket_lesen(mit.as_bytes()).expect("Paket mit IDs lesbar");
    assert_eq!(
        (p.begegnung.heim_verein_id, p.begegnung.gast_verein_id),
        (Some(17), None)
    );
    assert_eq!(sicht_aus(&p).heim_verein_id, Some(17));
}

/// Eine Vereins-ID in unerwarteter Form kostet nur das Logo, nie das Paket.
#[test]
fn unlesbare_vereins_id_legt_das_paket_nicht_lahm() {
    for roh in [
        r#""12""#,
        "-1",
        "0",
        "99999999999",
        "12.5",
        "true",
        "[1]",
        "{}",
    ] {
        let json = format!(
            r#"{{"schema":1,"begegnung":{{"uuid":"0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f","name":"A – B","heim":"A","gast":"B",
            "heim_verein_id":{roh},"gast_verein_id":23}},"felder":2,"zaehlweise":"3x21","spiele":[]}}"#
        );
        let p = paket_lesen(json.as_bytes()).unwrap_or_else(|e| panic!("{roh}: {e:?}"));
        assert_eq!(p.begegnung.heim_verein_id, None, "{roh}");
        assert_eq!(p.begegnung.gast_verein_id, Some(23), "{roh}");
    }
}
