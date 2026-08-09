//! Das BTP-Experiment zur Ergebnis-Korrektur — **schreibt in ein echtes BTP**.
//!
//! Beantwortet die offene Frage aus [`docs/btp_protocol.md`]: Was macht BTP,
//! wenn eine bereits gewertete KO-Paarung überschrieben wird? Rechnet es den
//! Turnierbaum neu, oder bleibt der alte Sieger in der nächsten Runde stehen?
//!
//! Davon hängt ab, wie weit die Turnierleitungs-Oberfläche Korrekturen
//! zulassen darf. Solange die Antwort fehlt, sperrt `tl::correction_blocker`
//! jeden Fall mit Folgespiel — und ein Mitschnitt aus einem echten Turnier
//! zeigt, dass das **alle** gewerteten Spiele betrifft.
//!
//! **Standardmäßig übersprungen** (`#[ignore]`): Der Test braucht ein
//! laufendes BTP mit eingeschalteter „TP Network"-Schnittstelle und
//! **verändert Ergebnisse darin**. Nur gegen ein Wegwerf-Turnier laufen
//! lassen:
//!
//! ```text
//! cargo test -p bts-light --test btp_overwrite_experiment -- --ignored --nocapture
//! ```
//!
//! Steuerung über Umgebungsvariablen: `BTP_HOST` (Standard 127.0.0.1),
//! `BTP_PORT` (9901), `BTP_PASSWORD` (leer).
//!
//! Der Test nennt **keine Spielernamen** — nur IDs und Strukturdaten.

use bts_light_lib::btp::{client, model, proto};

fn host() -> String {
    std::env::var("BTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn port() -> u16 {
    std::env::var("BTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9901)
}

fn password() -> Option<String> {
    std::env::var("BTP_PASSWORD").ok().filter(|p| !p.is_empty())
}

/// Holt den aktuellen Turnierstand.
async fn snapshot() -> model::BtpSnapshot {
    let raw = client::send_request(
        &host(),
        port(),
        &proto::tournament_info_request(password().as_deref()),
    )
    .await
    .expect("BTP erreichbar");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    model::parse_snapshot(&nodes).expect("Snapshot")
}

/// Schreibt eine Wertung und liefert die BTP-Antwort.
async fn write(update: &proto::MatchUpdate) -> Result<(), String> {
    let pw = password();
    let login_raw = client::send_request(&host(), port(), &proto::login_request(pw.as_deref()))
        .await
        .map_err(|e| e.to_string())?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let raw = client::send_request(
        &host(),
        port(),
        &proto::update_request(update, &session, pw.as_deref()),
    )
    .await
    .map_err(|e| e.to_string())?;
    proto::parse_update_response(&proto::decode_response(&raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Was über ein Spiel und sein Folgespiel zu berichten ist.
fn bericht(snap: &model::BtpSnapshot, vor_id: i64, folge_id: i64, wann: &str) {
    let finde = |id: i64| snap.matches.iter().find(|m| m.id == id);
    println!("\n── {wann} ──");
    if let Some(m) = finde(vor_id) {
        println!(
            "  Vorspiel   {vor_id}: winner={:?} status={:?} entry1={} entry2={}",
            m.winner, m.status, m.entry1_id, m.entry2_id
        );
    }
    if let Some(f) = finde(folge_id) {
        println!(
            "  Folgespiel {folge_id}: winner={:?} status={:?} entry1={} entry2={} \
             from1={:?} from2={:?}",
            f.winner, f.status, f.entry1_id, f.entry2_id, f.from1, f.from2
        );
        println!(
            "    → Teilnehmerzahl links/rechts: {}/{}",
            f.team1.len(),
            f.team2.len()
        );
    }
}

/// Der **vollständige** Versuch: erst den Baum bis zu einem besetzten
/// Folgespiel füllen, dann überschreiben.
///
/// Der kurze Versuch unten reicht nicht, solange kein Folgespiel besetzt
/// ist — und in einem frisch begonnenen Turnier ist das nie der Fall. Hier
/// werden deshalb **beide** Vorgänger eines Folgespiels gewertet (damit BTP
/// die nächste Paarung überhaupt füllt) und danach einer davon geändert.
/// Das ist der Fall, den die Spec offen lässt: „Nachfolger existiert, Sieger
/// dort eingesetzt, aber noch nicht gestartet."
#[tokio::test]
#[ignore = "schreibt MEHRERE Ergebnisse in ein echtes BTP — nur gegen ein Wegwerf-Turnier"]
async fn filling_a_bracket_then_overwriting_shows_what_btp_recomputes() {
    let snap = snapshot().await;
    println!("Turnier: {}", snap.tournament_name);

    // Ein Folgespiel suchen, dessen beide Vorgänger existieren und offen
    // sind — das lässt sich mit zwei Wertungen in den kritischen Zustand
    // bringen.
    let ziel = snap.matches.iter().find_map(|f| {
        let (p1, p2) = (f.from1?, f.from2?);
        if f.winner.is_some() {
            return None;
        }
        let v1 = snap
            .matches
            .iter()
            .find(|m| m.draw_id == f.draw_id && m.planning_id == p1)?;
        let v2 = snap
            .matches
            .iter()
            .find(|m| m.draw_id == f.draw_id && m.planning_id == p2)?;
        Some((f, v1, v2))
    });
    let Some((folge, v1, v2)) = ziel else {
        println!("Kein Folgespiel mit zwei bekannten Vorgängern gefunden.");
        return;
    };
    println!(
        "Versuchsaufbau: Vorgänger {} und {} → Folgespiel {} (Draw {})",
        v1.id, v2.id, folge.id, folge.draw_id
    );

    let wertung = |m: &model::BtpMatch, team1_won: bool| proto::MatchUpdate {
        btp_match_id: m.id,
        draw_id: m.draw_id,
        planning_id: m.planning_id,
        sets: if team1_won {
            vec![(21, 15), (21, 10)]
        } else {
            vec![(15, 21), (10, 21)]
        },
        team1_won,
        duration_mins: 20,
        score_status: 0,
        free_court_id: None,
        player_ids: Vec::new(),
        end_ts_ms: None,
    };

    bericht(&snap, v1.id, folge.id, "SCHRITT 0 — Ausgangslage");

    // Beide Vorgänger werten, damit BTP die nächste Paarung füllt.
    for (m, name) in [(v1, "Vorgänger 1"), (v2, "Vorgänger 2")] {
        if m.winner.is_none() {
            match write(&wertung(m, true)).await {
                Ok(()) => println!("  {name} ({}) gewertet: Team 1 gewinnt.", m.id),
                Err(e) => println!("  {name} ({}) NICHT schreibbar: {e}", m.id),
            }
        } else {
            println!("  {name} ({}) war schon gewertet.", m.id);
        }
    }
    let gefuellt = snapshot().await;
    bericht(
        &gefuellt,
        v1.id,
        folge.id,
        "SCHRITT 1 — beide Vorgänger gewertet",
    );
    let besetzt_vorher = gefuellt
        .matches
        .iter()
        .find(|m| m.id == folge.id)
        .map(|f| (f.entry1_id, f.entry2_id, f.team1.len(), f.team2.len()));
    println!("  Folgespiel besetzt mit: {besetzt_vorher:?}");

    // Jetzt der eigentliche Versuch: einen der Vorgänger umdrehen.
    match write(&wertung(v1, false)).await {
        Ok(()) => println!("\nÜberschreiben von {}: angenommen (Result=1).", v1.id),
        Err(e) => println!("\nÜberschreiben von {}: ABGELEHNT — {e}", v1.id),
    }
    let danach = snapshot().await;
    bericht(
        &danach,
        v1.id,
        folge.id,
        "SCHRITT 2 — nach dem Überschreiben",
    );
    let besetzt_nachher = danach
        .matches
        .iter()
        .find(|m| m.id == folge.id)
        .map(|f| (f.entry1_id, f.entry2_id, f.team1.len(), f.team2.len()));
    println!("  Folgespiel besetzt mit: {besetzt_nachher:?}");

    println!(
        "\n═══ ANTWORT: BTP rechnet den Baum beim Überschreiben {} ═══",
        if besetzt_vorher == besetzt_nachher {
            "NICHT neu — der alte Sieger bleibt in der nächsten Runde stehen."
        } else {
            "NEU — die nächste Paarung wandert mit."
        }
    );
}

#[tokio::test]
#[ignore = "schreibt in ein echtes BTP — nur gegen ein Wegwerf-Turnier"]
async fn overwriting_a_ko_result_changes_the_bracket_or_not() {
    let snap = snapshot().await;
    println!("Turnier: {}", snap.tournament_name);

    // Ein Paar suchen, an dem sich die Frage zeigt: ein **bereits
    // gewertetes** KO-Spiel, dessen Folgespiel noch offen ist und auf keinem
    // Feld steht.
    //
    // Bewusst ein gewertetes: Ein Spiel mit feststehenden Teilnehmern **und**
    // Folgespiel ist im KO-Baum praktisch immer schon gewertet — sonst wüsste
    // BTP die Teilnehmer der nächsten Runde gar nicht. Und genau dieses
    // Spiel will die Turnierleitung korrigieren, wenn sie sich vertippt hat.
    let paar = snap.matches.iter().find_map(|vor| {
        vor.winner?;
        let folge = snap.matches.iter().find(|f| {
            f.draw_id == vor.draw_id
                && f.id != vor.id
                && (f.from1 == Some(vor.planning_id) || f.from2 == Some(vor.planning_id))
                && f.winner.is_none()
                && f.court_id.is_none()
        })?;
        Some((vor, folge))
    });
    let Some((vor, folge)) = paar else {
        println!("Kein geeignetes KO-Paar gefunden — bitte ein Turnier mit offenem Baum laden.");
        return;
    };
    let (vor_id, folge_id) = (vor.id, folge.id);
    println!(
        "Versuchsobjekt: Spiel {vor_id} (Draw {}, Position {}) → Folgespiel {folge_id}",
        vor.draw_id, vor.planning_id
    );
    bericht(&snap, vor_id, folge_id, "VORHER (so steht es in BTP)");
    // Der bisherige Sieger — überschrieben wird mit dem **anderen**.
    let bisher_team1 = vor.winner == Some(1);

    let update = |team1_won: bool| proto::MatchUpdate {
        btp_match_id: vor_id,
        draw_id: vor.draw_id,
        planning_id: vor.planning_id,
        sets: if team1_won {
            vec![(21, 15), (21, 10)]
        } else {
            vec![(15, 21), (10, 21)]
        },
        team1_won,
        duration_mins: 20,
        score_status: 0,
        free_court_id: None,
        player_ids: Vec::new(),
        end_ts_ms: None,
    };

    // **Die eigentliche Frage:** dieselbe Paarung mit dem anderen Sieger
    // überschreiben.
    match write(&update(!bisher_team1)).await {
        Ok(()) => println!("\nÜberschreiben: BTP hat angenommen (Result=1)."),
        Err(e) => println!("\nÜberschreiben: BTP hat ABGELEHNT — {e}"),
    }
    let nach_zweiter = snapshot().await;
    bericht(
        &nach_zweiter,
        vor_id,
        folge_id,
        "NACH DEM ÜBERSCHREIBEN (anderer Sieger)",
    );

    // Die Antwort in einem Satz.
    let vorher_entry = snap
        .matches
        .iter()
        .find(|m| m.id == folge_id)
        .map(|f| (f.entry1_id, f.entry2_id));
    let nachher_entry = nach_zweiter
        .matches
        .iter()
        .find(|m| m.id == folge_id)
        .map(|f| (f.entry1_id, f.entry2_id));
    println!(
        "\n═══ ERGEBNIS: Das Folgespiel {} sich beim Überschreiben. ═══",
        if vorher_entry == nachher_entry {
            "ändert NICHT"
        } else {
            "ÄNDERT"
        }
    );
    println!("   vorher: {vorher_entry:?}   nachher: {nachher_entry:?}");
}
