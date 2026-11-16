//! Ein-Klick-Zugang zur Turnierleitungs-Oberfläche **am Turnier-PC selbst**.
//!
//! Wer am Rechner sitzt, auf dem bts-light läuft, soll die Oberfläche nicht
//! erst koppeln müssen (Gerät anlegen, Adresse kopieren, im Browser
//! einfügen). Der Knopf „Turnierleitung öffnen" legt deshalb beim ersten
//! Druck einen eigenen Zugang für diesen PC an und öffnet die Seite damit.
//!
//! Bewusst **kein** Sonderweg an der Anmeldung vorbei: Der PC bekommt einen
//! gewöhnlichen Eintrag in der Geräteliste („Dieser PC"). Er ist dort
//! sichtbar, lässt sich entziehen wie jeder andere, zählt mit — und die
//! Seite prüft ihn auf demselben Weg wie ein gekoppeltes Tablet. Eine
//! Ausnahme „Anfragen von 127.0.0.1 brauchen keinen Zugang" wäre kürzer,
//! aber jedes andere Programm auf dem PC (und jede Webseite im Browser, die
//! auf `127.0.0.1` zugreift) käme dann ebenfalls hinein.
//!
//! Hier steht nur die reine Entscheidung; der Tauri-Command liegt in
//! `commands.rs` (`tl_web_open_local`).

use crate::config::{AppConfig, TlDevice};

/// Feste Kennung des Zugangs für diesen PC. Fest, damit ein zweiter Klick
/// denselben Eintrag wiederfindet statt einen weiteren anzulegen.
pub const DIESER_PC_ID: &str = "tl-dieser-pc";

/// Name in der Geräteliste.
pub const DIESER_PC_LABEL: &str = "Dieser PC";

/// Auf welchem Weg dieser PC seine eigene Oberfläche erreicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weg {
    /// Der eingebettete Server, über die Schleifenadresse — braucht weder
    /// Hallennetz noch Internet.
    Lan,
    /// Reiner Cloud-Betrieb: Hier läuft kein eingebetteter Server, die Seite
    /// kommt vom Relay.
    Cloud,
}

/// Wählt den Weg. Im Doppelbetrieb gewinnt der eingebettete Server: Er
/// hängt nicht am Internet und antwortet ohne Umweg.
pub fn weg(cfg: &AppConfig) -> Result<Weg, String> {
    if cfg.slave_mode {
        // Der PC der fernen Halle hat keine eigene Oberfläche — sie gehört
        // zum Turnier-PC, und dessen Zugänge kennt nur er.
        return Err(
            "Dieser PC ist als ferne Halle eingerichtet. Die Turnierleitungs-Oberfläche \
             gehört zum Turnier-PC und wird dort gekoppelt."
                .to_string(),
        );
    }
    if cfg.connection_mode.lan_enabled() {
        Ok(Weg::Lan)
    } else {
        Ok(Weg::Cloud)
    }
}

/// Der Zugang dieses PCs, falls er schon besteht **und** die Oberfläche
/// eingeschaltet ist. Dann muss beim Klick nichts gespeichert werden.
pub fn bereit(cfg: &AppConfig) -> Option<&str> {
    if !cfg.tl_web.enabled {
        return None;
    }
    cfg.tl_web
        .devices
        .iter()
        .find(|d| d.id == DIESER_PC_ID)
        .map(|d| d.token.as_str())
}

/// Sorgt dafür, dass dieser PC einen Zugang hat und die Oberfläche läuft.
/// Liefert den gültigen Zugang.
///
/// Ein bestehender Eintrag bleibt, wie er ist — `frischer_zugang` wird nur
/// gebraucht, wenn es noch keinen gibt (erster Klick, oder der Eintrag wurde
/// in der Geräteliste entfernt). Sonst flöge bei jedem Klick ein schon
/// offener Browser-Tab hinaus.
///
/// Einschalten gehört dazu: Wer „Turnierleitung öffnen" drückt, will die
/// Oberfläche — dieselbe Überlegung wie beim Koppeln eines Geräts.
pub fn sicherstellen(
    cfg: &mut AppConfig,
    frischer_zugang: &str,
    jetzt_ms: u64,
) -> Result<String, String> {
    let vorhanden = cfg
        .tl_web
        .devices
        .iter()
        .find(|d| d.id == DIESER_PC_ID)
        .map(|d| d.token.clone());
    let zugang = match vorhanden {
        Some(z) => z,
        None => {
            // Über `add_device`, nicht direkt in die Liste: Dort sitzt die
            // Obergrenze, die der Relay sonst stillschweigend durchsetzt.
            cfg.tl_web.add_device(TlDevice {
                id: DIESER_PC_ID.to_string(),
                token: frischer_zugang.to_string(),
                label: DIESER_PC_LABEL.to_string(),
                created_at_ms: jetzt_ms,
                // Keine Halle, kein Profil: Der Turnier-PC sieht alles, bis
                // jemand es in der Oberfläche anders wählt.
                hall: String::new(),
                profile_id: String::new(),
            })?;
            frischer_zugang.to_string()
        }
    };
    cfg.tl_web.enabled = true;
    Ok(zugang)
}

/// Die Adresse, die der Browser öffnet.
///
/// Der Zugang steht im Fragment (`#t=…`) — das schickt kein Browser an
/// einen Server. `app=1` daneben sagt der Seite, dass der Knopf in bts-light
/// sie geöffnet hat: Nur dann räumt sie einen schon offenen Tab weg, statt
/// einen zweiten danebenzustellen (siehe `src/io/einTab.mjs`).
///
/// `127.0.0.1` statt des Rechnernamens oder der Netz-Adresse: Die Adresse
/// gilt auch ohne Netzwerk, und der Browser legt Zugang und Einstellungen
/// der Seite je Adresse ab — bliebe sie nicht immer dieselbe, fände ein
/// zweiter Klick den offenen Tab nicht wieder.
pub fn adresse(weg: Weg, zugang: &str, relay_https: &str) -> String {
    // Kodiert, weil der Zugang aus der Konfigurationsdatei stammen kann:
    // Ein `&` oder Leerzeichen darin zerrisse sonst das Fragment. Die Seite
    // dekodiert ihn wieder (`decodeURIComponent`).
    let z = relay_proto::url_encode(zugang);
    match weg {
        Weg::Lan => format!(
            "http://127.0.0.1:{}/tl#t={z}&app=1",
            crate::tablet::server::TABLET_PORT
        ),
        Weg::Cloud => format!("{relay_https}/tl#t={z}&app=1"),
    }
}

/// Wie lange vor dem Öffnen zu warten ist.
///
/// Nur im reinen Cloud-Betrieb und nur, wenn der Zugang gerade erst
/// gespeichert wurde: Der Turnier-PC meldet seine Zugänge im Zwei-Sekunden-
/// Takt an den Relay. Wer schneller ist, sieht „Zugang gilt nicht mehr" —
/// das heilte zwar von selbst, sähe beim allerersten Klick aber nach einem
/// Fehler aus. Im Hallennetz prüft der eigene Server und kennt den Zugang
/// sofort.
pub fn wartezeit(weg: Weg, gerade_gespeichert: bool) -> std::time::Duration {
    match (weg, gerade_gespeichert) {
        (Weg::Cloud, true) => std::time::Duration::from_millis(2500),
        _ => std::time::Duration::ZERO,
    }
}

/// Ein neuer, zufälliger Zugang (128 Bit, als 32 Hex-Zeichen).
///
/// Aus der Zufallsquelle des Betriebssystems — ein erratbarer Zugang wäre
/// im Cloud-Betrieb ein offenes Turnier.
pub fn zufalls_zugang() -> Result<String, String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "Kein Zufall vom Betriebssystem – Zugang nicht angelegt.".to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectionMode;

    fn cfg(mode: ConnectionMode) -> AppConfig {
        AppConfig {
            connection_mode: mode,
            ..Default::default()
        }
    }

    fn geraet(id: &str, token: &str) -> TlDevice {
        TlDevice {
            id: id.to_string(),
            token: token.to_string(),
            label: "Tablet".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn erster_klick_legt_den_zugang_an_und_schaltet_ein() {
        let mut c = cfg(ConnectionMode::Lan);
        assert!(!c.tl_web.enabled);
        assert_eq!(bereit(&c), None);

        let zugang = sicherstellen(&mut c, "frisch", 42).unwrap();

        assert_eq!(zugang, "frisch");
        assert!(c.tl_web.enabled);
        assert_eq!(c.tl_web.devices.len(), 1);
        let d = &c.tl_web.devices[0];
        assert_eq!(d.id, DIESER_PC_ID);
        assert_eq!(d.label, DIESER_PC_LABEL);
        assert_eq!(d.created_at_ms, 42);
        // Alle Hallen, Standardprofil.
        assert_eq!(d.hall, "");
        assert_eq!(d.profile_id, "");
        assert_eq!(bereit(&c), Some("frisch"));
    }

    #[test]
    fn zweiter_klick_behaelt_den_zugang() {
        // Ein neuer Zugang bei jedem Klick würfe den offenen Tab hinaus.
        let mut c = cfg(ConnectionMode::Lan);
        sicherstellen(&mut c, "erster", 1).unwrap();
        let zugang = sicherstellen(&mut c, "zweiter", 2).unwrap();
        assert_eq!(zugang, "erster");
        assert_eq!(c.tl_web.devices.len(), 1);
        assert_eq!(c.tl_web.devices[0].created_at_ms, 1);
    }

    #[test]
    fn gekoppelte_geraete_bleiben_unberuehrt() {
        let mut c = cfg(ConnectionMode::Lan);
        c.tl_web.enabled = true;
        c.tl_web.devices.push(geraet("tl-abc", "tok-tablet"));
        sicherstellen(&mut c, "frisch", 1).unwrap();
        assert_eq!(c.tl_web.devices.len(), 2);
        assert_eq!(c.tl_web.devices[0].token, "tok-tablet");
        assert_eq!(c.tl_web.devices[1].id, DIESER_PC_ID);
    }

    #[test]
    fn abgeschaltete_oberflaeche_ist_nicht_bereit_und_wird_eingeschaltet() {
        // Zugang da, Schalter aus: Die Seite käme nicht hinein. Der Klick
        // muss also speichern — und behält dabei den Zugang.
        let mut c = cfg(ConnectionMode::Lan);
        sicherstellen(&mut c, "alt", 1).unwrap();
        c.tl_web.enabled = false;
        assert_eq!(bereit(&c), None);
        assert_eq!(sicherstellen(&mut c, "neu", 2).unwrap(), "alt");
        assert!(c.tl_web.enabled);
    }

    #[test]
    fn entfernter_zugang_wird_neu_vergeben() {
        // In der Geräteliste entzogen heißt: Der alte Zugang gilt nie
        // wieder. Der nächste Klick bekommt einen neuen.
        let mut c = cfg(ConnectionMode::Lan);
        sicherstellen(&mut c, "alt", 1).unwrap();
        assert!(c.tl_web.remove_device(DIESER_PC_ID));
        assert_eq!(bereit(&c), None);
        assert_eq!(sicherstellen(&mut c, "neu", 2).unwrap(), "neu");
    }

    #[test]
    fn volle_geraeteliste_wird_gemeldet_statt_verschluckt() {
        let mut c = cfg(ConnectionMode::Lan);
        for i in 0..relay_proto::MAX_TL_DEVICES_MIRRORED {
            c.tl_web.devices.push(geraet(&format!("tl-{i}"), "tok"));
        }
        let vorher = c.clone();
        assert!(sicherstellen(&mut c, "frisch", 1).is_err());
        // Nichts halb verändert — auch der Schalter nicht.
        assert_eq!(c, vorher);
    }

    #[test]
    fn weg_folgt_der_verbindungsart() {
        assert_eq!(weg(&cfg(ConnectionMode::Lan)), Ok(Weg::Lan));
        assert_eq!(weg(&cfg(ConnectionMode::Cloud)), Ok(Weg::Cloud));
        // Im Doppelbetrieb der Weg ohne Internet.
        assert_eq!(weg(&cfg(ConnectionMode::LanAndCloud)), Ok(Weg::Lan));
    }

    #[test]
    fn ferne_halle_hat_keine_eigene_oberflaeche() {
        let mut c = cfg(ConnectionMode::Lan);
        c.slave_mode = true;
        assert!(weg(&c).is_err());
    }

    #[test]
    fn adresse_im_hallennetz_ist_die_schleifenadresse() {
        assert_eq!(
            adresse(Weg::Lan, "abc-123", "https://badhub.de/bts-relay"),
            "http://127.0.0.1:8088/tl#t=abc-123&app=1"
        );
    }

    #[test]
    fn adresse_in_der_cloud_ist_der_relay() {
        assert_eq!(
            adresse(Weg::Cloud, "abc-123", "https://badhub.de/bts-relay"),
            "https://badhub.de/bts-relay/tl#t=abc-123&app=1"
        );
    }

    #[test]
    fn zugang_mit_sonderzeichen_zerreisst_das_fragment_nicht() {
        // Ein von Hand in die Konfiguration geschriebener Zugang darf weder
        // die Marke `app=1` verschlucken noch eine eigene einschleusen.
        let url = adresse(Weg::Lan, "a&app=0 b#c", "");
        assert_eq!(url, "http://127.0.0.1:8088/tl#t=a%26app%3D0%20b%23c&app=1");
        assert!(!url.contains(' '));
    }

    #[test]
    fn nur_der_frische_cloud_zugang_wartet_auf_den_relay() {
        assert!(wartezeit(Weg::Cloud, true) > std::time::Duration::from_secs(2));
        assert!(wartezeit(Weg::Cloud, false).is_zero());
        assert!(wartezeit(Weg::Lan, true).is_zero());
        assert!(wartezeit(Weg::Lan, false).is_zero());
    }

    #[test]
    fn zufalls_zugang_ist_lang_genug_und_jedes_mal_anders() {
        let a = zufalls_zugang().unwrap();
        let b = zufalls_zugang().unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
