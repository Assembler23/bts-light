//! Prüfungen der Feldvergabe — rein, ohne Netz und ohne Zustand.
//!
//! Bis hierher lagen diese Regeln ausschließlich in `sync.rs::auto_assign`,
//! also in der Schleife der **automatischen** Vergabe. Mit der
//! Turnierleitungs-Oberfläche vergeben mehrere Geräte gleichzeitig, deshalb
//! liegen sie jetzt hier: einmal formuliert, geteilt von der automatischen
//! Vergabe und dem kommenden Web-Pfad (Muster `process_result`, R5).
//! Details: `docs/features/turnierleitung-web.md`.
//!
//! **Was hier NICHT gilt.** Der Tauri-Command `commands::assign_court`
//! (Desktop-Oberfläche) benutzt diese Prüfungen bewusst **nicht** — er
//! bleibt unverändert, damit die Extraktion den turniererprobten Stand nicht
//! anfasst. Er prüft daher weiterhin nur die Hallen-Regel; die Hallen-Regel
//! steht deshalb an zwei Stellen. Wer den Desktop-Pfad später hierher
//! umzieht, räumt diese Doppelung mit auf.
//!
//! **Bewusst asymmetrisch zur Automatik:** Die automatische Vergabe ist
//! konservativ, weil sie ohne Aufsicht nach BTP schreibt — sie verlangt im
//! Mehr-Hallen-Betrieb einen Aufruf für genau diese Halle und respektiert
//! die Spieler-Pause. Ein Mensch darf beides übergehen; das ist der Sinn
//! der manuellen Vergabe. Was ein Mensch **nicht** darf, ist ein laufendes
//! Spiel verdrängen oder jemanden auf zwei Felder gleichzeitig stellen.

use std::collections::{HashMap, HashSet};

use crate::btp::model::{BtpMatch, BtpPlayer, BtpSnapshot, MatchStatus};
use crate::config::AppConfig;
use crate::tablet::queue_order::QueueOrderStore;
use relay_proto::CourtExpectation;

/// Stabiler Schlüssel zur Spieler-Identität für die Verfügbarkeitsprüfung:
/// bevorzugt die Lizenznummer (`member_id`), sonst der normalisierte Name. So
/// greift die Prüfung auch über Disziplinen hinweg (dieselbe Person hat je
/// Disziplin eine andere EntryID, aber dieselbe Lizenz).
///
/// Achtung: Ohne `member_id` (Turniere ohne Lizenzen) können zwei verschiedene
/// Spieler mit identischem Namen verschmelzen — in lizenzierten Turnieren ist
/// die `member_id` praktisch immer gesetzt, daher hier akzeptiert.
pub fn player_key(p: &BtpPlayer) -> String {
    match p
        .member_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id) => id.to_ascii_lowercase(),
        None => p.name.trim().to_ascii_lowercase(),
    }
}

/// Reihenfolge, in der Spiele auf Felder kommen — **ohne** manuellen Präfix:
/// manuell in die Vorbereitung gerufene zuerst, dann der BTP-Zeitplan von
/// oben nach unten, **dann die Ansetzungsreihenfolge des Turnierplans**,
/// zuletzt Spielnummer und ID. Spiele ohne Ansetzung landen am Ende ihrer
/// Gruppe.
///
/// Die Ansetzungsreihenfolge (Auslosung, `DrawID`) muss vor die Spielnummer:
/// In BTP tragen alle Spiele eines Zeitfensters dieselbe Zeit — ein ganzer
/// Vormittag steht auf 9:00 —, und was darin zuerst drankommt, sagt allein
/// dieses Feld. Ohne es entschied die Spielnummer, und die läuft quer: Aus
/// der gedruckten Liste (Nr 2, 6, 2, 6 …) wurde „erst alle Nummer 2, dann
/// alle Nummer 6". Die Turnierleitung sah eine Reihenfolge, die in ihrem
/// Turnierplan nirgends steht.
///
/// **Kein** produktiver Aufrufer nutzt diese Funktion mehr direkt (Spec
/// `spielliste-manuelle-reihenfolge`, ADR 0023) — alle fünf Sortier-Stellen
/// gehen über [`resolve_and_sort_key`], das den manuellen Präfix davor
/// schaltet und intern auf [`sort_key_parts`] aufbaut. `sort_key` bleibt als
/// eigenständige, weiterhin genutzte Basis-Definition bestehen: für die
/// BTP-Mess-Sonden (`tests/btp_location_probe.rs`,
/// `tests/btp_displayorder_probe.rs`), die bewusst die reine BTP-Reihenfolge
/// ohne jede lokale Überschreibung zeigen sollen.
pub fn sort_key(m: &BtpMatch, called: bool) -> (bool, i64, i64, i64, i64) {
    sort_key_parts(called, m.planned_time, Some(m.draw_id), m.match_num, m.id)
}

/// Wie [`sort_key`], aber aus Einzelwerten — für Aufrufer, die kein
/// `BtpMatch` mehr zur Hand haben. Basis-Baustein von
/// [`sort_key_with_manual_order`] (siehe dort).
pub fn sort_key_parts(
    called: bool,
    planned_time: Option<i64>,
    draw_id: Option<i64>,
    match_num: Option<i64>,
    id: i64,
) -> (bool, i64, i64, i64, i64) {
    (
        !called,
        planned_time.unwrap_or(i64::MAX),
        draw_id.unwrap_or(i64::MAX),
        match_num.unwrap_or(i64::MAX),
        id,
    )
}

/// Rückgabetyp von [`sort_key_with_manual_order`]/[`resolve_and_sort_key`] —
/// benannt, damit ihn Aufrufer (an allen fünf Sortier-Stellen) nicht als
/// unbenanntes Sechs-Tupel wiederholen müssen.
pub type ManualOrderSortKey = (bool, usize, i64, i64, i64, i64);

/// Wie [`sort_key_parts`], aber mit einem zusätzlichen, davorgeschalteten
/// Sortierschlüssel: dem Rang im **manuellen Präfix** seiner Halle (Spec
/// `spielliste-manuelle-reihenfolge.md`, ADR 0023). `manual_rank` kommt aus
/// [`crate::tablet::queue_order::QueueOrderStore::rank`].
///
/// **Löst den Vorrang „gerufen vor Präfix" ohne eigene Fallunterscheidung:**
/// `!called` bleibt die erste Tupel-Komponente, `manual_rank` die zweite —
/// ein bereits gerufenes Spiel (`called = true`, `!called = false`) steht
/// damit immer vor jedem nicht gerufenen Spiel, unabhängig von dessen
/// Präfix-Rang, weil Tupel lexikografisch verglichen werden und `false <
/// true`. Ein Match ohne Präfix-Eintrag (`manual_rank = None`) fällt hinter
/// jeden Präfix-Eintrag zurück, behält aber unter den übrigen
/// nicht-eingereihten Spielen weiterhin BTPs eigene Reihenfolge.
pub fn sort_key_with_manual_order(
    called: bool,
    manual_rank: Option<usize>,
    planned_time: Option<i64>,
    draw_id: Option<i64>,
    match_num: Option<i64>,
    id: i64,
) -> ManualOrderSortKey {
    // Baut auf `sort_key_parts` auf (nicht nur begrifflich, auch im Code) —
    // der Präfix-Rang schiebt sich als zweite Komponente davor, der Rest
    // bleibt exakt die Basis-Definition.
    let (not_called, planned_time, draw_id, match_num, id) =
        sort_key_parts(called, planned_time, draw_id, match_num, id);
    (
        not_called,
        manual_rank.unwrap_or(usize::MAX),
        planned_time,
        draw_id,
        match_num,
        id,
    )
}

/// Bündelt Hallen-Auflösung + Präfix-Rang-Nachschlag + Schlüsselbau — der
/// **verpflichtende gemeinsame Helfer** für alle fünf Sortier-Stellen
/// (Blocker 4 der Klärung, ADR 0023): Ohne ihn müsste jede Stelle Halle und
/// Rang einzeln verketten, und genau diese Verkettung ist das Risiko, vor
/// dem `docs/btp_protocol.md` warnt.
///
/// Liefert Halle + Herkunft **mit zurück** (Code-Review-Fund 14.08.2026):
/// Aufrufer, die die Halle ohnehin brauchen (Gruppierung, Anzeige), riefen
/// sonst `hall_for_match` ein zweites Mal mit denselben Argumenten auf —
/// unnötige doppelte Locations-/Regel-Suche bei jedem Sortierlauf.
#[allow(clippy::too_many_arguments)]
pub fn resolve_and_sort_key(
    config: &AppConfig,
    snap: &BtpSnapshot,
    m: &BtpMatch,
    manual_hall: Option<&str>,
    called_hall: Option<&str>,
    auto_hall: Option<&str>,
    called: bool,
    order: &QueueOrderStore,
) -> (String, HallSource, ManualOrderSortKey) {
    let (hall, source) = hall_for_match(config, snap, m, manual_hall, called_hall, auto_hall);
    // Der Rang kommt aus EINER globalen Reihenfolge (ADR 0026) — die Halle
    // wird hier nur noch aufgelöst, weil die Aufrufer sie für die Anzeige
    // brauchen, nicht mehr für den Nachschlag.
    let rank = order.rank(m.id);
    let key = sort_key_with_manual_order(
        called,
        rank,
        m.planned_time,
        Some(m.draw_id),
        m.match_num,
        m.id,
    );
    (hall, source, key)
}

/// Die aktuelle, vollständig sortierte Match-ID-Liste des Turniers —
/// **nur** noch nicht gerufene, spielbereite Spiele (BTP-Reihenfolge +
/// bisheriger manueller Präfix kombiniert). Das ist die „aktuell gültige
/// Liste", auf die ein `QueueReorder`-Zug angewendet wird
/// ([`QueueOrderStore::reorder`]) — gerufene Spiele bleiben außen vor, sie
/// sind nie Teil des ziehbaren Bereichs (Blocker 1 der Klärung).
///
/// **Ohne Hallenfilter** (ADR 0026): Die Spielliste ist eine einzige
/// Abfolge über alle Hallen, ein Zug wirkt entsprechend global.
///
/// Geteilter Helfer für TL-Web-Dispatch (`tl.rs`) und den Desktop-Command
/// (`commands.rs`) — beide dürfen keine eigene Berechnung dieser Liste
/// haben, sonst könnte ein Zug auf zwei Oberflächen unterschiedlich
/// wirken.
pub fn ready_queue(
    config: &AppConfig,
    snap: &BtpSnapshot,
    manual_halls: &HashMap<i64, String>,
    auto_halls: &HashMap<i64, String>,
    called_match_ids: &HashSet<i64>,
    order: &QueueOrderStore,
) -> Vec<i64> {
    let mut ordered: Vec<(ManualOrderSortKey, i64)> = snap
        .matches
        .iter()
        // Auch Spiele mit noch offener Paarung gehören hierher (Spec
        // `tl-offene-paarungen`, ADR 0053): Diese Liste ist das Ziel jeder
        // Umsortierung, und was die Turnierleitung sieht, muss sie auch
        // greifen können. Stünde ein offenes Spiel nicht darin, verwürfe
        // `QueueOrderStore::reorder` den Zug still — und die Seite meldete
        // trotzdem Erfolg. Aufs Feld kommen sie deshalb nicht: Das verhindert
        // `check_assign` (MatchNotPlayable) und die Vergabe selbst.
        .filter(|m| m.status == MatchStatus::Scheduled && !called_match_ids.contains(&m.id))
        .map(|m| {
            let manual = manual_halls.get(&m.id).map(String::as_str);
            let auto = auto_halls.get(&m.id).map(String::as_str);
            let (_, _, key) =
                resolve_and_sort_key(config, snap, m, manual, None, auto, false, order);
            (key, m.id)
        })
        .collect();
    ordered.sort_by_key(|(key, _)| *key);
    ordered.into_iter().map(|(_, id)| id).collect()
}

/// Woher die Hallen-Angabe eines noch nicht vergebenen Spiels stammt.
///
/// Die Herkunft wird mitgeliefert, damit die Turnierleitung einschätzen kann,
/// wie belastbar die Angabe ist: Eine Turnier-Festlegung wiegt schwerer als
/// eine Tagesentscheidung, und „unbekannt" muss als solches erkennbar sein.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HallSource {
    /// Aus der Disziplin/Klasse→Halle-Regel (Turnier-Festlegung).
    Rule,
    /// Von der Turnierleitung für dieses eine Spiel gesetzt.
    Manual,
    /// **Aus BTP** (`Match.LocationID`) — der im Turnierplan hinterlegte
    /// Spielort. Nur vorhanden, wenn das Turnier ihn pflegt.
    Btp,
    /// Aus dem Vorbereitungs-Aufruf (Tagesentscheidung).
    Call,
    /// Von der **automatischen Hallen-Vorverteilung** gesetzt (Spec
    /// `hallen-vorverteilung`, ADR 0029) — die schwächste echte Quelle:
    /// jede andere Zuordnung schlägt sie, und Aufruf oder Hand-Eingriff
    /// räumen den Auto-Eintrag sogar aktiv.
    Auto,
    /// Nicht bekannt.
    None,
}

/// In welche Halle gehört ein noch nicht vergebenes Spiel — und woher wissen
/// wir das?
///
/// Kaskade: Disziplin-Regel → **von Hand gesetzt** → **BTP** →
/// Vorbereitungs-Aufruf → **automatische Vorverteilung** → unbekannt.
///
/// Die **Auto-Stufe** (Spec `hallen-vorverteilung`) steht bewusst ganz
/// hinten: Die Automatik füllt nur Spiele, die sonst gar keine Halle
/// hätten — wird später ein BTP-Ort nachgepflegt oder gerufen, gewinnt
/// das (der Aufruf räumt den Auto-Eintrag zusätzlich aktiv, E3).
///
/// - Die **Regel** gewinnt, weil sie eine Turnier-Festlegung ist und auch die
///   Vergabe bindet (`hall_allows_match`); ein widersprechender Ort stellte
///   das Spiel in eine Halle, in der es nie aufs Feld dürfte.
/// - Die **Hand** schlägt BTP: Wer den Ort während des Turniers eigens
///   umsetzt, disponiert um — das ist frischer als der Plan von heute früh.
/// - **BTP** schlägt den Aufruf: Der Turnierplan meint es ernster als ein
///   Aufruf, der die Halle nur nebenbei mitnimmt.
///
/// **Zur Geschichte dieser Kaskade** (damit niemand denselben Weg noch einmal
/// geht): Schritt 15 hatte an zwei Mitschnitten gemessen, dass ein
/// angesetztes Spiel *keinen* Spielort trägt, und daraus geschlossen, es
/// gebe ihn nicht. Beide Turniere pflegten die Spalte nur nicht. Am
/// 09.08.2026 an einem Turnier gemessen, das sie pflegt: **48 Matches mit
/// `Match.LocationID`**, die meisten ohne jede Feldzuweisung. Der Ort wird
/// jetzt gelesen — die abgeleiteten Quellen bleiben für Turniere, die ihn
/// nicht pflegen.
/// Die „von Hand gesetzte" Halle eines Spiels — aus dem **Wunschfeld** oder
/// aus der direkt gesetzten Halle.
///
/// Ein Wunschfeld (Spec `tl-wunschfeld`) legt die Halle mit fest: Es benennt
/// ein konkretes Feld, und das liegt nun einmal in genau einer Halle. Damit
/// kann der Widerspruch „Wunschfeld in Halle B, Hallenzuordnung sagt Halle A"
/// strukturell nicht entstehen — das Spiel würde sonst auf ein Feld warten,
/// das seine Hallenbindung ihm verbietet, und niemand sähe den Grund.
///
/// Der Wunsch zählt bewusst als **`Manual`** und bekommt keine eigene
/// Kaskadenstufe: Er *ist* ein Hand-Eingriff der Turnierleitung für dieses
/// eine Spiel — dieselbe Bedeutung, dieselbe Verbindlichkeit. Das erspart
/// eine neue Quelle auf der Wire, deren Bedeutung ältere Anzeigen nicht
/// kennten. Die Turnierleitung sieht das Wunschfeld ohnehin als eigene Marke.
///
/// Das Wunschfeld schlägt die direkt gesetzte Halle: Es ist die genauere
/// Angabe (ein Feld statt einer Halle) und in aller Regel die jüngere.
pub fn manual_hall_from_wish<'a>(
    snap: &'a BtpSnapshot,
    wish_court: Option<i64>,
    manual_hall: Option<&'a str>,
) -> Option<&'a str> {
    let aus_wunsch = wish_court
        .and_then(|c| snap.court_infos.iter().find(|ci| ci.id == c))
        .and_then(|ci| ci.location_id)
        .and_then(|lid| snap.locations.iter().find(|l| l.id == lid))
        .map(|l| l.name.as_str())
        .filter(|n| !n.trim().is_empty());
    aus_wunsch.or(manual_hall)
}

pub fn hall_for_match(
    config: &AppConfig,
    snap: &BtpSnapshot,
    m: &BtpMatch,
    manual_hall: Option<&str>,
    called_hall: Option<&str>,
    auto_hall: Option<&str>,
) -> (String, HallSource) {
    if let Some(hall) = config.allowed_hall_for(m.discipline.as_str(), &m.draw_name) {
        return (canonical_hall(snap, hall), HallSource::Rule);
    }
    fn gesetzt(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|s| !s.is_empty())
    }
    if let Some(hall) = gesetzt(manual_hall) {
        return (canonical_hall(snap, hall), HallSource::Manual);
    }
    // Der in BTP hinterlegte Spielort — sofern das Turnier ihn pflegt.
    if let Some(name) = m.location_id.and_then(|id| {
        snap.locations
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.name.trim().to_string())
    }) {
        if !name.is_empty() {
            return (name, HallSource::Btp);
        }
    }
    if let Some(hall) = gesetzt(called_hall) {
        return (canonical_hall(snap, hall), HallSource::Call);
    }
    match gesetzt(auto_hall) {
        Some(hall) => (canonical_hall(snap, hall), HallSource::Auto),
        None => (String::new(), HallSource::None),
    }
}

/// Übersetzt einen von Hand getippten Hallennamen in die Schreibweise, die
/// BTP führt.
///
/// Die Vergabe vergleicht Hallennamen ohne Rücksicht auf Groß- und
/// Kleinschreibung — eine Regel „halle b" belegt also korrekt Felder in
/// „Halle B". Die Anzeige darf das nicht ignorieren: Gäbe sie die getippte
/// Schreibweise aus, fände der Hallenfilter das Spiel nicht, und weil es
/// eine Halle *hat*, landete es auch nicht im Abschnitt „ohne
/// Hallenzuordnung" — es verschwände lautlos. Unbekannte Namen bleiben, wie
/// sie sind (dann stimmt wenigstens die Anzeige mit der Konfiguration
/// überein).
fn canonical_hall(snap: &BtpSnapshot, name: &str) -> String {
    let n = name.trim();
    snap.locations
        .iter()
        .find(|l| l.name.trim().eq_ignore_ascii_case(n))
        .map(|l| l.name.trim().to_string())
        .unwrap_or_else(|| n.to_string())
}

/// Warum ein Spiel gerade nicht aufs Feld kann.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// Mindestens ein Spieler steht gerade auf einem Feld.
    Playing {
        players: Vec<String>,
        /// [`player_key`]-Schlüssel **parallel** zu `players`: Die Anzeige
        /// färbt einzelne Namen — über den bloßen Namen verschmölzen zwei
        /// gleichnamige Spieler einer Paarung (Review-Fund 17.08.2026).
        player_keys: Vec<String>,
    },
    /// Mindestens ein Spieler ist noch in seiner Pause.
    Pause {
        /// Unix-ms, ab wann der Letzte wieder darf.
        until_ms: u64,
        players: Vec<String>,
        /// Siehe [`Blocked::Playing::player_keys`].
        player_keys: Vec<String>,
    },
}

/// Wer gerade spielt und wer noch pausiert — einmal aus dem BTP-Stand gebaut,
/// dann für beliebig viele Spiele abfragbar.
///
/// Die Regeln stammen aus der automatischen Feldvergabe; sie liegen hier,
/// damit die Anzeige exakt dieselbe Auskunft gibt wie die Automatik ihrer
/// Entscheidung zugrunde legt.
pub struct PlayerAvailability {
    /// Spieler, die gerade auf einem Feld stehen, samt dem Spiel, in dem sie
    /// stehen. Bewusst nur `OnCourt`: Ein beendetes Spiel hält sein Feld, gibt
    /// seine Spieler aber frei — die unterliegen dann der Pausenregel, nicht
    /// der harten Sperre.
    ///
    /// Das Spiel wird mitgeführt, damit ein laufendes Spiel sich beim
    /// Umhängen nicht selbst blockiert.
    busy: HashMap<String, i64>,
    /// Wann jeder Spieler frühestens wieder darf: das Maximum aus
    /// `Spielende + der bei DIESEM Spielende geltenden Pflichtpause` über
    /// alle beendeten Spiele des Spielers. Nur Einträge in der Zukunft
    /// wären interessant — gespeichert wird trotzdem alles, die Abfrage
    /// vergleicht ohnehin gegen `now`.
    ///
    /// Bewusst der fertige Zeitpunkt statt „Ende + globale Pause": Die
    /// Pausenlänge wird beim Spielende eingefroren (`BtpMatch::pause_ms`),
    /// damit eine geänderte Einstellung nicht rückwirkend alle schon
    /// beendeten Spiele umdatiert.
    free_at: HashMap<String, u64>,
}

/// Die aktuell geltende Pflichtpause in Millisekunden: Ein gesetzter Wert
/// in der Konfiguration (>0) schlägt BTP-Setting 1303; `0` = keine Pause.
///
/// **Die** eine Stelle, an der diese Regel steht — die Anzeige
/// (`tl::effective_rest_minutes`) nennt dieselbe Zahl in Minuten.
pub fn pflichtpause_ms(snap: &BtpSnapshot, config: &AppConfig) -> u64 {
    let mins = if config.auto_assign.pause_minutes > 0.0 {
        config.auto_assign.pause_minutes
    } else {
        snap.rest_minutes.unwrap_or(0) as f64
    };
    if mins.is_finite() && mins > 0.0 {
        (mins * 60_000.0) as u64
    } else {
        0
    }
}

impl PlayerAvailability {
    pub fn from_snapshot(snap: &BtpSnapshot, config: &AppConfig) -> Self {
        let busy: HashMap<String, i64> = snap
            .matches
            .iter()
            .filter(|m| m.court_id.is_some() && m.status == MatchStatus::OnCourt)
            .flat_map(|m| {
                m.team1
                    .iter()
                    .chain(m.team2.iter())
                    .map(|p| (player_key(p), m.id))
            })
            .collect();

        // Die aktuell eingestellte Pause gilt nur für Spiele, die ohne
        // eigenen Stempel dastehen (Altbestand aus einer Version vor
        // v0.9.253) — alle anderen bringen ihre eigene mit.
        let jetzt_gueltig = pflichtpause_ms(snap, config);

        let mut free_at: HashMap<String, u64> = HashMap::new();
        for m in snap
            .matches
            .iter()
            .filter(|m| m.status == MatchStatus::Finished)
        {
            let Some(end) = m.finished_at else { continue };
            let pause = m.pause_ms.unwrap_or(jetzt_gueltig);
            if pause == 0 {
                continue;
            }
            let frei = end.saturating_add(pause);
            for p in m.team1.iter().chain(m.team2.iter()) {
                let e = free_at.entry(player_key(p)).or_insert(0);
                *e = (*e).max(frei);
            }
        }

        Self { busy, free_at }
    }

    /// Steht diesem Spiel gerade jemand im Weg? `None` = spielbereit.
    ///
    /// „Spielt gerade" schlägt „pausiert": Wer auf dem Feld steht, ist auch
    /// nach Ablauf jeder Pause nicht verfügbar.
    pub fn blocked(&self, m: &BtpMatch, now_ms: u64) -> Option<Blocked> {
        // Spieler, die in einem ANDEREN Spiel auf dem Feld stehen. Das
        // eigene Spiel zählt nicht — sonst könnte ein laufendes Spiel nie
        // auf ein anderes Feld umgehängt werden.
        let playing: Vec<&BtpPlayer> = m
            .team1
            .iter()
            .chain(m.team2.iter())
            .filter(|p| {
                self.busy
                    .get(&player_key(p))
                    .is_some_and(|&in_match| in_match != m.id)
            })
            .collect();
        if !playing.is_empty() {
            return Some(Blocked::Playing {
                players: playing.iter().map(|p| p.name.clone()).collect(),
                player_keys: playing.iter().map(|p| player_key(p)).collect(),
            });
        }

        if self.free_at.is_empty() {
            return None;
        }
        let mut until = 0u64;
        let mut resting: Vec<&BtpPlayer> = Vec::new();
        for p in m.team1.iter().chain(m.team2.iter()) {
            if let Some(&frei) = self.free_at.get(&player_key(p)) {
                if now_ms < frei {
                    until = until.max(frei);
                    resting.push(p);
                }
            }
        }
        if resting.is_empty() {
            None
        } else {
            Some(Blocked::Pause {
                until_ms: until,
                players: resting.iter().map(|p| p.name.clone()).collect(),
                player_keys: resting.iter().map(|p| player_key(p)).collect(),
            })
        }
    }
}

/// Warum eine Vergabe abgelehnt wurde. Trägt den maschinenlesbaren Grund und
/// die Angaben, die eine verständliche Meldung braucht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignError {
    /// Das Feld ist belegt. `finished` unterscheidet den Fall, dass das
    /// Spiel schon vorbei ist und BTP das Feld nur noch nicht abgeräumt hat
    /// — auf dem Monitor sieht die Turnierleitung dann ein leeres Feld, und
    /// die Meldung muss das erklären statt zu verwirren.
    CourtTaken { by_match: i64, finished: bool },
    /// Das Feld ist leer, obwohl dort ein bestimmtes Spiel erwartet wurde.
    CourtFree,
    /// Das Feld ist von der Turnierleitung gesperrt.
    CourtLocked,
    /// Das Spiel steht bereits auf einem anderen Feld.
    MatchElsewhere { court_id: i64 },
    /// Das Spiel ist nicht spielbereit — schon beendet, schon auf dem Feld,
    /// oder die Paarung steht noch nicht fest.
    MatchNotPlayable,
    /// Mindestens ein Spieler steht gerade in einem anderen Spiel auf dem
    /// Feld. Anders als die Pausenregel ist das keine Ermessensfrage.
    PlayerOnCourt { players: Vec<String> },
    /// Disziplin/Klasse dürfen in dieser Halle nicht gespielt werden.
    HallNotAllowed,
    /// Das Spiel gibt es im aktuellen BTP-Stand nicht (mehr).
    UnknownMatch,
    /// Das Feld gibt es im aktuellen BTP-Stand nicht (mehr).
    UnknownCourt,
}

/// Welches Spiel belegt dieses Feld?
///
/// „Belegt" heißt: ein **noch nicht beendetes** Spiel referenziert das Feld.
///
/// Der Status muss mit hinein, und zwar aus einem Grund, der beim Lesen von
/// BTP nicht auffällt: Ein beendetes Match **behält** seine `CourtID` für
/// immer — als Turnier-Doku „wo wurde gespielt" (Tilo 19.07.2026, Vorbild
/// Original-BTS; `proto.rs` setzt sie beim Ergebnis ausdrücklich **nicht**
/// auf 0). Wer nur fragt „referenziert irgendein Spiel dieses Feld?", hält
/// es deshalb ab dem ersten beendeten Spiel bis zum Turnierende für besetzt.
/// Genau das ist am 09.08.2026 im Test aufgeschlagen: Feld 03 stand nach
/// einem Ergebnis dauerhaft auf „wird geräumt" und nahm nichts mehr an.
///
/// Die **Feld-Seite** gibt derselbe Schreibvorgang frei (Court ohne
/// MatchID) — physisch ist das Feld also frei, sobald das Ergebnis
/// geschrieben ist. Solange BTP das Match noch als laufend führt (Ergebnis
/// unterwegs), bleibt es hier zu Recht belegt.
pub fn court_occupied_by(snap: &BtpSnapshot, court_id: i64) -> Option<i64> {
    snap.matches
        .iter()
        .find(|m| m.court_id == Some(court_id) && !match_is_over(m))
        .map(|m| m.id)
}

/// Ist dieses Spiel durch? Beides zählt: der Status aus BTP und ein
/// eingetragener Sieger — ein kampflos gewertetes Spiel steht nie „auf dem
/// Feld", trägt aber einen Sieger.
fn match_is_over(m: &BtpMatch) -> bool {
    m.status == MatchStatus::Finished || m.winner.is_some()
}

/// Alle belegten Felder auf einmal — dieselbe Lesart wie
/// [`court_occupied_by`], nur als Menge für die automatische Vergabe, die
/// ohnehin über alle Felder läuft.
///
/// Beide Wege teilen sich diese Definition bewusst: Liefen sie auseinander,
/// vergäbe der eine Pfad ein Feld, das der andere für besetzt hält.
pub fn occupied_courts(snap: &BtpSnapshot) -> std::collections::HashSet<i64> {
    snap.matches
        .iter()
        .filter(|m| !match_is_over(m))
        .filter_map(|m| m.court_id)
        .collect()
}

/// Auf welchem Feld steht dieses Spiel?
pub fn match_on_court(snap: &BtpSnapshot, match_id: i64) -> Option<i64> {
    snap.matches
        .iter()
        .find(|m| m.id == match_id)
        .and_then(|m| m.court_id)
}

/// Trifft zu, was das Gerät vorgefunden zu haben glaubt?
///
/// `Any` heißt ausdrücklich „ich habe keine Erwartung" und trifft immer zu —
/// damit bleibt der bisherige Weg der Desktop-Oberfläche unverändert, die
/// den Feldzustand nie mitprüfte.
pub fn expectation_holds(actual: Option<i64>, expect: CourtExpectation) -> bool {
    match expect {
        CourtExpectation::Any => true,
        CourtExpectation::Free => actual.is_none(),
        CourtExpectation::Match { match_id } => actual == Some(match_id),
    }
}

/// Darf dieses Spiel auf dieses Feld?
///
/// Geprüft wird in dieser Reihenfolge: Existenz, Sperre, Erwartung an den
/// Feldzustand, „steht schon woanders", Hallenregel. **`Any` setzt nur die
/// Erwartung an den Feldzustand außer Kraft** — Sperre, Doppelvergabe und
/// Hallenregel gelten immer, denn sie hängen nicht daran, was das Gerät
/// gesehen hat.
pub fn check_assign(
    snap: &BtpSnapshot,
    config: &AppConfig,
    locked: &[i64],
    reserved: &[(i64, i64)],
    match_id: i64,
    court_id: i64,
    expect: CourtExpectation,
) -> Result<(), AssignError> {
    let m = snap
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .ok_or(AssignError::UnknownMatch)?;
    let court = snap
        .court_infos
        .iter()
        .find(|c| c.id == court_id)
        .ok_or(AssignError::UnknownCourt)?;

    // Spielbereit? Dieselbe Bedingung, nach der die automatische Vergabe ihre
    // Kandidaten auswählt: geplant und mit feststehender Paarung. Ohne sie
    // landete ein beendetes Spiel auf einem Feld, das danach niemand mehr
    // vergeben kann.
    if m.status != MatchStatus::Scheduled || m.team1.is_empty() || m.team2.is_empty() {
        return Err(AssignError::MatchNotPlayable);
    }

    if locked.contains(&court_id) {
        return Err(AssignError::CourtLocked);
    }

    // Was das Gerät gesehen hat, muss noch stimmen …
    let occupied = court_occupied_by(snap, court_id);
    if !expectation_holds(occupied, expect) {
        return Err(occupancy_error(snap, occupied));
    }
    // … und unabhängig davon muss das Feld frei sein. „Auf ein Feld legen"
    // heißt: auf ein FREIES Feld. Ein laufendes Spiel zu verdrängen ist eine
    // eigene Handlung (Umhängen) und darf nicht als Nebenwirkung passieren.
    if let Some(by_match) = occupied {
        if by_match != match_id {
            return Err(occupancy_error(snap, occupied));
        }
    }
    // Auch eine Zuweisung, die BTP noch nicht bestätigt hat, belegt das Feld
    // — im Schnappschuss ist sie bis zur Rückmeldung unsichtbar.
    if let Some(&(_, by_match)) = reserved.iter().find(|(c, _)| *c == court_id) {
        if by_match != match_id {
            return Err(AssignError::CourtTaken {
                by_match,
                finished: false,
            });
        }
    }

    // Steht das Spiel schon auf einem ANDEREN Feld?
    if let Some(other) = match_on_court(snap, match_id) {
        if other != court_id {
            return Err(AssignError::MatchElsewhere { court_id: other });
        }
    }

    // Spieler, die gerade in einem anderen Spiel auf dem Feld stehen. Die
    // Pausenregel bleibt bewusst außen vor: Sie darf die Turnierleitung
    // übergehen, „steht gerade auf einem Feld" nicht.
    let availability = PlayerAvailability::from_snapshot(snap, config);
    if let Some(Blocked::Playing { players, .. }) = availability.blocked(m, 0) {
        return Err(AssignError::PlayerOnCourt { players });
    }

    let court_hall = snap.court_location_name(court.id);
    if !config.hall_allows_match(m.discipline.as_str(), &m.draw_name, &court_hall) {
        return Err(AssignError::HallNotAllowed);
    }

    Ok(())
}

/// Baut den passenden Belegt-Fehler und sagt dabei, ob das belegende Spiel
/// schon vorbei ist — davon hängt ab, was die Oberfläche sinnvollerweise
/// anzeigt.
fn occupancy_error(snap: &BtpSnapshot, occupied: Option<i64>) -> AssignError {
    match occupied {
        Some(by_match) => AssignError::CourtTaken {
            by_match,
            finished: snap
                .matches
                .iter()
                .any(|m| m.id == by_match && m.status == MatchStatus::Finished),
        },
        None => AssignError::CourtFree,
    }
}

/// Darf dieses **laufende** Spiel auf jenes Feld umziehen?
///
/// Eigene Prüfung, weil zwei Bedingungen von [`check_assign`] hier
/// erlaubterweise verletzt sind: Das Spiel steht bereits auf einem Feld (dem
/// Quellfeld), und es ist nicht mehr „geplant", sondern läuft. Alles andere
/// gilt unverändert — Zielfeld frei, nicht gesperrt, Hallenregel erfüllt.
///
/// **Genau deshalb wird die Voraussetzung hier zuerst geprüft**: Das Spiel
/// muss wirklich auf dem Quellfeld stehen und wirklich laufen. Ohne diese
/// Prüfung wäre Umhängen ein Zuweisen ohne jede Kontrolle — man könnte ein
/// beliebiges, auch beendetes Spiel auf ein freies Feld setzen und dieses
/// damit dauerhaft unbelegbar machen.
///
/// Die Spieler-Prüfung entfällt bewusst: Sie stehen ja bereits auf dem
/// Quellfeld, und genau dieses Spiel soll umziehen.
#[allow(clippy::too_many_arguments)]
pub fn check_move_target(
    snap: &BtpSnapshot,
    config: &AppConfig,
    locked: &[i64],
    reserved: &[(i64, i64)],
    match_id: i64,
    from_court_id: i64,
    to_court_id: i64,
    expect_to: CourtExpectation,
) -> Result<(), AssignError> {
    if from_court_id == to_court_id {
        // Keine Bewegung — vermutlich ein Fehlgriff, und ein sinnloser
        // Schreibvorgang nach BTP.
        return Err(AssignError::MatchElsewhere {
            court_id: to_court_id,
        });
    }
    let m = snap
        .matches
        .iter()
        .find(|m| m.id == match_id)
        .ok_or(AssignError::UnknownMatch)?;

    // Die tragende Voraussetzung, siehe oben.
    if m.court_id != Some(from_court_id) {
        return Err(match m.court_id {
            Some(other) => AssignError::MatchElsewhere { court_id: other },
            None => AssignError::MatchNotPlayable,
        });
    }
    if m.status != MatchStatus::OnCourt {
        // Ein beendetes Spiel umzuhängen machte das Zielfeld dauerhaft
        // unbelegbar: Die Vergabe zählt es als besetzt, und niemand räumt
        // es je wieder ab.
        return Err(AssignError::MatchNotPlayable);
    }
    let court = snap
        .court_infos
        .iter()
        .find(|c| c.id == to_court_id)
        .ok_or(AssignError::UnknownCourt)?;

    if locked.contains(&to_court_id) {
        return Err(AssignError::CourtLocked);
    }

    let occupied = court_occupied_by(snap, to_court_id);
    if !expectation_holds(occupied, expect_to) {
        return Err(occupancy_error(snap, occupied));
    }
    if occupied.is_some() {
        return Err(occupancy_error(snap, occupied));
    }
    if let Some(&(_, by_match)) = reserved.iter().find(|(c, _)| *c == to_court_id) {
        if by_match != match_id {
            return Err(AssignError::CourtTaken {
                by_match,
                finished: false,
            });
        }
    }

    let court_hall = snap.court_location_name(court.id);
    if !config.hall_allows_match(m.discipline.as_str(), &m.draw_name, &court_hall) {
        return Err(AssignError::HallNotAllowed);
    }
    Ok(())
}

/// Darf dieses Feld geräumt werden?
///
/// Die Erwartung schützt davor, dass ein Gerät ein Feld freigibt, auf dem
/// inzwischen ein ganz anderes Spiel steht — was den laufenden Spielstand
/// eines Unbeteiligten verwerfen würde.
pub fn check_free(
    snap: &BtpSnapshot,
    court_id: i64,
    expect: CourtExpectation,
) -> Result<(), AssignError> {
    if !snap.court_infos.iter().any(|c| c.id == court_id) {
        return Err(AssignError::UnknownCourt);
    }
    let occupied = court_occupied_by(snap, court_id);
    if !expectation_holds(occupied, expect) {
        return Err(occupancy_error(snap, occupied));
    }
    Ok(())
}

/// Steht dieses Spiel gerade auf einem Feld und wird gespielt?
///
/// Bewusst enger als [`court_occupied_by`]: Ein beendetes Spiel hält sein
/// Feld, gibt seine Spieler aber frei — die unterliegen dann der Pausenregel,
/// nicht der harten Sperre.
pub fn is_on_court(snap: &BtpSnapshot, match_id: i64) -> bool {
    snap.matches
        .iter()
        .any(|m| m.id == match_id && m.court_id.is_some() && m.status == MatchStatus::OnCourt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::{
        BtpCourt, BtpLocation, BtpMatch, BtpPlayer, BtpSnapshot, Discipline, MatchResult,
        MatchStatus,
    };
    use crate::config::DisciplineHallRule;

    fn player(name: &str) -> BtpPlayer {
        BtpPlayer {
            id: 0,
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: None,
            club: None,
        }
    }

    fn a_match(id: i64) -> BtpMatch {
        BtpMatch {
            display_order: None,
            from1: None,
            from2: None,
            id,
            draw_id: 1,
            planning_id: id,
            draw_name: "HE".to_string(),
            discipline: Discipline::MensSingles,
            class_label: String::new(),
            round_name: "G1".to_string(),
            match_num: Some(id),
            planned_time: None,
            team1: vec![player("A")],
            team2: vec![player("B")],
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
            official1_id: None,
            official2_id: None,
            scoring: crate::btp::model::ScoringFormat::default(),
        }
    }

    /// Turnier ohne bekannte Hallen — dann bleibt ein getippter Hallenname
    /// so stehen, wie er konfiguriert wurde.
    fn empty_snap() -> BtpSnapshot {
        snap(Vec::new(), Vec::new(), Vec::new())
    }

    fn a_court(id: i64, location_id: Option<i64>) -> BtpCourt {
        BtpCourt {
            id,
            name: id.to_string(),
            location_id,
            sort_order: id,
        }
    }

    fn snap(courts: Vec<BtpCourt>, matches: Vec<BtpMatch>, locs: Vec<BtpLocation>) -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "T".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: locs,
            court_infos: courts,
            matches,
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
        }
    }

    #[test]
    fn a_free_court_has_no_occupant() {
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        assert_eq!(court_occupied_by(&s, 1), None);
    }

    #[test]
    fn a_finished_match_releases_its_court() {
        // Bis 09.08.2026 hielt ein beendetes Spiel sein Feld — „bis BTP es
        // abräumt". BTP räumt aber nie ab: Die CourtID bleibt als Doku am
        // Match stehen (siehe `court_occupied_by`). Aus „kurz warten" wurde
        // so „für immer besetzt".
        let mut done = a_match(7);
        done.status = MatchStatus::Finished;
        done.court_id = Some(1);
        let s = snap(vec![a_court(1, None)], vec![done], Vec::new());
        assert_eq!(court_occupied_by(&s, 1), None);
        assert!(!is_on_court(&s, 7), "beendet zählt nicht als spielend");
    }

    #[test]
    fn occupied_courts_matches_the_single_court_lookup() {
        // Beide Wege müssen dieselbe Definition von „belegt" benutzen —
        // die automatische Vergabe fragt die Menge ab, der Web-Pfad das
        // einzelne Feld. Liefen sie auseinander, vergäbe der eine Pfad ein
        // Feld, das der andere für besetzt hält.
        let mut running = a_match(7);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        let mut done = a_match(8);
        done.court_id = Some(2);
        done.status = MatchStatus::Finished;
        let s = snap(
            vec![a_court(1, None), a_court(2, None), a_court(3, None)],
            vec![running, done, a_match(9)],
            Vec::new(),
        );

        // Feld 2 hält ein **beendetes** Spiel — das belegt seit 09.08.2026
        // nicht mehr (die CourtID bleibt dort für immer stehen).
        let set = occupied_courts(&s);
        assert_eq!(set, std::collections::HashSet::from([1]));
        for court_id in [1, 2, 3] {
            assert_eq!(
                set.contains(&court_id),
                court_occupied_by(&s, court_id).is_some(),
                "Feld {court_id} wird unterschiedlich bewertet"
            );
        }
    }

    #[test]
    fn expectation_any_always_holds() {
        assert!(expectation_holds(None, CourtExpectation::Any));
        assert!(expectation_holds(Some(7), CourtExpectation::Any));
    }

    #[test]
    fn expectation_free_only_holds_on_an_empty_court() {
        assert!(expectation_holds(None, CourtExpectation::Free));
        assert!(!expectation_holds(Some(7), CourtExpectation::Free));
    }

    #[test]
    fn expectation_match_only_holds_for_that_exact_match() {
        let e = CourtExpectation::Match { match_id: 7 };
        assert!(expectation_holds(Some(7), e));
        assert!(!expectation_holds(Some(8), e));
        assert!(!expectation_holds(None, e));
    }

    /// Kurzform für die Tests: keine Sperren, keine Reservierungen.
    fn check(
        s: &BtpSnapshot,
        cfg: &AppConfig,
        match_id: i64,
        court_id: i64,
        expect: CourtExpectation,
    ) -> Result<(), AssignError> {
        check_assign(s, cfg, &[], &[], match_id, court_id, expect)
    }

    #[test]
    fn assigning_to_a_court_taken_meanwhile_is_rejected_and_names_the_match() {
        // Der Kern des Mehrbenutzer-Schutzes: Gerät B sah das Feld leer,
        // Gerät A war schneller. B bekommt eine Ablehnung, die sagt, WER
        // dort steht — sonst kann die Seite keine verständliche Meldung
        // bauen.
        let mut taken = a_match(9);
        taken.court_id = Some(1);
        taken.status = MatchStatus::OnCourt;
        let s = snap(vec![a_court(1, None)], vec![a_match(7), taken], Vec::new());
        let err = check(&s, &AppConfig::default(), 7, 1, CourtExpectation::Free).unwrap_err();
        assert_eq!(
            err,
            AssignError::CourtTaken {
                by_match: 9,
                finished: false
            }
        );
    }

    #[test]
    fn a_court_whose_match_is_finished_takes_the_next_game() {
        // Umgekehrt seit 09.08.2026: Früher wurde hier abgelehnt („wird
        // geräumt"), weil das beendete Spiel seine CourtID behält. Da BTP
        // sie nie entfernt, blockierte das jedes Feld dauerhaft, sobald
        // darauf einmal ein Spiel fertig geworden war.
        let mut done = a_match(9);
        done.court_id = Some(1);
        done.status = MatchStatus::Finished;
        let s = snap(vec![a_court(1, None)], vec![a_match(7), done], Vec::new());
        assert!(
            check(&s, &AppConfig::default(), 7, 1, CourtExpectation::Free).is_ok(),
            "das Feld ist wieder vergebbar"
        );
    }

    #[test]
    fn an_occupied_court_is_rejected_even_when_the_device_expected_that_occupant() {
        // „Spiel auf ein Feld legen" heißt: auf ein FREIES Feld. Wer ein
        // laufendes Spiel verdrängen will, nimmt das Umhängen — sonst
        // verlöre das verdrängte Spiel mitten im Satz seine Feldbindung,
        // ohne dass irgendjemand gewarnt wurde.
        let mut running = a_match(9);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        let s = snap(
            vec![a_court(1, None)],
            vec![a_match(7), running],
            Vec::new(),
        );
        for expect in [
            CourtExpectation::Any,
            CourtExpectation::Match { match_id: 9 },
        ] {
            assert!(
                check(&s, &AppConfig::default(), 7, 1, expect).is_err(),
                "belegtes Feld darf nie überschrieben werden, auch nicht mit {expect:?}"
            );
        }
    }

    #[test]
    fn a_court_reserved_by_a_write_btp_has_not_confirmed_yet_is_not_free() {
        // Zwischen dem Schreiben nach BTP und der Rückmeldung sieht der
        // Schnappschuss das Feld noch leer. Ohne diese Prüfung landete eine
        // zweite Zuweisung obendrauf, und die Spieler der ersten stünden vor
        // einem Feld, auf dem ein fremdes Spiel läuft.
        let s = snap(
            vec![a_court(1, None)],
            vec![a_match(7), a_match(8)],
            Vec::new(),
        );
        let reserved = [(1i64, 8i64)];
        let err = check_assign(
            &s,
            &AppConfig::default(),
            &[],
            &reserved,
            7,
            1,
            CourtExpectation::Free,
        )
        .unwrap_err();
        assert_eq!(
            err,
            AssignError::CourtTaken {
                by_match: 8,
                finished: false
            }
        );
        // Dasselbe Spiel erneut auf dasselbe reservierte Feld ist in Ordnung
        // (Wiederholung), sonst scheiterte ein berechtigter zweiter Versuch.
        assert!(check_assign(
            &s,
            &AppConfig::default(),
            &[],
            &reserved,
            8,
            1,
            CourtExpectation::Free
        )
        .is_ok());
    }

    #[test]
    fn a_match_that_is_not_playable_is_rejected() {
        // Die automatische Vergabe nimmt nur Spiele mit Status „geplant" und
        // feststehender Paarung. Ohne dieselbe Prüfung könnte die Oberfläche
        // ein beendetes Spiel auf ein Feld legen — BTP zeigte dort ein
        // fertiges Spiel, und die automatische Vergabe hielte das Feld für
        // belegt, ohne es je wieder zu vergeben.
        let mut done = a_match(7);
        done.status = MatchStatus::Finished;
        let s = snap(vec![a_court(1, None)], vec![done], Vec::new());
        assert_eq!(
            check(&s, &AppConfig::default(), 7, 1, CourtExpectation::Free).unwrap_err(),
            AssignError::MatchNotPlayable
        );

        let mut open = a_match(8);
        open.team2 = Vec::new(); // Gegner steht noch nicht fest
        let s2 = snap(vec![a_court(1, None)], vec![open], Vec::new());
        assert_eq!(
            check(&s2, &AppConfig::default(), 8, 1, CourtExpectation::Free).unwrap_err(),
            AssignError::MatchNotPlayable
        );
    }

    #[test]
    fn a_match_whose_player_is_currently_on_another_court_is_rejected() {
        // Physisch unmöglich, nicht Ermessenssache: Müller kann nicht auf
        // zwei Feldern gleichzeitig stehen.
        let mut running = a_match(9);
        running.court_id = Some(2);
        running.status = MatchStatus::OnCourt;
        running.team1 = vec![player("Müller")];
        running.team2 = vec![player("Gegner")];
        let mut wanted = a_match(7);
        wanted.team1 = vec![player("Müller")];
        wanted.team2 = vec![player("Frei")];

        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![wanted, running],
            Vec::new(),
        );
        assert_eq!(
            check(&s, &AppConfig::default(), 7, 1, CourtExpectation::Free).unwrap_err(),
            AssignError::PlayerOnCourt {
                players: vec!["Müller".to_string()]
            }
        );
    }

    #[test]
    fn a_player_still_in_their_break_does_not_block_a_manual_assignment() {
        // Bewusst asymmetrisch zur Automatik: Die Turnierleitung darf die
        // Pause übergehen (der Spieler sagt, er kann sofort). Die Anzeige
        // weist darauf hin, das Tor lehnt nicht ab.
        let mut done = a_match(9);
        done.status = MatchStatus::Finished;
        done.finished_at = Some(100_000);
        done.team1 = vec![player("Müller")];
        done.team2 = vec![player("Gegner")];
        let mut wanted = a_match(7);
        wanted.team1 = vec![player("Müller")];
        wanted.team2 = vec![player("Frei")];

        let s = snap(vec![a_court(1, None)], vec![wanted, done], Vec::new());
        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 10.0;
        assert!(check(&s, &cfg, 7, 1, CourtExpectation::Free).is_ok());
    }

    #[test]
    fn assigning_to_a_locked_court_is_rejected_regardless_of_expectation() {
        // Die Sperre hängt nicht daran, was das Gerät gesehen hat — sie gilt
        // auch ohne Erwartung.
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        for expect in [CourtExpectation::Any, CourtExpectation::Free] {
            assert_eq!(
                check_assign(&s, &AppConfig::default(), &[1], &[], 7, 1, expect).unwrap_err(),
                AssignError::CourtLocked
            );
        }
    }

    #[test]
    fn assigning_a_match_that_already_runs_on_another_court_is_rejected() {
        // Ein laufendes Spiel ist nicht mehr „geplant" — die
        // Eignungsprüfung greift zuerst und sagt genau das.
        let mut elsewhere = a_match(7);
        elsewhere.court_id = Some(2);
        elsewhere.status = MatchStatus::OnCourt;
        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![elsewhere],
            Vec::new(),
        );
        assert_eq!(
            check(&s, &AppConfig::default(), 7, 1, CourtExpectation::Free).unwrap_err(),
            AssignError::MatchNotPlayable
        );
    }

    #[test]
    fn assigning_against_the_hall_rule_is_rejected() {
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "Halle A".to_string(),
        });
        let s = snap(
            vec![a_court(1, Some(2))],
            vec![a_match(7)],
            vec![
                BtpLocation {
                    id: 1,
                    name: "Halle A".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle B".to_string(),
                },
            ],
        );
        assert_eq!(
            check(&s, &cfg, 7, 1, CourtExpectation::Free).unwrap_err(),
            AssignError::HallNotAllowed
        );
    }

    #[test]
    fn unknown_match_or_court_is_rejected_before_anything_else() {
        let s = snap(vec![a_court(1, None)], vec![a_match(7)], Vec::new());
        assert_eq!(
            check(&s, &AppConfig::default(), 99, 1, CourtExpectation::Free).unwrap_err(),
            AssignError::UnknownMatch
        );
        assert_eq!(
            check(&s, &AppConfig::default(), 7, 99, CourtExpectation::Free).unwrap_err(),
            AssignError::UnknownCourt
        );
    }

    #[test]
    fn hall_comes_from_the_discipline_rule_when_one_is_configured() {
        // Die Disziplin/Klasse→Halle-Regel ist die einzige Quelle, die
        // *heute* schon für ungerufene Spiele existiert — und sie deckt
        // sortenreine Turniere („Damen in Halle 2") vollständig ab.
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "Halle A".to_string(),
        });
        let m = a_match(7);
        assert_eq!(
            hall_for_match(&cfg, &empty_snap(), &m, None, None, None),
            ("Halle A".to_string(), HallSource::Rule)
        );
    }

    #[test]
    fn hall_falls_back_to_the_preparation_call() {
        // Ohne Regel weiß nur der Aufruf, wohin das Spiel soll.
        let m = a_match(7);
        assert_eq!(
            hall_for_match(
                &AppConfig::default(),
                &empty_snap(),
                &m,
                None,
                Some("Halle B"),
                None
            ),
            ("Halle B".to_string(), HallSource::Call)
        );
    }

    #[test]
    fn the_rule_wins_over_the_call() {
        // Die Regel ist eine Turnier-Festlegung, der Aufruf eine
        // Tagesentscheidung — widersprechen sie sich, gilt die Festlegung,
        // genau wie bei der Vergabe selbst (`hall_allows_match`).
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "Halle A".to_string(),
        });
        let m = a_match(7);
        assert_eq!(
            hall_for_match(&cfg, &empty_snap(), &m, None, Some("Halle B"), None),
            ("Halle A".to_string(), HallSource::Rule)
        );
    }

    #[test]
    fn a_finished_match_does_not_hold_its_court_forever() {
        // Aus dem Betrieb gemeldet (09.08.): Nach dem ersten beendeten Spiel
        // blieb das Feld dauerhaft auf „wird geräumt" stehen und nahm kein
        // neues Spiel mehr an.
        //
        // Grund: Das beendete Match **behält** seine `CourtID` — bewusst so,
        // als Turnier-Doku „wo wurde gespielt" (Tilo 19.07., Vorbild
        // Original-BTS), und BTP entfernt sie nie wieder. Wer nur fragt „hat
        // irgendein Spiel diese CourtID?", hält das Feld deshalb bis zum
        // Turnierende für besetzt. Frei ist ein Feld, sobald das Spiel darauf
        // **beendet** ist — die Feld-Seite gibt derselbe Schreibvorgang frei.
        let mut fertig = a_match(9);
        fertig.status = MatchStatus::Finished;
        fertig.court_id = Some(1);
        fertig.winner = Some(1);
        let snap = snap(vec![a_court(1, None)], vec![fertig], Vec::new());

        assert_eq!(court_occupied_by(&snap, 1), None);
        assert!(!occupied_courts(&snap).contains(&1));
    }

    #[test]
    fn a_running_match_still_holds_its_court() {
        // Gegenprobe: Solange gespielt wird, ist das Feld belegt. Sonst legte
        // die Automatik ein zweites Spiel auf ein laufendes.
        let mut laeuft = a_match(9);
        laeuft.status = MatchStatus::OnCourt;
        laeuft.court_id = Some(1);
        let snap = snap(vec![a_court(1, None)], vec![laeuft], Vec::new());

        assert_eq!(court_occupied_by(&snap, 1), Some(9));
        assert!(occupied_courts(&snap).contains(&1));
    }

    #[test]
    fn btp_can_carry_the_planned_venue_after_all() {
        // **Korrektur des Befunds aus Schritt 15.** Dort war an zwei
        // Mitschnitten gemessen worden, dass ein angesetztes Spiel keinen
        // Spielort trägt — beide Turniere pflegten die Spalte schlicht nicht.
        // Am 09.08.2026 an einem Turnier gemessen, das sie pflegt: 48 Matches
        // mit `LocationID`, die meisten ohne jede Feldzuweisung.
        let mut m = a_match(7);
        m.location_id = Some(2);
        let s = snap(
            Vec::new(),
            vec![m.clone()],
            vec![
                BtpLocation {
                    id: 1,
                    name: "Kyritzer".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Luckenwalder".to_string(),
                },
            ],
        );
        assert_eq!(
            hall_for_match(&AppConfig::default(), &s, &m, None, None, None),
            ("Luckenwalder".to_string(), HallSource::Btp)
        );
    }

    #[test]
    fn a_hall_set_by_hand_beats_the_one_from_btp() {
        // Wer den Ort während des Turniers eigens umsetzt, disponiert um —
        // das ist frischer als der Plan, der morgens in BTP stand.
        let mut m = a_match(7);
        m.location_id = Some(1);
        let s = snap(
            Vec::new(),
            vec![m.clone()],
            vec![
                BtpLocation {
                    id: 1,
                    name: "Kyritzer".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Luckenwalder".to_string(),
                },
            ],
        );
        assert_eq!(
            hall_for_match(
                &AppConfig::default(),
                &s,
                &m,
                Some("Luckenwalder"),
                None,
                None
            ),
            ("Luckenwalder".to_string(), HallSource::Manual)
        );
    }

    #[test]
    fn a_hall_set_by_hand_beats_the_preparation_call() {
        // BTP führt an angesetzten Spielen keinen Spielort (siehe Doku oben).
        // Die Turnierleitung kann ihn deshalb selbst setzen — und dann meint
        // sie es ernster als ein Aufruf, der die Halle nur nebenbei mitnimmt.
        let m = a_match(7);
        assert_eq!(
            hall_for_match(
                &AppConfig::default(),
                &empty_snap(),
                &m,
                Some("Halle C"),
                Some("Halle B"),
                None,
            ),
            ("Halle C".to_string(), HallSource::Manual)
        );
    }

    #[test]
    fn the_rule_still_wins_over_a_hall_set_by_hand() {
        // Sonst könnte jemand ein Spiel in eine Halle stellen, in der es die
        // Vergabe-Prüfung nie aufs Feld ließe (`hall_allows_match`) — es
        // stünde dort und ginge nie los.
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "Halle A".to_string(),
        });
        let m = a_match(7);
        assert_eq!(
            hall_for_match(&cfg, &empty_snap(), &m, Some("Halle C"), None, None),
            ("Halle A".to_string(), HallSource::Rule)
        );
    }

    #[test]
    fn without_rule_or_call_the_hall_is_openly_unknown() {
        // Wichtig für die Anzeige: „unbekannt" muss unterscheidbar sein von
        // „gehört in Halle X". Ein Spiel ohne Halle darf nicht stillschweigend
        // aus dem gefilterten Bild fallen — es würde sonst nie vergeben.
        let m = a_match(7);
        assert_eq!(
            hall_for_match(&AppConfig::default(), &empty_snap(), &m, None, None, None),
            (String::new(), HallSource::None)
        );
    }

    #[test]
    fn die_auto_halle_greift_als_letzte_stufe_und_wird_kanonisiert() {
        // Spec `hallen-vorverteilung`: Die Vorverteilung füllt nur Spiele,
        // die sonst GAR keine Halle hätten — und liefert die
        // BTP-Schreibweise (Kanonisierung wie bei der Hand-Halle).
        let m = a_match(7);
        let s = snap(
            Vec::new(),
            vec![m.clone()],
            vec![BtpLocation {
                id: 1,
                name: "Halle B".to_string(),
            }],
        );
        assert_eq!(
            hall_for_match(&AppConfig::default(), &s, &m, None, None, Some("halle b")),
            ("Halle B".to_string(), HallSource::Auto)
        );
    }

    #[test]
    fn der_aufruf_schlaegt_die_auto_halle() {
        // E3 als Kaskaden-Vorrang (zusätzlich räumt der Aufruf den
        // Auto-Eintrag aktiv — hier geht es nur um die Auflösung).
        let m = a_match(7);
        assert_eq!(
            hall_for_match(
                &AppConfig::default(),
                &empty_snap(),
                &m,
                None,
                Some("Halle A"),
                Some("Halle B"),
            ),
            ("Halle A".to_string(), HallSource::Call)
        );
    }

    #[test]
    fn btp_schlaegt_die_auto_halle() {
        // Wird der Spielort in BTP nachgepflegt, gewinnt der Turnierplan
        // über die Tages-Automatik.
        let mut m = a_match(7);
        m.location_id = Some(2);
        let s = snap(
            Vec::new(),
            vec![m.clone()],
            vec![BtpLocation {
                id: 2,
                name: "Luckenwalder".to_string(),
            }],
        );
        assert_eq!(
            hall_for_match(&AppConfig::default(), &s, &m, None, None, Some("Kyritzer")),
            ("Luckenwalder".to_string(), HallSource::Btp)
        );
    }

    #[test]
    fn die_auto_herkunft_reist_als_auto_ueber_die_leitung() {
        // Alte tl.html-Stände prüfen nur bekannte Werte und tolerieren
        // "auto" — der Wire-Wert ist damit Teil des Vertrags.
        assert_eq!(
            serde_json::to_string(&HallSource::Auto).unwrap(),
            "\"auto\""
        );
    }

    #[test]
    fn player_key_prefers_member_id_then_name() {
        let mut p = player("Müller");
        assert_eq!(player_key(&p), "müller");
        p.member_id = Some("  08-001234 ".to_string());
        assert_eq!(player_key(&p), "08-001234");
    }

    #[test]
    fn matches_at_the_same_time_follow_the_tournament_plan() {
        // In BTP haben alle Spiele eines Zeitfensters **dieselbe** angesetzte
        // Zeit — der ganze Vormittag steht auf 9:00. Die Reihenfolge darin
        // gibt `DisplayOrder` vor; das ist die Liste, die die Turnierleitung
        // ausdruckt und abarbeitet.
        //
        // Ohne dieses Feld entschied die Spielnummer, und die läuft quer:
        // Aus der echten Ansetzung (Nr 2, 6, 2, 6 …) wurde bei uns „erst
        // alle Nummer 2, dann alle Nummer 6" — eine Reihenfolge, die im
        // Turnierplan nirgends steht.
        // Sortiert wird nach der **Auslosung** (DrawID), dann nach der
        // Spielnummer — genau so steht es in der aus BTP exportierten Liste:
        // „Gruppe 3 Nr 2, Gruppe 3 Nr 6, Gruppe 4 Nr 2, …".
        //
        // Bis 09.08.2026 stand hier die `DisplayOrder` des Matches. Die haben
        // aber nur wenige Spiele (im gemessenen Turnier rund jedes zehnte),
        // und die ohne landeten hinter allen anderen — das erste Spiel des
        // Tages stand plötzlich an fünfter Stelle.
        let plan = |id, nr, draw| {
            let mut m = a_match(id);
            m.planned_time = Some(202_702_050_900); // alle 9:00
            m.match_num = Some(nr);
            m.draw_id = draw;
            m
        };
        // Gruppe 3 (Draw 24) vor Gruppe 4 (Draw 25); innerhalb der Gruppe
        // entscheidet die Spielnummer.
        let erst = plan(1241, 2, 24);
        let dann = plan(1236, 6, 24);
        let drittens = plan(1266, 2, 25);

        let mut list = [
            sort_key(&drittens, false),
            sort_key(&dann, false),
            sort_key(&erst, false),
        ];
        list.sort();
        assert_eq!(list[0], sort_key(&erst, false), "1241 zuerst (Ansetzung 1)");
        assert_eq!(list[1], sort_key(&dann, false), "dann 1236 — trotz Nr 6");
        assert_eq!(list[2], sort_key(&drittens, false));
    }

    #[test]
    fn without_a_tournament_plan_the_match_number_still_decides() {
        // Nicht jedes Turnier pflegt die Ansetzungsreihenfolge. Fehlt sie,
        // bleibt es beim bisherigen Verhalten — sonst würfe die Umstellung
        // erprobte Turniere durcheinander.
        let ohne = |id, nr| {
            let mut m = a_match(id);
            m.planned_time = Some(202_702_050_900);
            m.match_num = Some(nr);
            m.display_order = None;
            m
        };
        let klein = ohne(500, 2);
        let gross = ohne(400, 6);
        let mut list = [sort_key(&gross, false), sort_key(&klein, false)];
        list.sort();
        assert_eq!(list[0], sort_key(&klein, false), "Nummer 2 vor Nummer 6");
    }

    #[test]
    fn sort_key_puts_called_matches_first_then_schedule_then_number() {
        // Dieselbe Reihenfolge, die die automatische Vergabe benutzt — die
        // Liste in der Oberfläche muss sie spiegeln, sonst zeigen zwei
        // Geräte zwei Reihenfolgen und niemand versteht, was als Nächstes
        // dran ist.
        let mut called = a_match(5);
        called.planned_time = Some(202_608_071_400);
        let mut early = a_match(6);
        early.planned_time = Some(202_608_071_200);
        let mut late = a_match(7);
        late.planned_time = Some(202_608_071_600);

        let mut list = [
            sort_key(&late, false),
            sort_key(&early, false),
            sort_key(&called, true),
        ];
        list.sort();
        assert_eq!(list[0], sort_key(&called, true), "Gerufene zuerst");
        assert_eq!(
            list[1],
            sort_key(&early, false),
            "dann die frühere Ansetzung"
        );
        assert_eq!(list[2], sort_key(&late, false));
    }

    #[test]
    fn manual_order_ein_gerufenes_match_schlaegt_jeden_praefix_eintrag() {
        // Blocker 1 der Klärung: der Präfix wirkt nur INNERHALB der noch
        // nicht gerufenen Spiele — ein Aufruf sticht immer.
        let called = sort_key_with_manual_order(true, None, Some(2), Some(2), Some(2), 2);
        let praefix_erster =
            sort_key_with_manual_order(false, Some(0), Some(1), Some(1), Some(1), 1);
        assert!(called < praefix_erster, "gerufen schlägt Präfix-Rang 0");
    }

    #[test]
    fn manual_order_praefix_schlaegt_den_btp_zeitplan() {
        // Ein spät angesetztes Match mit Präfix-Rang 0 steht trotzdem vor
        // einem früh angesetzten Match ohne Präfix-Eintrag.
        let vorgezogen = sort_key_with_manual_order(false, Some(0), Some(999), Some(1), Some(1), 9);
        let frueh_ohne_praefix =
            sort_key_with_manual_order(false, None, Some(1), Some(1), Some(1), 1);
        assert!(vorgezogen < frueh_ohne_praefix);
    }

    #[test]
    fn manual_order_ohne_praefix_eintrag_bleibt_die_btp_reihenfolge_massgeblich() {
        let frueh = sort_key_with_manual_order(false, None, Some(1), Some(1), Some(1), 1);
        let spaet = sort_key_with_manual_order(false, None, Some(2), Some(1), Some(1), 2);
        assert!(frueh < spaet);
    }

    #[test]
    fn manual_order_leerer_praefix_verhaelt_sich_wie_sort_key_parts() {
        // Rückwärtskompatibilität: ohne jeden Präfix-Eintrag ist die
        // Reihenfolge identisch zu `sort_key_parts` — nur mit einem
        // zusätzlichen, für alle gleichen Rang-Feld in der Mitte.
        let a = sort_key_parts(false, Some(1), Some(3), Some(2), 10);
        let b = sort_key_parts(false, Some(1), Some(3), Some(4), 11);
        let ma = sort_key_with_manual_order(false, None, Some(1), Some(3), Some(2), 10);
        let mb = sort_key_with_manual_order(false, None, Some(1), Some(3), Some(4), 11);
        assert_eq!(a < b, ma < mb);
    }

    #[test]
    fn resolve_and_sort_key_nutzt_den_globalen_praefix() {
        use crate::tablet::queue_order::QueueOrderStore;

        let mut vorgezogen = a_match(4);
        vorgezogen.planned_time = Some(202_608_071_600); // spät angesetzt
        let mut frueh = a_match(1);
        frueh.planned_time = Some(202_608_071_200); // früh angesetzt

        let s = snap(
            Vec::new(),
            vec![vorgezogen.clone(), frueh.clone()],
            Vec::new(),
        );
        let config = AppConfig::default();
        let order = QueueOrderStore::default();
        order.reorder(&[1, 4], 4, Some(1)); // Match 4 vor Match 1 ziehen

        let (hall_vorgezogen, _, key_vorgezogen) =
            resolve_and_sort_key(&config, &s, &vorgezogen, None, None, None, false, &order);
        let (_, _, key_frueh) =
            resolve_and_sort_key(&config, &s, &frueh, None, None, None, false, &order);
        assert_eq!(hall_vorgezogen, "", "Halle wird mit zurückgegeben");
        assert!(
            key_vorgezogen < key_frueh,
            "manuell vorgezogenes Match schlägt trotz späterer Ansetzung"
        );
    }

    #[test]
    fn der_praefix_wirkt_ueber_hallengrenzen_hinweg() {
        // ADR 0026: Der Rang wird global nachgeschlagen, nicht in der Halle
        // des Matches. Ein in Halle B vorgezogenes Spiel steht damit auch
        // vor einem früher angesetzten Spiel aus Halle A.
        use crate::tablet::queue_order::QueueOrderStore;

        let mut a = a_match(1);
        a.planned_time = Some(202_608_071_200); // Halle A, früh
        let mut b = a_match(2);
        b.planned_time = Some(202_608_071_900); // Halle B, spät

        let s = snap(Vec::new(), vec![a.clone(), b.clone()], Vec::new());
        let config = AppConfig::default();
        let order = QueueOrderStore::default();
        order.reorder(&[1, 2], 2, Some(1));

        let (hall_a, _, key_a) =
            resolve_and_sort_key(&config, &s, &a, Some("Halle A"), None, None, false, &order);
        let (hall_b, _, key_b) =
            resolve_and_sort_key(&config, &s, &b, Some("Halle B"), None, None, false, &order);
        assert_eq!(hall_a, "Halle A");
        assert_eq!(hall_b, "Halle B");
        assert!(key_b < key_a, "der Präfix kennt keine Hallengrenze mehr");
    }

    #[test]
    fn ready_queue_schliesst_gerufene_spiele_aus_und_respektiert_den_praefix() {
        use crate::tablet::queue_order::QueueOrderStore;
        use std::collections::HashSet;

        let mut m1 = a_match(1);
        m1.planned_time = Some(202_608_071_200);
        let mut m2 = a_match(2);
        m2.planned_time = Some(202_608_071_300);
        let mut m3_gerufen = a_match(3);
        m3_gerufen.planned_time = Some(202_608_071_100); // früheste Zeit, aber gerufen

        let s = snap(
            Vec::new(),
            vec![m1.clone(), m2.clone(), m3_gerufen.clone()],
            Vec::new(),
        );
        let config = AppConfig::default();
        let order = QueueOrderStore::default();
        let manual: HashMap<i64, String> = HashMap::new();
        let called: HashSet<i64> = [3].into_iter().collect();

        let liste = ready_queue(&config, &s, &manual, &HashMap::new(), &called, &order);
        assert_eq!(liste, vec![1, 2], "gerufenes Spiel 3 taucht nicht auf");

        order.reorder(&liste, 2, Some(1));
        let liste2 = ready_queue(&config, &s, &manual, &HashMap::new(), &called, &order);
        assert_eq!(liste2, vec![2, 1], "Präfix wirkt in der effektiven Liste");
    }

    #[test]
    fn check_assign_lehnt_ein_offenes_spiel_weiterhin_ab() {
        // Nicht-Ziel A3 der Spec `tl-offene-paarungen`: Anzeigen ja,
        // vergeben nein. Bestandsverhalten — hier festgenagelt, damit es
        // niemand beim Öffnen der Liste versehentlich mit aufmacht.
        let mut offen = a_match(1);
        offen.team1 = Vec::new();
        offen.team2 = Vec::new();
        let s = snap(vec![a_court(1, None)], vec![offen], Vec::new());

        assert!(
            matches!(
                check(&s, &AppConfig::default(), 1, 1, CourtExpectation::Any),
                Err(AssignError::MatchNotPlayable)
            ),
            "ein Spiel ohne Teilnehmer darf nie auf ein Feld"
        );
    }

    #[test]
    fn ready_queue_nimmt_offene_spiele_auf() {
        // ADR 0053: Offene Spiele nehmen an der globalen manuellen
        // Reihenfolge teil — sonst wäre jeder Zug an ihnen ein stiller
        // No-Op mit grüner Erfolgsmeldung.
        use crate::tablet::queue_order::QueueOrderStore;
        use std::collections::HashSet;

        let mut echt = a_match(1);
        echt.planned_time = Some(202_608_301_200);
        let mut offen = a_match(2);
        offen.planned_time = Some(202_608_301_300);
        offen.team1 = Vec::new();
        offen.team2 = Vec::new();

        let s = snap(Vec::new(), vec![echt, offen], Vec::new());
        let liste = ready_queue(
            &AppConfig::default(),
            &s,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &QueueOrderStore::default(),
        );
        assert_eq!(liste, vec![1, 2]);
    }

    #[test]
    fn ein_gerufenes_spiel_bleibt_auch_neben_offenen_spielen_aussen_vor() {
        use crate::tablet::queue_order::QueueOrderStore;
        use std::collections::HashSet;

        let mut gerufen = a_match(1);
        gerufen.planned_time = Some(202_608_301_100);
        let mut offen = a_match(2);
        offen.planned_time = Some(202_608_301_300);
        offen.team1 = Vec::new();
        offen.team2 = Vec::new();

        let s = snap(Vec::new(), vec![gerufen, offen], Vec::new());
        let called: HashSet<i64> = [1].into_iter().collect();
        let liste = ready_queue(
            &AppConfig::default(),
            &s,
            &HashMap::new(),
            &HashMap::new(),
            &called,
            &QueueOrderStore::default(),
        );
        assert_eq!(liste, vec![2], "das gerufene Spiel bleibt draußen");
    }

    #[test]
    fn die_reihenfolge_der_echten_spiele_aendert_sich_durch_offene_nicht() {
        // Die Zusage aus ADR 0053: Offene Spiele reihen sich EIN, sie
        // sortieren nichts um.
        use crate::tablet::queue_order::QueueOrderStore;
        use std::collections::HashSet;

        let mut e1 = a_match(1);
        e1.planned_time = Some(202_608_301_200);
        let mut e2 = a_match(3);
        e2.planned_time = Some(202_608_301_400);
        let mut offen = a_match(2);
        offen.planned_time = Some(202_608_301_300);
        offen.team1 = Vec::new();
        offen.team2 = Vec::new();

        let cfg = AppConfig::default();
        let order = QueueOrderStore::default();
        let leer = HashSet::new();

        let ohne = ready_queue(
            &cfg,
            &snap(Vec::new(), vec![e1.clone(), e2.clone()], Vec::new()),
            &HashMap::new(),
            &HashMap::new(),
            &leer,
            &order,
        );
        let mit = ready_queue(
            &cfg,
            &snap(Vec::new(), vec![e1, offen, e2], Vec::new()),
            &HashMap::new(),
            &HashMap::new(),
            &leer,
            &order,
        );

        assert_eq!(ohne, vec![1, 3]);
        assert_eq!(mit, vec![1, 2, 3]);
        assert_eq!(
            mit.iter().filter(|id| **id != 2).copied().collect::<Vec<_>>(),
            ohne,
            "ohne die offenen bleibt exakt die alte Abfolge übrig"
        );
    }

    #[test]
    fn ready_queue_enthaelt_alle_hallen_in_einer_liste() {
        // ADR 0026: kein Hallenfilter mehr — die ziehbare Liste ist die
        // gesamte Spielliste des Turniers.
        use crate::tablet::queue_order::QueueOrderStore;
        use std::collections::HashSet;

        let mut m1 = a_match(1);
        m1.planned_time = Some(202_608_071_200);
        let mut m2 = a_match(2);
        m2.planned_time = Some(202_608_071_300);
        let mut m3 = a_match(3);
        m3.planned_time = Some(202_608_071_400);

        let s = snap(Vec::new(), vec![m1, m2, m3], Vec::new());
        let config = AppConfig::default();
        let order = QueueOrderStore::default();
        // 1 in Halle A, 2 in Halle B, 3 ohne Hallenzuordnung.
        let manual: HashMap<i64, String> = [(1, "Halle A".to_string()), (2, "Halle B".to_string())]
            .into_iter()
            .collect();
        let called: HashSet<i64> = HashSet::new();

        assert_eq!(
            ready_queue(&config, &s, &manual, &HashMap::new(), &called, &order),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn a_match_without_a_schedule_sorts_after_scheduled_ones() {
        let mut scheduled = a_match(9);
        scheduled.planned_time = Some(202_608_071_600);
        let unscheduled = a_match(1); // ohne planned_time, kleinere Nummer
        assert!(
            sort_key(&scheduled, false) < sort_key(&unscheduled, false),
            "ohne Ansetzung ans Ende, trotz kleinerer Spielnummer"
        );
    }

    #[test]
    fn a_player_currently_on_court_blocks_the_match_and_is_named() {
        // „Gesperrt" ohne Namen ist eine Blackbox — die Turnierleitung muss
        // sehen, WER blockiert, sonst misstraut sie der Anzeige.
        let mut running = a_match(1);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        running.team1 = vec![player("Müller")];
        running.team2 = vec![player("Gegner")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];
        wanted.team2 = vec![player("Frei")];

        let s = snap(
            vec![a_court(1, None)],
            vec![running, wanted.clone()],
            Vec::new(),
        );
        let av = PlayerAvailability::from_snapshot(&s, &AppConfig::default());
        match av.blocked(&wanted, 1_000) {
            Some(Blocked::Playing { players, .. }) => {
                assert_eq!(players, vec!["Müller".to_string()])
            }
            other => panic!("erwartet: spielt gerade, war: {other:?}"),
        }
    }

    #[test]
    fn a_finished_match_does_not_block_its_players() {
        // Ein beendetes Spiel hält sein FELD, gibt seine Spieler aber frei —
        // die unterliegen dann der Pausenregel, nicht der harten Sperre.
        let mut done = a_match(1);
        done.court_id = Some(1);
        done.status = MatchStatus::Finished;
        done.team1 = vec![player("Müller")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];

        let s = snap(
            vec![a_court(1, None)],
            vec![done, wanted.clone()],
            Vec::new(),
        );
        let av = PlayerAvailability::from_snapshot(&s, &AppConfig::default());
        assert_eq!(av.blocked(&wanted, 1_000), None);
    }

    #[test]
    fn a_player_still_in_their_break_blocks_and_reports_when_they_are_free() {
        // „frei ab 14:32" ist der Unterschied zwischen raten und planen.
        let mut done = a_match(1);
        done.status = MatchStatus::Finished;
        done.finished_at = Some(100_000);
        done.team1 = vec![player("Müller")];
        done.team2 = vec![player("Gegner")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];
        wanted.team2 = vec![player("Frei")];

        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 10.0; // 600_000 ms

        let s = snap(Vec::new(), vec![done, wanted.clone()], Vec::new());
        let av = PlayerAvailability::from_snapshot(&s, &cfg);
        match av.blocked(&wanted, 200_000) {
            Some(Blocked::Pause {
                until_ms, players, ..
            }) => {
                assert_eq!(until_ms, 700_000, "Spielende + Pausenlänge");
                assert_eq!(players, vec!["Müller".to_string()]);
            }
            other => panic!("erwartet: Pause, war: {other:?}"),
        }
        // Nach Ablauf der Pause ist er frei.
        assert_eq!(av.blocked(&wanted, 700_000), None);
    }

    #[test]
    fn the_configured_break_overrides_the_btp_setting() {
        // config.auto_assign.pause_minutes > 0 schlägt BTP-Setting 1303 —
        // exakt die Regel, nach der die automatische Vergabe schon arbeitet.
        let mut done = a_match(1);
        done.status = MatchStatus::Finished;
        done.finished_at = Some(0);
        done.team1 = vec![player("Müller")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];

        let mut s = snap(Vec::new(), vec![done, wanted.clone()], Vec::new());
        s.rest_minutes = Some(30); // BTP sagt 30 Minuten

        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 5.0; // die Config sagt 5
        let av = PlayerAvailability::from_snapshot(&s, &cfg);
        match av.blocked(&wanted, 60_000) {
            Some(Blocked::Pause { until_ms, .. }) => assert_eq!(until_ms, 300_000),
            other => panic!("erwartet: 5-Minuten-Pause, war: {other:?}"),
        }

        // Ohne Override greift der BTP-Wert.
        let cfg_no_override = AppConfig::default();
        let av2 = PlayerAvailability::from_snapshot(&s, &cfg_no_override);
        match av2.blocked(&wanted, 60_000) {
            Some(Blocked::Pause { until_ms, .. }) => assert_eq!(until_ms, 1_800_000),
            other => panic!("erwartet: 30-Minuten-Pause aus BTP, war: {other:?}"),
        }
    }

    #[test]
    fn die_beim_spielende_eingefrorene_pause_schlaegt_die_aktuelle_einstellung() {
        // Feldtest 22.08.2026: Eine geänderte Pausenzeit verlängerte auch die
        // schon laufenden Pausen. Trägt das Spiel seine eigene Pause, gilt
        // sie — ganz gleich, was gerade eingestellt ist.
        let mut done = a_match(1);
        done.status = MatchStatus::Finished;
        done.finished_at = Some(0);
        done.pause_ms = Some(5 * 60_000); // beim Ende galten 5 Minuten
        done.team1 = vec![player("Müller")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];

        let mut s = snap(Vec::new(), vec![done, wanted.clone()], Vec::new());
        s.rest_minutes = Some(30);
        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 45.0; // inzwischen auf 45 gestellt

        let av = PlayerAvailability::from_snapshot(&s, &cfg);
        match av.blocked(&wanted, 60_000) {
            Some(Blocked::Pause { until_ms, .. }) => assert_eq!(until_ms, 300_000),
            other => panic!("erwartet: die eingefrorenen 5 Minuten, war: {other:?}"),
        }
        assert_eq!(
            av.blocked(&wanted, 300_000),
            None,
            "nach den eingefrorenen 5 Minuten ist er frei"
        );
    }

    #[test]
    fn ein_spiel_ohne_eigenen_stempel_faellt_auf_die_aktuelle_einstellung_zurueck() {
        // Altbestand aus einer Version vor v0.9.253: kein `pause_ms` am
        // Spiel. Dann gilt weiterhin die eingestellte Pause.
        let mut done = a_match(1);
        done.status = MatchStatus::Finished;
        done.finished_at = Some(0);
        done.pause_ms = None;
        done.team1 = vec![player("Müller")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];

        let s = snap(Vec::new(), vec![done, wanted.clone()], Vec::new());
        let mut cfg = AppConfig::default();
        cfg.auto_assign.pause_minutes = 10.0;

        let av = PlayerAvailability::from_snapshot(&s, &cfg);
        match av.blocked(&wanted, 60_000) {
            Some(Blocked::Pause { until_ms, .. }) => assert_eq!(until_ms, 600_000),
            other => panic!("erwartet: 10 Minuten aus der Einstellung, war: {other:?}"),
        }
    }

    #[test]
    fn without_any_break_configured_nobody_is_blocked_by_a_break() {
        let mut done = a_match(1);
        done.status = MatchStatus::Finished;
        done.finished_at = Some(100_000);
        done.team1 = vec![player("Müller")];
        let mut wanted = a_match(2);
        wanted.team1 = vec![player("Müller")];

        let s = snap(Vec::new(), vec![done, wanted.clone()], Vec::new());
        let av = PlayerAvailability::from_snapshot(&s, &AppConfig::default());
        assert_eq!(av.blocked(&wanted, 100_001), None);
    }

    #[test]
    fn moving_a_running_match_to_a_free_court_is_allowed() {
        // Beim Umhängen steht das Spiel erlaubterweise schon auf einem Feld
        // (dem Quellfeld) und ist nicht mehr „geplant". Beides würde
        // `check_assign` ablehnen — deshalb hat das Zielfeld seine eigene
        // Prüfung.
        let mut running = a_match(7);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![running],
            Vec::new(),
        );
        assert!(check_move_target(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            7,
            1,
            2,
            CourtExpectation::Free
        )
        .is_ok());
    }

    #[test]
    fn moving_a_match_that_does_not_stand_on_the_source_court_is_rejected() {
        // DIE tragende Voraussetzung: Diese Prüfung lässt Spielbereitschaft
        // und Spieler-Verfügbarkeit bewusst weg, weil das Spiel ja schon
        // läuft. Stimmt das nicht, wäre Umhängen ein Zuweisen ohne jede
        // Prüfung — man könnte ein beliebiges Spiel auf ein Feld setzen.
        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![a_match(7)], // steht auf gar keinem Feld
            Vec::new(),
        );
        assert!(
            check_move_target(
                &s,
                &AppConfig::default(),
                &[],
                &[],
                7,
                1,
                2,
                CourtExpectation::Free
            )
            .is_err(),
            "ohne Spiel auf dem Quellfeld darf nichts umgehängt werden"
        );
    }

    #[test]
    fn moving_a_finished_match_is_rejected() {
        // Ein beendetes Spiel auf ein freies Feld zu setzen, machte dieses
        // Feld dauerhaft unbelegbar: Die Vergabe zählt es als besetzt, und
        // niemand räumt es je wieder ab.
        let mut done = a_match(7);
        done.court_id = Some(1);
        done.status = MatchStatus::Finished;
        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![done],
            Vec::new(),
        );
        assert_eq!(
            check_move_target(
                &s,
                &AppConfig::default(),
                &[],
                &[],
                7,
                1,
                2,
                CourtExpectation::Free
            )
            .unwrap_err(),
            AssignError::MatchNotPlayable
        );
    }

    #[test]
    fn moving_onto_an_occupied_court_is_rejected() {
        let mut running = a_match(7);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        let mut other = a_match(8);
        other.court_id = Some(2);
        other.status = MatchStatus::OnCourt;
        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![running, other],
            Vec::new(),
        );
        assert_eq!(
            check_move_target(
                &s,
                &AppConfig::default(),
                &[],
                &[],
                7,
                1,
                2,
                CourtExpectation::Free
            )
            .unwrap_err(),
            AssignError::CourtTaken {
                by_match: 8,
                finished: false
            }
        );
    }

    #[test]
    fn moving_respects_the_hall_rule_and_locks() {
        let mut cfg = AppConfig::default();
        cfg.discipline_hall_rules.push(DisciplineHallRule {
            discipline: "mens_singles".to_string(),
            draw_name: String::new(),
            hall: "Halle A".to_string(),
        });
        let mut running = a_match(7);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        let s = snap(
            vec![a_court(1, Some(1)), a_court(2, Some(2))],
            vec![running],
            vec![
                BtpLocation {
                    id: 1,
                    name: "Halle A".to_string(),
                },
                BtpLocation {
                    id: 2,
                    name: "Halle B".to_string(),
                },
            ],
        );
        assert_eq!(
            check_move_target(&s, &cfg, &[], &[], 7, 1, 2, CourtExpectation::Free).unwrap_err(),
            AssignError::HallNotAllowed
        );
        assert_eq!(
            check_move_target(
                &s,
                &AppConfig::default(),
                &[2],
                &[],
                7,
                1,
                2,
                CourtExpectation::Free
            )
            .unwrap_err(),
            AssignError::CourtLocked
        );
    }

    #[test]
    fn moving_a_match_to_the_court_it_already_stands_on_is_rejected() {
        // Quelle und Ziel identisch ist keine Bewegung, sondern vermutlich
        // ein Fehlgriff — und würde einen sinnlosen BTP-Schreibvorgang
        // auslösen.
        let mut running = a_match(7);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        let s = snap(vec![a_court(1, None)], vec![running], Vec::new());
        assert!(check_move_target(
            &s,
            &AppConfig::default(),
            &[],
            &[],
            7,
            1,
            1,
            CourtExpectation::Free
        )
        .is_err());
    }

    #[test]
    fn freeing_a_court_that_holds_another_match_is_rejected() {
        // Sonst verwürfe ein Gerät den laufenden Spielstand eines Spiels,
        // das es gar nicht gemeint hat.
        let mut other = a_match(9);
        other.court_id = Some(1);
        other.status = MatchStatus::OnCourt;
        let s = snap(vec![a_court(1, None)], vec![other], Vec::new());
        let err = check_free(&s, 1, CourtExpectation::Match { match_id: 7 }).unwrap_err();
        assert_eq!(
            err,
            AssignError::CourtTaken {
                by_match: 9,
                finished: false
            }
        );
    }

    #[test]
    fn a_matchs_own_players_do_not_block_it_from_moving() {
        // Beim Umhängen wird dasselbe Spiel geprüft, das gerade läuft —
        // seine eigenen Spieler dürfen es nicht blockieren, sonst wäre kein
        // Feldwechsel eines laufenden Spiels möglich.
        let mut running = a_match(7);
        running.court_id = Some(1);
        running.status = MatchStatus::OnCourt;
        running.team1 = vec![player("Müller")];
        running.team2 = vec![player("Gegner")];

        let s = snap(
            vec![a_court(1, None), a_court(2, None)],
            vec![running.clone()],
            Vec::new(),
        );
        let av = PlayerAvailability::from_snapshot(&s, &AppConfig::default());
        assert_eq!(
            av.blocked(&running, 1_000),
            None,
            "das Spiel blockiert sich nicht selbst"
        );
    }

    #[test]
    fn freeing_an_already_empty_court_is_rejected_when_a_match_was_expected() {
        let s = snap(vec![a_court(1, None)], Vec::new(), Vec::new());
        assert_eq!(
            check_free(&s, 1, CourtExpectation::Match { match_id: 7 }).unwrap_err(),
            AssignError::CourtFree
        );
        // Ohne Erwartung ist dasselbe in Ordnung (Desktop-Verhalten).
        assert!(check_free(&s, 1, CourtExpectation::Any).is_ok());
    }
}
