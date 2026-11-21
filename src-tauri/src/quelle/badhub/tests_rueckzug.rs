//! Tests Kern 0.13.0/0.14.0: Rückzug während des Turniers (Abbildung,
//! Kernbefehl aus badhub, Rückzug aus dem TL-Web) und Zählweise je Spiel.

use turnierkern::*;

use super::abbildung::abbilden;
use super::schreiben;
use super::tests::{befehl, eine_halle, paket_fuer, turnier};
use super::tests_l3::{gekoppelt, lease_schritt, mit};
use super::*;
use crate::btp::model::{MatchResult, MatchStatus};

const JETZT: &str = "2026-12-06T11:00:00";

fn rueckzug(m: u32, e: Rueckzug) -> TurnierBefehl {
    TurnierBefehl::Rueckzug {
        id: format!("rz-{m}"),
        konkurrenz: "HE".into(),
        meldung: m,
        ergebnisse: e,
        jetzt: JETZT.into(),
    }
}

#[test]
fn entfallene_spiele_fehlen_im_modell_und_ruckzug_erscheint_wie_walkover() {
    // Gruppe mit vier Meldungen, eine zieht mit „streichen“ zurück: ihre
    // drei offenen Spiele entfallen — nicht aufrufbar, nicht in der Vergabe.
    let mut log = turnier(4, System::Gruppe { rueckspiel: false }, &[("H1", 2)]);
    let vorher = abbilden(
        &falten_turnier(&log),
        &paket_fuer(log.clone(), 4, eine_halle()),
    );
    assert_eq!(vorher.matches.len(), 6);
    befehl(&mut log, rueckzug(1, Rueckzug::Streichen)).unwrap();
    let st = falten_turnier(&log);
    assert_eq!(
        st.durchfuehrung["HE"]
            .spiele
            .iter()
            .filter(|s| s.status == SpielStatus::Entfaellt)
            .count(),
        3
    );
    let snap = abbilden(&st, &paket_fuer(log.clone(), 4, eine_halle()));
    assert_eq!(snap.matches.len(), 3);
    assert!(snap
        .matches
        .iter()
        .all(|m| m.entry1_id != 1 && m.entry2_id != 1));
    let zeile = st.durchfuehrung["HE"].tabellen[0]
        .zeilen
        .iter()
        .find(|z| z.meldung == 1)
        .unwrap();
    assert!(zeile.zurueckgezogen);

    // „behalten“: die offenen Spiele gehen kampflos an die Gegner — im
    // Modell beendet wie ein Walkover, mit Sieger.
    let mut log = turnier(4, System::Gruppe { rueckspiel: false }, &[("H1", 2)]);
    befehl(&mut log, rueckzug(2, Rueckzug::Behalten)).unwrap();
    let st = falten_turnier(&log);
    let snap = abbilden(&st, &paket_fuer(log, 4, eine_halle()));
    assert_eq!(snap.matches.len(), 6);
    let mit_zwei: Vec<_> = snap
        .matches
        .iter()
        .filter(|m| m.entry1_id == 2 || m.entry2_id == 2)
        .collect();
    assert_eq!(mit_zwei.len(), 3);
    for m in mit_zwei {
        assert_eq!(m.status, MatchStatus::Finished);
        assert_eq!(m.result, MatchResult::Walkover);
        let sieger_ist_zwei =
            (m.winner == Some(1) && m.entry1_id == 2) || (m.winner == Some(2) && m.entry2_id == 2);
        assert!(!sieger_ist_zwei, "der Gegner gewinnt kampflos");
    }
    assert!(st.durchfuehrung["HE"]
        .spiele
        .iter()
        .filter(|s| s.oben == Some(2) || s.unten == Some(2))
        .all(|s| s.durch_rueckzug));
}

#[test]
fn ko_rueckzug_entscheidet_das_offene_spiel_kampflos() {
    let mut log = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    let st = falten_turnier(&log);
    let s1 = st.durchfuehrung["HE"].spiele[0].clone();
    let weg = s1.oben.unwrap();
    befehl(&mut log, rueckzug(weg, Rueckzug::Behalten)).unwrap();
    let st = falten_turnier(&log);
    let snap = abbilden(&st, &paket_fuer(log.clone(), 4, eine_halle()));
    let m = snap.matches.iter().find(|m| m.planning_id == 1).unwrap();
    assert_eq!(m.status, MatchStatus::Finished);
    assert_eq!(m.result, MatchResult::Walkover);
    assert_eq!(m.winner, Some(2));
    // Ein Ergebnis für das so entschiedene Spiel lehnt der Kern verständlich ab.
    let e = pruefen_turnier(
        &st,
        &TurnierBefehl::ErgebnisMelden {
            id: "x".into(),
            konkurrenz: "HE".into(),
            spiel: 1,
            saetze: vec![(21, 10), (21, 10)],
            ende: None,
            jetzt: JETZT.into(),
            beginn: None,
            bis: None,
            netto_min: None,
        },
    )
    .unwrap_err();
    assert!(schreiben::ablehnung_text(&e).contains("Rückzug"), "{e:?}");
}

#[test]
fn zaehlweise_je_spiel_landet_am_spiel_im_modell() {
    let mut log = turnier(4, System::Ko { platz3: false }, &[("H1", 2)]);
    befehl(
        &mut log,
        TurnierBefehl::ZaehlweiseFestlegen {
            id: "zw".into(),
            konkurrenz: "HE".into(),
            spiel: 1,
            zaehlweise: Some(Zaehlweise::badminton_15()),
        },
    )
    .unwrap();
    let snap = abbilden(&falten_turnier(&log), &paket_fuer(log, 4, eine_halle()));
    let s1 = snap.matches.iter().find(|m| m.planning_id == 1).unwrap();
    let s2 = snap.matches.iter().find(|m| m.planning_id == 2).unwrap();
    assert_eq!(s1.scoring.target_score, 15);
    assert_eq!(s2.scoring.target_score, 21);
}

#[test]
fn ablehnungen_des_rueckzugs_sind_verstaendlich() {
    let k = || KonkurrenzKey::from("HE");
    for (a, wort) in [
        (
            TurnierAblehnung::SchonZurueckgezogen {
                konkurrenz: k(),
                meldung: 3,
            },
            "schon zurückgezogen",
        ),
        (
            TurnierAblehnung::NichtZurueckgezogen {
                konkurrenz: k(),
                meldung: 3,
            },
            "nicht zurückgezogen",
        ),
        (
            TurnierAblehnung::NichtZuruecknehmbar {
                konkurrenz: k(),
                meldung: 3,
                spiel: 5,
            },
            "Spiel 5",
        ),
        (
            TurnierAblehnung::MeldungZurueckgezogen {
                konkurrenz: k(),
                spiel: 5,
            },
            "Rückzug",
        ),
    ] {
        let t = schreiben::ablehnung_text(&a);
        assert!(t.contains(wort), "{t}");
        assert!(
            !t.contains("lehnt ab ("),
            "eigener Text statt Rückfall: {t}"
        );
    }
}

#[tokio::test]
async fn rueckzug_als_kernbefehl_aus_badhub() {
    let (_d, _b, _c, z) = gekoppelt().await;
    lease_schritt(&z, Some("ns")).await;
    let befehl = serde_json::json!({
        "art": "rueckzug", "id": "bh-90", "konkurrenz": "HE", "meldung": 1, "ergebnisse": "behalten"
    });
    // `jetzt` fehlt: setzt der Knoten.
    assert_eq!(mit(&z, |q| q.befehl_von_badhub(befehl.clone())), Ok(1));
    mit(&z, |q| {
        let p = q.protokoll.posten().last().unwrap();
        assert!(matches!(&p.befehl, TurnierBefehl::Rueckzug { jetzt, .. } if !jetzt.is_empty()));
        assert_eq!(p.akteur, AKTEUR_BADHUB);
    });
    // Noch einmal mit neuer Kennung: Ablehnung im Wortlaut des Kerns.
    let e = mit(&z, |q| {
        q.befehl_von_badhub(serde_json::json!({
            "art": "rueckzug", "id": "bh-91", "konkurrenz": "HE", "meldung": 1, "ergebnisse": "streichen"
        }))
    });
    assert!(
        matches!(&e, Err(SchreibFehler::Kern { grund, text }) if grund["grund"] == "schon_zurueckgezogen" && text.contains("schon")),
        "{e:?}"
    );
    // Zurücknehmen, dann noch einmal → nicht zurückgezogen.
    let zurueck = serde_json::json!({"art": "rueckzug_zuruecknehmen", "id": "bh-92", "konkurrenz": "HE", "meldung": 1});
    assert_eq!(mit(&z, |q| q.befehl_von_badhub(zurueck)), Ok(1));
    let e = mit(&z, |q| {
        q.befehl_von_badhub(serde_json::json!({"art": "rueckzug_zuruecknehmen", "id": "bh-93", "konkurrenz": "HE", "meldung": 1}))
    });
    assert!(
        matches!(&e, Err(SchreibFehler::Kern { grund, .. }) if grund["grund"] == "nicht_zurueckgezogen"),
        "{e:?}"
    );
    // Feldprüfung: Meldung als Text, unbekannte Wahl → unlesbar.
    for roh in [
        serde_json::json!({"art": "rueckzug", "id": "bh-94", "konkurrenz": "HE", "meldung": "1"}),
        serde_json::json!({"art": "rueckzug", "id": "bh-95", "konkurrenz": "HE", "meldung": 1, "ergebnisse": "loeschen"}),
    ] {
        let e = mit(&z, |q| q.befehl_von_badhub(roh));
        assert!(matches!(e, Err(SchreibFehler::Ungueltig(_))), "{e:?}");
    }
}

#[tokio::test]
async fn rueckzug_aus_dem_tl_web_nur_unter_der_lease_mit_liste() {
    let (_d, _b, _c, z) = gekoppelt().await;
    // Ohne Lease: gesperrt, und das TL-Web bekommt keine Liste.
    let e = mit(&z, |q| q.rueckzug(1, Some(Rueckzug::Behalten))).unwrap_err();
    assert!(matches!(e, SchreibFehler::Gesperrt(_)), "{e:?}");
    lease_schritt(&z, None).await;
    let liste = mit(&z, |q| q.rueckzug_meldungen());
    assert_eq!(liste.len(), 4);
    assert!(liste
        .iter()
        .all(|m| !m.zurueckgezogen && m.konkurrenz == "Herreneinzel A"));
    assert_eq!(mit(&z, |q| q.rueckzug(2, Some(Rueckzug::Streichen))), Ok(1));
    let liste = mit(&z, |q| q.rueckzug_meldungen());
    let zwei = liste.iter().find(|m| m.entry_id == 2).unwrap();
    assert!(zwei.zurueckgezogen);
    assert_eq!(zwei.ergebnisse.as_deref(), Some("streichen"));
    assert_eq!(zwei.label, "V2 N2");
    // Unbekannte Meldung: klare Meldung, nichts geschrieben.
    let e = mit(&z, |q| q.rueckzug(99, None)).unwrap_err();
    assert!(matches!(e, SchreibFehler::Ungueltig(_)), "{e:?}");
    assert_eq!(mit(&z, |q| q.rueckzug(2, None)), Ok(1));
    assert!(mit(&z, |q| q.rueckzug_meldungen())
        .iter()
        .all(|m| !m.zurueckgezogen));
}
