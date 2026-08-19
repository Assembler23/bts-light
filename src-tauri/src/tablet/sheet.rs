//! Ereignis-Speicher des Hosts für den Schiedsrichterzettel
//! (Spec `docs/features/schiedsrichterzettel-druck.md`, ADR 0037/0038).
//!
//! Hält je Match die erfassten Ereignisse (`MatchEvent` aus `relay-proto`)
//! im RAM und persistiert sie **je Turnier** als eine JSON-Datei
//! `zettel/<slug>.json` — dauerhaft und **ohne Namen**: nur `team`/`player`
//! als Koordinaten, die Namen kommen beim Drucken aus dem BTP-Snapshot.
//!
//! Zweiter Strom **neben** [`super::timeline`] (ADR 0037). Karten sind
//! personenbezogene Sanktionsdaten; sie in `MatchTimeline` zu legen kippte
//! deren Begründung, personenbezugsfrei und badhub-tauglich zu sein
//! (ADR 0015). Getrennte Deckel sorgen zusätzlich dafür, dass ein
//! Ereignis-Schwall den Punktverlauf strukturell nicht verdrängen kann.
//!
//! Grundsätze:
//! - **Append-only, vereinigend statt ersetzend** (ADR 0038). Das ist der
//!   eine Punkt, in dem sich dieser Store vom `TimelineStore` bewusst
//!   unterscheidet: Dort **ersetzt** `apply_sync` den Stand komplett —
//!   für die Ereignisse wäre das ein Datenverlust, denn ein übernehmendes
//!   Ersatz-Tablet kennt die Karten seines Vorgängers nicht. Für sie hält
//!   der **Host** die Wahrheit.
//! - **Vereinigung ist idempotent und kommutativ.** Es ist egal, in
//!   welcher Reihenfolge Host und Tablet ihre Stände lernen; ein Tablet,
//!   das offline war, bringt seine Ereignisse beim Reconnect einfach nach.
//! - **Best effort wie `persist_scores`**: Ein Schreibfehler kostet den
//!   Zettel, nie das Zählen oder ein Ergebnis.
//! - **Verwerfen statt raten**: Ein ungültiges Ereignis wird komplett
//!   verworfen, nie beschnitten — ein halbes Ereignis wäre eine stille
//!   Lüge auf einem Archivbeleg.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use relay_proto::{match_events_valid, MatchEvent, MAX_EVENTS_PER_MATCH, MAX_SHEET_LEN};
use serde::{Deserialize, Serialize};

use super::timeline::{slugify, today};

/// Debounce fürs Schreiben, wie beim Punktverlauf. Ereignisse kommen
/// deutlich seltener als Ballwechsel; der Deckel schützt trotzdem vor
/// einem Tablet, das im Dauerfeuer synct.
const PERSIST_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(3);

/// Ereignisse und Abschluss-Vermerke eines Matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSheet {
    /// Sortiert nach `(set, after_n, seq, id)` — siehe [`sortiere`].
    #[serde(default)]
    pub events: Vec<MatchEvent>,
    /// Ergebnis abgegeben. **Eigene** Vermerke statt eines Blicks in den
    /// Punktverlauf, weil ein Match Ereignisse **ohne** Punktverlauf haben
    /// kann (eine Karte vor dem ersten Ballwechsel, ADR 0037).
    #[serde(default)]
    pub finished: bool,
    /// Sonderausgang (Aufgabe/Disqualifikation) — Ergebnisart im Zettelfuß.
    #[serde(default)]
    pub retired: bool,
}

/// Dateiform: Turnier-Kopf + Zettel. Bewusst **ohne Namen**, wie die
/// Punktverlauf-Datei.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SheetFile {
    #[serde(default)]
    tournament: String,
    #[serde(default)]
    first_seen: String,
    #[serde(default)]
    guid: String,
    #[serde(default)]
    matches: HashMap<i64, MatchSheet>,
}

#[derive(Default)]
struct Inner {
    file: SheetFile,
    /// Dateiname (Slug) des aktuellen Turniers. Leer = noch kein Turnier
    /// gesehen → nichts wird geschrieben.
    slug: String,
    dirty: bool,
    last_write: Option<Instant>,
}

/// Der Zettel-Ereignis-Speicher. Lebt im
/// [`TabletState`](super::state::TabletState) und wird von LAN-Server,
/// Relay-Client und Tauri-Commands geteilt.
#[derive(Default)]
pub struct SheetStore {
    /// Ablage-Verzeichnis (`<app_data>/zettel`). `None` = Persistenz aus
    /// (Tests, Slave-Betrieb).
    dir: RwLock<Option<PathBuf>>,
    guid: RwLock<String>,
    inner: Mutex<Inner>,
    /// Serialisiert die Dateizugriffe (Muster `persist_scores`).
    persist_lock: Mutex<()>,
}

impl SheetStore {
    /// Ablage-Verzeichnis setzen (beim Start). Aktiviert die Persistenz.
    pub fn set_dir(&self, dir: PathBuf) {
        let _ = std::fs::create_dir_all(&dir);
        *self.dir.write().unwrap() = Some(dir);
    }

    /// turnier.de-GUID aus der Config (leer = nicht gepflegt).
    pub fn set_guid(&self, guid: &str) {
        *self.guid.write().unwrap() = guid.trim().to_string();
        let mut inner = self.inner.lock().unwrap();
        if !inner.slug.is_empty() {
            let guid = self.guid.read().unwrap().clone();
            if inner.file.guid != guid {
                inner.file.guid = guid;
                inner.dirty = true;
            }
        }
    }

    /// Aktuelles Turnier melden (vom Sync-Loop, je Snapshot). Bei einem
    /// Wechsel wird der bisherige Stand gesichert und die neue Datei
    /// geladen oder angelegt — die alte bleibt erhalten.
    pub fn set_tournament(&self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let slug = slugify(name);
        {
            let inner = self.inner.lock().unwrap();
            if inner.slug == slug {
                return; // unverändert — der Normalfall jedes Sync-Ticks
            }
        }
        self.persist(true);
        let loaded = self.load_file(&slug);
        let mut inner = self.inner.lock().unwrap();
        inner.slug = slug;
        inner.dirty = false;
        inner.last_write = None;
        inner.file = loaded.unwrap_or_else(|| SheetFile {
            tournament: name.to_string(),
            first_seen: today(),
            guid: self.guid.read().unwrap().clone(),
            matches: HashMap::new(),
        });
    }

    /// Ein einzelnes Ereignis aufnehmen.
    ///
    /// `false` = nicht aufgenommen (ungültig, Fremd-Match, bereits
    /// bekannt, Deckel erreicht). Ein bereits bekanntes Ereignis ist
    /// **kein Fehler**, sondern der Normalfall bei doppelter Zustellung —
    /// der Bestand bleibt unverändert.
    ///
    /// **Der Bestand wird erst angefasst, wenn das Ergebnis feststeht.**
    /// Gerechnet wird auf einer Kopie; scheitert eine Prüfung, bleibt das
    /// Original unberührt. Ein Zurückrollen *nach* dem Sortieren wäre
    /// positionsbasiert und träfe womöglich ein bereits akzeptiertes
    /// Ereignis, weil die Ordnung neue zwischen alte einsortiert
    /// (Review-Befund 19.08.2026, zwei Datenverluste).
    pub fn apply_event(&self, match_id: i64, event: MatchEvent) -> bool {
        if match_id <= 0 || !event.is_valid() {
            return false;
        }
        let mut inner = self.inner.lock().unwrap();
        let sheet = inner.file.matches.entry(match_id).or_default();
        if sheet.events.iter().any(|e| e.id == event.id) {
            return false; // Dedupe über die Kennung (ADR 0038)
        }
        // Deckel: verwerfen statt wachsen, wie beim Punktverlauf. Der
        // Bestand wächst monoton — die Rücknahmen zählen mit.
        if sheet.events.len() >= MAX_EVENTS_PER_MATCH {
            return false;
        }
        let mut kandidat = sheet.events.clone();
        kandidat.push(event);
        sortiere(&mut kandidat);
        // Größen-Deckel auch im Store, nicht nur im Relay (Spec): Der
        // LAN-Weg läuft an keinem Relay vorbei.
        if !passt_in_deckel(&kandidat) {
            return false;
        }
        sheet.events = kandidat;
        inner.dirty = true;
        drop(inner);
        self.persist(false);
        true
    }

    /// Kompletter Abgleich mit einem Tablet: **Vereinigung**, kein Ersatz
    /// (ADR 0038).
    ///
    /// Bekannte Kennungen werden ignoriert, unbekannte aufgenommen. Die
    /// Operation ist **idempotent und kommutativ**; ein Ersatz-Tablet kann
    /// nichts wegnehmen, und ein Tablet, das offline war, bringt seinen
    /// Rückstand einfach nach.
    ///
    /// **Am Deckel gilt: Vorhandenes ist unantastbar.** Passt nicht alles
    /// Neue, werden die Neuen zuerst in die kanonische Ordnung gebracht
    /// und dann von vorn aufgenommen, so weit der Platz reicht. Damit
    /// hängt das Ergebnis nur von der *Menge* der Ereignisse ab, nicht von
    /// ihrer Reihenfolge im Frame — sonst endeten zwei Hosts mit
    /// demselben Wissen bei verschiedenen Zetteln. Ein Verdrängen
    /// vorhandener Ereignisse durch besser sortierte neue wäre die
    /// Alternative, verstieße aber gegen ADR 0038: nichts wird gelöscht.
    ///
    /// `false` = ungültig und komplett verworfen, **oder** der Deckel hat
    /// etwas abgeschnitten. Ein Abgleich, der nichts Neues bringt, ist
    /// dagegen `true`.
    pub fn apply_event_sync(&self, match_id: i64, events: Vec<MatchEvent>) -> bool {
        if match_id <= 0 || !match_events_valid(&events) {
            return false;
        }
        let mut inner = self.inner.lock().unwrap();
        let sheet = inner.file.matches.entry(match_id).or_default();

        // Nur die wirklich neuen Kennungen, jede höchstens einmal — ein
        // Frame darf dieselbe Kennung doppelt tragen.
        let mut neue: Vec<MatchEvent> = Vec::new();
        for event in events {
            let bekannt = sheet.events.iter().any(|e| e.id == event.id)
                || neue.iter().any(|e| e.id == event.id);
            if !bekannt {
                neue.push(event);
            }
        }
        if neue.is_empty() {
            return true; // nichts Neues — kein Schreibvorgang nötig
        }

        // Kanonische Ordnung VOR der Auswahl: Sie macht die Auswahl am
        // Deckel reihenfolgeunabhängig.
        sortiere(&mut neue);
        let platz = MAX_EVENTS_PER_MATCH.saturating_sub(sheet.events.len());
        let gekappt = neue.len() > platz;
        neue.truncate(platz);
        if gekappt {
            tracing::warn!(
                "Zettel-Abgleich für Match {match_id}: Deckel erreicht,                  {} Ereignis(se) nicht aufgenommen",
                MAX_EVENTS_PER_MATCH
            );
        }
        if neue.is_empty() {
            return false; // Deckel war schon voll — nichts ging hinein
        }

        // Auf einer Kopie rechnen: Scheitert der Größen-Deckel, bleibt der
        // vorhandene Bestand unberührt (Review-Befund 19.08.2026).
        let mut kandidat = sheet.events.clone();
        kandidat.extend(neue);
        sortiere(&mut kandidat);
        if !passt_in_deckel(&kandidat) {
            return false;
        }
        sheet.events = kandidat;
        inner.dirty = true;
        drop(inner);
        self.persist(true);
        !gekappt
    }

    /// Match abschließen (aus dem Ergebnisweg). Ohne erfasstes Ereignis
    /// entsteht bewusst **kein** Eintrag — ein Papier-Spiel bekommt keinen
    /// Geister-Zettel.
    ///
    /// Der Vermerk **blockiert nichts**: Der Bestand bleibt append-only,
    /// und ein wiedereröffnetes Match muss weiter erfassen können. Der
    /// wirkliche Riegel gegen Nachzügler ist der Ingest-Filter (nur der
    /// aktive Halter, nur fürs aktuelle Court-Match).
    pub fn finalize(&self, match_id: i64, retired: bool) {
        let mut inner = self.inner.lock().unwrap();
        let Some(sheet) = inner.file.matches.get_mut(&match_id) else {
            return;
        };
        sheet.finished = true;
        if retired {
            sheet.retired = true;
        }
        inner.dirty = true;
        drop(inner);
        // **Ohne Mindestabstand**, anders als bei jedem anderen Schreiber:
        // Der Abschluss kommt genau einmal je Match und taugt nicht zum
        // Dauerfeuer — der Deckel schützt gegen `apply_event_sync`, nicht
        // gegen ihn. Mit Mindestabstand ginge der Vermerk verloren, wenn
        // der Abgleich Millisekunden vorher geschrieben hat und die App
        // danach beendet wird; auf einem Archivbeleg stünde dann eine
        // Aufgabe als reguläres Spielende.
        self.persist_mit_abstand(std::time::Duration::ZERO);
    }

    /// Gibt es zu diesem Match erfasste Ereignisse? Grundlage der
    /// Zettel-Knöpfe — kein „Klick ins Leere".
    pub fn has_events(&self, match_id: i64) -> bool {
        let inner = self.inner.lock().unwrap();
        inner
            .file
            .matches
            .get(&match_id)
            .is_some_and(|s| !s.events.is_empty())
    }

    /// Der Zettel-Stand eines Matches (für Projektion und Renderer).
    pub fn sheet(&self, match_id: i64) -> Option<MatchSheet> {
        let inner = self.inner.lock().unwrap();
        inner.file.matches.get(&match_id).cloned()
    }

    // ── Persistenz ────────────────────────────────────────────────────

    fn load_file(&self, slug: &str) -> Option<SheetFile> {
        let dir = self.dir.read().unwrap().clone()?;
        let text = std::fs::read_to_string(dir.join(format!("{slug}.json"))).ok()?;
        match serde_json::from_str::<SheetFile>(&text) {
            Ok(file) => Some(file),
            Err(_) => {
                tracing::warn!("Zettel-Datei {slug}.json unlesbar – beginne leer");
                None
            }
        }
    }

    /// Schreiben, wenn fällig. Best effort: Fehler stören das Zählen nie.
    /// Aufbau wie `TimelineStore::persist` — Schreib-Guard **vor** dem
    /// Schnappschuss, Mindestabstand auch bei `force`, atomarer Tausch.
    fn persist(&self, force: bool) {
        self.persist_mit_abstand(if force {
            std::time::Duration::from_millis(500)
        } else {
            PERSIST_DEBOUNCE
        });
    }

    fn persist_mit_abstand(&self, mindestabstand: std::time::Duration) {
        let Some(dir) = self.dir.read().unwrap().clone() else {
            return;
        };
        let _guard = self.persist_lock.lock().unwrap();
        let data = {
            let mut inner = self.inner.lock().unwrap();
            if inner.slug.is_empty() || !inner.dirty {
                return;
            }
            if inner
                .last_write
                .is_some_and(|t| t.elapsed() < mindestabstand)
            {
                return;
            }
            inner.dirty = false;
            inner.last_write = Some(Instant::now());
            (inner.slug.clone(), inner.file.clone())
        };
        let path = dir.join(format!("{}.json", data.0));
        if let Ok(json) = serde_json::to_string(&data.1) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

/// Stabile Ordnung `(set, after_n, seq, id)`.
///
/// Die Kennung als letztes Kriterium ist kein Schmuck: Ohne sie hinge die
/// Reihenfolge zweier gleichzeitig erfasster Ereignisse davon ab, in
/// welcher Reihenfolge die Frames eintrafen — und der gedruckte Zettel
/// sähe je nach Netzlaune anders aus.
fn sortiere(events: &mut [MatchEvent]) {
    events.sort_by(|a, b| (a.set, a.after_n, a.seq, &a.id).cmp(&(b.set, b.after_n, b.seq, &b.id)));
}

/// Zahl **und** serialisierte Größe im Deckel?
fn passt_in_deckel(events: &[MatchEvent]) -> bool {
    match_events_valid(events)
        && serde_json::to_string(events).is_ok_and(|json| json.len() <= MAX_SHEET_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_proto::{EventKind, Phase, MAX_EVENT_ID_LEN};
    use std::path::Path;

    fn store_mit_datei(dir: &Path) -> SheetStore {
        let store = SheetStore::default();
        store.set_dir(dir.to_path_buf());
        store.set_tournament("Test BTS Light");
        store
    }

    /// Ereignis mit sprechender Kennung — die Kennung muss Hex sein,
    /// deshalb `e0`, `e1`, … statt Klartext.
    fn ereignis(id: &str, set: i64, after_n: i64, seq: i64, kind: EventKind) -> MatchEvent {
        MatchEvent {
            id: id.to_string(),
            seq,
            set,
            after_n,
            score_a: 5,
            score_b: 3,
            ts_ms: 1_755_600_000_000,
            kind,
            team: 0,
            player: 0,
            receiver_team: 1,
            receiver_player: 0,
            phase: Phase::Play,
            retracts: String::new(),
        }
    }

    fn karte(id: &str) -> MatchEvent {
        ereignis(id, 1, 7, 1, EventKind::CardYellow)
    }

    fn kennungen(store: &SheetStore, match_id: i64) -> Vec<String> {
        store
            .sheet(match_id)
            .map(|s| s.events.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Der Kern von ADR 0038. Beim Punktverlauf **ersetzt** `apply_sync`;
    /// täte das der Zettel-Store auch, löschte ein übernehmendes Tablet
    /// die Karten seines Vorgängers, die es gar nicht kennen kann.
    #[test]
    fn ersatz_tablet_loescht_keine_ereignisse() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());

        // Gerät A erfasst drei Ereignisse.
        for (i, id) in ["a1", "a2", "a3"].iter().enumerate() {
            assert!(store.apply_event(5, ereignis(id, 1, i as i64, i as i64, EventKind::Overrule)));
        }
        assert_eq!(kennungen(&store, 5).len(), 3);

        // Gerät B übernimmt und synct mit leerer Liste — es kennt die
        // Ereignisse des Vorgängers nicht.
        assert!(store.apply_event_sync(5, vec![]));
        assert_eq!(kennungen(&store, 5), vec!["a1", "a2", "a3"]);

        // Und bringt danach sein eigenes ein, ohne die alten zu verdrängen.
        assert!(store.apply_event_sync(5, vec![ereignis("b1", 2, 0, 0, EventKind::CardRed)]));
        assert_eq!(kennungen(&store, 5), vec!["a1", "a2", "a3", "b1"]);
    }

    #[test]
    fn dasselbe_ereignis_zweimal_aendert_nichts() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert!(store.apply_event(5, karte("ab")));
        // Zweite Zustellung: kein Fehler im Sinne von „kaputt", aber auch
        // keine Änderung — deshalb `false`.
        assert!(!store.apply_event(5, karte("ab")));
        assert_eq!(kennungen(&store, 5), vec!["ab"]);
        // Über den Abgleich ebenso — der meldet `true`, weil der Abgleich
        // gültig war, ändert aber nichts.
        assert!(store.apply_event_sync(5, vec![karte("ab")]));
        assert_eq!(kennungen(&store, 5), vec!["ab"]);
    }

    /// ADR 0038: Eine Rücknahme ist ein **zusätzlicher** Eintrag. Nichts
    /// wird gelöscht — ein Archivbeleg, aus dem Einträge spurlos
    /// verschwinden können, ist weniger wert als einer, der Korrekturen
    /// zeigt.
    #[test]
    fn eine_ruecknahme_ist_ein_zusaetzlicher_eintrag() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert!(store.apply_event(5, karte("ab")));

        let mut zurueck = ereignis("cd", 1, 7, 2, EventKind::Retract);
        zurueck.retracts = "ab".to_string();
        assert!(store.apply_event(5, zurueck));

        let sheet = store.sheet(5).unwrap();
        assert_eq!(sheet.events.len(), 2);
        assert_eq!(sheet.events[0].id, "ab");
        assert_eq!(sheet.events[1].kind, EventKind::Retract);
        assert_eq!(sheet.events[1].retracts, "ab");
    }

    #[test]
    fn das_65_ereignis_wird_verworfen() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        for i in 0..MAX_EVENTS_PER_MATCH {
            let id = format!("{i:04x}");
            assert!(
                store.apply_event(5, ereignis(&id, 1, 0, i as i64, EventKind::Overrule)),
                "Ereignis {i} sollte passen"
            );
        }
        assert_eq!(kennungen(&store, 5).len(), MAX_EVENTS_PER_MATCH);

        // Das 65. wird verworfen — und der Bestand bleibt unversehrt.
        assert!(!store.apply_event(5, ereignis("ffff", 1, 0, 999, EventKind::Overrule)));
        assert_eq!(kennungen(&store, 5).len(), MAX_EVENTS_PER_MATCH);
        assert!(!kennungen(&store, 5).contains(&"ffff".to_string()));
    }

    #[test]
    fn fremdes_match_wird_verworfen() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert!(!store.apply_event(0, karte("ab")));
        assert!(!store.apply_event(-1, karte("ab")));
        assert!(!store.apply_event_sync(0, vec![karte("ab")]));
        assert!(store.sheet(0).is_none());
        assert!(store.sheet(-1).is_none());
    }

    /// Ungültiges wird komplett verworfen, nie beschnitten — ein halbes
    /// Ereignis wäre eine stille Lüge auf einem Archivbeleg.
    #[test]
    fn ungueltiges_wird_komplett_verworfen() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        let mut kaputt = karte("ab");
        kaputt.team = 9;
        assert!(!store.apply_event(5, kaputt.clone()));
        assert!(store.sheet(5).is_none_or(|s| s.events.is_empty()));

        // Ein einziges kaputtes Ereignis verwirft den ganzen Abgleich —
        // sonst hinge der Rest von der Reihenfolge in der Liste ab.
        assert!(!store.apply_event_sync(5, vec![karte("ab"), kaputt]));
        assert!(store.sheet(5).is_none_or(|s| s.events.is_empty()));
    }

    #[test]
    fn der_bestand_ueberlebt_einen_app_neustart() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert!(store.apply_event_sync(5, vec![karte("ab")]));
        store.finalize(5, true);

        // „Neustart": frischer Store, gleiches Verzeichnis, gleiches Turnier.
        let store2 = SheetStore::default();
        store2.set_dir(dir.path().to_path_buf());
        store2.set_tournament("Test BTS Light");
        let sheet = store2.sheet(5).unwrap();
        assert_eq!(sheet.events.len(), 1);
        assert_eq!(sheet.events[0].kind, EventKind::CardYellow);
        assert!(sheet.finished);
        assert!(sheet.retired);
    }

    #[test]
    fn ein_turnierwechsel_oeffnet_eine_neue_datei() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        assert!(store.apply_event_sync(5, vec![karte("ab")]));

        store.set_tournament("Anderes Turnier");
        assert!(store.sheet(5).is_none());
        assert!(store.apply_event_sync(7, vec![karte("cd")]));

        // Zurück zum ersten Turnier: der alte Stand liegt noch da.
        store.set_tournament("Test BTS Light");
        assert_eq!(kennungen(&store, 5), vec!["ab"]);
        assert!(store.sheet(7).is_none());
        assert!(dir.path().join("test-bts-light.json").exists());
        assert!(dir.path().join("anderes-turnier.json").exists());
    }

    /// Die Ordnung ist `(set, after_n, seq, id)` — unabhängig davon, in
    /// welcher Reihenfolge die Frames eintrafen.
    #[test]
    fn die_reihenfolge_ist_stabil() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        // Bewusst vertauscht eingespielt.
        assert!(store.apply_event(5, ereignis("d4", 2, 3, 1, EventKind::Overrule)));
        assert!(store.apply_event(5, ereignis("a1", 1, 0, 1, EventKind::Overrule)));
        assert!(store.apply_event(5, ereignis("c3", 1, 9, 2, EventKind::Overrule)));
        assert!(store.apply_event(5, ereignis("b2", 1, 9, 1, EventKind::Overrule)));
        assert_eq!(kennungen(&store, 5), vec!["a1", "b2", "c3", "d4"]);

        // Gleicher Anker, gleiche seq: die Kennung entscheidet — sonst
        // hinge das gedruckte Bild an der Netzlaune.
        assert!(store.apply_event(5, ereignis("00", 1, 0, 1, EventKind::Overrule)));
        assert_eq!(kennungen(&store, 5), vec!["00", "a1", "b2", "c3", "d4"]);
    }

    /// ADR 0038: idempotent und kommutativ. Deshalb darf ein Tablet, das
    /// offline war, seinen Rückstand in beliebiger Reihenfolge nachbringen.
    #[test]
    fn vereinigung_ist_idempotent_und_reihenfolgeunabhaengig() {
        let dir = tempfile::tempdir().unwrap();
        let a = store_mit_datei(dir.path());
        let e1 = ereignis("a1", 1, 0, 1, EventKind::Overrule);
        let e2 = ereignis("b2", 1, 5, 2, EventKind::CardRed);
        let e3 = ereignis("c3", 2, 1, 3, EventKind::InjuryStart);

        assert!(a.apply_event_sync(5, vec![e1.clone(), e2.clone()]));
        assert!(a.apply_event_sync(5, vec![e2.clone(), e3.clone()]));
        assert!(a.apply_event_sync(5, vec![e1.clone(), e2.clone(), e3.clone()]));

        let dir2 = tempfile::tempdir().unwrap();
        let b = store_mit_datei(dir2.path());
        assert!(b.apply_event_sync(5, vec![e3, e2]));
        assert!(b.apply_event_sync(5, vec![e1]));

        assert_eq!(kennungen(&a, 5), kennungen(&b, 5));
        assert_eq!(kennungen(&a, 5), vec!["a1", "b2", "c3"]);
    }

    /// Regression zum Review-Befund vom 19.08.2026 (zwei Datenverluste):
    /// Das Zurückrollen am Deckel war **positionsbasiert** (`pop`,
    /// `truncate`) und lief **nach** dem Sortieren. Da die Ordnung neue
    /// Ereignisse zwischen alte einsortiert, traf es womöglich ein bereits
    /// akzeptiertes statt des neuen — auf einem Archivbeleg verschwand so
    /// still eine rote Karte, sichtbar erst beim Druck nach dem Turnier.
    #[test]
    fn der_deckel_verliert_niemals_ein_vorhandenes_ereignis() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());

        // Ein vorhandenes Ereignis, das ganz HINTEN einsortiert — genau
        // das, was ein `pop()` nach dem Sortieren erwischt hätte.
        let spaet = ereignis("ff", 5, 120, i64::MAX, EventKind::CardRed);
        assert!(store.apply_event(5, spaet.clone()));

        // Maximal große Ereignisse, die alle DAVOR einsortieren: volle
        // Kennung, volle Rücknahme-Kennung, volle Zahlen. So reißt der
        // Größen-Deckel, bevor der Zähl-Deckel greift.
        let mut abgewiesen = 0;
        for i in 0..MAX_EVENTS_PER_MATCH {
            let mut e = ereignis(&format!("{i:032x}"), 1, 0, i64::MAX, EventKind::Retract);
            e.retracts = "a".repeat(MAX_EVENT_ID_LEN);
            e.ts_ms = u64::MAX;
            if !store.apply_event(5, e) {
                abgewiesen += 1;
            }
        }
        // Der Zähl-Deckel allein bewiese nichts: Er greift VOR jeder
        // Änderung am Bestand. Nur der Größen-Deckel führte in das
        // fehlerhafte Zurückrollen — er muss also wirklich zuschlagen.
        // Er ist knapp: 64 maximale Ereignisse wiegen 17.345 Bytes gegen
        // einen Deckel von 16.384, mit `seq = 0` dagegen nur 16.193 —
        // ohne volle Zahlenwerte prüfte dieser Test den kaputten Pfad nie.
        assert!(
            abgewiesen > 0 && kennungen(&store, 5).len() < MAX_EVENTS_PER_MATCH,
            "Größen-Deckel nie erreicht — der Test prüft den fehlerhaften Pfad nicht \
             (aufgenommen: {}, abgewiesen: {abgewiesen})",
            kennungen(&store, 5).len()
        );

        // Egal wie viel abgewiesen wurde: das Vorhandene lebt.
        let ids = kennungen(&store, 5);
        assert!(
            ids.contains(&spaet.id),
            "vorhandenes Ereignis verloren: {ids:?}"
        );

        // Und dasselbe über den Abgleich statt über Einzel-Ereignisse.
        let schwall: Vec<MatchEvent> = (0..MAX_EVENTS_PER_MATCH)
            .map(|i| {
                let mut e = ereignis(
                    &format!("{:032x}", i + 1000),
                    1,
                    0,
                    i64::MAX,
                    EventKind::Retract,
                );
                e.retracts = "b".repeat(MAX_EVENT_ID_LEN);
                e.ts_ms = u64::MAX;
                e
            })
            .collect();
        store.apply_event_sync(5, schwall);
        assert!(
            kennungen(&store, 5).contains(&spaet.id),
            "Abgleich hat das vorhandene Ereignis verdrängt"
        );
    }

    /// Am Deckel darf das Ergebnis nur von der **Menge** der Ereignisse
    /// abhängen, nicht von ihrer Reihenfolge im Frame — sonst endeten zwei
    /// Hosts mit demselben Wissen bei verschiedenen Zetteln.
    #[test]
    fn die_auswahl_am_deckel_haengt_nicht_an_der_frame_reihenfolge() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = store_mit_datei(dir_a.path());
        let b = store_mit_datei(dir_b.path());

        // Beide Stores bis knapp unter den Zähl-Deckel füllen.
        for i in 0..(MAX_EVENTS_PER_MATCH - 2) {
            let e = ereignis(&format!("{i:04x}"), 1, 0, i as i64, EventKind::Overrule);
            assert!(a.apply_event(5, e.clone()));
            assert!(b.apply_event(5, e));
        }

        // Dieselben DREI neuen Ereignisse, aber nur noch Platz für zwei.
        let e1 = ereignis("aaaa", 2, 1, 1, EventKind::CardYellow);
        let e2 = ereignis("bbbb", 2, 2, 2, EventKind::CardRed);
        let e3 = ereignis("cccc", 2, 3, 3, EventKind::InjuryStart);

        // Beide melden `false`: der Deckel hat etwas abgeschnitten.
        assert!(!a.apply_event_sync(5, vec![e1.clone(), e2.clone(), e3.clone()]));
        assert!(!b.apply_event_sync(5, vec![e3, e2, e1]));

        assert_eq!(kennungen(&a, 5), kennungen(&b, 5));
        assert_eq!(kennungen(&a, 5).len(), MAX_EVENTS_PER_MATCH);
    }

    /// Ein Match ohne erfasstes Ereignis bekommt keinen Geister-Zettel —
    /// wie beim Punktverlauf, wo ein Papier-Spiel keinen Eintrag erzeugt.
    #[test]
    fn finalisieren_ohne_ereignis_erzeugt_keinen_eintrag() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_mit_datei(dir.path());
        store.finalize(5, false);
        assert!(store.sheet(5).is_none());
        assert!(!store.has_events(5));
    }

    /// Der Datenschutz-Kern von ADR 0037, als Wächter statt als Zusage:
    /// Die Punktverlauf-Datei bleibt frei von Sanktionsdaten, die
    /// Zettel-Datei trägt sie — aber **beide** ohne Namen.
    #[test]
    fn punktverlauf_datei_bleibt_frei_von_sanktionsdaten() {
        let dir = tempfile::tempdir().unwrap();
        let verlauf_dir = dir.path().join("punktverlauf");
        let zettel_dir = dir.path().join("zettel");

        let verlauf = super::super::timeline::TimelineStore::default();
        verlauf.set_dir(verlauf_dir.clone());
        verlauf.set_tournament("Test BTS Light");
        let zettel = SheetStore::default();
        zettel.set_dir(zettel_dir.clone());
        zettel.set_tournament("Test BTS Light");

        // Derselbe Spielverlauf in beiden Strömen: ein Ballwechsel und
        // eine rote Karte gegen Spieler 1 der Mannschaft B.
        assert!(verlauf.apply_rally(5, 1, 1, "A", 1, 0));
        let mut rot = ereignis("ff01", 1, 1, 1, EventKind::CardRed);
        rot.team = 1;
        rot.player = 1;
        assert!(zettel.apply_event_sync(5, vec![rot]));

        let verlauf_json =
            std::fs::read_to_string(verlauf_dir.join("test-bts-light.json")).unwrap();
        let zettel_json = std::fs::read_to_string(zettel_dir.join("test-bts-light.json")).unwrap();

        // Positiv-Nachweis zuerst: Die Karte steht wirklich in der
        // Zettel-Datei — sonst prüfte die Gegenprobe unten nichts.
        assert!(
            zettel_json.contains("card_red"),
            "Fixture trägt keine Karte: {zettel_json}"
        );

        // Gegenprobe: In der Punktverlauf-Datei taucht keine Sanktion auf.
        for verboten in ["card", "retract", "injury", "overrule", "disqualified"] {
            assert!(
                !verlauf_json.contains(verboten),
                "Sanktionsdatum '{verboten}' in der Punktverlauf-Datei: {verlauf_json}"
            );
        }

        // Und in KEINER der beiden Dateien steht ein Personenbezug
        // (ADR 0015). Geprüft werden JSON-**Schlüssel**, nicht blanke
        // Zeichenfolgen: „tournament" enthält sonst „name", und der Test
        // schlüge auf dem Turniernamen an, der hier ausdrücklich erlaubt
        // ist.
        for json in [&verlauf_json, &zettel_json] {
            for verboten in [
                "\"name\":",
                "\"names\":",
                "\"club\":",
                "\"birth",
                "\"licence\":",
                "\"license\":",
                "\"nationality\":",
            ] {
                assert!(
                    !json.contains(verboten),
                    "Personenbezug '{verboten}' in einer Datei: {json}"
                );
            }
        }
    }
}
