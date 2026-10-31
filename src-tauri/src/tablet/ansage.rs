//! Ablage der Ansagen für die Browser-Seite `/ansage` (Spec
//! `ansage-im-browser`, Etappe E1).
//!
//! Ein Eintrag ist eine **fertige** Ansage: Text und Gong-Wahl kommen aus der
//! Oberfläche des Turnier-PCs (dieselbe Logik wie für den eigenen
//! Lautsprecher), das Audio aus Azure. Die Seite spielt nur ab.
//!
//! Drei Dinge hält diese Ablage zusammen, weil sie unter **einem** Schloss
//! stimmen müssen:
//!
//! - **die Einträge** samt Audio — nur im RAM, 60 s (Audio enthält
//!   Spielernamen, Grundentscheidung „Speicher");
//! - **die Zuhörer**: welche Seite gerade in welcher Halle spielt. Ohne
//!   Zuhörer wird nicht vertont — keine Azure-Kosten, kein spätes
//!   Nachsprechen;
//! - **die Beanspruchung** der Quellen (TL-Aufträge, Freitexte): Jede Quelle
//!   geht genau einmal an den Vertoner, auch wenn zwei Abrufe direkt
//!   hintereinander kommen (Muster `print_log().merken`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use relay_proto::{
    halle_passt, AnsageArt, AnsageAudio, AnsageBelegung, AnsageEintrag, AnsageGong, ANSAGE_TTL_MS,
    MAX_ANSAGE_AUDIO, MAX_ANSAGE_AUDIO_GESAMT, MAX_ANSAGE_EINTRAEGE,
};

use super::state::{AnnounceJob, FreetextItem};

/// Ohne Lebenszeichen des Vertoners seit so langer Zeit gilt die Oberfläche
/// des Turnier-PCs als weg (Fenster zu, WebView hängt) — der Feed meldet dann
/// `renderer_weg`, statt Seiten auf Ansagen warten zu lassen, die nie kommen.
pub const VERTONER_FRIST_MS: u64 = 10_000;
/// Eine Seite fragt alle 1,5 s; wer 30 s schweigt, hört nicht mehr zu
/// (dieselbe Frist wie bei den Ansage-Geräten).
pub const SEITEN_FRIST_MS: u64 = 30_000;
/// Höchstzahl verschiedener Hallen in der Zuhörerliste. Die Route steht im
/// Hallennetz offen, der Hallenname kommt ungeprüft aus der Anfrage.
const MAX_SEITEN_HALLEN: usize = 64;
/// Länge eines Hallennamens in der Zuhörerliste.
const MAX_HALLEN_LAENGE: usize = 128;

/// Was die Oberfläche zum Vertonen übergibt — alles außer `id`, Zeitpunkt
/// und Audio, die die Ablage selbst setzt.
#[derive(Debug, Clone, PartialEq)]
pub struct AnsageEntwurf {
    pub hall: String,
    pub art: AnsageArt,
    pub court_id: Option<i64>,
    pub match_id: Option<i64>,
    pub gong: Option<AnsageGong>,
    pub lang: String,
    pub rate: f32,
    pub segmente: Vec<String>,
}

struct Eintrag {
    eintrag: AnsageEintrag,
    audio: Option<Arc<Vec<u8>>>,
}

#[derive(Default)]
struct Innen {
    eintraege: VecDeque<Eintrag>,
    /// Laufende Nummer, mindestens die Uhrzeit — sonst begänne sie nach einem
    /// Neustart wieder klein, und eine Seite mit gemerkter Nummer verstummte.
    seq: u64,
    /// Halle (klein) → (Anzeigename, zuletzt gehört).
    seiten: HashMap<String, (String, u64)>,
    /// Letztes Lebenszeichen des Vertoners.
    vertoner_ms: u64,
    /// Schon beanspruchte Quellen. `None` = noch keine Bestandsaufnahme:
    /// Was beim Einschalten schon lag, wird nicht nachträglich vertont.
    jobs_vergeben: Option<HashSet<u64>>,
    freitexte_vergeben: Option<HashSet<u64>>,
}

/// Die Ablage. Lebt im `TabletState`.
#[derive(Default)]
pub struct AnsageStore {
    innen: Mutex<Innen>,
}

/// Eine beanspruchte Quelle — sie geht an genau einen Vertoner-Abruf.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct OffeneQuellen {
    pub jobs: Vec<AnnounceJob>,
    pub freitexte: Vec<FreetextItem>,
}

impl Innen {
    fn aufraeumen(&mut self, now_ms: u64) {
        self.eintraege
            .retain(|e| e.eintrag.created_at_ms + ANSAGE_TTL_MS > now_ms);
        self.seiten
            .retain(|_, (_, gesehen)| *gesehen + SEITEN_FRIST_MS > now_ms);
    }

    fn hat_seite(&self, hall: &str, now_ms: u64) -> bool {
        self.seiten
            .values()
            .any(|(name, gesehen)| *gesehen + SEITEN_FRIST_MS > now_ms && halle_passt(hall, name))
    }

    fn audio_gesamt(&self) -> usize {
        self.eintraege
            .iter()
            .filter_map(|e| e.audio.as_ref().map(|a| a.len()))
            .sum()
    }
}

impl AnsageStore {
    fn innen(&self) -> std::sync::MutexGuard<'_, Innen> {
        self.innen.lock().expect("Ansage-Ablage nicht vergiftet")
    }

    /// Lebenszeichen des Vertoners (jeder Abruf der offenen Quellen).
    pub fn vertoner_lebt(&self, now_ms: u64) {
        self.innen().vertoner_ms = now_ms;
    }

    /// Lebt der Vertoner? Grundlage von `aktiv` im Feed.
    pub fn vertoner_aktiv(&self, now_ms: u64) -> bool {
        let v = self.innen().vertoner_ms;
        v != 0 && v + VERTONER_FRIST_MS > now_ms
    }

    /// Eine Seite hört in `hall` zu. Gekappt wie die Ansage-Geräte: Ein
    /// fehlkonfiguriertes Gerät soll den Turnier-PC nicht mit erfundenen
    /// Hallennamen volllaufen lassen.
    pub fn seite_hoert(&self, hall: &str, now_ms: u64) {
        let name: String = hall.trim().chars().take(MAX_HALLEN_LAENGE).collect();
        let key = name.to_lowercase();
        let mut g = self.innen();
        g.aufraeumen(now_ms);
        if g.seiten.len() < MAX_SEITEN_HALLEN || g.seiten.contains_key(&key) {
            g.seiten.insert(key, (name, now_ms));
        }
    }

    /// Hört in `hall` gerade eine Seite zu? Leere Halle = irgendeine.
    pub fn hat_seite(&self, hall: &str, now_ms: u64) -> bool {
        self.innen().hat_seite(hall, now_ms)
    }

    /// Hallen, in denen gerade Seiten zuhören (Anzeigenamen, sortiert; eine
    /// Seite ohne Halle erscheint als leerer Name).
    pub fn seiten_hallen(&self, now_ms: u64) -> Vec<String> {
        let mut g = self.innen();
        g.aufraeumen(now_ms);
        let mut v: Vec<String> = g.seiten.values().map(|(n, _)| n.clone()).collect();
        v.sort();
        v
    }

    /// Beansprucht die noch nicht vergebenen Quellen — **atomar**: Was ein
    /// Abruf bekommt, bekommt kein zweiter.
    ///
    /// Der erste Abruf ist Bestandsaufnahme und liefert nichts. Eine Quelle,
    /// für deren Halle gerade keine Seite zuhört, gilt ebenfalls als
    /// vergeben: Sie wird verworfen, nicht aufgehoben — sonst spräche eine
    /// Seite, die später dazukommt, längst Erledigtes nach.
    pub fn beanspruche(
        &self,
        jobs: Vec<AnnounceJob>,
        freitexte: Vec<FreetextItem>,
        now_ms: u64,
    ) -> OffeneQuellen {
        let mut g = self.innen();
        g.aufraeumen(now_ms);
        let mut raus = OffeneQuellen::default();

        let job_ids: HashSet<u64> = jobs.iter().map(|j| j.id).collect();
        match g.jobs_vergeben.take() {
            None => g.jobs_vergeben = Some(job_ids),
            Some(mut vergeben) => {
                // Nur merken, was es noch gibt — die Quelle räumt selbst auf.
                vergeben.retain(|id| job_ids.contains(id));
                for j in jobs {
                    if vergeben.insert(j.id) && g.hat_seite(&j.hall, now_ms) {
                        raus.jobs.push(j);
                    }
                }
                g.jobs_vergeben = Some(vergeben);
            }
        }

        let ft_ids: HashSet<u64> = freitexte.iter().map(|f| f.id).collect();
        match g.freitexte_vergeben.take() {
            None => g.freitexte_vergeben = Some(ft_ids),
            Some(mut vergeben) => {
                vergeben.retain(|id| ft_ids.contains(id));
                for f in freitexte {
                    if vergeben.insert(f.id) && g.hat_seite(&f.hall, now_ms) {
                        raus.freitexte.push(f);
                    }
                }
                g.freitexte_vergeben = Some(vergeben);
            }
        }
        raus
    }

    /// Legt eine fertige Ansage ab und liefert ihre Nummer. Audio über
    /// [`MAX_ANSAGE_AUDIO`] wird weggelassen — die Seite spricht dann die
    /// Segmente mit der Gerätestimme.
    pub fn ablegen(
        &self,
        entwurf: AnsageEntwurf,
        audio: Option<(String, Vec<u8>)>,
        now_ms: u64,
    ) -> u64 {
        let audio = audio.filter(|(_, b)| {
            if b.len() > MAX_ANSAGE_AUDIO {
                // Sonst bliebe ein Pi ohne Gerätestimme ohne jede Spur stumm.
                tracing::warn!(
                    bytes = b.len(),
                    "Ansage-Seite: Audio über dem Deckel, Eintrag ohne Audio"
                );
            }
            !b.is_empty() && b.len() <= MAX_ANSAGE_AUDIO
        });
        let mut g = self.innen();
        g.aufraeumen(now_ms);
        g.seq = g.seq.max(now_ms) + 1;
        let id = g.seq;
        let hall: String = entwurf
            .hall
            .trim()
            .chars()
            .take(MAX_HALLEN_LAENGE)
            .collect();
        let eintrag = AnsageEintrag {
            id,
            hall,
            art: entwurf.art,
            court_id: entwurf.court_id,
            match_id: entwurf.match_id,
            created_at_ms: now_ms,
            gong: entwurf.gong,
            lang: entwurf.lang,
            rate: entwurf.rate,
            segmente: entwurf.segmente,
            audio: audio.as_ref().map(|(mime, b)| AnsageAudio {
                mime: mime.clone(),
                bytes: b.len() as u32,
            }),
        };
        g.eintraege.push_back(Eintrag {
            eintrag,
            audio: audio.map(|(_, b)| Arc::new(b)),
        });
        while g.eintraege.len() > MAX_ANSAGE_EINTRAEGE {
            g.eintraege.pop_front();
        }
        // Über dem Gesamtdeckel verlieren die ältesten Einträge nur ihr Audio,
        // nicht den Eintrag: Eine Seite, die beim Schub noch hinterherspielt,
        // spricht dann den Text, statt die Ansage ganz zu verlieren.
        let mut i = 0;
        while g.audio_gesamt() > MAX_ANSAGE_AUDIO_GESAMT && i + 1 < g.eintraege.len() {
            let e = &mut g.eintraege[i];
            e.audio = None;
            e.eintrag.audio = None;
            i += 1;
        }
        id
    }

    /// Einträge für eine Seite: neuer als `since`, für ihre Halle, nicht
    /// verfallen — und **nur**, wenn ein Eintrag mit Spiel-Bezug noch zur
    /// Belegung passt. Steht inzwischen ein anderes Spiel auf dem Feld, wäre
    /// die Ansage falsch; lieber gar keine.
    pub fn eintraege_fuer(
        &self,
        hall: &str,
        since: u64,
        belegung: &[AnsageBelegung],
        now_ms: u64,
    ) -> Vec<AnsageEintrag> {
        let mut g = self.innen();
        g.aufraeumen(now_ms);
        g.eintraege
            .iter()
            .map(|e| &e.eintrag)
            .filter(|e| e.id > since)
            .filter(|e| halle_passt(&e.hall, hall))
            .filter(|e| belegung_passt(e, belegung))
            .cloned()
            .collect()
    }

    /// Das Audio eines Eintrags, solange er nicht verfallen ist.
    pub fn audio(&self, id: u64, now_ms: u64) -> Option<(String, Arc<Vec<u8>>)> {
        let mut g = self.innen();
        g.aufraeumen(now_ms);
        g.eintraege
            .iter()
            .find(|e| e.eintrag.id == id)
            .and_then(|e| {
                let mime = e.eintrag.audio.as_ref()?.mime.clone();
                Some((mime, e.audio.clone()?))
            })
    }
}

/// Darf dieser Eintrag bei dieser Belegung noch erklingen? Ein Eintrag ohne
/// Spiel (Freitext, Besetzung, Vorbereitungs-Aufruf) immer; einer mit Spiel
/// nur, wenn genau dieses Spiel auf genau diesem Feld steht.
pub fn belegung_passt(e: &AnsageEintrag, belegung: &[AnsageBelegung]) -> bool {
    match (e.court_id, e.match_id) {
        (Some(court), Some(spiel)) => belegung
            .iter()
            .any(|b| b.court_id == court && b.match_id == spiel),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tablet::state::AnnounceJobKind;

    fn entwurf(hall: &str) -> AnsageEntwurf {
        AnsageEntwurf {
            hall: hall.into(),
            art: AnsageArt::Freetext,
            court_id: None,
            match_id: None,
            gong: Some(AnsageGong::Info),
            lang: "de".into(),
            rate: 0.8,
            segmente: vec!["Hallo".into()],
        }
    }

    fn job(id: u64, hall: &str) -> AnnounceJob {
        AnnounceJob {
            id,
            hall: hall.into(),
            created_at_ms: 0,
            kind: AnnounceJobKind::Officials { court_id: 1 },
        }
    }

    fn freitext(id: u64, hall: &str) -> FreetextItem {
        FreetextItem {
            id,
            hall: hall.into(),
            text: "Siegerehrung".into(),
        }
    }

    #[test]
    fn ein_eintrag_verfaellt_nach_einer_minute_samt_audio() {
        let s = AnsageStore::default();
        let id = s.ablegen(
            entwurf("Halle A"),
            Some(("audio/mpeg".into(), vec![1, 2, 3])),
            100_000,
        );
        assert_eq!(s.eintraege_fuer("Halle A", 0, &[], 159_000).len(), 1);
        assert!(s.audio(id, 159_000).is_some());
        assert!(s.eintraege_fuer("Halle A", 0, &[], 160_000).is_empty());
        assert!(s.audio(id, 160_000).is_none(), "Audio nach 60 s weg");
    }

    #[test]
    fn der_aelteste_eintrag_fliegt_beim_deckel() {
        let s = AnsageStore::default();
        let erste = s.ablegen(entwurf(""), None, 1_000);
        for _ in 0..MAX_ANSAGE_EINTRAEGE {
            s.ablegen(entwurf(""), None, 1_000);
        }
        let alle = s.eintraege_fuer("", 0, &[], 1_000);
        assert_eq!(alle.len(), MAX_ANSAGE_EINTRAEGE);
        assert!(alle.iter().all(|e| e.id != erste));
    }

    #[test]
    fn zu_grosses_audio_geht_ohne_audio_raus_und_der_gesamtdeckel_haelt() {
        let s = AnsageStore::default();
        let id = s.ablegen(
            entwurf(""),
            Some(("audio/wav".into(), vec![0; MAX_ANSAGE_AUDIO + 1])),
            1_000,
        );
        let e = &s.eintraege_fuer("", 0, &[], 1_000)[0];
        assert_eq!(e.id, id);
        assert!(e.audio.is_none());
        assert!(!e.segmente.is_empty(), "Segmente bleiben als Rückfall");

        let schub = MAX_ANSAGE_AUDIO_GESAMT / MAX_ANSAGE_AUDIO + 3;
        for _ in 0..schub {
            s.ablegen(
                entwurf(""),
                Some(("audio/wav".into(), vec![0; MAX_ANSAGE_AUDIO])),
                1_000,
            );
        }
        assert!(s.innen().audio_gesamt() <= MAX_ANSAGE_AUDIO_GESAMT);
        // Kein Eintrag geht verloren — die ältesten verlieren nur ihr Audio.
        let alle = s.eintraege_fuer("", 0, &[], 1_000);
        assert_eq!(alle.len(), schub + 1);
        assert!(alle[1].audio.is_none(), "ältester Schub-Eintrag ohne Audio");
        assert!(alle.last().unwrap().audio.is_some(), "neuester behält es");
    }

    #[test]
    fn eine_seite_hoert_nur_ihre_halle_und_die_an_alle() {
        let s = AnsageStore::default();
        s.ablegen(entwurf("Halle A"), None, 1_000);
        s.ablegen(entwurf("Halle B"), None, 1_000);
        s.ablegen(entwurf(""), None, 1_000);
        let a = s.eintraege_fuer("halle a", 0, &[], 1_000);
        assert_eq!(a.len(), 2, "eigene Halle (andere Schreibweise) + alle");
        assert!(a.iter().all(|e| e.hall != "Halle B"));
        // `since` filtert, was die Seite schon kennt.
        let letzte = a.last().unwrap().id;
        assert!(s.eintraege_fuer("Halle A", letzte, &[], 1_000).is_empty());
    }

    #[test]
    fn ein_eintrag_fuer_ein_anderes_spiel_auf_dem_feld_wird_nicht_ausgeliefert() {
        let s = AnsageStore::default();
        let mut e = entwurf("Halle A");
        e.court_id = Some(3);
        e.match_id = Some(42);
        s.ablegen(e, None, 1_000);
        let passt = [AnsageBelegung {
            court_id: 3,
            match_id: 42,
        }];
        let anders = [AnsageBelegung {
            court_id: 3,
            match_id: 43,
        }];
        assert_eq!(s.eintraege_fuer("Halle A", 0, &passt, 1_000).len(), 1);
        assert!(s.eintraege_fuer("Halle A", 0, &anders, 1_000).is_empty());
        assert!(s.eintraege_fuer("Halle A", 0, &[], 1_000).is_empty());
    }

    #[test]
    fn die_erste_beanspruchung_ist_bestandsaufnahme() {
        let s = AnsageStore::default();
        s.seite_hoert("Halle A", 1_000);
        let erst = s.beanspruche(vec![job(1, "Halle A")], vec![freitext(1, "")], 1_000);
        assert_eq!(erst, OffeneQuellen::default());
        let dann = s.beanspruche(
            vec![job(1, "Halle A"), job(2, "Halle A")],
            vec![freitext(1, ""), freitext(2, "")],
            1_000,
        );
        assert_eq!(dann.jobs.iter().map(|j| j.id).collect::<Vec<_>>(), [2]);
        assert_eq!(dann.freitexte.iter().map(|f| f.id).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn eine_quelle_geht_genau_einmal_raus_auch_bei_zwei_abrufen_hintereinander() {
        let s = AnsageStore::default();
        s.seite_hoert("Halle A", 1_000);
        s.beanspruche(vec![], vec![], 1_000);
        let quellen = || (vec![job(5, "Halle A")], vec![freitext(5, "Halle A")]);
        let (j, f) = quellen();
        let erster = s.beanspruche(j, f, 1_000);
        let (j, f) = quellen();
        let zweiter = s.beanspruche(j, f, 1_000);
        assert_eq!(erster.jobs.len() + erster.freitexte.len(), 2);
        assert_eq!(zweiter, OffeneQuellen::default());
    }

    #[test]
    fn ohne_zuhoerer_wird_nichts_vergeben_und_nichts_nachgeholt() {
        let s = AnsageStore::default();
        s.seite_hoert("Halle A", 1_000);
        s.beanspruche(vec![], vec![], 1_000);
        // Auftrag für Halle B: dort hört niemand → nicht vertonen.
        let r = s.beanspruche(vec![job(1, "Halle B")], vec![], 1_000);
        assert!(r.jobs.is_empty());
        // Kommt später eine Seite in Halle B dazu, wird er nicht nachgeholt.
        s.seite_hoert("Halle B", 2_000);
        let r = s.beanspruche(vec![job(1, "Halle B")], vec![], 2_000);
        assert!(r.jobs.is_empty());
    }

    #[test]
    fn ein_freitext_ohne_halle_braucht_irgendeine_seite() {
        let s = AnsageStore::default();
        s.beanspruche(vec![], vec![], 1_000);
        assert!(s
            .beanspruche(vec![], vec![freitext(1, "")], 1_000)
            .freitexte
            .is_empty());
        s.seite_hoert("Halle B", 1_000);
        assert_eq!(
            s.beanspruche(vec![], vec![freitext(2, "")], 1_000)
                .freitexte
                .len(),
            1
        );
    }

    #[test]
    fn zuhoerer_verfallen_und_die_liste_ist_gedeckelt() {
        let s = AnsageStore::default();
        s.seite_hoert("Halle Ü", 1_000);
        assert!(s.hat_seite("HALLE ü", 1_000), "Umlaute ohne Schreibweise");
        assert!(s.hat_seite("", 1_000), "leere Halle = irgendeine Seite");
        assert!(!s.hat_seite("Halle B", 1_000));
        assert!(!s.hat_seite("Halle Ü", 1_000 + SEITEN_FRIST_MS));
        for i in 0..200 {
            s.seite_hoert(&format!("Halle {i}"), 50_000);
        }
        s.seite_hoert(&"X".repeat(4096), 50_000);
        let hallen = s.seiten_hallen(50_000);
        assert!(hallen.len() <= MAX_SEITEN_HALLEN);
        assert!(hallen
            .iter()
            .all(|h| h.chars().count() <= MAX_HALLEN_LAENGE));
    }

    #[test]
    fn der_vertoner_gilt_zehn_sekunden_nach_dem_letzten_lebenszeichen_als_weg() {
        let s = AnsageStore::default();
        assert!(!s.vertoner_aktiv(1_000), "nie gesehen = nicht aktiv");
        s.vertoner_lebt(1_000);
        assert!(s.vertoner_aktiv(1_000 + VERTONER_FRIST_MS - 1));
        assert!(!s.vertoner_aktiv(1_000 + VERTONER_FRIST_MS));
    }

    #[test]
    fn die_nummer_steigt_auch_ueber_einen_neustart() {
        let vorher = AnsageStore::default();
        let alt = vorher.ablegen(entwurf(""), None, 5_000);
        let nachher = AnsageStore::default();
        let neu = nachher.ablegen(entwurf(""), None, 6_000);
        assert!(neu > alt);
    }
}
