//! Tests Paket S2: Standby-Übernahme ohne Netz — Spiegeln der Posten,
//! Notfall-Sicherung, Übernahme und Outbox im Standby-Modus gegen die
//! badhub-Attrappe (Antworten wie badhub S1, PR #1401).

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex as StdMutex};

use super::paket::PaketLease;
use super::standby::{self, Sicherung};
use super::tests::UUID;
use super::tests_l3::{aufruf, gekoppelt, lease_schritt, mit, Badhub, TOKEN};
use super::*;
use crate::btp::model::MatchStatus;

type Z = StdMutex<Option<KernQuelle>>;

/// Hauptrechner unter der Lease (Epoche 1) und ein Standby-PC mit eigenem
/// Paket, den badhub als Standby kennt; beide mit demselben Slave-Schlüssel.
async fn aufbau() -> (
    tempfile::TempDir,
    Arc<Badhub>,
    Z,
    tempfile::TempDir,
    Z,
    String,
) {
    let (dm, b, cfg, zm) = gekoppelt().await;
    lease_schritt(&zm, None).await;
    let k = slave::neuer_schluessel().unwrap();
    mit(&zm, |q| q.slave_schluessel = Some(k.clone()));
    let zs = standby_pc(&b, &cfg, true);
    (dm, b, zm, zs.0, zs.1, k)
}

/// Ein zweiter gekoppelter PC (eigener Ordner, Paket von badhub).
fn standby_pc(b: &Badhub, cfg: &BadhubTurnierConfig, als_standby: bool) -> (tempfile::TempDir, Z) {
    let ds = tempfile::tempdir().unwrap();
    let a = Ablage::neu(ds.path(), UUID).unwrap();
    let mut p = b.paket.lock().unwrap().clone().unwrap();
    p.ereignisse = b.ereignisse.lock().unwrap().clone();
    a.paket_speichern(&p, None).unwrap();
    a.token_speichern(TOKEN).unwrap();
    let mut q = KernQuelle::laden(ds.path(), cfg).unwrap();
    if als_standby {
        q.rolle_setzen(Some(PaketLease {
            epoche: 1,
            geraet: "Halle 1 PC".into(),
            standby: Some("install-standby".into()),
            du_bist: Some("standby".into()),
        }));
    }
    (ds, StdMutex::new(Some(q)))
}

/// Ein Spiegel-Abruf wie über das LAN (`?ab=<n>&posten=1`).
fn spiegeln(zm: &Z, zs: &Z, k: &str) {
    let ab = mit(zs, |q| q.spiegel_n());
    let m = mit(zm, |q| slave::fuer_slave_aus(q, Some(k), ab, true).unwrap());
    mit(zs, |q| q.spiegel_aufnehmen(m)).unwrap();
}

fn zuruecknehmen(spiel: u32) -> turnierkern::TurnierBefehl {
    turnierkern::TurnierBefehl::AufrufZuruecknehmen {
        id: String::new(),
        konkurrenz: "HE".into(),
        spiel,
    }
}

#[tokio::test]
async fn standby_spiegelt_die_posten_des_hauptrechners() {
    let (_dm, _b, zm, _ds, zs, k) = aufbau().await;
    let basis = mit(&zs, |q| q.paket.n());
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    assert_eq!(mit(&zs, |q| q.spiegel.posten().len()), 1);
    assert_eq!(mit(&zs, |q| q.spiegel_n()), basis + 1);
    // Befehl samt Kennung, nicht nur Ereignisse.
    let id_master = mit(&zm, |q| q.protokoll.posten()[0].befehl.id().to_string());
    assert_eq!(
        mit(&zs, |q| q.spiegel.posten()[0].befehl.id().to_string()),
        id_master
    );
    let snap = mit(&zs, |q| q.stand_standby());
    let m = snap.matches.iter().find(|m| m.planning_id == 1).unwrap();
    assert_eq!(m.status, MatchStatus::OnCourt);

    // Der Hauptrechner lädt hoch → bestätigt; der Standby übernimmt das
    // Bestätigte ins Paket, das Neue in den Spiegel.
    assert_eq!(outbox_schritt(&zm).await, OutboxSchritt::Weiter);
    mit(&zm, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    assert_eq!(mit(&zs, |q| q.paket.n()), basis + 1);
    assert_eq!(mit(&zs, |q| q.spiegel_n()), basis + 2);
    let lage = mit(&zs, |q| q.standby_lage());
    assert_eq!(lage.rolle, "standby");
    assert_eq!(lage.epoche, 1);
    assert!(
        lage.text.contains("Standby für Halle 1 PC"),
        "{}",
        lage.text
    );
    assert!(
        lage.text
            .contains(&format!("gespiegelt bis n = {}", basis + 2)),
        "{}",
        lage.text
    );
    assert!(lage.kann_uebernehmen);
    // Dauerhaft: nach einem Neustart steht der Spiegel noch da.
    let dir = mit(&zs, |q| q.ablage.dir().to_path_buf());
    assert_eq!(
        protokoll::Protokoll::laden_als(&dir, standby::SPIEGEL_DATEI)
            .unwrap()
            .n(),
        basis + 2
    );

    // Ein älterer Hauptrechner liefert keine Posten: klare Meldung.
    let mut alt = mit(&zm, |q| {
        slave::fuer_slave_aus(q, Some(&k), 0, true).unwrap()
    });
    alt.posten = None;
    alt.bestaetigt_n = None;
    let e = mit(&zs, |q| q.spiegel_aufnehmen(alt)).unwrap_err();
    assert!(e.contains("ältere"), "{e}");
}

#[tokio::test]
async fn ohne_posten_wunsch_bleibt_die_antwort_des_masters_wie_bisher() {
    let (_dm, _b, zm, _ds, _zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    let m = mit(&zm, |q| {
        slave::fuer_slave_aus(q, Some(&k), 0, false).unwrap()
    });
    let json = serde_json::to_value(&m).unwrap();
    let schluessel: Vec<&String> = json.as_object().unwrap().keys().collect();
    assert_eq!(schluessel, vec!["ereignisse", "n", "uuid"]);
}

#[tokio::test]
async fn notfall_sicherung_schreiben_und_einlesen() {
    let (_dm, b, zm, _ds, zs, _k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    mit(&zm, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap();
    let ordner = tempfile::tempdir().unwrap();
    assert!(notfall_sichern_in(&zm, ordner.path(), true));
    // Unverändert: keine zweite Sicherung.
    assert!(!notfall_sichern_in(&zm, ordner.path(), true));
    let datei = ordner.path().join(standby::sicherung_dateiname(UUID));
    let text = std::fs::read_to_string(&datei).unwrap();
    // Keine Namen, keine Tokens.
    assert!(
        !text.contains("V1") && !text.contains("N1") && !text.contains(TOKEN),
        "{text}"
    );
    assert!(!text.contains("akteur"), "{text}");
    let s = standby::sicherung_lesen(&datei).unwrap();
    assert_eq!(s.epoche, 1);
    assert_eq!(s.posten.len(), 2);
    let n_master = mit(&zm, |q| q.n_lokal());
    assert_eq!(s.n, n_master);

    // Einlesen am Standby (ohne LAN-Spiegel).
    assert!(mit(&zs, |q| q.sicherung_einlesen(s.clone())).is_ok());
    assert_eq!(mit(&zs, |q| q.spiegel_n()), n_master);
    let lage = mit(&zs, |q| q.standby_lage());
    assert_eq!(lage.quelle, "sicherung");
    assert!(lage.text.contains("Notfall-Sicherung"), "{}", lage.text);
    // Noch einmal: nichts Neues.
    let e = mit(&zs, |q| q.sicherung_einlesen(s.clone())).unwrap_err();
    assert!(e.contains("nichts"), "{e}");

    // Abgeschnittene Datei: abgelehnt.
    let halb = ordner.path().join("halb.btsk");
    std::fs::write(&halb, &text.as_bytes()[..text.len() / 2]).unwrap();
    let e = standby::sicherung_lesen(&halb).unwrap_err();
    assert!(e.contains("unvollständig"), "{e}");
    // Falsches Turnier.
    let mut fremd: Sicherung = s.clone();
    fremd.uuid = "0f8b2c1e-9999-4222-8333-444455556666".into();
    let (_d2, z2) = standby_pc(&b, &cfg_von(&zm), true);
    let e = mit(&z2, |q| q.sicherung_einlesen(fremd)).unwrap_err();
    assert!(e.contains("anderen Turnier"), "{e}");
    // Ohne Anschluss (erster Posten fehlt): abgelehnt.
    let mut luecke = s.clone();
    luecke.posten.remove(0);
    let e = mit(&z2, |q| q.sicherung_einlesen(luecke)).unwrap_err();
    assert!(e.contains("schließt nicht an"), "{e}");
    // Fremde Datei.
    let fremd_datei = ordner.path().join("x.btsk");
    std::fs::write(&fremd_datei, br#"{"format":"anders","version":1}"#).unwrap();
    assert!(standby::sicherung_lesen(&fremd_datei).is_err());
}

fn cfg_von(z: &Z) -> BadhubTurnierConfig {
    mit(z, |q| BadhubTurnierConfig {
        uuid: UUID.into(),
        id: 17,
        name: "Wintercup".into(),
        basis: q.basis.clone(),
        geraet_name: "Standby".into(),
    })
}

#[tokio::test]
async fn uebernahme_und_erster_upload_mit_standby() {
    let (_dm, b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    // Hauptrechner fällt aus (hat nichts hochgeladen).
    let text = mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    assert!(text.contains("neu koppeln"), "{text}");
    mit(&zs, |q| {
        let l = q.lease.as_ref().unwrap();
        assert!(l.aktiv() && l.standby);
        assert_eq!(l.epoche, 1);
        assert!(q.spiegel.posten().is_empty());
        assert_eq!(q.protokoll.posten().len(), 1);
    });
    // Ein zweites Übernehmen geht nicht.
    assert!(mit(&zs, |q| q.als_standby_uebernehmen()).is_err());
    // Lokaler Schreiber wie der Hauptrechner.
    assert_eq!(
        mit(&zs, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)),
        Ok(1)
    );
    // Vor dem ersten Upload: kein Abruf, keine Lease-Anfrage.
    let vorher = b.lease_aufrufe.load(Ordering::SeqCst);
    abgleich_schritt_in(&zs, true, Some("ns-standby")).await;
    assert_eq!(b.lease_aufrufe.load(Ordering::SeqCst), vorher);
    assert!(mit(&zs, |q| q.abgleich().text.contains("Standby")));

    b.standby_erlaubt.store(true, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Weiter);
    let body = b.letzter_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["standby"], serde_json::json!(true));
    mit(&zs, |q| {
        let l = q.lease.as_ref().unwrap();
        assert_eq!(l.epoche, 2);
        assert!(!l.standby);
        assert_eq!(q.wartende_ereignisse(), 0);
    });
    assert_eq!(b.n(), mit(&zs, |q| q.n_lokal()));
    // Danach Relay-Angaben nachmelden (`POST turnier/lease`).
    lease_schritt(&zs, Some("ns-standby")).await;
    assert_eq!(b.lease_aufrufe.load(Ordering::SeqCst), vorher + 1);
    assert!(mit(&zs, |q| q.tl_zugang.is_some()));
    // Der alte Hauptrechner bekommt danach `epoche_veraltet`.
    assert_eq!(outbox_schritt(&zm).await, OutboxSchritt::Angehalten);
    assert_eq!(mit(&zm, |q| q.abgleich().zustand), "entzogen");
}

#[tokio::test]
async fn ohne_bestaetigte_rolle_kein_uebernehmen() {
    let (_dm, b, zm, _ds, _zs, _k) = aufbau().await;
    let (_d2, z2) = standby_pc(&b, &cfg_von(&zm), false);
    let e = mit(&z2, |q| q.als_standby_uebernehmen()).unwrap_err();
    assert!(e.contains("nicht als Standby"), "{e}");
    // Rolle bekannt, aber nichts gespiegelt und keine Sicherung: Epoche aus badhub.
    mit(&z2, |q| {
        q.rolle_setzen(Some(PaketLease {
            epoche: 0,
            geraet: "Halle 1 PC".into(),
            standby: None,
            du_bist: Some("standby".into()),
        }))
    });
    let e = mit(&z2, |q| q.als_standby_uebernehmen()).unwrap_err();
    assert!(e.contains("Epoche"), "{e}");
}

#[tokio::test]
async fn schon_hochgeladene_posten_werden_uebersprungen() {
    let (_dm, b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    // Der Hauptrechner hat es noch hochgeladen, bevor er ausfiel.
    assert_eq!(outbox_schritt(&zm).await, OutboxSchritt::Weiter);
    mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    b.standby_erlaubt.store(true, Ordering::SeqCst);
    let n_vorher = b.n();
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Weiter);
    assert_eq!(b.n(), n_vorher, "nichts doppelt");
    mit(&zs, |q| {
        assert_eq!(q.lease.as_ref().unwrap().epoche, 2);
        assert_eq!(q.wartende_ereignisse(), 0);
        assert!(q.abgleich().warnung.is_none());
    });
}

#[tokio::test]
async fn teilweise_abgewiesen_zeigt_es_an_und_laedt_den_stand_neu() {
    let (_dm, b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    assert_eq!(outbox_schritt(&zm).await, OutboxSchritt::Weiter);
    // Noch eine Eingabe am Hauptrechner, hochgeladen, aber nicht gespiegelt.
    mit(&zm, |q| q.ausfuehren(vec![aufruf(2, "2")], AKTEUR_HALLE)).unwrap();
    assert_eq!(outbox_schritt(&zm).await, OutboxSchritt::Weiter);
    mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    mit(&zs, |q| q.ausfuehren(vec![zuruecknehmen(1)], AKTEUR_HALLE)).unwrap();
    b.standby_erlaubt.store(true, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Angehalten);
    mit(&zs, |q| {
        let l = q.lease.as_ref().unwrap();
        assert_eq!(l.epoche, 2);
        assert_eq!(l.abgewiesen, 1);
        assert!(l.neu_laden);
        let w = q.abgleich().warnung.unwrap();
        assert!(
            w.contains("1 Eingabe wurde") && w.contains("abgewiesen"),
            "{w}"
        );
        assert!(matches!(q.schreibrecht(), Err(SchreibFehler::Gesperrt(_))));
    });
    // Der Abgleich lädt den Stand von badhub; das Abgewiesene liegt daneben.
    abgleich_schritt_in(&zs, false, None).await;
    mit(&zs, |q| {
        assert!(!q.lease.as_ref().unwrap().neu_laden);
        assert_eq!(q.n_lokal(), b.n());
        assert!(q.protokoll.posten().is_empty());
        assert!(q.schreibrecht().is_ok());
        assert!(std::fs::read_dir(q.ablage.dir())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("verworfen-")));
    });
}

#[tokio::test]
async fn kein_standby_haelt_an_und_sagt_warum() {
    let (_dm, _b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Angehalten);
    let a = mit(&zs, |q| q.abgleich());
    assert!(a.rot && a.text.contains("nicht als Standby"), "{}", a.text);
    // Eingaben bleiben möglich, nichts verworfen.
    assert!(mit(&zs, |q| q.schreibrecht()).is_ok());
    assert_eq!(mit(&zs, |q| q.protokoll.posten().len()), 1);
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Ruhe);
}

#[tokio::test]
async fn inhaber_noch_online_behaelt_die_posten_und_versucht_spaeter() {
    let (_dm, b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    b.standby_erlaubt.store(true, Ordering::SeqCst);
    b.inhaber_aktiv.store(true, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Spaeter);
    mit(&zs, |q| {
        let w = q.abgleich().warnung.unwrap();
        assert!(w.contains("vor 30 s") && w.contains("2 Minuten"), "{w}");
        assert!(q.standby_offen());
        assert_eq!(q.protokoll.posten().len(), 1);
        assert!(q.schreibrecht().is_ok());
    });
    b.inhaber_aktiv.store(false, Ordering::SeqCst);
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Weiter);
    assert!(mit(&zs, |q| q.abgleich().warnung.is_none()));
}

#[tokio::test]
async fn epoche_veraltet_fragt_badhub_ob_die_uebernahme_schon_galt() {
    // a) Die Antwort ging verloren: badhub nennt diesen PC Inhaber.
    let (_dm, b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    *b.epoche.lock().unwrap() = 2;
    *b.lease.lock().unwrap() = Some(2);
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Angehalten);
    assert!(mit(&zs, |q| q.standby_pruefen));
    abgleich_schritt_in(&zs, false, None).await;
    mit(&zs, |q| {
        let l = q.lease.as_ref().unwrap();
        assert!(l.aktiv() && !l.standby && l.neu_laden);
        assert_eq!(l.epoche, 2);
    });

    // b) Wirklich veraltet (badhub hat zurückgeholt): entzogen.
    let (_dm, b, zm, _ds, zs, k) = aufbau().await;
    mit(&zm, |q| q.ausfuehren(vec![aufruf(1, "1")], AKTEUR_HALLE)).unwrap();
    spiegeln(&zm, &zs, &k);
    mit(&zs, |q| q.als_standby_uebernehmen()).unwrap();
    *b.epoche.lock().unwrap() = 2;
    *b.lease.lock().unwrap() = None;
    assert_eq!(outbox_schritt(&zs).await, OutboxSchritt::Angehalten);
    abgleich_schritt_in(&zs, false, None).await;
    assert_eq!(mit(&zs, |q| q.abgleich().zustand), "entzogen");
}

#[tokio::test]
async fn rolle_aus_badhub_wird_abgelegt() {
    let (_dm, b, zm, _ds, _zs, _k) = aufbau().await;
    let (_d2, z2) = standby_pc(&b, &cfg_von(&zm), false);
    assert!(!mit(&z2, |q| q.ist_standby()));
    b.standby_erlaubt.store(true, Ordering::SeqCst);
    *b.lease.lock().unwrap() = None; // aus Sicht dieses Geräts: kein Inhaber
    let (basis, token) = mit(&z2, |q| (q.basis.clone(), q.token.clone().unwrap()));
    let sicht = api::lease_lesen(&basis, &token).await.unwrap();
    mit(&z2, |q| q.rolle_setzen(sicht.lease));
    assert!(mit(&z2, |q| q.ist_standby()));
    let dir = mit(&z2, |q| q.ablage.dir().to_path_buf());
    assert!(standby::rolle_laden(&dir)
        .and_then(|r| r.lease)
        .is_some_and(|l| l.ist_standby()));
}
