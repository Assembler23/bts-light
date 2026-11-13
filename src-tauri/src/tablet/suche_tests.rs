//! Tests der Spielersuche (Spec `tl-spielersuche`).

use super::*;
use crate::btp::model::{
    BtpCourt, BtpMatch, BtpOfficial, BtpPlayer, BtpSnapshot, Discipline, MatchResult, MatchStatus,
};

fn player(name: &str) -> BtpPlayer {
    BtpPlayer {
        id: 0,
        name: name.to_string(),
        first: String::new(),
        last: name.to_string(),
        member_id: Some(format!("LZ-{name}")),
        nationality: Some("GER".to_string()),
        club: Some("TSV Beispiel".to_string()),
    }
}

fn spiel(id: i64, t1: &[&str], t2: &[&str]) -> BtpMatch {
    BtpMatch {
        id,
        draw_id: 1,
        planning_id: id,
        display_order: None,
        highlight: 0,
        shuttles: 0,
        from1: None,
        from2: None,
        draw_name: "HE A".to_string(),
        discipline: Discipline::MensSingles,
        class_label: "A".to_string(),
        round_name: "R1".to_string(),
        match_num: Some(id),
        planned_time: None,
        team1: t1.iter().map(|n| player(n)).collect(),
        team2: t2.iter().map(|n| player(n)).collect(),
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
        preparation_call_rank: None,
        official1_id: None,
        official2_id: None,
        scoring: crate::btp::model::ScoringFormat::default(),
        team1_seed: Some("1".to_string()),
        team2_seed: None,
    }
}

fn beendet(mut m: BtpMatch, result: MatchResult) -> BtpMatch {
    m.status = MatchStatus::Finished;
    m.winner = Some(1);
    m.result = result;
    m.sets = vec![(21, 15), (21, 18)];
    m
}

fn am_feld(mut m: BtpMatch, court: i64) -> BtpMatch {
    m.status = MatchStatus::OnCourt;
    m.court_id = Some(court);
    m.court = Some(format!("Feld {court}"));
    m
}

fn snap(courts: Vec<i64>, matches: Vec<BtpMatch>) -> BtpSnapshot {
    BtpSnapshot {
        tournament_name: "Testturnier".to_string(),
        rest_minutes: None,
        courts: Vec::new(),
        locations: Vec::new(),
        court_infos: courts
            .into_iter()
            .map(|id| BtpCourt {
                id,
                name: format!("Feld {id}"),
                location_id: None,
                sort_order: id,
            })
            .collect(),
        matches,
        events: Vec::new(),
        entries: Vec::new(),
        officials: Vec::new(),
    }
}

fn tablet_mit(s: BtpSnapshot) -> TabletState {
    let t = TabletState::default();
    t.set_snapshot(s);
    t
}

fn suchen(t: &TabletState, cfg: &AppConfig, q: &str) -> TlSearch {
    suche(t, cfg, q, true, 1_000_000, TL_SEARCH_HITS_PER_GROUP).expect("gültiger Begriff")
}

fn ids<T>(g: &Gruppe<T>, id: impl Fn(&T) -> i64) -> Vec<i64> {
    g.hits.iter().map(id).collect()
}

// ── Faltung und Matching ──────────────────────────────────────────────

#[test]
fn faltung_macht_umlaute_und_akzente_vergleichbar() {
    assert_eq!(falte("Müller"), "muller");
    assert_eq!(falte("Mueller"), "muller");
    assert_eq!(falte("MULLER"), "muller");
    assert_eq!(falte("Straße"), "strasse");
    assert_eq!(falte("José"), "jose");
    assert_eq!(falte("Łukasz"), "lukasz");
    assert_eq!(falte("Anna-Maria"), "anna maria");
    assert_eq!(falte("  O'Brien ,  Jörg "), "o brien jorg");
    assert_eq!(falte("Renée"), "renee");
}

#[test]
fn ein_begriffsanfang_bleibt_teil_des_gefalteten_namens() {
    // `ue → u` darf Tippen mittendrin nicht brechen.
    for (anfang, name) in [
        ("manu", "Manuel"),
        ("mue", "Müller"),
        ("micha", "Michael"),
        ("joe", "Joerg"),
    ] {
        let w = woerter(anfang).unwrap();
        assert!(passt(name, &w), "„{anfang}“ soll „{name}“ finden");
    }
}

#[test]
fn woerter_pruefen_die_grenzen() {
    assert_eq!(woerter("a"), Err(SuchFehler::Kurz));
    assert_eq!(woerter("  - "), Err(SuchFehler::Kurz));
    assert!(woerter("ab").is_ok());
    assert_eq!(woerter(&"x".repeat(65)), Err(SuchFehler::Lang));
    assert!(woerter(&"x".repeat(64)).is_ok());
    assert_eq!(woerter("a b c d e f g"), Err(SuchFehler::VieleWoerter));
    assert_eq!(woerter("Max  Müller").unwrap(), vec!["max", "muller"]);
}

#[test]
fn alle_woerter_muessen_in_derselben_person_stehen() {
    let w = woerter("mül max").unwrap();
    assert!(passt("Max Müller", &w));
    let w = woerter("Müller Schmidt").unwrap();
    assert!(!passt("Max Müller", &w));
    assert!(!passt("Eva Schmidt", &w));
}

#[test]
fn fehler_reisen_als_code_zum_relay() {
    assert_eq!(
        woerter("ue"),
        Err(SuchFehler::Kurz),
        "„ue“ faltet zu einem Zeichen"
    );
    assert_eq!(SuchFehler::Kurz.relay_json(), r#"{"error":"invalid"}"#);
    assert_eq!(SuchFehler::Belegt.relay_json(), r#"{"error":"busy"}"#);
}

#[test]
fn hoechstens_zwei_suchen_gleichzeitig() {
    let zaehler = std::sync::atomic::AtomicUsize::new(0);
    let a = Platz::nehmen(&zaehler).expect("erster Platz");
    let b = Platz::nehmen(&zaehler).expect("zweiter Platz");
    assert!(
        Platz::nehmen(&zaehler).is_none(),
        "dritte Suche wird abgewiesen"
    );
    drop(a);
    assert!(
        Platz::nehmen(&zaehler).is_some(),
        "freigegebener Platz ist wieder da"
    );
    drop(b);
    assert_eq!(zaehler.load(std::sync::atomic::Ordering::Acquire), 0);
}

// ── Gruppen ───────────────────────────────────────────────────────────

#[test]
fn findet_am_feld_in_der_warteliste_und_beendet_auch_alte_spiele() {
    let mut alt = beendet(
        spiel(1, &["Max Müller"], &["Ben Becker"]),
        MatchResult::Normal,
    );
    alt.finished_at = Some(10);
    // 40 neuere beendete Spiele drängen Spiel 1 aus den letzten 30.
    let mut matches: Vec<BtpMatch> = (100..140)
        .map(|id| {
            let mut m = beendet(
                spiel(id, &["Anna Andere"], &["Bea Berta"]),
                MatchResult::Normal,
            );
            m.finished_at = Some(1_000 + id as u64);
            m
        })
        .collect();
    matches.push(alt);
    matches.push(am_feld(spiel(2, &["Max Müller"], &["Carl Clausen"]), 1));
    matches.push(spiel(3, &["Max Müller"], &["Dora Dahl"]));
    let t = tablet_mit(snap(vec![1], matches));
    let r = suchen(&t, &AppConfig::default(), "mueller");
    assert!(r.has_data);
    assert_eq!(ids(&r.court, |h| h.kern.match_id), vec![2]);
    assert_eq!(
        ids(&r.waiting, |h| h.kern.match_id),
        vec![3],
        "Spiel 2 läuft — nicht zusätzlich in der Warteliste"
    );
    assert_eq!(ids(&r.finished, |h| h.kern.match_id), vec![1]);
    assert_eq!(r.finished.hits[0].position, 41, "Platz im Beendet-Panel");
    assert_eq!(r.court.hits[0].kern.roles, vec!["player"]);
    assert_eq!(r.court.hits[0].court, "Feld 1");
}

#[test]
fn beendete_treffer_stehen_nach_spielnummer_absteigend() {
    let ms = vec![
        beendet(spiel(5, &["Max Müller"], &["X"]), MatchResult::Normal),
        beendet(spiel(9, &["Max Müller"], &["Y"]), MatchResult::Walkover),
        beendet(spiel(7, &["Max Müller"], &["Z"]), MatchResult::Retired),
    ];
    let t = tablet_mit(snap(vec![], ms));
    let r = suchen(&t, &AppConfig::default(), "müller");
    assert_eq!(ids(&r.finished, |h| h.kern.match_id), vec![9, 7, 5]);
    assert_eq!(r.finished.hits[0].result, "walkover");
    assert_eq!(r.finished.hits[1].result, "retired");
}

#[test]
fn deckel_je_gruppe_zaehlt_den_rest() {
    let ms: Vec<BtpMatch> = (1..=120)
        .map(|id| beendet(spiel(id, &["Max Müller"], &["Gegner"]), MatchResult::Normal))
        .collect();
    let t = tablet_mit(snap(vec![], ms));
    let r = suchen(&t, &AppConfig::default(), "müller");
    assert_eq!(r.finished.hits.len(), 50);
    assert_eq!(r.finished.more, 70);
}

#[test]
fn kuerzungsleiter_haelt_die_groessengrenze() {
    let lang = "Maximilian-Alexander von Müller-Lüdenscheidt".to_string();
    let ms: Vec<BtpMatch> = (1..=150)
        .map(|id| {
            let mut m = spiel(id, &[&lang, &lang], &[&lang, &lang]);
            m.discipline = Discipline::Mixed;
            if id > 100 {
                beendet(m, MatchResult::Normal)
            } else {
                m
            }
        })
        .collect();
    let t = tablet_mit(snap(vec![], ms));
    let tr = treffer(&t, &AppConfig::default(), "müller", true, 1).unwrap();
    let grenze = 20_000;
    let json = json_unter_deckel(&tr, false, 1, grenze, std::time::Instant::now());
    assert!(json.len() <= grenze, "{} > {grenze}", json.len());
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let n = v["waiting"]["hits"].as_array().unwrap().len();
    assert!(n < 50 && n > 0);
    assert_eq!(n + v["waiting"]["more"].as_u64().unwrap() as usize, 100);
}

#[test]
fn ohne_turnierdaten_meldet_die_suche_das() {
    let t = TabletState::default();
    let r = suchen(&t, &AppConfig::default(), "müller");
    assert!(!r.has_data);
    assert!(r.court.hits.is_empty() && r.waiting.hits.is_empty() && r.finished.hits.is_empty());
}

// ── Offene Paarungen ──────────────────────────────────────────────────

fn mit_offener_paarung() -> TabletState {
    let mut vorspiel = spiel(8, &["Max Müller"], &["Eva Engel"]);
    vorspiel.planning_id = 808;
    let mut offen = spiel(9, &[], &["Fritz Fuchs"]);
    offen.from1 = Some(808);
    tablet_mit(snap(vec![], vec![vorspiel, offen]))
}

#[test]
fn kandidat_einer_offenen_paarung_ist_markiert_und_ohne_personenfelder() {
    let t = mit_offener_paarung();
    let r = suchen(&t, &AppConfig::default(), "müller");
    let offen = r
        .waiting
        .hits
        .iter()
        .find(|h| h.kern.match_id == 9)
        .expect("Kandidaten-Treffer");
    assert!(offen.open);
    assert_eq!(offen.kern.roles, vec!["candidate"]);
    assert!(offen.kern.team1_ids.is_none() && offen.kern.team2_ids.is_none());
    assert!(offen.kern.team1_club.is_none() && offen.kern.team2_club.is_none());
    assert_eq!(offen.open_slot1_label, "Max Müller oder Eva Engel");
    // Spielbereite zuerst, dann die offene Paarung.
    assert_eq!(ids(&r.waiting, |h| h.kern.match_id), vec![8, 9]);
}

#[test]
fn ohne_kandidaten_erscheint_die_offene_paarung_nicht() {
    let t = mit_offener_paarung();
    let r = suche(&t, &AppConfig::default(), "müller", false, 1, 50).unwrap();
    assert_eq!(ids(&r.waiting, |h| h.kern.match_id), vec![8]);
}

// ── Prognose ──────────────────────────────────────────────────────────

fn mit_prognose() -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.prediction.enabled = true;
    cfg
}

#[test]
fn prognose_wie_im_tl_zustand_und_nur_im_fenster() {
    let mut ms: Vec<BtpMatch> = Vec::new();
    for id in 1..=125 {
        let name = if id == 3 || id == 121 {
            "Max Müller".to_string()
        } else {
            format!("Spieler{id}")
        };
        ms.push(spiel(id, &[&name], &[&format!("Gegner{id}")]));
    }
    let t = tablet_mit(snap(vec![1, 2], ms));
    let cfg = mit_prognose();
    let r = suchen(&t, &cfg, "müller");
    assert_eq!(
        t.predicted_start_ms(3),
        None,
        "die Suche merkt keine Prognose"
    );

    let state = crate::tablet::tl::build_state(&t, &cfg, 1_000_000, 1);
    let erwartet = state
        .queue
        .iter()
        .find(|m| m.match_id == 3)
        .and_then(|m| m.predicted_start_ms);
    assert!(
        erwartet.is_some(),
        "Testvoraussetzung: Platz 3 hat eine Prognose"
    );
    let p3 = r
        .waiting
        .hits
        .iter()
        .find(|h| h.kern.match_id == 3)
        .unwrap();
    assert_eq!(p3.position, 3);
    assert_eq!(p3.predicted_start_ms, erwartet);
    let p121 = r
        .waiting
        .hits
        .iter()
        .find(|h| h.kern.match_id == 121)
        .unwrap();
    assert_eq!(p121.position, 121);
    assert_eq!(p121.predicted_start_ms, None);
}

#[test]
fn ohne_prognose_keine_startzeit() {
    let t = tablet_mit(snap(vec![1], vec![spiel(3, &["Max Müller"], &["X"])]));
    let mut cfg = AppConfig::default();
    cfg.prediction.enabled = false;
    let r = suchen(&t, &cfg, "müller");
    assert_eq!(r.waiting.hits[0].predicted_start_ms, None);
}

// ── Schiedsrichter ────────────────────────────────────────────────────

fn mit_sr(enabled: bool) -> TabletState {
    let mut lauf = am_feld(spiel(10, &["A"], &["B"]), 1);
    lauf.official1_id = Some(1);
    let mut warte = spiel(11, &["C"], &["D"]);
    warte.official2_id = Some(1);
    let mut fertig = beendet(spiel(12, &["E"], &["F"]), MatchResult::Normal);
    fertig.official1_id = Some(1);
    let mut wo = beendet(spiel(13, &["G"], &["H"]), MatchResult::Walkover);
    wo.official1_id = Some(1);
    let mut s = snap(vec![1], vec![lauf, warte, fertig, wo]);
    s.officials = vec![BtpOfficial {
        id: 1,
        name: "Schiri".into(),
        first: "Sabine".into(),
        nationality: None,
    }];
    let t = tablet_mit(s);
    t.officials_store().set_enabled(enabled);
    t
}

#[test]
fn schiedsrichter_wird_am_feld_wartend_und_beendet_gefunden() {
    let t = mit_sr(true);
    let r = suchen(&t, &AppConfig::default(), "sabine");
    assert!(r.officials_managed);
    assert_eq!(ids(&r.court, |h| h.kern.match_id), vec![10]);
    assert_eq!(r.court.hits[0].kern.roles, vec!["sr"]);
    assert_eq!(ids(&r.waiting, |h| h.kern.match_id), vec![11]);
    assert_eq!(r.waiting.hits[0].kern.roles, vec!["ar"]);
    assert_eq!(
        ids(&r.finished, |h| h.kern.match_id),
        vec![12],
        "Walkover zählt nicht als Einsatz"
    );
    assert_eq!(r.finished.hits[0].kern.sr, vec!["Sabine Schiri"]);
}

#[test]
fn ohne_schiedsrichter_betrieb_keine_sr_treffer() {
    let t = mit_sr(false);
    let r = suchen(&t, &AppConfig::default(), "sabine");
    assert!(!r.officials_managed);
    assert!(r.court.hits.is_empty() && r.waiting.hits.is_empty() && r.finished.hits.is_empty());
}

#[test]
fn spieler_und_sr_im_selben_spiel_tragen_beide_rollen() {
    let mut m = spiel(20, &["Sabine Schiri"], &["X"]);
    m.official1_id = Some(1);
    let mut s = snap(vec![], vec![m]);
    s.officials = vec![BtpOfficial {
        id: 1,
        name: "Schiri".into(),
        first: "Sabine".into(),
        nationality: None,
    }];
    let t = tablet_mit(s);
    t.officials_store().set_enabled(true);
    let r = suchen(&t, &AppConfig::default(), "sabine schiri");
    assert_eq!(r.waiting.hits[0].kern.roles, vec!["player", "sr"]);
}

// ── Spielende ─────────────────────────────────────────────────────────

#[test]
fn spielende_nur_wenn_gemessen() {
    let ms = vec![
        beendet(spiel(30, &["Max Müller"], &["X"]), MatchResult::Normal),
        beendet(spiel(31, &["Max Müller"], &["Y"]), MatchResult::Normal),
    ];
    let t = tablet_mit(snap(vec![], ms));
    let store = t.match_times_store();
    store.reconcile(
        &[(31, "A", "HE", "")],
        &std::collections::HashSet::new(),
        1_000_000,
    );
    store.stamp_finished(31, true, 1_000_000 + 30 * 60_000);
    // Spiel 30 nur „zuerst als beendet gesehen" — keine echte Messung.
    store.stamp_finished_seen(30, 0, 2_000_000);
    let r = suchen(&t, &AppConfig::default(), "müller");
    let ende = |id| {
        r.finished
            .hits
            .iter()
            .find(|h| h.kern.match_id == id)
            .unwrap()
            .ended_ms
    };
    assert_eq!(ende(31), Some(1_000_000 + 30 * 60_000));
    assert_eq!(ende(30), None);
}

// ── Datenschutz-Wächter ───────────────────────────────────────────────

/// Alle Schlüssel eines JSON-Werts mit Pfad.
fn schluessel(v: &serde_json::Value, pfad: &str, aus: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(o) => {
            for (k, w) in o {
                let p = format!("{pfad}.{k}");
                aus.push(p.clone());
                schluessel(w, &p, aus);
            }
        }
        serde_json::Value::Array(a) => {
            for w in a {
                schluessel(w, &format!("{pfad}[]"), aus);
            }
        }
        _ => {}
    }
}

/// Ein Turnier, das jede Gruppe, Kandidaten und SR an Beendet abdeckt.
fn wachter_fixture() -> String {
    let mut vorspiel = spiel(8, &["Max Müller"], &["Eva Engel"]);
    vorspiel.planning_id = 808;
    let mut offen = spiel(9, &[], &["Max Müller"]);
    offen.from1 = Some(808);
    let mut lauf = am_feld(spiel(10, &["Max Müller"], &["B"]), 1);
    lauf.official1_id = Some(1);
    let mut fertig = beendet(spiel(12, &["Max Müller"], &["F"]), MatchResult::Normal);
    fertig.official1_id = Some(1);
    let mut s = snap(vec![1], vec![vorspiel, offen, lauf, fertig]);
    s.officials = vec![BtpOfficial {
        id: 1,
        name: "Müller".into(),
        first: "Max".into(),
        nationality: Some("GER".into()),
    }];
    let t = tablet_mit(s);
    let store = t.officials_store();
    store.set_enabled(true);
    store.set_club(1, "TSV Sperrverein");
    store.set_blocklists(1, vec!["SC Gesperrt".into()], vec![4711]);
    search_json(&t, &mit_prognose(), "max müller", true, 1_000_000).unwrap()
}

#[test]
fn search_answer_fields_are_deliberately_allowed() {
    let json = wachter_fixture();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Die Fixture muss alles abdecken, was der Wächter prüfen soll.
    assert!(!v["court"]["hits"].as_array().unwrap().is_empty());
    assert!(v["waiting"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["open"] == true));
    assert!(!v["finished"]["hits"][0]["sr"]
        .as_array()
        .unwrap()
        .is_empty());

    const ERLAUBT: &[&str] = &[
        "server_now_ms",
        "has_data",
        "officials_managed",
        "court",
        "waiting",
        "finished",
        "hits",
        "more",
        "match_id",
        "match_num",
        "draw_name",
        "round_name",
        "class_label",
        "discipline",
        "team1",
        "team2",
        "team1_club",
        "team2_club",
        "team1_ids",
        "team2_ids",
        "hall",
        "sr",
        "ar",
        "roles",
        "court_id",
        "on_court_since_ms",
        "sets",
        "clearing",
        "has_timeline",
        "position",
        "predicted_start_ms",
        "predicted_uncertain",
        "called",
        "excluded",
        "wish_court",
        "open",
        "open_slot1_label",
        "open_slot2_label",
        "winner",
        "result",
        "ended_ms",
    ];
    let mut alle = Vec::new();
    schluessel(&v, "", &mut alle);
    for p in alle {
        let feld = p.rsplit('.').next().unwrap();
        assert!(
            ERLAUBT.contains(&feld),
            "Feld „{p}“ steht nicht auf der Positivliste"
        );
    }
}

#[test]
fn search_answer_never_carries_personal_data_beyond_its_purpose() {
    let json = wachter_fixture();
    for verboten in [
        "Sperrverein",
        "SC Gesperrt",
        "4711",
        "seed",
        "birth",
        "GER",
        "nation",
    ] {
        assert!(
            !json.contains(verboten),
            "„{verboten}“ darf nicht in der Suchantwort stehen: {json}"
        );
    }
    assert!(
        !json.contains("max müller"),
        "der Suchbegriff wird nicht zurückgespiegelt"
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for h in v["waiting"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h["open"] == true)
    {
        for f in ["team1_ids", "team2_ids", "team1_club", "team2_club"] {
            assert!(h.get(f).is_none(), "offene Paarung trägt „{f}“");
        }
    }
}

// ── Last ──────────────────────────────────────────────────────────────

#[test]
#[ignore = "Lasttest — manuell: cargo test -p bts-light suche_lasttest -- --ignored --nocapture"]
fn suche_lasttest_1500_spiele() {
    let mut ms = Vec::new();
    for id in 1..=1500 {
        let mut m = spiel(
            id,
            &[&format!("Vorname{id} Nachname{id}"), "Partner"],
            &[&format!("Gegner{id}"), "Zweiter"],
        );
        if id <= 900 {
            m = beendet(m, MatchResult::Normal);
        }
        ms.push(m);
    }
    let t = tablet_mit(snap((1..=20).collect(), ms));
    let cfg = mit_prognose();
    let start = std::time::Instant::now();
    for _ in 0..10 {
        search_json(&t, &cfg, "nachname1", true, 1_000_000).unwrap();
    }
    let ms = start.elapsed().as_millis() / 10;
    println!("Suche über 1500 Spiele: {ms} ms");
    assert!(ms < 20, "{ms} ms");
}
