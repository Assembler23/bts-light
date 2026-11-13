//! **Messwerkzeug, kein Regressionstest.** Wie behandelt BTP die Ballzahl
//! `Match.Shuttles`?
//!
//! Anlass (13.11.2026, Spec `docs/features/tablet-ballzaehler.md`, ADR 0093):
//! Das Zähl-Tablet soll die verbrauchten Bälle zählen und mit dem Ergebnis
//! nach BTP schreiben. bts-light liest und schreibt `Shuttles` bisher nicht.
//! Für `CourtID`, `Official1ID/2ID` und `Highlight` ist gemessen, dass BTP
//! sie aus **jedem** Match-Write übernimmt und ein fehlendes Element als 0
//! liest (`btp_highlight_sr_write_probe.rs`). Gehört `Shuttles` in dieselbe
//! Klasse, löscht jeder Feld-/Farb-/Schiedsrichter-Write die Ballzahl —
//! dann müssen alle Schreibwege sie mittragen (Zweig B der Spec). Bleibt sie
//! stehen, genügt der Ergebnis-Write (Zweig A).
//!
//! Drei Messungen, in dieser Reihenfolge laufen lassen:
//!
//! 1. `does_btp_deliver_shuttles` — **liest nur.** Kommt `Shuttles` in
//!    `SENDTOURNAMENTINFO` an, mit welchem Typ, und fehlt es bei 0?
//!    Aussagekräftig erst, wenn in BTP an mindestens einem Spiel von Hand
//!    eine Ballzahl eingetragen ist.
//! 2. `does_a_waiting_match_keep_shuttles` — **schreibt und stellt zurück.**
//!    An einem wartenden Spiel ohne Feld: Nimmt BTP `Shuttles` in einem
//!    Match-Write an? Bleibt der Wert nach einem Farb-Write und nach einem
//!    Schiedsrichter-Write, die `Shuttles` weglassen? Dazu die Latenz, bis
//!    der eigene Write beim Nachlesen sichtbar ist.
//! 3. `does_a_result_write_carry_shuttles` — **schreibt ein Ergebnis, nicht
//!    umkehrbar.** Nur an einem Testturnier. Läuft nur, wenn
//!    `BTP_SHUTTLES_MATCH=<DrawID>:<Match-ID>` ein Spiel nennt, das gerade **auf
//!    einem Feld** steht und noch kein Ergebnis hat. Misst am laufenden
//!    Spiel dasselbe wie 2, dann den Ergebnis-Write mit `Shuttles`, danach
//!    Farb- und Schiedsrichter-Write am **gewerteten** Spiel und einen
//!    zweiten Ergebnis-Write ohne `Shuttles`. Die Writes an ein gewertetes
//!    Spiel sind in BTP selbst nicht vermessen — sie können die Wertung
//!    berühren; das Feld am beendeten Spiel kann dabei auf 0 fallen.
//!
//! Die Writes entstehen aus den **echten** Schreibfunktionen
//! (`proto::highlight_request`, `proto::court_assign_request`,
//! `proto::update_request`); `Shuttles` wird
//! nachträglich in den fertigen Match-Knoten gehängt. So misst das Werkzeug
//! die Form, die bts-light später wirklich sendet.
//!
//! **Wie ein Folge-Write zählt:** Jeder Write ohne `Shuttles` ändert die
//! Zeilenfarbe auf einen Marker und stellt sie danach zurück. Erst wenn der
//! Marker beim Nachlesen steht, gilt der Write als verarbeitet; sonst meldet
//! das Werkzeug „nicht belegt" statt „bleibt".
//!
//! **Vor dem Lauf:** bts-light darf nicht gegen dasselbe BTP laufen, und in
//! BTP fasst währenddessen niemand das Zielspiel an — die Writes tragen Feld
//! und Besetzung aus dem zuletzt gelesenen Stand mit. Bricht ein Lauf mit
//! einer Fehlermeldung ab, kann am Zielspiel eine Ballzahl (2, 3 oder 5) und
//! die Marker-Farbe stehen bleiben; beides dann in BTP von Hand löschen.
//!
//! **Ergebnis (13.11.2026, Testturnier, 245 Match-Knoten): Zweig B.**
//! - BTP liefert `Shuttles` an jedem Match-Knoten als `Integer`, auch bei 0.
//! - Ein Match-Write mit `Shuttles` wird angenommen (wartend nach 420 ms
//!   sichtbar), ebenso der Ergebnis-Write (349 ms).
//! - Jeder Write ohne `Shuttles` setzt die Ballzahl auf 0: Farb- und
//!   Schiedsrichter-Write am wartenden, laufenden und gewerteten Spiel
//!   (317–682 ms), auch ein zweiter Ergebnis-Write.
//! - Nebenbefund: Farb- und Schiedsrichter-Write am **gewerteten** Spiel
//!   nahmen ihm den Sieger; der zweite Ergebnis-Write stellte ihn wieder her.
//!
//! Einzelheiten in `docs/btp_protocol.md` („Ballzahl `Match.Shuttles`").
//!
//! Standardmäßig übersprungen:
//!
//! ```text
//! cargo test -p bts-light --test btp_shuttles_probe does_btp_deliver_shuttles -- --ignored --nocapture
//! cargo test -p bts-light --test btp_shuttles_probe does_a_waiting_match_keep_shuttles -- --ignored --nocapture
//! $env:BTP_SHUTTLES_MATCH = "<DrawID>:<Match-ID>"
//! cargo test -p bts-light --test btp_shuttles_probe does_a_result_write_carry_shuttles -- --ignored --nocapture
//! ```
//!
//! Den Wert für Messung 3 nennt Messung 1 („Spiele auf einem Feld").

use bts_light_lib::btp::model::{BtpMatch, BtpSnapshot, MatchStatus};
use bts_light_lib::btp::xml::Node;
use bts_light_lib::btp::{client, proto, wire, xml};
use std::time::{Duration, Instant};

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

/// Roh-Knoten **und** geparster Snapshot: `BtpMatch` kennt `Shuttles` noch
/// nicht, die Ballzahl wird deshalb direkt aus dem Baum gelesen.
async fn lese(pw: Option<&str>) -> (Vec<Node>, BtpSnapshot) {
    let raw = client::send_request(&host(), port(), &proto::tournament_info_request(pw))
        .await
        .expect("BTP erreichbar (Lesen)");
    let nodes = proto::decode_response(&raw).expect("dekodierbar");
    let snap = bts_light_lib::btp::model::parse_snapshot(&nodes).expect("Snapshot");
    (nodes, snap)
}

async fn login(pw: Option<&str>) -> String {
    let raw = client::send_request(&host(), port(), &proto::login_request(pw))
        .await
        .expect("BTP erreichbar (LOGIN)");
    proto::parse_login_response(&proto::decode_response(&raw).expect("LOGIN-Antwort dekodierbar"))
        .expect("LOGIN akzeptiert (Passwort korrekt?)")
}

/// Alle `Match`-Gruppen des Baums (Turnier-Matches; Liga-`PlayerMatch`
/// heißt anders und bleibt außen vor).
fn alle_matches<'a>(nodes: &'a [Node], out: &mut Vec<&'a Node>) {
    for n in nodes {
        if let Node::Group { id, children } = n {
            if id == "Match" {
                out.push(n);
            } else {
                alle_matches(children, out);
            }
        }
    }
}

fn kind<'a>(n: &'a Node, id: &str) -> Option<&'a Node> {
    n.children().iter().find(|c| c.id() == id)
}

/// Ganzzahl-Wert eines Skalar-Knotens.
fn zahl(n: &Node) -> Option<i64> {
    n.value().and_then(|v| v.as_int())
}

/// Adresse eines Matches: Auslosung + ID, so wie BTP ein Match im
/// `SENDUPDATE` selbst adressiert. Ob die `Match.ID` allein im Turnier
/// eindeutig ist, zählt `does_btp_deliver_shuttles` nach (laut Messung vom
/// 26.10.2026 ist sie der Primärschlüssel der TP-Datei).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ziel {
    draw: i64,
    id: i64,
}

impl Ziel {
    fn von(m: &BtpMatch) -> Self {
        Ziel {
            draw: m.draw_id,
            id: m.id,
        }
    }

    /// Liest `<DrawID>:<Match-ID>`, wie `does_btp_deliver_shuttles` es ausgibt.
    fn aus_text(text: &str) -> Option<Self> {
        let (draw, id) = text.trim().split_once(':')?;
        Some(Ziel {
            draw: draw.trim().parse().ok()?,
            id: id.trim().parse().ok()?,
        })
    }
}

impl std::fmt::Display for Ziel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.draw, self.id)
    }
}

/// Der rohe `Shuttles`-Knoten eines Matches, `None` = Element fehlt.
fn shuttles_knoten(nodes: &[Node], ziel: Ziel) -> Option<Node> {
    let mut alle = Vec::new();
    alle_matches(nodes, &mut alle);
    alle.into_iter()
        .find(|m| {
            kind(m, "ID").and_then(zahl) == Some(ziel.id)
                && kind(m, "DrawID").and_then(zahl) == Some(ziel.draw)
        })
        .and_then(|m| kind(m, "Shuttles").cloned())
}

/// Ballzahl als Zahl; ein fehlendes Element zählt als 0.
fn shuttles(nodes: &[Node], ziel: Ziel) -> i64 {
    shuttles_knoten(nodes, ziel)
        .as_ref()
        .and_then(zahl)
        .unwrap_or(0)
}

/// Hängt `Shuttles` in jeden `Match`-Knoten eines fertigen Requests.
fn haenge_shuttles_an(nodes: &mut [Node], wert: i64) {
    for n in nodes {
        if let Node::Group { id, children } = n {
            if id == "Match" {
                children.push(Node::integer("Shuttles", wert));
            } else {
                haenge_shuttles_an(children, wert);
            }
        }
    }
}

/// Fertige Request-Bytes einer echten Schreibfunktion, um `Shuttles` ergänzt.
fn mit_shuttles(bytes: &[u8], wert: i64) -> Vec<u8> {
    let mut nodes = proto::decode_response(bytes).expect("eigener Request dekodierbar");
    haenge_shuttles_an(&mut nodes, wert);
    wire::encode_message(&xml::encode(&nodes))
}

/// Welche echte Schreibform ein Folge-Write nachstellt.
#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// `proto::highlight_request` (Farb-Write).
    Farbe,
    /// `proto::court_assign_request` ohne `Courts`-Block — der eigenständige
    /// Schiedsrichter-Write (`sync.rs::reconcile_officials`).
    Besetzung,
}

impl Form {
    fn name(self) -> &'static str {
        match self {
            Form::Farbe => "Farb-Write",
            Form::Besetzung => "Schiedsrichter-Write",
        }
    }
}

/// Match-Write der gewählten Form mit Feld und Besetzung aus `m` und der
/// Farbe `farbe` — **ohne** `Shuttles`. Beide Formen tragen alle Felder,
/// die BTP sonst auf 0 setzt (`CourtID`, `Official1ID/2ID`, `Highlight`).
fn schreibe(m: &BtpMatch, form: Form, farbe: u8, session: &str, pw: Option<&str>) -> Vec<u8> {
    let court_id = m.court_id.unwrap_or(0);
    let officials = (m.official1_id.unwrap_or(0), m.official2_id.unwrap_or(0));
    match form {
        Form::Farbe => proto::highlight_request(
            &[proto::HighlightEntry {
                match_id: m.id,
                draw_id: m.draw_id,
                planning_id: m.planning_id,
                highlight: farbe,
                shuttles: 0,
                court_id,
                officials,
            }],
            session,
            pw,
        ),
        Form::Besetzung => proto::court_assign_request(
            &[],
            &[proto::MatchCourt {
                match_id: m.id,
                draw_id: m.draw_id,
                planning_id: m.planning_id,
                court_id,
                officials: Some(officials),
                highlight: Some(farbe),
                shuttles: 0,
            }],
            session,
            pw,
        ),
    }
}

/// Sendet und meldet, ob BTP den Write angenommen hat.
async fn sende(bytes: Vec<u8>) -> bool {
    let raw = client::send_request(&host(), port(), &bytes)
        .await
        .expect("BTP erreichbar (Schreiben)");
    let antwort = proto::parse_update_response(&proto::decode_response(&raw).expect("dekodierbar"));
    println!("    Antwort: {antwort:?}");
    antwort.is_ok()
}

/// Frischer Stand eines Matches (Roh-Ballzahl + geparstes Match).
async fn frisch(pw: Option<&str>, id: Ziel) -> (i64, BtpMatch) {
    let (nodes, snap) = lese(pw).await;
    let m = snap
        .matches
        .iter()
        .find(|m| Ziel::von(m) == id)
        .cloned()
        .expect("Match da");
    (shuttles(&nodes, id), m)
}

fn zeige(s: i64, m: &BtpMatch) {
    println!(
        "    Stand: Shuttles={s} · Feld {:?} · SR {:?}/{:?} · Farbe {} · Sieger {:?}",
        m.court_id, m.official1_id, m.official2_id, m.highlight, m.winner
    );
}

/// Liest alle 100 ms nach, bis `ok` zutrifft (höchstens 6 s). Liefert den
/// Stand und die Wartezeit; `None` = die Bedingung trat nie ein.
async fn warte(
    pw: Option<&str>,
    id: Ziel,
    ok: impl Fn(i64, &BtpMatch) -> bool,
) -> Option<(i64, BtpMatch, u128)> {
    let t0 = Instant::now();
    for _ in 0..60 {
        let (s, m) = frisch(pw, id).await;
        if ok(s, &m) {
            return Some((s, m, t0.elapsed().as_millis()));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// Setzt die Ballzahl über einen Farb-Write mit unveränderter Farbe und
/// wartet, bis sie beim Nachlesen steht. `None` = nie sichtbar.
async fn setze_shuttles(pw: Option<&str>, id: Ziel, wert: i64, session: &str) -> Option<u128> {
    let (_, m) = frisch(pw, id).await;
    let bytes = mit_shuttles(&schreibe(&m, Form::Farbe, m.highlight, session, pw), wert);
    if !sende(bytes).await {
        return None;
    }
    let (s, m, ms) = warte(pw, id, |s, _| s == wert).await?;
    zeige(s, &m);
    Some(ms)
}

/// Ein Folge-Write **ohne** `Shuttles` an einem Spiel, das die Ballzahl
/// `wert` trägt. `Some(true)` = bleibt, `Some(false)` = weg, `None` = nicht
/// belegt.
///
/// Der Write ändert die Zeilenfarbe auf einen Marker. Erst wenn der Marker
/// beim Nachlesen steht, hat BTP den Write nachweislich verarbeitet — ein
/// abgelehnter oder verspäteter Write sähe sonst wie „bleibt" aus. Danach
/// kommt die alte Farbe zurück, zusammen mit der Ballzahl, damit die
/// nächste Messung von einem definierten Stand ausgeht.
async fn folge_write(
    pw: Option<&str>,
    id: Ziel,
    form: Form,
    wert: i64,
    session: &str,
) -> Option<bool> {
    let (vorher, m) = frisch(pw, id).await;
    if vorher != wert {
        println!(
            "  {}: Ausgangswert ist {vorher} statt {wert} — NICHT BELEGT",
            form.name()
        );
        return None;
    }
    let alt = m.highlight;
    let marker: u8 = if alt == 5 { 4 } else { 5 };
    println!(
        "  {} OHNE Shuttles (Farbe {alt} → Marker {marker})",
        form.name()
    );
    let angenommen = sende(schreibe(&m, form, marker, session, pw)).await;
    let ergebnis = match warte(pw, id, |_, m| m.highlight == marker).await {
        Some((s, m, ms)) if angenommen => {
            zeige(s, &m);
            println!(
                "    → Write nach {ms} ms sichtbar · Ballzahl {}",
                if s == wert {
                    "BLEIBT"
                } else {
                    "WEG / verändert"
                }
            );
            Some(s == wert)
        }
        _ => {
            println!("    → Write abgelehnt oder nie sichtbar — NICHT BELEGT");
            None
        }
    };
    println!("  zurück: Farbe {alt}, Ballzahl {wert}");
    let (_, m) = frisch(pw, id).await;
    sende(mit_shuttles(&schreibe(&m, form, alt, session, pw), wert)).await;
    if warte(pw, id, |s, m| s == wert && m.highlight == alt)
        .await
        .is_none()
    {
        println!("    ACHTUNG: Rückstellen nicht sichtbar — Farbe/Ballzahl in BTP prüfen.");
    }
    ergebnis
}

fn text(e: Option<bool>) -> &'static str {
    match e {
        Some(true) => "bleibt",
        Some(false) => "WEG",
        None => "nicht belegt",
    }
}

/// Zweig-Aussage aus den Folge-Writes: B, sobald einer die Zahl löscht;
/// A nur, wenn **alle** belegt halten.
fn zweig(folge: &[Option<bool>]) -> &'static str {
    if folge.contains(&Some(false)) {
        "Zweig B: alle Match-Writes müssen Shuttles mittragen"
    } else if folge.iter().all(|e| *e == Some(true)) {
        "spricht für Zweig A (nur der Ergebnis-Write trägt Shuttles)"
    } else {
        "keine Aussage — mindestens ein Write nicht belegt, Messung wiederholen"
    }
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP; liest nur"]
async fn does_btp_deliver_shuttles() {
    let pw = password();
    let pw = pw.as_deref();
    let (nodes, snap) = lese(pw).await;
    println!("\n=== Turnier: {} ===", snap.tournament_name);

    let mut alle = Vec::new();
    alle_matches(&nodes, &mut alle);
    let mit: Vec<&Node> = alle
        .iter()
        .copied()
        .filter(|m| kind(m, "Shuttles").is_some())
        .collect();
    println!(
        "{} Match-Knoten, davon {} mit einem Shuttles-Element",
        alle.len(),
        mit.len()
    );
    let nullen = mit
        .iter()
        .filter(|m| kind(m, "Shuttles").and_then(zahl) == Some(0))
        .count();
    println!(
        "davon mit Wert 0: {nullen} (0 → BTP lässt das Element bei 0 weg — \
         sofern überhaupt Werte da sind)"
    );
    for m in mit.iter().take(10) {
        println!(
            "  Match {:?}:{:?}: {:?}",
            kind(m, "DrawID").and_then(zahl),
            kind(m, "ID").and_then(zahl),
            kind(m, "Shuttles")
        );
    }
    // Eindeutigkeit der Match-ID: roh über alle Knoten und über die
    // geparsten Spiele (nur `IsMatch=true`).
    let doppelt = |ids: Vec<i64>| {
        let gesamt = ids.len();
        let verschieden = ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        gesamt - verschieden
    };
    println!(
        "Match-IDs doppelt: {} unter den rohen Knoten, {} unter den {} geparsten Spielen",
        doppelt(
            alle.iter()
                .filter_map(|m| kind(m, "ID").and_then(zahl))
                .collect()
        ),
        doppelt(snap.matches.iter().map(|m| m.id).collect()),
        snap.matches.len()
    );
    if mit.is_empty() {
        println!(
            "Kein Spiel trägt eine Ballzahl. In BTP an einem Spiel von Hand eine \
             eintragen und erneut laufen lassen — sonst ist offen, ob BTP das Feld \
             überhaupt ausliefert."
        );
    }

    println!(
        "\nSpiele auf einem Feld (Kandidaten für BTP_SHUTTLES_MATCH, Form <DrawID>:<Match-ID>):"
    );
    let laufend: Vec<&BtpMatch> = snap
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::OnCourt && m.court_id.is_some() && m.winner.is_none())
        .collect();
    if laufend.is_empty() {
        println!("  keins — für Messung 3 in BTP ein Spiel auf ein Feld setzen");
    }
    for m in laufend {
        println!(
            "  {} · Feld-ID {:?} · {} {}",
            Ziel::von(m),
            m.court_id,
            m.class_label,
            m.round_name
        );
    }
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT probeweise und stellt zurück"]
async fn does_a_waiting_match_keep_shuttles() {
    let pw = password();
    let pw = pw.as_deref();
    let (nodes, snap) = lese(pw).await;
    println!("\n=== Turnier: {} ===", snap.tournament_name);
    // Vom Ende der Liste, damit Rotation und Aufruf-Automatik das Spiel
    // nicht gerade anfassen; ohne Ballzahl, damit der Restore eindeutig ist.
    let id = snap
        .matches
        .iter()
        .rev()
        .find(|m| {
            m.status == MatchStatus::Scheduled
                && m.court_id.is_none()
                && m.winner.is_none()
                && !m.team1.is_empty()
                && !m.team2.is_empty()
                && shuttles(&nodes, Ziel::von(m)) == 0
        })
        .map(Ziel::von)
        .expect("wartendes Spiel ohne Feld und ohne Ballzahl");
    println!("Ziel: wartendes Match {id}");
    let session = login(pw).await;

    println!("\n1) Match-Write MIT Shuttles=3");
    let latenz = setze_shuttles(pw, id, 3, &session).await;
    let folge = match latenz {
        Some(ms) => {
            println!("    sichtbar nach {ms} ms");
            println!("\n2) Folge-Writes am wartenden Spiel");
            vec![
                folge_write(pw, id, Form::Farbe, 3, &session).await,
                folge_write(pw, id, Form::Besetzung, 3, &session).await,
            ]
        }
        None => {
            println!(
                "    Shuttles=3 ist nach dem Write nicht lesbar. Entweder nimmt BTP das \
                 Feld so nicht an, oder es liefert es nicht aus — Messung 1 mit einem \
                 Handwert grenzt das ein."
            );
            Vec::new()
        }
    };

    println!("\nRestore: Shuttles=0");
    let zurueck = setze_shuttles(pw, id, 0, &session).await.is_some();

    println!("\n=== Zusammenfassung (wartendes Spiel) ===");
    println!(
        "Shuttles im Match-Write gesetzt und gelesen: {}",
        latenz.is_some()
    );
    if let [farbe, besetzung] = folge[..] {
        println!("nach Farb-Write ohne Shuttles:           {}", text(farbe));
        println!(
            "nach Schiedsrichter-Write ohne Shuttles: {}",
            text(besetzung)
        );
        println!("→ {}", zweig(&folge));
    }
    if !zurueck {
        println!("ACHTUNG: Restore nicht bestätigt — Ballzahl an Match {id} in BTP prüfen.");
    }
}

#[tokio::test]
#[ignore = "braucht ein laufendes BTP; SCHREIBT EIN ERGEBNIS, nicht umkehrbar — nur Testturnier"]
async fn does_a_result_write_carry_shuttles() {
    let Some(id) = std::env::var("BTP_SHUTTLES_MATCH")
        .ok()
        .and_then(|v| Ziel::aus_text(&v))
    else {
        println!(
            "\nBTP_SHUTTLES_MATCH fehlt oder hat nicht die Form <DrawID>:<Match-ID> — nichts geschrieben. Die Messung \
             wertet das genannte Spiel und ist nicht umkehrbar; den Wert aus \
             `does_btp_deliver_shuttles` nehmen."
        );
        return;
    };
    let pw = password();
    let pw = pw.as_deref();
    let (_, snap) = lese(pw).await;
    println!("\n=== Turnier: {} ===", snap.tournament_name);
    let m = snap
        .matches
        .iter()
        .find(|m| Ziel::von(m) == id)
        .cloned()
        .expect("BTP_SHUTTLES_MATCH nennt ein Match dieses Turniers");
    assert!(
        m.status == MatchStatus::OnCourt && m.court_id.is_some() && m.winner.is_none(),
        "Match {id} muss auf einem Feld stehen und darf kein Ergebnis haben \
         (gelesen: Status {:?}, Feld {:?}, Sieger {:?}, {} {})",
        m.status,
        m.court_id,
        m.winner,
        m.class_label,
        m.round_name
    );
    println!("Ziel: laufendes Match {id} auf Feld-ID {:?}", m.court_id);
    let session = login(pw).await;

    println!("\n1) laufendes Spiel: Match-Write MIT Shuttles=2");
    let laufend_gesetzt = setze_shuttles(pw, id, 2, &session).await.is_some();
    let laufend = if laufend_gesetzt {
        println!("\n2) Folge-Writes am laufenden Spiel");
        vec![
            folge_write(pw, id, Form::Farbe, 2, &session).await,
            folge_write(pw, id, Form::Besetzung, 2, &session).await,
        ]
    } else {
        println!("\n2) übersprungen — Shuttles am laufenden Spiel nach dem Write nicht lesbar");
        Vec::new()
    };

    // Ergebnis in der Form von `process_result`, aus dem **frischen** Stand:
    // Feldfreigabe im selben Request, Besetzung reasserted, sobald das Match
    // eine trägt.
    let (_, m) = frisch(pw, id).await;
    assert!(
        m.winner.is_none() && m.court_id.is_some(),
        "Match {id} hat inzwischen ein Ergebnis oder kein Feld mehr — Abbruch vor dem Ergebnis-Write"
    );
    let ergebnis = |m: &BtpMatch, free_court_id: Option<i64>| proto::MatchUpdate {
        btp_match_id: m.id,
        draw_id: m.draw_id,
        planning_id: m.planning_id,
        sets: vec![(21, 15), (21, 10)],
        team1_won: true,
        duration_mins: 20,
        score_status: 0,
        free_court_id,
        player_ids: Vec::new(),
        end_ts_ms: None,
        officials: (m.official1_id.is_some() || m.official2_id.is_some())
            .then(|| (m.official1_id.unwrap_or(0), m.official2_id.unwrap_or(0))),
        shuttles: 0,
    };

    println!("\n3) Ergebnis-Write MIT Shuttles=5 (21:15 21:10, Feld wird frei)");
    let angenommen = sende(mit_shuttles(
        &proto::update_request(&ergebnis(&m, m.court_id), &session, pw),
        5,
    ))
    .await;
    let gewertet = warte(pw, id, |_, m| m.winner.is_some()).await;
    let im_ergebnis = match &gewertet {
        Some((s, m, ms)) if angenommen => {
            zeige(*s, m);
            println!("    Wertung nach {ms} ms sichtbar");
            Some(*s == 5)
        }
        _ => {
            println!("    → Ergebnis abgelehnt oder nie sichtbar — NICHT BELEGT");
            None
        }
    };

    // Ab hier ist das Spiel gewertet. Die folgenden Writes an ein gewertetes
    // Spiel sind in BTP selbst nicht vermessen: Ein Match-Knoten ohne
    // Ergebnisfelder kann die Wertung berühren — deshalb zeigt jeder Stand
    // auch den Sieger.
    let beendet = if im_ergebnis == Some(true) {
        println!("\n4) Folge-Writes am GEWERTETEN Spiel (Sieger im Stand beachten)");
        vec![
            folge_write(pw, id, Form::Farbe, 5, &session).await,
            folge_write(pw, id, Form::Besetzung, 5, &session).await,
        ]
    } else {
        println!("\n4) übersprungen — keine Ballzahl am gewerteten Spiel");
        Vec::new()
    };

    // Zweiter Ergebnis-Write, wie ihn die Ergebnis-Korrektur sendet — ohne
    // Feldfreigabe, weil das Feld inzwischen ein anderes Spiel tragen kann.
    // `CourtID` fehlt dadurch im Knoten; das Feld am beendeten Spiel kann
    // dabei auf 0 fallen. Wie BTP eine zweite Wertung behandelt, ist ebenfalls
    // nicht abschließend vermessen (siehe `btp_overwrite_experiment.rs`).
    let rewrite = if frisch(pw, id).await.0 == 5 {
        println!("\n5) dasselbe Ergebnis noch einmal OHNE Shuttles");
        let (_, m) = frisch(pw, id).await;
        let ok = sende(proto::update_request(&ergebnis(&m, None), &session, pw)).await;
        // Kein sichtbarer Marker möglich (gleiches Ergebnis) — fest warten.
        tokio::time::sleep(Duration::from_millis(3000)).await;
        let (s, m) = frisch(pw, id).await;
        zeige(s, &m);
        ok.then_some(s == 5)
    } else {
        println!("\n5) übersprungen — Ballzahl steht nicht mehr auf 5");
        None
    };

    let (ende, m) = frisch(pw, id).await;
    println!("\n=== Zusammenfassung (laufendes Spiel + Ergebnis) ===");
    println!("Shuttles am laufenden Spiel gesetzt und gelesen: {laufend_gesetzt}");
    if let [farbe, besetzung] = laufend[..] {
        println!(
            "laufend, nach Farb-Write ohne Shuttles:           {}",
            text(farbe)
        );
        println!(
            "laufend, nach Schiedsrichter-Write ohne Shuttles: {}",
            text(besetzung)
        );
    }
    println!(
        "Shuttles im Ergebnis-Write:                       {}",
        match im_ergebnis {
            Some(true) => "angenommen",
            Some(false) => "NICHT übernommen",
            None => "nicht belegt",
        }
    );
    if let [farbe, besetzung] = beendet[..] {
        println!(
            "gewertet, nach Farb-Write ohne Shuttles:          {}",
            text(farbe)
        );
        println!(
            "gewertet, nach Schiedsrichter-Write ohne Shuttles: {}",
            text(besetzung)
        );
    }
    println!(
        "nach zweitem Ergebnis-Write ohne Shuttles:        {} (ohne Marker gemessen, 3 s Wartezeit)",
        text(rewrite)
    );
    let alle: Vec<Option<bool>> = laufend
        .iter()
        .chain(&beendet)
        .copied()
        .chain([rewrite])
        .collect();
    println!("→ {}", zweig(&alle));
    println!(
        "Nicht zurückgestellt: Match {id} ist gewertet (Sieger {:?}, Ballzahl {ende}, Feld {:?}).",
        m.winner, m.court_id
    );
}

/// Läuft ohne BTP (auch in der CI): Die Messung taugt nur, wenn `Shuttles`
/// wirklich im Match-Knoten der echten Requests landet und der Rest des
/// Requests unverändert bleibt.
#[test]
fn shuttles_lands_in_the_match_node_of_real_requests() {
    const ZIEL: Ziel = Ziel { draw: 2, id: 7 };
    let update = proto::MatchUpdate {
        btp_match_id: 7,
        draw_id: 2,
        planning_id: 3,
        sets: vec![(21, 15), (21, 10)],
        team1_won: true,
        duration_mins: 20,
        score_status: 0,
        free_court_id: Some(4),
        player_ids: Vec::new(),
        end_ts_ms: None,
        officials: Some((11, 12)),
        shuttles: 0,
    };
    let farbe = proto::highlight_request(
        &[proto::HighlightEntry {
            match_id: 7,
            draw_id: 2,
            planning_id: 3,
            highlight: 0,
            shuttles: 0,
            court_id: 0,
            officials: (0, 0),
        }],
        "sitzung",
        None,
    );
    let besetzung = proto::court_assign_request(
        &[],
        &[proto::MatchCourt {
            match_id: 7,
            draw_id: 2,
            planning_id: 3,
            court_id: 0,
            officials: Some((0, 0)),
            highlight: Some(0),
            shuttles: 0,
        }],
        "sitzung",
        None,
    );
    for original in [
        proto::update_request(&update, "sitzung", None),
        farbe,
        besetzung,
    ] {
        let vorher = proto::decode_response(&original).expect("dekodierbar");
        // Dekodieren und neu kodieren verändert kein Byte.
        assert_eq!(wire::encode_message(&xml::encode(&vorher)), original);
        assert_eq!(
            shuttles_knoten(&vorher, ZIEL),
            None,
            "Original ohne Shuttles"
        );

        let nachher = proto::decode_response(&mit_shuttles(&original, 5)).expect("dekodierbar");
        assert_eq!(shuttles(&nachher, ZIEL), 5);

        // Bis auf das angehängte Element ist der Request derselbe.
        let mut erwartet = vorher;
        haenge_shuttles_an(&mut erwartet, 5);
        assert_eq!(nachher, erwartet);
        let mut treffer = Vec::new();
        alle_matches(&nachher, &mut treffer);
        assert_eq!(treffer.len(), 1, "genau ein Match-Knoten");
    }
}
