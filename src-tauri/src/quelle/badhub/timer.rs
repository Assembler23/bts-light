//! Timer des Turniers („Spiel nach Zeit“, Kern 0.24.0) für die Feldmonitore.
//!
//! Der Kern hält nur Zeitpunkte (Start, Pausen) als Wandzeit. Hier werden sie
//! in eine Form gebracht, die sich zwischen zwei Abrufen **nicht ändert**
//! (sonst bräche die Zustands-Marke der Monitore): läuft der Timer, das Ende
//! als Unix-Millisekunden; steht er, die eingefrorene Restzeit. Herunter zählt
//! die Anzeige selbst gegen die Server-Uhr (`serverNowMs`).
//!
//! Rechenregel wie in badhub (`Badhub\Turnierorga\Timer::stand`):
//! Rest = Minuten × 60 − (jetzt − Start − Summe der Pausen), nie negativ.

use relay_proto::TurnierTimerView;
use turnierkern::TimerStand;

/// Kern-Zeit `JJJJ-MM-TTTHH:MM:SS` (lokale Wandzeit) → Unix-Millisekunden.
pub fn wandzeit_ms(zeit: &str) -> Option<i64> {
    use chrono::TimeZone;
    let naiv = chrono::NaiveDateTime::parse_from_str(zeit, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(zeit, "%Y-%m-%d %H:%M:%S"))
        .ok()?;
    match chrono::Local.from_local_datetime(&naiv) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => {
            Some(t.timestamp_millis())
        }
        chrono::LocalResult::None => None,
    }
}

/// Kern-Stand → Anzeige. `versatz_ms` gleicht eine abweichende Uhr des
/// Absenders aus (Uhr dieses PCs minus Uhr, die den Befehl gestempelt hat);
/// er wirkt nur auf den laufenden Timer. `None`, wenn eine Zeit unlesbar ist.
pub fn ansicht(t: &TimerStand, versatz_ms: i64) -> Option<TurnierTimerView> {
    let start = wandzeit_ms(&t.gestartet)?;
    let dauer = i64::from(t.minuten) * 60_000;
    let mut pause = 0i64;
    let mut offen: Option<i64> = None;
    for p in &t.pausen {
        let von = wandzeit_ms(&p.von)?;
        match &p.bis {
            Some(bis) => pause += (wandzeit_ms(bis)? - von).max(0),
            None => offen = Some(von),
        }
    }
    let titel = t
        .titel
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some(match offen {
        // Angehalten: gelaufen ist die Zeit bis zum Beginn der offenen Pause.
        Some(von) => {
            let gelaufen = (von - start - pause).max(0);
            TurnierTimerView {
                minutes: t.minuten,
                title: titel,
                paused: true,
                ends_at_ms: None,
                rest_s: Some(((dauer - gelaufen).max(0) / 1000) as u32),
            }
        }
        None => TurnierTimerView {
            minutes: t.minuten,
            title: titel,
            paused: false,
            ends_at_ms: Some((start + dauer + pause + versatz_ms).max(0) as u64),
            rest_s: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use turnierkern::TimerPause;

    fn stand(pausen: Vec<(&str, Option<&str>)>) -> TimerStand {
        TimerStand {
            minuten: 10,
            titel: Some("Runde 3".into()),
            gestartet: "2026-11-23T10:00:00".into(),
            pausen: pausen
                .into_iter()
                .map(|(von, bis)| TimerPause {
                    von: von.into(),
                    bis: bis.map(Into::into),
                })
                .collect(),
        }
    }

    #[test]
    fn laufender_timer_traegt_das_ende() {
        let start = wandzeit_ms("2026-11-23T10:00:00").unwrap();
        let a = ansicht(&stand(vec![]), 0).unwrap();
        assert!(!a.paused);
        assert_eq!(a.ends_at_ms, Some((start + 600_000) as u64));
        assert_eq!(a.rest_s, None);
        assert_eq!(a.title.as_deref(), Some("Runde 3"));
        assert_eq!(a.minutes, 10);
    }

    #[test]
    fn abgeschlossene_pausen_schieben_das_ende() {
        let start = wandzeit_ms("2026-11-23T10:00:00").unwrap();
        let a = ansicht(
            &stand(vec![
                ("2026-11-23T10:02:00", Some("2026-11-23T10:03:30")),
                ("2026-11-23T10:05:00", Some("2026-11-23T10:05:10")),
            ]),
            0,
        )
        .unwrap();
        assert_eq!(a.ends_at_ms, Some((start + 600_000 + 100_000) as u64));
    }

    #[test]
    fn angehalten_friert_die_restzeit_ein() {
        let a = ansicht(
            &stand(vec![
                ("2026-11-23T10:02:00", Some("2026-11-23T10:03:00")),
                ("2026-11-23T10:05:00", None),
            ]),
            5_000,
        )
        .unwrap();
        assert!(a.paused);
        // 5 min seit Start, davon 1 min Pause → 4 min gelaufen, 6 min Rest.
        assert_eq!(a.rest_s, Some(360));
        assert_eq!(a.ends_at_ms, None);
    }

    #[test]
    fn rest_wird_nie_negativ() {
        let a = ansicht(&stand(vec![("2026-11-23T11:00:00", None)]), 0).unwrap();
        assert_eq!(a.rest_s, Some(0));
    }

    #[test]
    fn versatz_gleicht_die_uhr_des_absenders_aus() {
        let ohne = ansicht(&stand(vec![]), 0).unwrap().ends_at_ms.unwrap();
        let mit = ansicht(&stand(vec![]), -90_000)
            .unwrap()
            .ends_at_ms
            .unwrap();
        assert_eq!(ohne - mit, 90_000);
    }

    #[test]
    fn unlesbare_zeit_ergibt_keine_anzeige() {
        let mut t = stand(vec![]);
        t.gestartet = "gestern".into();
        assert!(ansicht(&t, 0).is_none());
        let mut t = stand(vec![]);
        t.titel = Some("   ".into());
        assert_eq!(ansicht(&t, 0).unwrap().title, None);
    }

    #[test]
    fn ohne_timer_bleibt_der_zustand_wie_bisher() {
        // Harte Bedingung „BTP-Turniere unverändert“: das Feld fehlt im JSON.
        let s = crate::tablet::monitor::unassigned_monitor_state("abc");
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("turnierTimer").is_none());
    }
}
