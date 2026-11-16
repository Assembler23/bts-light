//! Angebot, Übernahme und Rückweg eines Seiten-Pakets (Spec
//! `hotfix-ohne-neustart`, ADR 0099).
//!
//! Die App lädt ein passendes Paket im Hintergrund vor und zeigt es im
//! Banner an. **Nichts wechselt ohne Klick am Turnier-PC**: Erst „Jetzt
//! übernehmen" legt das Paket ab und tauscht den aktiven Seitenstand; „Zurück
//! zum vorigen Stand" nimmt es wieder weg und sperrt die Folge.
//!
//! Der Tausch berührt keine Verbindung. Tablets bemerken die neue Marke am
//! nächsten Lebenszeichen und laden neu, sobald kein Spiel läuft.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, ConnectionMode};
use crate::seiten_paket::{self, Ablage, Erwartung, MAX_PAKET_BYTES};
use crate::signatur::Schluessel;
use crate::tablet::assets;

/// Fester Ort der Pakete — derselbe Server und Pfad wie der Updater. Die
/// Lade-Adresse baut die App **selbst** aus der eigenen Version und der Zahl
/// aus dem Index; der Index kann sie nirgendwohin umlenken.
const BASIS: &str = "https://badhub.de/download/bts-light";

/// Obergrenze für Index und Signatur — beides sind wenige hundert Bytes.
const MAX_KLEIN_BYTES: usize = 64 * 1024;

/// Wen ein Seiten-Tausch an diesem PC erreicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reichweite {
    /// Reiner Cloud-Betrieb: Die Seiten kommen vom Relay, nicht von hier.
    Keine,
    /// Alle Geräte hängen an diesem PC.
    Lan,
    /// Geräte im Hallennetz ja, Geräte über die Cloud nein.
    LanUndCloud,
}

impl Reichweite {
    fn als_text(self) -> &'static str {
        match self {
            Reichweite::Keine => "keine",
            Reichweite::Lan => "lan",
            Reichweite::LanUndCloud => "lan+cloud",
        }
    }
}

/// Liefert dieser PC als Slave-Träger die Seiten selbst aus (ADR 0048)?
pub fn ist_slave_traeger(cfg: &AppConfig) -> bool {
    cfg.slave_mode && !cfg.master_namespace.trim().is_empty() && cfg.slave_mux.enabled
}

/// Bestimmt die Reichweite aus der Verbindungsart. Ein Slave-Träger liefert
/// seine Seiten selbst aus — auch wenn er „Cloud" eingestellt hat.
pub fn reichweite(modus: ConnectionMode, slave_traeger: bool) -> Reichweite {
    match modus {
        ConnectionMode::Lan => Reichweite::Lan,
        ConnectionMode::LanAndCloud => Reichweite::LanUndCloud,
        ConnectionMode::Cloud if slave_traeger => Reichweite::Lan,
        ConnectionMode::Cloud => Reichweite::Keine,
    }
}

#[derive(Deserialize)]
struct Index {
    format: u64,
    #[serde(default)]
    pakete: Vec<IndexEintrag>,
}

#[derive(Deserialize)]
struct IndexEintrag {
    kern: String,
    folge: u32,
}

/// Der Index ließ sich nicht lesen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexUnlesbar;

/// Die Folge, die der Index für **genau diese** Kern-Version nennt.
///
/// `Err` = der Index ist unlesbar (kein JSON, fremdes Format — etwa die
/// Anmeldeseite eines Hallen-WLANs mit Status 200). `Ok(None)` = lesbar, aber
/// ohne Eintrag für diese Version. Der Unterschied zählt: Nur ein **lesbarer**
/// Index darf ein Angebot zurückziehen.
///
/// Der Index ist unsigniert. Er kann deshalb nur zweierlei bewirken: dass ein
/// Paket geladen wird (das dann an der Signatur geprüft wird) oder dass
/// keines geladen wird. Mehr als eine Zahl wird ihm nicht geglaubt.
pub fn folge_aus_index(json: &[u8], kern: &str) -> Result<Option<u32>, IndexUnlesbar> {
    let index: Index = serde_json::from_slice(json).map_err(|_| IndexUnlesbar)?;
    if index.format != 1 {
        return Err(IndexUnlesbar);
    }
    Ok(index
        .pakete
        .iter()
        .filter(|p| p.kern == kern)
        .map(|p| p.folge)
        .max())
}

/// Soll die im Index genannte Folge angeboten werden? Nur eine höhere als
/// die aktive, und keine zurückgenommene.
pub fn anzubieten(index_folge: Option<u32>, aktive_folge: u32, gesperrt: &[u32]) -> Option<u32> {
    index_folge.filter(|f| *f > aktive_folge && !gesperrt.contains(f))
}

fn index_url() -> String {
    format!("{BASIS}/seiten.json")
}

fn paket_url(kern: &str, folge: u32) -> String {
    format!("{BASIS}/seiten/seiten-{kern}-{folge}.btsp")
}

/// Ein geladenes, geprüftes, aber noch nicht übernommenes Paket.
struct Bereit {
    folge: u32,
    bytes: Vec<u8>,
    signatur: String,
}

/// Was das Banner nach einer Aktion einmalig meldet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Meldung {
    Keine,
    Uebernommen { folge: u32 },
    Zurueckgenommen { folge: u32 },
}

#[derive(Default)]
struct Innen {
    bereit: Option<Bereit>,
    meldung: Option<Meldung>,
    /// Die zuletzt abgelehnte Folge samt Grund. Solange der Index dieselbe
    /// Folge nennt, wird sie **nicht noch einmal geladen** — sonst kostete
    /// ein kaputtes Paket alle zwanzig Minuten bis zu 8 MB im Hallennetz.
    abgelehnt: Option<(u32, String)>,
    /// Läuft gerade ein Prüflauf? Verhindert doppeltes Laden.
    laeuft: bool,
}

/// Was der Index-Abruf ergab.
pub enum Abruf {
    /// Kein Netz, Zeitüberschreitung, Server-Fehler.
    NetzWeg,
    /// Der Index, wie er kam.
    Index(Vec<u8>),
}

/// Anzeige-Stand der Seiten für das Frontend (Teil von `update_info`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeitenInfo {
    /// Aktiver Seiten-Stand; 0 = eingebaut.
    pub aktive_folge: u32,
    /// Folge eines bereitliegenden Pakets.
    pub bereit_folge: Option<u32>,
    /// `keine` | `uebernommen` | `zurueckgenommen`.
    pub meldung: String,
    /// Folge, auf die sich die Meldung bezieht.
    pub meldung_folge: Option<u32>,
    /// `keine` | `lan` | `lan+cloud`.
    pub reichweite: String,
    /// Dieser PC ist Teil eines Verbunds mehrerer PCs (Slave).
    pub verbund: bool,
    /// Grund der letzten Ablehnung (nur Wartungsseite).
    pub abgelehnt: Option<String>,
    /// Gibt es einen Stand, zu dem „Zurück" führt?
    pub rueckweg: bool,
}

/// Zustand des Seiten-Angebots. Lebt im `AppState`.
#[derive(Default)]
pub struct SeitenAngebot {
    innen: Mutex<Innen>,
    /// Hält Übernehmen und Zurück auseinander. Getrennt von `innen`, damit
    /// die Datei-Arbeit (Prüfen, Schreiben, fsync) das Lesen des Stands fürs
    /// Banner nicht aufhält.
    aktion: Mutex<()>,
}

/// Ein laufender Prüflauf. Fällt er weg — auch durch einen Abbruch oder eine
/// Panik mitten im Laden —, ist der Weg für den nächsten wieder frei.
pub struct Lauf<'a>(&'a SeitenAngebot);

impl Drop for Lauf<'_> {
    fn drop(&mut self) {
        self.0.innen().laeuft = false;
    }
}

impl SeitenAngebot {
    fn innen(&self) -> std::sync::MutexGuard<'_, Innen> {
        self.innen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Beginnt einen Prüflauf; `None`, wenn schon einer läuft.
    pub fn beginne(&self) -> Option<Lauf<'_>> {
        let mut i = self.innen();
        if i.laeuft {
            return None;
        }
        i.laeuft = true;
        Some(Lauf(self))
    }

    pub fn bereit_folge(&self) -> Option<u32> {
        self.innen().bereit.as_ref().map(|b| b.folge)
    }

    /// Wertet den Index aus. Rückgabe: die Folge, die jetzt zu laden ist.
    ///
    /// - Netz weg **oder Index unlesbar**: nichts ändert sich — ein
    ///   bereitliegendes Paket bleibt (offline ist in der Halle der
    ///   Normalfall, und die Anmeldeseite eines WLANs ist kein Index).
    /// - Der lesbare Index nennt das bereitliegende Paket nicht mehr
    ///   (zurückgezogen): Es wird verworfen.
    /// - Die genannte Folge wurde schon einmal abgelehnt: nicht noch einmal
    ///   laden.
    pub fn index_auswerten(
        &self,
        abruf: Abruf,
        kern: &str,
        aktive_folge: u32,
        gesperrt: &[u32],
    ) -> Option<u32> {
        let Abruf::Index(json) = abruf else {
            return None;
        };
        let Ok(index_folge) = folge_aus_index(&json, kern) else {
            return None;
        };
        let angebot = anzubieten(index_folge, aktive_folge, gesperrt);
        let mut i = self.innen();
        if i.abgelehnt.as_ref().map(|(f, _)| *f) != angebot {
            // Der Index nennt etwas anderes als das Abgelehnte: Der alte
            // Grund sagt nichts mehr.
            i.abgelehnt = None;
        }
        match (angebot, i.bereit.as_ref().map(|b| b.folge)) {
            (Some(neu), Some(da)) if neu == da => None,
            (Some(_), _) if i.abgelehnt.is_some() => None,
            (Some(neu), _) => Some(neu),
            (None, _) => {
                i.bereit = None;
                None
            }
        }
    }

    /// Prüft ein geladenes Paket und legt es als „bereit" in den Speicher —
    /// **nicht** auf die Platte und nicht aktiv. Ein abgelehntes Paket
    /// erzeugt kein Banner, nur einen Grund für die Wartungsseite.
    ///
    /// `index_folge` ist die Zahl, unter der das Paket geladen wurde. Das
    /// Paket muss **dieselbe** Folge tragen: Sonst ließe sich unter einer
    /// hohen Zahl im (unsignierten) Index ein altes, echt signiertes Paket
    /// unterschieben. Die aktive Folge wird hier frisch gelesen — zwischen
    /// Index-Abruf und Ende des Ladens kann jemand geklickt haben.
    pub fn geladen(
        &self,
        index_folge: u32,
        bytes: Vec<u8>,
        signatur: String,
        schluessel: &Schluessel,
        kern: &str,
        gesperrt: &[u32],
    ) -> bool {
        let erwartung = Erwartung {
            kern,
            aktive_folge: assets::aktiv().folge(),
            gesperrt,
        };
        let grund = match seiten_paket::pruefen(&bytes, &signatur, schluessel, &erwartung) {
            Ok(paket) if paket.folge == index_folge => {
                tracing::info!(
                    "Seiten-Paket Folge {} geladen, wartet auf die Übernahme",
                    paket.folge
                );
                let mut i = self.innen();
                i.bereit = Some(Bereit {
                    folge: paket.folge,
                    bytes,
                    signatur,
                });
                i.abgelehnt = None;
                return true;
            }
            Ok(paket) => format!(
                "Paket trägt Folge {} statt der angekündigten {index_folge}",
                paket.folge
            ),
            Err(grund) => grund.to_string(),
        };
        tracing::warn!("Seiten-Paket abgelehnt, nichts geändert: {grund}");
        self.innen().abgelehnt = Some((index_folge, grund));
        false
    }

    /// „Jetzt übernehmen": legt das bereitliegende Paket ab und macht es zum
    /// aktiven Seitenstand. Rückgabe: die neue Folge.
    ///
    /// Erst die Platte, dann der Tausch: Scheitert das Ablegen (Platte
    /// voll), bleibt der aktive Stand, wie er war, und das Paket bleibt
    /// bereit für einen zweiten Versuch.
    pub fn uebernehmen(
        &self,
        ablage: &Ablage,
        kern: &str,
        schluessel: &Schluessel,
    ) -> Result<u32, String> {
        let _aktion = self
            .aktion
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        // Das Paket für die Dauer der Datei-Arbeit herausnehmen, ohne den
        // Stand fürs Banner zu sperren.
        let bereit = self
            .innen()
            .bereit
            .take()
            .ok_or("Es liegt kein Seiten-Paket bereit.")?;
        let aktive_folge = assets::aktiv().folge();
        let ergebnis = ablage.ablegen(
            kern,
            aktive_folge,
            schluessel,
            &bereit.bytes,
            &bereit.signatur,
        );
        let mut i = self.innen();
        match ergebnis {
            Ok(paket) => {
                let folge = paket.folge;
                assets::tauschen(Arc::new(paket.stand));
                tracing::info!(
                    "Seiten-Paket Folge {folge} übernommen (vorher Folge {aktive_folge}, Commit {})",
                    paket.commit
                );
                i.meldung = Some(Meldung::Uebernommen { folge });
                i.abgelehnt = None;
                Ok(folge)
            }
            Err(e) => {
                tracing::warn!("Seiten-Paket nicht übernommen: {e}");
                // Zurücklegen — außer inzwischen liegt schon ein neueres da.
                if i.bereit.is_none() {
                    i.bereit = Some(bereit);
                }
                Err(format!("Die Seiten wurden nicht geändert ({e})."))
            }
        }
    }

    /// „Zurück zum vorigen Stand": sperrt die aktive Folge und aktiviert das
    /// vorige Paket oder den eingebauten Stand. Rückgabe: die jetzt aktive
    /// Folge.
    pub fn zurueck(
        &self,
        ablage: &Ablage,
        kern: &str,
        schluessel: &Schluessel,
    ) -> Result<u32, String> {
        let _aktion = self
            .aktion
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        let zurueckgenommen = assets::aktiv().folge();
        if zurueckgenommen == 0 {
            return Err("Es ist kein Seiten-Paket aktiv.".into());
        }
        let voriges = ablage
            .zurueck(kern, zurueckgenommen, schluessel)
            .map_err(|e| {
                tracing::warn!("Seiten-Rückweg gescheitert: {e}");
                format!("Die Seiten wurden nicht geändert ({e}).")
            })?;
        let stand = match voriges {
            Some(paket) => Arc::new(paket.stand),
            None => assets::eingebauter_stand(),
        };
        let jetzt = stand.folge();
        assets::tauschen(stand);
        tracing::info!(
            "Seiten-Paket Folge {zurueckgenommen} zurückgenommen und gesperrt, aktiv ist Folge {jetzt}"
        );
        let mut i = self.innen();
        i.meldung = Some(Meldung::Zurueckgenommen {
            folge: zurueckgenommen,
        });
        i.abgelehnt = None;
        Ok(jetzt)
    }

    /// Schließt die einmalige Meldung im Banner.
    pub fn meldung_weg(&self) {
        self.innen().meldung = None;
    }

    /// Der Stand fürs Frontend. Reichweite und Verbund reicht der Aufrufer
    /// herein — so muss hier nicht bei jedem Lese-Takt die ganze
    /// Konfiguration kopiert werden.
    pub fn info(&self, reichweite: Reichweite, verbund: bool) -> SeitenInfo {
        let i = self.innen();
        let aktive_folge = assets::aktiv().folge();
        let (meldung, meldung_folge) = match i.meldung.unwrap_or(Meldung::Keine) {
            Meldung::Keine => ("keine", None),
            Meldung::Uebernommen { folge } => ("uebernommen", Some(folge)),
            Meldung::Zurueckgenommen { folge } => ("zurueckgenommen", Some(folge)),
        };
        SeitenInfo {
            aktive_folge,
            // Im reinen Cloud-Betrieb gibt es kein Angebot — die Seiten
            // dieses PCs sieht dort niemand.
            bereit_folge: match reichweite {
                Reichweite::Keine => None,
                _ => i.bereit.as_ref().map(|b| b.folge),
            },
            meldung: meldung.to_string(),
            meldung_folge,
            reichweite: reichweite.als_text().to_string(),
            verbund,
            abgelehnt: i.abgelehnt.as_ref().map(|(_, grund)| grund.clone()),
            rueckweg: aktive_folge > 0,
        }
    }

    /// Wie [`Self::info`], Reichweite und Verbund aus der Konfiguration.
    pub fn info_fuer(&self, cfg: &AppConfig) -> SeitenInfo {
        self.info(
            reichweite(cfg.connection_mode, ist_slave_traeger(cfg)),
            cfg.slave_mode,
        )
    }
}

/// Holt eine Datei mit Größendeckel. `None` bei jedem Fehler.
async fn holen(client: &reqwest::Client, url: &str, deckel: usize) -> Option<Vec<u8>> {
    let mut antwort = client.get(url).send().await.ok()?;
    if !antwort.status().is_success() {
        return None;
    }
    // Nur eine Abkürzung; maßgeblich ist die Zählung beim Lesen unten.
    if antwort.content_length().is_some_and(|n| n > deckel as u64) {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(stueck) = antwort.chunk().await.ok()? {
        if bytes.len() + stueck.len() > deckel {
            return None;
        }
        bytes.extend_from_slice(&stueck);
    }
    Some(bytes)
}

/// Ein Prüflauf: Index holen, bei Bedarf Paket und Signatur laden, prüfen,
/// bereitlegen. Still bei jedem Netzfehler. Ändert weder die Platte noch den
/// aktiven Stand.
pub async fn pruefen_und_laden(
    angebot: &SeitenAngebot,
    reichweite: Reichweite,
    gesperrt: Vec<u32>,
    schluessel: &Schluessel,
) {
    if reichweite == Reichweite::Keine {
        return;
    }
    let Some(_lauf) = angebot.beginne() else {
        return;
    };
    let kern = env!("CARGO_PKG_VERSION");
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        // Großzügig für das Paket über ein langsames Hallennetz; der Lauf
        // hält nichts auf, er läuft im Hintergrund.
        .timeout(std::time::Duration::from_secs(90))
        // Keine Weiterleitung: Die Adresse ist fest, ein Umweg wäre nie
        // gewollt.
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return;
    };
    let abruf = match holen(&client, &index_url(), MAX_KLEIN_BYTES).await {
        Some(json) => Abruf::Index(json),
        None => Abruf::NetzWeg,
    };
    let aktive_folge = assets::aktiv().folge();
    let Some(folge) = angebot.index_auswerten(abruf, kern, aktive_folge, &gesperrt) else {
        return;
    };
    let url = paket_url(kern, folge);
    let Some(bytes) = holen(&client, &url, MAX_PAKET_BYTES).await else {
        return;
    };
    let Some(signatur) = holen(&client, &format!("{url}.sig"), MAX_KLEIN_BYTES).await else {
        return;
    };
    let Ok(signatur) = String::from_utf8(signatur) else {
        return;
    };
    angebot.geladen(folge, bytes, signatur, schluessel, kern, &gesperrt);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seiten_paket::testhilfe::{paket_bytes, KERN};
    use crate::signatur::testhilfe::Testschluessel;

    fn index(eintraege: &[(&str, u32)]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "format": 1,
            "pakete": eintraege
                .iter()
                .map(|(k, f)| serde_json::json!({"kern": k, "folge": f}))
                .collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn cfg(modus: ConnectionMode) -> AppConfig {
        AppConfig {
            connection_mode: modus,
            ..AppConfig::default()
        }
    }

    /// Lädt ein gültiges Paket der Folge ins Angebot.
    fn lade(angebot: &SeitenAngebot, schluessel: &Testschluessel, folge: u32) {
        let bytes = paket_bytes(folge);
        let sig = schluessel.signieren(&bytes);
        assert!(angebot.geladen(folge, bytes, sig, &schluessel.oeffentlich(), KERN, &[]));
    }

    fn stand(angebot: &SeitenAngebot, modus: ConnectionMode) -> SeitenInfo {
        angebot.info_fuer(&cfg(modus))
    }

    #[test]
    fn angebot_nur_bei_gleicher_kern_version() {
        let json = index(&[("1.2.2", 9), (KERN, 2), ("1.2.4", 7)]);
        assert_eq!(folge_aus_index(&json, KERN), Ok(Some(2)));
        assert_eq!(folge_aus_index(&json, "1.2.5"), Ok(None));
        // Kein „ab Version": Ein Paket für eine ältere oder neuere Version
        // wird nie angeboten.
        assert_eq!(
            folge_aus_index(&index(&[("1.2.2", 9), ("1.2.4", 7)]), KERN),
            Ok(None)
        );
        // Unlesbar ist etwas anderes als „kein Eintrag".
        for kaputt in [
            &b"kein json"[..],
            b"{}",
            br#"{"format":2,"pakete":[]}"#,
            b"",
            b"<html>Bitte im WLAN anmelden</html>",
        ] {
            assert_eq!(folge_aus_index(kaputt, KERN), Err(IndexUnlesbar));
        }
    }

    /// Die Anmeldeseite eines Hallen-WLANs kommt mit Status 200 — sie darf
    /// ein schon geladenes Paket nicht aus dem Banner werfen.
    #[test]
    fn unlesbarer_index_laesst_bereites_paket() {
        let schluessel = Testschluessel::neu(1);
        let angebot = SeitenAngebot::default();
        lade(&angebot, &schluessel, 2);
        let portal = Abruf::Index(b"<html>Bitte anmelden</html>".to_vec());
        assert_eq!(angebot.index_auswerten(portal, KERN, 0, &[]), None);
        assert_eq!(angebot.bereit_folge(), Some(2));
    }

    /// Unter einer hohen Zahl im (unsignierten) Index ein altes, echt
    /// signiertes Paket: abgelehnt, weil die Folge im Paket nicht passt.
    #[test]
    fn paket_mit_anderer_folge_als_im_index_abgelehnt() {
        let schluessel = Testschluessel::neu(1);
        let angebot = SeitenAngebot::default();
        let bytes = paket_bytes(2);
        let sig = schluessel.signieren(&bytes);
        assert!(!angebot.geladen(9, bytes, sig, &schluessel.oeffentlich(), KERN, &[]));
        assert_eq!(angebot.bereit_folge(), None);
        let grund = stand(&angebot, ConnectionMode::Lan).abgelehnt.unwrap();
        assert!(
            grund.contains("Folge 2 statt der angekündigten 9"),
            "{grund}"
        );
    }

    /// Ein abgelehntes Paket wird nicht bei jedem Lauf neu geladen. Erst
    /// wenn der Index eine andere Folge nennt, geht es weiter.
    #[test]
    fn abgelehnte_folge_wird_nicht_erneut_geladen() {
        let eigener = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let angebot = SeitenAngebot::default();
        let nennt = |f| Abruf::Index(index(&[(KERN, f)]));
        assert_eq!(angebot.index_auswerten(nennt(2), KERN, 0, &[]), Some(2));
        let bytes = paket_bytes(2);
        let sig = fremder.signieren(&bytes);
        assert!(!angebot.geladen(2, bytes, sig, &eigener.oeffentlich(), KERN, &[]));

        assert_eq!(angebot.index_auswerten(nennt(2), KERN, 0, &[]), None);
        assert_eq!(angebot.index_auswerten(nennt(2), KERN, 0, &[]), None);
        assert!(stand(&angebot, ConnectionMode::Lan).abgelehnt.is_some());

        // Eine neue Folge: wieder laden, der alte Grund ist erledigt.
        assert_eq!(angebot.index_auswerten(nennt(3), KERN, 0, &[]), Some(3));
        assert!(stand(&angebot, ConnectionMode::Lan).abgelehnt.is_none());
    }

    /// Zwischen Index-Abruf und Ende des Ladens hat jemand geklickt: Das
    /// Paket wird gegen den JETZT aktiven Stand geprüft.
    #[test]
    fn geladen_prueft_gegen_den_jetzt_aktiven_stand() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        let angebot = SeitenAngebot::default();
        let eingebaut = assets::aktiv();
        lade(&angebot, &schluessel, 4);
        angebot
            .uebernehmen(&ablage, KERN, &schluessel.oeffentlich())
            .unwrap();

        let bytes = paket_bytes(3);
        let sig = schluessel.signieren(&bytes);
        assert!(!angebot.geladen(3, bytes, sig, &schluessel.oeffentlich(), KERN, &[]));
        assert_eq!(angebot.bereit_folge(), None);
        assets::tauschen(eingebaut);
    }

    #[test]
    fn gesperrte_folge_nicht_angeboten_hoehere_schon() {
        assert_eq!(anzubieten(Some(3), 0, &[3]), None);
        assert_eq!(anzubieten(Some(4), 0, &[3]), Some(4));
        // Nicht neuer als aktiv: kein Angebot.
        assert_eq!(anzubieten(Some(2), 2, &[]), None);
        assert_eq!(anzubieten(Some(1), 2, &[]), None);
        assert_eq!(anzubieten(Some(3), 2, &[]), Some(3));
        assert_eq!(anzubieten(None, 0, &[]), None);
    }

    #[test]
    fn netz_weg_laesst_bereites_paket() {
        let schluessel = Testschluessel::neu(1);
        let angebot = SeitenAngebot::default();
        lade(&angebot, &schluessel, 2);

        assert_eq!(angebot.index_auswerten(Abruf::NetzWeg, KERN, 0, &[]), None);
        assert_eq!(angebot.bereit_folge(), Some(2), "offline ändert nichts");

        // Dieselbe Folge wird nicht noch einmal geladen.
        let gleich = Abruf::Index(index(&[(KERN, 2)]));
        assert_eq!(angebot.index_auswerten(gleich, KERN, 0, &[]), None);
        assert_eq!(angebot.bereit_folge(), Some(2));

        // Eine höhere schon.
        let hoeher = Abruf::Index(index(&[(KERN, 3)]));
        assert_eq!(angebot.index_auswerten(hoeher, KERN, 0, &[]), Some(3));

        // Der Index nennt die Version nicht mehr: Angebot zurückgezogen.
        let leer = Abruf::Index(index(&[]));
        assert_eq!(angebot.index_auswerten(leer, KERN, 0, &[]), None);
        assert_eq!(angebot.bereit_folge(), None);
    }

    #[test]
    fn abgelehntes_paket_wird_nicht_angeboten_aber_begruendet() {
        let eigener = Testschluessel::neu(1);
        let fremder = Testschluessel::neu(2);
        let angebot = SeitenAngebot::default();
        let bytes = paket_bytes(1);
        let sig = fremder.signieren(&bytes);
        assert!(!angebot.geladen(1, bytes, sig, &eigener.oeffentlich(), KERN, &[]));
        let info = stand(&angebot, ConnectionMode::Lan);
        assert_eq!(info.bereit_folge, None);
        assert!(info.abgelehnt.unwrap().contains("Signatur"));
    }

    #[test]
    fn uebernehmen_aktiviert_und_merkt_vorher() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        let angebot = SeitenAngebot::default();
        let eingebaut = assets::aktiv();
        assert!(angebot
            .uebernehmen(&ablage, KERN, &schluessel.oeffentlich())
            .is_err());

        lade(&angebot, &schluessel, 1);
        assert_eq!(assets::aktiv().folge(), 0, "Laden allein wechselt nichts");
        assert_eq!(
            angebot.uebernehmen(&ablage, KERN, &schluessel.oeffentlich()),
            Ok(1)
        );
        assert_eq!(assets::aktiv().folge(), 1);
        assert_ne!(assets::aktiv().marke(), eingebaut.marke());

        let info = stand(&angebot, ConnectionMode::Lan);
        assert_eq!(info.aktive_folge, 1);
        assert_eq!(info.bereit_folge, None);
        assert_eq!(info.meldung, "uebernommen");
        assert_eq!(info.meldung_folge, Some(1));
        assert!(info.rueckweg);

        // Zweites Paket: Das erste liegt als „vorher" und überlebt den Neustart.
        lade(&angebot, &schluessel, 2);
        assert_eq!(
            angebot.uebernehmen(&ablage, KERN, &schluessel.oeffentlich()),
            Ok(2)
        );
        assert_eq!(
            ablage
                .beim_start_laden(KERN, &schluessel.oeffentlich())
                .map(|p| p.folge),
            Some(2)
        );
        assert!(
            dir.path().join("vorher.paket").is_file(),
            "das erste Paket liegt als voriges"
        );
        angebot.meldung_weg();
        assert_eq!(stand(&angebot, ConnectionMode::Lan).meldung, "keine");

        assets::tauschen(eingebaut);
    }

    #[test]
    fn zurueck_sperrt_die_folge() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        let ablage = Ablage::neu(dir.path());
        let angebot = SeitenAngebot::default();
        let eingebaut = assets::aktiv();
        assert!(angebot
            .zurueck(&ablage, KERN, &schluessel.oeffentlich())
            .is_err());

        lade(&angebot, &schluessel, 1);
        angebot
            .uebernehmen(&ablage, KERN, &schluessel.oeffentlich())
            .unwrap();
        lade(&angebot, &schluessel, 2);
        angebot
            .uebernehmen(&ablage, KERN, &schluessel.oeffentlich())
            .unwrap();

        // Zurück von 2 auf 1.
        assert_eq!(
            angebot.zurueck(&ablage, KERN, &schluessel.oeffentlich()),
            Ok(1)
        );
        assert_eq!(assets::aktiv().folge(), 1);
        assert_eq!(ablage.gesperrt(KERN), vec![2]);
        let info = stand(&angebot, ConnectionMode::Lan);
        assert_eq!(info.meldung, "zurueckgenommen");
        assert_eq!(info.meldung_folge, Some(2));

        // Die zurückgenommene Folge kommt nicht wieder — auch wenn der Index
        // sie weiter nennt. Eine höhere schon.
        let gesperrt = ablage.gesperrt(KERN);
        let wieder = Abruf::Index(index(&[(KERN, 2)]));
        assert_eq!(angebot.index_auswerten(wieder, KERN, 1, &gesperrt), None);
        let hoeher = Abruf::Index(index(&[(KERN, 3)]));
        assert_eq!(angebot.index_auswerten(hoeher, KERN, 1, &gesperrt), Some(3));

        // Noch einmal zurück: eingebauter Stand, alte Marke.
        assert_eq!(
            angebot.zurueck(&ablage, KERN, &schluessel.oeffentlich()),
            Ok(0)
        );
        assert_eq!(assets::aktiv().marke(), eingebaut.marke());
        assert!(!stand(&angebot, ConnectionMode::Lan).rueckweg);

        assets::tauschen(eingebaut);
    }

    /// Scheitert das Ablegen, bleibt der aktive Stand und das Paket bleibt
    /// bereit — die Turnierleitung kann es noch einmal versuchen.
    #[test]
    fn gescheitertes_ablegen_wechselt_nichts() {
        let dir = tempfile::tempdir().unwrap();
        let schluessel = Testschluessel::neu(1);
        // Das „Verzeichnis" der Ablage ist eine Datei: Ablegen scheitert.
        let blockiert = dir.path().join("seiten");
        std::fs::write(&blockiert, b"x").unwrap();
        let ablage = Ablage::neu(&blockiert);
        let angebot = SeitenAngebot::default();
        lade(&angebot, &schluessel, 1);

        let fehler = angebot
            .uebernehmen(&ablage, KERN, &schluessel.oeffentlich())
            .unwrap_err();
        assert!(fehler.contains("nicht geändert"), "{fehler}");
        assert_eq!(assets::aktiv().folge(), 0);
        assert_eq!(angebot.bereit_folge(), Some(1));
        assert_eq!(stand(&angebot, ConnectionMode::Lan).meldung, "keine");
    }

    #[test]
    fn reine_cloud_bietet_nichts_an() {
        assert_eq!(reichweite(ConnectionMode::Cloud, false), Reichweite::Keine);
        assert_eq!(reichweite(ConnectionMode::Lan, false), Reichweite::Lan);
        assert_eq!(
            reichweite(ConnectionMode::LanAndCloud, false),
            Reichweite::LanUndCloud
        );

        // Auch ein schon geladenes Paket erscheint im Cloud-Betrieb nicht.
        let schluessel = Testschluessel::neu(1);
        let angebot = SeitenAngebot::default();
        lade(&angebot, &schluessel, 1);
        let info = stand(&angebot, ConnectionMode::Cloud);
        assert_eq!(info.reichweite, "keine");
        assert_eq!(info.bereit_folge, None);
        assert_eq!(
            stand(&angebot, ConnectionMode::LanAndCloud).reichweite,
            "lan+cloud"
        );
        assert_eq!(stand(&angebot, ConnectionMode::Lan).bereit_folge, Some(1));
    }

    #[test]
    fn slave_modus_bietet_an() {
        // Ein Slave-Träger liefert seine Seiten selbst aus — auch mit
        // Verbindungsart „Cloud".
        assert_eq!(reichweite(ConnectionMode::Cloud, true), Reichweite::Lan);

        let mut c = cfg(ConnectionMode::Cloud);
        c.slave_mode = true;
        c.master_namespace = "abc".into();
        c.slave_mux.enabled = true;
        assert!(ist_slave_traeger(&c));
        let info = SeitenAngebot::default().info_fuer(&c);
        assert_eq!(info.reichweite, "lan");
        assert!(info.verbund);

        // Ohne Bündelung leitet der Slave nur um: kein Träger.
        c.slave_mux.enabled = false;
        assert!(!ist_slave_traeger(&c));
        c.slave_mux.enabled = true;
        c.master_namespace = "  ".into();
        assert!(!ist_slave_traeger(&c));
    }

    #[test]
    fn nur_ein_prueflauf_zur_zeit() {
        let angebot = SeitenAngebot::default();
        let lauf = angebot.beginne().expect("erster Lauf");
        assert!(angebot.beginne().is_none());
        // Auch ein abgebrochener Lauf gibt den Weg wieder frei.
        drop(lauf);
        assert!(angebot.beginne().is_some());
    }

    #[test]
    fn adressen_entstehen_aus_fester_basis_version_und_zahl() {
        assert_eq!(
            index_url(),
            "https://badhub.de/download/bts-light/seiten.json"
        );
        assert_eq!(
            paket_url("0.9.432", 3),
            "https://badhub.de/download/bts-light/seiten/seiten-0.9.432-3.btsp"
        );
    }
}
