//! BTP als Turnierquelle: die Schreibwege nach BTP (ADR 0060 Schreib-Gate,
//! `eigene_writes`, frisches Lesen vor dem Write).
//!
//! Wortgleich aus `tablet/server.rs` hierher verschoben (Paket L1 der
//! badhub-Spec `docs/features/bts_light_steuerung.md`): BTP wird eine von zwei
//! Quellen. Am Verhalten ändert sich nichts; `tablet::server` exportiert die
//! Funktionen unter den alten Namen weiter, damit die Aufrufer unberührt
//! bleiben.

use crate::btp::{client, proto};
use crate::config::AppConfig;
use crate::tablet::state::TabletState;

/// LOGIN → Session-Schlüssel → `SENDUPDATE`. Schreibt ein einzelnes
/// Match-Ergebnis nach BTP – auch für kampflose Wertungen (Walkover)
/// aus der Turnierleitung wiederverwendet.
pub(crate) async fn write_result_to_btp(
    config: &AppConfig,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    // Schreib-Gate (ADR 0060): solange der TP-Upload die Datei liest, wartet
    // dieser Write — und umgekehrt wartet der Upload auf uns. Guard bis zum Ende.
    let _schreibt = crate::btp::write_gate::GATE.schreiben().await;
    let _seriell = crate::btp::eigene_writes::SERIELL.lock().await; // Match-Writes nacheinander
                                                                    // Das Ergebnis setzt Feld und Besetzung neu, ohne dass wir sie kennen
                                                                    // (`free_court_id: None`) — ab jetzt gilt BTP, nicht der Merkstand.
    crate::btp::eigene_writes::EIGENE.loeschen(update.btp_match_id);
    // Farb- und Schiedsrichter-Writes an dieses Spiel ab jetzt zurückhalten:
    // Sie nähmen ihm die Wertung (`ist_gewertet`). Der Merker bleibt auch
    // nach einem Fehler stehen: Ein Write, der in eine Zeitüberschreitung
    // lief, kann trotzdem angekommen sein — 5 s Zurückhaltung kosten nichts,
    // der nächste Sync-Lauf holt Farbe und Besetzung nach.
    crate::btp::eigene_writes::EIGENE
        .wertung_merken(update.btp_match_id, crate::tablet::monitor::now_ms());
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let login_raw = client::send_request(host, port, &proto::login_request(pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    let session = proto::parse_login_response(
        &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let upd_raw = client::send_request(host, port, &proto::update_request(update, &session, pw))
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
    proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Schreibt eine Wertung nach BTP und führt die Buchführung, die **jedes
/// Mal** dieselbe ist: den Nachschub-Eintrag dieses Spiels löschen und den
/// Schreibvorgang für die Selbstheilung vermerken.
///
/// Der Zeitstempel wird bewusst **nach** dem Schreiben genommen: Der
/// Nachschub-Lauf vergleicht ihn gegen seinen eigenen Startzeitpunkt, um zu
/// erkennen, ob ihn eine neuere Wertung überholt hat. Ein vor dem Schreiben
/// abgelesener Wert (BTP-Anmeldung + Aktualisierung dauern) kann älter sein
/// als dieser Start — dann bliebe die frische Wertung überschrieben.
/// Genau diese Falle ist der Grund, warum es die Funktion gibt und der
/// Ablauf nicht mehr an vier Stellen abgeschrieben steht.
pub(crate) async fn write_result_settled(
    config: &AppConfig,
    tablet: &TabletState,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    write_result_to_btp(config, update).await?;
    tablet.clear_btp_retry(update.btp_match_id);
    tablet.note_direct_btp_write(update.clone(), crate::tablet::monitor::now_ms());
    Ok(())
}

/// LOGIN → Session-Schlüssel → `SENDUPDATE` mit Courts-Block. Schreibt
/// **Feld-Zuweisungen** nach BTP (Match auf Feld setzen / Feld freigeben) –
/// nach dem Vorbild des Original-BTS. Bidirektional: das, was hier geschrieben
/// wird, liest bts-light beim nächsten Poll als OnCourt zurück.
pub(crate) async fn write_courts_to_btp(
    config: &AppConfig,
    courts: &[proto::CourtAssignment],
    match_courts: &[proto::MatchCourt],
) -> Result<(), String> {
    if courts.is_empty() && match_courts.is_empty() {
        return Ok(());
    }
    let _schreibt = crate::btp::write_gate::GATE.schreiben().await; // ADR 0060
    let _seriell = crate::btp::eigene_writes::SERIELL.lock().await; // Match-Writes nacheinander
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    // Ein Spiel, an das gerade ein Ergebnis ging, bekommt keinen
    // Match-Knoten: BTP zeigt die Wertung womöglich noch nicht, und der
    // Knoten nähme sie wieder weg ([`ist_gewertet`]). Der Courts-Block
    // bleibt — das Feld wird trotzdem geräumt bzw. vergeben.
    let jetzt = crate::tablet::monitor::now_ms();
    let match_courts: Vec<proto::MatchCourt> = match_courts
        .iter()
        .filter(|m| !crate::btp::eigene_writes::EIGENE.kuerzlich_gewertet(m.match_id, jetzt))
        .copied()
        .collect();
    let match_courts = match_courts.as_slice();
    if courts.is_empty() && match_courts.is_empty() {
        return Ok(());
    }
    let gemerkt = merke_eigene(match_courts.iter().map(|m| (m.match_id, m.planung())));
    let ergebnis = async {
        let login_raw = client::send_request(host, port, &proto::login_request(pw))
            .await
            .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
        let session = proto::parse_login_response(
            &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let upd_raw = client::send_request(
            host,
            port,
            &proto::court_assign_request(courts, match_courts, &session, pw),
        )
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
        proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    .await;
    if ergebnis.is_err() {
        vergiss_eigene(&gemerkt);
    }
    ergebnis
}

/// Merkt vor dem Senden, welche vier Planungsfelder jeder Match-Knoten in
/// BTP setzt (`btp::eigene_writes`) — BTP zeigt sie erst nach bis zu 840 ms,
/// ein folgender Write liest sie bis dahin noch nicht. Liefert, was gemerkt
/// wurde, damit ein gescheiterter Write es zurücknehmen kann.
fn merke_eigene(
    knoten: impl Iterator<Item = (i64, crate::btp::eigene_writes::Planung)>,
) -> Vec<(i64, crate::btp::eigene_writes::Planung, u64)> {
    let jetzt = crate::tablet::monitor::now_ms();
    knoten
        .map(|(id, p)| {
            crate::btp::eigene_writes::EIGENE.merken(id, p, jetzt);
            (id, p, jetzt)
        })
        .collect()
}

/// Gegenstück zu [`merke_eigene`] für einen gescheiterten Write.
fn vergiss_eigene(gemerkt: &[(i64, crate::btp::eigene_writes::Planung, u64)]) {
    for (id, p, seit) in gemerkt {
        crate::btp::eigene_writes::EIGENE.vergessen(*id, *p, *seit);
    }
}

/// Verwirft jeden Merkstand, den BTP im frisch gelesenen Stand schon zeigt
/// (`EigeneWrites::abgleichen`): Ab da gilt wieder BTP.
fn abgleichen_mit(frisch: &crate::btp::model::BtpSnapshot) {
    for m in &frisch.matches {
        crate::btp::eigene_writes::EIGENE.abgleichen(
            m.id,
            crate::btp::eigene_writes::Planung {
                court_id: m.court_id.unwrap_or(0),
                officials: (m.official1_id.unwrap_or(0), m.official2_id.unwrap_or(0)),
                highlight: m.highlight,
                shuttles: m.shuttles,
            },
        );
    }
}

/// Ist das Spiel gewertet — laut frisch gelesenem Stand oder durch einen
/// eigenen Ergebnis-Write der letzten Sekunden, den BTP noch nicht zeigt?
///
/// Ein Match-Write ohne Ergebnis-Felder nimmt einem gewerteten Spiel in BTP
/// den Sieger (Messung 13.11.2026, `tests/btp_shuttles_probe.rs`: Farb- und
/// Schiedsrichter-Write). Farbe und Besetzung eines beendeten Spiels sind
/// das nicht wert — solche Einträge gehen deshalb gar nicht erst raus.
pub(crate) fn ist_gewertet(frisch: &crate::btp::model::BtpSnapshot, match_id: i64) -> bool {
    frisch
        .matches
        .iter()
        .any(|m| m.id == match_id && m.winner.is_some())
        || crate::btp::eigene_writes::EIGENE
            .kuerzlich_gewertet(match_id, crate::tablet::monitor::now_ms())
}

/// Der eigene Merkstand eines Spiels — Vorrang vor dem frisch gelesenen.
fn eigener_stand(match_id: i64) -> Option<crate::btp::eigene_writes::Planung> {
    crate::btp::eigene_writes::EIGENE.aktuell(match_id, crate::tablet::monitor::now_ms())
}

/// Schreibt **nur** die Schiedsrichter-Besetzung nach BTP (ADR 0021),
/// Muster [`write_courts_to_btp`]: eigene Sitzung, ein `SENDUPDATE`, Antwort
/// geprüft. Der Aufrufer übernimmt den Stand erst bei `Ok` — ein Fehlschlag
/// wird im nächsten Sync-Zyklus wiederholt.
///
/// Läuft über [`proto::court_assign_request`] mit leerem `courts`-Block —
/// jeder Eintrag trägt seine aktuelle `CourtID` mit (siehe
/// [`proto::MatchCourt::court_id`]), damit dieser eigenständige Write nie
/// eine gerade erst angekommene Feldzuweisung überschreiben kann.
///
/// „Aktuell" heißt: **unmittelbar vorher frisch gelesen**. BTP setzt bei
/// jedem Schiedsrichter-Write die CourtID aus dem Write (Messung
/// 09.11.2026) — eine CourtID aus dem Snapshot des Zyklus löschte ein Feld,
/// das jemand inzwischen in BTP von Hand vergeben hat. Scheitert das Lesen,
/// geht nichts raus (Wiederholung im nächsten Zyklus). Liefert die
/// tatsächlich gesendeten Einträge; Spiele, die BTP nicht mehr kennt,
/// fehlen darin — ebenso gewertete Spiele ([`ist_gewertet`]).
pub(crate) async fn write_officials_to_btp(
    config: &AppConfig,
    entries: &[proto::MatchCourt],
    zyklus: &crate::btp::model::BtpSnapshot,
    tablet: &TabletState,
) -> Result<Vec<proto::MatchCourt>, String> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let _schreibt = crate::btp::write_gate::GATE.schreiben().await; // ADR 0060
    let _seriell = crate::btp::eigene_writes::SERIELL.lock().await; // Match-Writes nacheinander
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let frisch = client::fetch_snapshot(host, port, pw)
        .await
        .map_err(|e| format!("BTP-Stand vor dem Schiedsrichter-Write nicht lesbar: {e}"))?;
    abgleichen_mit(&frisch);
    // Reservierungen erst NACH dem Lesen nehmen: Eine Vergabe, die während
    // des Lesens startete, ist dann sicher dabei.
    let reserviert: std::collections::HashSet<i64> = tablet
        .reserved_courts(crate::tablet::monitor::now_ms())
        .into_iter()
        .map(|(_, match_id)| match_id)
        .collect();
    let vorher = entries.len();
    let mut entries = crate::sync::feld_aus_frischem_stand(
        entries,
        &zyklus.matches,
        &frisch.matches,
        &reserviert,
        &eigener_stand,
    );
    entries.retain(|e| !ist_gewertet(&frisch, e.match_id));
    if entries.len() < vorher {
        tracing::debug!(
            "BTP-Schiedsrichter: {} von {vorher} Einträgen nach frischem Lesen zurückgestellt              (Feld gerade in Vergabe, in BTP umgeteilt, Spiel gewertet oder weg)",
            vorher - entries.len()
        );
    }
    if entries.is_empty() {
        return Ok(entries);
    }

    let gemerkt = merke_eigene(entries.iter().map(|m| (m.match_id, m.planung())));
    let ergebnis = async {
        let login_raw = client::send_request(host, port, &proto::login_request(pw))
            .await
            .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
        let session = proto::parse_login_response(
            &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let upd_raw = client::send_request(
            host,
            port,
            &proto::court_assign_request(&[], &entries, &session, pw),
        )
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
        proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    .await;
    if let Err(e) = ergebnis {
        vergiss_eigene(&gemerkt);
        return Err(e);
    }
    Ok(entries)
}

/// Schreibt `Match.Highlight` nach BTP — die Aufrufmarke (P1) und die
/// Zeilenfarbe der Turnierleitungs-Seite. Eigene Sitzung, ein `SENDUPDATE`
/// (`proto::highlight_request`, kein `Status`/Ergebnis). Best-effort: Aufrufer
/// fangen den Fehler ab — der interne Aufruf-Zustand bleibt davon unberührt.
///
/// BTP übernimmt `CourtID` und `Official1ID/2ID` aus jedem Match-Write, ein
/// fehlendes Element löscht sie (Messung 09.11.2026). Der Write trägt beide
/// deshalb mit dem **unmittelbar vorher frisch gelesenen** Wert (Muster
/// [`write_officials_to_btp`]); ein Wert aus dem Zyklus-Snapshot nähme ein
/// inzwischen vergebenes Feld wieder weg. Ein eigener Write desselben
/// Spiels, den BTP noch nicht zeigt, geht dem frischen Stand vor
/// (`btp::eigene_writes`). Scheitert das Lesen, geht nichts raus. Liefert die
/// Zahl der gesendeten Einträge — Spiele, die BTP nicht mehr kennt, und
/// gewertete Spiele ([`ist_gewertet`]) fehlen.
pub(crate) async fn write_highlight_to_btp(
    config: &AppConfig,
    entries: &[proto::HighlightEntry],
) -> Result<usize, String> {
    if entries.is_empty() {
        return Ok(0);
    }
    let _schreibt = crate::btp::write_gate::GATE.schreiben().await; // ADR 0060
    let _seriell = crate::btp::eigene_writes::SERIELL.lock().await; // Match-Writes nacheinander
    let host = &config.btp.host;
    let port = config.btp.port;
    let pw = config.btp.password.as_deref();

    let frisch = client::fetch_snapshot(host, port, pw)
        .await
        .map_err(|e| format!("BTP-Stand vor dem Farb-Write nicht lesbar: {e}"))?;
    abgleichen_mit(&frisch);
    let mut entries =
        crate::sync::planung_aus_frischem_stand(entries, &frisch.matches, &eigener_stand);
    entries.retain(|e| !ist_gewertet(&frisch, e.match_id));
    if entries.is_empty() {
        return Ok(0);
    }

    let gemerkt = merke_eigene(entries.iter().map(|h| (h.match_id, h.planung())));
    let ergebnis = async {
        let login_raw = client::send_request(host, port, &proto::login_request(pw))
            .await
            .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
        let session = proto::parse_login_response(
            &proto::decode_response(&login_raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let upd_raw = client::send_request(
            host,
            port,
            &proto::highlight_request(&entries, &session, pw),
        )
        .await
        .map_err(|e| format!("BTP nicht erreichbar: {e}"))?;
        proto::parse_update_response(&proto::decode_response(&upd_raw).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    .await;
    if let Err(e) = ergebnis {
        vergiss_eigene(&gemerkt);
        return Err(e);
    }
    Ok(entries.len())
}
