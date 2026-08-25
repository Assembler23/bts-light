//! TLS für den LAN-Tablet-Server — selbstsigniertes, persistiertes Zertifikat
//! ([ADR 0047](../../../docs/adr/0047-lan-tls-konkretisierung.md), löst
//! [ADR 0005](../../../docs/adr/0005-lan-https-selbstsigniert.md) ein).
//!
//! Warum überhaupt: Die Battery-API der Tablets liefert nur im **Secure
//! Context**. LAN-Tablets auf `http://…:8088` haben keinen — die
//! Turnierleitung sieht dort keine Akkus. Nebenbei reisen Spielernamen und
//! Lizenznummern im Hallennetz dann verschlüsselt.
//!
//! Zwei Eigenheiten, die aus dem Feld kommen und nicht verhandelbar sind:
//!
//! 1. **`GUELTIG_AB` liegt bewusst weit in der Vergangenheit.** Die
//!    Court-Monitor-Pis haben keine Echtzeituhr; bootet einer ohne NTP, steht
//!    seine Uhr falsch und er verwirft ein frisch ausgestelltes Zertifikat
//!    **still** als „noch nicht gültig" (`pi/shared-startbrowser.sh:46-48`
//!    hält genau diesen Vorfall fest).
//! 2. **ALPN bietet ausschließlich `http/1.1` an.** Käme `h2` zustande, bräche
//!    der WebSocket-Upgrade und damit `/ws`, `/monitor-ws` und `/tl-ws`.
//!
//! Abgelegt wird **DER**, nicht PEM: `rustls` liest DER direkt, PEM bräuchte
//! zusätzlich einen Parser (`rustls-pemfile`) — eine Abhängigkeit, die wir uns
//! für nichts einhandeln würden.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, KeyPair};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

use super::mdns::MDNS_HOST;

/// Geduld für einen einzelnen TLS-Handschlag. Ein Gerät im schwachen
/// Hallen-WLAN darf ruhig lange brauchen; ein Client, der die Verbindung
/// öffnet und dann schweigt, soll aber nicht ewig einen Platz halten.
const HANDSHAKE_GEDULD: Duration = Duration::from_secs(15);

/// Wie viele fertig ausgehandelte Verbindungen zwischengepuffert werden,
/// bevor der Annahme-Task bremst.
const ANNAHME_PUFFER: usize = 32;

/// Dateinamen im App-Konfig-Verzeichnis (neben `config.json`).
pub const CERT_DATEI: &str = "tls-cert.der";
pub const KEY_DATEI: &str = "tls-key.der";

/// Beginn der Gültigkeit — absichtlich weit vor jeder denkbaren Turnieruhr,
/// damit RTC-lose Pis das Zertifikat nicht still verwerfen (siehe Modul-Doku).
const GUELTIG_AB: (i32, u8, u8) = (2020, 1, 1);

/// Ende der Gültigkeit. Lang, weil jede Neuausstellung **alle** weggeklickten
/// Browser-Ausnahmen auf **allen** Tablets entwertet.
const GUELTIG_BIS: (i32, u8, u8) = (2036, 1, 1);

/// Namen und Adressen, unter denen der Server erreichbar sein soll.
///
/// **Beide Wege müssen im Zertifikat stehen.** Die QR-Codes zeigen auf die
/// **IP**, weil Chrome unter Android `.local` vielerorts nicht auflöst (siehe
/// `server::lan_host_tls`). Der **mDNS-Name** ist dafür die stabile Identität,
/// die einen DHCP-Wechsel übersteht — wer eine dauerhafte Einrichtung will,
/// trägt ihn von Hand ein. Fehlte einer der beiden, wäre entweder das Feature
/// auf Android unerreichbar oder die dauerhafte Einrichtung unmöglich.
pub fn sans_sammeln() -> Vec<String> {
    let mut sans = vec![
        MDNS_HOST.to_string(),
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ];
    if let Ok(ip) = local_ip_address::local_ip() {
        let ip = ip.to_string();
        if !sans.contains(&ip) {
            sans.push(ip);
        }
    }
    sans
}

/// Baut die Zertifikatsparameter. Eigene Funktion, damit Gültigkeit und SANs
/// prüfbar sind, ohne ein Zertifikat zu erzeugen und wieder zu parsen.
pub fn params_bauen(sans: &[String]) -> Result<CertificateParams, String> {
    let mut params =
        CertificateParams::new(sans.to_vec()).map_err(|e| format!("Zertifikatsparameter: {e}"))?;
    params.not_before = rcgen::date_time_ymd(GUELTIG_AB.0, GUELTIG_AB.1, GUELTIG_AB.2);
    params.not_after = rcgen::date_time_ymd(GUELTIG_BIS.0, GUELTIG_BIS.1, GUELTIG_BIS.2);
    Ok(params)
}

/// Erzeugt ein frisches selbstsigniertes Paar als DER.
fn erzeugen(sans: &[String]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let params = params_bauen(sans)?;
    let key = KeyPair::generate().map_err(|e| format!("Schlüsselpaar: {e}"))?;
    let cert = params
        .self_signed(&key)
        .map_err(|e| format!("Selbstsignatur: {e}"))?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn pfade(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(CERT_DATEI), dir.join(KEY_DATEI))
}

/// Setzt die Dateirechte des privaten Schlüssels auf `0600`.
///
/// Nur unter Unix — unter Windows trägt das Nutzerprofil den Schutz, und der
/// Workspace kennt sonst keine gesetzten Dateirechte. Das `cfg`-Gate hält
/// zugleich die Linux-CI grün.
#[cfg(unix)]
fn schluessel_schuetzen(pfad: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(pfad, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("Dateirechte {}: {e}", pfad.display()))
}

#[cfg(not(unix))]
fn schluessel_schuetzen(_pfad: &Path) -> Result<(), String> {
    Ok(())
}

/// Lädt das persistierte Zertifikat oder erzeugt es beim ersten Mal.
///
/// **Lädt bewusst unverändert**, auch wenn sich die lokale IP geändert hat:
/// Eine Neuausstellung entwertete jede weggeklickte Browser-Ausnahme. Wer sie
/// wirklich braucht, löscht die beiden Dateien.
///
/// Beschädigte Dateien ergeben einen Fehler statt einer stillen Neuausstellung
/// — der Aufrufer lässt den HTTP-Server dann unbeeinträchtigt weiterlaufen.
pub fn laden_oder_erzeugen(dir: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (cert_pfad, key_pfad) = pfade(dir);

    if cert_pfad.exists() && key_pfad.exists() {
        let cert = std::fs::read(&cert_pfad)
            .map_err(|e| format!("Zertifikat lesen ({}): {e}", cert_pfad.display()))?;
        let key = std::fs::read(&key_pfad)
            .map_err(|e| format!("Schlüssel lesen ({}): {e}", key_pfad.display()))?;
        if cert.is_empty() || key.is_empty() {
            return Err(format!(
                "Zertifikatsdateien in {} sind leer — bitte löschen, dann werden sie neu erzeugt",
                dir.display()
            ));
        }
        return Ok((cert, key));
    }

    let (cert, key) = erzeugen(&sans_sammeln())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Verzeichnis {}: {e}", dir.display()))?;
    std::fs::write(&cert_pfad, &cert)
        .map_err(|e| format!("Zertifikat schreiben ({}): {e}", cert_pfad.display()))?;
    std::fs::write(&key_pfad, &key)
        .map_err(|e| format!("Schlüssel schreiben ({}): {e}", key_pfad.display()))?;
    schluessel_schuetzen(&key_pfad)?;
    tracing::info!(
        "TLS-Zertifikat erzeugt für {:?} — Ablage {}",
        sans_sammeln(),
        dir.display()
    );
    Ok((cert, key))
}

/// Baut die rustls-Serverkonfiguration.
///
/// **ALPN ausschließlich `http/1.1`** — siehe Modul-Doku.
///
/// Der Krypto-Provider wird **ausdrücklich** übergeben statt über
/// `ServerConfig::builder()` aus dem Prozess-Default gezogen: Im Baum liegen
/// `ring` **und** `aws-lc-rs` (beide als rustls-Abhängigkeit). Ohne
/// eindeutigen Default paniekt der bequeme Weg erst **zur Laufzeit** — also
/// im Turnier, nicht im Build.
pub fn server_config(cert_der: Vec<u8>, key_der: Vec<u8>) -> Result<Arc<ServerConfig>, String> {
    let certs = vec![CertificateDer::from(cert_der)];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS-Protokollversionen: {e}"))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS-Konfiguration: {e}"))?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(cfg))
}

/// TLS-Listener für `axum::serve`.
///
/// Der Handschlag läuft **nicht** in `accept`, sondern je Verbindung in einem
/// eigenen Task; fertige Verbindungen kommen über einen Kanal herein. Grund:
/// Läge der Handschlag in `accept`, blockierte ein einziger Client, der die
/// TCP-Verbindung öffnet und dann schweigt, die Annahme **aller** anderen
/// Geräte — in einem schwachen Hallen-WLAN kein Randfall. Zusätzlich begrenzt
/// [`HANDSHAKE_GEDULD`] jeden einzelnen Versuch.
pub struct TlsListener {
    rx: mpsc::Receiver<(tokio_rustls::server::TlsStream<TcpStream>, SocketAddr)>,
    local: SocketAddr,
    /// Wächter über den Annahme-Task — siehe [`Drop`]-Umsetzung.
    annahme: tokio::task::JoinHandle<()>,
}

impl Drop for TlsListener {
    /// Beendet den Annahme-Task, sobald der Listener fällt.
    ///
    /// **Ohne das bleibt der Port belegt.** `stop_sync` bricht den
    /// Server-Task ab; damit fällt zwar `axum::serve` samt Listener, ein
    /// fallengelassenes `JoinHandle` beendet in Tokio aber gar nichts — der
    /// Annahme-Task hielte `0.0.0.0:8443` weiter offen, und das nächste
    /// Starten scheiterte mit „Adresse belegt". Da jedes Speichern der
    /// Einstellungen ein Stoppen und Starten auslöst, wäre HTTPS nach der
    /// ersten Änderung bis zum Programm-Neustart tot. Gleiches Muster wie
    /// `TaktWaechter` in `server.rs`, aus demselben Grund.
    fn drop(&mut self) {
        self.annahme.abort();
    }
}

impl TlsListener {
    /// Bindet den Port und startet die Annahme im Hintergrund.
    ///
    /// Port `0` vergibt das Betriebssystem — dafür ist [`Self::local_addr`]
    /// da, damit Tests ohne feste Portwahl auskommen.
    pub async fn binden(port: u16, cfg: Arc<ServerConfig>) -> std::io::Result<Self> {
        let tcp = TcpListener::bind(("0.0.0.0", port)).await?;
        let local = tcp.local_addr()?;
        let (tx, rx) = mpsc::channel(ANNAHME_PUFFER);
        let acceptor = TlsAcceptor::from(cfg);

        let annahme = tokio::spawn(async move {
            loop {
                let (stream, addr) = match tcp.accept().await {
                    Ok(v) => v,
                    Err(e) => {
                        // Einzelne Annahmefehler sind normal (Client weg,
                        // Dateideskriptoren knapp) — niemals die Schleife
                        // verlassen, sonst ist der Port bis zum Neustart tot.
                        tracing::warn!("TLS-Annahme fehlgeschlagen: {e}");
                        continue;
                    }
                };
                let acceptor = acceptor.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    match tokio::time::timeout(HANDSHAKE_GEDULD, acceptor.accept(stream)).await {
                        Ok(Ok(tls)) => {
                            let _ = tx.send((tls, addr)).await;
                        }
                        // Häufigster Fall im Betrieb: Der Browser zeigt die
                        // Zertifikatswarnung und der Nutzer bricht ab. Das ist
                        // keine Störung, deshalb nur `debug`.
                        Ok(Err(e)) => tracing::debug!("TLS-Handschlag mit {addr} abgelehnt: {e}"),
                        Err(_) => tracing::debug!("TLS-Handschlag mit {addr} überfällig"),
                    }
                });
            }
        });

        Ok(Self { rx, local, annahme })
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.rx.recv().await {
            Some(v) => v,
            // Der Annahme-Task ist weg. `axum::serve` erwartet hier einen
            // Wert und würde bei einer Rückkehr sofort erneut fragen — ein
            // nie endendes Future hält die Schleife still, statt eine CPU zu
            // verheizen. Der HTTP-Server läuft davon unberührt weiter.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bts-tls-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("Testverzeichnis");
        dir
    }

    #[test]
    fn erzeugt_zertifikat_in_leerem_verzeichnis() {
        let dir = temp_dir("erzeugen");
        let (cert, key) = laden_oder_erzeugen(&dir).expect("Erzeugung");
        assert!(!cert.is_empty(), "Zertifikat leer");
        assert!(!key.is_empty(), "Schlüssel leer");
        assert!(dir.join(CERT_DATEI).exists());
        assert!(dir.join(KEY_DATEI).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Die Zusage „einmal bestätigen, dann Ruhe": Ein zweiter Start darf das
    /// Zertifikat NICHT neu ausstellen, sonst ist die Browser-Ausnahme jedes
    /// Mal wieder fällig.
    #[test]
    fn zweiter_aufruf_laedt_statt_neu_zu_erzeugen() {
        let dir = temp_dir("laden");
        let (cert1, key1) = laden_oder_erzeugen(&dir).expect("erste Erzeugung");
        let (cert2, key2) = laden_oder_erzeugen(&dir).expect("zweites Laden");
        assert_eq!(cert1, cert2, "Zertifikat wurde neu ausgestellt");
        assert_eq!(key1, key2, "Schlüssel wurde neu erzeugt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wächter für die RTC-lose Pi-Uhr: `notBefore` muss so weit zurück
    /// liegen, dass ein Gerät mit falsch gestellter Uhr das Zertifikat nicht
    /// still als „noch nicht gültig" verwirft. Dieser Test darf nicht
    /// „repariert" werden, indem man das Datum näher an heute rückt.
    #[test]
    fn not_before_liegt_weit_in_der_vergangenheit() {
        let params = params_bauen(&sans_sammeln()).expect("Parameter");
        assert!(
            params.not_before < rcgen::date_time_ymd(2021, 1, 1),
            "notBefore zu spät — RTC-lose Pis verwerfen das Zertifikat still"
        );
        assert!(
            params.not_after > rcgen::date_time_ymd(2030, 1, 1),
            "Laufzeit zu kurz — jede Neuausstellung entwertet alle Ausnahmen"
        );
    }

    #[test]
    fn san_enthaelt_mdns_namen_und_localhost() {
        let sans = sans_sammeln();
        assert!(
            sans.contains(&MDNS_HOST.to_string()),
            "mDNS-Name fehlt — er ist die einzige stabile Identität bei DHCP"
        );
        assert!(sans.contains(&"localhost".to_string()));
        assert!(sans.contains(&"127.0.0.1".to_string()));
    }

    #[test]
    fn beschaedigte_dateien_ergeben_fehler_ohne_panic() {
        let dir = temp_dir("kaputt");
        std::fs::write(dir.join(CERT_DATEI), b"").expect("leeres Zertifikat");
        std::fs::write(dir.join(KEY_DATEI), b"").expect("leerer Schlüssel");
        let ergebnis = laden_oder_erzeugen(&dir);
        assert!(ergebnis.is_err(), "leere Dateien müssen einen Fehler geben");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Der Listener muss sich binden lassen und seinen Port nennen können.
    ///
    /// Port 0 lässt das Betriebssystem wählen — nach dem Vorbild von
    /// `spawn_mock_btp` in `server.rs`, damit der Test keinen festen Port
    /// belegt und nicht mit einer laufenden App kollidiert.
    ///
    /// Bewusst begrenzt: Das ist ein Rauchtest für Bindung und Adressabfrage,
    /// **kein** Beweis, dass ein echter Handschlag zustande kommt — dafür
    /// bräuchte es einen Client mit passendem Vertrauensanker. Der Handschlag
    /// gehört in den Feldtest (siehe Spec).
    #[tokio::test]
    async fn listener_bindet_sich_und_nennt_seinen_port() {
        let dir = temp_dir("listener");
        let (cert, key) = laden_oder_erzeugen(&dir).expect("Erzeugung");
        let cfg = server_config(cert, key).expect("Serverkonfiguration");
        let listener = TlsListener::binden(0, cfg).await.expect("Bindung");
        let addr = {
            use axum::serve::Listener;
            listener.local_addr().expect("lokale Adresse")
        };
        assert!(addr.port() > 0, "Betriebssystem hat keinen Port vergeben");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wächter gegen einen zurückgelassenen Annahme-Task.
    ///
    /// Jedes Speichern der Einstellungen stoppt und startet die Übertragung.
    /// Gäbe der fallengelassene Listener seinen Port nicht frei, scheiterte
    /// das nächste Binden mit „Adresse belegt" und HTTPS bliebe bis zum
    /// Programm-Neustart tot (Review-Fund 25.08.2026).
    #[tokio::test]
    async fn listener_gibt_seinen_port_beim_fallenlassen_frei() {
        let dir = temp_dir("port-frei");
        let (cert, key) = laden_oder_erzeugen(&dir).expect("Erzeugung");
        let cfg = server_config(cert, key).expect("Serverkonfiguration");

        let port = {
            let listener = TlsListener::binden(0, cfg.clone()).await.expect("Bindung");
            use axum::serve::Listener;
            listener.local_addr().expect("lokale Adresse").port()
            // `listener` faellt hier -> `Drop` muss den Annahme-Task beenden.
        };

        // Dem abgebrochenen Task einen Moment geben, den Socket zu schliessen.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        TlsListener::binden(port, cfg)
            .await
            .expect("Port muss nach dem Fallenlassen wieder bindbar sein");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Böte der Stack `h2` an und der Browser wählte es, bräche der
    /// WebSocket-Upgrade — und mit ihm Tablet, Court-Monitor und TL-Web.
    #[test]
    fn alpn_bietet_nur_http11() {
        let dir = temp_dir("alpn");
        let (cert, key) = laden_oder_erzeugen(&dir).expect("Erzeugung");
        let cfg = server_config(cert, key).expect("Serverkonfiguration");
        assert_eq!(
            cfg.alpn_protocols,
            vec![b"http/1.1".to_vec()],
            "ALPN muss ausschliesslich http/1.1 anbieten"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
