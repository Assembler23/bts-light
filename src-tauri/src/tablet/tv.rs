//! Fernseher der Court-Monitore aus der Ferne an- und ausschalten.
//!
//! Der Pi hängt per HDMI am Fernseher und kann ihn über CEC schalten
//! (`bts-cec.sh`, eingerichtet von `pi/setup-monitor.sh`). Der Browser auf
//! dem Pi kann das nicht — deshalb fragt das CEC-Skript selbst alle paar
//! Sekunden beim Turnier-PC nach (`GET /pi-tv`), meldet dabei den Zustand
//! des Fernsehers und bekommt den zuletzt gegebenen Schaltbefehl zurück.
//!
//! Ein Befehl ist ein **Soll mit Marke**: Der Pi führt ihn genau einmal aus,
//! wenn sich die Marke ändert. Wer den Fernseher danach mit der
//! Fernbedienung umschaltet, wird nicht zurückgeholt. Meldet sich ein Pi
//! nach einem Neustart (`start=1`), verfällt sein Soll: „Strom an" heißt wie
//! bisher „Fernseher an", auch wenn vorher jemand „aus" gedrückt hat.
//!
//! Alles lebt nur im Arbeitsspeicher — nach einem Neustart von bts-light
//! gibt es keinen Befehl, die Fernseher bleiben, wie sie sind.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

/// So lange nach der letzten Meldung gilt ein Pi als „schaltbar". Der Pi
/// fragt alle 5 s; vier verpasste Abfragen darf er sich leisten.
pub const TV_FRISCH_MS: u64 = 25_000;
/// Nach so langer Funkstille wird der Eintrag vergessen.
const TV_VERGESSEN_MS: u64 = 10 * 60 * 1000;
/// Deckel gegen unbegrenztes Wachstum — die Route ist im Hallennetz offen.
const MAX_GERAETE: usize = 256;

/// Was die Turnierleitung für einen Fernseher verlangt hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TvSoll {
    An,
    Aus,
}

/// Was der Pi über seinen Fernseher meldet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TvZustand {
    An,
    Standby,
    /// Der Fernseher antwortet nicht (ohne Strom, CEC am TV aus).
    Unbekannt,
}

impl TvZustand {
    /// Liest den Wert, den `cec-ctl` als `pwr-state` nennt. Alles Fremde
    /// (auch ein leerer Wert) heißt „unbekannt".
    pub fn aus_meldung(wert: &str) -> Self {
        match wert {
            "on" | "to-on" => Self::An,
            "standby" | "to-standby" => Self::Standby,
            _ => Self::Unbekannt,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Eintrag {
    zustand: TvZustand,
    gemeldet_ms: u64,
    /// Soll samt Marke; `None` = kein Befehl offen.
    soll: Option<(TvSoll, u64)>,
}

/// Ein Fernseher, wie ihn die Verwaltungsseite zeigt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TvSicht {
    pub zustand: TvZustand,
    /// Der zuletzt gegebene Befehl, solange der Pi ihn noch nicht durch
    /// einen Neustart verworfen hat.
    pub soll: Option<TvSoll>,
}

/// Zustand und Schaltbefehle aller Fernseher, je Geräte-Kennung des Pi.
#[derive(Default)]
pub struct TvStore {
    inner: Mutex<HashMap<String, Eintrag>>,
}

impl TvStore {
    /// Der Pi meldet sich: Zustand vermerken, offenen Befehl zurückgeben.
    /// `start` = erste Abfrage nach dem Hochfahren → der alte Befehl
    /// verfällt (siehe Modul-Kommentar).
    pub fn melden(
        &self,
        geraet: &str,
        zustand: TvZustand,
        start: bool,
        jetzt_ms: u64,
    ) -> Option<(TvSoll, u64)> {
        let mut inner = self.inner.lock().expect("TV-Mutex nicht vergiftet");
        inner.retain(|_, e| jetzt_ms.saturating_sub(e.gemeldet_ms) <= TV_VERGESSEN_MS);
        if !inner.contains_key(geraet) && inner.len() >= MAX_GERAETE {
            // Voll: den am längsten stummen Eintrag räumen statt den Neuen
            // abzuweisen — sonst sperrte ein Gerät, das sich unter vielen
            // erfundenen Kennungen meldet, die echten Pis aus.
            let aeltester = inner
                .iter()
                .min_by_key(|(_, e)| e.gemeldet_ms)
                .map(|(id, _)| id.clone());
            if let Some(id) = aeltester {
                inner.remove(&id);
            }
        }
        let eintrag = inner.entry(geraet.to_string()).or_insert(Eintrag {
            zustand,
            gemeldet_ms: jetzt_ms,
            soll: None,
        });
        eintrag.zustand = zustand;
        eintrag.gemeldet_ms = jetzt_ms;
        if start {
            eintrag.soll = None;
        }
        eintrag.soll
    }

    /// Gibt den genannten Geräten einen Schaltbefehl. Nur Geräte, die sich
    /// gerade melden, bekommen ihn — ein Pi ohne Strom soll nicht Stunden
    /// später einen alten Befehl ausführen. Liefert, wie viele es waren.
    pub fn setzen(&self, geraete: &[String], soll: TvSoll, jetzt_ms: u64) -> usize {
        let mut inner = self.inner.lock().expect("TV-Mutex nicht vergiftet");
        let mut anzahl = 0;
        for id in geraete {
            let Some(e) = inner.get_mut(id) else { continue };
            if jetzt_ms.saturating_sub(e.gemeldet_ms) > TV_FRISCH_MS {
                continue;
            }
            // Marke = Uhrzeit, mindestens eins über der alten: zwei Befehle
            // in derselben Millisekunde bleiben unterscheidbar, und nach
            // einem Neustart von bts-light kehrt keine alte Marke wieder.
            let marke = jetzt_ms.max(e.soll.map(|(_, m)| m + 1).unwrap_or(0));
            e.soll = Some((soll, marke));
            anzahl += 1;
        }
        anzahl
    }

    /// Alle Fernseher, deren Pi sich gerade meldet.
    pub fn sicht(&self, jetzt_ms: u64) -> HashMap<String, TvSicht> {
        self.inner
            .lock()
            .expect("TV-Mutex nicht vergiftet")
            .iter()
            .filter(|(_, e)| jetzt_ms.saturating_sub(e.gemeldet_ms) <= TV_FRISCH_MS)
            .map(|(id, e)| {
                (
                    id.clone(),
                    TvSicht {
                        zustand: e.zustand,
                        soll: e.soll.map(|(s, _)| s),
                    },
                )
            })
            .collect()
    }
}

/// Die Antwortzeile für den Pi: `an <Marke>`, `aus <Marke>` oder `-`.
/// Bewusst reiner Text — das CEC-Skript ist eine Shell ohne JSON-Werkzeug.
pub fn antwort_zeile(soll: Option<(TvSoll, u64)>) -> String {
    match soll {
        Some((TvSoll::An, marke)) => format!("an {marke}\n"),
        Some((TvSoll::Aus, marke)) => format!("aus {marke}\n"),
        None => "-\n".to_string(),
    }
}

/// Kürzt die Geräte-Kennung auf harmlose Zeichen (wie beim Pi-Log).
pub fn geraet_bereinigt(roh: &str) -> String {
    roh.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(namen: &[&str]) -> Vec<String> {
        namen.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ohne_befehl_bekommt_der_pi_einen_strich() {
        let store = TvStore::default();
        let soll = store.melden("pi-1", TvZustand::An, true, 1_000);
        assert_eq!(soll, None);
        assert_eq!(antwort_zeile(soll), "-\n");
    }

    #[test]
    fn ein_befehl_reist_mit_marke_und_bleibt_bis_zum_naechsten() {
        let store = TvStore::default();
        store.melden("pi-1", TvZustand::An, true, 1_000);
        assert_eq!(store.setzen(&ids(&["pi-1"]), TvSoll::Aus, 2_000), 1);
        let erste = store.melden("pi-1", TvZustand::An, false, 3_000);
        assert_eq!(erste, Some((TvSoll::Aus, 2_000)));
        // Gleiche Marke bei der nächsten Abfrage → der Pi schaltet nicht noch einmal.
        assert_eq!(
            store.melden("pi-1", TvZustand::Standby, false, 8_000),
            erste
        );
        assert_eq!(antwort_zeile(erste), "aus 2000\n");
    }

    #[test]
    fn zwei_befehle_in_derselben_millisekunde_haben_verschiedene_marken() {
        let store = TvStore::default();
        store.melden("pi-1", TvZustand::An, true, 1_000);
        store.setzen(&ids(&["pi-1"]), TvSoll::Aus, 2_000);
        store.setzen(&ids(&["pi-1"]), TvSoll::An, 2_000);
        assert_eq!(
            store.melden("pi-1", TvZustand::An, false, 2_500),
            Some((TvSoll::An, 2_001))
        );
    }

    #[test]
    fn ein_neu_gestarteter_pi_verwirft_den_alten_befehl() {
        let store = TvStore::default();
        store.melden("pi-1", TvZustand::An, true, 1_000);
        store.setzen(&ids(&["pi-1"]), TvSoll::Aus, 2_000);
        // Steckdosenleiste aus und wieder an: Der Fernseher soll angehen.
        assert_eq!(store.melden("pi-1", TvZustand::Standby, true, 9_000), None);
        assert_eq!(store.sicht(9_000)["pi-1"].soll, None);
    }

    #[test]
    fn nur_geraete_die_sich_gerade_melden_bekommen_den_befehl() {
        let store = TvStore::default();
        store.melden("pi-alt", TvZustand::An, true, 1_000);
        store.melden("pi-neu", TvZustand::An, true, 1_000 + TV_FRISCH_MS);
        let jetzt = 1_001 + TV_FRISCH_MS;
        let n = store.setzen(&ids(&["pi-alt", "pi-neu", "pi-fremd"]), TvSoll::Aus, jetzt);
        assert_eq!(n, 1);
        let sicht = store.sicht(jetzt);
        assert_eq!(sicht.len(), 1);
        assert_eq!(sicht["pi-neu"].soll, Some(TvSoll::Aus));
    }

    #[test]
    fn die_meldung_des_pi_wird_auf_drei_zustaende_gelesen() {
        assert_eq!(TvZustand::aus_meldung("on"), TvZustand::An);
        assert_eq!(TvZustand::aus_meldung("to-on"), TvZustand::An);
        assert_eq!(TvZustand::aus_meldung("standby"), TvZustand::Standby);
        assert_eq!(TvZustand::aus_meldung("to-standby"), TvZustand::Standby);
        assert_eq!(TvZustand::aus_meldung(""), TvZustand::Unbekannt);
        assert_eq!(TvZustand::aus_meldung("quatsch"), TvZustand::Unbekannt);
    }

    #[test]
    fn der_speicher_waechst_nicht_ueber_den_deckel() {
        let store = TvStore::default();
        for n in 0..MAX_GERAETE + 5 {
            store.melden(&format!("pi-{n}"), TvZustand::An, true, 1_000 + n as u64);
        }
        let sicht = store.sicht(2_000);
        assert_eq!(sicht.len(), MAX_GERAETE);
        // Geräumt wird der älteste Eintrag, der neueste kommt immer hinein.
        assert!(!sicht.contains_key("pi-0"));
        assert!(sicht.contains_key(&format!("pi-{}", MAX_GERAETE + 4)));
    }

    #[test]
    fn lange_stumme_geraete_werden_vergessen() {
        let store = TvStore::default();
        store.melden("pi-1", TvZustand::An, true, 1_000);
        store.melden("pi-2", TvZustand::An, true, 2_000 + TV_VERGESSEN_MS);
        assert_eq!(
            store.inner.lock().unwrap().len(),
            1,
            "pi-1 ist seit über zehn Minuten stumm"
        );
    }

    #[test]
    fn die_kennung_wird_auf_harmlose_zeichen_gekuerzt() {
        assert_eq!(geraet_bereinigt("pi-00ab/../x y"), "pi-00abxy");
        assert_eq!(geraet_bereinigt(&"a".repeat(200)).len(), 64);
    }

    #[test]
    fn die_sicht_serialisiert_in_kleinbuchstaben() {
        let sicht = TvSicht {
            zustand: TvZustand::Standby,
            soll: Some(TvSoll::Aus),
        };
        assert_eq!(
            serde_json::to_string(&sicht).unwrap(),
            r#"{"zustand":"standby","soll":"aus"}"#
        );
    }
}
