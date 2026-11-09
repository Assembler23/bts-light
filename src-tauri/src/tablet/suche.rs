//! Spielersuche der Turnierleitung (Spec `tl-spielersuche`, ADR 0081).
//!
//! Sucht über den **ganzen** BTP-Bestand — nicht über den gedeckelten
//! TL-Zustand — nach Spielern und (bei eingeschaltetem
//! Schiedsrichter-Betrieb) nach SR/AR: am Feld, in der kompletten Warteliste
//! und in allen beendeten Spielen. LAN-Route und Relay-Client rufen
//! denselben Einstieg [`search_json`] (R3).
//!
//! Lesend und ohne Seiteneffekt: Die Prognose wird mit derselben Rechnung
//! wie im TL-Zustand ermittelt, aber nicht gemerkt (kein
//! `merge_predicted_starts`).
//!
//! **Datenschutz:** Die Antwort ist eine Positivliste (siehe die Structs
//! unten und die Wächter-Tests). Der Suchbegriff wird nie protokolliert —
//! Host-Logs gehen per Diagnose-Upload an badhub.

use serde::Serialize;

use crate::btp::model::{BtpMatch, BtpPlayer, BtpSnapshot, MatchResult, MatchStatus};
use crate::config::AppConfig;
use crate::tablet::state::TabletState;
use crate::tablet::tl;
use relay_proto::{
    MAX_TL_SEARCH_LEN, TL_SEARCH_HITS_PER_GROUP, TL_SEARCH_MAX_LEN, TL_SEARCH_MAX_WORDS,
    TL_SEARCH_MIN_LEN,
};

/// Warum ein Suchbegriff abgewiesen wurde — die Seite verhindert das vorab,
/// Host und Relay prüfen trotzdem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuchFehler {
    /// Weniger als [`TL_SEARCH_MIN_LEN`] Zeichen ohne Leerraum.
    Kurz,
    /// Mehr als [`TL_SEARCH_MAX_LEN`] Zeichen.
    Lang,
    /// Mehr als [`TL_SEARCH_MAX_WORDS`] Wörter.
    VieleWoerter,
    /// Schon [`MAX_GLEICHZEITIG`] Suchen in Arbeit — die Anfrage wird
    /// abgewiesen statt eingereiht (Security-Review 09.11.2026).
    Belegt,
}

impl SuchFehler {
    /// Fehler-Antwort an den Relay: `{"error":"<code>"}` — der Relay macht
    /// daraus 400 bzw. 429 statt eines irreführenden 503 (Code-Review
    /// 09.11.2026: „ue" faltet zu einem Zeichen, das prüft nur der Host).
    pub(crate) fn relay_json(self) -> String {
        let code = match self {
            SuchFehler::Belegt => relay_proto::TL_SEARCH_ERR_BUSY,
            _ => relay_proto::TL_SEARCH_ERR_INVALID,
        };
        serde_json::json!({ "error": code }).to_string()
    }

    /// Klartext für die 400-Antwort.
    pub(crate) fn text(self) -> &'static str {
        match self {
            SuchFehler::Kurz => "Bitte mindestens 2 Zeichen eingeben.",
            SuchFehler::Lang => "Bitte kürzer suchen.",
            SuchFehler::VieleWoerter => "Bitte kürzer suchen.",
            SuchFehler::Belegt => "Zu viele Suchen gleichzeitig — bitte kurz warten.",
        }
    }
}

/// Prüft die Länge eines Suchbegriffs, **bevor** er gefaltet wird — der
/// Relay nutzt das, um einen zu langen Begriff gar nicht erst zum Host zu
/// schicken.
pub(crate) fn laenge_pruefen(q: &str) -> Result<(), SuchFehler> {
    if q.chars().count() > TL_SEARCH_MAX_LEN {
        return Err(SuchFehler::Lang);
    }
    Ok(())
}

/// Faltet einen Namen oder Suchbegriff auf eine vergleichbare Form:
/// Kleinschreibung, `ß→ss`, Umlaute und gängige Akzente auf den
/// Grundbuchstaben, danach `ae/oe/ue → a/o/u` (Müller = Mueller = Muller),
/// Trennzeichen (`- , ' . /`) als Leerzeichen, Leerraum zusammengefasst.
///
/// Eigene Tabelle statt `unicode-normalization` — keine neue Abhängigkeit
/// für eine Handvoll Buchstaben (globale Regel 5).
pub(crate) fn falte(s: &str) -> String {
    let mut roh = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ß' => roh.push_str("ss"),
            'æ' => roh.push('a'),
            'œ' => roh.push('o'),
            c if c.is_whitespace() || matches!(c, '-' | ',' | '\'' | '.' | '/' | '’') => {
                roh.push(' ')
            }
            c => roh.push(grundbuchstabe(c)),
        }
    }
    // `ae/oe/ue → a/o/u`: Ein `e` direkt nach a/o/u fällt weg. Nur das
    // erste — „aee" bleibt „ae", damit nicht ganze Silben verschwinden.
    let mut aus = String::with_capacity(roh.len());
    let mut e_faellt = false;
    let mut letztes_leer = true;
    for c in roh.chars() {
        if c == ' ' {
            if !letztes_leer {
                aus.push(' ');
            }
            letztes_leer = true;
            e_faellt = false;
            continue;
        }
        letztes_leer = false;
        if c == 'e' && e_faellt {
            e_faellt = false;
            continue;
        }
        e_faellt = matches!(c, 'a' | 'o' | 'u');
        aus.push(c);
    }
    if aus.ends_with(' ') {
        aus.pop();
    }
    aus
}

/// Ein Buchstabe ohne Akzent — Tabelle der in Meldelisten üblichen Zeichen.
fn grundbuchstabe(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ą' | 'ă' | 'ā' => 'a',
        'ç' | 'č' | 'ć' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ę' | 'ě' | 'ē' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ı' | 'ī' => 'i',
        'ł' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ő' | 'ō' => 'o',
        'ř' => 'r',
        'š' | 'ś' | 'ş' | 'ș' => 's',
        'ť' | 'ț' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ů' | 'ű' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        'ž' | 'ź' | 'ż' => 'z',
        c => c,
    }
}

/// Zerlegt einen Suchbegriff in gefaltete Wörter und prüft die Grenzen.
pub(crate) fn woerter(q: &str) -> Result<Vec<String>, SuchFehler> {
    laenge_pruefen(q)?;
    let gefaltet = falte(q);
    let woerter: Vec<String> = gefaltet
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect();
    let zeichen: usize = woerter.iter().map(|w| w.chars().count()).sum();
    if zeichen < TL_SEARCH_MIN_LEN {
        return Err(SuchFehler::Kurz);
    }
    if woerter.len() > TL_SEARCH_MAX_WORDS {
        return Err(SuchFehler::VieleWoerter);
    }
    Ok(woerter)
}

/// Passt der Name **einer** Person? Alle Wörter müssen als Teilstring
/// vorkommen, Reihenfolge egal („max mül" findet „Max Müller", „Müller
/// Schmidt" findet kein Doppel Müller/Schmidt).
fn passt(name: &str, woerter: &[String]) -> bool {
    let n = falte(name);
    woerter.iter().all(|w| n.contains(w.as_str()))
}

fn passt_einer<'a>(namen: impl IntoIterator<Item = &'a str>, woerter: &[String]) -> bool {
    namen.into_iter().any(|n| passt(n, woerter))
}

// ── Antwortformat (Positivliste) ───────────────────────────────────────

/// Eine Gruppe der Trefferliste: höchstens `TL_SEARCH_HITS_PER_GROUP`
/// Treffer, der Rest nur gezählt.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Gruppe<T> {
    pub hits: Vec<T>,
    pub more: usize,
}

/// Was jeder Treffer trägt.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Kern {
    pub match_id: i64,
    pub match_num: Option<i64>,
    pub draw_name: String,
    pub round_name: String,
    pub class_label: String,
    pub discipline: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// Vereine — die Seite zeigt sie nur bei „Vereine anzeigen" (wie im
    /// TL-Zustand). Fehlt bei Kandidaten offener Paarungen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team1_club: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team2_club: Option<Vec<String>>,
    /// Lizenznummern als badhub-Link-Ziel (wie im TL-Zustand). Fehlt bei
    /// Kandidaten offener Paarungen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team1_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team2_ids: Option<Vec<String>>,
    pub hall: String,
    pub sr: Vec<String>,
    pub ar: Vec<String>,
    /// Rollen der gesuchten Person in diesem Spiel: `player`, `sr`, `ar`,
    /// `candidate`.
    pub roles: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CourtHit {
    #[serde(flatten)]
    pub kern: Kern,
    pub court_id: i64,
    pub court: String,
    pub on_court_since_ms: Option<u64>,
    pub sets: Vec<(i64, i64)>,
    pub clearing: bool,
    pub has_timeline: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WaitingHit {
    #[serde(flatten)]
    pub kern: Kern,
    /// 1-basiert: bei spielbereiten Spielen der Platz in der turnierweiten
    /// Warteliste, bei offenen Paarungen der Platz, vor dem sie stehen.
    pub position: usize,
    /// Nur auf Platz ≤ `QUEUE_LIMIT` und bei eingeschalteter Prognose.
    pub predicted_start_ms: Option<u64>,
    pub predicted_uncertain: bool,
    pub called: bool,
    pub excluded: bool,
    pub wish_court: Option<i64>,
    pub open: bool,
    pub open_slot1_label: String,
    pub open_slot2_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FinishedHit {
    #[serde(flatten)]
    pub kern: Kern,
    /// 1-basiert: Platz in der Reihenfolge des Beendet-Panels (neueste
    /// zuerst) — sagt der Seite, ob die Zeile dort überhaupt steht.
    pub position: usize,
    pub winner: u8,
    pub sets: Vec<(i64, i64)>,
    pub result: &'static str,
    pub court: String,
    /// Spielende nur, wenn die App das Spiel selbst laufen sah (gemessene
    /// Spielzeit) — BTP liefert kein Spielende, und der Stempel „zuerst als
    /// beendet gesehen" ist nach Neuinstallation oder am Folgetag falsch.
    pub ended_ms: Option<u64>,
    pub has_timeline: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TlSearch {
    pub server_now_ms: u64,
    pub has_data: bool,
    pub officials_managed: bool,
    pub court: Gruppe<CourtHit>,
    pub waiting: Gruppe<WaitingHit>,
    pub finished: Gruppe<FinishedHit>,
}

fn kappen<T>(alle: Vec<T>, grenze: usize) -> Gruppe<T> {
    let more = alle.len().saturating_sub(grenze);
    let mut hits = alle;
    hits.truncate(grenze);
    Gruppe { hits, more }
}

fn namen(spieler: &[BtpPlayer]) -> Vec<String> {
    spieler.iter().map(|p| p.name.clone()).collect()
}

fn vereine(spieler: &[BtpPlayer]) -> Vec<String> {
    spieler
        .iter()
        .map(|p| p.club.clone().unwrap_or_default())
        .collect()
}

/// Rollen über Spieler und SR/AR eines feststehenden Spiels.
fn rollen(m: &BtpMatch, sr: &[String], ar: &[String], woerter: &[String]) -> Vec<&'static str> {
    let mut r = Vec::new();
    if passt_einer(
        m.team1
            .iter()
            .chain(m.team2.iter())
            .map(|p| p.name.as_str()),
        woerter,
    ) {
        r.push("player");
    }
    if passt_einer(sr.iter().map(String::as_str), woerter) {
        r.push("sr");
    }
    if passt_einer(ar.iter().map(String::as_str), woerter) {
        r.push("ar");
    }
    r
}

fn kern(
    m: &BtpMatch,
    hall: String,
    sr: Vec<String>,
    ar: Vec<String>,
    roles: Vec<&'static str>,
    mit_person: bool,
) -> Kern {
    Kern {
        match_id: m.id,
        match_num: m.match_num,
        draw_name: m.draw_name.clone(),
        round_name: m.round_name.clone(),
        class_label: m.class_label.clone(),
        discipline: m.discipline.as_str().to_string(),
        team1: namen(&m.team1),
        team2: namen(&m.team2),
        team1_club: mit_person.then(|| vereine(&m.team1)),
        team2_club: mit_person.then(|| vereine(&m.team2)),
        team1_ids: mit_person.then(|| tl::license_ids(&m.team1)),
        team2_ids: mit_person.then(|| tl::license_ids(&m.team2)),
        hall,
        sr,
        ar,
        roles,
    }
}

fn result_text(r: MatchResult) -> &'static str {
    match r {
        MatchResult::Normal => "normal",
        MatchResult::Walkover => "walkover",
        MatchResult::Retired => "retired",
        MatchResult::Disqualified => "disqualified",
    }
}

/// Die Suche selbst — ohne Deckel, alle Treffer je Gruppe in Reihenfolge.
fn alle_treffer(
    tablet: &TabletState,
    config: &AppConfig,
    snap: &BtpSnapshot,
    woerter: &[String],
    candidates: bool,
    now_ms: u64,
) -> (Vec<CourtHit>, Vec<WaitingHit>, Vec<FinishedHit>) {
    let match_by_id: std::collections::HashMap<i64, &BtpMatch> =
        snap.matches.iter().map(|m| (m.id, m)).collect();

    // ── Am Feld: dieselben Kacheln wie im TL-Zustand.
    let mut courts = tl::felder_bauen(tablet, config, snap, &match_by_id);
    let mut am_feld: Vec<CourtHit> = Vec::new();
    for c in &courts {
        if c.match_id == 0 {
            continue;
        }
        let Some(m) = match_by_id.get(&c.match_id).copied() else {
            continue;
        };
        let roles = rollen(m, &c.sr, &c.ar, woerter);
        if roles.is_empty() {
            continue;
        }
        am_feld.push(CourtHit {
            kern: kern(
                m,
                c.location.clone(),
                c.sr.clone(),
                c.ar.clone(),
                roles,
                true,
            ),
            court_id: c.court_id,
            court: c.court.clone(),
            on_court_since_ms: c.on_court_since_ms,
            sets: c.sets.clone(),
            clearing: c.clearing.is_some(),
            has_timeline: c.has_timeline,
        });
    }

    // ── Warteliste: dieselbe turnierweite Ordnung wie das Panel.
    let calls = tablet.preparation_calls();
    let (ordered, offene) = tl::wartelisten_ordnung(tablet, config, snap, &calls);
    let mut spielbereit: Vec<(usize, WaitingHit)> = Vec::new();
    for (i, (_, m, hall, _)) in ordered.iter().enumerate() {
        let (sr, ar, _) = tablet.wartende_officials(m, snap);
        let roles = rollen(m, &sr, &ar, woerter);
        if roles.is_empty() {
            continue;
        }
        spielbereit.push((
            i,
            WaitingHit {
                kern: kern(m, hall.clone(), sr, ar, roles, true),
                position: i + 1,
                predicted_start_ms: None,
                predicted_uncertain: false,
                called: tl::aufruf_halle(&calls, snap, m.id).is_some(),
                excluded: tablet.auto_assign_excluded(m.id),
                wish_court: tablet.wish_court(m.id),
                open: false,
                open_slot1_label: String::new(),
                open_slot2_label: String::new(),
            },
        ));
    }
    // Prognose nur rechnen, wenn ein Treffer im Prognose-Fenster liegt —
    // dieselbe Rechnung und dasselbe Fenster wie der TL-Zustand
    // (kanonisch `QUEUE_LIMIT`), ohne sie zu merken.
    if config.prediction.enabled && spielbereit.iter().any(|(i, _)| *i < tl::QUEUE_LIMIT) {
        let availability = crate::tablet::assign::PlayerAvailability::from_snapshot(snap, config);
        let (_, prognose) = tl::prognose_rechnen(
            tablet,
            config,
            snap,
            &mut courts,
            &ordered,
            tl::QUEUE_LIMIT,
            &availability,
            &match_by_id,
            now_ms,
        );
        for (i, hit) in spielbereit.iter_mut() {
            if *i >= tl::QUEUE_LIMIT {
                continue;
            }
            if let Some(p) = prognose.get(&hit.kern.match_id) {
                hit.predicted_start_ms = Some(p.start_min * 60_000);
                hit.predicted_uncertain = p.uncertain;
            }
        }
    }
    let mut wartend: Vec<WaitingHit> = spielbereit.into_iter().map(|(_, h)| h).collect();

    // Offene Paarungen: die feststehende Seite zählt als Spieler, die
    // Teilnehmer des Vorspiels als „möglicher Teilnehmer" — Letztere nur,
    // wenn das Profil offene Paarungen zeigt.
    if candidates {
        for (queue_index, (_, m, hall, _)) in &offene {
            let (sr, ar, _) = tablet.wartende_officials(m, snap);
            let mut roles = rollen(m, &sr, &ar, woerter);
            let kandidat = [1u8, 2].into_iter().any(|seite| {
                let leer = if seite == 1 {
                    m.team1.is_empty()
                } else {
                    m.team2.is_empty()
                };
                leer && tl::vorspiel_fuer(snap, m, seite).is_some_and(|v| {
                    !v.team1.is_empty()
                        && !v.team2.is_empty()
                        && passt_einer(
                            v.team1
                                .iter()
                                .chain(v.team2.iter())
                                .map(|p| p.name.as_str()),
                            woerter,
                        )
                })
            });
            if kandidat {
                roles.push("candidate");
            }
            if roles.is_empty() {
                continue;
            }
            wartend.push(WaitingHit {
                // Ohne Lizenznummer und Verein: Die Kandidatennamen stehen
                // nur im Label, und auch die feststehende Seite bleibt hier
                // schlank wie im TL-Zustand (`TlOpenMatch`).
                kern: kern(m, hall.clone(), sr, ar, roles, false),
                position: queue_index + 1,
                predicted_start_ms: None,
                predicted_uncertain: false,
                called: false,
                excluded: tablet.auto_assign_excluded(m.id),
                wish_court: tablet.wish_court(m.id),
                open: true,
                open_slot1_label: if m.team1.is_empty() {
                    tl::offener_platz_text(snap, m, 1)
                } else {
                    String::new()
                },
                open_slot2_label: if m.team2.is_empty() {
                    tl::offener_platz_text(snap, m, 2)
                } else {
                    String::new()
                },
            });
        }
    }

    // ── Beendet: alle Spiele mit Sieger. Position = Platz im Panel
    //    (neueste zuerst, wie `build_state_limited`).
    let mut beendet: Vec<&BtpMatch> = snap
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::Finished && m.winner.is_some())
        .collect();
    beendet.sort_by(|a, b| {
        b.finished_at
            .unwrap_or(0)
            .cmp(&a.finished_at.unwrap_or(0))
            .then(b.match_num.unwrap_or(0).cmp(&a.match_num.unwrap_or(0)))
            .then(b.id.cmp(&a.id))
    });
    let mut fertig: Vec<FinishedHit> = Vec::new();
    for (i, m) in beendet.iter().enumerate() {
        // Ein Walkover ist kein Einsatz (Regel aus
        // `OfficialsStore::appearances`): kein SR, kein SR-Treffer.
        let (sr, ar) = if m.result == MatchResult::Walkover {
            (Vec::new(), Vec::new())
        } else {
            let (sr, ar, _) = tablet.court_officials(Some(m), snap);
            (sr, ar)
        };
        let roles = rollen(m, &sr, &ar, woerter);
        if roles.is_empty() {
            continue;
        }
        let ended_ms = tl::finished_times(tablet, m.id)
            .and_then(|_| tablet.match_times_store().entry(m.id))
            .and_then(|e| e.finished_ms);
        let hall = m
            .court_id
            .map(|cid| snap.court_location_name(cid))
            .unwrap_or_default();
        fertig.push(FinishedHit {
            kern: kern(m, hall, sr, ar, roles, true),
            position: i + 1,
            winner: m.winner.unwrap_or(0),
            sets: m.sets.clone(),
            result: result_text(m.result),
            court: m.court.clone().unwrap_or_default(),
            ended_ms,
            has_timeline: tablet.timeline_store().has_timeline(m.id),
        });
    }
    // Trefferliste: nach Spielnummer absteigend (Spec), nicht nach dem
    // unsicheren Ende-Stempel.
    fertig.sort_by(|a, b| {
        b.kern
            .match_num
            .unwrap_or(0)
            .cmp(&a.kern.match_num.unwrap_or(0))
            .then(b.kern.match_id.cmp(&a.kern.match_id))
    });

    (am_feld, wartend, fertig)
}

/// Alle Treffer ungedeckelt — `None` heißt „noch keine Turnierdaten".
type Treffer = Option<(Vec<CourtHit>, Vec<WaitingHit>, Vec<FinishedHit>)>;

fn treffer(
    tablet: &TabletState,
    config: &AppConfig,
    q: &str,
    candidates: bool,
    now_ms: u64,
) -> Result<Treffer, SuchFehler> {
    let woerter = woerter(q)?;
    Ok(tablet
        .snapshot_clone()
        .map(|snap| alle_treffer(tablet, config, &snap, &woerter, candidates, now_ms)))
}

fn gedeckelt(treffer: &Treffer, officials_managed: bool, now_ms: u64, grenze: usize) -> TlSearch {
    let (court, waiting, finished) = treffer.clone().unwrap_or_default();
    TlSearch {
        server_now_ms: now_ms,
        has_data: treffer.is_some(),
        officials_managed,
        court: kappen(court, grenze),
        waiting: kappen(waiting, grenze),
        finished: kappen(finished, grenze),
    }
}

/// Die Trefferliste mit Deckel je Gruppe (Tests lesen sie strukturiert).
#[cfg(test)]
pub(crate) fn suche(
    tablet: &TabletState,
    config: &AppConfig,
    q: &str,
    candidates: bool,
    now_ms: u64,
    grenze: usize,
) -> Result<TlSearch, SuchFehler> {
    let t = treffer(tablet, config, q, candidates, now_ms)?;
    Ok(gedeckelt(
        &t,
        tablet.officials_store().enabled(),
        now_ms,
        grenze,
    ))
}

/// Einstieg für LAN-Route und Relay-Client: die Trefferliste als JSON,
/// sicher unter [`MAX_TL_SEARCH_LEN`]. Wird die Antwort zu groß, halbiert
/// sich der Deckel je Gruppe — der Host kürzt selbst, statt dass der Relay
/// still verwirft. Protokolliert wird nur Trefferzahl und Dauer, nie der
/// Begriff.
pub(crate) fn search_json(
    tablet: &TabletState,
    config: &AppConfig,
    q: &str,
    candidates: bool,
    now_ms: u64,
) -> Result<String, SuchFehler> {
    let start = std::time::Instant::now();
    let t = treffer(tablet, config, q, candidates, now_ms)?;
    let officials_managed = tablet.officials_store().enabled();
    Ok(json_unter_deckel(
        &t,
        officials_managed,
        now_ms,
        MAX_TL_SEARCH_LEN,
        start,
    ))
}

/// Höchstzahl gleichzeitig laufender Suchen am Turnier-PC, LAN und Cloud
/// zusammen. Jede Suche läuft über den ganzen Bestand; ohne Grenze könnte ein
/// einzelnes Gerät mit einer Suchschleife den Blocking-Pool füllen und
/// Sync-Takt und Punktstände ausbremsen (Security-Review 09.11.2026). Der
/// Relay gibt eine Anfrage nach 20 s auf, der Host merkte davon nichts.
pub(crate) const MAX_GLEICHZEITIG: usize = 2;

static LAUFEND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Belegt einen der [`MAX_GLEICHZEITIG`] Plätze eines Zählers; gibt ihn
/// beim Fallenlassen frei. Der Zähler ist ein Parameter, damit Tests einen
/// eigenen benutzen und parallel laufende Tests sich nicht gegenseitig
/// die Plätze wegnehmen.
struct Platz<'a>(&'a std::sync::atomic::AtomicUsize);

impl<'a> Platz<'a> {
    fn nehmen(zaehler: &'a std::sync::atomic::AtomicUsize) -> Option<Platz<'a>> {
        use std::sync::atomic::Ordering;
        // Schleife statt `fetch_update`/`try_update`: Der Name wechselt
        // gerade zwischen Rust-Versionen, die CI soll mit beiden bauen.
        let mut n = zaehler.load(Ordering::Acquire);
        loop {
            if n >= MAX_GLEICHZEITIG {
                return None;
            }
            match zaehler.compare_exchange(n, n + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(Platz(zaehler)),
                Err(aktuell) => n = aktuell,
            }
        }
    }
}

impl Drop for Platz<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// [`search_json`] mit Platzgrenze — der Einstieg für LAN-Route und
/// Relay-Client.
pub(crate) fn search_json_begrenzt(
    tablet: &TabletState,
    config: &AppConfig,
    q: &str,
    candidates: bool,
    now_ms: u64,
) -> Result<String, SuchFehler> {
    let Some(_platz) = Platz::nehmen(&LAUFEND) else {
        return Err(SuchFehler::Belegt);
    };
    search_json(tablet, config, q, candidates, now_ms)
}

fn json_unter_deckel(
    t: &Treffer,
    officials_managed: bool,
    now_ms: u64,
    max_len: usize,
    start: std::time::Instant,
) -> String {
    let mut grenze = TL_SEARCH_HITS_PER_GROUP;
    loop {
        let ergebnis = gedeckelt(t, officials_managed, now_ms, grenze);
        let json = serde_json::to_string(&ergebnis).unwrap_or_default();
        if json.len() <= max_len || grenze <= 1 {
            let anzahl = ergebnis.court.hits.len()
                + ergebnis.waiting.hits.len()
                + ergebnis.finished.hits.len();
            tracing::debug!(
                "TL-Suche: {anzahl} Treffer in {} ms",
                start.elapsed().as_millis()
            );
            return json;
        }
        grenze /= 2;
    }
}

#[cfg(test)]
#[path = "suche_tests.rs"]
mod tests;
