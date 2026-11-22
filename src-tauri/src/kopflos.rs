//! Betrieb ohne Oberfläche: `bts-light --kopflos <ordner>`.
//!
//! Für ein Punktspiel, das ohne Windows-PC gezählt wird (badhub ADR-0092
//! Schritt 4, `docs/features/punktspiel-kopflos.md`): je Begegnung läuft ein
//! Prozess auf dem Server. Er liest das Punktspiel aus badhub, hängt sich als
//! Host an den Cloud-Relay — Tablets zählen über ihren Link, die Feldvergabe
//! läuft im Turnierleitungs-Web — und meldet Feldvergabe, Punktestand und
//! Ergebnis an badhub. Kein Fenster, kein LAN-Server, kein Liveticker-Push.
//!
//! Der Ordner enthält `config/config.json`, `daten/` und `logs/`. Beim ersten
//! Start liegt dort stattdessen ein **Auftrag** (`auftrag.json`), den badhub
//! geschrieben hat: Adresse, Kopplungscode, Kennung der Installation und der
//! Zugang für das Turnierleitungs-Web. Der Prozess koppelt sich damit selbst,
//! schreibt die Konfiguration und löscht den Auftrag — der Code gilt nur
//! einmal.
//!
//! Läuft **vor** Tauri (`main.rs`): es gibt kein Display, und die
//! Einzelinstanz-Sperre des Desktops würde den zweiten Prozess beenden.
//!
//! Ende-Codes: `1` Start fehlgeschlagen (neuer Auftrag nötig), `2` falscher
//! Aufruf, `70` Absturz in der laufenden Übertragung (neu starten).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::commands::{kopflos_starten, OrtKopflos};
use crate::config::{AppConfig, ConnectionMode, QuelleArt, TlDevice};

/// Auftrag von badhub für den ersten Start.
#[derive(Deserialize)]
struct Auftrag {
    /// Liveticker-Adresse des Systems (`https://badhub.de/api/live_update.php`)
    /// — aus ihr folgen API-Basis und Relay, wie am Desktop.
    push_url: String,
    /// Einmal-Kopplungscode des Punktspiels.
    code: String,
    /// Kennung dieser Installation = Namespace am Relay (UUID).
    install_id: String,
    /// Name, unter dem das Gerät in badhub erscheint.
    #[serde(default)]
    name: String,
    /// Zugang für das Turnierleitungs-Web.
    tl: AuftragTl,
}

#[derive(Deserialize)]
struct AuftragTl {
    id: String,
    token: String,
    #[serde(default)]
    label: String,
}

fn jetzt_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Konfiguration eines Punktspiels ohne Oberfläche: nur Cloud, kein
/// Liveticker, keine Log-Uploads, das Turnierleitungs-Web mit genau dem
/// Zugang aus dem Auftrag.
fn grundkonfiguration(a: &Auftrag) -> Result<AppConfig, String> {
    // Nur badhub selbst: an diese Adresse gehen Kopplungscode und Gerätetoken.
    // Eine lokale Adresse (Attrappe) nur im Entwicklungs-Build.
    let lokal = cfg!(debug_assertions) && a.push_url.trim().starts_with("http://127.0.0.1:");
    if !crate::badhub_host::ist_badhub_adresse(&a.push_url) && !lokal {
        return Err("Auftrag: push_url zeigt nicht auf badhub.".into());
    }
    if !crate::tablet::relay_client::valid_relay_namespace(a.install_id.trim()) {
        return Err("Auftrag: install_id ist keine gültige Kennung.".into());
    }
    let mut cfg = AppConfig::neu_installation();
    cfg.upload_logs = false;
    cfg.badhub.url = a.push_url.trim().to_string();
    cfg.quelle = QuelleArt::Punktspiel;
    cfg.connection_mode = ConnectionMode::Cloud;
    cfg.install_id = a.install_id.trim().to_string();
    cfg.liveticker_aus = true;
    cfg.tl_web.enabled = true;
    cfg.tl_web.devices.clear();
    cfg.tl_web.add_device(TlDevice {
        id: a.tl.id.trim().to_string(),
        token: a.tl.token.trim().to_string(),
        label: if a.tl.label.trim().is_empty() {
            "Mein Badhub".to_string()
        } else {
            a.tl.label.trim().to_string()
        },
        created_at_ms: jetzt_ms(),
        ..TlDevice::default()
    })?;
    Ok(cfg)
}

/// Erster Start: Auftrag lesen, koppeln, Konfiguration schreiben, Auftrag
/// löschen. Schlägt das Koppeln fehl, bleibt nichts zurück als der (dann
/// verbrauchte oder abgelaufene) Auftrag — badhub stellt einen neuen aus.
fn einrichten(wurzel: &Path, auftrag_datei: &Path) -> Result<AppConfig, String> {
    let roh = std::fs::read(auftrag_datei).map_err(|e| format!("Auftrag nicht lesbar: {e}"))?;
    let auftrag: Auftrag =
        serde_json::from_slice(&roh).map_err(|e| format!("Auftrag ungültig: {e}"))?;
    let mut cfg = grundkonfiguration(&auftrag)?;
    // Live- oder Testsystem festlegen, bevor irgendetwas verbindet: der Relay
    // folgt diesem Schalter (am Desktop setzt ihn das Laden der Konfiguration).
    crate::badhub_host::set_aus_push_url(&cfg.badhub.url);
    let basis = crate::quelle::punktspiel::api_basis(&cfg.badhub.url)?;
    let daten = OrtKopflos::daten_ordner(wurzel);
    std::fs::create_dir_all(&daten).map_err(|e| e.to_string())?;
    let name = if auftrag.name.trim().is_empty() {
        "badhub (ohne PC)"
    } else {
        auftrag.name.trim()
    };
    let kopplung = tauri::async_runtime::block_on(crate::quelle::punktspiel::koppeln(
        &basis,
        &daten,
        &auftrag.code,
        &cfg.install_id,
        name,
    ))?;
    cfg.punktspiel = kopplung.punktspiel;
    let datei = OrtKopflos::config_datei(wurzel);
    if let Some(dir) = datei.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    cfg.save_to(&datei).map_err(|e| {
        format!("Konfiguration nicht geschrieben ({e}) — der Kopplungscode ist verbraucht, bitte einen neuen Auftrag ausstellen.")
    })?;
    nur_eigner(&datei);
    // Der Code ist eingelöst: nie liegen lassen.
    if let Err(e) = std::fs::remove_file(auftrag_datei) {
        tracing::warn!(target: "kopflos", "Auftrag nicht gelöscht: {e}");
    }
    tracing::info!(target: "kopflos", "gekoppelt mit „{}“ ({} Spiele, {} Felder)", cfg.punktspiel.name, kopplung.spiele, kopplung.felder);
    Ok(cfg)
}

/// Datei oder Ordner nur für den Dienstbenutzer lesbar machen: in der
/// Konfiguration steht der Zugang zum Turnierleitungs-Web im Klartext.
fn nur_eigner(pfad: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modus = if pfad.is_dir() { 0o700 } else { 0o600 };
        if let Err(e) = std::fs::set_permissions(pfad, std::fs::Permissions::from_mode(modus)) {
            tracing::warn!(target: "kopflos", "Rechte nicht gesetzt ({}): {e}", pfad.display());
        }
    }
    #[cfg(not(unix))]
    let _ = pfad;
}

fn laden(wurzel: &Path) -> Result<AppConfig, String> {
    let datei = OrtKopflos::config_datei(wurzel);
    let auftrag = wurzel.join("auftrag.json");
    if !datei.exists() {
        return einrichten(wurzel, &auftrag);
    }
    // Ein Auftrag neben einer fertigen Konfiguration ist ein Rest (Neustart
    // nach dem Koppeln): der Code darin ist verbraucht. Je Begegnung gibt es
    // einen eigenen Ordner — ein neuer Auftrag gehört nie in einen benutzten.
    if auftrag.exists() {
        tracing::warn!(target: "kopflos", "Auftrag neben fertiger Konfiguration verworfen");
        let _ = std::fs::remove_file(&auftrag);
    }
    let mut cfg = AppConfig::load_from(&datei).map_err(|e| e.to_string())?;
    // Diese Schalter gelten hier immer, was auch in der Datei steht.
    cfg.liveticker_aus = true;
    cfg.connection_mode = ConnectionMode::Cloud;
    cfg.upload_logs = false;
    if cfg.quelle != QuelleArt::Punktspiel {
        return Err("Ohne Oberfläche läuft nur die Quelle „Punktspiel“.".into());
    }
    if !crate::badhub_host::ist_badhub_adresse(&cfg.badhub.url)
        && !(cfg!(debug_assertions) && cfg.badhub.url.starts_with("http://127.0.0.1:"))
    {
        return Err("Die Konfiguration zeigt nicht auf badhub.".into());
    }
    Ok(cfg)
}

fn lauf(wurzel: &Path) -> Result<(), String> {
    // Der Ordner selbst gehört badhub (es legt dort Auftrag und die Marke
    // `ende` ab) — seine Rechte bleiben. Was dieser Prozess darin anlegt,
    // gehört nur ihm.
    std::fs::create_dir_all(wurzel).map_err(|e| e.to_string())?;
    for ordner in [
        wurzel.join("config"),
        wurzel.join("daten"),
        wurzel.join("logs"),
    ] {
        std::fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
        nur_eigner(&ordner);
    }
    let cfg = laden(wurzel)?;
    // Ohne Kopplung oder ohne Gerätetoken liefe die Übertragung leer weiter
    // und meldete jeden Zyklus denselben Fehler — das ist ein Startfehler.
    if !cfg.punktspiel.gekoppelt() {
        return Err("Kein Punktspiel gekoppelt — bitte einen neuen Auftrag ausstellen.".into());
    }
    if !crate::quelle::punktspiel::starten(&cfg, &OrtKopflos::daten_ordner(wurzel)) {
        return Err(
            "Zugang zum Punktspiel fehlt oder ist ungültig — bitte einen neuen Auftrag ausstellen."
                .into(),
        );
    }
    kopflos_starten(wurzel, cfg)?;
    tracing::info!(target: "kopflos", "Übertragung läuft");
    Ok(())
}

/// Ende-Code nach einer Panik in der laufenden Übertragung — der
/// Dienstverwalter startet dann neu. `1` = Start fehlgeschlagen (kein
/// Neustart: es braucht einen neuen Auftrag), `2` = falscher Aufruf.
pub const ENDE_ABSTURZ: i32 = 70;

/// `--kopflos <ordner>`: startet die Übertragung ohne Oberfläche und kehrt
/// nicht zurück, solange sie läuft. `None` = kein solcher Aufruf, der normale
/// Start folgt. `Some(code)` nur bei einem Fehler vor dem Start.
pub fn einstieg(args: &[String]) -> Option<i32> {
    let i = args.iter().position(|a| a == "--kopflos")?;
    // Protokoll auf die Standardfehlerausgabe: der Dienstverwalter sammelt es.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    let Some(wurzel) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("Aufruf: bts-light --kopflos <ordner>");
        return Some(2);
    };
    // TLS-Anbieter festlegen: Im Programm stecken zwei (ring und aws-lc-rs),
    // und rustls wählt dann keinen von selbst. Am Desktop erledigt das ein
    // Tauri-Plugin beim Start; hier gibt es keins — ohne diese Zeile bräche
    // die Verbindung zum Relay beim ersten Aufbau ab.
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    // Stirbt eine Aufgabe der Übertragung (Panik), liefe der Prozess sonst als
    // leere Hülle weiter: der Dienstverwalter sähe ihn gesund, die Halle
    // zählte ins Leere. Deshalb beendet jede Panik den Prozess — mit einem
    // eigenen Code, damit er neu gestartet wird (anders als ein Startfehler).
    let voriger = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        voriger(info);
        std::process::exit(ENDE_ABSTURZ);
    }));
    if let Err(e) = lauf(&wurzel) {
        tracing::error!(target: "kopflos", "Start fehlgeschlagen: {e}");
        eprintln!("bts-light --kopflos: {e}");
        return Some(1);
    }
    // Die Übertragung läuft in ihren eigenen Aufgaben. Beendet wird der
    // Prozess von außen (SIGTERM); alle Stände liegen laufend auf der Platte.
    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auftrag(install_id: &str, token: &str) -> Auftrag {
        Auftrag {
            push_url: "https://test.badhub.de/api/live_update.php".into(),
            code: "ABCD2345".into(),
            install_id: install_id.into(),
            name: String::new(),
            tl: AuftragTl {
                id: "mein-badhub".into(),
                token: token.into(),
                label: String::new(),
            },
        }
    }

    #[test]
    fn kein_kopflos_aufruf_ist_kein_einstieg() {
        assert_eq!(einstieg(&[]), None);
        assert_eq!(einstieg(&["--selbsttest".to_string()]), None);
    }

    #[test]
    fn kopflos_ohne_ordner_endet_mit_fehler() {
        assert_eq!(einstieg(&["--kopflos".to_string()]), Some(2));
    }

    #[test]
    fn grundkonfiguration_nur_cloud_ohne_liveticker_mit_tl_zugang() {
        let cfg =
            grundkonfiguration(&auftrag("0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f", "geheim")).unwrap();
        assert_eq!(cfg.quelle, QuelleArt::Punktspiel);
        assert_eq!(cfg.connection_mode, ConnectionMode::Cloud);
        assert!(cfg.liveticker_aus && !cfg.upload_logs);
        assert!(cfg.tl_web.enabled);
        assert_eq!(cfg.tl_web.devices.len(), 1);
        assert_eq!(cfg.tl_web.devices[0].token, "geheim");
        assert_eq!(cfg.tl_web.devices[0].label, "Mein Badhub");
        assert!(
            cfg.badhub.password.is_empty(),
            "kein Liveticker-Passwort nötig"
        );
    }

    #[test]
    fn ungueltiger_auftrag_wird_abgelehnt() {
        assert!(grundkonfiguration(&auftrag("keine-uuid", "geheim")).is_err());
        assert!(
            grundkonfiguration(&auftrag("0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f", "")).is_err(),
            "ohne Zugang kein Turnierleitungs-Web"
        );
        let mut reserviert = auftrag("0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f", "geheim");
        reserviert.tl.id = relay_proto::TL_GERAET_KERN.into();
        assert!(grundkonfiguration(&reserviert).is_err());
    }

    #[test]
    fn auftrag_darf_nur_auf_badhub_zeigen() {
        let mut fremd = auftrag("0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f", "geheim");
        fremd.push_url = "https://badhub.de.example.org/api/live_update.php".into();
        assert!(grundkonfiguration(&fremd).unwrap_err().contains("badhub"));
        fremd.push_url = "http://badhub.de/api/live_update.php".into();
        assert!(grundkonfiguration(&fremd).is_err(), "nie im Klartext");
        fremd.push_url = "https://badhub.de/api/live_update.php".into();
        assert!(grundkonfiguration(&fremd).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn ordner_und_dateien_nur_fuer_den_dienstbenutzer() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = tempfile::tempdir().unwrap();
        let _ = lauf(ordner.path());
        for teil in ["config", "daten", "logs"] {
            let modus = std::fs::metadata(ordner.path().join(teil))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(modus, 0o700, "{teil}");
        }
        let datei = ordner.path().join("config").join("x.json");
        std::fs::write(&datei, b"{}").unwrap();
        nur_eigner(&datei);
        assert_eq!(
            std::fs::metadata(&datei).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn konfiguration_ohne_kopplung_ist_ein_startfehler() {
        let ordner = tempfile::tempdir().unwrap();
        let datei = OrtKopflos::config_datei(ordner.path());
        std::fs::create_dir_all(datei.parent().unwrap()).unwrap();
        let mut cfg =
            grundkonfiguration(&auftrag("0f3a1c2e-7b4d-4c1a-9e2f-5a6b7c8d9e0f", "geheim")).unwrap();
        cfg.badhub.url = "https://badhub.de/api/live_update.php".into();
        cfg.save_to(&datei).unwrap();
        let fehler = lauf(ordner.path()).unwrap_err();
        assert!(fehler.contains("gekoppelt"), "{fehler}");
    }

    #[test]
    fn start_ohne_auftrag_und_ohne_konfiguration_scheitert_sauber() {
        let ordner = tempfile::tempdir().unwrap();
        let fehler = lauf(ordner.path()).unwrap_err();
        assert!(fehler.contains("Auftrag"), "{fehler}");
    }

    #[test]
    fn fremde_quelle_startet_nicht() {
        let ordner = tempfile::tempdir().unwrap();
        let datei = OrtKopflos::config_datei(ordner.path());
        std::fs::create_dir_all(datei.parent().unwrap()).unwrap();
        AppConfig::neu_installation().save_to(&datei).unwrap();
        std::fs::write(ordner.path().join("auftrag.json"), b"{}").unwrap();
        let fehler = laden(ordner.path()).unwrap_err();
        assert!(fehler.contains("Punktspiel"), "{fehler}");
        assert!(
            !ordner.path().join("auftrag.json").exists(),
            "Rest-Auftrag entfernt"
        );
    }
}
