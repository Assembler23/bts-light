//! Schiedsrichter-Roster des Hosts (Spec
//! `docs/features/schiedsrichter-management.md`, ADR 0021/0022).
//!
//! BTP liefert die Schiedsrichter-Stammliste (`Officials`) — sie ist die
//! Wahrheit (R2) und wird hier **nicht** gespeichert. Dieser Speicher hält
//! ausschließlich die Zusatzdaten, die BTP nicht kennt: Rotationsreihenfolge,
//! Pausen, Stammverein (BTP überträgt keinen — Messung 13.08.2026),
//! Sperrlisten, feldweise Schalter und die lokalen SR/AR-Zuweisungen.
//!
//! Grundsätze:
//! - **Turniergebunden** (ADR 0022): Alles steht in einer eigenen Datei im
//!   App-Datenverzeichnis, im Kopf das Turnier. Wechselt das Turnier, wird
//!   der Stand verworfen — BTP-IDs gelten nur innerhalb eines Turniers, und
//!   Sperrlisten sind Personendaten, die nicht über Turniere hinweg
//!   weiterleben dürfen.
//! - **Außerhalb der `AppConfig`**: `export_identity` nimmt die komplette
//!   Konfiguration mit; Sperrlisten haben dort nichts zu suchen.
//! - **Best effort** wie `persist_scores`: Ein Schreibfehler kostet
//!   höchstens die Einteilung, nie ein Ergebnis.
//! - **Zuweisungen hängen am Match, nicht am Feld**: Nach Spielende bleiben
//!   sie liegen — sie sind die Grundlage der Einsatz-Ableitung (Spec Nr. 11,
//!   keine eigene Historien-Datenhaltung).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::btp::model::{BtpMatch, BtpPlayer, BtpSnapshot, MatchStatus};
use crate::config::RotationMode;

/// Warum ein Official nicht zu einem Spiel passt. Mehr Kategorien braucht
/// die Anzeige nicht — der *Grund* (welcher Verein, welcher Spieler) bleibt
/// bewusst auf dem Turnier-PC (Personendaten).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictKind {
    /// Eigener Stammverein oder ein gesperrter Verein ist beteiligt.
    Club,
    /// Ein namentlich gesperrter Spieler ist beteiligt.
    Person,
}

impl ConflictKind {
    /// Wort für die Anzeige („Verein"/„Person").
    pub fn label(&self) -> &'static str {
        match self {
            ConflictKind::Club => "Verein",
            ConflictKind::Person => "Person",
        }
    }
}

/// Hat dieser Official einen Konflikt mit den Spielern dieses Spiels?
/// (Spec Nr. 3). Reine Funktion — die Auto-Rotation überspringt Konflikte,
/// eine manuelle Zuweisung wird trotzdem ausgeführt und nur gewarnt.
///
/// Geprüft wird in der Reihenfolge der Spec: eigener Verein, Sperr-Verein,
/// Sperr-Spieler. Treffen mehrere zu, gewinnt der erste — die Warnung nennt
/// ohnehin nur die Kategorie.
pub fn official_conflict(extra: &OfficialExtra, players: &[BtpPlayer]) -> Option<ConflictKind> {
    let vereine: Vec<&str> = players
        .iter()
        .filter_map(|p| p.club.as_deref())
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect();
    // Vereinsnamen sind Handeingabe (BTP liefert am Official keinen) —
    // Groß-/Kleinschreibung und Leerzeichen dürfen nicht entscheiden.
    let gleich = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());

    let eigener = extra.club.trim();
    if !eigener.is_empty() && vereine.iter().any(|c| gleich(c, eigener)) {
        return Some(ConflictKind::Club);
    }
    if extra
        .blocked_clubs
        .iter()
        .any(|b| !b.trim().is_empty() && vereine.iter().any(|c| gleich(c, b)))
    {
        return Some(ConflictKind::Club);
    }
    if players
        .iter()
        .any(|p| p.id > 0 && extra.blocked_players.contains(&p.id))
    {
        return Some(ConflictKind::Person);
    }
    None
}

/// Ein Spieler zur Auswahl in der Sperrlisten-Pflege.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PickPlayer {
    /// BTP-`PlayerID` — das, was gespeichert wird.
    pub id: i64,
    /// Anzeigename, nach dem gesucht wird.
    pub name: String,
    /// Verein, um Namensgleiche zu unterscheiden (leer, wenn BTP keinen führt).
    pub club: String,
}

/// Spieler und Vereine des Turniers zur **Auswahl** — Grundlage der
/// Sperrlisten-Pflege und des Stammvereins.
///
/// Ohne sie müsste die Turnierleitung BTP-`PlayerID`s von Hand eintippen;
/// niemand kennt die Kennung von Anna Müller. Quelle ist die Meldeliste
/// (`BtpSnapshot::entries`) statt der Paarungen: Sie ist vollständig, auch
/// für Klassen ohne Auslosung. Beide Listen sind einmalig und alphabetisch,
/// damit die Vorschläge beim Tippen vorhersagbar sind.
pub fn pick_lists(entries: &[crate::btp::model::BtpEntry]) -> (Vec<PickPlayer>, Vec<String>) {
    let mut spieler: HashMap<i64, PickPlayer> = HashMap::new();
    let mut vereine: HashSet<String> = HashSet::new();
    for e in entries {
        for p in &e.players {
            if p.id <= 0 {
                continue; // ohne Kennung nicht speicherbar
            }
            let club = p.club.clone().unwrap_or_default().trim().to_string();
            if !club.is_empty() {
                vereine.insert(club.clone());
            }
            spieler.entry(p.id).or_insert_with(|| PickPlayer {
                id: p.id,
                name: p.name.clone(),
                club,
            });
        }
    }
    let mut spieler: Vec<PickPlayer> = spieler.into_values().collect();
    spieler.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let mut vereine: Vec<String> = vereine.into_iter().collect();
    vereine.sort();
    (spieler, vereine)
}

/// Ein beendetes Spiel, so viel davon, wie die Einsatz-Ableitung braucht.
/// Bewusst schlank statt `BtpMatch`: Der Speicher soll nichts über Spieler
/// oder Klassen wissen müssen, um Einsätze zu zählen.
pub struct FinishedMatch {
    pub match_id: i64,
    /// `Official1ID` des BTP-Matches (gewinnt gegen die lokale Zuweisung).
    pub btp_sr: Option<i64>,
    /// `Official2ID` des BTP-Matches.
    pub btp_ar: Option<i64>,
    /// Feld, auf dem gespielt wurde (falls BTP es noch führt).
    pub court_id: Option<i64>,
    /// Endezeit (Unix-ms) aus `stamp_finished`.
    pub finished_at: Option<u64>,
    /// Kampflos entschieden (Walkover): Es wurde nicht gespielt, also war
    /// auch niemand im Einsatz (Spec `tl-warteliste-schiedsrichter` AK 17).
    pub walkover: bool,
}

impl FinishedMatch {
    /// Die eine Stelle, an der ein BTP-Match zur Einsatz-Eingabe wird —
    /// damit Desktop-Zähler, TL-Web-Zähler und Detail-Listen nie
    /// auseinanderlaufen (z. B. beim Walkover).
    pub fn aus_btp(m: &BtpMatch) -> Self {
        FinishedMatch {
            match_id: m.id,
            btp_sr: m.official1_id,
            btp_ar: m.official2_id,
            court_id: m.court_id,
            finished_at: m.finished_at,
            walkover: m.result == crate::btp::model::MatchResult::Walkover,
        }
    }
}

/// Ein Einsatz eines Officials (abgeleitet, nicht gespeichert).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Appearance {
    pub match_id: i64,
    pub role: OfficialRole,
    pub court_id: Option<i64>,
    pub finished_at: Option<u64>,
}

/// Eingabe der Auto-Rotation für **ein** Feld. Als Struct, weil die Regel
/// von sieben Dingen abhängt — und weil so am Aufrufer lesbar steht, was
/// womit verglichen wird.
pub struct RotationInput<'a> {
    /// Das Spiel, das gerade auf dem Feld steht.
    pub match_id: i64,
    /// Seine Spieler (Grundlage der Konflikt-Prüfung).
    pub players: &'a [BtpPlayer],
    /// `Official1ID` des BTP-Matches, falls gesetzt — BTP gewinnt.
    pub btp_sr: Option<i64>,
    /// `Official2ID` des BTP-Matches, falls gesetzt.
    pub btp_ar: Option<i64>,
    /// Official-IDs, die BTP aktuell kennt; nur die dürfen zugewiesen werden.
    pub bekannt: &'a [i64],
    /// Wer für dieses Spiel nicht in Frage kommt: Dienst auf einem anderen
    /// Feld oder reserviert für ein anderes wartendes Spiel
    /// ([`Belegung::belegt_ausser`], ADR 0078), plus vorläufige
    /// Meetingpoint-SR und in diesem Lauf schon Vergebene.
    pub belegt: &'a HashSet<i64>,
    /// SR-Rotation für dieses Feld aktiv (globaler Schalter UND Feldschalter)?
    pub sr: bool,
    /// AR-Rotation für dieses Feld aktiv?
    pub ar: bool,
    /// Einteilungsprinzip (ADR 0079), vom Aufrufer aus
    /// [`OfficialsStore::rotation_mode`] gelesen.
    pub modus: RotationMode,
    /// Vorrolle je Official ([`OfficialsStore::vorrollen`]); nur in Modus C
    /// gelesen. Einmal je Takt gerechnet — Besetzungen desselben Takts
    /// ändern sie nicht.
    pub vorrolle: &'a HashMap<i64, OfficialRole>,
    /// Wer besetzt: die Feld-Rotation oder die Zuteilung beim Aufruf
    /// (ADR 0080). Nur Letztere setzt den Herkunfts-Merker.
    pub herkunft: Herkunft,
}

/// Anlass einer automatischen Besetzung (ADR 0080).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Herkunft {
    /// Neu belegtes Feld (`track_officials`).
    Feld,
    /// Aufruf in Vorbereitung (`meetingpoint::gerufene_besetzen`).
    Aufruf,
}

/// Eingabe der reinen Wahl-Funktion [`waehle_besetzung`] (ADR 0079).
pub struct Wahleingabe<'a> {
    pub modus: RotationMode,
    /// Gemeinsame Liste (B/C) bzw. SR-Liste (A).
    pub order: &'a [i64],
    /// AR-Liste — nur in Modus A gelesen.
    pub order_ar: &'a [i64],
    /// Darf diese Person überhaupt eingesetzt werden? (BTP kennt sie, nicht
    /// pausiert, nicht belegt oder reserviert, kein Konflikt.)
    pub frei: &'a dyn Fn(i64) -> bool,
    /// Rolle je Person.
    pub rolle: &'a dyn Fn(i64) -> Einsatzrolle,
    /// Dienst im zuletzt beendeten Spiel (nur Modus C).
    pub vorrolle: &'a HashMap<i64, OfficialRole>,
    pub sr_offen: bool,
    pub ar_offen: bool,
}

/// Der jeweils andere Dienst.
fn gegenteil(d: OfficialRole) -> OfficialRole {
    match d {
        OfficialRole::Sr => OfficialRole::Ar,
        OfficialRole::Ar => OfficialRole::Sr,
    }
}

/// Besetzt die offenen Dienste eines Spiels gemeinsam (ADR 0079) — der
/// **einzige** Weg, auf dem Officials automatisch gewählt werden. Liefert
/// `(sr, ar)`; `None` = niemand Passendes frei (der Dienst bleibt leer).
///
/// - **A (getrennt):** SR = erste passende Person der SR-Liste, AR = erste
///   passende der AR-Liste, die nicht schon SR ist.
/// - **B (abwechselnd):** wie A, beide aus der gemeinsamen Liste.
/// - **C (Rollenwechsel):** personenweise von oben: Jede freie Person
///   bekommt den offenen Dienst, den sie zuletzt nicht hatte (feste Rolle
///   schlägt die Vorrolle; ohne Vorrolle SR vor AR). Ist ihr Wunsch
///   vergeben oder nicht offen, wird sie übersprungen. Was danach offen
///   bleibt, bekommt die erste freie Person mit passender Rolle
///   (Rückfall) — so bleibt kein Dienst leer, solange jemand passt.
pub fn waehle_besetzung(e: &Wahleingabe) -> (Option<i64>, Option<i64>) {
    let kann = |id: i64, d: OfficialRole| (e.frei)(id) && (e.rolle)(id).erlaubt(d);
    match e.modus {
        RotationMode::Getrennt | RotationMode::Abwechselnd => {
            let liste_ar = if e.modus == RotationMode::Getrennt {
                e.order_ar
            } else {
                e.order
            };
            let sr = if e.sr_offen {
                e.order
                    .iter()
                    .copied()
                    .find(|&id| kann(id, OfficialRole::Sr))
            } else {
                None
            };
            let ar = if e.ar_offen {
                liste_ar
                    .iter()
                    .copied()
                    .find(|&id| Some(id) != sr && kann(id, OfficialRole::Ar))
            } else {
                None
            };
            (sr, ar)
        }
        RotationMode::Rollenwechsel => {
            let mut offen: Vec<OfficialRole> = Vec::new();
            if e.sr_offen {
                offen.push(OfficialRole::Sr);
            }
            if e.ar_offen {
                offen.push(OfficialRole::Ar);
            }
            let mut sr = None;
            let mut ar = None;
            // Durchgang 1: Rollenwechsel je Person.
            for &id in e.order {
                if offen.is_empty() {
                    break;
                }
                if !(e.frei)(id) {
                    continue;
                }
                let rolle = (e.rolle)(id);
                let erlaubt: Vec<OfficialRole> = offen
                    .iter()
                    .copied()
                    .filter(|d| rolle.erlaubt(*d))
                    .collect();
                let Some(&erster) = erlaubt.first() else {
                    continue;
                };
                let dienst = if !rolle.ist_beides() {
                    // Feste Rolle: nur ein Dienst ist überhaupt möglich.
                    erster
                } else {
                    match e.vorrolle.get(&id) {
                        // Ohne Vorgeschichte: SR, falls offen, sonst AR.
                        None => erster,
                        Some(&v) => {
                            let wunsch = gegenteil(v);
                            if !erlaubt.contains(&wunsch) {
                                continue; // nur die Vorrolle wäre frei
                            }
                            wunsch
                        }
                    }
                };
                offen.retain(|d| *d != dienst);
                match dienst {
                    OfficialRole::Sr => sr = Some(id),
                    OfficialRole::Ar => ar = Some(id),
                }
            }
            // Durchgang 2: Rückfall auf die erste passende freie Person.
            for d in offen {
                let gewaehlt = e
                    .order
                    .iter()
                    .copied()
                    .find(|&id| Some(id) != sr && Some(id) != ar && kann(id, d));
                match d {
                    OfficialRole::Sr => sr = gewaehlt,
                    OfficialRole::Ar => ar = gewaehlt,
                }
            }
            (sr, ar)
        }
    }
}

/// Dienst eines Officials an einem Spiel. BTP: `Official1ID` = SR,
/// `Official2ID` = AR (an der BTP-Maske verifiziert, Messung 13.08.2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OfficialRole {
    /// Schiedsrichter (`Official1ID`).
    Sr,
    /// Aufschlagrichter (`Official2ID`).
    Ar,
}

/// Was eine Person übernehmen darf (Spec `sr-rotationsmodus`, ADR 0079).
/// Gilt in allen Rotationsmodi: Die Automatik setzt niemanden in einen
/// Dienst, den seine Rolle nicht erlaubt. Eine Handeinteilung dagegen bleibt
/// möglich (nur Warnung).
///
/// Bewusst nicht `OfficialRole` genannt — das ist im Code schon der Dienst.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Einsatzrolle {
    /// SR und AR (Standard).
    #[default]
    #[serde(rename = "beides")]
    Beides,
    /// Nur Schiedsrichter.
    #[serde(rename = "sr")]
    NurSr,
    /// Nur Aufschlagrichter.
    #[serde(rename = "ar")]
    NurAr,
}

impl Einsatzrolle {
    /// Darf die Person diesen Dienst übernehmen?
    pub fn erlaubt(self, dienst: OfficialRole) -> bool {
        match self {
            Einsatzrolle::Beides => true,
            Einsatzrolle::NurSr => dienst == OfficialRole::Sr,
            Einsatzrolle::NurAr => dienst == OfficialRole::Ar,
        }
    }

    /// Standardrolle? (Für `skip_serializing_if` — „beides" steht nicht in
    /// der Datei, damit leere Zusatzdaten leer bleiben.)
    pub fn ist_beides(&self) -> bool {
        *self == Einsatzrolle::Beides
    }
}

/// Welche Rotationsliste gemeint ist. In Modus A (getrennt) gibt es eine
/// eigene AR-Liste; sonst wirkt alles auf die gemeinsame Hauptliste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reihenfolge {
    /// Gemeinsame Liste (B/C) bzw. SR-Liste (A).
    Haupt,
    /// AR-Liste (nur Modus A).
    Ar,
}

/// Zusatzdaten je Official — alles, was BTP nicht liefert.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OfficialExtra {
    /// Pausiert (Pause, kommt später, geht früher)? Die Rotation überspringt
    /// ihn, seine Position in der Reihenfolge bleibt erhalten.
    pub paused: bool,
    /// Stammverein — in BTS Light gepflegt, weil BTP am Official keinen
    /// Verein überträgt (Messung 13.08.2026). Grundlage der Konflikt-Warnung
    /// „Verein".
    pub club: String,
    /// Zusätzlich gesperrte Vereine (Personendaten — nie im Broadcast-State).
    pub blocked_clubs: Vec<String>,
    /// Gesperrte Spieler (BTP-`PlayerID`; Personendaten).
    pub blocked_players: Vec<i64>,
    /// Was die Person übernehmen darf (ADR 0079). Fehlt in älteren Dateien
    /// ⇒ „beides"; „beides" wird nicht geschrieben.
    #[serde(skip_serializing_if = "Einsatzrolle::ist_beides")]
    pub role: Einsatzrolle,
}

impl OfficialExtra {
    /// Trägt der Eintrag überhaupt etwas? Leere Einträge werden nicht
    /// aufbewahrt — sonst wüchse die Datei mit jedem Blick in einen Dialog.
    fn is_empty(&self) -> bool {
        !self.paused
            && self.club.is_empty()
            && self.blocked_clubs.is_empty()
            && self.blocked_players.is_empty()
            && self.role.ist_beides()
    }
}

/// Lokale SR/AR-Zuweisung eines Spiels (Overlay; trägt das BTP-Match eigene
/// `Official1ID`/`Official2ID`, gewinnt BTP — Spec „Konfliktregel").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchOfficials {
    pub sr: Option<i64>,
    pub ar: Option<i64>,
}

impl MatchOfficials {
    /// `None` = in BTS Light nie angefasst, `Some(0)` = **ausdrücklich
    /// keiner** (von Hand gelöst). Der Unterschied zählt: Ohne ihn ließe
    /// sich ein Schiedsrichter, den BTP schon trägt, nie wieder entfernen —
    /// der Rücksync fände keinen Unterschied und schriebe nie eine `0`.
    fn is_empty(&self) -> bool {
        self.sr.is_none() && self.ar.is_none()
    }
}

/// BTP-Stand eines Spiels zum Zeitpunkt einer lokalen Einteilung — je
/// Dienst `None` = noch nicht erfasst, `Some(0)` = BTP trug niemanden.
///
/// Grundlage der Frage „hat jemand IN BTP umgeteilt?" (AK 12, Spec
/// `tl-warteliste-schiedsrichter`): Weicht BTP von der lokalen Absicht ab,
/// zählt das nur dann als Entscheidung in BTP, wenn BTP inzwischen etwas
/// anderes führt als beim Einteilen. Liegt in der Datei statt im Sync-Lauf,
/// weil der bei jedem Speichern der Einstellungen neu startet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Ausgangswert {
    sr: Option<i64>,
    ar: Option<i64>,
    /// Eine frühere, noch unbestätigte eigene Einteilung desselben Dienstes
    /// (beim Umteilen, während der erste Write unterwegs war). Zeigt BTP
    /// sie, ist das die verspätete Übernahme des eigenen Writes — keine
    /// Umteilung in BTP.
    sr_vorher: Option<i64>,
    ar_vorher: Option<i64>,
}

/// Einen Dienst gegen den BTP-Stand prüfen (siehe
/// [`OfficialsStore::confirm`]): loslassen, wenn BTP den lokalen Wert trägt
/// oder vom Ausgangswert abweicht; sonst den Ausgangswert erfassen. Ein
/// bestätigtes „ohne" (`Some(0)`, BTP führt niemanden) bleibt stehen
/// (ADR 0080).
fn pruefe_dienst(
    lokal: &mut Option<i64>,
    ausgang: &mut Option<i64>,
    vorher: &mut Option<i64>,
    btp: Option<i64>,
) {
    let Some(wert) = *lokal else {
        *ausgang = None;
        *vorher = None;
        return;
    };
    let btp = dienst(btp).unwrap_or(0);
    let in_btp_umgeteilt = ausgang.is_some_and(|b| b != btp) && *vorher != Some(btp);
    if wert == 0 && btp == 0 {
        // „Ohne" bleibt dauerhaft stehen (ADR 0080): BTP führt ebenfalls
        // niemanden, also ist es bestätigt — auch wenn BTP vorher jemanden
        // trug (dann ist die 0 unser eigener Write, keine Umteilung in BTP).
        // Loslassen hieße, dass ein Lauf, der offene Dienste auffüllt, das
        // Spiel im nächsten Takt wieder besetzt. Der Ausgangswert 0 sorgt
        // dafür, dass eine spätere Einteilung in BTP trotzdem gewinnt.
        // Solche Einträge bleiben bis zum Turnierwechsel liegen — höchstens
        // einer je Spiel.
        *ausgang = Some(0);
        *vorher = None;
    } else if btp == wert || in_btp_umgeteilt {
        *lokal = None;
        *ausgang = None;
        *vorher = None;
    } else if ausgang.is_none() {
        *ausgang = Some(btp);
    }
}

/// Eine Official-ID als echter Dienst: `Some(0)` (ausdrücklich keiner) und
/// `None` werden beide zu „kein Dienst".
fn dienst(id: Option<i64>) -> Option<i64> {
    id.filter(|v| *v > 0)
}

/// Reserviert ein wartendes Spiel seine Besetzung (ADR 0078)? Ja, wenn es
/// am heutigen Tag oder davor angesetzt ist oder gar keine Ansetzung hat.
/// Spiele späterer Tage reservieren nicht — sonst verkleinerten
/// BTP-Vorabeinträge den Pool tagelang. „Heute" kommt von außen (lokales
/// Datum des Turnier-PCs), damit der Kern ohne Uhr auskommt.
pub fn ist_reservierend(m: &BtpMatch, heute: chrono::NaiveDate) -> bool {
    use chrono::Datelike;
    if m.status != MatchStatus::Scheduled {
        return false;
    }
    let heute_schluessel =
        i64::from(heute.year()) * 10_000 + i64::from(heute.month()) * 100 + i64::from(heute.day());
    // `planned_time` ist `YYYYMMDDHHMM` — ohne die Uhrzeit bleibt der Tag.
    m.planned_time
        .is_none_or(|t| t / 10_000 <= heute_schluessel)
}

/// Wer ist wo gebunden? Die **eine** Quelle für Dienst und Reservierung
/// (ADR 0078) — Rotation, Meetingpoint-Vorab-Verteilung, Übernahme des
/// Vorab-SR und Auto-Vergabe lesen alle hier, damit keine zwei Stellen
/// verschieden rechnen.
///
/// - **Im Dienst:** SR/AR eines laufenden Spiels (Sicht `effective`, so
///   wie das Feld es zeigt).
/// - **Reserviert:** SR/AR eines reservierenden wartenden Spiels (Sicht
///   [`OfficialsStore::fuer_schreiben`]; `Some(0)` reserviert niemanden).
///
/// Gezählt wird nur, wer in der BTP-Officials-Liste steht. Das Ende der
/// Reservierung ergibt sich aus dem Status: aufs Feld ⇒ im Dienst,
/// beendet ⇒ frei, vom Feld genommen ⇒ wieder reserviert. Ohne
/// Schiedsrichter-Betrieb ist die Belegung leer (AK 19).
#[derive(Debug, Clone, Default)]
pub struct Belegung {
    /// Official-ID → (Match-ID, CourtID) der laufenden Spiele. Eine Liste,
    /// weil ein inkonsistenter Stand (BTP oder Hand) dieselbe Person an zwei
    /// Feldern führen kann — dann ist sie für beide gebunden.
    im_dienst: HashMap<i64, Vec<(i64, Option<i64>)>>,
    /// Official-ID → Match-IDs der wartenden Spiele, für die er reserviert
    /// ist (aufsteigend).
    reserviert: HashMap<i64, Vec<i64>>,
}

impl Belegung {
    /// Belegung aus dem Roster-Speicher und dem BTP-Schnappschuss.
    pub fn aus(store: &OfficialsStore, snap: &BtpSnapshot, heute: chrono::NaiveDate) -> Self {
        let mut out = Self::default();
        if !store.enabled() {
            return out;
        }
        let bekannt: HashSet<i64> = snap.officials.iter().map(|o| o.id).collect();
        // Eine gerufene Partie mit Rotations-Zuteilung reserviert unabhängig
        // vom Tag (ADR 0080) — sonst vergäbe die Rotation ihre Leute doppelt.
        let mit_merker: HashSet<i64> = store.vorab_rotation_spiele().into_iter().collect();
        for m in &snap.matches {
            let besetzung = match m.status {
                MatchStatus::OnCourt => store.effective(m.id, m.official1_id, m.official2_id),
                MatchStatus::Scheduled
                    if ist_reservierend(m, heute) || mit_merker.contains(&m.id) =>
                {
                    store.fuer_schreiben(m.id, m.official1_id, m.official2_id)
                }
                _ => continue,
            };
            for id in [dienst(besetzung.sr), dienst(besetzung.ar)]
                .into_iter()
                .flatten()
                .filter(|id| bekannt.contains(id))
            {
                if m.status == MatchStatus::OnCourt {
                    out.im_dienst
                        .entry(id)
                        .or_default()
                        .push((m.id, m.court_id));
                } else {
                    let liste = out.reserviert.entry(id).or_default();
                    if !liste.contains(&m.id) {
                        liste.push(m.id);
                    }
                }
            }
        }
        for liste in out.reserviert.values_mut() {
            liste.sort_unstable();
        }
        for felder in out.im_dienst.values_mut() {
            felder.sort_unstable();
        }
        out
    }

    /// Tut der Official gerade auf einem Feld Dienst? Liefert die CourtID
    /// (`None` im Ergebnis heißt: nicht im Dienst; ein laufendes Spiel ohne
    /// CourtID meldet `Some(0)`).
    pub fn im_dienst_bei(&self, official_id: i64) -> Option<i64> {
        self.im_dienst
            .get(&official_id)
            .and_then(|felder| felder.first())
            .map(|(_, court)| court.unwrap_or(0))
    }

    /// Die wartenden Spiele, für die der Official reserviert ist.
    pub fn reserviert_fuer(&self, official_id: i64) -> &[i64] {
        self.reserviert
            .get(&official_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// An zwei oder mehr wartenden Spielen eingeteilt? Dienst auf einem
    /// laufenden Feld zählt nicht als Doppel-Einteilung (AK 11).
    pub fn doppelt(&self, official_id: i64) -> bool {
        self.reserviert_fuer(official_id).len() >= 2
    }

    /// Im Dienst oder irgendwo reserviert?
    pub fn ist_belegt(&self, official_id: i64) -> bool {
        self.im_dienst.contains_key(&official_id) || self.reserviert.contains_key(&official_id)
    }

    /// Wer für **dieses** Spiel nicht in Frage kommt: Dienst an einem
    /// anderen Spiel oder reserviert für ein anderes. Die eigene Einteilung
    /// bleibt draußen — sonst sperrte sich ein Spiel selbst.
    pub fn belegt_ausser(&self, match_id: i64) -> HashSet<i64> {
        let dienst = self
            .im_dienst
            .iter()
            .filter(|(_, spiele)| spiele.iter().any(|(m, _)| *m != match_id))
            .map(|(id, _)| *id);
        let reserviert = self
            .reserviert
            .iter()
            .filter(|(_, spiele)| spiele.iter().any(|m| *m != match_id))
            .map(|(id, _)| *id);
        dienst.chain(reserviert).collect()
    }

    /// Alle Gebundenen (Dienst und Reservierung).
    pub fn alle(&self) -> HashSet<i64> {
        self.im_dienst
            .keys()
            .chain(self.reserviert.keys())
            .copied()
            .collect()
    }

    /// Die Spielnummer (Papierplan) für die Marke „reserviert Sp. N" in der
    /// SR-Liste (AK 16). Bei mehreren Reservierungen die **kleinste**
    /// Spielnummer — ein fester, für alle Geräte gleicher Wert, der nicht
    /// mit der Warteliste umspringt; die Doppel-Einteilung meldet ohnehin
    /// die Zeile („SR doppelt"). `None`, wenn nicht reserviert oder keines
    /// der Spiele eine Nummer trägt (dann nur „reserviert").
    pub fn reserviert_spielnummer(&self, official_id: i64, snap: &BtpSnapshot) -> Option<i64> {
        self.reserviert_fuer(official_id)
            .iter()
            .filter_map(|id| snap.matches.iter().find(|m| m.id == *id))
            .filter_map(|m| m.match_num)
            .min()
    }
}

/// Darf die Besetzung dieses Spiels so geändert werden (AK 4)? Prüft eine
/// Einteilung (`official_id = Some(..)`) oder ein Lösen (`None`), **bevor**
/// etwas gespeichert wird — TL-Web (LAN wie Cloud) und Desktop teilen
/// sich diese Prüfung, weil über die Cloud jede beliebige Match-ID
/// ankommen kann.
///
/// Abgelehnt werden:
/// - ein Spiel, das der aktuelle Turnierstand nicht kennt (auch: noch kein
///   Turnierstand),
/// - ein beendetes Spiel — Einteilen **und** Lösen, die Besetzung steht
///   dann im Spielbericht,
/// - eine Person, die nicht in der BTP-Officials-Liste steht (sie ginge
///   sonst als unbekannte ID nach BTP),
/// - dieselbe Person als SR **und** AR desselben Spiels, in der Sicht
///   [`OfficialsStore::fuer_schreiben`] (also gegen das, was nach BTP
///   ginge, nicht gegen den zuletzt bestätigten Stand).
///
/// Der Fehlertext geht unverändert an die Turnierleitung.
pub fn pruefe_einteilung(
    store: &OfficialsStore,
    snap: Option<&BtpSnapshot>,
    match_id: i64,
    role: OfficialRole,
    official_id: Option<i64>,
) -> Result<(), String> {
    let unbekannt = || format!("Spiel {match_id} gibt es im aktuellen Turnierstand nicht.");
    let Some(snap) = snap else {
        return Err(unbekannt());
    };
    let Some(m) = snap.matches.iter().find(|m| m.id == match_id) else {
        return Err(unbekannt());
    };
    let nummer = m.match_num.unwrap_or(m.id);
    if m.status == MatchStatus::Finished {
        return Err(format!(
            "Spiel {nummer} ist schon beendet — die Besetzung lässt sich nicht mehr ändern."
        ));
    }
    let Some(neu) = official_id else {
        return Ok(());
    };
    // `0`/negativ ist keine Person — `assign` verwürfe sie still, und die
    // Seite meldete „eingeteilt", obwohl nichts geschah.
    if neu <= 0 {
        return Err("Keine gültige Person ausgewählt.".to_string());
    }
    let Some(person) = snap.official(neu) else {
        return Err(format!(
            "Diese Person steht nicht in der Schiedsrichter-Liste aus BTP (ID {neu})."
        ));
    };
    let jetzt = store.fuer_schreiben(m.id, m.official1_id, m.official2_id);
    let (andere, andere_rolle) = match role {
        OfficialRole::Sr => (jetzt.ar, "Aufschlagrichter"),
        OfficialRole::Ar => (jetzt.sr, "Schiedsrichter"),
    };
    if dienst(andere) == Some(neu) {
        return Err(format!(
            "{} ist an Spiel {nummer} schon {andere_rolle} — eine Person kann nicht \
             zugleich Schiedsrichter und Aufschlagrichter sein.",
            person.display_name()
        ));
    }
    Ok(())
}

/// Feldweise Schalter (Spec Nr. 6): SR-Rotation, AR-Rotation und
/// Tabletbediener-Vergabe je Feld — daraus ergeben sich die drei
/// Betriebsformen. Default **alles aktiv**, damit sich das Verhalten
/// bestehender Installationen (Bediener-Vergabe) nicht ändert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CourtSwitches {
    /// SR-Rotation auf diesem Feld aktiv?
    pub sr: bool,
    /// AR-Rotation auf diesem Feld aktiv?
    pub ar: bool,
    /// Zähltafelbediener-Vergabe auf diesem Feld aktiv?
    pub operator: bool,
}

impl Default for CourtSwitches {
    fn default() -> Self {
        Self {
            sr: true,
            ar: true,
            operator: true,
        }
    }
}

/// Dateiform. Im Kopf steht das Turnier — passt es beim Start nicht zum
/// laufenden Turnier, wird der Inhalt verworfen (ADR 0022: lieber verwerfen
/// als falsch zuordnen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OfficialsFile {
    /// BTP-Turniername (Setting 1001) — der Schlüssel des Stands.
    #[serde(default)]
    tournament: String,
    /// Rotationsreihenfolge (Official-IDs). In Modus A die SR-Liste, sonst
    /// die gemeinsame Liste.
    #[serde(default)]
    order: Vec<i64>,
    /// Eigene AR-Liste für Modus A (ADR 0079). `None` = noch nie gebraucht;
    /// gelesen wird dann eine Kopie von `order`. Angelegt wird sie erst beim
    /// ersten Schreibbedarf — so entsteht keine Migration, und eine ältere
    /// App liest die Datei weiter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order_ar: Option<Vec<i64>>,
    /// Official-ID → Zusatzdaten.
    #[serde(default)]
    extras: HashMap<i64, OfficialExtra>,
    /// Match-ID → lokale Zuweisung (bleibt nach Spielende liegen).
    #[serde(default)]
    assignments: HashMap<i64, MatchOfficials>,
    /// Match-ID → BTP-Stand beim Einteilen (nur solange eine Zuweisung
    /// besteht; fehlt in älteren Dateien ⇒ wird beim nächsten Abgleich
    /// erfasst).
    #[serde(default)]
    ausgang: HashMap<i64, Ausgangswert>,
    /// CourtID → feldweise Schalter (fehlt = Default „alles aktiv").
    #[serde(default)]
    courts: HashMap<i64, CourtSwitches>,
    /// Match-ID → Herkunfts-Merker der Zuteilung beim Aufruf (ADR 0080).
    /// Unterscheidet Rotations-Zuteilungen von Hand- und BTP-Einteilungen:
    /// Nur sie werden frei, wenn der Aufruf ohne Feld endet. Überlebt
    /// `confirm` und Neustart.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    vorab_rotation: HashMap<i64, VorabRotation>,
}

/// Herkunfts-Merker je Dienst eines Spiels (ADR 0080).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct VorabRotation {
    #[serde(skip_serializing_if = "Option::is_none")]
    sr: Option<VorabDienst>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ar: Option<VorabDienst>,
}

impl VorabRotation {
    fn rolle(&mut self, role: OfficialRole) -> &mut Option<VorabDienst> {
        match role {
            OfficialRole::Sr => &mut self.sr,
            OfficialRole::Ar => &mut self.ar,
        }
    }
    fn leer(&self) -> bool {
        self.sr.is_none() && self.ar.is_none()
    }
}

/// Die von der Rotation beim Aufruf gesetzte Person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct VorabDienst {
    id: i64,
    /// Freigabe angestoßen: lokal steht `Some(0)`, die 0 ist unterwegs
    /// nach BTP. Bis zur Bestätigung gilt der Dienst als offen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    frei: bool,
}

/// Was eine Zuweisung mit dem Herkunfts-Merker der Rolle macht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Merker {
    /// Hand, „ohne", Tausch oder Feld-Rotation: Merker der Rolle weg.
    Entfernen,
    /// Zuteilung beim Aufruf: Merker auf diese Person.
    Setzen,
}

/// Ergebnis eines Ladeversuchs. Der Unterschied zwischen „gibt es nicht"
/// und „komme gerade nicht dran" entscheidet, ob überschrieben werden darf.
enum Ladung {
    /// Ein gelesener Stand (kann zu einem anderen Turnier gehören).
    // Geboxt: Die Datei ist groß, die übrigen Varianten sind leer.
    Stand(Box<OfficialsFile>),
    /// Keine Datei (oder Persistenz aus, oder kaputter Inhalt) ⇒ leer starten.
    Leer,
    /// Datei vorhanden, aber nicht lesbar ⇒ nicht anfassen, später erneut.
    Unlesbar,
}

#[derive(Default)]
struct Inner {
    file: OfficialsFile,
    /// Wurde die Datei schon einmal gelesen? Der Pfad steht beim Start fest,
    /// das Turnier kommt erst mit dem ersten Snapshot — geladen wird deshalb
    /// beim ersten `set_tournament` (Muster BTP-Nachschub-Queue, ADR 0018).
    loaded: bool,
}

/// Der Roster-Speicher. Lebt im [`TabletState`](super::state::TabletState),
/// damit LAN-Server, Relay-Client und Tauri-Commands denselben Stand sehen.
#[derive(Default)]
pub struct OfficialsStore {
    /// Ablage-Datei. `None` = Persistenz aus (Tests, Slave-Betrieb).
    path: RwLock<Option<PathBuf>>,
    /// Läuft das Turnier mit Schiedsrichtern? Reiner Laufzeit-Spiegel der
    /// Konfiguration (nicht Teil der Datei — der Schalter ist geräteweit).
    enabled: RwLock<bool>,
    /// Automatische Rotation (SR, AR) — ebenfalls Spiegel der Konfiguration.
    rotation: RwLock<(bool, bool)>,
    /// Einteilungsprinzip (ADR 0079) — Spiegel der Konfiguration, aus
    /// demselben Grund wie `rotation`: Der Sync-Lauf liest seine Config nie
    /// neu.
    rotation_mode: RwLock<RotationMode>,
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `scores_persist_lock`).
    persist_lock: Mutex<()>,
    /// Vergabe-Sperre (ADR 0080): Ein Hand-Aufruf läuft außerhalb des
    /// Sync-Laufs. Ohne sie könnten Aufruf und Feld-Rotation dieselbe Person
    /// zweimal vergeben. Nie über ein `await` halten.
    vergabe: Mutex<()>,
}

impl OfficialsStore {
    /// Ablage-Datei setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_path(&self, path: PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        *self.path.write().unwrap() = Some(path);
        // Ein neuer Pfad ist ungelesen — sonst hinge die Richtigkeit daran,
        // dass `set_path` immer VOR dem ersten Snapshot läuft; in der
        // umgekehrten Reihenfolge überschriebe der leere Stand die Datei.
        self.inner.lock().unwrap().loaded = false;
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot). Beim ersten
    /// Mal wird der passende Stand geladen; bei Turnierwechsel verworfen.
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return; // Startphase ohne Namen ändert nichts
        }
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.loaded && inner.file.tournament == name {
                return; // unverändert — der Normalfall jedes Sync-Ticks
            }
            if !inner.loaded {
                match self.load_file() {
                    // Erststart: Nur ein Stand DESSELBEN Turniers wird
                    // übernommen — sonst gehörten fremde BTP-IDs und fremde
                    // Sperrlisten zum neuen Turnier (ADR 0022).
                    Ladung::Stand(file) => {
                        inner.loaded = true;
                        if file.tournament == name {
                            inner.file = *file;
                            return;
                        }
                    }
                    Ladung::Leer => inner.loaded = true,
                    // Datei da, aber gerade nicht lesbar (Virenscanner,
                    // hängendes Handle): NICHTS anfassen und NICHT schreiben
                    // — der nächste Snapshot versucht es erneut. Andernfalls
                    // löschte ein Moment Pech die Einteilung des Turniers.
                    Ladung::Unlesbar => return,
                }
            }
            // Turnierwechsel (oder fremder Dateistand): verwerfen.
            inner.file = OfficialsFile {
                tournament: name.to_string(),
                ..Default::default()
            };
        }
        // Der leere Stand geht sofort auf die Platte: Ein Absturz direkt
        // nach dem Wechsel darf die Personendaten des alten Turniers nicht
        // wieder auferstehen lassen.
        self.persist();
    }

    /// Wird mit Schiedsrichtern gespielt (`officials.enabled`)? Laufzeit-
    /// Spiegel der Konfiguration, damit Anzeige-Pfade (Feldübersicht,
    /// TL-State, Tablet) das Feature ausblenden können, ohne die
    /// `AppConfig` zu kennen. Aus ⇒ nirgends ein SR/AR-Element (Spec Nr. 1).
    pub fn enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    /// Schalter setzen (beim App-Start und beim Speichern der Einstellungen).
    pub fn set_enabled(&self, on: bool) {
        *self.enabled.write().unwrap() = on;
    }

    /// Laufen die automatischen Rotationen (SR, AR)?
    pub fn rotation(&self) -> (bool, bool) {
        *self.rotation.read().unwrap()
    }

    /// Rotations-Schalter setzen (wie [`set_enabled`](Self::set_enabled)).
    ///
    /// Die globalen Schalter liegen bewusst **hier** und nicht in der
    /// Sync-Konfiguration: Der Sync-Lauf bekommt seine Config einmal beim
    /// Start und liest sie nie neu — ein Häkchen in den Einstellungen bliebe
    /// sonst bis zum Neustart der Übertragung wirkungslos.
    pub fn set_rotation(&self, sr: bool, ar: bool) {
        *self.rotation.write().unwrap() = (sr, ar);
    }

    /// Einteilungsprinzip der Rotation (ADR 0079).
    pub fn rotation_mode(&self) -> RotationMode {
        *self.rotation_mode.read().unwrap()
    }

    /// Einteilungsprinzip setzen (beim Start und beim Speichern der
    /// Einstellungen). Wirkt ab der nächsten Besetzung; es werden keine
    /// Daten umgebaut.
    pub fn set_rotation_mode(&self, mode: RotationMode) {
        *self.rotation_mode.write().unwrap() = mode;
    }

    /// Turnier, zu dem der aktuelle Stand gehört (leer = noch keins).
    pub fn tournament(&self) -> String {
        self.inner.lock().unwrap().file.tournament.clone()
    }

    // ── Zusatzdaten je Official ───────────────────────────────────────

    /// Zusatzdaten eines Officials (nie `None` — ein unbekannter Official
    /// hat schlicht Default-Zusatzdaten).
    pub fn extra(&self, official_id: i64) -> OfficialExtra {
        self.inner
            .lock()
            .unwrap()
            .file
            .extras
            .get(&official_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Pausiert-Schalter setzen.
    pub fn set_paused(&self, official_id: i64, paused: bool) {
        self.mit_extra(official_id, |e| e.paused = paused);
    }

    /// Stammverein pflegen (leer = keiner).
    pub fn set_club(&self, official_id: i64, club: &str) {
        let club = club.trim().to_string();
        self.mit_extra(official_id, |e| e.club = club);
    }

    /// Warnungen, wenn eine Besetzung der Rolle widerspricht (ADR 0079):
    /// „SR: nur AR" bzw. „AR: nur SR". Nur Warnung — eine Einteilung von
    /// Hand oder in BTP bleibt erlaubt (der SR⇄AR-Tausch muss gehen, und BTP
    /// gewinnt ohnehin, R2).
    pub fn rollen_warnung(&self, sr: Option<i64>, ar: Option<i64>) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(id) = sr.filter(|v| *v > 0) {
            if !self.extra(id).role.erlaubt(OfficialRole::Sr) {
                out.push("SR: nur AR".to_string());
            }
        }
        if let Some(id) = ar.filter(|v| *v > 0) {
            if !self.extra(id).role.erlaubt(OfficialRole::Ar) {
                out.push("AR: nur SR".to_string());
            }
        }
        out
    }

    /// Rolle setzen (beides / nur SR / nur AR, ADR 0079).
    pub fn set_role(&self, official_id: i64, role: Einsatzrolle) {
        self.mit_extra(official_id, |e| e.role = role);
    }

    /// Sperrlisten setzen (ersetzen beide Listen).
    pub fn set_blocklists(&self, official_id: i64, clubs: Vec<String>, players: Vec<i64>) {
        let clubs: Vec<String> = clubs
            .into_iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        self.mit_extra(official_id, |e| {
            e.blocked_clubs = clubs;
            e.blocked_players = players;
        });
    }

    /// Zusatzdaten ändern und speichern. Ein Eintrag, der nichts mehr trägt,
    /// wird entfernt — die Datei soll nicht mit Leereinträgen wachsen.
    fn mit_extra(&self, official_id: i64, f: impl FnOnce(&mut OfficialExtra)) {
        {
            let mut inner = self.inner.lock().unwrap();
            let e = inner.file.extras.entry(official_id).or_default();
            f(e);
            if e.is_empty() {
                inner.file.extras.remove(&official_id);
            }
        }
        self.persist();
    }

    // ── Rotationsreihenfolge ──────────────────────────────────────────

    /// Die gespeicherte Rotationsreihenfolge.
    pub fn order(&self) -> Vec<i64> {
        self.inner.lock().unwrap().file.order.clone()
    }

    /// Die AR-Liste (Modus A). Gibt es sie noch nicht, gilt eine Kopie der
    /// Hauptliste. Lesen legt nichts an und speichert nichts.
    pub fn order_ar(&self) -> Vec<i64> {
        ar_liste(&self.inner.lock().unwrap().file)
    }

    /// Reihenfolge komplett setzen (manuelles Umsortieren).
    pub fn set_order(&self, order: Vec<i64>) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.file.order = order;
        }
        self.persist();
    }

    /// Roster mit der BTP-Liste abgleichen: neue Officials hinten anhängen,
    /// bekannte in ihrer Position lassen. Officials, die aus BTP
    /// verschwinden, bleiben in der Reihenfolge stehen — kehren sie zurück,
    /// gelten Position und Zusatzdaten wieder (Spec „Fehlerfälle").
    pub fn sync_roster(&self, btp_ids: &[i64]) {
        {
            let mut inner = self.inner.lock().unwrap();
            let mut neu: Vec<i64> = Vec::new();
            for id in btp_ids.iter().copied() {
                // Auch gegen `neu` prüfen: Ein doppelt gelisteter Official
                // bekäme sonst zwei Plätze und käme doppelt so oft dran.
                if id > 0 && !inner.file.order.contains(&id) && !neu.contains(&id) {
                    neu.push(id);
                }
            }
            if neu.is_empty() {
                return;
            }
            // Eine schon angelegte AR-Liste bekommt die Neuen ebenfalls
            // hinten (ohne Doppel) — sonst fehlten sie dort bis zur
            // Selbstheilung beim Lesen.
            if let Some(ar) = inner.file.order_ar.as_mut() {
                for id in &neu {
                    if !ar.contains(id) {
                        ar.push(*id);
                    }
                }
            }
            inner.file.order.extend(neu);
        }
        self.persist();
    }

    // ── Zuweisungen (am Match, nicht am Feld) ─────────────────────────

    /// Lokale Zuweisung eines Spiels.
    pub fn assignment(&self, match_id: i64) -> MatchOfficials {
        self.inner
            .lock()
            .unwrap()
            .file
            .assignments
            .get(&match_id)
            .copied()
            .unwrap_or_default()
    }

    /// Alle lokalen Zuweisungen (Grundlage der Einsatz-Ableitung).
    pub fn assignments(&self) -> HashMap<i64, MatchOfficials> {
        self.inner.lock().unwrap().file.assignments.clone()
    }

    /// Official einem Spiel zuweisen.
    pub fn assign(&self, match_id: i64, role: OfficialRole, official_id: i64) {
        if match_id <= 0 || official_id <= 0 {
            return;
        }
        self.mit_zuweisung(match_id, role, official_id, Merker::Entfernen);
    }

    /// Zuweisung eines Spiels lösen.
    ///
    /// Merkt sich das als **ausdrückliches** „keiner" (`Some(0)`), nicht als
    /// „nie angefasst": Nur so schreibt der Rücksync die `0` nach BTP und
    /// der Dienst verschwindet auch dort (ADR 0021).
    pub fn clear_assignment(&self, match_id: i64, role: OfficialRole) {
        if match_id <= 0 {
            return;
        }
        self.mit_zuweisung(match_id, role, 0, Merker::Entfernen);
    }

    /// Alle Zuweisungen räumen — beim Abschalten von `officials.enabled`
    /// mitten im Turnier (Spec Nr. 1, analog `clear_scorekeeper_assignments`).
    pub fn clear_assignments(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.file.assignments.is_empty() && inner.file.vorab_rotation.is_empty() {
                return;
            }
            inner.file.assignments.clear();
            inner.file.ausgang.clear();
            inner.file.vorab_rotation.clear();
        }
        self.persist();
    }

    /// Einen Dienst lokal setzen (`0` = ausdrücklich keiner). Eine neue
    /// Einteilung misst gegen den **dann** gültigen BTP-Stand: Der
    /// Ausgangswert des Dienstes wird verworfen und beim nächsten Abgleich
    /// neu erfasst. Ausnahme: Ist die bisherige Einteilung noch unbestätigt
    /// (ihr Write womöglich unterwegs), bleibt der Ausgangswert, und die
    /// bisherige Einteilung wird als „eigene" gemerkt — sonst hielte der
    /// Abgleich ihre verspätete Übernahme für eine Umteilung in BTP.
    fn mit_zuweisung(&self, match_id: i64, role: OfficialRole, wert: i64, merker: Merker) {
        {
            let mut inner = self.inner.lock().unwrap();
            let file = &mut inner.file;
            // Herkunft im selben Lock wie der Wert — sonst sähe ein
            // gleichzeitiger Leser eine Rotations-Zuteilung ohne Merker.
            let v = file.vorab_rotation.entry(match_id).or_default();
            *v.rolle(role) = match merker {
                Merker::Setzen if wert > 0 => Some(VorabDienst {
                    id: wert,
                    frei: false,
                }),
                _ => None,
            };
            if v.leer() {
                file.vorab_rotation.remove(&match_id);
            }
            let a = file.assignments.entry(match_id).or_default();
            let b = file.ausgang.entry(match_id).or_default();
            let (lokal, ausgang, vorher) = match role {
                OfficialRole::Sr => (&mut a.sr, &mut b.sr, &mut b.sr_vorher),
                OfficialRole::Ar => (&mut a.ar, &mut b.ar, &mut b.ar_vorher),
            };
            match *lokal {
                Some(alt) if alt != wert => *vorher = Some(alt),
                Some(_) => {}
                None => {
                    *ausgang = None;
                    *vorher = None;
                }
            }
            *lokal = Some(wert);
            if *b == Ausgangswert::default() {
                file.ausgang.remove(&match_id);
            }
        }
        self.persist();
    }

    /// Die **wirksame** Besetzung eines Spiels: Trägt das BTP-Match
    /// `Official1ID`/`Official2ID`, gewinnt BTP (R2); sonst gilt die lokale
    /// Zuweisung (Overlay-Modell, Spec „Konfliktregel").
    pub fn effective(
        &self,
        match_id: i64,
        btp_sr: Option<i64>,
        btp_ar: Option<i64>,
    ) -> MatchOfficials {
        let lokal = self.assignment(match_id);
        MatchOfficials {
            sr: dienst(btp_sr).or(dienst(lokal.sr)),
            ar: dienst(btp_ar).or(dienst(lokal.ar)),
        }
    }

    /// Die Besetzung in der Sicht **„Hand vor BTP"**: die lokale Absicht,
    /// falls vorhanden, sonst der BTP-Wert. `Some(0)` heißt ausdrücklich
    /// kein Dienst.
    ///
    /// Grundlage des Rücksyncs (`TabletState::officials_for_write`) und der
    /// Reservierung wartender Spiele (ADR 0078): Wer gerade umgeteilt wurde,
    /// ist ab sofort reserviert, nicht erst nach der Bestätigung durch BTP.
    /// Die Anzeige bleibt bei [`effective`](Self::effective) (R2).
    pub fn fuer_schreiben(
        &self,
        match_id: i64,
        btp_sr: Option<i64>,
        btp_ar: Option<i64>,
    ) -> MatchOfficials {
        let lokal = self.assignment(match_id);
        MatchOfficials {
            sr: lokal.sr.or(btp_sr),
            ar: lokal.ar.or(btp_ar),
        }
    }

    /// Freie Dienste eines neu belegten Felds aus der Reihenfolge besetzen
    /// (Spec Nr. 4). Belegte Dienste bleiben unangetastet — auch deshalb ist
    /// der Aufruf idempotent.
    ///
    /// Liefert die neu gesetzten Personen `(sr, ar)`.
    pub fn rotate_court(&self, input: RotationInput<'_>) -> (Option<i64>, Option<i64>) {
        if input.match_id <= 0 {
            return (None, None);
        }
        let wirksam = self.effective(input.match_id, input.btp_sr, input.btp_ar);
        // Ein ausdrücklich gelöster Dienst (`Some(0)`) ist **kein** offener
        // Dienst: Wer von Hand ohne Schiedsrichter spielen lässt, soll ihn
        // auch nach einem Neustart nicht zurückbekommen — dort sieht jedes
        // belegte Feld wie neu belegt aus. Ausnahme: eine freigegebene
        // Rotations-Zuteilung (`frei`, ADR 0080) — ihr `Some(0)` ist nur die
        // unterwegs befindliche Löschung, BTP zeigt womöglich noch die alte
        // Person.
        let lokal = self.assignment(input.match_id);
        let (frei_sr, frei_ar) = self.frei_merker(input.match_id);
        let sr_offen = input.sr && (frei_sr || (wirksam.sr.is_none() && lokal.sr.is_none()));
        let ar_offen = input.ar && (frei_ar || (wirksam.ar.is_none() && lokal.ar.is_none()));
        if !sr_offen && !ar_offen {
            return (None, None);
        }
        // Wer an DIESEM Spiel schon Dienst tut, kommt für den zweiten Dienst
        // nicht in Frage — dieselbe Person ist nie SR und AR zugleich.
        let mut vergeben: HashSet<i64> = input.belegt.clone();
        vergeben.extend(wirksam.sr);
        vergeben.extend(wirksam.ar);

        let (sr, ar) = self.waehle(
            input.modus,
            input.bekannt,
            &vergeben,
            input.players,
            input.vorrolle,
            sr_offen,
            ar_offen,
        );
        // Niemand frei ⇒ das Feld bleibt ohne diesen Dienst.
        let merker = match input.herkunft {
            Herkunft::Aufruf => Merker::Setzen,
            Herkunft::Feld => Merker::Entfernen,
        };
        if let Some(id) = sr {
            self.mit_zuweisung(input.match_id, OfficialRole::Sr, id, merker);
        }
        if let Some(id) = ar {
            self.mit_zuweisung(input.match_id, OfficialRole::Ar, id, merker);
        }
        (sr, ar)
    }

    // ── Zuteilung beim Aufruf: Herkunft und Freigabe (ADR 0080) ───────

    /// Vergabe-Sperre nehmen (siehe Feld `vergabe`).
    pub fn vergabe_sperre(&self) -> std::sync::MutexGuard<'_, ()> {
        self.vergabe.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Eine Person als Zuteilung beim Aufruf eintragen (mit Merker) — für
    /// die einmalige Übernahme alter vorläufiger SR (ADR 0080).
    pub fn uebernimm_als_aufruf(&self, match_id: i64, role: OfficialRole, official_id: i64) {
        if match_id <= 0 || official_id <= 0 {
            return;
        }
        self.mit_zuweisung(match_id, role, official_id, Merker::Setzen);
    }

    /// Steht für dieses Spiel ein Herkunfts-Merker (irgendeine Rolle)?
    pub fn hat_vorab_rotation(&self, match_id: i64) -> bool {
        self.inner
            .lock()
            .unwrap()
            .file
            .vorab_rotation
            .contains_key(&match_id)
    }

    /// Die von der Rotation beim Aufruf gesetzte Person je Rolle (ohne
    /// freigegebene) — für Tests und Anzeige.
    pub fn vorab_rotation(&self, match_id: i64) -> (Option<i64>, Option<i64>) {
        let inner = self.inner.lock().unwrap();
        let v = inner
            .file
            .vorab_rotation
            .get(&match_id)
            .copied()
            .unwrap_or_default();
        let id = |d: Option<VorabDienst>| d.filter(|d| !d.frei).map(|d| d.id);
        (id(v.sr), id(v.ar))
    }

    /// Match-IDs mit Herkunfts-Merker.
    pub fn vorab_rotation_spiele(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .inner
            .lock()
            .unwrap()
            .file
            .vorab_rotation
            .keys()
            .copied()
            .collect();
        ids.sort_unstable();
        ids
    }

    fn frei_merker(&self, match_id: i64) -> (bool, bool) {
        let inner = self.inner.lock().unwrap();
        let v = inner
            .file
            .vorab_rotation
            .get(&match_id)
            .copied()
            .unwrap_or_default();
        (v.sr.is_some_and(|d| d.frei), v.ar.is_some_and(|d| d.frei))
    }

    /// Eine Rotations-Zuteilung freigeben (Aufruf endet ohne Feld, Feld ohne
    /// diesen Dienst). Nur mit gültigem Merker; Hand- und BTP-Einteilungen
    /// bleiben. Hinterlässt **kein** bleibendes `Some(0)`:
    /// - BTP trägt die Person nicht ⇒ lokal weg.
    /// - BTP trägt sie (unser Write kam schon an) ⇒ lokal `Some(0)` mit
    ///   `frei`; die 0 geht nach BTP, `confirm` räumt nach der Bestätigung.
    ///
    /// Liefert `true`, wenn etwas freigegeben wurde.
    pub fn rotation_freigeben(&self, match_id: i64, role: OfficialRole, btp: Option<i64>) -> bool {
        {
            let mut inner = self.inner.lock().unwrap();
            let file = &mut inner.file;
            let Some(v) = file.vorab_rotation.get_mut(&match_id) else {
                return false;
            };
            let Some(d) = *v.rolle(role) else {
                return false;
            };
            if d.frei {
                return false;
            }
            let a = file.assignments.entry(match_id).or_default();
            let lokal = match role {
                OfficialRole::Sr => &mut a.sr,
                OfficialRole::Ar => &mut a.ar,
            };
            // Gültig nur, solange die Sicht „Hand vor BTP" noch genau diese
            // Person trägt — sonst hat jemand anders entschieden.
            if lokal.or(btp).filter(|x| *x > 0) != Some(d.id) {
                *v.rolle(role) = None;
            } else if dienst(btp) == Some(d.id) {
                *lokal = Some(0);
                *v.rolle(role) = Some(VorabDienst {
                    id: d.id,
                    frei: true,
                });
            } else {
                *lokal = None;
                *v.rolle(role) = None;
                if let Some(b) = file.ausgang.get_mut(&match_id) {
                    match role {
                        OfficialRole::Sr => {
                            b.sr = None;
                            b.sr_vorher = None;
                        }
                        OfficialRole::Ar => {
                            b.ar = None;
                            b.ar_vorher = None;
                        }
                    }
                }
            }
            if v.leer() {
                file.vorab_rotation.remove(&match_id);
            }
            if file
                .assignments
                .get(&match_id)
                .is_some_and(|a| a.is_empty())
            {
                file.assignments.remove(&match_id);
                file.ausgang.remove(&match_id);
            }
        }
        self.persist();
        true
    }

    /// Kommt ein Spiel auf ein Feld, dessen Schalter einen Dienst nicht
    /// will, wird **nur** die Rotations-Zuteilung dieser Rolle gelöst
    /// (ADR 0080). Vor jedem Feld-Write aufrufen und zustandsbasiert für
    /// laufende Spiele.
    ///
    /// Bewusst nur der **Feldschalter**, nicht der globale
    /// `rotation_sr`/`rotation_ar`: Schaltet die Turnierleitung die Rotation
    /// mitten im Turnier ab, bleiben bestehende Zuteilungen stehen — es kommt
    /// nur nichts Neues hinzu (Spec R-D.6).
    pub fn rotation_loesen_am_feld(&self, m: &BtpMatch, court_id: i64) {
        if !self.hat_vorab_rotation(m.id) {
            return;
        }
        let sw = self.court_switches(court_id);
        if !sw.sr {
            self.rotation_freigeben(m.id, OfficialRole::Sr, m.official1_id);
        }
        if !sw.ar {
            self.rotation_freigeben(m.id, OfficialRole::Ar, m.official2_id);
        }
    }

    /// Merker gegen den Turnierstand pflegen (je Takt, vor dem Besetzen):
    /// - Spiel beendet ⇒ Merker weg; trägt BTP die Person nicht, auch die
    ///   nie geschriebene lokale Zuteilung (kein Einsatz ohne Dienst).
    /// - Merker ohne `frei` gilt nur, solange „Hand vor BTP" seine Person
    ///   trägt; ein freigegebener nur, solange lokal `Some(0)` steht.
    /// - Spiel fehlt im Snapshot ⇒ nichts (kurz leerer Abruf).
    pub fn pflege_vorab_rotation(&self, snap: &BtpSnapshot) {
        let geaendert = {
            let mut inner = self.inner.lock().unwrap();
            let file = &mut inner.file;
            let mut geaendert = false;
            let ids: Vec<i64> = file.vorab_rotation.keys().copied().collect();
            for mid in ids {
                let Some(m) = snap.matches.iter().find(|m| m.id == mid) else {
                    continue;
                };
                let lokal = file.assignments.get(&mid).copied().unwrap_or_default();
                let v = file
                    .vorab_rotation
                    .get_mut(&mid)
                    .expect("Schlüssel eben gelesen");
                for (role, l, btp) in [
                    (OfficialRole::Sr, lokal.sr, m.official1_id),
                    (OfficialRole::Ar, lokal.ar, m.official2_id),
                ] {
                    let Some(d) = *v.rolle(role) else { continue };
                    let gueltig = if m.status == MatchStatus::Finished {
                        false
                    } else if d.frei {
                        l == Some(0)
                    } else {
                        l.or(btp).filter(|x| *x > 0) == Some(d.id)
                    };
                    if gueltig {
                        continue;
                    }
                    *v.rolle(role) = None;
                    geaendert = true;
                    if m.status == MatchStatus::Finished && dienst(btp) != Some(d.id) {
                        if let Some(a) = file.assignments.get_mut(&mid) {
                            let feld = match role {
                                OfficialRole::Sr => &mut a.sr,
                                OfficialRole::Ar => &mut a.ar,
                            };
                            if *feld == Some(d.id) || (d.frei && *feld == Some(0)) {
                                *feld = None;
                            }
                        }
                    }
                }
                if v.leer() {
                    file.vorab_rotation.remove(&mid);
                }
                if file.assignments.get(&mid).is_some_and(|a| a.is_empty()) {
                    file.assignments.remove(&mid);
                    file.ausgang.remove(&mid);
                }
            }
            geaendert
        };
        if geaendert {
            self.persist();
        }
    }

    /// Wählt über [`waehle_besetzung`] auf einem einmal geklonten Stand
    /// (Listen, Zusatzdaten) — ein Lock statt einem je Kandidat. Frei ist,
    /// wen BTP kennt, wer nicht pausiert ist, nicht in `vergeben` steht und
    /// keinen Konflikt mit den Spielern hat. Konflikte werden **still**
    /// übersprungen — gewarnt wird nur bei manueller Zuweisung (Spec Nr. 4).
    #[allow(clippy::too_many_arguments)]
    pub fn waehle(
        &self,
        modus: RotationMode,
        bekannt: &[i64],
        vergeben: &HashSet<i64>,
        players: &[BtpPlayer],
        vorrolle: &HashMap<i64, OfficialRole>,
        sr_offen: bool,
        ar_offen: bool,
    ) -> (Option<i64>, Option<i64>) {
        let (order, order_ar, extras) = {
            let inner = self.inner.lock().unwrap();
            (
                inner.file.order.clone(),
                ar_liste(&inner.file),
                inner.file.extras.clone(),
            )
        };
        let leer = OfficialExtra::default();
        let extra = |id: i64| extras.get(&id).unwrap_or(&leer);
        let frei = |id: i64| {
            bekannt.contains(&id)
                && !vergeben.contains(&id)
                && !extra(id).paused
                && official_conflict(extra(id), players).is_none()
        };
        let rolle = |id: i64| extra(id).role;
        waehle_besetzung(&Wahleingabe {
            modus,
            order: &order,
            order_ar: &order_ar,
            frei: &frei,
            rolle: &rolle,
            vorrolle,
            sr_offen,
            ar_offen,
        })
    }

    /// Vorrolle je Official: der Dienst im zuletzt beendeten Spiel (ADR
    /// 0079). Dieselbe Sicht wie die Einsatz-Ableitung (`effective`, Hand,
    /// BTP und Automatik gleich); ein Walkover zählt nicht. Maßgeblich ist
    /// `(finished_at oder 0, match_id)` — gleiche Endezeit entscheidet die
    /// höhere Match-ID, ein Spiel ohne Endezeit ist älter als jedes
    /// gestempelte.
    pub fn vorrollen(&self, finished: &[FinishedMatch]) -> HashMap<i64, OfficialRole> {
        let assignments = self.assignments();
        let mut zuletzt: HashMap<i64, ((u64, i64), OfficialRole)> = HashMap::new();
        for f in finished.iter().filter(|f| !f.walkover) {
            let lokal = assignments.get(&f.match_id).copied().unwrap_or_default();
            let sr = dienst(f.btp_sr).or(dienst(lokal.sr));
            let ar = dienst(f.btp_ar).or(dienst(lokal.ar));
            let schluessel = (f.finished_at.unwrap_or(0), f.match_id);
            for (id, rolle) in [(sr, OfficialRole::Sr), (ar, OfficialRole::Ar)] {
                let Some(id) = id else { continue };
                let neuer = zuletzt
                    .get(&id)
                    .map(|(k, _)| schluessel > *k)
                    .unwrap_or(true);
                if neuer {
                    zuletzt.insert(id, (schluessel, rolle));
                }
            }
        }
        zuletzt.into_iter().map(|(id, (_, r))| (id, r)).collect()
    }

    /// Einen Official in der Hauptliste vor einen anderen ziehen
    /// (`before = None` ⇒ ans Ende). Unbekannte IDs ändern nichts.
    pub fn reorder(&self, official_id: i64, before: Option<i64>) {
        self.reorder_in(Reihenfolge::Haupt, official_id, before);
    }

    /// Wie [`reorder`](Self::reorder), aber in der gewählten Liste. Die
    /// AR-Liste wird dabei bei Bedarf als Kopie der Hauptliste angelegt; die
    /// jeweils andere Liste bleibt unberührt.
    pub fn reorder_in(&self, liste: Reihenfolge, official_id: i64, before: Option<i64>) {
        {
            let mut inner = self.inner.lock().unwrap();
            let file = &mut inner.file;
            if liste == Reihenfolge::Ar {
                // Eine unbekannte ID darf keine AR-Liste anlegen.
                if !ar_liste(file).contains(&official_id) {
                    return;
                }
                let geheilt = ar_liste(file);
                file.order_ar = Some(geheilt);
            }
            let order = match liste {
                Reihenfolge::Haupt => &mut file.order,
                Reihenfolge::Ar => file.order_ar.as_mut().expect("eben angelegt"),
            };
            if !verschiebe(order, official_id, before) {
                return;
            }
        }
        self.persist();
    }

    /// Einsätze je Official, abgeleitet aus den **beendeten** Spielen
    /// (Spec Nr. 11 — keine eigene Historien-Datenhaltung). Je Official
    /// chronologisch nach Endezeit.
    ///
    /// Ein Walkover zählt **nicht**: Ohne Spiel gab es keinen Dienst. Zähler
    /// und Detail-Liste kommen beide von hier, deshalb taucht das Spiel auch
    /// in der Liste nicht auf. Aufgabe und Disqualifikation zählen weiter —
    /// da stand jemand am Feld. Die Rotation (`move_to_end`) ist davon
    /// unberührt.
    pub fn appearances(&self, finished: &[FinishedMatch]) -> HashMap<i64, Vec<Appearance>> {
        let mut out: HashMap<i64, Vec<Appearance>> = HashMap::new();
        for f in finished.iter().filter(|f| !f.walkover) {
            let wirksam = self.effective(f.match_id, f.btp_sr, f.btp_ar);
            for (id, role) in [
                (wirksam.sr, OfficialRole::Sr),
                (wirksam.ar, OfficialRole::Ar),
            ] {
                let Some(id) = id else { continue };
                out.entry(id).or_default().push(Appearance {
                    match_id: f.match_id,
                    role,
                    court_id: f.court_id,
                    finished_at: f.finished_at,
                });
            }
        }
        for liste in out.values_mut() {
            // Ohne Endezeit (Altbestand) ans Ende, sonst chronologisch.
            // Achtung: `vorrollen` behandelt dieselben Spiele umgekehrt (als
            // ältestes, ADR 0079) — hier geht es um die Anzeige-Reihenfolge,
            // dort darum, dass ein ungestempeltes Spiel keine Vorrolle prägt.
            liste.sort_by_key(|a| (a.finished_at.unwrap_or(u64::MAX), a.match_id));
        }
        out
    }

    /// Eine lokale Absicht loslassen, sobald BTP sie zeigt — oder sobald
    /// jemand **in BTP** umgeteilt hat (R2, AK 12).
    ///
    /// Der Schreibweg bevorzugt die lokale Zuweisung — sonst ließe sich eine
    /// einmal geschriebene Besetzung nie wieder ändern. Bliebe sie danach
    /// aber liegen, überschriebe der nächste Sync-Zyklus **jede** spätere
    /// Änderung in BTP selbst, für den Rest des Turniers. Deshalb wird ein
    /// Dienst losgelassen, wenn
    ///
    /// - BTP genau den lokalen Wert trägt (bestätigt) — außer beim
    ///   ausdrücklichen „keiner", das stehen bleibt (ADR 0080), oder
    /// - BTP etwas **anderes** führt als beim Einteilen (Ausgangswert):
    ///   Dann hat jemand in BTP entschieden, und BTP gewinnt.
    ///
    /// Führt BTP noch den Ausgangswert, bleibt die Absicht stehen und geht
    /// nach BTP — sonst ließe sich ein Schiedsrichter, den BTP schon führt,
    /// nie umteilen (früher: losgelassen, sobald BTP irgendeinen Wert trug).
    /// Der Ausgangswert wird beim ersten Abgleich nach dem Einteilen
    /// erfasst; liegen Einteilung und Änderung in BTP im selben Sync-Takt,
    /// gilt die Einteilung.
    ///
    /// Die Einsatz-Ableitung verliert dadurch nichts: Sie liest denselben
    /// Wert dann aus dem BTP-Match.
    pub fn confirm(&self, match_id: i64, btp_sr: Option<i64>, btp_ar: Option<i64>) {
        {
            let mut inner = self.inner.lock().unwrap();
            let file = &mut inner.file;
            let Some(a) = file.assignments.get_mut(&match_id) else {
                return;
            };
            let vorher = (*a, file.ausgang.get(&match_id).copied());
            let merker_vorher = file.vorab_rotation.get(&match_id).copied();
            // Freigabe abschließen (ADR 0080): BTP hat die 0 übernommen ⇒ das
            // `Some(0)` verschwindet samt Merker — sonst bliebe es als „ohne"
            // stehen und sperrte die spätere Feld-Rotation.
            if let Some(v) = file.vorab_rotation.get_mut(&match_id) {
                for (role, btp) in [(OfficialRole::Sr, btp_sr), (OfficialRole::Ar, btp_ar)] {
                    let feld = match role {
                        OfficialRole::Sr => &mut a.sr,
                        OfficialRole::Ar => &mut a.ar,
                    };
                    if v.rolle(role).is_some_and(|d| d.frei)
                        && *feld == Some(0)
                        && dienst(btp).is_none()
                    {
                        *feld = None;
                        *v.rolle(role) = None;
                    }
                }
                if v.leer() {
                    file.vorab_rotation.remove(&match_id);
                }
            }
            let merker_geaendert = merker_vorher != file.vorab_rotation.get(&match_id).copied();
            let b = file.ausgang.entry(match_id).or_default();
            pruefe_dienst(&mut a.sr, &mut b.sr, &mut b.sr_vorher, btp_sr);
            pruefe_dienst(&mut a.ar, &mut b.ar, &mut b.ar_vorher, btp_ar);
            let a = *a;
            if a.is_empty() {
                file.assignments.remove(&match_id);
                file.ausgang.remove(&match_id);
            } else if *b == Ausgangswert::default() {
                file.ausgang.remove(&match_id);
            }
            let nachher = (
                file.assignments.get(&match_id).copied().unwrap_or_default(),
                file.ausgang.get(&match_id).copied(),
            );
            if vorher == nachher && !merker_geaendert {
                return; // der Normalfall jedes Sync-Takts — nicht schreiben
            }
        }
        self.persist();
    }

    /// Nach Spielende rücken SR und AR ans Ende (ADR 0079):
    /// - B/C: in der gemeinsamen Liste erst der AR, dann der SR — das Paar
    ///   kommt getauscht wieder hoch (früher `[SR, AR]`: Die Dienste blieben
    ///   an den Personen kleben).
    /// - A: der SR nur in der SR-Liste, der AR nur in der AR-Liste (die bei
    ///   Bedarf angelegt wird).
    pub fn nach_spielende(&self, sr: Option<i64>, ar: Option<i64>, modus: RotationMode) {
        if modus != RotationMode::Getrennt {
            let folge: Vec<i64> = ar.into_iter().chain(sr).collect();
            self.move_to_end(&folge);
            return;
        }
        {
            let mut inner = self.inner.lock().unwrap();
            let file = &mut inner.file;
            // AR-Liste VOR dem SR-Rücken ziehen: Gibt es sie noch nicht, ist
            // sie eine Kopie der Hauptliste — und zwar der unveränderten.
            let mut liste = ar_liste(file);
            let mut geaendert = false;
            if let Some(id) = sr {
                geaendert |= ans_ende(&mut file.order, &[id]);
            }
            if let Some(id) = ar {
                geaendert |= ans_ende(&mut liste, &[id]);
            }
            if !geaendert {
                return;
            }
            // Auch wenn nur der SR rückte: Ab jetzt ist die AR-Liste eigen —
            // sonst machte ihre Kopie das SR-Rücken mit.
            file.order_ar = Some(liste);
        }
        self.persist();
    }

    /// Officials ans Ende der Hauptliste rücken (Baustein von
    /// [`nach_spielende`](Self::nach_spielende)).
    pub fn move_to_end(&self, official_ids: &[i64]) {
        {
            let mut inner = self.inner.lock().unwrap();
            let betroffen: Vec<i64> = official_ids
                .iter()
                .copied()
                .filter(|id| inner.file.order.contains(id))
                .collect();
            if betroffen.is_empty() {
                return;
            }
            inner.file.order.retain(|id| !betroffen.contains(id));
            inner.file.order.extend(betroffen);
        }
        self.persist();
    }

    // ── Feldweise Schalter ────────────────────────────────────────────

    /// Schalter eines Felds (fehlt ein Eintrag: alles aktiv).
    pub fn court_switches(&self, court_id: i64) -> CourtSwitches {
        self.inner
            .lock()
            .unwrap()
            .file
            .courts
            .get(&court_id)
            .copied()
            .unwrap_or_default()
    }

    /// Schalter eines Felds setzen.
    pub fn set_court_switches(&self, court_id: i64, switches: CourtSwitches) {
        {
            let mut inner = self.inner.lock().unwrap();
            if switches == CourtSwitches::default() {
                // Default braucht keinen Eintrag — hält die Datei klein und
                // die Regel „ohne Eintrag alles aktiv" eindeutig.
                inner.file.courts.remove(&court_id);
            } else {
                inner.file.courts.insert(court_id, switches);
            }
        }
        self.persist();
    }

    // ── Persistenz ────────────────────────────────────────────────────

    fn load_file(&self) -> Ladung {
        let Some(path) = self.path.read().unwrap().clone() else {
            return Ladung::Leer; // Persistenz aus — sauberer Start im RAM
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ladung::Leer,
            Err(e) => {
                // Alles andere (Sperre, Rechte, defekter Datenträger) heißt:
                // Es gibt einen Stand, wir kommen nur gerade nicht dran.
                tracing::warn!("officials-state.json nicht lesbar ({e}) – Stand bleibt unberührt");
                return Ladung::Unlesbar;
            }
        };
        match serde_json::from_str::<OfficialsFile>(&text) {
            Ok(file) => Ladung::Stand(Box::new(file)),
            Err(_) => {
                // Kaputter Inhalt ist nicht zu retten — hier ist Überschreiben
                // richtig, anders als beim Lesefehler.
                tracing::warn!("officials-state.json unlesbar – beginne leer");
                Ladung::Leer
            }
        }
    }

    /// Schreiben (best effort, atomar). Fehler kosten höchstens die
    /// Einteilung, nie ein Ergebnis.
    fn persist(&self) {
        let Some(path) = self.path.read().unwrap().clone() else {
            return; // Persistenz aus (Tests, Slave-Betrieb)
        };
        // Schreib-Guard VOR dem Schnappschuss (Muster `TimelineStore`):
        // sonst könnten zwei Schreiber ihre Klone in vertauschter
        // Reihenfolge auf die Platte bringen.
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let inner = self.inner.lock().unwrap();
            if inner.file.tournament.is_empty() {
                return; // ohne Turnier-Kopf wäre der Stand nicht zuzuordnen
            }
            inner.file.clone()
        };
        if let Ok(json) = serde_json::to_string(&data) {
            // Erst Temp-Datei, dann Umbenennen — ein Absturz mitten im
            // Schreiben hinterlässt nie eine halbe Datei.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

/// Die AR-Liste in ihrer wirksamen Form: die angelegte Liste, ergänzt um
/// IDs der Hauptliste, die dort fehlen (etwa weil eine ältere App sie nicht
/// pflegte) — oder, solange es keine gibt, eine Kopie der Hauptliste.
fn ar_liste(file: &OfficialsFile) -> Vec<i64> {
    match &file.order_ar {
        None => file.order.clone(),
        Some(ar) => {
            let mut out = ar.clone();
            for id in &file.order {
                if !out.contains(id) {
                    out.push(*id);
                }
            }
            out
        }
    }
}

/// Rückt die bekannten unter `ids` in dieser Folge ans Ende. `false`, wenn
/// keine davon in der Liste steht.
fn ans_ende(order: &mut Vec<i64>, ids: &[i64]) -> bool {
    let betroffen: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|id| order.contains(id))
        .collect();
    if betroffen.is_empty() {
        return false;
    }
    order.retain(|id| !betroffen.contains(id));
    order.extend(betroffen);
    true
}

/// Verschiebt `id` vor `before` (`None` ⇒ ans Ende). `false`, wenn sich
/// nichts geändert hat (unbekannte ID, unbekanntes Ziel, vor sich selbst).
fn verschiebe(order: &mut Vec<i64>, id: i64, before: Option<i64>) -> bool {
    let Some(von) = order.iter().position(|x| *x == id) else {
        return false;
    };
    // Vor sich selbst ziehen ist keine Bewegung.
    if before == Some(id) {
        return false;
    }
    let ziel = match before {
        Some(b) => match order.iter().position(|x| *x == b) {
            Some(p) => p,
            None => return false, // unbekanntes Ziel ⇒ lieber nichts tun
        },
        None => order.len(),
    };
    let id = order.remove(von);
    // Nach dem Entfernen rutscht alles hinter `von` eine Stelle vor.
    let ziel = if ziel > von { ziel - 1 } else { ziel };
    order.insert(ziel, id);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::BtpPlayer;
    use std::path::Path;

    /// Ein Spieler mit Namen und Verein.
    fn spieler_mit(id: i64, name: &str, club: &str) -> BtpPlayer {
        BtpPlayer {
            id,
            name: name.to_string(),
            first: String::new(),
            last: name.to_string(),
            member_id: None,
            nationality: None,
            club: (!club.is_empty()).then(|| club.to_string()),
        }
    }

    /// Eine Meldung mit diesen Spielern.
    fn entry(players: Vec<BtpPlayer>) -> crate::btp::model::BtpEntry {
        crate::btp::model::BtpEntry {
            id: players.first().map(|p| p.id).unwrap_or(0),
            event_id: 1,
            players,
            seed: None,
        }
    }

    /// Ein Spieler mit Verein (leer = ohne Vereinszuordnung).
    fn spieler(id: i64, club: &str) -> BtpPlayer {
        BtpPlayer {
            id,
            name: format!("Spieler{id}"),
            first: String::new(),
            last: format!("Spieler{id}"),
            member_id: None,
            nationality: None,
            club: (!club.is_empty()).then(|| club.to_string()),
        }
    }

    /// Rotations-Eingabe für ein Feld mit diesen Spielern; nichts aus BTP
    /// gesetzt, niemand im Dienst, beide Rotationen an.
    fn eingabe<'a>(
        match_id: i64,
        players: &'a [BtpPlayer],
        bekannt: &'a [i64],
        belegt: &'a HashSet<i64>,
    ) -> RotationInput<'a> {
        RotationInput {
            match_id,
            players,
            btp_sr: None,
            btp_ar: None,
            bekannt,
            belegt,
            sr: true,
            ar: true,
            modus: RotationMode::Abwechselnd,
            vorrolle: Box::leak(Box::new(HashMap::new())),
            herkunft: Herkunft::Feld,
        }
    }

    #[test]
    fn die_auswahllisten_kommen_aus_der_meldeliste() {
        // Sperr-Spieler und Vereine sollen sich AUSWÄHLEN lassen — niemand
        // kennt die BTP-PlayerID von Anna Müller. Grundlage ist die
        // Meldeliste (vollständiger als die Paarungen: auch Klassen ohne
        // Auslosung sind dabei).
        let entries = vec![
            entry(vec![spieler_mit(7, "Anna Müller", "TSV Musterstadt")]),
            entry(vec![
                spieler_mit(8, "Ben Weber", "SC Nachbar"),
                spieler_mit(9, "Cara Fischer", "TSV Musterstadt"),
            ]),
            // Dieselbe Person in einer zweiten Klasse — nur einmal listen.
            entry(vec![spieler_mit(7, "Anna Müller", "TSV Musterstadt")]),
            // Ohne Verein: taucht als Spieler auf, erweitert die Vereinsliste
            // aber nicht.
            entry(vec![spieler_mit(10, "Dora Ohne", "")]),
        ];

        let (spieler, vereine) = pick_lists(&entries);
        assert_eq!(spieler.len(), 4, "jede Person genau einmal");
        // Alphabetisch, damit die Liste beim Tippen vorhersagbar ist.
        assert_eq!(spieler[0].name, "Anna Müller");
        assert_eq!(spieler[0].id, 7);
        assert_eq!(spieler[0].club, "TSV Musterstadt");
        assert_eq!(spieler[3].name, "Dora Ohne");
        assert_eq!(spieler[3].club, "");

        assert_eq!(
            vereine,
            vec!["SC Nachbar".to_string(), "TSV Musterstadt".to_string()],
            "Vereine einmalig und alphabetisch, Leereinträge draußen"
        );
    }

    #[test]
    fn konflikt_erkennt_eigenen_verein_sperrverein_und_sperrspieler() {
        let players = vec![spieler(1, "TSV Musterstadt"), spieler(2, "SC Nachbar")];

        // Kein Bezug ⇒ kein Konflikt.
        let neutral = OfficialExtra::default();
        assert_eq!(official_conflict(&neutral, &players), None);

        // (a) eigener Stammverein ist beteiligt ⇒ „Verein"
        let eigener = OfficialExtra {
            club: "TSV Musterstadt".into(),
            ..Default::default()
        };
        assert_eq!(
            official_conflict(&eigener, &players),
            Some(ConflictKind::Club)
        );
        // Handeingabe: Groß-/Kleinschreibung und Leerzeichen dürfen nicht
        // darüber entscheiden, ob gewarnt wird.
        let geschludert = OfficialExtra {
            club: "  tsv musterstadt ".into(),
            ..Default::default()
        };
        assert_eq!(
            official_conflict(&geschludert, &players),
            Some(ConflictKind::Club)
        );

        // (b) zusätzlich gesperrter Verein ⇒ „Verein"
        let sperrverein = OfficialExtra {
            blocked_clubs: vec!["SC Nachbar".into()],
            ..Default::default()
        };
        assert_eq!(
            official_conflict(&sperrverein, &players),
            Some(ConflictKind::Club)
        );

        // (c) gesperrter Spieler ⇒ „Person"
        let sperrspieler = OfficialExtra {
            blocked_players: vec![2],
            ..Default::default()
        };
        assert_eq!(
            official_conflict(&sperrspieler, &players),
            Some(ConflictKind::Person)
        );

        // Ein Spieler ohne Vereinszuordnung löst nie einen Vereins-Konflikt
        // aus — sonst warnte ein leeres Feld gegen ein leeres Feld.
        let ohne_verein = vec![spieler(3, "")];
        let leerer_verein = OfficialExtra {
            club: String::new(),
            ..Default::default()
        };
        assert_eq!(official_conflict(&leerer_verein, &ohne_verein), None);
        assert_eq!(official_conflict(&eigener, &ohne_verein), None);
    }

    #[test]
    fn rotation_nimmt_den_naechsten_freien_aus_der_reihenfolge() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        let players = vec![spieler(10, "TSV Musterstadt")];
        let bekannt = [1, 2, 3];
        let dienst = HashSet::new();

        store.rotate_court(eingabe(500, &players, &bekannt, &dienst));
        // SR = erster, AR = zweiter: dieselbe Person tut nie zwei Dienste.
        assert_eq!(
            store.assignment(500),
            MatchOfficials {
                sr: Some(1),
                ar: Some(2)
            }
        );

        // Idempotent: ein zweiter Lauf ändert nichts.
        store.rotate_court(eingabe(500, &players, &bekannt, &dienst));
        assert_eq!(store.assignment(500).sr, Some(1));
    }

    #[test]
    fn rotation_ueberspringt_pausierte_im_dienst_und_konflikt() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3, 4]);
        store.set_paused(1, true); // pausiert
        store.set_club(2, "TSV Musterstadt"); // Vereins-Konflikt
        let dienst: HashSet<i64> = [3].into_iter().collect(); // 3 pfeift woanders
        let players = vec![spieler(10, "TSV Musterstadt")];
        let bekannt = [1, 2, 3, 4];

        let mut e = eingabe(500, &players, &bekannt, &dienst);
        e.ar = false; // nur SR besetzen
        store.rotate_court(e);
        assert_eq!(
            store.assignment(500),
            MatchOfficials {
                sr: Some(4),
                ar: None
            },
            "1 pausiert, 2 Konflikt, 3 im Dienst ⇒ 4"
        );
    }

    #[test]
    fn rotation_laesst_das_feld_leer_wenn_niemand_frei_ist() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1]);
        store.set_paused(1, true);
        let players = vec![spieler(10, "")];
        let bekannt = [1];
        let dienst = HashSet::new();
        store.rotate_court(eingabe(500, &players, &bekannt, &dienst));
        assert_eq!(store.assignment(500), MatchOfficials::default());
    }

    #[test]
    fn rotation_weist_nur_officials_zu_die_btp_noch_kennt() {
        // Verschwindet ein Official aus der BTP-Liste, bleibt seine Position
        // stehen (inert) — zugewiesen wird er nicht mehr.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2]);
        let players = vec![spieler(10, "")];
        let bekannt = [2]; // 1 ist aus BTP verschwunden
        let dienst = HashSet::new();
        let mut e = eingabe(500, &players, &bekannt, &dienst);
        e.ar = false;
        store.rotate_court(e);
        assert_eq!(store.assignment(500).sr, Some(2));
        assert_eq!(store.order(), vec![1, 2], "Position bleibt");
    }

    #[test]
    fn btp_gewinnt_gegen_die_lokale_zuweisung() {
        // Spec-Konfliktregel: Trägt das BTP-Match Official1/2, gilt BTP —
        // die Rotation füllt dort nichts nach, und die Anzeige zeigt BTP.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.assign(500, OfficialRole::Sr, 3);
        let players = vec![spieler(10, "")];
        let bekannt = [1, 2, 3];
        let dienst = HashSet::new();

        let mut e = eingabe(500, &players, &bekannt, &dienst);
        e.btp_sr = Some(9);
        e.ar = false;
        store.rotate_court(e);
        // Lokal bleibt 3 stehen, wirksam ist aber 9 (BTP).
        assert_eq!(store.assignment(500).sr, Some(3));
        assert_eq!(
            store.effective(500, Some(9), None),
            MatchOfficials {
                sr: Some(9),
                ar: None
            }
        );
        // Ohne BTP-Wert gilt die lokale Zuweisung.
        assert_eq!(store.effective(500, None, None).sr, Some(3));
    }

    #[test]
    fn reihenfolge_laesst_sich_von_hand_umsortieren() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3, 4]);
        // 4 vor 2 ziehen.
        store.reorder(4, Some(2));
        assert_eq!(store.order(), vec![1, 4, 2, 3]);
        // Ohne Ziel ans Ende.
        store.reorder(1, None);
        assert_eq!(store.order(), vec![4, 2, 3, 1]);
        // An denselben Platz bzw. vor sich selbst: unverändert.
        store.reorder(4, Some(4));
        assert_eq!(store.order(), vec![4, 2, 3, 1]);
        // Unbekannte IDs ändern nichts.
        store.reorder(99, Some(2));
        store.reorder(2, Some(99));
        assert_eq!(store.order(), vec![4, 2, 3, 1]);
    }

    #[test]
    fn einsaetze_werden_aus_den_beendeten_spielen_abgeleitet() {
        // Spec Nr. 11: keine eigene Historie — Zähler und Liste entstehen
        // aus den beendeten Spielen (BTP-Wert ODER lokale Zuweisung).
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.assign(501, OfficialRole::Ar, 1);
        store.assign(502, OfficialRole::Sr, 1); // läuft noch

        let beendet = vec![
            FinishedMatch {
                match_id: 500,
                btp_sr: None,
                btp_ar: None,
                court_id: Some(5),
                finished_at: Some(1_000),
                walkover: false,
            },
            FinishedMatch {
                match_id: 501,
                btp_sr: None,
                btp_ar: None,
                court_id: Some(6),
                finished_at: Some(2_000),
                walkover: false,
            },
            // Ein Spiel, an dem BTP selbst einen Schiedsrichter trägt.
            FinishedMatch {
                match_id: 600,
                btp_sr: Some(2),
                btp_ar: None,
                court_id: Some(5),
                finished_at: Some(3_000),
                walkover: false,
            },
        ];
        let e = store.appearances(&beendet);
        let von_1 = e.get(&1).expect("Official 1 hat Einsätze");
        assert_eq!(von_1.len(), 2, "das laufende Spiel zählt nicht");
        assert_eq!(von_1[0].role, OfficialRole::Sr);
        assert_eq!(von_1[0].match_id, 500);
        assert_eq!(von_1[0].court_id, Some(5));
        assert_eq!(von_1[0].finished_at, Some(1_000));
        assert_eq!(von_1[1].role, OfficialRole::Ar);
        // Chronologisch, damit die Liste im Overlay lesbar ist.
        assert!(von_1[0].finished_at <= von_1[1].finished_at);
        // BTP-Wert zählt genauso.
        assert_eq!(e.get(&2).map(Vec::len), Some(1));
        assert!(!e.contains_key(&3));
    }

    #[test]
    fn eine_vor_spielbeginn_entfernte_zuweisung_ergibt_keinen_einsatz() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.clear_assignment(500, OfficialRole::Sr);
        let beendet = vec![FinishedMatch {
            match_id: 500,
            btp_sr: None,
            btp_ar: None,
            court_id: Some(5),
            finished_at: Some(1_000),
            walkover: false,
        }];
        assert!(store.appearances(&beendet).is_empty());
    }

    #[test]
    fn eine_von_btp_bestaetigte_zuweisung_wird_lokal_losgelassen() {
        // R2: Sobald BTP den geschriebenen Wert trägt, ist BTP wieder allein
        // die Wahrheit. Ohne dieses Loslassen würde der Schreibweg (der die
        // lokale Absicht bevorzugt) eine spätere Änderung IN BTP bei jedem
        // Sync-Zyklus wieder überschreiben — für den Rest des Turniers.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.assign(501, OfficialRole::Ar, 2);
        store.clear_assignment(502, OfficialRole::Sr); // ausdrücklich keiner

        // BTP meldet den SR von 500 zurück ⇒ lokale Absicht ist erfüllt.
        store.confirm(500, Some(1), None);
        assert_eq!(store.assignment(500), MatchOfficials::default());

        // Führte BTP schon jemanden (9), als hier umgeteilt wurde, ist das
        // kein Widerspruch, sondern genau die Absicht (AK 12): Die
        // Einteilung bleibt stehen und geht nach BTP.
        store.confirm(501, None, Some(9));
        assert_eq!(store.assignment(501).ar, Some(2));
        // Ändert danach jemand IN BTP auf einen dritten Wert ⇒ loslassen:
        // BTP gewinnt, und der nächste Zyklus schreibt nichts mehr dagegen.
        store.confirm(501, None, Some(11));
        assert_eq!(store.assignment(501), MatchOfficials::default());

        // Ausdrückliches „keiner" bleibt stehen, auch wenn BTP nichts trägt
        // (ADR 0080) — sonst füllte ein Lauf über wartende Spiele es wieder
        // auf.
        store.confirm(502, None, None);
        assert_eq!(store.assignment(502).sr, Some(0));
    }

    #[test]
    fn ohne_bleibt_stehen_solange_btp_niemanden_fuehrt() {
        // ADR 0080 (Spec sr-ar-beim-aufruf AK6): Über beliebig viele Läufe.
        let store = OfficialsStore::default();
        store.clear_assignment(700, OfficialRole::Sr);
        for _ in 0..5 {
            store.confirm(700, None, None);
            assert_eq!(store.assignment(700).sr, Some(0));
        }
        // Die wirksame Sicht kennt keinen Dienst, die Schreib-Sicht „keiner".
        assert_eq!(store.effective(700, None, None).sr, None);
        assert_eq!(store.fuer_schreiben(700, None, None).sr, Some(0));
    }

    #[test]
    fn ohne_weicht_einer_einteilung_in_btp() {
        let store = OfficialsStore::default();
        store.clear_assignment(701, OfficialRole::Ar);
        store.confirm(701, None, None);
        assert_eq!(store.assignment(701).ar, Some(0));
        // Jemand trägt in BTP eine Person ein ⇒ BTP gewinnt (R2).
        store.confirm(701, None, Some(5));
        assert_eq!(store.assignment(701), MatchOfficials::default());
        assert_eq!(store.effective(701, None, Some(5)).ar, Some(5));
    }

    #[test]
    fn ohne_gegen_einen_btp_wert_bleibt_nach_der_bestaetigung_stehen() {
        // BTP führt 9, die TL löst ⇒ die 0 geht nach BTP; danach bleibt das
        // „ohne" stehen, statt losgelassen zu werden.
        let store = OfficialsStore::default();
        store.confirm(702, Some(9), None);
        store.clear_assignment(702, OfficialRole::Sr);
        store.confirm(702, Some(9), None);
        assert_eq!(
            store.fuer_schreiben(702, Some(9), None).sr,
            Some(0),
            "0 geht nach BTP"
        );
        store.confirm(702, None, None);
        assert_eq!(store.assignment(702).sr, Some(0));
        store.confirm(702, None, None);
        assert_eq!(store.assignment(702).sr, Some(0));
    }

    #[test]
    fn umteilen_gegen_einen_btp_wert_bleibt_bis_zur_uebernahme_stehen() {
        // AK 12 (Spec tl-warteliste-schiedsrichter): BTP führt Y, die TL
        // teilt X ein. Früher ließ `confirm` X sofort los, weil BTP
        // „irgendeinen" Wert trug — X ging nie nach BTP.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        // Mehrere Zyklen, in denen BTP noch Y (7) zeigt: X bleibt.
        store.confirm(500, Some(7), None);
        store.confirm(500, Some(7), None);
        assert_eq!(store.assignment(500).sr, Some(1));
        // Der Ausgangswert überlebt einen Neustart (Datei neu lesen) — der
        // Sync-Merker `officials_written` täte das nicht.
        let wieder = store_mit_datei(dir.path());
        wieder.confirm(500, Some(7), None);
        assert_eq!(wieder.assignment(500).sr, Some(1));
        // BTP übernimmt X ⇒ loslassen.
        wieder.confirm(500, Some(1), None);
        assert_eq!(wieder.assignment(500), MatchOfficials::default());
    }

    #[test]
    fn ein_ausdrueckliches_keiner_weicht_einer_aenderung_in_btp() {
        // „Ohne" gegen BTP-Y: bleibt, bis BTP niemanden mehr trägt — es sei
        // denn, jemand teilt IN BTP auf Z um; dann gilt Z.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.clear_assignment(500, OfficialRole::Sr);
        store.confirm(500, Some(7), None);
        assert_eq!(store.assignment(500).sr, Some(0));
        store.confirm(500, Some(8), None);
        assert_eq!(store.assignment(500), MatchOfficials::default());
    }

    #[test]
    fn dieselbe_person_wie_in_btp_wird_sofort_losgelassen() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 7);
        store.confirm(500, Some(7), None);
        assert_eq!(store.assignment(500), MatchOfficials::default());
    }

    #[test]
    fn umteilen_waehrend_der_erste_write_unterwegs_ist_bleibt_stehen() {
        // Review-Befund: BTP Y (7), TL teilt X (1) ein, Write geht raus; noch
        // bevor BTP X zeigt, teilt die TL auf X2 (2) um. Landet danach der
        // erste Write (BTP zeigt X), ist das keine Umteilung in BTP.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.confirm(500, Some(7), None);
        store.assign(500, OfficialRole::Sr, 2);
        store.confirm(500, Some(7), None);
        store.confirm(500, Some(1), None);
        assert_eq!(store.assignment(500).sr, Some(2), "X2 geht noch nach BTP");
        // Eine echte Änderung in BTP gewinnt weiterhin.
        store.confirm(500, Some(9), None);
        assert_eq!(store.assignment(500), MatchOfficials::default());
    }

    #[test]
    fn eine_neue_einteilung_misst_gegen_den_dann_gueltigen_btp_wert() {
        // Wer nach einer BTP-Änderung erneut umteilt, meint den neuen Stand:
        // Der Ausgangswert gehört zur jeweiligen Einteilung.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.assign(500, OfficialRole::Ar, 2);
        store.confirm(500, Some(7), Some(8)); // Ausgangswerte 7 / 8
        store.confirm(500, Some(9), Some(8)); // SR in BTP umgeteilt ⇒ los
        assert_eq!(store.assignment(500).sr, None);
        assert_eq!(store.assignment(500).ar, Some(2));
        store.assign(500, OfficialRole::Sr, 3);
        store.confirm(500, Some(9), Some(8)); // neuer Ausgangswert 9
        assert_eq!(store.assignment(500).sr, Some(3));
    }

    #[test]
    fn eine_noch_nicht_uebernommene_zuweisung_bleibt_stehen() {
        // Solange BTP den Wert nicht zeigt, bleibt die Absicht bestehen —
        // sonst ginge sie zwischen Schreiben und Übernahme (≤ 1 s) verloren.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.confirm(500, None, None);
        assert_eq!(store.assignment(500).sr, Some(1));

        // Und ein ausdrückliches „keiner" bleibt, solange BTP noch jemanden
        // trägt.
        store.clear_assignment(501, OfficialRole::Sr);
        store.confirm(501, Some(3), None);
        assert_eq!(store.assignment(501).sr, Some(0));
    }

    #[test]
    fn die_rotation_respektiert_ein_ausdrueckliches_keiner() {
        // Wer den Schiedsrichter von Hand löst, will ohne spielen — auch
        // nach einem Neustart, bei dem jedes belegte Feld wie neu belegt
        // aussieht.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2]);
        store.clear_assignment(500, OfficialRole::Sr);
        let players = vec![spieler(10, "")];
        let bekannt = [1, 2];
        let dienst = HashSet::new();
        let mut e = eingabe(500, &players, &bekannt, &dienst);
        e.ar = false;
        store.rotate_court(e);
        assert_eq!(
            store.assignment(500).sr,
            Some(0),
            "die Rotation füllt ein ausdrückliches „keiner“ nicht auf"
        );
    }

    #[test]
    fn nach_spielende_rueckt_der_official_ans_ende_der_reihenfolge() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.move_to_end(&[1]);
        assert_eq!(store.order(), vec![2, 3, 1]);
        // Mehrere auf einmal (SR + AR desselben Spiels) behalten ihre
        // relative Reihenfolge.
        store.move_to_end(&[2, 3]);
        assert_eq!(store.order(), vec![1, 2, 3]);
        // Unbekannte IDs ändern nichts.
        store.move_to_end(&[99]);
        assert_eq!(store.order(), vec![1, 2, 3]);
    }

    // ── Rotationsmodus: Datenmodell (ADR 0079, PR 1) ──────────────────

    fn datei_text(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("officials-state.json")).unwrap_or_default()
    }

    #[test]
    fn rolle_erlaubt_nur_passende_dienste() {
        use Einsatzrolle::*;
        assert!(Beides.erlaubt(OfficialRole::Sr) && Beides.erlaubt(OfficialRole::Ar));
        assert!(NurSr.erlaubt(OfficialRole::Sr) && !NurSr.erlaubt(OfficialRole::Ar));
        assert!(NurAr.erlaubt(OfficialRole::Ar) && !NurAr.erlaubt(OfficialRole::Sr));
        assert_eq!(serde_json::to_string(&NurSr).unwrap(), "\"sr\"");
        assert_eq!(serde_json::to_string(&NurAr).unwrap(), "\"ar\"");
        assert_eq!(serde_json::to_string(&Beides).unwrap(), "\"beides\"");
    }

    #[test]
    fn rolle_ueberlebt_neustart_und_beides_hinterlaesst_keinen_eintrag() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_role(7, Einsatzrolle::NurAr);
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.extra(7).role, Einsatzrolle::NurAr);

        // Zurück auf „beides" ohne weitere Zusatzdaten ⇒ Eintrag weg.
        neu.set_role(7, Einsatzrolle::Beides);
        assert!(
            !datei_text(dir.path()).contains("\"7\""),
            "leere Zusatzdaten werden nicht aufbewahrt"
        );
        assert!(!datei_text(dir.path()).contains("\"role\""));
    }

    #[test]
    fn rollen_warnung_nennt_jeden_widerspruch() {
        let store = OfficialsStore::default();
        store.set_role(1, Einsatzrolle::NurAr);
        store.set_role(2, Einsatzrolle::NurSr);
        assert_eq!(
            store.rollen_warnung(Some(1), Some(2)),
            vec!["SR: nur AR", "AR: nur SR"]
        );
        assert!(
            store.rollen_warnung(Some(2), Some(1)).is_empty(),
            "passt beides"
        );
        assert!(
            store.rollen_warnung(Some(3), Some(0)).is_empty(),
            "beides / ohne"
        );
        assert!(store.rollen_warnung(None, None).is_empty());
    }

    #[test]
    fn rolle_allein_haelt_den_eintrag_am_leben() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(7, true);
        store.set_role(7, Einsatzrolle::NurSr);
        store.set_paused(7, false);
        assert_eq!(store.extra(7).role, Einsatzrolle::NurSr);
    }

    #[test]
    fn alte_datei_ohne_rolle_und_ar_liste_laedt_unveraendert() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("officials-state.json"),
            r#"{"tournament":"Test BTS Light","order":[3,1,2],
                "extras":{"1":{"paused":true,"blocked_clubs":["SC X"]}}}"#,
        )
        .unwrap();
        let store = store_mit_datei(dir.path());
        assert_eq!(store.order(), vec![3, 1, 2]);
        assert_eq!(
            store.order_ar(),
            vec![3, 1, 2],
            "ohne AR-Liste gilt die Kopie"
        );
        let e = store.extra(1);
        assert!(e.paused);
        assert_eq!(e.blocked_clubs, vec!["SC X".to_string()]);
        assert_eq!(e.role, Einsatzrolle::Beides);
    }

    #[test]
    fn ar_liste_lesen_legt_nichts_an() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        let vorher = datei_text(dir.path());
        assert_eq!(store.order_ar(), vec![1, 2, 3]);
        assert_eq!(datei_text(dir.path()), vorher);
        assert!(!vorher.contains("order_ar"));
    }

    #[test]
    fn ar_umsortieren_legt_kopie_an_und_laesst_hauptliste_stehen() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.reorder_in(Reihenfolge::Ar, 3, Some(1));
        assert_eq!(store.order_ar(), vec![3, 1, 2]);
        assert_eq!(store.order(), vec![1, 2, 3], "Hauptliste unberührt");
        assert!(datei_text(dir.path()).contains("order_ar"));

        // Umgekehrt: Hauptliste umsortieren lässt die AR-Liste stehen.
        store.reorder_in(Reihenfolge::Haupt, 1, None);
        assert_eq!(store.order(), vec![2, 3, 1]);
        assert_eq!(store.order_ar(), vec![3, 1, 2]);

        // Überlebt einen Neustart.
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.order_ar(), vec![3, 1, 2]);
        assert_eq!(neu.order(), vec![2, 3, 1]);
    }

    #[test]
    fn ar_umsortieren_mit_unbekannter_id_oder_ziel_aendert_nichts() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.reorder_in(Reihenfolge::Ar, 99, Some(1));
        store.reorder_in(Reihenfolge::Ar, 1, Some(99));
        assert_eq!(store.order_ar(), vec![1, 2, 3]);
        assert!(!datei_text(dir.path()).contains("order_ar"));
    }

    #[test]
    fn neue_officials_kommen_an_beide_listen_ohne_doppel() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2]);
        store.reorder_in(Reihenfolge::Ar, 2, Some(1));
        store.sync_roster(&[1, 2, 3, 3]);
        assert_eq!(store.order(), vec![1, 2, 3]);
        assert_eq!(store.order_ar(), vec![2, 1, 3]);
    }

    #[test]
    fn ar_liste_heilt_fehlende_ids_beim_lesen() {
        let dir = tempfile::tempdir().unwrap();
        // Eine ältere App hat `order` erweitert, `order_ar` aber nicht.
        std::fs::write(
            dir.path().join("officials-state.json"),
            r#"{"tournament":"Test BTS Light","order":[1,2,3],"order_ar":[2,1]}"#,
        )
        .unwrap();
        let store = store_mit_datei(dir.path());
        assert_eq!(store.order_ar(), vec![2, 1, 3]);
    }

    #[test]
    fn turnierwechsel_verwirft_die_ar_liste() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2]);
        store.reorder_in(Reihenfolge::Ar, 2, Some(1));
        store.set_tournament("Anderes Turnier");
        assert!(store.order_ar().is_empty());
        assert!(!datei_text(dir.path()).contains("order_ar"));
    }

    #[test]
    fn rotationsmodus_ist_ein_laufzeit_spiegel() {
        let store = OfficialsStore::default();
        assert_eq!(store.rotation_mode(), RotationMode::Abwechselnd);
        store.set_rotation_mode(RotationMode::Getrennt);
        assert_eq!(store.rotation_mode(), RotationMode::Getrennt);
    }

    // ── Rotationsmodus: Wahl-Kern (ADR 0079, PR 2) ────────────────────

    /// Wahl mit allen frei, Rollen und Vorrollen aus Tabellen.
    fn wahl(
        modus: RotationMode,
        order: &[i64],
        order_ar: &[i64],
        rollen: &[(i64, Einsatzrolle)],
        vor: &[(i64, OfficialRole)],
        sr_offen: bool,
        ar_offen: bool,
    ) -> (Option<i64>, Option<i64>) {
        wahl_mit(modus, order, order_ar, rollen, vor, &[], sr_offen, ar_offen)
    }

    #[allow(clippy::too_many_arguments)]
    fn wahl_mit(
        modus: RotationMode,
        order: &[i64],
        order_ar: &[i64],
        rollen: &[(i64, Einsatzrolle)],
        vor: &[(i64, OfficialRole)],
        nicht_frei: &[i64],
        sr_offen: bool,
        ar_offen: bool,
    ) -> (Option<i64>, Option<i64>) {
        let rollen: HashMap<i64, Einsatzrolle> = rollen.iter().copied().collect();
        let vorrolle: HashMap<i64, OfficialRole> = vor.iter().copied().collect();
        let frei = |id: i64| !nicht_frei.contains(&id);
        let rolle = |id: i64| rollen.get(&id).copied().unwrap_or_default();
        waehle_besetzung(&Wahleingabe {
            modus,
            order,
            order_ar,
            frei: &frei,
            rolle: &rolle,
            vorrolle: &vorrolle,
            sr_offen,
            ar_offen,
        })
    }

    use OfficialRole::{Ar as AR, Sr as SR};
    use RotationMode::{Abwechselnd as B, Getrennt as A, Rollenwechsel as C};

    #[test]
    fn a_waehlt_aus_getrennten_listen() {
        // AK7: SR aus der SR-Liste, AR aus der AR-Liste.
        assert_eq!(
            wahl(A, &[1, 2], &[3, 4], &[], &[], true, true),
            (Some(1), Some(3))
        );
        // „beides" oben in beiden Listen ⇒ SR; der AR kommt vom Nächsten der
        // AR-Liste.
        assert_eq!(
            wahl(A, &[1, 2], &[1, 3], &[], &[], true, true),
            (Some(1), Some(3))
        );
        // Rollen filtern je Liste.
        assert_eq!(
            wahl(
                A,
                &[1, 2],
                &[3, 4],
                &[(1, Einsatzrolle::NurAr), (3, Einsatzrolle::NurSr)],
                &[],
                true,
                true
            ),
            (Some(2), Some(4))
        );
    }

    #[test]
    fn b_erster_sr_naechster_ar_mit_rollenfilter() {
        assert_eq!(
            wahl(B, &[1, 2, 3], &[9], &[], &[], true, true),
            (Some(1), Some(2))
        );
        // AK11: „nur AR" oben wird nie SR, bekommt aber den AR.
        assert_eq!(
            wahl(
                B,
                &[1, 2, 3],
                &[],
                &[(1, Einsatzrolle::NurAr)],
                &[],
                true,
                true
            ),
            (Some(2), Some(1))
        );
        // Die Vorrolle spielt in B keine Rolle.
        assert_eq!(
            wahl(B, &[1, 2], &[], &[], &[(1, SR)], true, true),
            (Some(1), Some(2))
        );
    }

    #[test]
    fn c_wechselt_je_person() {
        // AK12: P1 zuletzt SR, P2 zuletzt AR ⇒ getauscht.
        assert_eq!(
            wahl(C, &[1, 2], &[], &[], &[(1, SR), (2, AR)], true, true),
            (Some(2), Some(1))
        );
        // AK13: P2 will auch AR, das hat schon P1 ⇒ P2 übersprungen, P3 SR.
        assert_eq!(
            wahl(
                C,
                &[1, 2, 3],
                &[],
                &[],
                &[(1, SR), (2, SR), (3, AR)],
                true,
                true
            ),
            (Some(3), Some(1))
        );
    }

    #[test]
    fn c_ohne_vorgeschichte_sr_vor_ar() {
        // AK14
        assert_eq!(
            wahl(C, &[1, 2], &[], &[], &[], true, true),
            (Some(1), Some(2))
        );
        assert_eq!(
            wahl(C, &[1, 2], &[], &[], &[], false, true),
            (None, Some(1))
        );
    }

    #[test]
    fn c_faellt_zurueck_wenn_niemand_wechseln_kann() {
        // AK15: nur SR offen, alle zuletzt SR ⇒ erste freie SR-fähige Person.
        assert_eq!(
            wahl(C, &[1, 2], &[], &[], &[(1, SR), (2, SR)], true, false),
            (Some(1), None)
        );
        // Rückfall beachtet die Rolle.
        assert_eq!(
            wahl(
                C,
                &[1, 2],
                &[],
                &[(1, Einsatzrolle::NurAr)],
                &[(1, AR), (2, SR)],
                true,
                false
            ),
            (Some(2), None)
        );
    }

    #[test]
    fn c_feste_rolle_schlaegt_die_vorrolle() {
        // AK16
        assert_eq!(
            wahl(
                C,
                &[1, 2],
                &[],
                &[(1, Einsatzrolle::NurSr)],
                &[(1, SR), (2, SR)],
                true,
                true
            ),
            (Some(1), Some(2))
        );
    }

    #[test]
    fn niemand_passendes_frei_laesst_den_dienst_leer() {
        // AK20, in allen Modi.
        for m in [A, B, C] {
            assert_eq!(
                wahl(m, &[1], &[1], &[(1, Einsatzrolle::NurAr)], &[], true, false),
                (None, None),
                "{m:?}"
            );
            assert_eq!(
                wahl_mit(m, &[1, 2], &[1, 2], &[], &[], &[1, 2], true, true),
                (None, None),
                "{m:?}: niemand frei"
            );
        }
        // Belegte werden übersprungen.
        assert_eq!(
            wahl_mit(C, &[1, 2, 3], &[], &[], &[], &[1], true, true),
            (Some(2), Some(3))
        );
    }

    fn fertig(match_id: i64, sr: i64, ar: i64, ende: Option<u64>, walkover: bool) -> FinishedMatch {
        FinishedMatch {
            match_id,
            btp_sr: Some(sr),
            btp_ar: Some(ar),
            court_id: None,
            finished_at: ende,
            walkover,
        }
    }

    #[test]
    fn vorrolle_kommt_aus_dem_zuletzt_beendeten_spiel() {
        let store = OfficialsStore::default();
        let v = store.vorrollen(&[
            fertig(1, 10, 20, Some(100), false),
            fertig(2, 20, 10, Some(200), false),
        ]);
        assert_eq!(v.get(&10), Some(&AR));
        assert_eq!(v.get(&20), Some(&SR));
    }

    #[test]
    fn vorrolle_ignoriert_walkover() {
        // AK17
        let store = OfficialsStore::default();
        let v = store.vorrollen(&[
            fertig(1, 10, 20, Some(100), false),
            fertig(2, 20, 10, Some(200), true),
        ]);
        assert_eq!(v.get(&10), Some(&SR));
        assert_eq!(v.get(&20), Some(&AR));
    }

    #[test]
    fn vorrolle_gleichstand_hoehere_match_id_ohne_endezeit_aelter() {
        // AK18
        let store = OfficialsStore::default();
        let v = store.vorrollen(&[
            fertig(7, 10, 20, Some(100), false),
            fertig(5, 20, 10, Some(100), false),
        ]);
        assert_eq!(
            v.get(&10),
            Some(&SR),
            "Match 7 ist bei gleicher Zeit das spätere"
        );
        let v = store.vorrollen(&[
            fertig(9, 10, 20, None, false),
            fertig(3, 20, 10, Some(1), false),
        ]);
        assert_eq!(
            v.get(&10),
            Some(&AR),
            "ohne Endezeit älter als jedes gestempelte"
        );
    }

    #[test]
    fn vorrolle_liest_auch_lokale_zuweisungen() {
        let store = OfficialsStore::default();
        store.assign(1, OfficialRole::Sr, 10);
        let f = FinishedMatch {
            btp_sr: None,
            btp_ar: None,
            ..fertig(1, 0, 0, Some(5), false)
        };
        assert_eq!(store.vorrollen(&[f]).get(&10), Some(&SR));
    }

    #[test]
    fn nach_spielende_b_und_c_ruecken_ar_vor_sr_ans_ende() {
        // AK10
        for m in [B, C] {
            let dir = tempfile::tempdir().unwrap();
            let store = store_mit_datei(dir.path());
            store.sync_roster(&[1, 2, 3]);
            store.nach_spielende(Some(1), Some(2), m);
            assert_eq!(store.order(), vec![3, 2, 1], "{m:?}");
            assert!(!datei_text(dir.path()).contains("order_ar"));
        }
    }

    #[test]
    fn nach_spielende_a_rueckt_je_liste() {
        // AK8
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.nach_spielende(Some(1), Some(2), A);
        assert_eq!(store.order(), vec![2, 3, 1], "SR rückt nur in der SR-Liste");
        assert_eq!(
            store.order_ar(),
            vec![1, 3, 2],
            "AR rückt nur in der AR-Liste"
        );
        // Nur einer besetzt.
        store.nach_spielende(None, Some(3), A);
        assert_eq!(store.order(), vec![2, 3, 1]);
        assert_eq!(store.order_ar(), vec![1, 2, 3]);
    }

    #[test]
    fn nach_spielende_a_nur_sr_laesst_die_ar_reihenfolge_stehen() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.nach_spielende(Some(1), None, A);
        assert_eq!(store.order(), vec![2, 3, 1]);
        assert_eq!(
            store.order_ar(),
            vec![1, 2, 3],
            "AR-Liste macht das SR-Rücken nicht mit"
        );
    }

    #[test]
    fn rotation_folgt_dem_modus_und_der_rolle() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2, 3]);
        store.set_role(1, Einsatzrolle::NurAr);
        let players = vec![spieler(10, "")];
        let bekannt = [1, 2, 3];
        let dienst = HashSet::new();
        store.rotate_court(eingabe(500, &players, &bekannt, &dienst));
        assert_eq!(store.assignment(500).sr, Some(2), "„nur AR\" wird nicht SR");
        assert_eq!(store.assignment(500).ar, Some(1));

        // Modus C mit Vorrolle.
        let vor: HashMap<i64, OfficialRole> = [(2, SR), (3, AR)].into_iter().collect();
        let mut e = eingabe(501, &players, &bekannt, &dienst);
        e.modus = C;
        e.vorrolle = &vor;
        store.set_role(1, Einsatzrolle::Beides);
        let belegt: HashSet<i64> = [1].into_iter().collect();
        e.belegt = &belegt;
        store.rotate_court(e);
        assert_eq!(store.assignment(501).sr, Some(3));
        assert_eq!(store.assignment(501).ar, Some(2));
    }

    // ── Zuteilung beim Aufruf: Merker und Freigabe (ADR 0080, PR 2) ───

    fn aufruf_eingabe<'a>(
        match_id: i64,
        players: &'a [BtpPlayer],
        bekannt: &'a [i64],
        belegt: &'a HashSet<i64>,
    ) -> RotationInput<'a> {
        let mut e = eingabe(match_id, players, bekannt, belegt);
        e.herkunft = Herkunft::Aufruf;
        e.ar = false;
        e
    }

    #[test]
    fn merker_ueberlebt_bestaetigung_und_neustart() {
        // AK11
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[1, 2]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        assert_eq!(
            store.rotate_court(aufruf_eingabe(500, &players, &[1, 2], &dienst)),
            (Some(1), None)
        );
        store.confirm(500, Some(1), None);
        assert_eq!(store.assignment(500).sr, None, "lokal losgelassen");
        assert_eq!(store.vorab_rotation(500), (Some(1), None), "Merker bleibt");
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.vorab_rotation(500), (Some(1), None));
    }

    #[test]
    fn feld_rotation_setzt_keinen_merker() {
        let store = OfficialsStore::default();
        store.sync_roster(&[1]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        store.rotate_court(eingabe(500, &players, &[1], &dienst));
        assert!(!store.hat_vorab_rotation(500));
    }

    #[test]
    fn umteilung_in_btp_loescht_den_merker() {
        // AK13
        let store = OfficialsStore::default();
        store.sync_roster(&[1, 2]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        store.rotate_court(aufruf_eingabe(500, &players, &[1, 2], &dienst));
        store.confirm(500, Some(1), None);
        let mut m = crate::tablet::state::tests::match_on(500, None, MatchStatus::Scheduled);
        m.official1_id = Some(2);
        let snap = crate::tablet::state::tests::snapshot(vec![m], vec![]);
        store.pflege_vorab_rotation(&snap);
        assert!(!store.hat_vorab_rotation(500));
        assert!(
            !store.rotation_freigeben(500, OfficialRole::Sr, Some(2)),
            "nichts frei"
        );
    }

    #[test]
    fn freigabe_ohne_btp_gegenstueck_hinterlaesst_kein_ohne() {
        let store = OfficialsStore::default();
        store.sync_roster(&[1]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        store.rotate_court(aufruf_eingabe(500, &players, &[1], &dienst));
        assert!(store.rotation_freigeben(500, OfficialRole::Sr, None));
        assert_eq!(store.assignment(500), MatchOfficials::default());
        assert!(!store.hat_vorab_rotation(500));
    }

    #[test]
    fn freigabe_mit_btp_gegenstueck_laeuft_ueber_ein_freies_null() {
        // AK14 + AK15: BTP trägt die Person schon ⇒ 0 geht nach BTP, der
        // Dienst gilt sofort als offen, nach der Bestätigung bleibt nichts.
        let store = OfficialsStore::default();
        store.sync_roster(&[1, 2]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        store.rotate_court(aufruf_eingabe(500, &players, &[1, 2], &dienst));
        store.confirm(500, Some(1), None);
        assert!(store.rotation_freigeben(500, OfficialRole::Sr, Some(1)));
        assert_eq!(
            store.fuer_schreiben(500, Some(1), None).sr,
            Some(0),
            "0 geht nach BTP"
        );
        // Der Dienst ist offen: Die Feld-Rotation besetzt ihn, obwohl BTP noch
        // die alte Person zeigt.
        let mut e = eingabe(500, &players, &[1, 2], &dienst);
        e.btp_sr = Some(1);
        e.ar = false;
        let belegt: HashSet<i64> = [1].into_iter().collect();
        e.belegt = &belegt;
        assert_eq!(store.rotate_court(e), (Some(2), None));
        assert!(!store.hat_vorab_rotation(500));
    }

    #[test]
    fn freigabe_wird_nach_der_bestaetigung_der_null_geraeumt() {
        let store = OfficialsStore::default();
        store.sync_roster(&[1]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        store.rotate_court(aufruf_eingabe(500, &players, &[1], &dienst));
        store.confirm(500, Some(1), None);
        store.rotation_freigeben(500, OfficialRole::Sr, Some(1));
        store.confirm(500, Some(1), None); // 0 noch unterwegs
        assert_eq!(store.assignment(500).sr, Some(0));
        store.confirm(500, None, None); // BTP hat die 0
        assert_eq!(
            store.assignment(500),
            MatchOfficials::default(),
            "kein „ohne\""
        );
        assert!(!store.hat_vorab_rotation(500));
    }

    #[test]
    fn rotation_aus_raeumt_auch_die_merker() {
        // AK19
        let store = OfficialsStore::default();
        store.sync_roster(&[1]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        store.rotate_court(aufruf_eingabe(500, &players, &[1], &dienst));
        store.clear_assignments();
        assert!(!store.hat_vorab_rotation(500));
    }

    #[test]
    fn loesen_am_feld_nur_fuer_rollen_ohne_schalter() {
        let store = OfficialsStore::default();
        store.set_enabled(true);
        store.set_rotation(true, true);
        store.sync_roster(&[1, 2]);
        let players = vec![spieler(10, "")];
        let dienst = HashSet::new();
        let mut e = aufruf_eingabe(500, &players, &[1, 2], &dienst);
        e.ar = true;
        store.rotate_court(e);
        let mut sw = store.court_switches(7);
        sw.ar = false;
        store.set_court_switches(7, sw);
        let m = crate::tablet::state::tests::match_on(500, Some(7), MatchStatus::OnCourt);
        store.rotation_loesen_am_feld(&m, 7);
        assert_eq!(store.assignment(500).sr, Some(1), "SR bleibt");
        assert_eq!(store.assignment(500).ar, None, "Rotations-AR gelöst");
    }

    fn store_mit_datei(dir: &Path) -> OfficialsStore {
        let store = OfficialsStore::default();
        store.set_path(dir.join("officials-state.json"));
        store.set_tournament("Test BTS Light");
        store
    }

    #[test]
    fn zusatzdaten_werden_je_official_gehalten() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(7, true);
        store.set_club(7, "TSV Musterstadt");
        store.set_blocklists(7, vec!["SC Nachbar".into()], vec![42, 43]);

        let e = store.extra(7);
        assert!(e.paused);
        assert_eq!(e.club, "TSV Musterstadt");
        assert_eq!(e.blocked_clubs, vec!["SC Nachbar".to_string()]);
        assert_eq!(e.blocked_players, vec![42, 43]);
        // Ein unbekannter Official hat schlicht leere Zusatzdaten.
        assert_eq!(store.extra(999), OfficialExtra::default());
        // Pause wieder aufheben.
        store.set_paused(7, false);
        assert!(!store.extra(7).paused);
    }

    #[test]
    fn stand_ueberlebt_app_neustart() {
        // Spec Nr. 10: Reihenfolge, Pausen, Sperrlisten, Schalter und
        // Zuweisungen überleben Neustart/Absturz.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_order(vec![3, 1, 2]);
        store.set_paused(1, true);
        store.set_blocklists(2, vec!["SC Nachbar".into()], vec![42]);
        store.assign(500, OfficialRole::Sr, 3);
        store.assign(500, OfficialRole::Ar, 1);
        store.set_court_switches(
            9,
            CourtSwitches {
                sr: true,
                ar: false,
                operator: false,
            },
        );

        // „Neustart": frischer Speicher, gleiche Datei, gleiches Turnier.
        let neu = store_mit_datei(dir.path());
        assert_eq!(neu.order(), vec![3, 1, 2]);
        assert!(neu.extra(1).paused);
        assert_eq!(neu.extra(2).blocked_players, vec![42]);
        assert_eq!(
            neu.assignment(500),
            MatchOfficials {
                sr: Some(3),
                ar: Some(1)
            }
        );
        assert_eq!(
            neu.court_switches(9),
            CourtSwitches {
                sr: true,
                ar: false,
                operator: false
            }
        );
    }

    #[test]
    fn turnierwechsel_verwirft_den_stand() {
        // ADR 0022: BTP-IDs gelten nur im Turnier, Sperrlisten sind
        // Personendaten — beim Wechsel wird verworfen, nicht mitgeschleppt.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(1, true);
        store.set_blocklists(1, vec!["SC Nachbar".into()], vec![42]);
        store.assign(500, OfficialRole::Sr, 1);
        store.set_order(vec![1, 2]);

        store.set_tournament("Ganz anderes Turnier");
        assert!(!store.extra(1).paused);
        assert!(store.extra(1).blocked_players.is_empty());
        assert_eq!(store.assignment(500), MatchOfficials::default());
        assert!(store.order().is_empty());
        assert_eq!(store.tournament(), "Ganz anderes Turnier");

        // Und der verworfene Stand ist auch auf der Platte weg — ein
        // Neustart darf ihn nicht zurückholen.
        let neu = OfficialsStore::default();
        neu.set_path(dir.path().join("officials-state.json"));
        neu.set_tournament("Test BTS Light");
        assert!(!neu.extra(1).paused);
        assert!(neu.order().is_empty());
    }

    #[test]
    fn fremder_dateistand_wird_beim_start_verworfen() {
        // Datei aus einem früheren Turnier: Der erste Snapshot des neuen
        // Turniers wirft sie weg, statt fremde IDs zu übernehmen.
        let dir = tempfile::tempdir().unwrap();
        let alt = store_mit_datei(dir.path());
        alt.set_blocklists(1, vec!["SC Nachbar".into()], vec![42]);

        let neu = OfficialsStore::default();
        neu.set_path(dir.path().join("officials-state.json"));
        neu.set_tournament("Nächstes Wochenende");
        assert!(neu.extra(1).blocked_clubs.is_empty());
        assert_eq!(neu.tournament(), "Nächstes Wochenende");
    }

    #[test]
    fn reihenfolge_ergaenzt_neue_und_haelt_verschwundene_inert() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[5, 3]);
        assert_eq!(store.order(), vec![5, 3]);
        // Neuer Official aus BTP kommt hinten dazu, Reihenfolge bleibt.
        store.sync_roster(&[5, 3, 8]);
        assert_eq!(store.order(), vec![5, 3, 8]);
        // Manuell vorgezogen — der nächste Abgleich rührt das nicht an.
        store.set_order(vec![8, 5, 3]);
        store.sync_roster(&[5, 3, 8]);
        assert_eq!(store.order(), vec![8, 5, 3]);
        // Verschwindet einer aus BTP, bleibt seine Position stehen: kehrt er
        // zurück, gilt sie wieder (Spec „Fehlerfälle").
        store.set_paused(5, true);
        store.sync_roster(&[8, 3]);
        assert_eq!(store.order(), vec![8, 5, 3]);
        assert!(store.extra(5).paused);
    }

    #[test]
    fn zuweisungen_bleiben_dem_match_erhalten_und_sind_einzeln_loesbar() {
        // Spec Nr. 11: Zuweisungen hängen am Match und bleiben nach
        // Spielende liegen — sie sind die Grundlage der Einsatz-Ableitung.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.assign(500, OfficialRole::Sr, 1);
        store.assign(500, OfficialRole::Ar, 2);
        store.assign(501, OfficialRole::Sr, 3);
        assert_eq!(store.assignments().len(), 2);

        // Gelöst heißt **ausdrücklich keiner** (`Some(0)`), nicht „nie
        // angefasst" — nur so schreibt der Rücksync die 0 nach BTP.
        store.clear_assignment(500, OfficialRole::Ar);
        assert_eq!(
            store.assignment(500),
            MatchOfficials {
                sr: Some(1),
                ar: Some(0)
            }
        );
        // Wirksam ist der Dienst damit weg.
        assert_eq!(store.effective(500, None, None).ar, None);
        store.clear_assignment(500, OfficialRole::Sr);
        assert_eq!(store.effective(500, None, None), MatchOfficials::default());
        assert_eq!(store.assignments().len(), 2, "die Absicht bleibt gemerkt");

        // Globales Abschalten räumt alles (Spec Nr. 1).
        store.clear_assignments();
        assert!(store.assignments().is_empty());
    }

    #[test]
    fn feldschalter_sind_standardmaessig_alle_aktiv() {
        // Bestandsverhalten der Zähltafelbediener-Vergabe bleibt unverändert:
        // ohne Eintrag ist jedes Feld aktiv.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert_eq!(store.court_switches(1), CourtSwitches::default());
        assert!(store.court_switches(1).operator);

        store.set_court_switches(
            1,
            CourtSwitches {
                sr: false,
                ar: false,
                operator: false,
            },
        );
        assert!(!store.court_switches(1).operator);
        // Andere Felder bleibt davon unberührt.
        assert!(store.court_switches(2).operator);
    }

    #[test]
    fn eine_voruebergehend_unlesbare_datei_wird_nicht_ueberschrieben() {
        // Windows-Realität: Virenscanner oder ein hängendes Handle sperren
        // die Datei für einen Moment. Ein „nicht lesbar" ist NICHT dasselbe
        // wie „gibt es nicht" — würde der Speicher hier leer starten und
        // schreiben, wäre die komplette Einteilung des Turniers still weg.
        // Ersatz für die Sperre im Test: ein Verzeichnis unter dem
        // Dateinamen — lesbar ist das ebenso wenig, und es ist auch kein
        // `NotFound`.
        let dir = tempfile::tempdir().unwrap();
        let pfad = dir.path().join("officials-state.json");
        std::fs::create_dir(&pfad).unwrap();

        let store = OfficialsStore::default();
        store.set_path(pfad.clone());
        store.set_tournament("Cup A");
        // Kein Turnier gebunden ⇒ es wird nichts geschrieben …
        assert_eq!(store.tournament(), "");
        assert!(pfad.is_dir(), "der vorhandene Stand bleibt unangetastet");
        assert!(
            !pfad.with_extension("json.tmp").exists(),
            "auch keine halbe Schreiboperation"
        );
        // … der Betrieb läuft trotzdem weiter (im RAM).
        store.set_paused(1, true);
        assert!(store.extra(1).paused);

        // Sobald die Datei wieder lesbar ist, holt der nächste Snapshot den
        // Stand nach — ohne App-Neustart.
        std::fs::remove_dir(&pfad).unwrap();
        let vorlage = store_mit_datei(dir.path());
        vorlage.set_order(vec![4, 2]);
        store.set_tournament("Test BTS Light");
        assert_eq!(store.order(), vec![4, 2]);
        assert_eq!(store.tournament(), "Test BTS Light");
    }

    #[test]
    fn ein_doppelt_gelisteter_official_bekommt_nur_einen_platz() {
        // Ein doppelter Eintrag im BTP-Container (oder ein künftiger
        // Aufrufer, der zwei Hallen-Listen aneinanderhängt) darf niemandem
        // zwei Plätze in der Rotation geben — er käme doppelt so oft dran.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.sync_roster(&[5, 3, 5]);
        assert_eq!(store.order(), vec![5, 3]);
    }

    #[test]
    fn ein_spaeter_gesetzter_pfad_laedt_den_stand_noch() {
        // Der Speicher darf nicht davon abhängen, dass `set_path` vor dem
        // ersten Snapshot läuft: Wer die Reihenfolge umdreht, bekäme sonst
        // einen leeren Stand, der den vorhandenen beim ersten Klick
        // überschreibt.
        let dir = tempfile::tempdir().unwrap();
        let alt = store_mit_datei(dir.path());
        alt.set_order(vec![7, 1]);

        let store = OfficialsStore::default();
        store.set_tournament("Test BTS Light"); // Snapshot zuerst …
        store.set_path(dir.path().join("officials-state.json")); // … Pfad danach
        store.set_tournament("Test BTS Light");
        assert_eq!(store.order(), vec![7, 1]);
    }

    #[test]
    fn ohne_pfad_bleibt_alles_im_ram() {
        // Slave-Betrieb/Tests: kein Pfad gesetzt ⇒ keine Datei, aber der
        // Speicher funktioniert.
        let store = OfficialsStore::default();
        store.set_tournament("Ohne Ablage");
        store.set_paused(1, true);
        assert!(store.extra(1).paused);
    }

    #[test]
    fn leerer_turniername_aendert_nichts() {
        // Ein Snapshot ohne Turniernamen (Startphase) darf den Stand nicht
        // wegwerfen.
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.set_paused(1, true);
        store.set_tournament("");
        store.set_tournament("   ");
        assert!(store.extra(1).paused);
        assert_eq!(store.tournament(), "Test BTS Light");
    }

    // ── Reservierung wartender Spiele (ADR 0078) ──────────────────────

    use crate::btp::model::{BtpOfficial, MatchResult};

    /// Der Turniertag der Tests.
    fn heute() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()
    }

    /// Ein Spiel mit Status, Feld und BTP-Besetzung.
    fn spiel(id: i64, status: MatchStatus, sr: Option<i64>, ar: Option<i64>) -> BtpMatch {
        let court = (status == MatchStatus::OnCourt).then_some(id + 100);
        let mut m = crate::tablet::state::tests::match_on(id, court, status);
        m.official1_id = sr;
        m.official2_id = ar;
        m
    }

    /// Schnappschuss mit Spielen und der BTP-Officials-Liste.
    fn snap_mit(matches: Vec<BtpMatch>, officials: &[i64]) -> BtpSnapshot {
        let mut s = crate::tablet::state::tests::snapshot(matches, vec![]);
        s.officials = officials
            .iter()
            .map(|id| BtpOfficial {
                id: *id,
                name: format!("SR{id}"),
                first: String::new(),
                nationality: None,
            })
            .collect();
        s
    }

    fn store_an() -> OfficialsStore {
        let store = OfficialsStore::default();
        store.set_enabled(true);
        store
    }

    #[test]
    fn fuer_schreiben_nimmt_die_hand_vor_btp_und_kennt_das_ohne() {
        let store = store_an();
        // Nie angefasst: BTP.
        assert_eq!(
            store.fuer_schreiben(1, Some(7), None),
            MatchOfficials {
                sr: Some(7),
                ar: None
            }
        );
        // Umgeteilt: die Hand gewinnt, auch gegen einen BTP-Wert.
        store.assign(1, OfficialRole::Sr, 8);
        assert_eq!(store.fuer_schreiben(1, Some(7), None).sr, Some(8));
        // Ausdrücklich ohne: `Some(0)`, nicht der BTP-Wert.
        store.clear_assignment(1, OfficialRole::Sr);
        assert_eq!(store.fuer_schreiben(1, Some(7), None).sr, Some(0));
        // Die Anzeige bleibt bei BTP.
        assert_eq!(store.effective(1, Some(7), None).sr, Some(7));
    }

    #[test]
    fn belegung_reserviert_btp_vorab_und_hand_einteilung() {
        let store = store_an();
        store.assign(2, OfficialRole::Ar, 5);
        let snap = snap_mit(
            vec![
                spiel(1, MatchStatus::Scheduled, Some(4), None),
                spiel(2, MatchStatus::Scheduled, None, None),
            ],
            &[4, 5, 6],
        );
        let b = Belegung::aus(&store, &snap, heute());
        assert_eq!(b.reserviert_fuer(4), &[1]);
        assert_eq!(b.reserviert_fuer(5), &[2]);
        assert!(b.reserviert_fuer(6).is_empty());
        assert_eq!(b.im_dienst_bei(4), None);
    }

    #[test]
    fn belegung_reserviert_heute_davor_und_ohne_zeit_aber_nicht_spaeter() {
        let store = store_an();
        let mut heute_m = spiel(1, MatchStatus::Scheduled, Some(1), None);
        heute_m.planned_time = Some(202_611_081_830);
        let mut gestern = spiel(2, MatchStatus::Scheduled, Some(2), None);
        gestern.planned_time = Some(202_611_071_000);
        let ohne_zeit = spiel(3, MatchStatus::Scheduled, Some(3), None);
        let mut morgen = spiel(4, MatchStatus::Scheduled, Some(4), None);
        morgen.planned_time = Some(202_611_090_900);
        let snap = snap_mit(vec![heute_m, gestern, ohne_zeit, morgen], &[1, 2, 3, 4]);
        let b = Belegung::aus(&store, &snap, heute());
        assert_eq!(b.reserviert_fuer(1), &[1]);
        assert_eq!(b.reserviert_fuer(2), &[2], "liegengeblieben vom Vortag");
        assert_eq!(b.reserviert_fuer(3), &[3], "ohne Ansetzung");
        assert!(
            b.reserviert_fuer(4).is_empty(),
            "späterer Tag reserviert nicht"
        );
        assert!(!b.ist_belegt(4));
    }

    #[test]
    fn belegung_zaehlt_nur_wen_btp_kennt() {
        let store = store_an();
        let snap = snap_mit(
            vec![
                spiel(1, MatchStatus::Scheduled, Some(9), None),
                spiel(2, MatchStatus::OnCourt, Some(8), None),
            ],
            &[1],
        );
        let b = Belegung::aus(&store, &snap, heute());
        assert!(!b.ist_belegt(9));
        assert!(!b.ist_belegt(8));
    }

    #[test]
    fn belegung_endet_mit_spielende_auch_beim_walkover() {
        let store = store_an();
        store.assign(1, OfficialRole::Sr, 3);
        let mut fertig = spiel(1, MatchStatus::Finished, None, None);
        fertig.winner = Some(1);
        fertig.result = MatchResult::Walkover;
        let snap = snap_mit(vec![fertig], &[3]);
        let b = Belegung::aus(&store, &snap, heute());
        assert!(!b.ist_belegt(3));
    }

    #[test]
    fn belegung_aufs_feld_heisst_dienst_und_vom_feld_wieder_reserviert() {
        let store = store_an();
        store.assign(1, OfficialRole::Sr, 3);
        let auf_dem_feld = snap_mit(vec![spiel(1, MatchStatus::OnCourt, None, None)], &[3]);
        let b = Belegung::aus(&store, &auf_dem_feld, heute());
        assert_eq!(b.im_dienst_bei(3), Some(101));
        assert!(b.reserviert_fuer(3).is_empty());
        // Vom Feld genommen: wieder wartend ⇒ wieder reserviert.
        let zurueck = snap_mit(vec![spiel(1, MatchStatus::Scheduled, None, None)], &[3]);
        let b = Belegung::aus(&store, &zurueck, heute());
        assert_eq!(b.im_dienst_bei(3), None);
        assert_eq!(b.reserviert_fuer(3), &[1]);
    }

    #[test]
    fn belegung_ohne_reserviert_niemanden_auch_nicht_den_btp_wert() {
        let store = store_an();
        store.clear_assignment(1, OfficialRole::Sr);
        let snap = snap_mit(vec![spiel(1, MatchStatus::Scheduled, Some(3), None)], &[3]);
        let b = Belegung::aus(&store, &snap, heute());
        assert!(!b.ist_belegt(3));
    }

    #[test]
    fn belegt_ausser_nimmt_das_eigene_spiel_aus() {
        let store = store_an();
        let snap = snap_mit(
            vec![
                spiel(1, MatchStatus::Scheduled, Some(3), Some(4)),
                spiel(2, MatchStatus::OnCourt, Some(5), None),
            ],
            &[3, 4, 5],
        );
        let b = Belegung::aus(&store, &snap, heute());
        assert_eq!(b.belegt_ausser(1), HashSet::from([5]));
        assert_eq!(b.belegt_ausser(2), HashSet::from([3, 4]));
        assert_eq!(b.belegt_ausser(99), HashSet::from([3, 4, 5]));
        assert_eq!(b.alle(), HashSet::from([3, 4, 5]));
    }

    #[test]
    fn doppelt_nur_bei_zwei_wartenden_spielen() {
        let store = store_an();
        let snap = snap_mit(
            vec![
                spiel(1, MatchStatus::Scheduled, Some(3), None),
                spiel(2, MatchStatus::Scheduled, None, Some(3)),
                spiel(3, MatchStatus::OnCourt, Some(4), None),
                spiel(4, MatchStatus::Scheduled, Some(4), None),
            ],
            &[3, 4],
        );
        let b = Belegung::aus(&store, &snap, heute());
        assert!(b.doppelt(3));
        assert_eq!(b.reserviert_fuer(3), &[1, 2]);
        // Dienst auf einem laufenden Feld + eine Reservierung ist nicht doppelt.
        assert!(!b.doppelt(4));
        // An beiden fremden Spielen gebunden ⇒ für keines frei.
        assert!(b.belegt_ausser(1).contains(&3));
    }

    #[test]
    fn eine_offene_paarung_reserviert_ebenso() {
        let store = store_an();
        let mut finale = spiel(1, MatchStatus::Scheduled, None, None);
        finale.team1.clear();
        finale.team2.clear();
        store.assign(1, OfficialRole::Sr, 3);
        let snap = snap_mit(vec![finale], &[3]);
        assert_eq!(
            Belegung::aus(&store, &snap, heute()).reserviert_fuer(3),
            &[1]
        );
    }

    #[test]
    fn dienst_an_zwei_feldern_bindet_fuer_beide_und_ohne_feld_meldet_null() {
        let store = store_an();
        let mut ohne_feld = spiel(3, MatchStatus::OnCourt, Some(4), None);
        ohne_feld.court_id = None;
        let snap = snap_mit(
            vec![
                spiel(1, MatchStatus::OnCourt, Some(3), None),
                spiel(2, MatchStatus::OnCourt, None, Some(3)),
                ohne_feld,
            ],
            &[3, 4],
        );
        let b = Belegung::aus(&store, &snap, heute());
        assert!(b.belegt_ausser(1).contains(&3), "an Spiel 2 im Dienst");
        assert!(b.belegt_ausser(2).contains(&3), "an Spiel 1 im Dienst");
        assert_eq!(b.im_dienst_bei(3), Some(101));
        assert_eq!(b.im_dienst_bei(4), Some(0));
    }

    #[test]
    fn ohne_schiedsrichter_betrieb_ist_niemand_gebunden() {
        let store = OfficialsStore::default();
        let snap = snap_mit(
            vec![
                spiel(1, MatchStatus::Scheduled, Some(3), None),
                spiel(2, MatchStatus::OnCourt, Some(4), None),
            ],
            &[3, 4],
        );
        let b = Belegung::aus(&store, &snap, heute());
        assert!(b.alle().is_empty());
    }

    #[test]
    fn rotation_ueberspringt_reservierte_ausser_am_eigenen_spiel() {
        let store = store_an();
        store.sync_roster(&[1, 2]);
        let snap = snap_mit(
            vec![
                spiel(10, MatchStatus::OnCourt, None, None),
                spiel(11, MatchStatus::Scheduled, Some(1), None),
            ],
            &[1, 2],
        );
        let b = Belegung::aus(&store, &snap, heute());
        let belegt = b.belegt_ausser(10);
        let players = [spieler(50, "X")];
        let mut e = eingabe(10, &players, &[1, 2], &belegt);
        e.ar = false;
        store.rotate_court(e);
        assert_eq!(
            store.assignment(10).sr,
            Some(2),
            "1 ist für Spiel 11 reserviert"
        );
    }

    // ── Einsatz-Zählung ohne Walkover (AK 17) ─────────────────────────

    #[test]
    fn ein_walkover_zaehlt_nicht_als_einsatz_aufgabe_und_dq_schon() {
        // Kampflos heißt: kein Spiel, also auch kein Dienst. Wer bei
        // Aufgabe oder Disqualifikation am Feld stand, hat aber gearbeitet.
        let store = store_an();
        let beendet: Vec<FinishedMatch> = [
            (1, MatchResult::Normal),
            (2, MatchResult::Walkover),
            (3, MatchResult::Retired),
            (4, MatchResult::Disqualified),
        ]
        .into_iter()
        .map(|(id, result)| {
            let mut m = spiel(id, MatchStatus::Finished, Some(7), Some(8));
            m.result = result;
            m.finished_at = Some(id as u64 * 1_000);
            FinishedMatch::aus_btp(&m)
        })
        .collect();
        assert!(beendet[1].walkover);
        assert!(!beendet[2].walkover && !beendet[3].walkover);

        let e = store.appearances(&beendet);
        for official in [7, 8] {
            let ids: Vec<i64> = e[&official].iter().map(|a| a.match_id).collect();
            assert_eq!(ids, vec![1, 3, 4], "Walkover fehlt in Zähler und Liste");
        }
    }

    #[test]
    fn aus_btp_uebernimmt_besetzung_feld_und_endezeit() {
        let mut m = spiel(5, MatchStatus::Finished, Some(1), Some(2));
        m.court_id = Some(9);
        m.finished_at = Some(42);
        let f = FinishedMatch::aus_btp(&m);
        assert_eq!(
            (
                f.match_id,
                f.btp_sr,
                f.btp_ar,
                f.court_id,
                f.finished_at,
                f.walkover
            ),
            (5, Some(1), Some(2), Some(9), Some(42), false)
        );
    }

    // ── Prüfung der Einteilung (AK 4) ─────────────────────────────────

    #[test]
    fn pruefe_einteilung_lehnt_unbekanntes_und_beendetes_spiel_ab() {
        let store = store_an();
        let mut beendet = spiel(2, MatchStatus::Finished, Some(4), None);
        beendet.match_num = Some(42);
        let snap = snap_mit(
            vec![spiel(1, MatchStatus::Scheduled, None, None), beendet],
            &[4, 5],
        );
        // Unbekannt — auch ganz ohne Turnierstand.
        let fehler =
            pruefe_einteilung(&store, Some(&snap), 99, OfficialRole::Sr, Some(4)).unwrap_err();
        assert!(fehler.contains("Spiel 99 gibt es"), "{fehler}");
        assert!(pruefe_einteilung(&store, None, 1, OfficialRole::Sr, Some(4)).is_err());
        // Beendet — Einteilen und Lösen; der Text nennt die Spielnummer.
        for neu in [Some(5), None] {
            let fehler =
                pruefe_einteilung(&store, Some(&snap), 2, OfficialRole::Sr, neu).unwrap_err();
            assert!(fehler.contains("Spiel 42 ist schon beendet"), "{fehler}");
        }
        // Ein wartendes Spiel (Warteliste, `courtId 0`) ist erlaubt.
        assert!(pruefe_einteilung(&store, Some(&snap), 1, OfficialRole::Sr, Some(4)).is_ok());
        assert!(pruefe_einteilung(&store, Some(&snap), 1, OfficialRole::Ar, None).is_ok());
    }

    #[test]
    fn pruefe_einteilung_lehnt_sr_gleich_ar_in_sicht_fuer_schreiben_ab() {
        let store = store_an();
        // BTP führt 4 als AR; von Hand ist 5 als AR eingeteilt — für BTP
        // ist also 5 der AR, 4 ist frei.
        store.assign(1, OfficialRole::Ar, 5);
        let snap = snap_mit(
            vec![spiel(1, MatchStatus::Scheduled, None, Some(4))],
            &[4, 5],
        );
        let fehler =
            pruefe_einteilung(&store, Some(&snap), 1, OfficialRole::Sr, Some(5)).unwrap_err();
        assert!(
            fehler.contains("SR5"),
            "der Text nennt die Person: {fehler}"
        );
        assert!(fehler.contains("Aufschlagrichter"), "{fehler}");
        assert!(
            pruefe_einteilung(&store, Some(&snap), 1, OfficialRole::Sr, Some(4)).is_ok(),
            "der umgeteilte BTP-AR darf SR werden"
        );
        // Spiegelbildlich: BTP-SR 4 als AR einteilen.
        let snap = snap_mit(vec![spiel(3, MatchStatus::OnCourt, Some(4), None)], &[4, 5]);
        assert!(pruefe_einteilung(&store, Some(&snap), 3, OfficialRole::Ar, Some(4)).is_err());
        // Dieselbe Rolle erneut setzen ist kein Konflikt.
        assert!(pruefe_einteilung(&store, Some(&snap), 3, OfficialRole::Sr, Some(4)).is_ok());
    }

    #[test]
    fn pruefe_einteilung_lehnt_personen_ausserhalb_der_btp_liste_ab() {
        let store = store_an();
        let snap = snap_mit(vec![spiel(1, MatchStatus::Scheduled, None, None)], &[4]);
        let fehler =
            pruefe_einteilung(&store, Some(&snap), 1, OfficialRole::Sr, Some(77)).unwrap_err();
        assert!(fehler.contains("77"), "{fehler}");
        for kaputt in [0, -3] {
            assert!(
                pruefe_einteilung(&store, Some(&snap), 1, OfficialRole::Sr, Some(kaputt)).is_err(),
                "ID {kaputt} ist keine Person"
            );
        }
    }

    #[test]
    fn reserviert_spielnummer_nimmt_die_kleinste_nummer() {
        let store = store_an();
        let mut a = spiel(1, MatchStatus::Scheduled, Some(4), None);
        a.match_num = Some(30);
        let mut b = spiel(2, MatchStatus::Scheduled, Some(4), None);
        b.match_num = Some(12);
        let mut ohne = spiel(3, MatchStatus::Scheduled, Some(5), None);
        ohne.match_num = None;
        let snap = snap_mit(vec![a, b, ohne], &[4, 5, 6]);
        let bel = Belegung::aus(&store, &snap, heute());
        assert_eq!(bel.reserviert_spielnummer(4, &snap), Some(12));
        assert_eq!(
            bel.reserviert_spielnummer(5, &snap),
            None,
            "reserviert ohne Nummer"
        );
        assert!(bel.ist_belegt(5));
        assert_eq!(bel.reserviert_spielnummer(6, &snap), None);
    }
}
