//! Turnierquelle: woher bts-light den Turnierstand liest und wohin es
//! schreibt.
//!
//! Zwei Quellen: BTP und — seit Paket L2 — ein in badhub organisiertes
//! Turnier, dessen Stand der Turnierkern lokal faltet (badhub
//! `docs/features/bts_light_steuerung.md`, ADR-0084). **BTP-Turniere laufen
//! dabei unverändert** — dieses Modul ist die eine Stelle, an der sich die
//! Wege trennen; die Konsumenten fragen nie selbst nach der Quelle.

pub mod badhub;
pub mod btp;

use crate::btp::client;
use crate::btp::model::BtpSnapshot;
use crate::btp::proto;
use crate::config::{AppConfig, QuelleArt};
use crate::tablet::state::TabletState;

/// Liest den aktuellen Turnierstand aus der eingestellten Quelle.
///
/// Für BTP genau der bisherige Abruf (`client::fetch_snapshot` mit Host,
/// Port und Passwort aus `config.btp`), samt Fehlertext.
pub async fn stand(config: &AppConfig) -> Result<BtpSnapshot, String> {
    match config.quelle {
        QuelleArt::Btp => client::fetch_snapshot(
            &config.btp.host,
            config.btp.port,
            config.btp.password.as_deref(),
        )
        .await
        .map_err(|e| e.to_string()),
        QuelleArt::Badhub if config.slave_mode => badhub::slave::stand_vom_master(config).await,
        QuelleArt::Badhub => badhub::stand(config),
    }
}

/// Schlüssel der lokalen Stores: bei BTP `None` (es gilt wie bisher
/// `tournament_name`), bei badhub die Turnier-UUID (Paket L3).
pub fn turnier_schluessel(config: &AppConfig) -> Option<String> {
    match config.quelle {
        QuelleArt::Btp => None,
        QuelleArt::Badhub if config.slave_mode => None,
        QuelleArt::Badhub => badhub::turnier_schluessel(),
    }
}

/// Darf gerade geschrieben werden? Bei BTP immer (wie bisher — ob BTP
/// erreichbar ist, zeigt erst der Schreibversuch); bei badhub nur mit
/// gültiger Lease. Damit schweigen Auto-Vergabe und Schiedsrichter-Abgleich
/// ohne Lease, statt jeden Takt eine Warnung zu schreiben.
pub fn schreiben_moeglich(config: &AppConfig) -> bool {
    match config.quelle {
        QuelleArt::Btp => true,
        QuelleArt::Badhub => !config.slave_mode && badhub::schreiben_moeglich(),
    }
}

/// Antwort an ein TL-Gerät, wenn ein Schreibweg scheitert: bei BTP wortgleich
/// wie bisher (`BtpError`, „BTP hat abgelehnt: …“), bei badhub die Ablehnung
/// selbst — die Halle soll nicht „BTP“ lesen, wo es kein BTP gibt.
pub fn schreib_ablehnung(config: &AppConfig, fehler: &str) -> relay_proto::TlResponse {
    match config.quelle {
        QuelleArt::Btp => relay_proto::TlResponse::err(
            relay_proto::TlErrorCode::BtpError,
            format!("BTP hat abgelehnt: {fehler}"),
        ),
        QuelleArt::Badhub => {
            relay_proto::TlResponse::err(relay_proto::TlErrorCode::KernAbgelehnt, fehler)
        }
    }
}

/// Was beim Start der Übertragung je Quelle läuft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuellenStart {
    /// Abgleich mit badhub (Ereignisse, Paket) — nur bei Quelle badhub.
    pub kern_abgleich: bool,
    /// BTP-Nebendienste (TP-Takt, TP-Datei-Suche) — nur bei BTP.
    pub btp_dienste: bool,
}

/// Der eine Abzweig beim Start (`commands::start_sync_kern`): Bei BTP startet
/// nichts Neues (kein Kern, kein Abgleich, kein Zustand), bei badhub wird
/// das gekoppelte Turnier von der Platte geladen. `daten` = App-Datenordner.
pub fn starten(config: &AppConfig, daten: Option<&std::path::Path>) -> QuellenStart {
    match config.quelle {
        QuelleArt::Btp => QuellenStart {
            kern_abgleich: false,
            btp_dienste: true,
        },
        QuelleArt::Badhub => {
            // Auch der Slave lädt sein Paket (Namen, Felder); einen eigenen
            // Abgleich, eine Lease oder eine Outbox hat er nicht — den Stand
            // liest er vom Master (`badhub::slave`).
            let geladen = daten.is_some_and(|d| badhub::starten(config, d));
            QuellenStart {
                kern_abgleich: !config.slave_mode && geladen,
                btp_dienste: false,
            }
        }
    }
}

/// Laufen die BTP-eigenen Nebendienste (TP-Datei-Upload, TP-Takt,
/// TP-Datei-Suche)? Nur bei Quelle BTP — für badhub-Turniere gibt es keine
/// TP-Datei (badhub-Spec „Turnierseite und Liveticker“).
pub fn btp_dienste(config: &AppConfig) -> bool {
    config.quelle == QuelleArt::Btp
}

/// Darf ein gescheiterter Ergebnis-Write in die BTP-Nachschub-Queue? Nur bei
/// BTP: Bei badhub scheitert ein Write nie am Netz, nur am Kern oder an der
/// fehlenden Lease — ein Nachschub wiederholte dieselbe Ablehnung.
pub fn nachschub_moeglich(config: &AppConfig) -> bool {
    config.quelle == QuelleArt::Btp
}

// ── Schreibwege ─────────────────────────────────────────────────────────────
//
// Dünne Weichen statt des früheren Re-Exports: bei BTP derselbe Aufruf wie
// vorher (gleiche Argumente, gleiche Reihenfolge, kein zusätzliches Warten),
// bei badhub kein BTP-Kontakt und keine Nebenwirkung auf BTP-Merker
// (`eigene_writes`, Schreib-Gate, Nachschub), sondern Kernbefehle unter der
// Lease (Paket L3, `badhub::schreiben`). Am Slave schreibt niemand.

/// Fehler eines Schreibwegs am badhub-Slave.
const SLAVE_SCHREIBT_NICHT: &str =
    "Dieser Rechner ist ein Ansage-Slave — Eingaben nur am Turnier-PC.";

fn badhub_weiche<T>(
    config: &AppConfig,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if config.slave_mode {
        return Err(SLAVE_SCHREIBT_NICHT.into());
    }
    f()
}

pub(crate) async fn write_result_to_btp(
    config: &AppConfig,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    match config.quelle {
        QuelleArt::Btp => btp::write_result_to_btp(config, update).await,
        QuelleArt::Badhub => badhub_weiche(config, || badhub::ergebnis(update)),
    }
}

pub(crate) async fn write_result_settled(
    config: &AppConfig,
    tablet: &TabletState,
    update: &proto::MatchUpdate,
) -> Result<(), String> {
    match config.quelle {
        QuelleArt::Btp => btp::write_result_settled(config, tablet, update).await,
        QuelleArt::Badhub => badhub_weiche(config, || badhub::ergebnis(update)),
    }
}

pub(crate) async fn write_courts_to_btp(
    config: &AppConfig,
    courts: &[proto::CourtAssignment],
    match_courts: &[proto::MatchCourt],
) -> Result<(), String> {
    match config.quelle {
        QuelleArt::Btp => btp::write_courts_to_btp(config, courts, match_courts).await,
        QuelleArt::Badhub => badhub_weiche(config, || badhub::feld_vergabe(courts)),
    }
}

pub(crate) async fn write_officials_to_btp(
    config: &AppConfig,
    entries: &[proto::MatchCourt],
    zyklus: &BtpSnapshot,
    tablet: &TabletState,
) -> Result<Vec<proto::MatchCourt>, String> {
    match config.quelle {
        QuelleArt::Btp => btp::write_officials_to_btp(config, entries, zyklus, tablet).await,
        QuelleArt::Badhub => badhub_weiche(config, || badhub::offizielle(entries)),
    }
}

pub(crate) async fn write_highlight_to_btp(
    config: &AppConfig,
    entries: &[proto::HighlightEntry],
) -> Result<usize, String> {
    match config.quelle {
        QuelleArt::Btp => btp::write_highlight_to_btp(config, entries).await,
        // Zeilenfarbe gibt es nur in BTP; bei Kern-Turnieren gilt der lokale
        // Aufrufzustand — Erfolg ohne Kernbefehl.
        QuelleArt::Badhub => Ok(entries.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btp::proto::MatchUpdate;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Konfiguration mit badhub als Quelle und einer BTP-Adresse, an der ein
    /// Lauscher jede Verbindung zählt.
    async fn badhub_mit_lauscher() -> (AppConfig, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let zaehler = Arc::new(AtomicUsize::new(0));
        let z = zaehler.clone();
        tokio::spawn(async move {
            while let Ok((_s, _)) = listener.accept().await {
                z.fetch_add(1, Ordering::SeqCst);
            }
        });
        let config = AppConfig {
            quelle: QuelleArt::Badhub,
            btp: crate::config::BtpConfig {
                host: "127.0.0.1".into(),
                port,
                password: None,
            },
            ..AppConfig::default()
        };
        (config, zaehler)
    }

    fn update(id: i64) -> MatchUpdate {
        MatchUpdate {
            btp_match_id: id,
            draw_id: 1,
            planning_id: 1,
            sets: vec![(21, 10), (21, 10)],
            team1_won: true,
            duration_mins: 20,
            netto_mins: None,
            score_status: 0,
            free_court_id: Some(1),
            player_ids: vec![],
            end_ts_ms: Some(1),
            officials: None,
            shuttles: 0,
        }
    }

    fn leer() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: String::new(),
            rest_minutes: None,
            matches: vec![],
            courts: vec![],
            locations: vec![],
            court_infos: vec![],
            events: vec![],
            entries: vec![],
            officials: vec![],
        }
    }

    #[tokio::test]
    async fn badhub_schreibt_nie_nach_btp_und_legt_keinen_nachschub_an() {
        let (config, zaehler) = badhub_mit_lauscher().await;
        let tablet = TabletState::default();
        let dir = tempfile::tempdir().unwrap();
        tablet.set_btp_retry_path(dir.path().join("btp-retry.json"));

        // Ohne Lease (hier: ohne geladenes Turnier) lehnt der Kern-Weg ab —
        // ohne BTP zu berühren.
        assert!(write_result_to_btp(&config, &update(7)).await.is_err());
        assert!(write_result_settled(&config, &tablet, &update(7))
            .await
            .is_err());
        let _ = write_courts_to_btp(&config, &[], &[]).await;
        let _ = write_officials_to_btp(&config, &[], &leer(), &tablet).await;
        // Zeilenfarbe gibt es nur in BTP: lokal Erfolg ohne Kernbefehl.
        assert_eq!(write_highlight_to_btp(&config, &[]).await, Ok(0));

        // Keine Verbindung zu BTP, kein Nachschub-Eintrag, keine Datei.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(zaehler.load(Ordering::SeqCst), 0);
        assert!(!tablet.btp_retry_pending(7));
        assert!(!dir.path().join("btp-retry.json").exists());
        assert!(!nachschub_moeglich(&config));
        assert!(!btp_dienste(&config));
    }

    #[test]
    fn btp_bleibt_vorgabe_mit_nachschub_und_tp_diensten() {
        // Konfiguration ohne Quellenangabe → BTP, kein Kern-Task.
        let mut alt = serde_json::to_value(AppConfig::default()).unwrap();
        alt.as_object_mut().unwrap().remove("quelle");
        let config: AppConfig = serde_json::from_value(alt).unwrap();
        assert_eq!(config.quelle, QuelleArt::Btp);
        assert!(nachschub_moeglich(&config));
        assert!(btp_dienste(&config));
        let dir = tempfile::tempdir().unwrap();
        let start = starten(&config, Some(dir.path()));
        assert_eq!(
            start,
            QuellenStart {
                kern_abgleich: false,
                btp_dienste: true
            }
        );
        // Und es entsteht nichts unter `turniere/`.
        assert!(!dir.path().join("turniere").exists());
        // Paket L3: kein Kern-Schlüssel, Schreiben wie bisher immer erlaubt,
        // Ablehnungstext wortgleich wie vor L3.
        assert_eq!(turnier_schluessel(&config), None);
        assert!(schreiben_moeglich(&config));
        let r = schreib_ablehnung(&config, "Zeitüberschreitung");
        assert_eq!(r.code, Some(relay_proto::TlErrorCode::BtpError));
        assert_eq!(
            r.error.as_deref(),
            Some("BTP hat abgelehnt: Zeitüberschreitung")
        );
        assert_eq!(r.ablehnung, None);
    }

    #[tokio::test]
    async fn badhub_liest_kein_btp() {
        let (config, zaehler) = badhub_mit_lauscher().await;
        // Der Prozesszustand der Kern-Quelle kann von anderen Tests gesetzt
        // sein; hier zählt nur, dass BTP nicht angesprochen wird.
        let _ = stand(&config).await;
        let slave = AppConfig {
            slave_mode: true,
            ..config.clone()
        };
        // Der Slave liest vom Master (hier: nicht gekoppelt), nie BTP.
        let e = stand(&slave).await.unwrap_err();
        assert!(e.contains("Slave") || e.contains("koppeln"), "{e}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(zaehler.load(Ordering::SeqCst), 0);
    }
}
