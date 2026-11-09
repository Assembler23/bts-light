//! Meetingpoint-Zustand: Vorbereitungs-Aufrufe, turniergebunden auf der
//! Platte (Spec `docs/features/meetingpoint-spielbereit.md`, ADR 0071).
//!
//! Bis hierher lebten die „in Vorbereitung"-Aufrufe nur im Arbeitsspeicher:
//! Nach jedem App-Neustart — also auch nach jedem Update im Turnierbetrieb —
//! waren sie weg, und das BTP-Gelb (P1) blieb verwaist stehen. Der Meetingpoint
//! hängt künftig die Check-ins der Beteiligten an den Aufruf; beides muss
//! einen Neustart überleben.
//!
//! **Lebenszyklus eines Aufrufs** (Spec, Entscheidung 4):
//! - Neuaufruf in **dieselbe** Halle behält den ersten Aufrufzeitpunkt (die
//!   Uhr für Fehlende läuft weiter); Neuaufruf in eine **andere** Halle ersetzt
//!   den Aufruf samt Zeitpunkt.
//! - Kommt das Spiel aufs Feld, wird der Aufruf **geparkt**, nicht gelöscht.
//!   Führt BTP es danach [`DEASSIGN_CONFIRM_POLLS`] Abrufe in Folge wieder als
//!   geplant (= vom Feld genommen), lebt er wieder auf — egal auf welchem Weg
//!   das Spiel vom Feld kam (Desktop, TL-Web, BTP). Ein einzelnes Flackern
//!   reaktiviert nicht, sonst schriebe der P1-Abgleich zweimal nach BTP.
//! - Spielende, verschwundenes Match oder fehlende Paarung löschen ihn.
//!
//! Grundsätze (Muster ADR 0022, wie `wish_court.rs`): turniergebunden,
//! außerhalb der `AppConfig`, best effort, unlesbare Datei → leerer Stand.
//! Kein Rückschreiben nach BTP (R2).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use relay_proto::MpPlatz;

use crate::btp::model::{BtpMatch, BtpPlayer, BtpSnapshot, MatchStatus};
use crate::config::MeetingpointConfig;
use crate::tablet::match_times::DEASSIGN_CONFIRM_POLLS;
use crate::tablet::state::{PreparationCall, TabletState};

/// Phase eines Aufrufs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Gerufen und wartend — zählt als Aufruf (Anzeige, Ansage, P1-Gelb).
    #[default]
    Aktiv,
    /// Das Spiel steht auf dem Feld; der Aufruf ruht, bis es wieder
    /// vom Feld genommen wird.
    Geparkt,
    /// Unbekannter Wert aus einer neueren Version — wie geparkt behandeln
    /// (lieber stumm als ein Geister-Aufruf).
    #[serde(other)]
    Unbekannt,
}

/// Ein gespeicherter Aufruf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredCall {
    match_id: i64,
    #[serde(default)]
    location_id: Option<i64>,
    called_at_ms: u64,
    #[serde(default)]
    phase: Phase,
    /// Check-ins am Meetingpoint (je Platz höchstens einer).
    #[serde(default)]
    checkins: Vec<StoredCheckin>,
    /// Vorläufig eingeteilter Schiedsrichter (ADR 0072, abgelöst durch ADR
    /// 0080). Nur noch lesbar: Der erste Lauf übernimmt ihn einmalig als
    /// feste Zuteilung ([`MeetingpointStore::nimm_vorab_sr`]); danach steht
    /// er nicht mehr in der Datei.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vorab_sr: Option<i64>,
    /// Fehlende wurden schon angesagt (genau einmal je Partie und Halle).
    #[serde(default)]
    missing_announced: bool,
    /// Von der Aufruf-Automatik gerufen (nur Anzeige).
    #[serde(default)]
    auto: bool,
    /// Seit wann geparkt (aufs Feld gekommen, Unix-ms) — die Seite zeigt die
    /// Partie dann noch kurz mit ihrem Feld an.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    geparkt_ms: Option<u64>,
}

/// Ein Check-in: Platz, Personen-Fingerabdruck, Zeitpunkt. Der Fingerabdruck
/// entwertet den Check-in, sobald BTP eine andere Person auf den Platz setzt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredCheckin {
    platz: MpPlatz,
    fp: String,
    at_ms: u64,
}

/// Ein aktiver Aufruf mit allem, was Bewertung und Anzeige brauchen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpCall {
    pub match_id: i64,
    pub location_id: Option<i64>,
    pub called_at_ms: u64,
    pub auto: bool,
    pub missing_announced: bool,
    /// Platz → Fingerabdruck der eingecheckten Person.
    pub checkins: Vec<(MpPlatz, String)>,
}

impl MpCall {
    /// Ist dieser Platz mit genau dieser Person eingecheckt?
    pub fn eingecheckt(&self, platz: MpPlatz, fp: &str) -> bool {
        self.checkins.iter().any(|(p, f)| *p == platz && f == fp)
    }
}

/// Fingerabdruck eines Spielers: BTP-`PlayerID`, ersatzweise der Name.
/// Bleibt lokal (Datei im App-Verzeichnis), geht nie auf den Draht.
pub fn spieler_fp(p: &BtpPlayer) -> String {
    if p.id > 0 {
        format!("p{}", p.id)
    } else {
        format!("n{}", p.name.trim().to_lowercase())
    }
}

/// Fingerabdruck eines Schiedsrichters (BTP-OfficialID).
pub fn sr_fp(official_id: i64) -> String {
    format!("o{official_id}")
}

/// Die Spieler-Plätze einer Partie mit ihrer Person.
pub fn spieler_plaetze(m: &BtpMatch) -> Vec<(MpPlatz, &BtpPlayer)> {
    let mut out = Vec::new();
    for (seite, team) in [(1u8, &m.team1), (2u8, &m.team2)] {
        for (pos, p) in team.iter().enumerate().take(2) {
            out.push((
                MpPlatz::Spieler {
                    seite,
                    pos: pos as u8,
                },
                p,
            ));
        }
    }
    out
}

/// Fingerabdruck der Person, die jetzt auf diesem Spieler-Platz steht.
pub fn platz_fp(m: &BtpMatch, platz: MpPlatz) -> Option<String> {
    spieler_plaetze(m)
        .into_iter()
        .find(|(p, _)| *p == platz)
        .map(|(_, pl)| spieler_fp(pl))
}

/// Schiedsrichter-Lage einer Partie, vom Aufrufer aus Officials-Store und
/// BTP zusammengesetzt (Sicht „Hand vor BTP", ADR 0080).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SrLage {
    /// Eingeteilter SR; `None` = keiner.
    pub id: Option<i64>,
    pub name: Option<String>,
    /// Die TL hat „ohne Schiedsrichter" gesetzt.
    pub ohne: bool,
    /// Zählt der SR für „spielbereit" (Schalter + SR-Management an)?
    pub noetig: bool,
}

/// Ergebnis der Bewertung einer gerufenen Partie.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bewertung {
    pub team1_present: Vec<bool>,
    pub team2_present: Vec<bool>,
    pub sr_present: bool,
    /// Spielbereit (im Code `mp_bereit`; „spielbereit" heißt dort schon
    /// „Paarung steht, nicht blockiert").
    pub bereit: bool,
    /// Namen der Fehlenden, Spieler zuerst, dann ggf. der SR.
    pub fehlend: Vec<String>,
}

/// Bewertet eine gerufene Partie: Wer ist da, wer fehlt, ist sie bereit?
pub fn bewerte(m: &BtpMatch, call: &MpCall, sr: &SrLage) -> Bewertung {
    let mut b = Bewertung::default();
    let mut alle_spieler = true;
    for (platz, p) in spieler_plaetze(m) {
        let da = call.eingecheckt(platz, &spieler_fp(p));
        if let MpPlatz::Spieler { seite, .. } = platz {
            if seite == 1 {
                b.team1_present.push(da);
            } else {
                b.team2_present.push(da);
            }
        }
        if !da {
            alle_spieler = false;
            b.fehlend.push(p.name.clone());
        }
    }
    b.sr_present = sr
        .id
        .is_some_and(|id| call.eingecheckt(MpPlatz::Sr, &sr_fp(id)));
    let sr_ok = !sr.noetig || sr.ohne || b.sr_present;
    if sr.noetig && !sr.ohne && !b.sr_present {
        if let Some(n) = &sr.name {
            b.fehlend.push(n.clone());
        }
    }
    b.bereit = alle_spieler && sr_ok && !m.team1.is_empty() && !m.team2.is_empty();
    b
}

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MeetingpointFile {
    #[serde(default)]
    tournament: String,
    /// In Aufrufreihenfolge.
    #[serde(default)]
    calls: Vec<StoredCall>,
    /// Anwesenheits-Bestätigungen am Zähl-Tablet je Partie (nur Anzeige,
    /// unabhängig vom Aufruf, gelöscht bei Spielende).
    #[serde(default)]
    feld_check: HashMap<i64, Vec<StoredCheckin>>,
    /// Stand der Reihenfolge-Bedeutung (ADR 0077). Fehlt das Feld (Datei aus
    /// einer älteren Version), ordnet [`MeetingpointStore::ordne_einmalig`]
    /// die Aufrufe beim ersten Binden einmal nach dem bisherigen
    /// Vergabe-Schlüssel — sonst sprängen gerufene Partien beim Update-Neustart
    /// mitten im Turnier.
    #[serde(default)]
    order_v: u32,
}

/// Aktueller Stand von [`MeetingpointFile::order_v`]: Die Position in
/// `calls` ist der Aufruf-Rang der Vergabe.
const REIHENFOLGE_V: u32 = 1;

/// Warum ein Umsortieren der Gerufenen abgelehnt wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerschiebeFehler {
    /// Das zu verschiebende Spiel ist nicht (mehr) aktiv gerufen.
    NichtGerufen,
    /// Das Ziel ist nicht (mehr) aktiv gerufen.
    ZielNichtGerufen,
    /// Ziel steht in einer anderen Aufruf-Halle — dort wirkte der Zug nicht.
    AndereHalle,
}

impl VerschiebeFehler {
    /// Meldung für die Turnierleitung.
    pub fn text(self) -> &'static str {
        match self {
            Self::NichtGerufen => "Das Spiel ist nicht mehr gerufen.",
            Self::ZielNichtGerufen => "Das Zielspiel ist nicht mehr gerufen.",
            Self::AndereHalle => {
                "Gerufene Spiele lassen sich nur innerhalb ihrer Halle umsortieren."
            }
        }
    }
}

/// Ergebnis eines Ladeversuchs (Muster `WishCourtStore`).
enum Ladung {
    Stand(MeetingpointFile),
    Leer,
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: MeetingpointFile,
    loaded: bool,
    /// Abrufe in Folge, in denen ein geparktes Spiel wieder als geplant
    /// geführt wurde. Bewusst nicht auf der Platte: Nach einem Neustart
    /// beginnt die Bestätigung von vorn, das kostet höchstens drei Abrufe.
    off_court_polls: HashMap<i64, u32>,
    /// Abrufe in Folge mit leerer Paarung. Ein einzelner leerer Abruf (BTP
    /// mitten im Speichern) soll Aufruf und Platz nicht kosten; erst nach
    /// [`DEASSIGN_CONFIRM_POLLS`] wird gelöscht. Nur RAM, wie oben.
    leer_polls: HashMap<i64, u32>,
    /// Geladene Altdatei wartet auf die einmalige Ordnung (ADR 0077).
    ordnung_offen: bool,
}

/// Der Meetingpoint-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState), damit Commands, TL-Web und
/// Sync-Lauf denselben Stand sehen.
#[derive(Default)]
pub struct MeetingpointStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests).
    path: RwLock<Option<PathBuf>>,
    inner: Mutex<Inner>,
    persist_lock: Mutex<()>,
}

impl MeetingpointStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (je Snapshot). Beim ersten Mal wird der
    /// passende Stand geladen; bei Turnierwechsel verworfen.
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.loaded && inner.file.tournament == name {
                return;
            }
            if !inner.loaded {
                match self.load_file() {
                    Ladung::Stand(file) => {
                        inner.loaded = true;
                        if file.tournament == name {
                            // Aufrufe, die im RAM entstanden sind, bevor das
                            // Turnier feststand, gehen nicht verloren.
                            let frueh = std::mem::take(&mut inner.file.calls);
                            inner.ordnung_offen = file.order_v < REIHENFOLGE_V;
                            inner.file = file;
                            if frueh.is_empty() {
                                return;
                            }
                            for c in frueh {
                                upsert(&mut inner.file.calls, c);
                            }
                            // Frühe Aufrufe gleich sichern, nicht erst mit
                            // der nächsten Änderung (Review 02.11.2026).
                            drop(inner);
                            self.persist();
                            return;
                        }
                    }
                    Ladung::Leer => {
                        inner.loaded = true;
                        inner.file.order_v = REIHENFOLGE_V;
                    }
                    Ladung::Unlesbar => return,
                }
            }
            if inner.file.tournament.is_empty() {
                // Erstbindung: frühe RAM-Aufrufe gehören zu diesem Turnier.
                inner.file.tournament = name.to_string();
                inner.file.order_v = REIHENFOLGE_V;
                inner.ordnung_offen = false;
            } else if inner.file.tournament != name {
                // Turnierwechsel: Match-IDs gelten nur innerhalb eines Turniers.
                inner.file = MeetingpointFile {
                    tournament: name.to_string(),
                    order_v: REIHENFOLGE_V,
                    ..Default::default()
                };
                inner.off_court_polls.clear();
                inner.leer_polls.clear();
                inner.ordnung_offen = false;
            }
        }
        self.persist();
    }

    /// Aktuell gebundenes Turnier.
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    /// Ordnet eine geladene Altdatei (ohne `order_v`) einmal stabil nach
    /// `schluessel` (dem bisherigen Vergabe-Schlüssel, ADR 0077) und setzt
    /// die Marke. Läuft nur einmal je Datei; sonst ohne Wirkung.
    pub fn ordne_einmalig<K: Ord>(&self, schluessel: impl Fn(i64) -> K) {
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.ordnung_offen {
                return;
            }
            inner.ordnung_offen = false;
            inner.file.calls.sort_by_key(|c| schluessel(c.match_id));
            inner.file.order_v = REIHENFOLGE_V;
        }
        self.persist();
    }

    /// Verschiebt einen aktiven Aufruf vor `before` (ein anderer aktiver
    /// Aufruf **derselben** Halle) bzw. mit `None` ans Ende. Zeitpunkt,
    /// Check-ins und vorläufiger SR bleiben. `Ok(false)` = keine Bewegung.
    pub fn verschiebe(&self, match_id: i64, before: Option<i64>) -> Result<bool, VerschiebeFehler> {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let calls = &mut inner.file.calls;
            let Some(von) = calls
                .iter()
                .position(|c| c.match_id == match_id && c.phase == Phase::Aktiv)
            else {
                return Err(VerschiebeFehler::NichtGerufen);
            };
            if before == Some(match_id) {
                return Ok(false);
            }
            let halle = calls[von].location_id;
            if let Some(b) = before {
                let Some(ziel) = calls
                    .iter()
                    .find(|c| c.match_id == b && c.phase == Phase::Aktiv)
                else {
                    return Err(VerschiebeFehler::ZielNichtGerufen);
                };
                if ziel.location_id != halle {
                    return Err(VerschiebeFehler::AndereHalle);
                }
            }
            let vorher: Vec<i64> = calls.iter().map(|c| c.match_id).collect();
            let eintrag = calls.remove(von);
            let nach = match before {
                Some(b) => calls
                    .iter()
                    .position(|c| c.match_id == b)
                    .expect("Ziel oben geprüft"),
                None => calls.len(),
            };
            calls.insert(nach, eintrag);
            calls.iter().map(|c| c.match_id).collect::<Vec<_>>() != vorher
        };
        if changed {
            self.persist();
        }
        Ok(changed)
    }

    /// Ruft ein Spiel (Hand oder Automatik). Gleiche Halle wie ein schon
    /// bestehender Aufruf → erster Zeitpunkt bleibt, ein geparkter Aufruf
    /// wird wieder aktiv; andere Halle → Aufruf wird ersetzt und rückt ans
    /// Ende der Reihenfolge.
    pub fn call(&self, call: PreparationCall) {
        self.call_mit(call, false);
    }

    /// Wie [`call`](Self::call); `auto` markiert einen Aufruf der
    /// Aufruf-Automatik. Ein Hand-Aufruf derselben Partie nimmt die Marke weg.
    pub fn call_mit(&self, call: PreparationCall, auto: bool) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            inner.off_court_polls.remove(&call.match_id);
            upsert(
                &mut inner.file.calls,
                StoredCall {
                    match_id: call.match_id,
                    location_id: call.location_id,
                    called_at_ms: call.called_at_ms,
                    phase: Phase::Aktiv,
                    checkins: Vec::new(),
                    vorab_sr: None,
                    missing_announced: false,
                    auto,
                    geparkt_ms: None,
                },
            )
        };
        if changed {
            self.persist();
        }
    }

    /// Nimmt den Aufruf eines Spiels zurück (löscht ihn ganz).
    pub fn retract(&self, match_id: i64) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            inner.off_court_polls.remove(&match_id);
            inner.leer_polls.remove(&match_id);
            let before = inner.file.calls.len();
            inner.file.calls.retain(|c| c.match_id != match_id);
            before != inner.file.calls.len()
        };
        if changed {
            self.persist();
        }
    }

    /// Alle **aktiven** Aufrufe in Aufrufreihenfolge — geparkte zählen
    /// nicht (sonst bliebe ein Spiel auf dem Feld gelb oder angesagt).
    pub fn active_calls(&self) -> Vec<PreparationCall> {
        self.inner
            .lock()
            .unwrap()
            .file
            .calls
            .iter()
            .filter(|c| c.phase == Phase::Aktiv)
            .map(|c| PreparationCall {
                match_id: c.match_id,
                location_id: c.location_id,
                called_at_ms: c.called_at_ms,
            })
            .collect()
    }

    /// Phase eines Aufrufs (für Tests und spätere Anzeigen).
    pub fn phase(&self, match_id: i64) -> Option<Phase> {
        self.inner
            .lock()
            .unwrap()
            .file
            .calls
            .iter()
            .find(|c| c.match_id == match_id)
            .map(|c| c.phase)
    }

    /// Gleicht die Aufrufe mit dem BTP-Stand ab (je Sync-Lauf): parken,
    /// reaktivieren, aufräumen. Liefert die Match-IDs der **gelöschten**
    /// Aufrufe (für das Vergessen der Nachruf-Stufen); leer, wenn nichts
    /// gelöscht wurde.
    pub fn reconcile(&self, snapshot: &BtpSnapshot) -> Vec<i64> {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let Inner {
                file,
                off_court_polls,
                leer_polls,
                ..
            } = &mut *inner;
            let mut changed = false;
            let mut geloescht: Vec<i64> = Vec::new();
            let mut keep: Vec<StoredCall> = Vec::with_capacity(file.calls.len());
            for mut c in file.calls.drain(..) {
                let Some(m) = snapshot.matches.iter().find(|m| m.id == c.match_id) else {
                    off_court_polls.remove(&c.match_id);
                    leer_polls.remove(&c.match_id);
                    changed = true;
                    geloescht.push(c.match_id);
                    continue; // verschwunden
                };
                if m.status == MatchStatus::Finished {
                    off_court_polls.remove(&c.match_id);
                    leer_polls.remove(&c.match_id);
                    changed = true;
                    geloescht.push(c.match_id);
                    continue; // beendet
                }
                if m.team1.is_empty() || m.team2.is_empty() {
                    // Paarung weg — erst nach bestätigten Abrufen löschen,
                    // sonst kostete ein einzelner leerer Abruf den Platz in
                    // der Reihenfolge (ADR 0077).
                    let n = leer_polls.entry(c.match_id).or_insert(0);
                    *n += 1;
                    if *n >= DEASSIGN_CONFIRM_POLLS {
                        leer_polls.remove(&c.match_id);
                        off_court_polls.remove(&c.match_id);
                        changed = true;
                        geloescht.push(c.match_id);
                        continue;
                    }
                    keep.push(c);
                    continue;
                }
                leer_polls.remove(&c.match_id);
                match (&m.status, c.phase) {
                    (&MatchStatus::OnCourt, Phase::Geparkt) => {
                        off_court_polls.remove(&c.match_id);
                    }
                    (&MatchStatus::OnCourt, _) => {
                        c.phase = Phase::Geparkt;
                        c.geparkt_ms = Some(crate::tablet::monitor::now_ms());
                        off_court_polls.remove(&c.match_id);
                        changed = true;
                    }
                    (&MatchStatus::Scheduled, Phase::Aktiv) => {}
                    (&MatchStatus::Scheduled, _) => {
                        let n = off_court_polls.entry(c.match_id).or_insert(0);
                        *n += 1;
                        if *n >= DEASSIGN_CONFIRM_POLLS {
                            off_court_polls.remove(&c.match_id);
                            c.phase = Phase::Aktiv;
                            c.geparkt_ms = None;
                            changed = true;
                        }
                    }
                    (&MatchStatus::Finished, _) => unreachable!("oben aussortiert"),
                }
                keep.push(c);
            }
            file.calls = keep;
            // Feld-Bestätigungen leben bis Spielende.
            let vorher = file.feld_check.len();
            file.feld_check.retain(|id, _| {
                snapshot
                    .matches
                    .iter()
                    .any(|m| m.id == *id && m.status != MatchStatus::Finished)
            });
            changed |= vorher != file.feld_check.len();
            (changed, geloescht)
        };
        let (changed, geloescht) = changed;
        if changed {
            self.persist();
        }
        geloescht
    }

    /// Alle aktiven Aufrufe mit Check-ins (Aufrufreihenfolge).
    pub fn active_entries(&self) -> Vec<MpCall> {
        self.inner
            .lock()
            .unwrap()
            .file
            .calls
            .iter()
            .filter(|c| c.phase == Phase::Aktiv)
            .map(|c| MpCall {
                match_id: c.match_id,
                location_id: c.location_id,
                called_at_ms: c.called_at_ms,
                auto: c.auto,
                missing_announced: c.missing_announced,
                checkins: c.checkins.iter().map(|k| (k.platz, k.fp.clone())).collect(),
            })
            .collect()
    }

    /// Geparkte Aufrufe, die höchstens `frist_ms` vor `now` aufs Feld kamen
    /// (neueste zuerst) — für „Aufgerufen – Feld N" auf der Seite.
    pub fn kuerzlich_aufgerufen(&self, now: u64, frist_ms: u64) -> Vec<MpCall> {
        let inner = self.inner.lock().unwrap();
        let mut out: Vec<(u64, MpCall)> = inner
            .file
            .calls
            .iter()
            .filter(|c| c.phase == Phase::Geparkt)
            .filter_map(|c| {
                let seit = c.geparkt_ms?;
                (now.saturating_sub(seit) < frist_ms).then(|| {
                    (
                        seit,
                        MpCall {
                            match_id: c.match_id,
                            location_id: c.location_id,
                            called_at_ms: c.called_at_ms,
                            auto: c.auto,
                            missing_announced: c.missing_announced,
                            checkins: c.checkins.iter().map(|k| (k.platz, k.fp.clone())).collect(),
                        },
                    )
                })
            })
            .collect();
        out.sort_by_key(|(seit, _)| std::cmp::Reverse(*seit));
        out.into_iter().map(|(_, c)| c).collect()
    }

    /// Einen Platz am Meetingpoint ein- (`Some(fp)`) oder auschecken
    /// (`None`). Nur an **aktiven** Aufrufen; `None`, wenn es den Aufruf
    /// nicht (mehr) gibt, sonst `Some(geändert)`.
    pub fn set_checkin(
        &self,
        match_id: i64,
        platz: MpPlatz,
        fp: Option<String>,
        now: u64,
    ) -> Option<bool> {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let c = inner
                .file
                .calls
                .iter_mut()
                .find(|c| c.match_id == match_id && c.phase == Phase::Aktiv)?;
            setze(&mut c.checkins, platz, fp, now)
        };
        if changed {
            self.persist();
        }
        Some(changed)
    }

    /// Bestätigung am Zähl-Tablet setzen oder zurücknehmen (nur Anzeige).
    /// Liefert, ob sich etwas geändert hat.
    pub fn set_feld_check(
        &self,
        match_id: i64,
        platz: MpPlatz,
        fp: Option<String>,
        now: u64,
    ) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let liste = inner.file.feld_check.entry(match_id).or_default();
            let changed = setze(liste, platz, fp, now);
            if liste.is_empty() {
                inner.file.feld_check.remove(&match_id);
            }
            changed
        };
        if changed {
            self.persist();
        }
        changed
    }

    /// Am Feld bestätigte Plätze einer Partie (Platz → Fingerabdruck).
    pub fn feld_checks(&self, match_id: i64) -> Vec<(MpPlatz, String)> {
        self.inner
            .lock()
            .unwrap()
            .file
            .feld_check
            .get(&match_id)
            .map(|l| l.iter().map(|k| (k.platz, k.fp.clone())).collect())
            .unwrap_or_default()
    }

    /// Nur für Tests: einen alten vorläufigen SR hinterlegen (ADR 0072).
    #[cfg(test)]
    pub fn setze_vorab_sr_fuer_test(&self, match_id: i64, sr: i64) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(c) = inner.file.calls.iter_mut().find(|c| c.match_id == match_id) {
            c.vorab_sr = Some(sr);
        }
    }

    /// Halle eines Aufrufs (aktiv oder geparkt); `None` = ohne Halle oder
    /// kein Aufruf.
    pub fn location_of(&self, match_id: i64) -> Option<i64> {
        self.inner
            .lock()
            .unwrap()
            .file
            .calls
            .iter()
            .find(|c| c.match_id == match_id)
            .and_then(|c| c.location_id)
    }

    /// Einmalige Übernahme (ADR 0080 löst ADR 0072 ab): liefert die noch
    /// gespeicherten vorläufigen SR (aktive **und** geparkte Aufrufe, Match →
    /// Official), deren Spiel `bekannt` gilt, und leert genau diese. Danach
    /// schreibt die Datei kein `vorab_sr` mehr. Ein kurz leerer Abruf (Spiel
    /// noch nicht im Snapshot) verwirft also nichts. Idempotent.
    pub fn nimm_vorab_sr(&self, bekannt: impl Fn(i64) -> bool) -> HashMap<i64, i64> {
        let out: HashMap<i64, i64> = {
            let mut inner = self.inner.lock().unwrap();
            inner
                .file
                .calls
                .iter_mut()
                .filter(|c| bekannt(c.match_id))
                .filter_map(|c| c.vorab_sr.take().map(|sr| (c.match_id, sr)))
                .collect()
        };
        if !out.is_empty() {
            self.persist();
        }
        out
    }

    /// Fehlende dieser Partie als angesagt merken.
    pub fn mark_missing_announced(&self, match_id: i64) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match inner.file.calls.iter_mut().find(|c| c.match_id == match_id) {
                Some(c) if !c.missing_announced => {
                    c.missing_announced = true;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.persist();
        }
    }

    /// Check-ins entfernen, deren Person jetzt woanders auf dem Feld steht
    /// oder nicht mehr auf dem Platz (BTP-Ersatz). `auf_dem_feld` ordnet
    /// jedem Fingerabdruck die Partie zu, in der die Person gerade spielt
    /// bzw. Schiedsrichter ist. Liefert `true` bei einer Änderung.
    ///
    /// `sr_je_partie` = Fingerabdruck des aktuell eingeteilten SR je Partie:
    /// Wechselt der SR, verfällt der alte SR-Check-in (ADR 0080).
    pub fn bereinige_checkins(
        &self,
        snapshot: &BtpSnapshot,
        auf_dem_feld: &HashMap<String, i64>,
        sr_je_partie: &HashMap<i64, String>,
    ) -> bool {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for c in inner.file.calls.iter_mut() {
                if c.phase != Phase::Aktiv {
                    continue; // geparkt: der eigene Stand bleibt für „vom Feld"
                }
                let m = snapshot.matches.iter().find(|m| m.id == c.match_id);
                let vorher = c.checkins.len();
                c.checkins.retain(|k| {
                    if auf_dem_feld
                        .get(&k.fp)
                        .is_some_and(|mid| *mid != c.match_id)
                    {
                        return false; // spielt/pfeift gerade anderswo
                    }
                    match k.platz {
                        MpPlatz::Spieler { .. } => {
                            m.and_then(|m| platz_fp(m, k.platz)).as_deref() == Some(&k.fp)
                        }
                        MpPlatz::Sr => sr_je_partie.get(&c.match_id) == Some(&k.fp),
                        MpPlatz::Unbekannt => false,
                    }
                });
                changed |= vorher != c.checkins.len();
            }
            changed
        };
        if changed {
            self.persist();
        }
        changed
    }

    // ── Persistenz ────────────────────────────────────────────────────

    fn load_file(&self) -> Ladung {
        let Some(path) = self.path.read().unwrap().clone() else {
            return Ladung::Leer;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ladung::Leer,
            Err(e) => {
                tracing::warn!("meetingpoint.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<MeetingpointFile>(&text) {
            Ok(file) => Ladung::Stand(file),
            Err(_) => {
                tracing::warn!("meetingpoint.json unlesbar – beginne leer");
                Ladung::Leer
            }
        }
    }

    fn persist(&self) {
        let Some(path) = self.path.read().unwrap().clone() else {
            return;
        };
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let inner = self.inner.lock().unwrap();
            if inner.file.tournament.is_empty() {
                return; // ohne Turnier-Kopf nicht zuzuordnen
            }
            inner.file.clone()
        };
        if let Ok(json) = serde_json::to_string(&data) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

/// Setzt (`Some(fp)`) oder entfernt (`None`) den Check-in eines Platzes.
fn setze(liste: &mut Vec<StoredCheckin>, platz: MpPlatz, fp: Option<String>, now: u64) -> bool {
    match fp {
        Some(fp) => {
            if liste.iter().any(|k| k.platz == platz && k.fp == fp) {
                return false; // schon da — erster Zeitpunkt bleibt
            }
            liste.retain(|k| k.platz != platz);
            liste.push(StoredCheckin {
                platz,
                fp,
                at_ms: now,
            });
            true
        }
        None => {
            let vorher = liste.len();
            liste.retain(|k| k.platz != platz);
            vorher != liste.len()
        }
    }
}

/// Fügt einen Aufruf ein bzw. aktualisiert ihn nach den Halle-gleich/anders-
/// Regeln. Liefert `true`, wenn sich etwas geändert hat.
fn upsert(calls: &mut Vec<StoredCall>, neu: StoredCall) -> bool {
    if let Some(alt) = calls.iter_mut().find(|c| c.match_id == neu.match_id) {
        if alt.location_id == neu.location_id {
            // Gleiche Halle: Zeitpunkt, Platz in der Reihenfolge, Check-ins,
            // vorläufiger SR und Ansage-Stand bleiben.
            let auto = alt.auto && neu.auto;
            let changed = alt.phase != neu.phase || alt.auto != auto;
            alt.phase = neu.phase;
            alt.auto = auto;
            if alt.phase == Phase::Aktiv {
                alt.geparkt_ms = None;
            }
            return changed;
        }
    }
    calls.retain(|c| c.match_id != neu.match_id);
    calls.push(neu);
    true
}

// ─────────────────────── Dienste am Tablet-Zustand ──────────────────────
// Eine Definition für LAN, Relay, Slave-Brücke, TL-Web, Desktop und
// Sync-Loop (Spec `meetingpoint-spielbereit`, R5-Analogie).

/// Wer steht gerade wo auf dem Feld? Fingerabdruck → Match-ID, für Spieler
/// und für wirksam eingeteilte Schiedsrichter/Assistenten laufender Spiele.
pub fn auf_dem_feld(tablet: &TabletState, snap: &BtpSnapshot) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    let officials = tablet.officials_store();
    for m in snap
        .matches
        .iter()
        .filter(|m| m.status == MatchStatus::OnCourt)
    {
        for p in m.team1.iter().chain(m.team2.iter()) {
            out.insert(spieler_fp(p), m.id);
        }
        if officials.enabled() {
            let w = officials.effective(m.id, m.official1_id, m.official2_id);
            for id in [w.sr, w.ar].into_iter().flatten() {
                out.insert(sr_fp(id), m.id);
            }
        }
    }
    out
}

/// Fingerabdruck des aktuell eingeteilten SR je Spiel (Sicht „Hand vor
/// BTP") — damit ein SR-Wechsel den alten SR-Check-in entwertet.
pub fn sr_fingerabdruecke(tablet: &TabletState, snap: &BtpSnapshot) -> HashMap<i64, String> {
    let officials = tablet.officials_store();
    if !officials.enabled() {
        return HashMap::new();
    }
    snap.matches
        .iter()
        .filter_map(|m| {
            officials
                .fuer_schreiben(m.id, m.official1_id, m.official2_id)
                .sr
                .filter(|v| *v > 0)
                .map(|id| (m.id, sr_fp(id)))
        })
        .collect()
}

/// Schiedsrichter-Lage einer gerufenen Partie.
pub fn sr_lage(
    tablet: &TabletState,
    m: &BtpMatch,
    snap: &BtpSnapshot,
    cfg: &MeetingpointConfig,
) -> SrLage {
    let officials = tablet.officials_store();
    if !officials.enabled() {
        return SrLage::default();
    }
    // Sicht „Hand vor BTP" wie an der Warteliste (ADR 0080): Die Zuteilung
    // beim Aufruf steht sofort hier, nicht erst nach der BTP-Bestätigung.
    let besetzung = officials.fuer_schreiben(m.id, m.official1_id, m.official2_id);
    let ohne = besetzung.sr == Some(0);
    let id = besetzung.sr.filter(|v| *v > 0);
    SrLage {
        id,
        name: id.and_then(|i| snap.official(i)).map(|o| o.display_name()),
        ohne,
        noetig: cfg.require_umpire,
    }
}

/// Alle aktiven Aufrufe mit Partie, SR-Lage und Bewertung — in
/// **Vergabe-Reihenfolge**: derselbe Schlüssel, nach dem `auto_assign` die
/// gerufenen Partien aufs Feld bringt. Seit ADR 0077 ist das die
/// Aufruf-Reihenfolge (samt Umsortierung durch die Turnierleitung); der
/// Schlüssel bleibt trotzdem die eine Quelle, damit Seite und Vergabe nie
/// auseinanderlaufen.
pub fn bewertungen(
    tablet: &TabletState,
    snap: &BtpSnapshot,
    cfg: &MeetingpointConfig,
) -> Vec<(MpCall, SrLage, Bewertung)> {
    gerufene_in_vergabe_reihenfolge(tablet, snap)
        .into_iter()
        .map(|(call, m)| {
            let sr = sr_lage(tablet, m, snap, cfg);
            let b = bewerte(m, &call, &sr);
            (call, sr, b)
        })
        .collect()
}

/// Aktive Aufrufe mit ihrer Partie in **Vergabe-Reihenfolge** (ADR 0077) —
/// die eine Quelle für Seite, Feldvergabe und die Zuteilung beim Aufruf.
pub fn gerufene_in_vergabe_reihenfolge<'a>(
    tablet: &TabletState,
    snap: &'a BtpSnapshot,
) -> Vec<(MpCall, &'a BtpMatch)> {
    let order = tablet.queue_order_store();
    let mut liste: Vec<_> = tablet
        .meetingpoint_store()
        .active_entries()
        .into_iter()
        .enumerate()
        .filter_map(|(rang, call)| {
            let m = snap.matches.iter().find(|m| m.id == call.match_id)?;
            let key = crate::tablet::assign::sort_key_for(m, Some(rang), order);
            Some((key, (call, m)))
        })
        .collect();
    liste.sort_by_key(|(key, _)| *key);
    liste.into_iter().map(|(_, e)| e).collect()
}

/// Match-IDs aller spielbereiten gerufenen Partien (strikter Modus, Bedarf).
pub fn bereite_partien(
    tablet: &TabletState,
    snap: &BtpSnapshot,
    cfg: &MeetingpointConfig,
) -> HashSet<i64> {
    bewertungen(tablet, snap, cfg)
        .into_iter()
        .filter(|(_, _, b)| b.bereit)
        .map(|(c, _, _)| c.match_id)
        .collect()
}

/// Hallenname eines Aufrufs (BTP-Schreibweise); leer = hallenunabhängig.
pub fn hallen_name(snap: &BtpSnapshot, location_id: Option<i64>) -> String {
    location_id
        .and_then(|lid| snap.locations.iter().find(|l| l.id == lid))
        .map(|l| l.name.trim().to_string())
        .unwrap_or_default()
}

/// Ruft Spiele von Hand in die Vorbereitung (Desktop „Aufrufen" und TL-Web
/// „In Vorbereitung rufen") und sagt jeden **neuen** Aufruf an — als ersten
/// Aufruf („In Vorbereitung …"), genau wie die Aufruf-Automatik. Vorher
/// trug der Hand-Aufruf nur ein; gesprochen wurde erst über den
/// Lautsprecher-Knopf am Desktop, im TL-Web gar nicht.
///
/// Nicht erneut angesagt wird ein Spiel, das schon in **dieselbe** Halle
/// gerufen ist (dort bleibt auch die Uhr stehen), und eines, das gerade auf
/// dem Feld steht. Eine andere Halle ist ein neuer Aufruf und wird
/// angesagt. Der Aufruf räumt die automatisch vorverteilte Halle (E3, Spec
/// `hallen-vorverteilung`). `ansagen = false` am Slave: Ansage-Aufträge
/// legt nur der Turnier-PC ab, sonst gäbe es zwei Quellen — und nur er teilt
/// SR/AR zu (ADR 0080), und zwar **bevor** der Ansage-Auftrag entsteht.
/// Liefert die Zahl der Ansage-Aufträge.
pub fn hand_aufruf(
    tablet: &TabletState,
    match_ids: &[i64],
    location_id: Option<i64>,
    now_ms: u64,
    ansagen: bool,
) -> usize {
    let snap = tablet.snapshot_clone();
    let schon_gerufen: Vec<i64> = tablet
        .meetingpoint_store()
        .active_calls()
        .iter()
        .filter(|c| c.location_id == location_id)
        .map(|c| c.match_id)
        .collect();
    // Mehrfachruf: in Warteliste-Reihenfolge anstellen, nicht in Klickfolge
    // (Spec `meetingpoint-reihenfolge`, Regel 3). Unbekannte stabil ans Ende.
    let mut reihe: Vec<i64> = match_ids.to_vec();
    if let Some(snap) = snap.as_ref() {
        let order = tablet.queue_order_store();
        reihe.sort_by_cached_key(|id| {
            snap.matches
                .iter()
                .find(|m| m.id == *id)
                .map(|m| crate::tablet::assign::sort_key_for(m, None, order))
                .map_or((1u8, None), |k| (0u8, Some(k)))
        });
    }
    for &match_id in &reihe {
        tablet.add_preparation_call(PreparationCall {
            match_id,
            location_id,
            called_at_ms: now_ms,
        });
        tablet.auto_hall_store().remove(match_id);
    }
    // SR/AR zuteilen, bevor angesagt wird (ADR 0080) — nur am Turnier-PC.
    if ansagen {
        if let Some(snap) = snap.as_ref() {
            gerufene_besetzen(tablet, snap, crate::tablet::tl::heutiges_datum(now_ms));
        }
    }
    let mut auftraege = 0;
    for match_id in reihe {
        if !ansagen || schon_gerufen.contains(&match_id) {
            continue;
        }
        let Some(snap) = snap.as_ref() else { continue };
        let auf_dem_feld = snap
            .matches
            .iter()
            .any(|m| m.id == match_id && m.status == MatchStatus::OnCourt);
        if auf_dem_feld {
            continue;
        }
        tablet.publish_announce_job(
            hallen_name(snap, location_id),
            crate::tablet::state::AnnounceJobKind::MeetingpointCall { match_id },
            now_ms,
        );
        auftraege += 1;
    }
    auftraege
}

/// Passt der Hallenparameter einer Seite zum Aufruf? Ein-Hallen-Turnier:
/// immer. Mehr-Hallen: nur exakt diese Halle (ohne Groß/Klein).
fn halle_passt(snap: &BtpSnapshot, seite: &str, call_halle: &str) -> bool {
    if !snap.is_multi_hall() {
        return true;
    }
    !seite.trim().is_empty() && seite.trim().eq_ignore_ascii_case(call_halle)
}

/// Klasse + Runde für die Anzeige.
pub fn partie_label(m: &BtpMatch) -> String {
    let klasse = if m.class_label.trim().is_empty() {
        m.draw_name.trim()
    } else {
        m.class_label.trim()
    };
    match (klasse.is_empty(), m.round_name.trim().is_empty()) {
        (false, false) => format!("{klasse} · {}", m.round_name.trim()),
        (false, true) => klasse.to_string(),
        _ => m.round_name.trim().to_string(),
    }
}

/// Zustand der Meetingpoint-Seite. Nur Namen und Flags — keine
/// Lizenznummer, kein Verein, keine Nation (Datenschutz).
pub fn view(
    tablet: &TabletState,
    snap: Option<&BtpSnapshot>,
    cfg: &MeetingpointConfig,
    halle: &str,
) -> relay_proto::MeetingpointView {
    use relay_proto::{MeetingpointView, MpPartie, MpPlatzView};
    let Some(snap) = snap else {
        return MeetingpointView {
            enabled: cfg.enabled,
            ..Default::default()
        };
    };
    let multi = snap.is_multi_hall();
    let hallen: Vec<String> = if multi {
        snap.locations
            .iter()
            .map(|l| l.name.trim().to_string())
            .collect()
    } else {
        Vec::new()
    };
    let halle = halle.trim();
    let halle_ok = !multi || hallen.iter().any(|h| h.eq_ignore_ascii_case(halle));
    let mut v = MeetingpointView {
        enabled: cfg.enabled,
        halle: if halle_ok {
            halle.to_string()
        } else {
            String::new()
        },
        hallen,
        halle_waehlen: !halle_ok,
        partien: Vec::new(),
    };
    if !cfg.enabled || !halle_ok {
        return v;
    }
    let jetzt = crate::tablet::monitor::now_ms();
    // Gerade aufs Feld gerufen: noch kurz oben stehen lassen, mit Feld
    // (Nachtrag 07.11.2026) — wer nicht hinsah, erfährt so, wohin es geht.
    for call in tablet
        .meetingpoint_store()
        .kuerzlich_aufgerufen(jetzt, relay_proto::MP_AUFGERUFEN_ANZEIGE_MS)
    {
        let call_halle = hallen_name(snap, call.location_id);
        if !halle_passt(snap, halle, &call_halle) {
            continue;
        }
        let Some(m) = snap.matches.iter().find(|m| m.id == call.match_id) else {
            continue;
        };
        let Some(feld) = m
            .court
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty() && m.status == MatchStatus::OnCourt)
        else {
            continue;
        };
        let plaetze: Vec<MpPlatzView> = spieler_plaetze(m)
            .into_iter()
            .map(|(platz, p)| MpPlatzView {
                platz,
                name: p.name.clone(),
                present: call.eingecheckt(platz, &spieler_fp(p)),
            })
            .collect();
        v.partien.push(MpPartie {
            match_id: call.match_id,
            halle: call_halle,
            label: partie_label(m),
            called_at_ms: call.called_at_ms,
            plaetze,
            aufgerufen_feld: Some(feld.to_string()),
            ..Default::default()
        });
    }
    for (call, sr, b) in bewertungen(tablet, snap, cfg) {
        let prognose = tablet.mp_prognose(call.match_id, jetzt);
        let call_halle = hallen_name(snap, call.location_id);
        if !halle_passt(snap, halle, &call_halle) {
            continue;
        }
        let Some(m) = snap.matches.iter().find(|m| m.id == call.match_id) else {
            continue;
        };
        // Paarung kurz leer (BTP mitten im Speichern): Der Aufruf bleibt
        // stehen (ADR 0077), eine namenlose Karte zeigt die Seite aber nicht.
        if m.team1.is_empty() || m.team2.is_empty() {
            continue;
        }
        let mut plaetze: Vec<MpPlatzView> = spieler_plaetze(m)
            .into_iter()
            .map(|(platz, p)| MpPlatzView {
                platz,
                name: p.name.clone(),
                present: call.eingecheckt(platz, &spieler_fp(p)),
            })
            .collect();
        if let (Some(_), Some(name)) = (sr.id, sr.name.clone()) {
            plaetze.push(MpPlatzView {
                platz: MpPlatz::Sr,
                name,
                present: b.sr_present,
            });
        }
        v.partien.push(MpPartie {
            match_id: call.match_id,
            halle: call_halle,
            label: partie_label(m),
            called_at_ms: call.called_at_ms,
            plaetze,
            bereit: b.bereit,
            ohne_sr: sr.ohne,
            sr_noetig: sr.noetig && !sr.ohne,
            feld_frei_ms: prognose.map(|(ms, _)| ms),
            feld_frei_unsicher: prognose.is_some_and(|(_, u)| u),
            aufgerufen_feld: None,
        });
    }
    v
}

/// Gesamtstand für den Relay (alle Hallen; der Relay filtert je Seite).
pub fn view_alle_hallen(
    tablet: &TabletState,
    snap: Option<&BtpSnapshot>,
    cfg: &MeetingpointConfig,
) -> relay_proto::MeetingpointView {
    let Some(snap_ref) = snap else {
        return view(tablet, None, cfg, "");
    };
    if !snap_ref.is_multi_hall() {
        return view(tablet, snap, cfg, "");
    }
    let mut gesamt = relay_proto::MeetingpointView {
        enabled: cfg.enabled,
        hallen: snap_ref
            .locations
            .iter()
            .map(|l| l.name.trim().to_string())
            .collect(),
        ..Default::default()
    };
    for h in gesamt.hallen.clone() {
        gesamt.partien.extend(view(tablet, snap, cfg, &h).partien);
    }
    gesamt
}

/// Prüft und bucht einen Check-in (LAN, Relay, Slave, Zähl-Tablet, TL).
/// Die einzige Stelle, die Check-ins annimmt — die Sicherheits-Mitigation
/// der offenen Seite (ADR 0074).
pub fn checkin(
    tablet: &TabletState,
    cfg: &MeetingpointConfig,
    body: &relay_proto::MpCheckinBody,
    now: u64,
) -> Result<(), String> {
    use relay_proto::MpOrt;
    if !cfg.enabled {
        return Err("Der Meetingpoint ist ausgeschaltet.".into());
    }
    let snap = tablet
        .snapshot_clone()
        .ok_or_else(|| "Noch kein Turnierstand.".to_string())?;
    let m = snap
        .matches
        .iter()
        .find(|m| m.id == body.match_id)
        .ok_or_else(|| "Dieses Spiel gibt es nicht.".to_string())?;
    // Wer ist auf diesem Platz? Spieler direkt aus BTP, SR aus der Lage.
    let fp = match body.platz {
        MpPlatz::Spieler { .. } => {
            platz_fp(m, body.platz).ok_or_else(|| "Diesen Platz gibt es nicht.".to_string())?
        }
        MpPlatz::Sr => {
            let sr = sr_lage(tablet, m, &snap, cfg);
            sr_fp(sr.id.ok_or_else(|| {
                "Für dieses Spiel ist kein Schiedsrichter eingeteilt.".to_string()
            })?)
        }
        MpPlatz::Unbekannt => return Err("Unbekannter Platz.".into()),
    };
    // Wer gerade woanders auf dem Feld steht (spielt oder pfeift), kann hier
    // nicht einchecken — sonst verschwände der Haken beim nächsten Abgleich
    // still wieder.
    if body.present && body.ort == MpOrt::Meetingpoint {
        if let Some(anderswo) = auf_dem_feld(tablet, &snap).get(&fp) {
            if *anderswo != m.id {
                return Err("Diese Person steht gerade auf einem anderen Feld.".into());
            }
        }
    }
    let wert = body.present.then_some(fp);
    match body.ort {
        MpOrt::Meetingpoint => {
            let call = tablet
                .meetingpoint_store()
                .active_entries()
                .into_iter()
                .find(|c| c.match_id == body.match_id)
                .ok_or_else(|| "Dieses Spiel ist nicht zum Meetingpoint gerufen.".to_string())?;
            if !halle_passt(&snap, &body.halle, &hallen_name(&snap, call.location_id)) {
                return Err("Dieses Spiel ist in eine andere Halle gerufen.".into());
            }
            // Der Relay-Client pusht im 2-s-Takt nur bei geändertem
            // Fingerabdruck — ein Check-in ohne Änderung kostet nichts.
            if tablet
                .meetingpoint_store()
                .set_checkin(body.match_id, body.platz, wert, now)
                .is_none()
            {
                return Err("Der Aufruf ist inzwischen beendet.".into());
            }
        }
        MpOrt::Feld => {
            if !cfg.field_check_enabled {
                return Err("Die Bestätigung am Feld ist ausgeschaltet.".into());
            }
            if m.status != MatchStatus::OnCourt {
                return Err("Das Spiel steht nicht auf dem Feld.".into());
            }
            tablet
                .meetingpoint_store()
                .set_feld_check(body.match_id, body.platz, wert, now);
        }
        MpOrt::Unbekannt => return Err("Unbekannter Ort.".into()),
    }
    Ok(())
}

/// Will mindestens ein Feld der Aufruf-Halle diesen Dienst (Feldschalter)?
/// Ohne Halle, im Ein-Hallen-Turnier oder wenn kein Feld der Halle bekannt
/// ist, zählen alle Felder. Gesperrte Felder zählen mit — eine Sperre ist
/// vorübergehend. Ohne bekannte Felder: ja (Schalter-Default „an").
pub fn dienst_in_halle(
    snap: &BtpSnapshot,
    store: &crate::tablet::officials::OfficialsStore,
    location_id: Option<i64>,
    role: crate::tablet::officials::OfficialRole,
) -> bool {
    use crate::tablet::officials::OfficialRole;
    let alle: Vec<i64> = snap.court_infos.iter().map(|c| c.id).collect();
    let in_halle: Vec<i64> = match location_id {
        Some(lid) if snap.is_multi_hall() => snap
            .court_infos
            .iter()
            .filter(|c| c.location_id == Some(lid))
            .map(|c| c.id)
            .collect(),
        _ => Vec::new(),
    };
    let felder = if in_halle.is_empty() { alle } else { in_halle };
    if felder.is_empty() {
        return true;
    }
    felder.iter().any(|c| {
        let sw = store.court_switches(*c);
        match role {
            OfficialRole::Sr => sw.sr,
            OfficialRole::Ar => sw.ar,
        }
    })
}

/// Teilt gerufenen Partien SR und AR **fest** zu (ADR 0080, löst ADR 0072
/// ab). Zustandsbasiert: läuft synchron im Aufruf und in jedem Sync-Takt
/// (nach der Feld-Rotation, die Vorrang hat) — so überstehen Nachrücken,
/// Sync-Neustart und Migration jeden Neustart ohne Kanten-Merker. Nur am
/// Turnier-PC. Unter der Vergabe-Sperre, damit Aufruf und Sync-Lauf nie
/// dieselbe Person zweimal vergeben.
///
/// Ablauf: einmalige Übernahme alter `vorab_sr` → Merker pflegen →
/// Freigaben (Aufruf weg ohne Feld, Feld ohne Dienst) → gerufene Partien in
/// Vergabe-Reihenfolge besetzen (keine Obergrenze). Gewählt wird über
/// `rotate_court` → `waehle_besetzung` (Modus, Rolle, Vorrolle, ADR 0079).
/// Liefert die Match-IDs, deren Besetzung sich geändert hat.
pub fn gerufene_besetzen(
    tablet: &TabletState,
    snap: &BtpSnapshot,
    heute: chrono::NaiveDate,
) -> Vec<i64> {
    use crate::tablet::officials::{Belegung, Herkunft, OfficialRole, RotationInput};
    let store = tablet.officials_store();
    let mp = tablet.meetingpoint_store();
    if !store.enabled() {
        return Vec::new();
    }
    let _sperre = store.vergabe_sperre();
    let (rot_sr, rot_ar) = store.rotation();
    let bekannt: Vec<i64> = snap.officials.iter().map(|o| o.id).collect();
    let mut geaendert: Vec<i64> = Vec::new();

    // 1. Einmalige Übernahme alter vorläufiger SR (ADR 0072).
    let alt = mp.nimm_vorab_sr(|mid| snap.matches.iter().any(|m| m.id == mid));
    if !alt.is_empty() {
        let belegung = Belegung::aus(store, snap, heute);
        let mut ids: Vec<(i64, i64)> = alt.into_iter().collect();
        ids.sort_unstable();
        for (mid, sr) in ids {
            let Some(m) = snap.matches.iter().find(|m| m.id == mid) else {
                continue;
            };
            let extra = store.extra(sr);
            let besetzung = store.fuer_schreiben(m.id, m.official1_id, m.official2_id);
            let feld_will = match m.status {
                MatchStatus::OnCourt => m.court_id.is_none_or(|c| store.court_switches(c).sr),
                MatchStatus::Scheduled => {
                    let lid = mp.location_of(mid);
                    dienst_in_halle(snap, store, lid, OfficialRole::Sr)
                }
                _ => false,
            };
            let passt = rot_sr
                && extra.role.erlaubt(OfficialRole::Sr)
                && !extra.paused
                && besetzung.sr.is_none()
                && bekannt.contains(&sr)
                && !belegung.belegt_ausser(mid).contains(&sr)
                && besetzung.ar != Some(sr)
                && feld_will;
            if passt {
                store.uebernimm_als_aufruf(mid, OfficialRole::Sr, sr);
                geaendert.push(mid);
                tracing::info!("Meetingpoint: vorläufiger SR {sr} für Spiel {mid} übernommen");
            }
        }
    }

    // 2. Merker pflegen, 3. Freigaben.
    store.pflege_vorab_rotation(snap);
    for mid in store.vorab_rotation_spiele() {
        let Some(m) = snap.matches.iter().find(|m| m.id == mid) else {
            continue;
        };
        match m.status {
            MatchStatus::Scheduled if mp.phase(mid).is_none() => {
                // Aufruf zurückgenommen oder wegen leerer Paarung gelöscht.
                for (role, btp) in [
                    (OfficialRole::Sr, m.official1_id),
                    (OfficialRole::Ar, m.official2_id),
                ] {
                    if store.rotation_freigeben(mid, role, btp) {
                        geaendert.push(mid);
                    }
                }
            }
            MatchStatus::OnCourt => {
                if let Some(c) = m.court_id {
                    store.rotation_loesen_am_feld(m, c);
                }
            }
            _ => {}
        }
    }

    // 4. Besetzen, vorn zuerst.
    if !rot_sr && !rot_ar {
        return geaendert;
    }
    let belegung = Belegung::aus(store, snap, heute);
    let modus = store.rotation_mode();
    let vorrolle = if modus == crate::config::RotationMode::Rollenwechsel {
        let fertige: Vec<crate::tablet::officials::FinishedMatch> = snap
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::Finished)
            .map(crate::tablet::officials::FinishedMatch::aus_btp)
            .collect();
        store.vorrollen(&fertige)
    } else {
        HashMap::new()
    };
    let mut im_lauf: HashSet<i64> = HashSet::new();
    for (call, m) in gerufene_in_vergabe_reihenfolge(tablet, snap) {
        if m.team1.is_empty() || m.team2.is_empty() || m.status != MatchStatus::Scheduled {
            continue;
        }
        let sr = rot_sr && dienst_in_halle(snap, store, call.location_id, OfficialRole::Sr);
        let ar = rot_ar && dienst_in_halle(snap, store, call.location_id, OfficialRole::Ar);
        if !sr && !ar {
            continue;
        }
        let spieler: Vec<BtpPlayer> = m.team1.iter().chain(m.team2.iter()).cloned().collect();
        let mut belegt = belegung.belegt_ausser(m.id);
        belegt.extend(&im_lauf);
        let (neu_sr, neu_ar) = store.rotate_court(RotationInput {
            match_id: m.id,
            players: &spieler,
            btp_sr: m.official1_id,
            btp_ar: m.official2_id,
            bekannt: &bekannt,
            belegt: &belegt,
            sr,
            ar,
            modus,
            vorrolle: &vorrolle,
            herkunft: Herkunft::Aufruf,
        });
        if neu_sr.is_some() || neu_ar.is_some() {
            geaendert.push(m.id);
        }
        im_lauf.extend(neu_sr);
        im_lauf.extend(neu_ar);
    }
    geaendert.sort_unstable();
    geaendert.dedup();
    geaendert
}

/// Restzeit je nicht gesperrtem Feld in Minuten (`0` = frei), mit Halle —
/// Grundlage des Bedarfs der Aufruf-Automatik (ADR 0075). Dieselbe Rechnung
/// wie die Restzeit-Anzeige der TL (`predict::feld_rest_min`).
pub fn feld_restzeiten(
    tablet: &TabletState,
    config: &crate::config::AppConfig,
    now: u64,
) -> Vec<(String, u64)> {
    use crate::tablet::predict;
    let stats = tablet.cached_time_stats();
    let default_mins = config.prediction.default_duration_mins;
    tablet
        .overview()
        .into_iter()
        .filter(|c| !c.locked)
        .map(|c| {
            if c.match_id == 0 {
                return (c.location, 0);
            }
            let times = stats.group_times(&c.class_label, c.discipline.as_str(), default_mins);
            let (since, first_point) = tablet.court_time_stamps(c.match_id, Some(c.court_id));
            let rest = predict::feld_rest_min(predict::FeldRest {
                now_ms: now,
                sets: c.sets.clone(),
                best_of: c.best_of,
                target: c.target_score,
                cap: c.cap_score,
                tablet_connected: c.tablet_connected,
                first_assigned_ms: since,
                first_point_ms: first_point,
                times,
            });
            (c.location, rest)
        })
        .collect()
}

/// Ziel der Aufruf-Automatik für eine Halle: wie viele gerufene, vergebbare
/// Partien sollen warten? Rein, damit die Bemessung testbar bleibt.
pub fn auto_ziel(
    mode: crate::config::AutoCallMode,
    cfg: &crate::config::AutoCallConfig,
    restzeiten_der_halle: &[u64],
) -> usize {
    use crate::config::AutoCallMode;
    let cap = cfg.cap_per_hall as usize;
    let ziel = match mode {
        AutoCallMode::Fest => cfg.fixed_per_hall as usize,
        AutoCallMode::Restzeit => {
            let bald = restzeiten_der_halle
                .iter()
                .filter(|r| **r <= u64::from(cfg.horizon_minutes))
                .count();
            if bald == 0 {
                0
            } else {
                bald + cfg.buffer as usize
            }
        }
        _ => 0,
    };
    ziel.min(cap)
}

/// Bestimmt die Partien, die die Aufruf-Automatik jetzt rufen soll
/// (Match-ID, Halle). Nur am Master; hält bei TL-Pause der Automatik an und
/// nimmt nie selbst zurück (ADR 0075).
pub fn auto_aufrufe(
    tablet: &TabletState,
    snap: &BtpSnapshot,
    config: &crate::config::AppConfig,
    now: u64,
) -> Vec<(i64, Option<i64>)> {
    use crate::tablet::assign;
    let mode = config.meetingpoint.auto_call_mode();
    if mode == crate::config::AutoCallMode::Aus || tablet.auto_assign_paused() {
        return Vec::new();
    }
    let multi = snap.is_multi_hall();
    let hallen: Vec<(Option<i64>, String)> = if multi {
        snap.locations
            .iter()
            .map(|l| (Some(l.id), l.name.trim().to_string()))
            .collect()
    } else {
        vec![(None, String::new())]
    };
    let restzeiten = feld_restzeiten(tablet, config, now);
    let availability = assign::PlayerAvailability::from_snapshot(snap, config);
    let vergebbar =
        |m: &BtpMatch| !tablet.auto_assign_excluded(m.id) && availability.blocked(m, now).is_none();
    let calls = tablet.meetingpoint_store().active_entries();
    let called: HashSet<i64> = calls.iter().map(|c| c.match_id).collect();
    // Spieler, die schon in einer gerufenen Partie stehen — niemand wird
    // zweimal gleichzeitig zum Meetingpoint gerufen.
    let mut gerufen_spieler: HashSet<String> = calls
        .iter()
        .filter_map(|c| snap.matches.iter().find(|m| m.id == c.match_id))
        .flat_map(|m| m.team1.iter().chain(m.team2.iter()))
        .map(assign::player_key)
        .collect();
    let manual = tablet.manual_halls();
    let auto = tablet.auto_hall_store().halls();
    let queue = assign::ready_queue(
        config,
        snap,
        &manual,
        &auto,
        &called,
        tablet.queue_order_store(),
    );
    let mut out = Vec::new();
    for (loc, name) in hallen {
        let in_halle = |l: Option<i64>| !multi || l == loc;
        let mut aktive = calls.iter().filter(|c| in_halle(c.location_id)).count();
        let mut vorrat = calls
            .iter()
            .filter(|c| in_halle(c.location_id))
            .filter(|c| {
                snap.matches
                    .iter()
                    .find(|m| m.id == c.match_id)
                    .is_some_and(vergebbar)
            })
            .count();
        let r: Vec<u64> = restzeiten
            .iter()
            .filter(|(h, _)| !multi || h.trim().eq_ignore_ascii_case(&name))
            .map(|(_, r)| *r)
            .collect();
        let ziel = auto_ziel(mode, &config.meetingpoint.auto_call, &r);
        let cap = config.meetingpoint.auto_call.cap_per_hall as usize;
        for id in &queue {
            if vorrat >= ziel || aktive >= cap {
                break;
            }
            if out.iter().any(|(m, _)| m == id) {
                continue;
            }
            let Some(m) = snap.matches.iter().find(|m| m.id == *id) else {
                continue;
            };
            if m.team1.is_empty() || m.team2.is_empty() || !vergebbar(m) {
                continue;
            }
            let keys: Vec<String> = m
                .team1
                .iter()
                .chain(m.team2.iter())
                .map(assign::player_key)
                .collect();
            if keys.iter().any(|k| gerufen_spieler.contains(k)) {
                continue;
            }
            if multi {
                let manual_hall = assign::manual_hall_from_wish(
                    snap,
                    tablet.wish_court(m.id),
                    manual.get(&m.id).map(String::as_str),
                );
                let (halle, _, _) = assign::resolve_and_sort_key(
                    config,
                    snap,
                    m,
                    manual_hall,
                    None,
                    auto.get(&m.id).map(String::as_str),
                    None,
                    tablet.queue_order_store(),
                );
                if halle.trim().is_empty() || !halle.trim().eq_ignore_ascii_case(&name) {
                    continue; // ohne Halle bzw. andere Halle: nicht hier rufen
                }
            }
            gerufen_spieler.extend(keys);
            out.push((m.id, loc));
            vorrat += 1;
            aktive += 1;
        }
    }
    out
}

/// Sagt die Fehlenden gerufener Partien an, sobald sie `missing_announce_
/// minutes` nach dem Aufruf noch nicht spielbereit sind (B7). Genau einmal
/// je Partie und Halle; liefert die Zahl der Aufträge.
pub fn fehlende_ansagen(
    tablet: &TabletState,
    snap: &BtpSnapshot,
    cfg: &MeetingpointConfig,
    now: u64,
) -> usize {
    if !cfg.enabled || !cfg.missing_announce_enabled {
        return 0;
    }
    let frist = u64::from(cfg.missing_announce_minutes) * 60_000;
    let mut n = 0;
    for (call, _, b) in bewertungen(tablet, snap, cfg) {
        if b.bereit || call.missing_announced || b.fehlend.is_empty() {
            continue;
        }
        if now.saturating_sub(call.called_at_ms) < frist {
            continue;
        }
        tablet
            .meetingpoint_store()
            .mark_missing_announced(call.match_id);
        tablet.publish_announce_job(
            hallen_name(snap, call.location_id),
            crate::tablet::state::AnnounceJobKind::MeetingpointMissing {
                match_id: call.match_id,
                names: b.fehlend.clone(),
            },
            now,
        );
        n += 1;
    }
    n
}

/// Feld-Check eines laufenden Spiels: `[bestätigt, gesamt]` — gesamt sind
/// die Spieler plus ein wirksam eingeteilter Schiedsrichter.
pub fn feld_check_zaehlung(tablet: &TabletState, m: &BtpMatch, _snap: &BtpSnapshot) -> [u32; 2] {
    let checks = tablet.meetingpoint_store().feld_checks(m.id);
    let da_ist = |platz: MpPlatz, fp: &str| checks.iter().any(|(p, f)| *p == platz && f == fp);
    let mut da = 0u32;
    let mut gesamt = 0u32;
    for (platz, p) in spieler_plaetze(m) {
        gesamt += 1;
        if da_ist(platz, &spieler_fp(p)) {
            da += 1;
        }
    }
    let store = tablet.officials_store();
    if store.enabled() {
        if let Some(sr) = store.effective(m.id, m.official1_id, m.official2_id).sr {
            gesamt += 1;
            if da_ist(MpPlatz::Sr, &sr_fp(sr)) {
                da += 1;
            }
        }
    }
    [da, gesamt]
}

/// Korrektur durch die TL (TL-Web, Desktop): wie ein Check-in am
/// Meetingpoint der Halle, in die die Partie gerufen ist.
pub fn checkin_tl(
    tablet: &TabletState,
    cfg: &MeetingpointConfig,
    match_id: i64,
    platz: MpPlatz,
    present: bool,
    now: u64,
) -> Result<(), String> {
    let halle = tablet
        .snapshot_clone()
        .and_then(|snap| {
            tablet
                .meetingpoint_store()
                .active_entries()
                .into_iter()
                .find(|c| c.match_id == match_id)
                .map(|c| hallen_name(&snap, c.location_id))
        })
        .unwrap_or_default();
    checkin(
        tablet,
        cfg,
        &relay_proto::MpCheckinBody {
            halle,
            match_id,
            platz,
            present,
            ort: relay_proto::MpOrt::Meetingpoint,
        },
        now,
    )
}

/// TL-Sicht auf eine gerufene Partie (nur Flags + Namen, die der TL-Zustand
/// ohnehin trägt).
pub fn tl_meetingpoint(call: &MpCall, sr: &SrLage, b: &Bewertung) -> relay_proto::TlMeetingpoint {
    relay_proto::TlMeetingpoint {
        team1_present: b.team1_present.clone(),
        team2_present: b.team2_present.clone(),
        sr_name: sr.name.clone(),
        sr_present: b.sr_present,
        // Seit ADR 0080 gibt es keine vorläufige Zuteilung mehr; das Feld
        // bleibt für ältere Seiten auf dem Draht.
        sr_vorab: false,
        ohne_sr: sr.ohne,
        sr_noetig: sr.noetig,
        bereit: b.bereit,
        called_at_ms: call.called_at_ms,
        auto: call.auto,
        fehlend: b.fehlend.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::BtpMatch;
    use std::path::Path;

    const TURNIER: &str = "Test BTS Light";

    fn aufruf(match_id: i64, halle: Option<i64>, ts: u64) -> PreparationCall {
        PreparationCall {
            match_id,
            location_id: halle,
            called_at_ms: ts,
        }
    }

    fn store_mit_datei(dir: &Path) -> MeetingpointStore {
        let store = MeetingpointStore::default();
        store.set_path(dir.join("meetingpoint.json"));
        store.set_tournament(TURNIER);
        store
    }

    fn spiel(id: i64, status: MatchStatus) -> BtpMatch {
        crate::tablet::state::tests::match_on(id, None, status)
    }

    fn snap(matches: Vec<BtpMatch>) -> BtpSnapshot {
        let mut s = crate::tablet::state::tests::snapshot(matches, vec![]);
        s.tournament_name = TURNIER.into();
        s
    }

    fn ids(store: &MeetingpointStore) -> Vec<i64> {
        store.active_calls().iter().map(|c| c.match_id).collect()
    }

    #[test]
    fn aufrufe_ueberleben_einen_neustart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, Some(1), 1000));
            store.call(aufruf(11, None, 2000));
        }
        let neu = store_mit_datei(dir.path());
        assert_eq!(
            neu.active_calls(),
            vec![aufruf(10, Some(1), 1000), aufruf(11, None, 2000)]
        );
    }

    #[test]
    fn ein_turnierwechsel_verwirft_die_aufrufe() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, Some(1), 1000));
        }
        let neu = MeetingpointStore::default();
        neu.set_path(dir.path().join("meetingpoint.json"));
        neu.set_tournament("Ein anderes Turnier");
        assert!(neu.active_calls().is_empty());
        // Und auch im laufenden Betrieb.
        neu.call(aufruf(12, None, 3000));
        neu.set_tournament("Noch ein Turnier");
        assert!(neu.active_calls().is_empty());
    }

    #[test]
    fn eine_unlesbare_datei_beginnt_leer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("meetingpoint.json"), "{kaputt").unwrap();
        let store = store_mit_datei(dir.path());
        assert!(store.active_calls().is_empty());
        store.call(aufruf(10, None, 1));
        assert_eq!(ids(&store), vec![10]);
    }

    #[test]
    fn ein_aufruf_vor_der_turnierbindung_bleibt_erhalten() {
        // Startphase: Die TL ruft, bevor der erste Snapshot das Turnier
        // gemeldet hat. Der Aufruf darf beim Binden nicht verschwinden.
        let dir = tempfile::tempdir().unwrap();
        let store = MeetingpointStore::default();
        store.set_path(dir.path().join("meetingpoint.json"));
        store.call(aufruf(10, None, 1));
        store.set_tournament(TURNIER);
        assert_eq!(ids(&store), vec![10]);
        let neu = store_mit_datei(dir.path());
        assert_eq!(ids(&neu), vec![10], "und landet auf der Platte");
    }

    #[test]
    fn neuaufruf_gleiche_halle_behaelt_zeitpunkt_und_platz() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1000));
        store.call(aufruf(11, Some(1), 2000));
        store.call(aufruf(10, Some(1), 5000));
        assert_eq!(
            store.active_calls(),
            vec![aufruf(10, Some(1), 1000), aufruf(11, Some(1), 2000)]
        );
    }

    #[test]
    fn neuaufruf_andere_halle_ersetzt_und_rueckt_ans_ende() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1000));
        store.call(aufruf(11, Some(1), 2000));
        store.call(aufruf(10, Some(2), 5000));
        assert_eq!(
            store.active_calls(),
            vec![aufruf(11, Some(1), 2000), aufruf(10, Some(2), 5000)]
        );
    }

    #[test]
    fn ruecknahme_loescht_den_aufruf() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.retract(10);
        assert!(store.active_calls().is_empty());
        assert_eq!(store.phase(10), None);
    }

    #[test]
    fn auf_dem_feld_wird_geparkt_und_nach_bestaetigter_abnahme_reaktiviert() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1000));
        assert!(store
            .reconcile(&snap(vec![spiel(10, MatchStatus::OnCourt)]))
            .is_empty());
        assert_eq!(store.phase(10), Some(Phase::Geparkt));
        assert!(store.active_calls().is_empty(), "kein Aufruf am Feldspiel");

        let geplant = snap(vec![spiel(10, MatchStatus::Scheduled)]);
        for _ in 1..DEASSIGN_CONFIRM_POLLS {
            store.reconcile(&geplant);
            assert_eq!(store.phase(10), Some(Phase::Geparkt));
        }
        store.reconcile(&geplant);
        assert_eq!(store.active_calls(), vec![aufruf(10, Some(1), 1000)]);
    }

    #[test]
    fn ein_flackern_reaktiviert_nicht() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        let feld = snap(vec![spiel(10, MatchStatus::OnCourt)]);
        let geplant = snap(vec![spiel(10, MatchStatus::Scheduled)]);
        store.reconcile(&feld);
        for _ in 0..5 {
            store.reconcile(&geplant); // ein Abruf „geplant"
            store.reconcile(&feld); // und wieder auf dem Feld
        }
        assert_eq!(store.phase(10), Some(Phase::Geparkt));
    }

    #[test]
    fn spielende_verschwinden_und_fehlende_paarung_loeschen() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.call(aufruf(11, None, 2));
        store.call(aufruf(12, None, 3));
        store.call(aufruf(13, None, 4));
        let mut ohne_paarung = spiel(12, MatchStatus::Scheduled);
        ohne_paarung.team2.clear();
        let stand = snap(vec![
            spiel(10, MatchStatus::Finished),
            ohne_paarung,
            spiel(13, MatchStatus::Scheduled),
        ]);
        let geloescht = store.reconcile(&stand);
        // Beendet und verschwunden sofort; die leere Paarung erst bestätigt
        // (ADR 0077: ein einzelner leerer Abruf kostet nicht den Platz).
        assert_eq!(geloescht, vec![10, 11]);
        assert_eq!(ids(&store), vec![12, 13]);
        assert!(store.reconcile(&stand).is_empty());
        assert_eq!(store.reconcile(&stand), vec![12]);
        assert_eq!(ids(&store), vec![13]);
    }

    #[test]
    fn ein_abruf_mit_leerer_paarung_behaelt_aufruf_und_platz() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.call(aufruf(11, None, 2));
        let mut leer = spiel(10, MatchStatus::Scheduled);
        leer.team1.clear();
        store.reconcile(&snap(vec![leer.clone(), spiel(11, MatchStatus::Scheduled)]));
        store.reconcile(&snap(vec![leer, spiel(11, MatchStatus::Scheduled)]));
        // Paarung wieder da → Zähler von vorn, Platz unverändert.
        let gut = snap(vec![
            spiel(10, MatchStatus::Scheduled),
            spiel(11, MatchStatus::Scheduled),
        ]);
        assert!(store.reconcile(&gut).is_empty());
        assert_eq!(ids(&store), vec![10, 11]);
    }

    #[test]
    fn verschieben_vor_ein_ziel_und_ans_ende() {
        let store = MeetingpointStore::default();
        for id in [10, 11, 12] {
            store.call(aufruf(id, None, 1));
        }
        assert_eq!(store.verschiebe(12, Some(10)), Ok(true));
        assert_eq!(ids(&store), vec![12, 10, 11]);
        assert_eq!(store.verschiebe(12, None), Ok(true));
        assert_eq!(ids(&store), vec![10, 11, 12]);
        assert_eq!(store.verschiebe(12, None), Ok(false), "schon am Ende");
        assert_eq!(store.verschiebe(11, Some(11)), Ok(false), "vor sich selbst");
    }

    #[test]
    fn verschieben_meldet_jeden_fehler_und_aendert_nichts() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, Some(1), 1));
        store.call(aufruf(11, Some(1), 2));
        store.call(aufruf(20, Some(2), 3));
        store.call(aufruf(12, Some(1), 4));
        // 12 aufs Feld → geparkt
        store.reconcile(&snap(vec![
            spiel(10, MatchStatus::Scheduled),
            spiel(11, MatchStatus::Scheduled),
            spiel(20, MatchStatus::Scheduled),
            spiel(12, MatchStatus::OnCourt),
        ]));
        assert_eq!(
            store.verschiebe(99, None),
            Err(VerschiebeFehler::NichtGerufen)
        );
        assert_eq!(
            store.verschiebe(12, None),
            Err(VerschiebeFehler::NichtGerufen)
        );
        assert_eq!(
            store.verschiebe(11, Some(12)),
            Err(VerschiebeFehler::ZielNichtGerufen)
        );
        assert_eq!(
            store.verschiebe(11, Some(20)),
            Err(VerschiebeFehler::AndereHalle)
        );
        assert_eq!(ids(&store), vec![10, 11, 20]);
    }

    #[test]
    fn verschieben_behaelt_zeitpunkt_und_checkins_und_uebersteht_den_neustart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, None, 1000));
            store.call(aufruf(11, None, 2000));
            store.verschiebe(11, Some(10)).unwrap();
        }
        let neu = store_mit_datei(dir.path());
        assert_eq!(
            neu.active_calls(),
            vec![aufruf(11, None, 2000), aufruf(10, None, 1000)]
        );
    }

    #[test]
    fn eine_altdatei_wird_einmal_nach_dem_alten_schluessel_geordnet() {
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("meetingpoint.json");
        // Datei einer älteren Version: kein `order_v`, Reihenfolge [11, 10].
        let alt = serde_json::json!({
            "tournament": TURNIER,
            "calls": [
                {"match_id": 11, "called_at_ms": 1},
                {"match_id": 10, "called_at_ms": 2},
            ],
        });
        std::fs::write(&pfad, alt.to_string()).unwrap();
        let store = store_mit_datei(dir.path());
        // Bisheriger Schlüssel stellte 10 vor 11.
        store.ordne_einmalig(|id| id);
        assert_eq!(ids(&store), vec![10, 11]);
        // Ein zweites Mal ordnet nichts mehr — auch nicht nach Neustart.
        store.verschiebe(11, Some(10)).unwrap();
        store.ordne_einmalig(|id| id);
        assert_eq!(ids(&store), vec![11, 10]);
        let neu = store_mit_datei(dir.path());
        neu.ordne_einmalig(|id| id);
        assert_eq!(ids(&neu), vec![11, 10]);
    }

    #[test]
    fn die_migration_sieht_den_praefix_schon_im_ersten_abruf() {
        // Review-Fund 08.11.2026: Im ersten Abruf nach dem Start war der
        // Präfix-Speicher noch nicht geladen — die Ordnung sah keinen Rang.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("meetingpoint.json"),
            serde_json::json!({
                "tournament": TURNIER,
                "calls": [
                    {"match_id": 10, "called_at_ms": 1},
                    {"match_id": 11, "called_at_ms": 2},
                ],
            })
            .to_string(),
        )
        .unwrap();
        // 11 war von Hand vor 10 gezogen (Präfix-Rang 0).
        std::fs::write(
            dir.path().join("queue-order.json"),
            serde_json::json!({"tournament": TURNIER, "queue": [11, 10]}).to_string(),
        )
        .unwrap();
        let t = TabletState::default();
        t.set_meetingpoint_path(dir.path().join("meetingpoint.json"));
        t.set_queue_order_path(dir.path().join("queue-order.json"));
        let mut s = snap(vec![
            spiel(10, MatchStatus::Scheduled),
            spiel(11, MatchStatus::Scheduled),
        ]);
        s.matches[0].planned_time = Some(100);
        s.matches[1].planned_time = Some(200);
        t.binde_meetingpoint(&s);
        assert_eq!(ids(t.meetingpoint_store()), vec![11, 10]);
    }

    #[test]
    fn eine_neue_datei_braucht_keine_ordnung() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(11, None, 1));
            store.call(aufruf(10, None, 2));
        }
        let neu = store_mit_datei(dir.path());
        neu.ordne_einmalig(|id| id);
        assert_eq!(ids(&neu), vec![11, 10], "Aufrufreihenfolge bleibt");
    }

    #[test]
    fn ein_aufs_feld_gerufenes_spiel_bleibt_kurz_als_aufgerufen_stehen() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.call(aufruf(11, None, 2));
        store.reconcile(&snap(vec![
            spiel(10, MatchStatus::OnCourt),
            spiel(11, MatchStatus::Scheduled),
        ]));
        let jetzt = crate::tablet::monitor::now_ms();
        let frist = relay_proto::MP_AUFGERUFEN_ANZEIGE_MS;
        let k: Vec<i64> = store
            .kuerzlich_aufgerufen(jetzt, frist)
            .iter()
            .map(|c| c.match_id)
            .collect();
        assert_eq!(k, vec![10]);
        assert!(store.kuerzlich_aufgerufen(jetzt + frist, frist).is_empty());
    }

    #[test]
    fn eine_ergebniskorrektur_holt_den_aufruf_nicht_zurueck() {
        let store = MeetingpointStore::default();
        store.call(aufruf(10, None, 1));
        store.reconcile(&snap(vec![spiel(10, MatchStatus::Finished)]));
        for _ in 0..DEASSIGN_CONFIRM_POLLS {
            store.reconcile(&snap(vec![spiel(10, MatchStatus::Scheduled)]));
        }
        assert!(store.active_calls().is_empty());
    }

    #[test]
    fn ein_geparkter_aufruf_ueberlebt_den_neustart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store_mit_datei(dir.path());
            store.call(aufruf(10, None, 1));
            store.reconcile(&snap(vec![spiel(10, MatchStatus::OnCourt)]));
        }
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.phase(10), Some(Phase::Geparkt));
        assert!(neu.active_calls().is_empty());
    }

    #[test]
    fn eine_unbekannte_phase_gilt_nicht_als_aktiv() {
        let file: MeetingpointFile = serde_json::from_str(
            r#"{"tournament":"T","calls":[{"match_id":1,"called_at_ms":5,"phase":"kuenftig"}]}"#,
        )
        .unwrap();
        assert_eq!(file.calls[0].phase, Phase::Unbekannt);
    }

    #[test]
    fn ein_zweiter_start_ohne_datei_verwirft_nichts() {
        // `set_path` erneut (zweites `start_sync`), Datei fehlt: Der Stand im
        // RAM gehört zum selben Turnier und darf nicht verschwinden.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.call(aufruf(10, None, 1));
        std::fs::remove_file(dir.path().join("meetingpoint.json")).unwrap();
        store.set_path(dir.path().join("meetingpoint.json"));
        store.set_tournament(TURNIER);
        assert_eq!(ids(&store), vec![10]);
    }

    #[test]
    fn ohne_turnier_wird_nichts_geschrieben() {
        let dir = tempfile::tempdir().unwrap();
        let store = MeetingpointStore::default();
        store.set_path(dir.path().join("meetingpoint.json"));
        store.call(aufruf(10, None, 1));
        assert!(!dir.path().join("meetingpoint.json").exists());
    }

    // ───────────── Dienste: Check-in, Prüfung, Bereitschaft ─────────────

    use crate::btp::model::{BtpLocation, BtpOfficial};
    use crate::config::{AppConfig, AutoCallConfig, AutoCallMode, MeetingpointConfig};
    use relay_proto::{MpCheckinBody, MpOrt};

    fn sp(id: i64, name: &str) -> BtpPlayer {
        BtpPlayer {
            id,
            name: name.into(),
            first: String::new(),
            last: name.into(),
            member_id: Some(format!("LIZ-{id}")),
            nationality: Some("GER".into()),
            club: Some("TSV Geheim".into()),
        }
    }

    /// Doppel 10 (Anna/Bea gegen Cem/Dora), Einzel 11 (Emil gegen Fritz).
    fn turnier(multi: bool) -> BtpSnapshot {
        let mut d = spiel(10, MatchStatus::Scheduled);
        d.team1 = vec![sp(1, "Anna"), sp(2, "Bea")];
        d.team2 = vec![sp(3, "Cem"), sp(4, "Dora")];
        let mut e = spiel(11, MatchStatus::Scheduled);
        e.team1 = vec![sp(5, "Emil")];
        e.team2 = vec![sp(6, "Fritz")];
        let mut s = snap(vec![d, e]);
        if multi {
            s.locations = vec![
                BtpLocation {
                    id: 1,
                    name: "Halle A".into(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle B".into(),
                },
            ];
        }
        s
    }

    fn mp_an() -> MeetingpointConfig {
        MeetingpointConfig {
            enabled: true,
            ..Default::default()
        }
    }

    fn zustand(snap: BtpSnapshot) -> TabletState {
        let t = TabletState::default();
        t.set_snapshot(snap);
        t
    }

    fn rufe(t: &TabletState, match_id: i64, halle: Option<i64>) {
        t.add_preparation_call(aufruf(match_id, halle, 1_000));
    }

    fn body(halle: &str, match_id: i64, platz: MpPlatz, present: bool) -> MpCheckinBody {
        MpCheckinBody {
            halle: halle.into(),
            match_id,
            platz,
            present,
            ort: MpOrt::Meetingpoint,
        }
    }

    fn s(seite: u8, pos: u8) -> MpPlatz {
        MpPlatz::Spieler { seite, pos }
    }

    fn bereit(t: &TabletState, cfg: &MeetingpointConfig) -> HashSet<i64> {
        bereite_partien(t, &t.snapshot_clone().unwrap(), cfg)
    }

    #[test]
    fn alle_spieler_eingecheckt_heisst_spielbereit() {
        let t = zustand(turnier(false));
        let cfg = mp_an();
        rufe(&t, 11, None);
        checkin(&t, &cfg, &body("", 11, s(1, 0), true), 5).unwrap();
        assert!(bereit(&t, &cfg).is_empty(), "einer fehlt noch");
        checkin(&t, &cfg, &body("", 11, s(2, 0), true), 6).unwrap();
        assert_eq!(bereit(&t, &cfg), HashSet::from([11]));
        // Zweiter Tipp checkt aus.
        checkin(&t, &cfg, &body("", 11, s(2, 0), false), 7).unwrap();
        assert!(bereit(&t, &cfg).is_empty());
    }

    #[test]
    fn die_pruefung_lehnt_alles_ab_was_nicht_passt() {
        let t = zustand(turnier(true));
        let cfg = mp_an();
        rufe(&t, 10, Some(1)); // Halle A
                               // Falsche Halle (AK 10).
        assert!(checkin(&t, &cfg, &body("Halle B", 10, s(1, 0), true), 1).is_err());
        // Keine Halle im Mehr-Hallen-Turnier.
        assert!(checkin(&t, &cfg, &body("", 10, s(1, 0), true), 1).is_err());
        // Nicht gerufen (AK 11).
        assert!(checkin(&t, &cfg, &body("Halle A", 11, s(1, 0), true), 1).is_err());
        // Platz gibt es nicht (Einzel hat keine Position 1; Seite 3).
        assert!(checkin(&t, &cfg, &body("Halle A", 10, s(3, 0), true), 1).is_err());
        assert!(checkin(&t, &cfg, &body("Halle A", 10, MpPlatz::Unbekannt, true), 1).is_err());
        // SR ohne eingeteilten SR.
        assert!(checkin(&t, &cfg, &body("Halle A", 10, MpPlatz::Sr, true), 1).is_err());
        // Ausgeschaltet (AK 17).
        let aus = MeetingpointConfig::default();
        assert!(checkin(&t, &aus, &body("Halle A", 10, s(1, 0), true), 1).is_err());
        // Und richtig: Groß-/Kleinschreibung der Halle egal.
        assert!(checkin(&t, &cfg, &body("halle a", 10, s(1, 0), true), 1).is_ok());
        assert!(bereit(&t, &cfg).is_empty(), "nichts falsch gebucht");
    }

    #[test]
    fn neuaufruf_gleiche_halle_behaelt_andere_halle_verwirft_die_checkins() {
        let t = zustand(turnier(true));
        let cfg = mp_an();
        rufe(&t, 11, Some(1));
        checkin(&t, &cfg, &body("Halle A", 11, s(1, 0), true), 1).unwrap();
        checkin(&t, &cfg, &body("Halle A", 11, s(2, 0), true), 1).unwrap();
        rufe(&t, 11, Some(1));
        assert_eq!(
            bereit(&t, &cfg),
            HashSet::from([11]),
            "gleiche Halle behält"
        );
        rufe(&t, 11, Some(2));
        assert!(bereit(&t, &cfg).is_empty(), "andere Halle beginnt neu");
    }

    #[test]
    fn ruecknahme_loescht_die_checkins() {
        let t = zustand(turnier(false));
        let cfg = mp_an();
        rufe(&t, 11, None);
        checkin(&t, &cfg, &body("", 11, s(1, 0), true), 1).unwrap();
        checkin(&t, &cfg, &body("", 11, s(2, 0), true), 1).unwrap();
        t.remove_preparation_call(11);
        rufe(&t, 11, None);
        assert!(bereit(&t, &cfg).is_empty());
    }

    #[test]
    fn ein_ersatzspieler_aus_btp_ist_nicht_eingecheckt() {
        let t = zustand(turnier(false));
        let cfg = mp_an();
        rufe(&t, 11, None);
        checkin(&t, &cfg, &body("", 11, s(1, 0), true), 1).unwrap();
        checkin(&t, &cfg, &body("", 11, s(2, 0), true), 1).unwrap();
        let mut neu = turnier(false);
        neu.matches[1].team2 = vec![sp(9, "Gerd")];
        t.set_snapshot(neu.clone());
        assert!(bereit(&t, &cfg).is_empty(), "Gerd hat nicht eingecheckt");
        let call = t.meetingpoint_store().active_entries()[0].clone();
        let b = bewerte(&neu.matches[1], &call, &SrLage::default());
        assert_eq!(b.fehlend, vec!["Gerd".to_string()]);
        // Der Abgleich räumt den alten Check-in weg.
        t.apply_preparation_calls(&mut neu);
        assert_eq!(t.meetingpoint_store().active_entries()[0].checkins.len(), 1);
    }

    #[test]
    fn wer_anderswo_aufs_feld_kommt_wird_ausgecheckt() {
        let t = zustand(turnier(false));
        let cfg = mp_an();
        rufe(&t, 11, None);
        checkin(&t, &cfg, &body("", 11, s(1, 0), true), 1).unwrap();
        checkin(&t, &cfg, &body("", 11, s(2, 0), true), 1).unwrap();
        // Emil (Seite 1 von 11) steht plötzlich in Spiel 12 auf dem Feld.
        let mut snap = turnier(false);
        let mut anderes = spiel(12, MatchStatus::OnCourt);
        anderes.court_id = Some(3);
        anderes.team1 = vec![sp(5, "Emil")];
        anderes.team2 = vec![sp(7, "Hans")];
        snap.matches.push(anderes);
        t.set_snapshot(snap.clone());
        t.apply_preparation_calls(&mut snap);
        let call = t.meetingpoint_store().active_entries()[0].clone();
        assert_eq!(call.checkins.len(), 1, "Emil ist hier ausgecheckt");
        assert!(bereit(&t, &cfg).is_empty());
    }

    #[test]
    fn die_seite_traegt_nur_namen_und_flags() {
        // Datenschutz (AK 13): keine Lizenznummer, kein Verein, keine Nation.
        let t = zustand(turnier(true));
        let cfg = mp_an();
        rufe(&t, 10, Some(1));
        checkin(&t, &cfg, &body("Halle A", 10, s(1, 1), true), 1).unwrap();
        let snap = t.snapshot_clone().unwrap();
        let v = view(&t, Some(&snap), &cfg, "Halle A");
        let json = serde_json::to_string(&v).unwrap();
        for verboten in ["LIZ-", "TSV Geheim", "GER", "p1", "p2"] {
            assert!(!json.contains(verboten), "{verboten} steht in {json}");
        }
        assert_eq!(v.partien.len(), 1);
        assert_eq!(v.partien[0].plaetze.len(), 4);
        assert!(v.partien[0].plaetze[1].present, "Bea eingecheckt");
        // Andere Halle sieht die Partie nicht (AK 9), unbekannte Halle wählt.
        assert!(view(&t, Some(&snap), &cfg, "Halle B").partien.is_empty());
        let wahl = view(&t, Some(&snap), &cfg, "Halle X");
        assert!(wahl.halle_waehlen && wahl.partien.is_empty());
        // Gesamtstand für den Relay, zugeschnitten wie am Host.
        let gesamt = view_alle_hallen(&t, Some(&snap), &cfg);
        assert_eq!(gesamt.fuer_halle("halle a").partien, v.partien);
        assert!(gesamt.fuer_halle("Halle B").partien.is_empty());
        assert!(gesamt.fuer_halle("").halle_waehlen);
    }

    fn seiten_ids(t: &TabletState, cfg: &MeetingpointConfig) -> Vec<i64> {
        let snap = t.snapshot_clone().unwrap();
        view(t, Some(&snap), cfg, "")
            .partien
            .iter()
            .map(|p| p.match_id)
            .collect()
    }

    #[test]
    fn die_seite_zeigt_die_aufruf_reihenfolge() {
        // ADR 0077: Spiel 10 steht in der Warteliste vor 11, wurde aber
        // später gerufen (seine Spieler standen noch auf dem Feld). Wer
        // zuerst gerufen wurde, kommt zuerst dran — und steht oben.
        let mut snap = turnier(false);
        snap.matches[0].planned_time = Some(100);
        snap.matches[1].planned_time = Some(200);
        let t = zustand(snap);
        let cfg = mp_an();
        t.add_preparation_call(aufruf(11, None, 1_000));
        t.add_preparation_call(aufruf(10, None, 2_000));
        assert_eq!(seiten_ids(&t, &cfg), vec![11, 10]);
        // Umsortieren der Gerufenen wirkt hier.
        t.meetingpoint_store().verschiebe(10, Some(11)).unwrap();
        assert_eq!(seiten_ids(&t, &cfg), vec![10, 11]);
    }

    #[test]
    fn der_aufruf_nimmt_das_spiel_aus_dem_praefix_der_warteliste() {
        // Regel 4: In der Warteliste nach oben gezogen heißt „als Nächstes
        // gerufen" — unter den Gerufenen stellt es sich hinten an.
        let t = zustand(turnier(false));
        let cfg = mp_an();
        t.queue_order_store().reorder(&[10, 11], 11, Some(10));
        assert_eq!(t.queue_order_store().rank(11), Some(0));
        t.add_preparation_call(aufruf(10, None, 1));
        t.add_preparation_call(aufruf(11, None, 2));
        assert_eq!(t.queue_order_store().rank(11), None, "Präfix weg");
        assert_eq!(seiten_ids(&t, &cfg), vec![10, 11]);
    }

    #[test]
    fn ein_mehrfachruf_reiht_sich_in_warteliste_reihenfolge_an() {
        let mut snap = turnier(false);
        let mut drittes = spiel(12, MatchStatus::Scheduled);
        drittes.team1 = vec![sp(7, "Gabi")];
        drittes.team2 = vec![sp(8, "Hugo")];
        snap.matches.push(drittes);
        snap.matches[0].planned_time = Some(100); // 10
        snap.matches[1].planned_time = Some(200); // 11
        snap.matches[2].planned_time = Some(300); // 12
        let t = zustand(snap);
        hand_aufruf(&t, &[12, 10, 11], None, 1, false);
        let ids: Vec<i64> = t.preparation_calls().iter().map(|c| c.match_id).collect();
        assert_eq!(ids, vec![10, 11, 12], "Warteliste, nicht Klickfolge");
    }

    #[test]
    fn die_seite_zeigt_ein_aufs_feld_gerufenes_spiel_mit_feld() {
        let t = zustand(turnier(false));
        let cfg = mp_an();
        rufe(&t, 10, None);
        rufe(&t, 11, None);
        let mut feld = turnier(false);
        feld.matches[0].status = MatchStatus::OnCourt;
        feld.matches[0].court = Some("3".into());
        feld.matches[0].court_id = Some(3);
        t.set_snapshot(feld.clone());
        t.apply_preparation_calls(&mut feld);
        let snap = t.snapshot_clone().unwrap();
        let v = view(&t, Some(&snap), &cfg, "");
        let karten: Vec<(i64, Option<String>)> = v
            .partien
            .iter()
            .map(|p| (p.match_id, p.aufgerufen_feld.clone()))
            .collect();
        assert_eq!(karten, vec![(10, Some("3".into())), (11, None)]);
    }

    #[test]
    fn wer_anderswo_auf_dem_feld_steht_kann_nicht_einchecken() {
        let mut snap = turnier(false);
        let mut anderes = spiel(12, MatchStatus::OnCourt);
        anderes.court_id = Some(3);
        anderes.team1 = vec![sp(5, "Emil")];
        anderes.team2 = vec![sp(7, "Hans")];
        snap.matches.push(anderes);
        let t = zustand(snap);
        let cfg = mp_an();
        rufe(&t, 11, None);
        let e = checkin(&t, &cfg, &body("", 11, s(1, 0), true), 1).unwrap_err();
        assert!(e.contains("anderen Feld"), "{e}");
        // Auschecken geht immer.
        assert!(checkin(&t, &cfg, &body("", 11, s(1, 0), false), 1).is_ok());
    }

    #[test]
    fn der_feld_check_wird_fuer_die_tl_gezaehlt() {
        let mut snap = turnier(false);
        snap.matches[1].status = MatchStatus::OnCourt;
        snap.matches[1].court_id = Some(1);
        let t = zustand(snap.clone());
        let cfg = MeetingpointConfig {
            enabled: true,
            field_check_enabled: true,
            ..Default::default()
        };
        let mut b = body("", 11, s(2, 0), true);
        b.ort = MpOrt::Feld;
        checkin(&t, &cfg, &b, 1).unwrap();
        assert_eq!(feld_check_zaehlung(&t, &snap.matches[1], &snap), [1, 2]);
    }

    // ───────────────────── Schiedsrichter (ADR 0072) ─────────────────────

    /// Fester Turniertag der Tests (die Spiele haben keine oder kleine
    /// Ansetzungen und reservieren deshalb immer).
    fn heute() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()
    }

    fn mit_sr(t: &TabletState, ids: &[i64]) -> BtpSnapshot {
        let mut snap = t.snapshot_clone().unwrap();
        snap.officials = ids
            .iter()
            .map(|id| BtpOfficial {
                id: *id,
                name: format!("SR{id}"),
                first: String::new(),
                nationality: None,
            })
            .collect();
        let store = t.officials_store();
        store.set_enabled(true);
        store.set_rotation(true, false);
        store.set_tournament(&snap.tournament_name);
        store.sync_roster(ids);
        t.set_snapshot(snap.clone());
        snap
    }

    #[test]
    fn die_seite_traegt_die_frische_prognose_und_verschweigt_alte() {
        let t = zustand(turnier(false));
        let snap = t.snapshot_clone().unwrap();
        let cfg = mp_an();
        rufe(&t, 10, None);
        rufe(&t, 11, None);
        let jetzt = crate::tablet::monitor::now_ms();
        // Nur Spiel 10 hat eine Prognose (11 z. B. mit Wunschfeld).
        t.set_mp_prognose(
            jetzt,
            [(10, (jetzt + 6 * 60_000, true))].into_iter().collect(),
        );
        let v = view(&t, Some(&snap), &cfg, "");
        let p = |id: i64| v.partien.iter().find(|p| p.match_id == id).unwrap();
        assert_eq!(p(10).feld_frei_ms, Some(jetzt + 6 * 60_000));
        assert!(p(10).feld_frei_unsicher);
        assert_eq!(p(11).feld_frei_ms, None, "ohne Prognose kein Countdown");
        assert!(!p(11).feld_frei_unsicher);
        // Ein Stand von vor über drei Minuten (Sync-Loop steht) zählt nicht.
        let alt = jetzt - crate::tablet::state::MP_PROGNOSE_MAX_ALTER_MS - 1;
        t.set_mp_prognose(alt, [(10, (jetzt + 60_000, false))].into_iter().collect());
        let v = view(&t, Some(&snap), &cfg, "");
        assert!(v.partien.iter().all(|p| p.feld_frei_ms.is_none()));
    }

    use crate::tablet::officials::OfficialRole as Rolle;

    fn besetzen(t: &TabletState, snap: &BtpSnapshot) -> Vec<i64> {
        gerufene_besetzen(t, snap, heute())
    }

    #[test]
    fn der_aufruf_teilt_den_sr_fest_zu() {
        // AK1 (ADR 0080): feste Einteilung mit Herkunfts-Merker; die Seite
        // zeigt den SR, ohne „vorläufig".
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        let cfg = mp_an();
        rufe(&t, 10, None);
        rufe(&t, 11, None);
        assert_eq!(besetzen(&t, &snap), vec![10, 11]);
        let store = t.officials_store();
        assert_eq!(store.assignment(10).sr, Some(100));
        assert_eq!(store.assignment(11).sr, Some(101));
        assert_eq!(store.vorab_rotation(10), (Some(100), None));
        let v = view(&t, Some(&snap), &cfg, "");
        assert!(v.partien[0]
            .plaetze
            .iter()
            .any(|p| p.platz == MpPlatz::Sr && p.name == "SR100"));
        let (_, sr, b) = &bewertungen(&t, &snap, &cfg)[0];
        assert!(!tl_meetingpoint(&bewertungen(&t, &snap, &cfg)[0].0, sr, b).sr_vorab);
        // Ein zweiter Lauf ändert nichts.
        assert!(besetzen(&t, &snap).is_empty());
    }

    #[test]
    fn sr_und_ar_folgen_den_rotationsschaltern() {
        // AK3
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101, 102]);
        t.officials_store().set_rotation(true, true);
        rufe(&t, 10, None);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(10).sr, Some(100));
        assert_eq!(t.officials_store().assignment(10).ar, Some(101));

        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100]);
        t.officials_store().set_rotation(false, false);
        rufe(&t, 10, None);
        assert!(besetzen(&t, &snap).is_empty());
        assert_eq!(t.officials_store().assignment(10), Default::default());
    }

    #[test]
    fn hand_aufruf_besetzt_vor_dem_ansage_auftrag_und_nur_am_master() {
        // AK1 + AK28
        let t = zustand(turnier(false));
        let _ = mit_sr(&t, &[100]);
        assert_eq!(hand_aufruf(&t, &[11], None, 1_000, true), 1);
        assert_eq!(t.officials_store().assignment(11).sr, Some(100));

        let t = zustand(turnier(false));
        let _ = mit_sr(&t, &[100]);
        assert_eq!(hand_aufruf(&t, &[11], None, 1_000, false), 0);
        assert_eq!(
            t.officials_store().assignment(11).sr,
            None,
            "am Slave keine Zuteilung"
        );
    }

    #[test]
    fn engpass_und_nachruecken_in_vergabe_reihenfolge() {
        // AK7: Wer frei wird, geht an die erste gerufene Partie mit Lücke in
        // Vergabe-Reihenfolge (hier: Aufruf-Rang 11 vor 10).
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100]);
        let mut laufend = spiel(12, MatchStatus::OnCourt);
        laufend.court_id = Some(1);
        laufend.official1_id = Some(100);
        snap.matches.push(laufend);
        t.set_snapshot(snap.clone());
        rufe(&t, 11, None);
        rufe(&t, 10, None);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, None, "100 im Dienst");
        snap.matches[2].status = MatchStatus::Finished;
        t.set_snapshot(snap.clone());
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, Some(100));
        assert_eq!(t.officials_store().assignment(10).sr, None);
    }

    #[test]
    fn die_zuteilung_folgt_rolle_und_modus() {
        // AK5 (ADR 0079): „nur AR" wird nie SR; in A kommt der SR aus der
        // SR-Liste; in C bekommt ihn, wer zuletzt nicht SR war.
        use crate::tablet::officials::{Einsatzrolle, Reihenfolge};
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        t.officials_store().set_role(100, Einsatzrolle::NurAr);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, Some(101));

        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        let store = t.officials_store();
        store.set_rotation_mode(crate::config::RotationMode::Getrennt);
        store.reorder_in(Reihenfolge::Ar, 101, Some(100));
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        assert_eq!(store.assignment(11).sr, Some(100));

        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100, 101]);
        let mut fertig = spiel(12, MatchStatus::Finished);
        fertig.official1_id = Some(100);
        fertig.finished_at = Some(5);
        snap.matches.push(fertig);
        t.set_snapshot(snap.clone());
        t.officials_store()
            .set_rotation_mode(crate::config::RotationMode::Rollenwechsel);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, Some(101));
    }

    #[test]
    fn hand_btp_und_ohne_werden_nicht_ersetzt() {
        // AK6
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100, 101, 102]);
        snap.matches[0].official1_id = Some(102); // BTP-Vorab an 10
        t.set_snapshot(snap.clone());
        let store = t.officials_store();
        store.clear_assignment(11, Rolle::Sr); // „ohne" an 11
        rufe(&t, 10, None);
        rufe(&t, 11, None);
        for _ in 0..3 {
            store.confirm(11, None, None);
            besetzen(&t, &snap);
        }
        assert_eq!(store.assignment(10).sr, None, "BTP-Vorab bleibt");
        assert_eq!(store.assignment(11).sr, Some(0), "„ohne\" bleibt");
    }

    #[test]
    fn die_zuteilung_ueberspringt_reservierte() {
        // ADR 0078: 100 steht in BTP vorab an Spiel 12 (wartend, heute).
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100, 101]);
        let mut vorab = spiel(12, MatchStatus::Scheduled);
        vorab.official1_id = Some(100);
        snap.matches.push(vorab);
        t.set_snapshot(snap.clone());
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, Some(101));
    }

    #[test]
    fn die_zuteilung_reserviert_auch_an_einem_spaeteren_tag() {
        // AK9: Eine gerufene Partie mit Merker reserviert tagesunabhängig.
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100]);
        snap.matches[1].planned_time = Some(203_001_011_000); // weit in der Zukunft
        t.set_snapshot(snap.clone());
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, Some(100));
        let b = crate::tablet::officials::Belegung::aus(t.officials_store(), &snap, heute());
        assert!(b.ist_belegt(100), "reserviert trotz späterem Tag");
    }

    #[test]
    fn ruecknahme_gibt_nur_rotations_zuteilungen_frei() {
        // AK12 + AK14
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        let store = t.officials_store();
        store.assign(11, Rolle::Ar, 101); // Hand-AR
        t.remove_preparation_call(11);
        assert_eq!(store.assignment(11).sr, None, "Rotation frei, kein Some(0)");
        assert_eq!(store.assignment(11).ar, Some(101), "Hand bleibt");
        assert!(!store.hat_vorab_rotation(11));
    }

    #[test]
    fn hand_einteilung_loescht_den_merker() {
        // AK12: Umteilen von Hand ⇒ ab jetzt Hand-Einteilung, Rücknahme lässt
        // sie stehen.
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        let store = t.officials_store();
        store.assign(11, Rolle::Sr, 101);
        assert!(!store.hat_vorab_rotation(11));
        t.remove_preparation_call(11);
        assert_eq!(store.assignment(11).sr, Some(101));
    }

    #[test]
    fn ein_geloeschter_aufruf_gibt_im_naechsten_lauf_frei() {
        // Aufruf weg ohne Rücknahme-Befehl (z. B. leere Paarung): der Lauf
        // gibt frei.
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100]);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        t.meetingpoint_store().retract(11);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, None);
    }

    #[test]
    fn ein_spiel_ohne_feld_beendet_behaelt_keine_nie_geschriebene_zuteilung() {
        // AK17
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100]);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        snap.matches[1].status = MatchStatus::Finished;
        t.set_snapshot(snap.clone());
        besetzen(&t, &snap);
        let store = t.officials_store();
        assert_eq!(store.assignment(11).sr, None);
        assert!(!store.hat_vorab_rotation(11));
    }

    #[test]
    fn halle_ohne_ar_schalter_bekommt_keinen_ar() {
        // AK4
        let t = zustand(turnier(true));
        let mut snap = mit_sr(&t, &[100, 101, 102]);
        snap.court_infos = vec![
            crate::btp::model::BtpCourt {
                id: 1,
                name: "1".into(),
                location_id: Some(1),
                sort_order: 1,
            },
            crate::btp::model::BtpCourt {
                id: 2,
                name: "2".into(),
                location_id: Some(2),
                sort_order: 1,
            },
        ];
        t.set_snapshot(snap.clone());
        let store = t.officials_store();
        store.set_rotation(true, true);
        let mut sw = store.court_switches(1);
        sw.ar = false;
        store.set_court_switches(1, sw);
        rufe(&t, 11, Some(1));
        besetzen(&t, &snap);
        assert_eq!(store.assignment(11).sr, Some(100));
        assert_eq!(store.assignment(11).ar, None, "Halle A will keinen AR");
        // Ohne Halle zählt irgendein Feld (Feld 2 will AR).
        rufe(&t, 10, None);
        besetzen(&t, &snap);
        assert_eq!(store.assignment(10).ar, Some(102), "Feld 2 will einen AR");
    }

    #[test]
    fn ein_alter_vorlaeufiger_sr_wird_einmalig_uebernommen() {
        // AK20
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        rufe(&t, 10, None);
        rufe(&t, 11, None);
        let mp = t.meetingpoint_store();
        mp.setze_vorab_sr_fuer_test(11, 101);
        mp.setze_vorab_sr_fuer_test(10, 999); // nicht in BTP ⇒ verfällt
        besetzen(&t, &snap);
        let store = t.officials_store();
        assert_eq!(store.assignment(11).sr, Some(101), "übernommen");
        assert_eq!(store.vorab_rotation(11), (Some(101), None));
        assert_eq!(store.assignment(10).sr, Some(100), "neu gewählt");
        assert!(
            mp.nimm_vorab_sr(|_| true).is_empty(),
            "nichts mehr gespeichert"
        );
    }

    #[test]
    fn ein_alter_vorlaeufiger_sr_ueberlebt_einen_leeren_abruf() {
        // Spiel fehlt kurz im Snapshot ⇒ der Eintrag bleibt für später.
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        rufe(&t, 11, None);
        t.meetingpoint_store().setze_vorab_sr_fuer_test(11, 101);
        let mut leer = snap.clone();
        leer.matches.clear();
        besetzen(&t, &leer);
        besetzen(&t, &snap);
        assert_eq!(t.officials_store().assignment(11).sr, Some(101));
    }

    #[test]
    fn vom_feld_genommen_behaelt_die_zuteilung() {
        // AK16: Das Spiel kommt aufs Feld (Aufruf geparkt) und wird wieder
        // genommen (Aufruf lebt auf) — die Rotations-Zuteilung bleibt.
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100]);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        snap.matches[1].status = MatchStatus::OnCourt;
        snap.matches[1].court_id = Some(1);
        t.apply_preparation_calls(&mut snap.clone());
        t.set_snapshot(snap.clone());
        besetzen(&t, &snap);
        assert_eq!(
            t.officials_store().assignment(11).sr,
            Some(100),
            "geparkt: bleibt"
        );
        snap.matches[1].status = MatchStatus::Scheduled;
        snap.matches[1].court_id = None;
        t.apply_preparation_calls(&mut snap.clone());
        t.set_snapshot(snap.clone());
        besetzen(&t, &snap);
        assert_eq!(
            t.officials_store().assignment(11).sr,
            Some(100),
            "vom Feld: bleibt"
        );
        assert!(t.officials_store().hat_vorab_rotation(11));
    }

    #[test]
    fn rotation_aus_nimmt_laufenden_spielen_nichts_weg() {
        // AK19 / Spec R-D.6
        let t = zustand(turnier(false));
        let mut snap = mit_sr(&t, &[100]);
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        snap.matches[1].status = MatchStatus::OnCourt;
        snap.matches[1].court_id = Some(1);
        snap.matches[1].official1_id = Some(100); // BTP hat übernommen
        t.set_snapshot(snap.clone());
        t.officials_store().set_rotation(false, false);
        besetzen(&t, &snap);
        let store = t.officials_store();
        assert_ne!(
            store.fuer_schreiben(11, Some(100), None).sr,
            Some(0),
            "keine 0 nach BTP"
        );
        assert!(store.hat_vorab_rotation(11));
        // Ein neuer Aufruf bekommt nichts mehr.
        rufe(&t, 10, None);
        besetzen(&t, &snap);
        assert_eq!(store.assignment(10).sr, None);
    }

    #[test]
    fn ein_sr_wechsel_entwertet_den_alten_sr_check_in() {
        // AK27
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100, 101]);
        let cfg = mp_an();
        rufe(&t, 11, None);
        besetzen(&t, &snap);
        checkin(&t, &cfg, &body("", 11, MpPlatz::Sr, true), 1).unwrap();
        let sr_da = |t: &TabletState| {
            t.meetingpoint_store().active_entries()[0]
                .checkins
                .iter()
                .any(|(p, _)| *p == MpPlatz::Sr)
        };
        assert!(sr_da(&t));
        t.officials_store().assign(11, Rolle::Sr, 101);
        t.apply_preparation_calls(&mut snap.clone());
        assert!(!sr_da(&t), "der alte SR-Check-in ist verfallen");
    }

    #[test]
    fn sr_pflicht_und_ohne_schiedsrichter() {
        let t = zustand(turnier(false));
        let snap = mit_sr(&t, &[100]);
        let cfg = MeetingpointConfig {
            enabled: true,
            require_umpire: true,
            ..Default::default()
        };
        rufe(&t, 11, None);
        gerufene_besetzen(&t, &snap, heute());
        checkin(&t, &cfg, &body("", 11, s(1, 0), true), 1).unwrap();
        checkin(&t, &cfg, &body("", 11, s(2, 0), true), 1).unwrap();
        assert!(bereit(&t, &cfg).is_empty(), "SR fehlt noch");
        checkin(&t, &cfg, &body("", 11, MpPlatz::Sr, true), 1).unwrap();
        assert_eq!(bereit(&t, &cfg), HashSet::from([11]));
        // Ohne Schiedsrichter: SR-Platz zählt nicht mehr.
        checkin(&t, &cfg, &body("", 11, MpPlatz::Sr, false), 1).unwrap();
        t.officials_store()
            .clear_assignment(11, crate::tablet::officials::OfficialRole::Sr);
        gerufene_besetzen(&t, &snap, heute());
        assert_eq!(t.officials_store().assignment(11).sr, Some(0));
        assert_eq!(bereit(&t, &cfg), HashSet::from([11]));
        // Schalter aus: Spieler genügen ohnehin.
        assert_eq!(bereit(&t, &mp_an()), HashSet::from([11]));
    }

    // ───────────────────── Aufruf-Automatik (ADR 0075) ───────────────────

    fn auto(mode: AutoCallMode) -> AutoCallConfig {
        AutoCallConfig {
            mode,
            fixed_per_hall: 2,
            horizon_minutes: 10,
            buffer: 1,
            cap_per_hall: 4,
        }
    }

    #[test]
    fn das_ziel_der_aufruf_automatik() {
        // AK 35: vier belegt, eines in 5 min frei → 1 + Puffer 1 = 2.
        assert_eq!(
            auto_ziel(
                AutoCallMode::Restzeit,
                &auto(AutoCallMode::Restzeit),
                &[5, 20, 30, 40]
            ),
            2
        );
        // AK 36: Turnierbeginn, sechs freie Felder, Deckel 4.
        assert_eq!(
            auto_ziel(
                AutoCallMode::Restzeit,
                &auto(AutoCallMode::Restzeit),
                &[0; 6]
            ),
            4
        );
        // Nichts wird in X Minuten frei → nichts rufen (auch kein Puffer).
        assert_eq!(
            auto_ziel(
                AutoCallMode::Restzeit,
                &auto(AutoCallMode::Restzeit),
                &[25, 30]
            ),
            0
        );
        assert_eq!(
            auto_ziel(AutoCallMode::Fest, &auto(AutoCallMode::Fest), &[]),
            2
        );
        assert_eq!(
            auto_ziel(AutoCallMode::Aus, &auto(AutoCallMode::Fest), &[0]),
            0
        );
    }

    fn cfg_auto_call(mode: AutoCallMode) -> AppConfig {
        AppConfig {
            meetingpoint: MeetingpointConfig {
                enabled: true,
                auto_call: auto(mode),
                ..Default::default()
            },
            ..AppConfig::default()
        }
    }

    #[test]
    fn feste_anzahl_ruft_bis_zum_ziel_und_nie_doppelt() {
        let t = zustand(turnier(false));
        let cfg = cfg_auto_call(AutoCallMode::Fest);
        let snap = t.snapshot_clone().unwrap();
        let rufe_jetzt = auto_aufrufe(&t, &snap, &cfg, 0);
        assert_eq!(rufe_jetzt.len(), 2);
        for (id, loc) in &rufe_jetzt {
            t.meetingpoint_store().call_mit(aufruf(*id, *loc, 0), true);
        }
        assert!(auto_aufrufe(&t, &snap, &cfg, 0).is_empty(), "Ziel erreicht");
        assert!(t
            .meetingpoint_store()
            .active_entries()
            .iter()
            .all(|c| c.auto));
    }

    #[test]
    fn die_aufruf_automatik_haelt_an_und_ueberspringt() {
        let t = zustand(turnier(false));
        let snap = t.snapshot_clone().unwrap();
        // Ausgeschaltet bzw. Meetingpoint aus → nichts.
        assert!(auto_aufrufe(&t, &snap, &cfg_auto_call(AutoCallMode::Aus), 0).is_empty());
        let mut aus = cfg_auto_call(AutoCallMode::Fest);
        aus.meetingpoint.enabled = false;
        assert!(auto_aufrufe(&t, &snap, &aus, 0).is_empty());
        // Ausgenommene Partie wird übersprungen.
        let cfg = cfg_auto_call(AutoCallMode::Fest);
        t.set_auto_assign_excluded(10, true);
        let ids: Vec<i64> = auto_aufrufe(&t, &snap, &cfg, 0)
            .into_iter()
            .map(|(m, _)| m)
            .collect();
        assert_eq!(ids, vec![11]);
    }

    #[test]
    fn im_mehrhallenbetrieb_ruft_die_automatik_nur_partien_mit_halle() {
        // Ohne Regel/Hand/Auto-Halle hat keine Partie eine Halle → nichts.
        let t = zustand(turnier(true));
        let snap = t.snapshot_clone().unwrap();
        assert!(auto_aufrufe(&t, &snap, &cfg_auto_call(AutoCallMode::Fest), 0).is_empty());
    }

    // ───────────────────────── Fehlende (B7) ─────────────────────────────

    #[test]
    fn fehlende_werden_genau_einmal_angesagt() {
        let t = zustand(turnier(false));
        let cfg = MeetingpointConfig {
            enabled: true,
            missing_announce_enabled: true,
            missing_announce_minutes: 5,
            ..Default::default()
        };
        rufe(&t, 11, None); // gerufen bei 1_000 ms
        checkin(&t, &cfg, &body("", 11, s(1, 0), true), 1).unwrap();
        let snap = t.snapshot_clone().unwrap();
        assert_eq!(
            fehlende_ansagen(&t, &snap, &cfg, 1_000 + 4 * 60_000),
            0,
            "zu früh"
        );
        assert_eq!(fehlende_ansagen(&t, &snap, &cfg, 1_000 + 5 * 60_000), 1);
        assert_eq!(
            fehlende_ansagen(&t, &snap, &cfg, 1_000 + 9 * 60_000),
            0,
            "nur einmal"
        );
        let jobs = t.announce_jobs_since("", 0, 1_000 + 5 * 60_000, true);
        assert!(jobs.iter().any(|j| matches!(&j.kind,
            crate::tablet::state::AnnounceJobKind::MeetingpointMissing { names, .. } if names == &vec!["Fritz".to_string()])));
    }

    // ─────────────────────── Feld-Check (Nr. 11) ─────────────────────────

    #[test]
    fn der_feld_check_gilt_nur_auf_dem_feld_und_mit_schalter() {
        let mut snap = turnier(false);
        snap.matches[1].status = MatchStatus::OnCourt;
        snap.matches[1].court_id = Some(1);
        let t = zustand(snap);
        let mut cfg = mp_an();
        let mut b = body("", 11, s(1, 0), true);
        b.ort = MpOrt::Feld;
        assert!(checkin(&t, &cfg, &b, 1).is_err(), "Schalter aus");
        cfg.field_check_enabled = true;
        checkin(&t, &cfg, &b, 1).unwrap();
        assert_eq!(t.meetingpoint_store().feld_checks(11).len(), 1);
        b.match_id = 10; // geplant, nicht auf dem Feld
        assert!(checkin(&t, &cfg, &b, 1).is_err());
        // Spielende räumt den Feld-Check.
        let mut fertig = t.snapshot_clone().unwrap();
        fertig.matches[1].status = MatchStatus::Finished;
        t.meetingpoint_store().reconcile(&fertig);
        assert!(t.meetingpoint_store().feld_checks(11).is_empty());
    }

    // ── Ansage beim Hand-Aufruf ──

    fn ansagen(t: &TabletState, halle: &str) -> Vec<i64> {
        t.announce_jobs_since(halle, 0, 10_000, true)
            .into_iter()
            .filter_map(|j| match j.kind {
                crate::tablet::state::AnnounceJobKind::MeetingpointCall { match_id } => {
                    Some(match_id)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ein_hand_aufruf_wird_angesagt() {
        let t = zustand(turnier(false));
        assert_eq!(hand_aufruf(&t, &[10, 11], None, 1_000, true), 2);
        assert_eq!(ids(t.meetingpoint_store()), vec![10, 11]);
        assert_eq!(ansagen(&t, ""), vec![10, 11]);
    }

    #[test]
    fn erneut_in_dieselbe_halle_gerufen_wird_nicht_wieder_angesagt() {
        let t = zustand(turnier(true));
        assert_eq!(hand_aufruf(&t, &[10], Some(1), 1_000, true), 1);
        assert_eq!(hand_aufruf(&t, &[10], Some(1), 2_000, true), 0);
        assert_eq!(ansagen(&t, "Halle A"), vec![10]);
        // Die Uhr des ersten Aufrufs bleibt.
        assert_eq!(t.meetingpoint_store().active_calls()[0].called_at_ms, 1_000);
    }

    #[test]
    fn in_eine_andere_halle_gerufen_wird_neu_angesagt() {
        let t = zustand(turnier(true));
        hand_aufruf(&t, &[10], Some(1), 1_000, true);
        assert_eq!(hand_aufruf(&t, &[10], Some(2), 2_000, true), 1);
        assert_eq!(ansagen(&t, "Halle A"), vec![10]);
        assert_eq!(ansagen(&t, "Halle B"), vec![10]);
    }

    #[test]
    fn ein_geparkter_aufruf_wird_beim_erneuten_rufen_wieder_angesagt() {
        let t = zustand(turnier(false));
        hand_aufruf(&t, &[10], None, 1_000, true);
        let mut feld = turnier(false);
        feld.matches[0].status = MatchStatus::OnCourt;
        t.meetingpoint_store().reconcile(&feld);
        assert!(ids(t.meetingpoint_store()).is_empty(), "geparkt");
        assert_eq!(hand_aufruf(&t, &[10], None, 2_000, true), 1);
        assert_eq!(ansagen(&t, ""), vec![10, 10]);
    }

    #[test]
    fn am_slave_wird_eingetragen_aber_nicht_angesagt() {
        let t = zustand(turnier(false));
        assert_eq!(hand_aufruf(&t, &[10], None, 1_000, false), 0);
        assert_eq!(ids(t.meetingpoint_store()), vec![10]);
        assert!(ansagen(&t, "").is_empty());
    }

    #[test]
    fn ein_spiel_auf_dem_feld_wird_nicht_angesagt() {
        let mut s = turnier(false);
        s.matches[0].status = MatchStatus::OnCourt;
        let t = zustand(s);
        assert_eq!(hand_aufruf(&t, &[10], None, 1_000, true), 0);
        assert!(ansagen(&t, "").is_empty());
    }
}
