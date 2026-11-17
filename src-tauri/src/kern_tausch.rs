//! Kernwechsel ohne Installer: Vorladen der neuen Programmdatei und der
//! Stand fürs Banner (Spec `hotfix-ohne-neustart`, ADR 0100).
//!
//! Liegt ein Update bereit, lädt die App zusätzlich die nackte
//! Programmdatei, prüft sie gegen ihren signierten Kopf, legt sie als
//! `bts-light.neu.exe` neben sich und lässt sie zur Probe laufen. Erst wenn
//! das alles gelungen ist, bietet das Banner „Jetzt wechseln" an. Scheitert
//! irgendein Schritt, bleibt es still beim bisherigen Installer-Weg — der
//! Grund steht im Log.
//!
//! Der Wechsel selbst (`commands::kern_wechseln`) nutzt die Bausteine aus
//! [`crate::tausch`].

use std::io::Write;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::signatur::Schluessel;
use crate::tausch::{self, Lage, Pfade};
use crate::uebergabe::{self, KernKopf, Version};

/// Fester Ort der Kern-Artefakte — derselbe Server und Pfad wie der Updater.
/// Die Lade-Adressen baut die App selbst aus der Versionsnummer.
const BASIS: &str = "https://badhub.de/download/bts-light";

/// Obergrenze für Index und Signatur.
const MAX_KLEIN_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
struct Index {
    format: u64,
    version: String,
}

/// Die Version, die der (unsignierte) Index nennt. Er kann nur bewirken,
/// dass ein Kopf geladen wird oder nicht — geglaubt wird erst dem
/// signierten Kopf.
pub fn index_version(json: &[u8]) -> Option<Version> {
    let index: Index = serde_json::from_slice(json).ok()?;
    (index.format == 1)
        .then(|| Version::lesen(&index.version))
        .flatten()
}

fn index_url() -> String {
    format!("{BASIS}/kern.json")
}

fn kopf_url(version: Version) -> String {
    format!("{BASIS}/kern/kern-{version}.json")
}

fn exe_url(version: Version) -> String {
    format!("{BASIS}/kern/bts-light-{version}.exe")
}

/// Was der Kernwechsel dem Banner sagt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Stand {
    /// Nichts vorgeladen (kein Update, noch nicht versucht).
    #[default]
    Aus,
    /// Kopf und Programmdatei werden geladen und geprüft.
    Laedt,
    /// Die neue Programmdatei liegt geprüft daneben und ist zur Probe
    /// gelaufen.
    Bereit {
        version: String,
        /// Erlaubt die neue Version einen Rückweg zur jetzt laufenden?
        rueckweg: bool,
    },
    /// Der Tausch geht hier nicht — es bleibt beim Installer-Weg.
    NichtMoeglich { grund: String },
    /// Der Wechsel läuft; die App endet gleich.
    Wechselt { version: String, rueckweg: bool },
}

/// Warum das Vorladen scheiterte — und ob ein späterer Versuch lohnt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hindernis {
    /// Liegt am Rechner oder an der Datei selbst: nicht Windows, kein
    /// Schreibrecht, Kopf oder Programmdatei abgelehnt, Probelauf
    /// gescheitert. Ein neuer Versuch brächte dasselbe.
    Dauerhaft(String),
    /// Netz weg, Zeitüberschreitung, Artefakt noch nicht veröffentlicht (der
    /// Release-Job legt es erst nach `latest.json` ab). Später noch einmal.
    Voruebergehend(String),
}

impl Hindernis {
    pub fn text(&self) -> &str {
        match self {
            Hindernis::Dauerhaft(t) | Hindernis::Voruebergehend(t) => t,
        }
    }
}

/// So oft wird das Vorladen einer Version nach einem vorübergehenden
/// Hindernis höchstens versucht — je Prüflauf (rund alle 20 Minuten) einmal.
pub const MAX_VERSUCHE: u32 = 4;

/// Einmalige Meldung nach einem Wechsel, aus `uebergabe.json` gelesen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meldung {
    /// Diese Version kam über einen Kernwechsel und läuft.
    Gewechselt { von: String },
    /// Die neue Version lief nicht an; die alte läuft wieder.
    Zurueckgefallen { nach: String, grund: String },
    /// Diese (ältere) Version läuft wieder, weil `version` auf Wunsch
    /// zurückgenommen wurde.
    Zurueckgenommen { version: String },
}

/// Leitet die Meldung aus dem Zustand der letzten Übergabe ab — nur wenn sie
/// **diese** Version betrifft und noch nicht geschlossen wurde.
pub fn meldung_aus(zustand: Option<&tausch::Zustand>, eigene: &str) -> Option<Meldung> {
    let z = zustand?;
    if z.gesehen {
        return None;
    }
    match z.lage {
        // Von einer neueren Version hierher: Das war der Rückweg.
        Lage::Angekommen if ruecknahme_aus(zustand, eigene).is_some() => {
            Some(Meldung::Zurueckgenommen {
                version: z.von.clone(),
            })
        }
        Lage::Angekommen if z.nach == eigene => Some(Meldung::Gewechselt { von: z.von.clone() }),
        Lage::Zurueckgefallen if z.von == eigene => Some(Meldung::Zurueckgefallen {
            nach: z.nach.clone(),
            grund: z.grund.clone().unwrap_or_default(),
        }),
        _ => None,
    }
}

/// Kam diese Version über einen Rückweg an, nennt das die zurückgenommene —
/// unabhängig davon, ob die Meldung dazu schon geschlossen ist. Erst die
/// Ankunft beweist den Rückweg: Vorher wird nichts gesperrt, sonst bliebe
/// nach einem Rückfall die wieder laufende Version gesperrt.
pub fn ruecknahme_aus(zustand: Option<&tausch::Zustand>, eigene: &str) -> Option<String> {
    let z = zustand?;
    let aelter = matches!(
        (Version::lesen(&z.von), Version::lesen(&z.nach)),
        (Some(von), Some(nach)) if von > nach
    );
    (z.lage == Lage::Angekommen && z.nach == eigene && aelter).then(|| z.von.clone())
}

/// Die Version, die nach einem Rückfall nicht erneut per Tausch angeboten
/// wird. Unabhängig davon, ob die Meldung dazu schon geschlossen ist.
pub fn sperre_aus(zustand: Option<&tausch::Zustand>, eigene: &str) -> Option<String> {
    let z = zustand?;
    (z.lage == Lage::Zurueckgefallen && z.von == eigene).then(|| z.nach.clone())
}

/// Anzeige-Stand fürs Frontend (Teil von `update_info`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KernInfo {
    /// `aus` | `laedt` | `bereit` | `nicht_moeglich` | `wechselt`.
    pub tausch: String,
    /// Version, zu der gewechselt werden kann.
    pub version: Option<String>,
    /// Bei `bereit`: Führt nach dem Wechsel ein Weg zurück?
    pub rueckweg: bool,
    /// Bei `nicht_moeglich`: warum (nur zur Diagnose).
    pub grund: Option<String>,
    /// `keine` | `gewechselt` | `zurueckgefallen` | `zurueckgenommen`.
    pub meldung: String,
    /// `gewechselt`: von welcher Version. `zurueckgefallen`: welche nicht
    /// anlief. `zurueckgenommen`: welche zurückgenommen wurde.
    pub meldung_version: Option<String>,
    /// `zurueckgefallen`: der Grund des Wächters.
    pub meldung_grund: Option<String>,
    /// Die Vorversion, die neben dem Programm liegt und zu der ein Rückweg
    /// führen könnte.
    pub zurueck_version: Option<String>,
    /// Darf zu ihr gewechselt werden? `false` = sie kann die heutigen Daten
    /// nicht lesen (ADR 0101): Knopf sichtbar, aber aus.
    pub zurueck_erlaubt: bool,
    /// Zurückgenommene Versionen — sie werden nicht erneut angeboten.
    pub zurueckgenommen: Vec<String>,
}

#[derive(Default)]
struct Innen {
    stand: Stand,
    meldung: Option<Meldung>,
    /// Version, die nach einem Rückfall nicht erneut zum Tausch angeboten
    /// wird (der Installer-Weg bleibt).
    gesperrt: Option<String>,
    /// Version, für die das Vorladen läuft oder lief, und wie oft schon.
    versucht: Option<(String, u32)>,
    /// Auf Wunsch zurückgenommene Versionen (aus `rueckweg.json`): weder
    /// per Tausch noch per Installer erneut angeboten.
    zurueckgenommen: Vec<String>,
    /// Die Vorversion neben dem Programm und ob der Rückweg erlaubt ist.
    zurueck: Option<(String, bool)>,
    /// Der Stand vor einem begonnenen Rückweg — bricht er ab, bevor etwas
    /// verändert wurde, gilt er wieder.
    vor_rueckweg: Option<Stand>,
}

/// Zustand des Kernwechsels. Lebt im `AppState`.
#[derive(Default)]
pub struct KernTausch {
    innen: Mutex<Innen>,
}

impl KernTausch {
    fn innen(&self) -> std::sync::MutexGuard<'_, Innen> {
        self.innen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Beim Start (und nach der Ankunftsmeldung): Was sagt die letzte
    /// Übergabe über diese Version?
    pub fn start(&self, zustand: Option<&tausch::Zustand>, eigene: &str) {
        let mut i = self.innen();
        i.meldung = meldung_aus(zustand, eigene);
        // Eine Version, die hier nicht anlief, wird nicht noch einmal per
        // Tausch angeboten — auch wenn die Meldung schon geschlossen ist.
        i.gesperrt = sperre_aus(zustand, eigene);
    }

    /// Beim Start und nach jeder Änderung an `rueckweg.json`: Welche
    /// Vorversion liegt bereit, und was ist zurückgenommen?
    pub fn rueckweg_setzen(&self, urteil: crate::rueckweg::Urteil, gesperrt: Vec<String>) {
        use crate::rueckweg::Urteil;
        let mut i = self.innen();
        i.zurueckgenommen = gesperrt;
        i.zurueck = match urteil {
            Urteil::Keiner => None,
            Urteil::Erlaubt { version } => Some((version, true)),
            Urteil::NichtLesbar { version } => Some((version, false)),
        };
    }

    /// Wurde `version` auf Wunsch zurückgenommen? Dann wird sie gar nicht
    /// mehr angeboten — auch nicht über den Installer.
    pub fn ist_zurueckgenommen(&self, version: &str) -> bool {
        self.innen().zurueckgenommen.iter().any(|v| v == version)
    }

    /// Nimmt den Rückweg — genau einmal. Sperrt wie ein Wechsel den
    /// Installer-Weg.
    pub fn nimm_fuer_rueckweg(&self) -> Result<String, &'static str> {
        let mut i = self.innen();
        match &i.stand {
            // Das Vorladen schreibt gerade die Datei, deren Namen der
            // Rückweg braucht.
            Stand::Laedt => {
                return Err("Gerade wird ein Update vorbereitet – bitte gleich noch einmal.")
            }
            Stand::Wechselt { .. } => return Err("Es läuft schon ein Wechsel."),
            _ => {}
        }
        let version = match &i.zurueck {
            Some((version, true)) => version.clone(),
            Some((_, false)) => {
                return Err("Die vorige Version kann die heutigen Daten nicht lesen.")
            }
            None => return Err("Es liegt keine vorige Version bereit."),
        };
        i.vor_rueckweg = Some(std::mem::replace(
            &mut i.stand,
            Stand::Wechselt {
                version: version.clone(),
                rueckweg: false,
            },
        ));
        Ok(version)
    }

    /// Der Rückweg brach ab, **bevor** etwas verändert wurde.
    pub fn rueckweg_nicht_begonnen(&self) {
        let mut i = self.innen();
        if let Some(vorher) = i.vor_rueckweg.take() {
            i.stand = vorher;
        }
    }

    /// Der Rückweg ist gescheitert, nachdem Dateien bewegt wurden. Ein
    /// bereitliegendes Update bleibt (der Rückweg fasst seine Datei nicht
    /// an), aber der Knopf verschwindet bis zum nächsten Start: Ob die
    /// Vorversion noch unversehrt daliegt, ist dann nicht mehr sicher.
    pub fn rueckweg_gescheitert(&self) {
        let mut i = self.innen();
        if let Some(vorher) = i.vor_rueckweg.take() {
            i.stand = vorher;
            i.zurueck = None;
        }
    }

    /// Soll für `version` vorgeladen werden? Zählt den Versuch.
    ///
    /// Nein, wenn: gerade geladen wird, die Datei schon bereitliegt, ein
    /// Wechsel läuft, die Version gesperrt ist, das Hindernis dauerhaft war
    /// oder schon [`MAX_VERSUCHE`] Versuche liefen.
    pub fn beginne(&self, version: &str) -> bool {
        let mut i = self.innen();
        if i.gesperrt.as_deref() == Some(version) || i.zurueckgenommen.iter().any(|v| v == version)
        {
            return false;
        }
        let bisher = match &i.versucht {
            Some((v, n)) if v == version => *n,
            _ => 0,
        };
        let frei = match &i.stand {
            Stand::Aus => true,
            // Für diese Version nur nach einem vorübergehenden Hindernis
            // (dann wurde `Aus` gesetzt); für eine ANDERE Version immer.
            Stand::NichtMoeglich { .. } | Stand::Bereit { .. } => bisher == 0,
            Stand::Laedt | Stand::Wechselt { .. } => false,
        };
        if !frei || bisher >= MAX_VERSUCHE {
            return false;
        }
        i.versucht = Some((version.to_string(), bisher + 1));
        i.stand = Stand::Laedt;
        true
    }

    /// Gehört das laufende Vorladen noch zu `version`? Ein Ergebnis, das zu
    /// spät kommt (Update zurückgezogen, neuere Version aufgetaucht), darf
    /// den Stand nicht mehr ändern.
    fn laedt_noch(i: &Innen, version: &str) -> bool {
        i.stand == Stand::Laedt && i.versucht.as_ref().is_some_and(|(v, _)| v == version)
    }

    /// Das Vorladen für `version` ist gelungen. `false`, wenn das Ergebnis
    /// zu spät kam und verworfen wurde.
    pub fn bereit(&self, version: &str, rueckweg: bool) -> bool {
        let mut i = self.innen();
        if !Self::laedt_noch(&i, version) {
            return false;
        }
        i.stand = Stand::Bereit {
            version: version.to_string(),
            rueckweg,
        };
        true
    }

    /// Das Vorladen für `version` ist gescheitert.
    pub fn nicht_moeglich(&self, version: &str, hindernis: Hindernis) {
        let mut i = self.innen();
        if !Self::laedt_noch(&i, version) {
            return;
        }
        tracing::info!(
            "Kernwechsel ohne Installer nicht möglich: {}",
            hindernis.text()
        );
        i.stand = match hindernis {
            Hindernis::Dauerhaft(grund) => Stand::NichtMoeglich { grund },
            // Zurück auf „aus": Der nächste Prüflauf versucht es wieder,
            // bis `MAX_VERSUCHE` erreicht ist.
            Hindernis::Voruebergehend(_) => Stand::Aus,
        };
    }

    /// Das Update wurde zurückgezogen oder es gibt keines mehr.
    pub fn verwerfen(&self) {
        let mut i = self.innen();
        if matches!(i.stand, Stand::Wechselt { .. }) {
            return;
        }
        i.stand = Stand::Aus;
        i.versucht = None;
    }

    /// Läuft gerade ein Wechsel? Dann ist der Installer-Weg gesperrt.
    pub fn wechselt(&self) -> bool {
        matches!(self.innen().stand, Stand::Wechselt { .. })
    }

    /// Nimmt den Stand „bereit" für den Wechsel — genau einmal. Ein zweiter
    /// Klick findet nichts mehr.
    pub fn nimm_fuer_wechsel(&self) -> Option<String> {
        let mut i = self.innen();
        match &i.stand {
            Stand::Bereit { version, rueckweg } => {
                let (version, rueckweg) = (version.clone(), *rueckweg);
                i.stand = Stand::Wechselt {
                    version: version.clone(),
                    rueckweg,
                };
                Some(version)
            }
            _ => None,
        }
    }

    /// Der Wechsel brach ab, **bevor** etwas getauscht wurde: Die neue Datei
    /// liegt unverändert bereit, das Angebot bleibt.
    pub fn wechsel_nicht_begonnen(&self) {
        let mut i = self.innen();
        if let Stand::Wechselt { version, rueckweg } = &i.stand {
            i.stand = Stand::Bereit {
                version: version.clone(),
                rueckweg: *rueckweg,
            };
        }
    }

    /// Der Wechsel ist nach dem Tausch gescheitert und wurde zurückgenommen.
    /// Wirkt nur, solange wirklich ein Wechsel läuft.
    pub fn wechsel_gescheitert(&self, grund: String) {
        let mut i = self.innen();
        if matches!(i.stand, Stand::Wechselt { .. }) {
            i.stand = Stand::NichtMoeglich { grund };
        }
    }

    pub fn meldung_weg(&self) {
        self.innen().meldung = None;
    }

    pub fn info(&self) -> KernInfo {
        let i = self.innen();
        let (tausch, version, rueckweg, grund) = match &i.stand {
            Stand::Aus => ("aus", None, false, None),
            Stand::Laedt => ("laedt", None, false, None),
            Stand::Bereit { version, rueckweg } => {
                ("bereit", Some(version.clone()), *rueckweg, None)
            }
            Stand::NichtMoeglich { grund } => ("nicht_moeglich", None, false, Some(grund.clone())),
            Stand::Wechselt { version, .. } => ("wechselt", Some(version.clone()), false, None),
        };
        let (meldung, meldung_version, meldung_grund) = match &i.meldung {
            None => ("keine", None, None),
            Some(Meldung::Gewechselt { von }) => ("gewechselt", Some(von.clone()), None),
            Some(Meldung::Zurueckgefallen { nach, grund }) => {
                ("zurueckgefallen", Some(nach.clone()), Some(grund.clone()))
            }
            Some(Meldung::Zurueckgenommen { version }) => {
                ("zurueckgenommen", Some(version.clone()), None)
            }
        };
        KernInfo {
            tausch: tausch.to_string(),
            version,
            rueckweg,
            grund,
            meldung: meldung.to_string(),
            meldung_version,
            meldung_grund,
            zurueck_version: i.zurueck.as_ref().map(|(v, _)| v.clone()),
            zurueck_erlaubt: i.zurueck.as_ref().is_some_and(|(_, erlaubt)| *erlaubt),
            zurueckgenommen: i.zurueckgenommen.clone(),
        }
    }
}

/// Wie die Ankunftsmeldung nach einem Kernwechsel ausging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ankunft {
    /// Es ist keine Übergabe zu dieser Version offen — nichts mehr zu tun.
    NichtsOffen,
    /// Gemeldet; der Wächter kann enden.
    Gemeldet,
    /// Die Datei ließ sich gerade nicht lesen oder schreiben (Virenscanner
    /// hält sie kurz fest): beim nächsten Aufruf wieder versuchen. Würde
    /// hier aufgegeben, rollte der Wächter eine gesunde Version zurück.
    SpaeterNochmal,
}

/// Ein Versuch, die Ankunft dieser Version zu melden. Rein über die Datei,
/// ohne App — testbar.
pub fn ankunft_versuchen(daten: &std::path::Path, eigene: &str, jetzt_ms: u64) -> Ankunft {
    match tausch::zustand_lesen_genau(daten) {
        Err(_) => Ankunft::SpaeterNochmal,
        Ok(Some(z)) if z.lage == Lage::Gestartet && z.nach == eigene => {
            if tausch::ankunft_melden(daten, jetzt_ms, eigene) {
                Ankunft::Gemeldet
            } else {
                Ankunft::SpaeterNochmal
            }
        }
        Ok(_) => Ankunft::NichtsOffen,
    }
}

/// Legt die geprüfte Programmdatei als `neu` ab und prüft, was **auf der
/// Platte** steht. Geprüft wird nicht der Puffer, sondern die geschriebene
/// Datei — sonst könnte zwischen Prüfen und Schreiben etwas anderes
/// entstehen.
pub fn ablegen(pfade: &Pfade, kopf: &KernKopf, exe: &[u8]) -> Result<(), String> {
    uebergabe::exe_pruefen(kopf, exe).map_err(|e| e.to_string())?;
    // Neu anlegen, nie eine vorhandene Datei öffnen: Ein Rest oder ein
    // hingelegter Verweis unter dem Namen wird so nicht verfolgt.
    match std::fs::remove_file(&pfade.neu) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("alter Rest ließ sich nicht entfernen ({e})"))
        }
        _ => {}
    }
    let schreiben = || -> std::io::Result<()> {
        let mut datei = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pfade.neu)?;
        datei.write_all(exe)?;
        datei.sync_all()
    };
    if let Err(e) = schreiben() {
        let _ = std::fs::remove_file(&pfade.neu);
        return Err(format!("Ablegen gescheitert ({e})"));
    }
    let gelesen = std::fs::read(&pfade.neu).map_err(|e| format!("Rücklesen gescheitert ({e})"));
    let geprueft =
        gelesen.and_then(|b| uebergabe::exe_pruefen(kopf, &b).map_err(|e| e.to_string()));
    if geprueft.is_err() {
        let _ = std::fs::remove_file(&pfade.neu);
    }
    geprueft
}

/// Prüft unmittelbar vor dem Tausch noch einmal, dass `neu` die Datei ist,
/// die zum Kopf gehört. Zwischen Vorladen und Klick können Stunden liegen.
pub fn neu_passt_noch(pfade: &Pfade, kopf: &KernKopf) -> Result<(), String> {
    let bytes = std::fs::read(&pfade.neu).map_err(|e| format!("neue Programmdatei fehlt ({e})"))?;
    uebergabe::exe_pruefen(kopf, &bytes).map_err(|e| e.to_string())
}

/// Holt eine Datei mit Größendeckel. `None` bei jedem Fehler.
async fn holen(client: &reqwest::Client, url: &str, deckel: u64) -> Option<Vec<u8>> {
    let mut antwort = client.get(url).send().await.ok()?;
    if !antwort.status().is_success() {
        return None;
    }
    if antwort.content_length().is_some_and(|n| n > deckel) {
        return None;
    }
    // Gleich in passender Größe anlegen — sonst verdoppelt das Wachsen den
    // Bedarf kurzzeitig.
    let erwartet = antwort.content_length().unwrap_or(0).min(deckel) as usize;
    let mut bytes = Vec::with_capacity(erwartet);
    while let Some(stueck) = antwort.chunk().await.ok()? {
        if (bytes.len() + stueck.len()) as u64 > deckel {
            return None;
        }
        bytes.extend_from_slice(&stueck);
    }
    Some(bytes)
}

/// Ein geladener, geprüfter Kern: Kopf und Programmdatei.
pub struct Geladen {
    pub kopf: KernKopf,
    pub exe: Vec<u8>,
}

/// Lädt Kopf und Programmdatei für `erwartet` und prüft beides. Das
/// Hindernis sagt, ob ein späterer Versuch lohnt: Was am Netz hängt, ist
/// vorübergehend; was die Prüfung ablehnt, bleibt abgelehnt.
pub async fn laden(
    erwartet: &str,
    eigene: &str,
    schluessel: &Schluessel,
) -> Result<Geladen, Hindernis> {
    let spaeter = |t: &str| Hindernis::Voruebergehend(t.to_string());
    let erwartet = Version::lesen(erwartet)
        .ok_or_else(|| Hindernis::Dauerhaft("Version des Updates unlesbar".into()))?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        // Großzügig: rund 34 MB über ein Hallennetz, im Hintergrund.
        .timeout(std::time::Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Hindernis::Dauerhaft(e.to_string()))?;
    let index = holen(&client, &index_url(), MAX_KLEIN_BYTES as u64)
        .await
        .ok_or_else(|| spaeter("kein Kern-Artefakt veröffentlicht oder Netz weg"))?;
    if index_version(&index) != Some(erwartet) {
        return Err(spaeter(
            "das Kern-Artefakt gehört (noch) zu einer anderen Version",
        ));
    }
    let kopf_bytes = holen(
        &client,
        &kopf_url(erwartet),
        uebergabe::MAX_KOPF_BYTES as u64,
    )
    .await
    .ok_or_else(|| spaeter("Kern-Kopf nicht ladbar"))?;
    let signatur = holen(
        &client,
        &format!("{}.sig", kopf_url(erwartet)),
        MAX_KLEIN_BYTES as u64,
    )
    .await
    .and_then(|b| String::from_utf8(b).ok())
    .ok_or_else(|| spaeter("Signatur des Kern-Kopfs nicht ladbar"))?;
    let kopf = uebergabe::kopf_pruefen(&kopf_bytes, &signatur, schluessel, eigene)
        .map_err(|e| Hindernis::Dauerhaft(e.to_string()))?;
    // Der Kopf muss GENAU die Version nennen, die der Updater anbietet —
    // sonst ließe sich unter dem Namen der neuen eine andere, ebenfalls echt
    // signierte Version unterschieben.
    if kopf.version != erwartet {
        return Err(Hindernis::Dauerhaft(format!(
            "Kern-Kopf nennt {} statt {erwartet}",
            kopf.version
        )));
    }
    // Deckel = genau die im signierten Kopf genannte Größe.
    let exe = holen(&client, &exe_url(erwartet), kopf.exe_groesse)
        .await
        .ok_or_else(|| spaeter("Programmdatei nicht ladbar"))?;
    uebergabe::exe_pruefen(&kopf, &exe).map_err(|e| Hindernis::Dauerhaft(e.to_string()))?;
    Ok(Geladen { kopf, exe })
}

/// Der ganze Weg vom geladenen Kern bis „bereit": Kann hier getauscht
/// werden, ablegen, Probelauf. Blockiert (Datei-Arbeit, Probelauf bis zu
/// zwei Minuten) — auf einem Arbeits-Thread aufrufen.
pub fn vorbereiten(pfade: &Pfade, geladen: &Geladen, windows: bool) -> Result<(), String> {
    tausch::tausch_moeglich(pfade, windows).map_err(|e| e.to_string())?;
    ablegen(pfade, &geladen.kopf, &geladen.exe)?;
    // Der Probelauf beweist, dass das System die Datei ausführt — und er
    // erledigt die Prüfung durch den Virenscanner, bevor jemand klickt
    // (ADR 0100: erster Start bis 20 s, danach Sekundenbruchteile).
    tausch::probelauf(&pfade.neu).map(|_| ()).map_err(|e| {
        let _ = std::fs::remove_file(&pfade.neu);
        format!("Probelauf der neuen Programmdatei: {e}")
    })
}

/// Die Pfade neben der laufenden Programmdatei.
pub fn eigene_pfade() -> Result<Pfade, String> {
    let ich = std::env::current_exe().map_err(|e| e.to_string())?;
    Pfade::neben(&ich).ok_or_else(|| "Pfad der Programmdatei unbrauchbar".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatur::testhilfe::Testschluessel;
    use std::path::Path;

    fn exe() -> Vec<u8> {
        let mut bytes = b"MZ".to_vec();
        bytes.extend_from_slice(&[9u8; 2048]);
        bytes
    }

    fn kopf_fuer(exe: &[u8], version: &str, eigene: &str) -> KernKopf {
        let sha: String = ring::digest::digest(&ring::digest::SHA256, exe)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let json = serde_json::to_vec(&serde_json::json!({
            "format": 1, "art": "kern", "version": version,
            "exe_sha256": sha, "exe_groesse": exe.len(),
            "rueckwaerts_lesbar_ab": eigene,
        }))
        .unwrap();
        let schluessel = Testschluessel::neu(1);
        uebergabe::kopf_pruefen(
            &json,
            &schluessel.signieren(&json),
            &schluessel.oeffentlich(),
            eigene,
        )
        .unwrap()
    }

    fn pfade_in(dir: &Path) -> Pfade {
        Pfade::neben(&dir.join("bts-light.exe")).unwrap()
    }

    #[test]
    fn index_nennt_nur_eine_gueltige_version() {
        assert_eq!(
            index_version(br#"{"format":1,"version":"0.9.440"}"#),
            Version::lesen("0.9.440")
        );
        for kaputt in [
            &br#"{"format":2,"version":"0.9.440"}"#[..],
            br#"{"format":1,"version":"neu"}"#,
            br#"{"format":1}"#,
            b"<html>Bitte anmelden</html>",
            b"",
        ] {
            assert_eq!(index_version(kaputt), None);
        }
    }

    #[test]
    fn adressen_entstehen_aus_fester_basis_und_version() {
        let v = Version::lesen("0.9.440").unwrap();
        assert_eq!(
            index_url(),
            "https://badhub.de/download/bts-light/kern.json"
        );
        assert_eq!(
            kopf_url(v),
            "https://badhub.de/download/bts-light/kern/kern-0.9.440.json"
        );
        assert_eq!(
            exe_url(v),
            "https://badhub.de/download/bts-light/kern/bts-light-0.9.440.exe"
        );
    }

    #[test]
    fn ablegen_schreibt_nur_die_passende_datei() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        let exe = exe();
        let kopf = kopf_fuer(&exe, "1.2.4", "1.2.3");

        ablegen(&p, &kopf, &exe).unwrap();
        assert_eq!(std::fs::read(&p.neu).unwrap(), exe);
        assert_eq!(neu_passt_noch(&p, &kopf), Ok(()));

        // Ein alter Rest unter dem Namen wird ersetzt, nicht ergänzt.
        std::fs::write(&p.neu, b"alter rest, viel laenger als nichts").unwrap();
        ablegen(&p, &kopf, &exe).unwrap();
        assert_eq!(std::fs::read(&p.neu).unwrap(), exe);

        // Passt die Datei nicht zum Kopf, wird nichts geschrieben.
        std::fs::remove_file(&p.neu).unwrap();
        let mut falsch = exe.clone();
        falsch[100] ^= 1;
        assert!(ablegen(&p, &kopf, &falsch).is_err());
        assert!(!p.neu.exists(), "eine unpassende Datei bleibt nicht liegen");
    }

    /// Zwischen Vorladen und Klick wurde die Datei verändert oder gelöscht:
    /// Der Tausch findet nicht statt.
    #[test]
    fn vor_dem_tausch_wird_die_datei_erneut_geprueft() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        let exe = exe();
        let kopf = kopf_fuer(&exe, "1.2.4", "1.2.3");
        ablegen(&p, &kopf, &exe).unwrap();

        let mut manipuliert = exe.clone();
        manipuliert[500] ^= 0x10;
        std::fs::write(&p.neu, &manipuliert).unwrap();
        assert!(neu_passt_noch(&p, &kopf).is_err());

        std::fs::remove_file(&p.neu).unwrap();
        assert!(neu_passt_noch(&p, &kopf).unwrap_err().contains("fehlt"));
    }

    #[test]
    fn nicht_windows_bleibt_beim_installer_weg() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        let exe = exe();
        let geladen = Geladen {
            kopf: kopf_fuer(&exe, "1.2.4", "1.2.3"),
            exe,
        };
        let fehler = vorbereiten(&p, &geladen, false).unwrap_err();
        assert_eq!(fehler, "nur unter Windows");
        assert!(!p.neu.exists(), "ohne Tausch wird nichts abgelegt");
    }

    /// Lässt sich die neue Datei nicht ausführen (hier: kein Programm für
    /// dieses System), wird der Tausch nicht angeboten und die Datei wieder
    /// entfernt.
    #[test]
    fn probelauf_gescheitert_kein_tausch_angebot() {
        let dir = tempfile::tempdir().unwrap();
        let p = pfade_in(dir.path());
        let exe = exe();
        let geladen = Geladen {
            kopf: kopf_fuer(&exe, "1.2.4", "1.2.3"),
            exe,
        };
        let fehler = vorbereiten(&p, &geladen, true).unwrap_err();
        assert!(fehler.contains("Probelauf"), "{fehler}");
        assert!(!p.neu.exists());
    }

    #[test]
    fn stand_bereit_wird_genau_einmal_genommen() {
        let k = KernTausch::default();
        assert_eq!(k.info().tausch, "aus");
        assert!(k.beginne("1.2.4"));
        assert_eq!(k.info().tausch, "laedt");
        // Derselbe Prüflauf noch einmal: kein zweites Vorladen.
        assert!(!k.beginne("1.2.4"));
        assert!(k.nimm_fuer_wechsel().is_none(), "noch nicht bereit");
        assert!(!k.wechselt());

        assert!(k.bereit("1.2.4", true));
        let info = k.info();
        assert_eq!(
            (info.tausch.as_str(), info.version.as_deref(), info.rueckweg),
            ("bereit", Some("1.2.4"), true)
        );
        // Liegt die Datei bereit, wird sie nicht noch einmal geladen.
        assert!(!k.beginne("1.2.4"));
        assert_eq!(k.nimm_fuer_wechsel().as_deref(), Some("1.2.4"));
        assert!(k.wechselt(), "ab hier ist der Installer-Weg gesperrt");
        assert_eq!(k.info().tausch, "wechselt");
        assert!(
            k.nimm_fuer_wechsel().is_none(),
            "ein Doppelklick wechselt nicht zweimal"
        );
    }

    /// Scheitert der Wechsel VOR dem Tausch, bleibt das Angebot; danach nur,
    /// wenn wirklich ein Wechsel lief.
    #[test]
    fn gescheiterter_wechsel_laesst_das_angebot_oder_nimmt_es() {
        let k = KernTausch::default();
        k.beginne("1.2.4");
        k.bereit("1.2.4", true);

        // Ohne laufenden Wechsel ändern beide Meldungen nichts — ein zweiter
        // Klick darf den Stand des ersten nicht überschreiben.
        k.wechsel_gescheitert("x".into());
        k.wechsel_nicht_begonnen();
        assert_eq!(k.info().tausch, "bereit");

        k.nimm_fuer_wechsel().unwrap();
        k.wechsel_nicht_begonnen();
        let info = k.info();
        assert_eq!(
            info.tausch, "bereit",
            "nichts verändert: Das Angebot bleibt"
        );
        assert!(info.rueckweg, "samt der Angabe zum Rückweg");
        assert!(!k.wechselt());

        k.nimm_fuer_wechsel().unwrap();
        k.wechsel_gescheitert("Wächter startete nicht".into());
        assert_eq!(k.info().tausch, "nicht_moeglich");
        assert!(!k.wechselt(), "der Installer-Weg ist wieder frei");
        // Für dieselbe Version wird danach nicht neu geladen …
        assert!(!k.beginne("1.2.4"));
        // … für eine neuere schon.
        assert!(k.beginne("1.2.5"));
    }

    /// Netz weg oder Artefakt noch nicht veröffentlicht: Der nächste Prüflauf
    /// versucht es wieder — aber nicht endlos.
    #[test]
    fn voruebergehendes_hindernis_wird_spaeter_erneut_versucht() {
        let k = KernTausch::default();
        for versuch in 1..=MAX_VERSUCHE {
            assert!(k.beginne("1.2.4"), "Versuch {versuch}");
            k.nicht_moeglich("1.2.4", Hindernis::Voruebergehend("Netz weg".into()));
            assert_eq!(k.info().tausch, "aus", "still zurück zum Installer-Weg");
        }
        assert!(
            !k.beginne("1.2.4"),
            "nach {MAX_VERSUCHE} Versuchen ist Schluss"
        );
        // Eine neue Version zählt neu.
        assert!(k.beginne("1.2.5"));
    }

    #[test]
    fn dauerhaftes_hindernis_wird_nicht_wiederholt() {
        let k = KernTausch::default();
        assert!(k.beginne("1.2.4"));
        k.nicht_moeglich("1.2.4", Hindernis::Dauerhaft("nur unter Windows".into()));
        let info = k.info();
        assert_eq!(info.tausch, "nicht_moeglich");
        assert_eq!(info.grund.as_deref(), Some("nur unter Windows"));
        assert!(!k.beginne("1.2.4"));
    }

    /// Ein Ergebnis, das zu spät kommt, ändert nichts mehr: Das Update wurde
    /// inzwischen zurückgezogen, oder eine neuere Version wird schon geladen.
    #[test]
    fn verspaetetes_ergebnis_wird_verworfen() {
        // Zurückgezogen, während geladen wurde.
        let k = KernTausch::default();
        k.beginne("1.2.4");
        k.verwerfen();
        assert!(!k.bereit("1.2.4", true));
        assert_eq!(k.info().tausch, "aus");
        assert!(k.nimm_fuer_wechsel().is_none());

        // Eine neuere Version wird schon geladen; das alte Ergebnis trifft ein.
        let k = KernTausch::default();
        k.beginne("1.2.4");
        k.verwerfen();
        assert!(k.beginne("1.2.5"));
        assert!(
            !k.bereit("1.2.4", true),
            "gehört nicht zur laufenden Version"
        );
        k.nicht_moeglich("1.2.4", Hindernis::Dauerhaft("alt".into()));
        assert_eq!(k.info().tausch, "laedt", "1.2.5 lädt unbeirrt weiter");
        assert!(k.bereit("1.2.5", false));
        assert_eq!(k.info().version.as_deref(), Some("1.2.5"));

        // Mitten im Wechsel zieht ein Prüflauf nichts mehr zurück.
        k.nimm_fuer_wechsel().unwrap();
        k.verwerfen();
        assert!(k.wechselt());
    }

    fn zustand(von: &str, nach: &str, lage: Lage, grund: Option<&str>) -> tausch::Zustand {
        tausch::Zustand {
            von: von.into(),
            nach: nach.into(),
            zeit_ms: 1,
            lage,
            grund: grund.map(str::to_string),
            gesehen: false,
        }
    }

    #[test]
    fn meldung_nur_wenn_die_uebergabe_diese_version_betrifft() {
        // Angekommen bei uns: „läuft jetzt mit …".
        assert_eq!(
            meldung_aus(
                Some(&zustand("1.2.3", "1.2.4", Lage::Angekommen, None)),
                "1.2.4"
            ),
            Some(Meldung::Gewechselt {
                von: "1.2.3".into()
            })
        );
        // Zurückgefallen auf uns: „… ließ sich nicht starten".
        assert_eq!(
            meldung_aus(
                Some(&zustand(
                    "1.2.3",
                    "1.2.4",
                    Lage::Zurueckgefallen,
                    Some("Frist")
                )),
                "1.2.3"
            ),
            Some(Meldung::Zurueckgefallen {
                nach: "1.2.4".into(),
                grund: "Frist".into()
            })
        );
        // Eine alte Übergabe, die diese Version nicht betrifft: nichts.
        for (z, eigene) in [
            (zustand("1.2.3", "1.2.4", Lage::Angekommen, None), "1.2.5"),
            (
                zustand("1.2.3", "1.2.4", Lage::Zurueckgefallen, None),
                "1.2.4",
            ),
            (zustand("1.2.3", "1.2.4", Lage::Gestartet, None), "1.2.4"),
        ] {
            assert_eq!(meldung_aus(Some(&z), eigene), None);
        }
        assert_eq!(meldung_aus(None, "1.2.3"), None);
    }

    #[test]
    fn zurueckgefallene_version_gesperrt() {
        let k = KernTausch::default();
        k.start(
            Some(&zustand(
                "1.2.3",
                "1.2.4",
                Lage::Zurueckgefallen,
                Some("Code 3"),
            )),
            "1.2.3",
        );
        let info = k.info();
        assert_eq!(info.meldung, "zurueckgefallen");
        assert_eq!(info.meldung_version.as_deref(), Some("1.2.4"));
        assert_eq!(info.meldung_grund.as_deref(), Some("Code 3"));
        // Dieselbe Version wird nicht noch einmal per Tausch vorgeladen …
        assert!(!k.beginne("1.2.4"));
        assert_eq!(k.info().tausch, "aus");
        // … eine neuere schon.
        assert!(k.beginne("1.2.5"));
        k.meldung_weg();
        assert_eq!(k.info().meldung, "keine");
        // Die Sperre überlebt das Schließen der Meldung.
        k.verwerfen();
        assert!(!k.beginne("1.2.4"));
    }

    /// Geschlossen heißt geschlossen — auch nach dem nächsten Programmstart.
    /// Die Sperre bleibt trotzdem.
    #[test]
    fn geschlossene_meldung_kommt_nach_dem_neustart_nicht_wieder() {
        let dir = tempfile::tempdir().unwrap();
        tausch::zustand_schreiben(
            dir.path(),
            &zustand("1.2.3", "1.2.4", Lage::Zurueckgefallen, Some("Frist")),
        )
        .unwrap();
        let k = KernTausch::default();
        k.start(tausch::zustand_lesen(dir.path()).as_ref(), "1.2.3");
        assert_eq!(k.info().meldung, "zurueckgefallen");

        // „Verstanden".
        k.meldung_weg();
        tausch::meldung_gesehen(dir.path());

        // Nächster Programmstart.
        let k = KernTausch::default();
        k.start(tausch::zustand_lesen(dir.path()).as_ref(), "1.2.3");
        assert_eq!(k.info().meldung, "keine");
        assert!(!k.beginne("1.2.4"), "die Sperre gilt weiter");

        // Dasselbe für „läuft jetzt mit …".
        tausch::zustand_schreiben(
            dir.path(),
            &zustand("1.2.3", "1.2.4", Lage::Angekommen, None),
        )
        .unwrap();
        let k = KernTausch::default();
        k.start(tausch::zustand_lesen(dir.path()).as_ref(), "1.2.4");
        assert_eq!(k.info().meldung, "gewechselt");
        tausch::meldung_gesehen(dir.path());
        let k = KernTausch::default();
        k.start(tausch::zustand_lesen(dir.path()).as_ref(), "1.2.4");
        assert_eq!(k.info().meldung, "keine");
    }

    #[test]
    fn nach_dem_wechsel_meldet_die_neue_version_ihre_ankunft() {
        let dir = tempfile::tempdir().unwrap();
        // Keine Übergabe im Gang: nichts zu tun, endgültig.
        assert_eq!(
            ankunft_versuchen(dir.path(), "1.2.4", 5),
            Ankunft::NichtsOffen
        );
        // Übergabe zu einer ANDEREN Version (die alte App fragt während des
        // Wechsels noch ab): nichts anfassen.
        tausch::zustand_schreiben(
            dir.path(),
            &zustand("1.2.3", "1.2.4", Lage::Gestartet, None),
        )
        .unwrap();
        assert_eq!(
            ankunft_versuchen(dir.path(), "1.2.3", 5),
            Ankunft::NichtsOffen
        );
        assert_eq!(
            tausch::zustand_lesen(dir.path()).unwrap().lage,
            Lage::Gestartet
        );
        // Die neue Version meldet sich.
        assert_eq!(ankunft_versuchen(dir.path(), "1.2.4", 7), Ankunft::Gemeldet);
        let z = tausch::zustand_lesen(dir.path()).unwrap();
        assert_eq!((z.lage, z.zeit_ms), (Lage::Angekommen, 7));
        // Danach ist nichts mehr offen.
        assert_eq!(
            ankunft_versuchen(dir.path(), "1.2.4", 9),
            Ankunft::NichtsOffen
        );

        let k = KernTausch::default();
        k.start(Some(&z), "1.2.4");
        let info = k.info();
        assert_eq!(info.meldung, "gewechselt");
        assert_eq!(info.meldung_version.as_deref(), Some("1.2.3"));
    }

    /// Eine gerade nicht lesbare Zustandsdatei ist KEIN „nichts offen" —
    /// sonst gäbe die neue Version auf, und der Wächter rollte sie nach der
    /// Frist zurück, obwohl sie läuft.
    #[test]
    fn unlesbarer_zustand_heisst_spaeter_nochmal() {
        let dir = tempfile::tempdir().unwrap();
        // Halb geschrieben / kaputt.
        std::fs::write(dir.path().join(tausch::ZUSTAND_DATEI), b"{ halb").unwrap();
        assert_eq!(
            ankunft_versuchen(dir.path(), "1.2.4", 1),
            Ankunft::SpaeterNochmal
        );
        // Unter dem Namen liegt ein Ordner: Lesen scheitert, aber nicht mit
        // „nicht gefunden".
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(tausch::ZUSTAND_DATEI)).unwrap();
        assert_eq!(
            ankunft_versuchen(dir.path(), "1.2.4", 1),
            Ankunft::SpaeterNochmal
        );
    }

    fn mit_rueckweg(k: &KernTausch, version: &str, erlaubt: bool, gesperrt: &[&str]) {
        use crate::rueckweg::Urteil;
        let version = version.to_string();
        k.rueckweg_setzen(
            if erlaubt {
                Urteil::Erlaubt { version }
            } else {
                Urteil::NichtLesbar { version }
            },
            gesperrt.iter().map(|s| s.to_string()).collect(),
        );
    }

    #[test]
    fn rueckweg_wird_genau_einmal_genommen_und_sperrt_den_installer() {
        let k = KernTausch::default();
        assert!(
            k.nimm_fuer_rueckweg().is_err(),
            "ohne Vorversion kein Rückweg"
        );
        mit_rueckweg(&k, "1.2.3", true, &[]);
        let info = k.info();
        assert_eq!(info.zurueck_version.as_deref(), Some("1.2.3"));
        assert!(info.zurueck_erlaubt);

        assert_eq!(k.nimm_fuer_rueckweg().as_deref(), Ok("1.2.3"));
        assert!(k.wechselt(), "der Installer-Weg ist gesperrt");
        assert_eq!(k.info().version.as_deref(), Some("1.2.3"));
        assert!(k.nimm_fuer_rueckweg().is_err(), "kein zweiter Klick");
        assert!(k.nimm_fuer_wechsel().is_none());
    }

    #[test]
    fn rueckweg_knopf_aus_laesst_sich_nicht_druecken() {
        let k = KernTausch::default();
        mit_rueckweg(&k, "1.2.3", false, &[]);
        let info = k.info();
        assert_eq!(info.zurueck_version.as_deref(), Some("1.2.3"));
        assert!(!info.zurueck_erlaubt);
        assert!(k.nimm_fuer_rueckweg().is_err());
        assert!(!k.wechselt());
    }

    /// Während das Vorladen die Datei `neu` schreibt, darf der Rückweg sie
    /// nicht unter ihm wegziehen.
    #[test]
    fn kein_rueckweg_waehrend_des_vorladens() {
        let k = KernTausch::default();
        mit_rueckweg(&k, "1.2.3", true, &[]);
        k.beginne("1.2.5");
        assert!(k.nimm_fuer_rueckweg().is_err());
        assert_eq!(k.info().tausch, "laedt");
        // Danach schon.
        k.bereit("1.2.5", true);
        assert!(k.nimm_fuer_rueckweg().is_ok());
    }

    #[test]
    fn abgebrochener_rueckweg_stellt_den_stand_wieder_her() {
        // Vor jeder Änderung abgebrochen: Ein bereitliegendes Update bleibt.
        let k = KernTausch::default();
        mit_rueckweg(&k, "1.2.3", true, &[]);
        k.beginne("1.2.5");
        k.bereit("1.2.5", true);
        k.nimm_fuer_rueckweg().unwrap();
        k.rueckweg_nicht_begonnen();
        let info = k.info();
        assert_eq!(
            (info.tausch.as_str(), info.version.as_deref()),
            ("bereit", Some("1.2.5"))
        );
        assert_eq!(info.zurueck_version.as_deref(), Some("1.2.3"));

        // Nach dem Bewegen von Dateien gescheitert: Das Update bleibt
        // bereit, der Rückweg-Knopf verschwindet.
        k.nimm_fuer_rueckweg().unwrap();
        k.rueckweg_gescheitert();
        let info = k.info();
        assert_eq!(info.tausch, "bereit");
        assert_eq!(info.zurueck_version, None);
        assert!(!k.wechselt(), "der Installer-Weg ist wieder frei");
        assert!(k.nimm_fuer_rueckweg().is_err());

        // Ohne laufenden Rückweg tun beide nichts.
        let k = KernTausch::default();
        mit_rueckweg(&k, "1.2.3", true, &[]);
        k.rueckweg_gescheitert();
        k.rueckweg_nicht_begonnen();
        assert_eq!(k.info().zurueck_version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn zurueckgenommene_version_nicht_erneut_angeboten() {
        let k = KernTausch::default();
        k.rueckweg_setzen(crate::rueckweg::Urteil::Keiner, vec!["1.2.4".into()]);
        assert!(k.ist_zurueckgenommen("1.2.4"));
        assert!(!k.beginne("1.2.4"), "kein Vorladen für den Tausch");
        assert_eq!(k.info().zurueckgenommen, vec!["1.2.4"]);
        // Eine neuere schon.
        assert!(!k.ist_zurueckgenommen("1.2.5"));
        assert!(k.beginne("1.2.5"));
    }

    /// Kommt die ältere Version nach einem Rückweg an, sagt sie das — und
    /// nicht „läuft jetzt mit …" wie nach einem Update.
    #[test]
    fn nach_dem_rueckweg_meldet_die_alte_version_die_ruecknahme() {
        assert_eq!(
            meldung_aus(
                Some(&zustand("1.2.4", "1.2.3", Lage::Angekommen, None)),
                "1.2.3"
            ),
            Some(Meldung::Zurueckgenommen {
                version: "1.2.4".into()
            })
        );
        let k = KernTausch::default();
        k.start(
            Some(&zustand("1.2.4", "1.2.3", Lage::Angekommen, None)),
            "1.2.3",
        );
        let info = k.info();
        assert_eq!(info.meldung, "zurueckgenommen");
        assert_eq!(info.meldung_version.as_deref(), Some("1.2.4"));
        assert_eq!(
            ruecknahme_aus(
                Some(&zustand("1.2.4", "1.2.3", Lage::Angekommen, None)),
                "1.2.3"
            )
            .as_deref(),
            Some("1.2.4")
        );
        // Auch wenn die Meldung schon geschlossen ist.
        let mut gesehen = zustand("1.2.4", "1.2.3", Lage::Angekommen, None);
        gesehen.gesehen = true;
        assert_eq!(meldung_aus(Some(&gesehen), "1.2.3"), None);
        assert_eq!(
            ruecknahme_aus(Some(&gesehen), "1.2.3").as_deref(),
            Some("1.2.4")
        );
        // Ein gewöhnliches Update oder ein noch offener Rückweg sperrt nichts.
        for z in [
            zustand("1.2.3", "1.2.4", Lage::Angekommen, None),
            zustand("1.2.4", "1.2.3", Lage::Gestartet, None),
            zustand("1.2.4", "1.2.3", Lage::Zurueckgefallen, None),
        ] {
            assert_eq!(ruecknahme_aus(Some(&z), &z.nach), None);
            assert_eq!(ruecknahme_aus(Some(&z), &z.von), None);
        }
        // Läuft die Vorversion beim Rückweg NICHT an, läuft die neue wieder
        // und sagt das wie nach jedem Rückfall.
        assert_eq!(
            meldung_aus(
                Some(&zustand(
                    "1.2.4",
                    "1.2.3",
                    Lage::Zurueckgefallen,
                    Some("Frist")
                )),
                "1.2.4"
            ),
            Some(Meldung::Zurueckgefallen {
                nach: "1.2.3".into(),
                grund: "Frist".into()
            })
        );
    }
}
