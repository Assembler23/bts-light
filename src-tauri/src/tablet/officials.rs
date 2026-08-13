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

use crate::btp::model::BtpPlayer;

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
    /// Wer gerade auf einem ANDEREN Feld Dienst tut (SR oder AR).
    pub im_dienst: &'a HashSet<i64>,
    /// SR-Rotation für dieses Feld aktiv (globaler Schalter UND Feldschalter)?
    pub sr: bool,
    /// AR-Rotation für dieses Feld aktiv?
    pub ar: bool,
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
}

impl OfficialExtra {
    /// Trägt der Eintrag überhaupt etwas? Leere Einträge werden nicht
    /// aufbewahrt — sonst wüchse die Datei mit jedem Blick in einen Dialog.
    fn is_empty(&self) -> bool {
        !self.paused
            && self.club.is_empty()
            && self.blocked_clubs.is_empty()
            && self.blocked_players.is_empty()
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

/// Eine Official-ID als echter Dienst: `Some(0)` (ausdrücklich keiner) und
/// `None` werden beide zu „kein Dienst".
fn dienst(id: Option<i64>) -> Option<i64> {
    id.filter(|v| *v > 0)
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
    /// Rotationsreihenfolge (Official-IDs).
    #[serde(default)]
    order: Vec<i64>,
    /// Official-ID → Zusatzdaten.
    #[serde(default)]
    extras: HashMap<i64, OfficialExtra>,
    /// Match-ID → lokale Zuweisung (bleibt nach Spielende liegen).
    #[serde(default)]
    assignments: HashMap<i64, MatchOfficials>,
    /// CourtID → feldweise Schalter (fehlt = Default „alles aktiv").
    #[serde(default)]
    courts: HashMap<i64, CourtSwitches>,
}

/// Ergebnis eines Ladeversuchs. Der Unterschied zwischen „gibt es nicht"
/// und „komme gerade nicht dran" entscheidet, ob überschrieben werden darf.
enum Ladung {
    /// Ein gelesener Stand (kann zu einem anderen Turnier gehören).
    Stand(OfficialsFile),
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
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `scores_persist_lock`).
    persist_lock: Mutex<()>,
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
                            inner.file = file;
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
        self.mit_zuweisung(match_id, |a| match role {
            OfficialRole::Sr => a.sr = Some(official_id),
            OfficialRole::Ar => a.ar = Some(official_id),
        });
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
        self.mit_zuweisung(match_id, |a| match role {
            OfficialRole::Sr => a.sr = Some(0),
            OfficialRole::Ar => a.ar = Some(0),
        });
    }

    /// Alle Zuweisungen räumen — beim Abschalten von `officials.enabled`
    /// mitten im Turnier (Spec Nr. 1, analog `clear_scorekeeper_assignments`).
    pub fn clear_assignments(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.file.assignments.is_empty() {
                return;
            }
            inner.file.assignments.clear();
        }
        self.persist();
    }

    fn mit_zuweisung(&self, match_id: i64, f: impl FnOnce(&mut MatchOfficials)) {
        {
            let mut inner = self.inner.lock().unwrap();
            let a = inner.file.assignments.entry(match_id).or_default();
            f(a);
            if a.is_empty() {
                inner.file.assignments.remove(&match_id);
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

    /// Freie Dienste eines neu belegten Felds aus der Reihenfolge besetzen
    /// (Spec Nr. 4). Belegte Dienste bleiben unangetastet — auch deshalb ist
    /// der Aufruf idempotent.
    pub fn rotate_court(&self, input: RotationInput<'_>) {
        if input.match_id <= 0 {
            return;
        }
        let wirksam = self.effective(input.match_id, input.btp_sr, input.btp_ar);
        let sr_offen = input.sr && wirksam.sr.is_none();
        let ar_offen = input.ar && wirksam.ar.is_none();
        if !sr_offen && !ar_offen {
            return;
        }
        // Wer an DIESEM Spiel schon Dienst tut, kommt für den zweiten Dienst
        // nicht in Frage — dieselbe Person ist nie SR und AR zugleich.
        let mut vergeben: HashSet<i64> = input.im_dienst.clone();
        vergeben.extend(wirksam.sr);
        vergeben.extend(wirksam.ar);

        for (offen, role) in [(sr_offen, OfficialRole::Sr), (ar_offen, OfficialRole::Ar)] {
            if !offen {
                continue;
            }
            let Some(id) = self.next_free(input.bekannt, &vergeben, input.players) else {
                continue; // niemand frei ⇒ Feld bleibt ohne diesen Dienst
            };
            vergeben.insert(id);
            self.assign(input.match_id, role, id);
        }
    }

    /// Der nächste Official der Reihenfolge, der zugewiesen werden darf:
    /// BTP kennt ihn, er ist nicht pausiert, hat keinen Dienst und keinen
    /// Konflikt. Konflikte werden hier **still** übersprungen — gewarnt wird
    /// nur bei manueller Zuweisung (Spec Nr. 4).
    fn next_free(
        &self,
        bekannt: &[i64],
        vergeben: &HashSet<i64>,
        players: &[BtpPlayer],
    ) -> Option<i64> {
        for id in self.order() {
            if !bekannt.contains(&id) || vergeben.contains(&id) {
                continue;
            }
            let extra = self.extra(id);
            if extra.paused || official_conflict(&extra, players).is_some() {
                continue;
            }
            return Some(id);
        }
        None
    }

    /// Einen Official in der Reihenfolge vor einen anderen ziehen
    /// (`before = None` ⇒ ans Ende). Unbekannte IDs ändern nichts.
    pub fn reorder(&self, official_id: i64, before: Option<i64>) {
        {
            let mut inner = self.inner.lock().unwrap();
            let order = &mut inner.file.order;
            let Some(von) = order.iter().position(|id| *id == official_id) else {
                return;
            };
            // Vor sich selbst ziehen ist keine Bewegung.
            if before == Some(official_id) {
                return;
            }
            let ziel = match before {
                Some(b) => match order.iter().position(|id| *id == b) {
                    Some(p) => p,
                    None => return, // unbekanntes Ziel ⇒ lieber nichts tun
                },
                None => order.len(),
            };
            let id = order.remove(von);
            // Nach dem Entfernen rutscht alles hinter `von` eine Stelle vor.
            let ziel = if ziel > von { ziel - 1 } else { ziel };
            order.insert(ziel, id);
        }
        self.persist();
    }

    /// Einsätze je Official, abgeleitet aus den **beendeten** Spielen
    /// (Spec Nr. 11 — keine eigene Historien-Datenhaltung). Je Official
    /// chronologisch nach Endezeit.
    pub fn appearances(&self, finished: &[FinishedMatch]) -> HashMap<i64, Vec<Appearance>> {
        let mut out: HashMap<i64, Vec<Appearance>> = HashMap::new();
        for f in finished {
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
            liste.sort_by_key(|a| (a.finished_at.unwrap_or(u64::MAX), a.match_id));
        }
        out
    }

    /// Officials ans Ende der Reihenfolge rücken (nach Spielende, Spec Nr. 4).
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
            Ok(file) => Ladung::Stand(file),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::model::BtpPlayer;
    use std::path::Path;

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
        im_dienst: &'a HashSet<i64>,
    ) -> RotationInput<'a> {
        RotationInput {
            match_id,
            players,
            btp_sr: None,
            btp_ar: None,
            bekannt,
            im_dienst,
            sr: true,
            ar: true,
        }
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
            },
            FinishedMatch {
                match_id: 501,
                btp_sr: None,
                btp_ar: None,
                court_id: Some(6),
                finished_at: Some(2_000),
            },
            // Ein Spiel, an dem BTP selbst einen Schiedsrichter trägt.
            FinishedMatch {
                match_id: 600,
                btp_sr: Some(2),
                btp_ar: None,
                court_id: Some(5),
                finished_at: Some(3_000),
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
        }];
        assert!(store.appearances(&beendet).is_empty());
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
}
