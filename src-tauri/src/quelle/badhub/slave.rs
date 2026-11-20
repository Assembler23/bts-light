//! LAN-Ansage-Slave bei einem badhub-Turnier (Paket L3, badhub-Spec
//! „Einrichtung“): Der Slave liest kein BTP und nimmt keine Lease, sondern
//! holt die Kern-Ereignisse vom Master (`GET /quelle/ereignisse?ab=<n>` am
//! LAN-Server des Turnier-PCs, Adresse im Feld „Adresse des Turnier-PCs“)
//! und faltet sie mit seinem eigenen Turnierpaket.
//!
//! **Zugang:** Der Slave koppelt sich selbst mit badhub (Namen, Felder) und
//! beweist mit dem **Slave-Schlüssel** im Kopf `X-Bts-Slave`, dass er zum
//! Turnier-PC gehört. Den Schlüssel (16 Zufallsbytes) erzeugt der Master beim
//! Start, zeigt ihn im Dashboard, und die Turnierleitung trägt ihn am Slave
//! ein; beide legen ihn verschlüsselt unter `turniere/<uuid>/slave.bin` ab.
//! Verglichen wird in konstanter Zeit. Die Turnier-UUID allein reicht nicht —
//! sie ist öffentlich (Security-Review L3).
//!
//! Über die Leitung gehen nur Ereignisse (Nummern, keine Namen), seitenweise
//! ab dem Stand des Slaves.

use std::time::Duration;

use turnierkern::TurnierEreignis;

use crate::btp::model::BtpSnapshot;
use crate::config::AppConfig;

/// Kopf mit dem Slave-Schlüssel.
pub const KOPF_SCHLUESSEL: &str = "x-bts-slave";
/// Pfad am LAN-Server des Masters.
pub const PFAD: &str = "/quelle/ereignisse";
/// Dateiname des Schlüssels im Turnierordner.
pub const SCHLUESSEL_DATEI: &str = "slave.bin";
/// Höchstzahl der Ereignisse je Antwort.
pub const SEITE_MAX: usize = 500;
/// Höchstzahl der Seiten je Abruf des Slaves (der Rest kommt im nächsten Takt).
const SEITEN_JE_ABRUF: usize = 20;

/// Antwort des Masters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MasterEreignisse {
    pub uuid: String,
    /// Stand des Masters (höchstes `n`).
    pub n: u64,
    /// Ereignisse mit `n > ab`, höchstens [`SEITE_MAX`].
    pub ereignisse: Vec<TurnierEreignis>,
    /// Nur mit `posten=1` (Standby, Paket S2): eigene, noch nicht von badhub
    /// bestätigte Posten des Masters mit `n > ab`. Fehlt bei älteren Mastern
    /// und ohne `posten=1` — die Antwort bleibt dann Byte für Byte wie bisher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posten: Option<Vec<super::standby::PostenWire>>,
    /// Von badhub bestätigtes `n` des Masters (mit `posten=1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bestaetigt_n: Option<u64>,
    /// Epoche der Lease des Masters (mit `posten=1`, nur mit aktiver Lease).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoche: Option<u64>,
}

/// Schlüssel in der Eingabeform normalisieren: ohne Bindestriche und
/// Leerraum, klein, nur Hex. `None` bei falscher Länge.
pub fn schluessel_normal(eingabe: &str) -> Option<String> {
    let s: String = eingabe
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    (s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())).then_some(s)
}

/// Anzeigeform `abcd-ef01-…` (8 Gruppen à 4).
pub fn schluessel_anzeige(s: &str) -> String {
    s.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// Neuer Schlüssel (16 Zufallsbytes als Hex).
pub fn neuer_schluessel() -> Result<String, String> {
    super::lease::neuer_zugang().map(|z| z[..32].to_string())
}

/// Vergleich in konstanter Zeit (bei gleicher Länge).
pub fn gleich_konstant(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Was der Master auf `GET /quelle/ereignisse` antwortet: nur bei Quelle
/// badhub am Master und nur mit dem richtigen Slave-Schlüssel.
pub fn fuer_slave(
    config: &AppConfig,
    schluessel_kopf: Option<&str>,
    ab: u64,
    mit_posten: bool,
) -> Result<MasterEreignisse, u16> {
    if config.quelle != crate::config::QuelleArt::Badhub || config.slave_mode {
        return Err(404);
    }
    let g = super::zustand();
    let q = g.as_ref().ok_or(503u16)?;
    fuer_slave_aus(q, schluessel_kopf, ab, mit_posten)
}

/// Kern von [`fuer_slave`] am Zustand (testbar ohne globalen Zustand).
pub(crate) fn fuer_slave_aus(
    q: &super::KernQuelle,
    schluessel_kopf: Option<&str>,
    ab: u64,
    mit_posten: bool,
) -> Result<MasterEreignisse, u16> {
    let erwartet = q.slave_schluessel.as_deref().ok_or(403u16)?;
    let gegeben = schluessel_kopf
        .and_then(schluessel_normal)
        .unwrap_or_default();
    if !gleich_konstant(gegeben.as_bytes(), erwartet.as_bytes()) {
        return Err(403);
    }
    let (posten, bestaetigt_n, epoche) = if mit_posten {
        let bestaetigt = q.n_bestaetigt();
        let mut zahl = 0usize;
        let posten: Vec<_> = q
            .protokoll
            .offen_nach(ab.max(bestaetigt))
            .take_while(|p| {
                zahl += p.ereignisse.len();
                zahl <= SEITE_MAX
            })
            .map(super::standby::PostenWire::von)
            .collect();
        let epoche = q.lease.as_ref().filter(|l| l.aktiv()).map(|l| l.epoche);
        (Some(posten), Some(bestaetigt), epoche)
    } else {
        (None, None, None)
    };
    Ok(MasterEreignisse {
        uuid: q.paket.turnier.uuid.to_ascii_lowercase(),
        n: q.n_lokal(),
        ereignisse: q.ereignisse_ab(ab, SEITE_MAX),
        posten,
        bestaetigt_n,
        epoche,
    })
}

/// Stand des Slaves: Ereignisse vom Master (seitenweise ab dem eigenen
/// Stand), gefaltet mit dem eigenen Paket.
pub async fn stand_vom_master(config: &AppConfig) -> Result<BtpSnapshot, String> {
    let host = config.btp.host.trim().to_string();
    if host.is_empty() {
        return Err("Ansage-Slave: Adresse des Turnier-PCs (Master) fehlt.".into());
    }
    let (uuid, schluessel) = {
        let g = super::zustand();
        let q = g.as_ref().ok_or(
            "Ansage-Slave mit badhub-Turnier: bitte auch diesen PC mit einem Kopplungscode aus \
             badhub koppeln.",
        )?;
        let s = q.slave_schluessel.clone().ok_or(
            "Ansage-Slave: Slave-Schlüssel vom Turnier-PC fehlt — in den Einstellungen unter \
             „Turnierquelle“ eintragen.",
        )?;
        (q.paket.turnier.uuid.clone(), s)
    };
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    rolle_auffrischen();
    if super::zustand()
        .as_ref()
        .is_some_and(super::KernQuelle::ist_standby)
    {
        match spiegeln(&client, &host, &schluessel, &uuid).await {
            Ok(()) => {
                return match super::zustand().as_mut() {
                    Some(q) => Ok(q.stand_standby()),
                    None => Err("Kein badhub-Turnier geladen — bitte im Setup koppeln.".into()),
                }
            }
            Err(Spiegelfehler::Alt(t)) => {
                // Älterer Hauptrechner: wie bisher als Ansage-Slave lesen.
                if let Some(q) = super::zustand().as_mut() {
                    q.standby_hinweis = Some(t);
                }
            }
            Err(Spiegelfehler::Sonst(t)) => {
                // Hauptrechner nicht erreichbar: der letzte Spiegel bleibt
                // sichtbar — genau dann wird er gebraucht.
                if let Some(q) = super::zustand().as_mut() {
                    q.standby_hinweis = Some(t.clone());
                    if q.spiegel_n() > 0 {
                        return Ok(q.stand_standby());
                    }
                }
                return Err(t);
            }
        }
    }
    for _ in 0..SEITEN_JE_ABRUF {
        let ab = super::zustand().as_ref().map_or(0, |q| q.slave_n());
        let url = format!(
            "http://{host}:{}{PFAD}?ab={ab}",
            crate::tablet::server::TABLET_PORT
        );
        let antwort = client
            .get(&url)
            .header(KOPF_SCHLUESSEL, &schluessel)
            .send()
            .await
            .map_err(|e| format!("Turnier-PC nicht erreichbar ({})", e.without_url()))?;
        if !antwort.status().is_success() {
            return Err(format!(
                "Turnier-PC liefert keinen Stand (Status {}) — Slave-Schlüssel und Turnier prüfen.",
                antwort.status().as_u16()
            ));
        }
        let m: MasterEreignisse = antwort
            .json()
            .await
            .map_err(|_| "Stand vom Turnier-PC unlesbar".to_string())?;
        if !m.uuid.eq_ignore_ascii_case(&uuid) {
            return Err("Der Turnier-PC steuert ein anderes Turnier.".into());
        }
        let voll = m.ereignisse.len() >= SEITE_MAX;
        if let Some(q) = super::zustand().as_mut() {
            q.slave_uebernehmen(m.n, m.ereignisse);
        }
        if !voll {
            break;
        }
    }
    match super::zustand().as_mut() {
        Some(q) => Ok(q.stand_slave()),
        None => Err("Kein badhub-Turnier geladen — bitte im Setup koppeln.".into()),
    }
}

/// Warum das Spiegeln scheiterte.
enum Spiegelfehler {
    /// Der Hauptrechner liefert keine Posten (ältere Version).
    Alt(String),
    Sonst(String),
}

/// Standby (Paket S2): Posten des Hauptrechners seitenweise in den Spiegel.
async fn spiegeln(
    client: &reqwest::Client,
    host: &str,
    schluessel: &str,
    uuid: &str,
) -> Result<(), Spiegelfehler> {
    for _ in 0..SEITEN_JE_ABRUF {
        let ab = super::zustand().as_ref().map_or(0, |q| q.spiegel_n());
        let url = format!(
            "http://{host}:{}{PFAD}?ab={ab}&posten=1",
            crate::tablet::server::TABLET_PORT
        );
        let antwort = client
            .get(&url)
            .header(KOPF_SCHLUESSEL, schluessel)
            .send()
            .await
            .map_err(|e| {
                Spiegelfehler::Sonst(format!(
                    "Hauptrechner nicht erreichbar ({})",
                    e.without_url()
                ))
            })?;
        if !antwort.status().is_success() {
            return Err(Spiegelfehler::Sonst(format!(
                "Hauptrechner liefert keinen Stand (Status {}) — Slave-Schlüssel und Turnier prüfen.",
                antwort.status().as_u16()
            )));
        }
        let m: MasterEreignisse = antwort
            .json()
            .await
            .map_err(|_| Spiegelfehler::Sonst("Stand vom Hauptrechner unlesbar".into()))?;
        if !m.uuid.eq_ignore_ascii_case(uuid) {
            return Err(Spiegelfehler::Sonst(
                "Der Hauptrechner steuert ein anderes Turnier.".into(),
            ));
        }
        let vorher = super::zustand().as_ref().map_or(0, |q| q.spiegel_n());
        let master_n = m.n;
        let r = match super::zustand().as_mut() {
            Some(q) => q.spiegel_aufnehmen(m),
            None => return Ok(()),
        };
        r.map_err(Spiegelfehler::Alt)?;
        let nachher = super::zustand().as_ref().map_or(0, |q| q.spiegel_n());
        if nachher >= master_n || nachher <= vorher {
            break;
        }
    }
    Ok(())
}

/// Abstand der Rollen-Abfrage bei badhub (`GET /turnier/lease`).
const ROLLE_TAKT: Duration = Duration::from_secs(60);
static ROLLE_ZULETZT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Fragt höchstens einmal je Minute im Hintergrund bei badhub nach der Rolle
/// dieses PCs (Standby oder nicht). Ohne Netz bleibt die gespeicherte.
fn rolle_auffrischen() {
    {
        let mut z = ROLLE_ZULETZT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if z.is_some_and(|t| t.elapsed() < ROLLE_TAKT) {
            return;
        }
        *z = Some(std::time::Instant::now());
    }
    let Some((basis, token, _, _)) = super::zustand()
        .as_ref()
        .and_then(super::KernQuelle::zugang)
    else {
        return;
    };
    tokio::spawn(async move {
        match super::api::lease_lesen(&basis, &token).await {
            Ok(sicht) => {
                if let Some(q) = super::zustand().as_mut() {
                    q.rolle_setzen(sicht.lease);
                }
            }
            Err(e) => tracing::debug!("Quelle badhub: Rolle nicht abgefragt ({e})"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schluessel_form_und_vergleich() {
        let s = neuer_schluessel().unwrap();
        assert_eq!(s.len(), 32);
        let anzeige = schluessel_anzeige(&s);
        assert_eq!(anzeige.matches('-').count(), 7);
        assert_eq!(
            schluessel_normal(&anzeige.to_uppercase()).as_deref(),
            Some(s.as_str())
        );
        assert!(schluessel_normal("zu-kurz").is_none());
        assert!(gleich_konstant(b"abc", b"abc"));
        assert!(!gleich_konstant(b"abc", b"abd"));
        assert!(!gleich_konstant(b"abc", b"ab"));
    }
}
