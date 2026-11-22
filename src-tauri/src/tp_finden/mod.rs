//! TP-Datei selbst finden (Spec `docs/features/tp-datei-finden.md`, ADR 0096).
//!
//! BTP hält die geöffnete Turnierdatei dauerhaft offen. Dieses Modul liest
//! ihren Pfad aus den Datei-Handles des laufenden `btp.exe`, damit der
//! Turnierleiter sie nicht im Dateidialog suchen muss.
//!
//! Zwei Teile:
//!
//! - **Hier**: alles, was sich ohne Windows prüfen lässt — Auswahl der `.tp`
//!   aus einer Pfadliste, Pfad-Normalisierung, „ist BTP lokal?", der Deckel
//!   gegen hängende Abfragen und die Abbildung des Rohbefunds auf das Urteil
//!   fürs Setup.
//! - [`windows`]: der Handle-Durchlauf. Er liefert nur einen [`RohBefund`]
//!   und ist nur am echten BTP messbar (`tests/btp_tp_pfad_probe.rs`).
//!
//! Der Prozessname ist kein Beweis: Jeder Fund läuft durch
//! [`crate::badhub::tp_upload::tp_file_check`] (Jet-Kopf, Größe, Kennung),
//! bevor er etwas bewirkt.
//!
//! Datenschutz: Der Pfad enthält den Windows-Benutzernamen. Ins Log gehört
//! nur der Dateiname ([`TpSuche::log_text`]).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Serialize;

use crate::badhub::tp_upload::TpFileInfo;

#[cfg(windows)]
mod windows;

/// Ab so vielen noch blockierten Abfrage-Threads startet keine Suche mehr.
pub const MAX_LEBENDE_HAENGER: usize = 3;
/// Der wievielte Hänger eine Suche beendet: der erste wird übersprungen.
pub const HAENGER_JE_SUCHE: usize = 2;

/// `\\?\C:\x` → `C:\x`, `\\?\UNC\server\x` → `\\server\x`. So liefert
/// `GetFinalPathNameByHandleW` die Pfade; der Rest der App erwartet die
/// gewohnte Form.
pub fn lesbarer_pfad(roh: &str) -> String {
    if let Some(rest) = roh.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        roh.strip_prefix(r"\\?\").unwrap_or(roh).to_string()
    }
}

/// Was die offenen Dateien des Prozesses an Turnierdateien hergeben.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auswahl {
    Keine,
    /// Genau eine `.tp`, in lesbarer Form.
    Eine(String),
    /// Zahl der verschiedenen `.tp` — die Automatik rät dann nicht.
    Mehrere(usize),
}

/// Sucht die `.tp` unter den offenen Dateien. Endung ohne Groß/Klein;
/// derselbe Pfad über zwei Handles zählt einmal. BTPs eigene Sperrdateien
/// (`…\tp.ldb`, `<Turnier>.ldb`) enden nicht auf `.tp` und fallen heraus.
pub fn waehle_tp(pfade: &[String]) -> Auswahl {
    let mut gefunden: Vec<String> = Vec::new();
    for roh in pfade {
        let pfad = lesbarer_pfad(roh);
        let ist_tp = Path::new(&pfad)
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("tp"));
        if ist_tp
            && !gefunden
                .iter()
                .any(|g| g.to_lowercase() == pfad.to_lowercase())
        {
            gefunden.push(pfad);
        }
    }
    match gefunden.len() {
        0 => Auswahl::Keine,
        1 => Auswahl::Eine(gefunden.remove(0)),
        n => Auswahl::Mehrere(n),
    }
}

/// Liegt der Pfad auf einem lokalen Laufwerk (`X:\…`)? Alles andere —
/// Netzfreigabe (`\\server\…`), Gerätepfad (`\\.\…`), `GLOBALROOT` — öffnet
/// die Suche nicht selbst: Ein Prozess, der sich nur `btp.exe` nennt, könnte
/// bts-light sonst dazu bringen, sich an einem fremden Server anzumelden
/// (SMB/NTLM). Eine Datei im Netz wählt der Turnierleiter von Hand.
/// `GetFinalPathNameByHandleW` löst verbundene Netzlaufwerke zur Freigabe
/// auf; ein Laufwerksbuchstabe heißt hier also wirklich „lokal".
pub fn ist_lokaler_pfad(pfad: &str) -> bool {
    let b = pfad.as_bytes();
    b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}

/// Zeigt die BTP-Adresse auf diesen PC? Nur Loopback zählt — die LAN-IP
/// oder der Rechnername des eigenen PCs gelten bewusst als „nicht lokal"
/// (Spec, Annahmen): Sonst müsste die Suche Namen auflösen und könnte ein
/// zufällig lokal offenes Test-BTP für das Turnier-BTP halten.
pub fn btp_ist_lokal(host: &str) -> bool {
    let h = host.trim().trim_matches(['[', ']']);
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    h.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// Hält fest, wie viele Abfrage-Threads noch an einem Handle hängen. Ein
/// blockierter Thread lässt sich nicht beenden; ohne Deckel sammelte der
/// 60-s-Takt sie an.
#[derive(Debug, Default)]
pub struct HaengerDeckel {
    lebend: AtomicUsize,
}

impl HaengerDeckel {
    pub const fn neu() -> Self {
        Self {
            lebend: AtomicUsize::new(0),
        }
    }

    /// Darf eine neue Suche starten?
    pub fn suche_erlaubt(&self) -> bool {
        self.lebend.load(Ordering::Relaxed) < MAX_LEBENDE_HAENGER
    }

    /// Eine Abfrage hat die Frist gerissen; ihr Thread lebt weiter.
    pub fn haenger_gemeldet(&self) {
        self.lebend.fetch_add(1, Ordering::Relaxed);
    }

    /// Ein als Hänger gemeldeter Thread ist doch noch zurückgekehrt.
    pub fn aufgewacht(&self) {
        // Von Hand statt `fetch_update`/`try_update`: Der Name hängt an der
        // Toolchain-Version. Unter null fällt der Zähler nie.
        let mut n = self.lebend.load(Ordering::Relaxed);
        while n > 0 {
            match self
                .lebend
                .compare_exchange_weak(n, n - 1, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(jetzt) => n = jetzt,
            }
        }
    }
}

/// Was der Handle-Durchlauf gesehen hat — ohne jede Bewertung.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RohBefund {
    /// Zahl der laufenden `btp.exe`.
    pub prozesse: usize,
    /// Windows hat den Handle-Zugriff verweigert (BTP als Administrator).
    pub kein_zugriff: bool,
    /// Die Suche wurde wegen hängender Abfragen abgebrochen.
    pub haenger: bool,
    /// Pfade aller offenen Datenträger-Dateien, roh (`\\?\…`).
    pub pfade: Vec<String>,
}

/// Ergebnis einer Suche — serialisiert ans Frontend (`art`-Tag).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum TpSuche {
    /// Genau eine offene `.tp`, die die Dateiprüfung besteht.
    Gefunden {
        pfad: String,
        name: String,
        info: TpFileInfo,
    },
    BtpNichtGestartet,
    KeinTurnierOffen,
    /// BTP läuft als Administrator (oder unter einem anderen Benutzer).
    KeinZugriff,
    /// Abfragen blockierten; die Suche wurde abgebrochen oder gar nicht
    /// erst gestartet ([`HaengerDeckel`]).
    Haenger,
    Mehrere,
    /// Eine `.tp` ist offen, fällt aber in der Dateiprüfung durch.
    Vorpruefung {
        name: String,
        info: TpFileInfo,
    },
    /// Die offene `.tp` liegt nicht auf einem lokalen Laufwerk; die Suche
    /// öffnet sie nicht ([`ist_lokaler_pfad`]).
    Netzlaufwerk {
        name: String,
    },
    /// BTP-Adresse zeigt nicht auf diesen PC, oder bts-light ist Slave.
    NichtLokal,
    /// Kein Windows.
    NichtVerfuegbar,
    LaeuftSchon,
}

impl TpSuche {
    /// Zeile fürs Log: die Art, bei einem Fund der **Dateiname** — nie der
    /// Pfad (Windows-Benutzername; die Logs werden hochgeladen).
    pub fn log_text(&self) -> String {
        match self {
            Self::Gefunden { name, .. } => format!("gefunden ({name})"),
            Self::Vorpruefung { name, .. } => format!("Datei fällt durch die Prüfung ({name})"),
            Self::BtpNichtGestartet => "BTP läuft nicht".into(),
            Self::KeinTurnierOffen => "kein Turnier offen".into(),
            Self::KeinZugriff => "kein Zugriff auf BTP".into(),
            Self::Haenger => "Abfrage hängt".into(),
            Self::Mehrere => "mehrere Turnierdateien offen".into(),
            Self::Netzlaufwerk { name } => format!("Datei liegt im Netz ({name})"),
            Self::NichtLokal => "BTP nicht lokal".into(),
            Self::NichtVerfuegbar => "nicht verfügbar".into(),
            Self::LaeuftSchon => "läuft schon".into(),
        }
    }
}

/// Bewertet den Rohbefund. `pruefe` ist die Dateiprüfung
/// ([`crate::badhub::tp_upload::tp_file_check`]), für Tests austauschbar.
///
/// Reihenfolge: Ein Fund schlägt alles — hing nebenbei ein anderes Handle
/// oder verweigerte ein zweites `btp.exe` den Zugriff, ist die Datei trotzdem
/// gefunden. Ohne Fund zählt der schwerste Grund zuerst.
pub fn befund_zu_suche(roh: &RohBefund, pruefe: impl Fn(&Path) -> TpFileInfo) -> TpSuche {
    if roh.prozesse == 0 {
        return TpSuche::BtpNichtGestartet;
    }
    match waehle_tp(&roh.pfade) {
        Auswahl::Eine(pfad) => {
            let name = dateiname(&pfad);
            if !ist_lokaler_pfad(&pfad) {
                // Vor jeder Lesung: eine Netzdatei fasst die Suche nicht an.
                return TpSuche::Netzlaufwerk { name };
            }
            let info = pruefe(Path::new(&pfad));
            if matches!(info, TpFileInfo::Erkannt { .. }) {
                TpSuche::Gefunden { pfad, name, info }
            } else {
                TpSuche::Vorpruefung { name, info }
            }
        }
        Auswahl::Mehrere(_) => TpSuche::Mehrere,
        Auswahl::Keine if roh.kein_zugriff => TpSuche::KeinZugriff,
        Auswahl::Keine if roh.haenger => TpSuche::Haenger,
        Auswahl::Keine => TpSuche::KeinTurnierOffen,
    }
}

/// Dateiname eines Windows-Pfads. Von Hand getrennt, damit die Tests auch
/// unter Linux gelten (dort ist `\` kein Pfadtrenner).
fn dateiname(pfad: &str) -> String {
    pfad.rsplit(['\\', '/']).next().unwrap_or(pfad).to_string()
}

/// Höchstens eine Suche gleichzeitig, prozessweit.
static LAEUFT: AtomicBool = AtomicBool::new(false);
/// Prozessweiter Zähler der hängenden Abfrage-Threads.
#[cfg_attr(not(windows), allow(dead_code))]
static DECKEL: HaengerDeckel = HaengerDeckel::neu();

/// Gibt [`LAEUFT`] beim Verlassen frei — auch bei einer Panik im Durchlauf.
struct Laufsperre;

impl Laufsperre {
    fn nehmen() -> Option<Self> {
        LAEUFT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for Laufsperre {
    fn drop(&mut self) {
        LAEUFT.store(false, Ordering::Release);
    }
}

/// Sucht die in BTP geöffnete Turnierdatei.
///
/// `host` ist die BTP-Adresse, `slave` der Slave-Modus — beides so, wie es
/// gerade gilt (im Setup die Eingabe, sonst die Config). Blockiert bis zu
/// einigen Sekunden und liest die gefundene Datei einmal ganz: nie auf dem
/// UI-Thread und nie unter dem Schreib-Gate rufen.
pub fn suche(host: &str, slave: bool) -> TpSuche {
    suche_mit(host, slave, crate::badhub::tp_upload::tp_file_check)
}

/// Wie [`suche`], aber nur mit der billigen Vorprüfung (19 Bytes): Der Fund
/// trägt dann **keine** Kennung. Für den Takt — er liest die Datei erst
/// ganz, wenn sich der Pfad geändert hat, nicht jede Minute.
pub fn suche_ohne_kennung(host: &str, slave: bool) -> TpSuche {
    suche_mit(host, slave, crate::badhub::tp_upload::tp_file_vorpruefung)
}

fn suche_mit(host: &str, slave: bool, pruefe: impl Fn(&Path) -> TpFileInfo) -> TpSuche {
    let _ = &pruefe;
    if !cfg!(windows) {
        return TpSuche::NichtVerfuegbar;
    }
    if slave || !btp_ist_lokal(host) {
        return TpSuche::NichtLokal;
    }
    let Some(_sperre) = Laufsperre::nehmen() else {
        return TpSuche::LaeuftSchon;
    };
    #[cfg(windows)]
    {
        if !DECKEL.suche_erlaubt() {
            return TpSuche::Haenger;
        }
        let roh = windows::befund(&DECKEL);
        befund_zu_suche(&roh, pruefe)
    }
    #[cfg(not(windows))]
    {
        TpSuche::NichtVerfuegbar
    }
}

// ── Automatik außerhalb des Setups (PR 2, ADR 0097) ─────────────────────

/// Zwei Windows-Pfade meinen dieselbe Datei (Groß/Klein egal).
pub fn gleicher_pfad(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Ein Fund in der Form, die die Automatik braucht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fund {
    pub pfad: String,
    pub name: String,
    /// Turnier-Kennung aus der Datei; `None`, wenn sie keine eindeutige nennt.
    pub guid: Option<String>,
}

impl Fund {
    /// Nur aus einem erkannten Fund; alles andere ist kein Fund.
    pub fn aus(suche: &TpSuche) -> Option<Self> {
        match suche {
            TpSuche::Gefunden {
                pfad,
                name,
                info: TpFileInfo::Erkannt { guid, .. },
            } => Some(Self {
                pfad: pfad.clone(),
                name: name.clone(),
                guid: guid.clone(),
            }),
            _ => None,
        }
    }
}

/// Offene Rückfrage an den Turnierleiter: Die in BTP geöffnete Datei gehört
/// nicht (erkennbar) zum eingetragenen Turnier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frage {
    pub pfad: String,
    pub name: String,
    /// Kennung aus der Datei; `None` = die Datei nennt keine.
    pub datei_guid: Option<String>,
    /// Kanonische Kennung, die beim Stellen der Frage eingetragen war.
    pub eingetragen: String,
}

/// Was die Automatik mit einem Fund tut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aktion {
    /// Automatik aus, oder keine gültige Kennung eingetragen — dann führt
    /// ohnehin der Assistent zum Setup, das die Kennung sichtbar füllt.
    Nichts,
    /// Die Datei ist schon für dieses Turnier gebunden.
    Passt,
    /// Still übernehmen: Pfad binden, Kennung bleibt.
    Uebernehmen {
        pfad: String,
        bindung: String,
    },
    Fragen(Frage),
}

/// Entscheidet über einen Fund (ADR 0097). Außerhalb des Setups wird **nur**
/// bei gleicher Kennung still übernommen — eine leere Kennung füllt hier
/// niemand ohne Zutun (Security-Review 15.11.2026), das tut das Setup.
pub fn entscheide(cfg: &crate::config::AppConfig, fund: &Fund) -> Aktion {
    // badhub-Turniere haben keine TP-Datei (badhub-Spec „Turnierseite und
    // Liveticker“); ihre GUID passt nie zu einer — die Automatik fragte sonst
    // jeden Takt erneut.
    if !cfg.tp_upload.finden_automatisch || cfg.quelle != crate::config::QuelleArt::Btp {
        return Aktion::Nichts;
    }
    let Some(eingetragen) = cfg.tournament_uuid_kanonisch() else {
        return Aktion::Nichts;
    };
    let gebunden = cfg
        .tp_datei_fuer_turnier()
        .is_some_and(|p| gleicher_pfad(&p.to_string_lossy(), &fund.pfad));
    if gebunden {
        return Aktion::Passt;
    }
    match &fund.guid {
        Some(g) if *g == eingetragen => Aktion::Uebernehmen {
            pfad: fund.pfad.clone(),
            bindung: eingetragen,
        },
        _ => Aktion::Fragen(Frage {
            pfad: fund.pfad.clone(),
            name: fund.name.clone(),
            datei_guid: fund.guid.clone(),
            eingetragen,
        }),
    }
}

/// Was die Automatik über die Takte hinweg behält. Lebt im `AppState`, nicht
/// in der `SyncEngine` — die wird bei jedem Speichern neu gebaut.
#[derive(Debug, Default)]
pub struct Merker {
    /// Die offene Rückfrage. Sie bleibt stehen, bis sie beantwortet oder
    /// zurückgezogen ist — gestellt wird sie je Fund also nur einmal.
    pub frage: Option<Frage>,
    /// Dateiname der zuletzt **still** übernommenen Datei — die Oberfläche
    /// zeigt den Wechsel an, bis ihn jemand wegklickt (Security-Review: ein
    /// stiller Wechsel darf nicht unsichtbar sein, etwa wenn in BTP eine
    /// Sicherungskopie desselben Turniers geöffnet wurde).
    pub gewechselt: Option<String>,
    /// Pfad einer offenen Datei, die die volle Prüfung nicht bestand, und
    /// wie viele Takte seither vergingen — damit eine gesperrte oder
    /// defekte Datei nicht jede Minute ganz gelesen wird.
    pub abgelehnt: Option<(String, u8)>,
    /// Der Hänger wurde schon gemeldet (eine Warnzeile, nicht jede Minute).
    pub haenger_gemeldet: bool,
}

/// Nach so vielen Takten wird eine abgelehnte Datei erneut ganz geprüft.
pub const ABGELEHNT_NEU_PRUEFEN_NACH: u8 = 10;

impl Merker {
    /// Ist für genau diese Datei und diese eingetragene Kennung schon eine
    /// Frage offen? Dann muss der Takt die Datei nicht noch einmal lesen.
    pub fn fragt_schon(&self, pfad: &str, eingetragen: &str) -> bool {
        self.frage
            .as_ref()
            .is_some_and(|f| gleicher_pfad(&f.pfad, pfad) && f.eingetragen == eingetragen)
    }

    /// Wurde genau diese Datei kürzlich abgelehnt? Zählt den Takt mit; nach
    /// [`ABGELEHNT_NEU_PRUEFEN_NACH`] Takten heißt es wieder „nein" — dann
    /// wird neu geprüft (die Datei war vielleicht nur kurz gesperrt).
    pub fn kuerzlich_abgelehnt(&mut self, pfad: &str) -> bool {
        match &mut self.abgelehnt {
            Some((p, takte)) if gleicher_pfad(p, pfad) => {
                *takte += 1;
                if *takte >= ABGELEHNT_NEU_PRUEFEN_NACH {
                    self.abgelehnt = None;
                    false
                } else {
                    true
                }
            }
            _ => {
                self.abgelehnt = None;
                false
            }
        }
    }
}

/// Ein Schritt der Automatik: bewertet das Suchergebnis, pflegt die offene
/// Frage und nennt, was still zu übernehmen ist (`(Pfad, Bindung)`).
///
/// Die Frage wird zurückgezogen, sobald sie sich erledigt hat: BTP zeigt
/// wieder die passende Datei, hat gar keine mehr offen, oder die Automatik
/// ist aus. Bei „weiß nicht" (kein Zugriff, Hänger, Suche läuft schon)
/// bleibt sie stehen.
pub fn schritt(
    cfg: &crate::config::AppConfig,
    suche: &TpSuche,
    merker: &mut Merker,
) -> Option<(String, String)> {
    if !cfg.tp_upload.finden_automatisch {
        merker.frage = None;
        return None;
    }
    if matches!(
        suche,
        TpSuche::KeinZugriff | TpSuche::Haenger | TpSuche::LaeuftSchon | TpSuche::NichtVerfuegbar
    ) {
        return None;
    }
    let Some(fund) = Fund::aus(suche) else {
        merker.frage = None;
        return None;
    };
    match entscheide(cfg, &fund) {
        Aktion::Nichts | Aktion::Passt => {
            merker.frage = None;
            None
        }
        Aktion::Uebernehmen { pfad, bindung } => {
            merker.frage = None;
            Some((pfad, bindung))
        }
        Aktion::Fragen(frage) => {
            merker.frage = Some(frage);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn erkannt(guid: Option<&str>) -> TpFileInfo {
        TpFileInfo::Erkannt {
            groesse: 1,
            anzeige: "Turnierdatei erkannt, 0,0 MB".into(),
            guid: guid.map(str::to_string),
        }
    }

    fn roh(pfade: &[&str]) -> RohBefund {
        RohBefund {
            prozesse: 1,
            pfade: pfade.iter().map(|p| p.to_string()).collect(),
            ..RohBefund::default()
        }
    }

    #[test]
    fn entscheide_fragt_bei_quelle_badhub_nie() {
        let mut cfg = crate::config::AppConfig {
            tournament_uuid: "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".into(),
            ..Default::default()
        };
        cfg.tp_upload.finden_automatisch = true;
        let fund = Fund {
            pfad: "C:/t.tp".into(),
            name: "t.tp".into(),
            guid: Some("6B2352E2-2231-45A2-A338-5ED0CB2E1A5C".into()),
        };
        assert!(matches!(entscheide(&cfg, &fund), Aktion::Fragen(_)));
        cfg.quelle = crate::config::QuelleArt::Badhub;
        assert!(matches!(entscheide(&cfg, &fund), Aktion::Nichts));
    }

    #[test]
    fn lesbarer_pfad_entfernt_praefix_und_unc() {
        assert_eq!(lesbarer_pfad(r"\\?\C:\Turniere\a.TP"), r"C:\Turniere\a.TP");
        assert_eq!(
            lesbarer_pfad(r"\\?\UNC\server\freigabe\a.TP"),
            r"\\server\freigabe\a.TP"
        );
        // Ohne Präfix unverändert.
        assert_eq!(lesbarer_pfad(r"D:\a.tp"), r"D:\a.tp");
    }

    #[test]
    fn waehle_tp_ignoriert_ldb_und_gross_klein() {
        // Der gemessene Bestand eines laufenden BTP (15.11.2026): Turnierdatei,
        // ihre Sperrdatei und die Sperrdatei der Programmdatenbank.
        let pfade = [
            r"\\?\C:\ProgramData\Visual Reality\Badminton Turnier Planer\tp.ldb".to_string(),
            r"\\?\C:\Users\x\Turniere\Test.TP".to_string(),
            r"\\?\C:\Users\x\Turniere\Test.ldb".to_string(),
            r"\\?\C:\Windows\System32\de-DE\x.mui".to_string(),
            // Dieselbe Datei über ein zweites Handle, anders geschrieben.
            r"\\?\c:\users\x\turniere\test.tp".to_string(),
        ];
        assert_eq!(
            waehle_tp(&pfade),
            Auswahl::Eine(r"C:\Users\x\Turniere\Test.TP".to_string())
        );
        assert_eq!(waehle_tp(&[]), Auswahl::Keine);
        // Ein Ordner namens „tp" oder eine Datei „tp" ohne Endung zählt nicht.
        assert_eq!(waehle_tp(&[r"\\?\C:\tp".to_string()]), Auswahl::Keine);
    }

    #[test]
    fn waehle_tp_meldet_mehrere() {
        let pfade = [r"\\?\C:\a.tp".to_string(), r"\\?\C:\b.TP".to_string()];
        assert_eq!(waehle_tp(&pfade), Auswahl::Mehrere(2));
    }

    #[test]
    fn btp_ist_lokal_nur_loopback() {
        for lokal in [
            "127.0.0.1",
            " localhost ",
            "LOCALHOST",
            "127.0.0.5",
            "::1",
            "[::1]",
        ] {
            assert!(btp_ist_lokal(lokal), "{lokal}");
        }
        // LAN-IP, Rechnername und Leeres gelten als nicht lokal.
        for fremd in ["192.168.1.20", "turnier-pc", "", "0.0.0.0", "badhub.de"] {
            assert!(!btp_ist_lokal(fremd), "{fremd}");
        }
    }

    #[test]
    fn haenger_deckel_sperrt_ab_drei() {
        let d = HaengerDeckel::neu();
        assert!(d.suche_erlaubt());
        d.haenger_gemeldet();
        d.haenger_gemeldet();
        assert!(d.suche_erlaubt(), "zwei Hänger sperren noch nicht");
        d.haenger_gemeldet();
        assert!(!d.suche_erlaubt());
        // Wacht einer auf, geht es weiter; unter null fällt der Zähler nie.
        d.aufgewacht();
        assert!(d.suche_erlaubt());
        for _ in 0..5 {
            d.aufgewacht();
        }
        d.haenger_gemeldet();
        assert!(d.suche_erlaubt());
    }

    // ── befund_zu_suche: ein Test je Lage aus der Spec ────────────────────

    #[test]
    fn befund_btp_nicht_gestartet() {
        let r = RohBefund::default();
        assert_eq!(
            befund_zu_suche(&r, |_| unreachable!()),
            TpSuche::BtpNichtGestartet
        );
    }

    #[test]
    fn befund_kein_turnier_offen() {
        let r = roh(&[r"\\?\C:\ProgramData\Visual Reality\Badminton Turnier Planer\tp.ldb"]);
        assert_eq!(
            befund_zu_suche(&r, |_| unreachable!()),
            TpSuche::KeinTurnierOffen
        );
    }

    #[test]
    fn befund_kein_zugriff() {
        let r = RohBefund {
            prozesse: 1,
            kein_zugriff: true,
            ..RohBefund::default()
        };
        assert_eq!(
            befund_zu_suche(&r, |_| unreachable!()),
            TpSuche::KeinZugriff
        );
    }

    #[test]
    fn befund_haenger_ohne_fund() {
        let r = RohBefund {
            prozesse: 1,
            haenger: true,
            ..RohBefund::default()
        };
        assert_eq!(befund_zu_suche(&r, |_| unreachable!()), TpSuche::Haenger);
    }

    #[test]
    fn befund_mehrere() {
        let r = roh(&[r"\\?\C:\a.tp", r"\\?\C:\b.tp"]);
        assert_eq!(befund_zu_suche(&r, |_| unreachable!()), TpSuche::Mehrere);
    }

    #[test]
    fn befund_gefunden_reicht_pfad_name_und_urteil_durch() {
        let r = roh(&[r"\\?\C:\Turniere\Test.TP"]);
        let s = befund_zu_suche(&r, |p| {
            assert_eq!(p, Path::new(r"C:\Turniere\Test.TP"));
            erkannt(Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"))
        });
        let TpSuche::Gefunden { pfad, name, info } = s else {
            panic!("erwartet Gefunden, war {s:?}");
        };
        assert_eq!(pfad, r"C:\Turniere\Test.TP");
        assert_eq!(name, "Test.TP");
        assert_eq!(info, erkannt(Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA")));
    }

    #[test]
    fn befund_fund_schlaegt_haenger_und_verweigerten_zweitprozess() {
        let r = RohBefund {
            prozesse: 2,
            kein_zugriff: true,
            haenger: true,
            pfade: vec![r"\\?\C:\a.tp".to_string()],
        };
        assert!(matches!(
            befund_zu_suche(&r, |_| erkannt(None)),
            TpSuche::Gefunden { .. }
        ));
    }

    #[test]
    fn ist_lokaler_pfad_nur_mit_laufwerksbuchstabe() {
        for lokal in [r"C:\Turniere\a.TP", r"d:\a.tp"] {
            assert!(ist_lokaler_pfad(lokal), "{lokal}");
        }
        for fremd in [
            r"\\server\freigabe\a.TP",
            r"\\.\pipe\a.tp",
            r"GLOBALROOT\Device\x\a.tp",
            r"C:a.tp",
            "",
        ] {
            assert!(!ist_lokaler_pfad(fremd), "{fremd}");
        }
    }

    #[test]
    fn befund_netzlaufwerk_wird_nie_gelesen() {
        // Der Rohpfad einer Freigabe, wie Windows ihn liefert. `pruefe` darf
        // nicht laufen — schon das Öffnen wäre die Anmeldung am fremden Server.
        for pfad in [
            r"\\?\UNC\server\freigabe\Test.TP",
            r"\\?\GLOBALROOT\Device\x\Test.TP",
        ] {
            let s = befund_zu_suche(&roh(&[pfad]), |_| unreachable!("nie lesen"));
            assert_eq!(
                s,
                TpSuche::Netzlaufwerk {
                    name: "Test.TP".into()
                }
            );
        }
    }

    #[test]
    fn befund_vorpruefung_faellt_durch() {
        let r = roh(&[r"\\?\C:\a.tp"]);
        for urteil in [
            TpFileInfo::KeineTpDatei,
            TpFileInfo::ZuGross { groesse: 99 },
            TpFileInfo::NichtGefunden,
        ] {
            let s = befund_zu_suche(&r, |_| urteil.clone());
            assert!(
                matches!(&s, TpSuche::Vorpruefung { info, .. } if *info == urteil),
                "{s:?}"
            );
        }
    }

    #[test]
    fn suche_ohne_lokalen_btp_oder_ohne_windows() {
        // Unter Linux (CI) ist die Suche nie verfügbar; unter Windows greift
        // vor jedem Prozesszugriff die Lokal-Prüfung.
        let erwartet = if cfg!(windows) {
            TpSuche::NichtLokal
        } else {
            TpSuche::NichtVerfuegbar
        };
        assert_eq!(suche("192.168.1.20", false), erwartet);
        assert_eq!(suche("127.0.0.1", true), erwartet);
    }

    #[test]
    fn log_text_nennt_nie_den_pfad() {
        let s = TpSuche::Gefunden {
            pfad: r"C:\Users\geheim\Turniere\Test.TP".into(),
            name: "Test.TP".into(),
            info: erkannt(None),
        };
        assert_eq!(s.log_text(), "gefunden (Test.TP)");
        assert!(!s.log_text().contains("geheim"));
    }

    // ── Automatik (PR 2) ──────────────────────────────────────────────────

    const A: &str = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA";
    const B: &str = "9AA7FE89-0000-4000-8000-000000000001";

    fn cfg(eingetragen: &str, pfad: &str, bindung: &str) -> crate::config::AppConfig {
        let mut c = crate::config::AppConfig {
            tournament_uuid: eingetragen.to_string(),
            ..Default::default()
        };
        c.tp_upload.path = pfad.to_string();
        c.tp_upload.tournament_uuid = bindung.to_string();
        c
    }

    fn gefunden(pfad: &str, guid: Option<&str>) -> TpSuche {
        TpSuche::Gefunden {
            pfad: pfad.to_string(),
            name: dateiname(pfad),
            info: erkannt(guid),
        }
    }

    #[test]
    fn gleiche_kennung_zieht_pfad_still_nach() {
        // Datei verschoben oder unter neuem Namen gespeichert (AK13).
        let c = cfg(A, r"C:\alt\t.TP", A);
        let mut m = Merker::default();
        let u = schritt(&c, &gefunden(r"D:\neu\t.TP", Some(A)), &mut m);
        assert_eq!(u, Some((r"D:\neu\t.TP".to_string(), A.to_string())));
        assert!(m.frage.is_none());
        // Auch ohne bisherigen Pfad.
        let c = cfg(A, "", "");
        assert!(schritt(&c, &gefunden(r"D:\neu\t.TP", Some(A)), &mut m).is_some());
    }

    #[test]
    fn gebundene_datei_tut_nichts() {
        let c = cfg(A, r"C:\Turniere\t.TP", A);
        let mut m = Merker::default();
        // Groß/Klein des Pfads spielt keine Rolle; die Kennung des Funds
        // braucht es dafür nicht (der Takt liest sie dann gar nicht).
        assert_eq!(
            schritt(&c, &gefunden(r"c:\turniere\T.tp", None), &mut m),
            None
        );
        assert!(m.frage.is_none());
    }

    #[test]
    fn leere_kennung_wird_ausserhalb_des_setups_nicht_gefuellt() {
        // Abweichung von der ersten Spec-Fassung (Security-Review): Ohne
        // gültige Kennung übernimmt die Automatik nichts und fragt nicht —
        // der Assistent führt dann ohnehin ins Setup.
        for eingetragen in ["", "keine-kennung"] {
            let c = cfg(eingetragen, "", "");
            let mut m = Merker::default();
            assert_eq!(schritt(&c, &gefunden(r"C:\t.TP", Some(A)), &mut m), None);
            assert!(m.frage.is_none());
        }
    }

    #[test]
    fn fremde_kennung_fragt_einmal() {
        let c = cfg(A, r"C:\alt\t.TP", A);
        let mut m = Merker::default();
        let fund = gefunden(r"C:\neu\vorjahr.TP", Some(B));
        assert_eq!(schritt(&c, &fund, &mut m), None, "nichts still übernehmen");
        let frage = m.frage.clone().expect("Frage offen");
        assert_eq!(frage.datei_guid.as_deref(), Some(B));
        assert_eq!(frage.eingetragen, A);
        assert_eq!(frage.name, "vorjahr.TP");
        // Der nächste Takt erkennt die offene Frage, ohne neu zu lesen …
        assert!(m.fragt_schon(r"c:\NEU\vorjahr.tp", A));
        // … und ein zweiter Schritt ändert nichts.
        assert_eq!(schritt(&c, &fund, &mut m), None);
        assert_eq!(m.frage, Some(frage));
    }

    #[test]
    fn ohne_kennung_fragt_und_bindet_nur_pfad() {
        let c = cfg(A, "", "");
        let mut m = Merker::default();
        assert_eq!(schritt(&c, &gefunden(r"C:\t.TP", None), &mut m), None);
        let frage = m.frage.expect("Frage offen");
        assert_eq!(frage.datei_guid, None, "Antwort Ja bindet nur den Pfad");
        assert_eq!(frage.eingetragen, A);
    }

    #[test]
    fn abgelehnte_datei_wird_nicht_jede_minute_neu_gelesen() {
        let mut m = Merker::default();
        assert!(!m.kuerzlich_abgelehnt(r"C:\kaputt.TP"));
        m.abgelehnt = Some((r"C:\kaputt.TP".into(), 0));
        // Neun Takte Ruhe, im zehnten wird wieder geprüft.
        for takt in 1..ABGELEHNT_NEU_PRUEFEN_NACH {
            assert!(m.kuerzlich_abgelehnt(r"c:\KAPUTT.tp"), "Takt {takt}");
        }
        assert!(!m.kuerzlich_abgelehnt(r"C:\kaputt.TP"));
        // Eine andere Datei gilt sofort als ungeprüft.
        m.abgelehnt = Some((r"C:\kaputt.TP".into(), 0));
        assert!(!m.kuerzlich_abgelehnt(r"C:\andere.TP"));
        assert!(m.abgelehnt.is_none());
    }

    #[test]
    fn automatik_aus_tut_nichts() {
        let mut c = cfg(A, "", "");
        c.tp_upload.finden_automatisch = false;
        let mut m = Merker {
            frage: Some(Frage {
                pfad: "x".into(),
                name: "x".into(),
                datei_guid: None,
                eingetragen: A.into(),
            }),
            ..Merker::default()
        };
        assert_eq!(schritt(&c, &gefunden(r"C:\t.TP", Some(A)), &mut m), None);
        assert!(m.frage.is_none(), "Automatik aus zieht die Frage zurück");
    }

    #[test]
    fn offene_frage_aendert_nichts() {
        // Solange die Frage offen ist, bleiben Pfad und Kennung, wie sie sind:
        // `schritt` meldet keine Übernahme, die Config fasst niemand an.
        let c = cfg(A, r"C:\alt\t.TP", A);
        let mut m = Merker::default();
        for _ in 0..3 {
            assert_eq!(schritt(&c, &gefunden(r"C:\b.TP", Some(B)), &mut m), None);
        }
        assert_eq!(c.tp_upload.path, r"C:\alt\t.TP");
        // Bei „weiß nicht" bleibt die Frage stehen.
        for unklar in [TpSuche::KeinZugriff, TpSuche::Haenger, TpSuche::LaeuftSchon] {
            schritt(&c, &unklar, &mut m);
            assert!(m.frage.is_some(), "{unklar:?}");
        }
    }

    #[test]
    fn passende_datei_zieht_frage_zurueck() {
        let c = cfg(A, r"C:\alt\t.TP", A);
        let mut m = Merker::default();
        schritt(&c, &gefunden(r"C:\b.TP", Some(B)), &mut m);
        assert!(m.frage.is_some());
        // BTP zeigt wieder die gebundene Datei.
        schritt(&c, &gefunden(r"C:\alt\t.TP", None), &mut m);
        assert!(m.frage.is_none());
        // Ebenso, wenn BTP gar kein Turnier mehr offen hat.
        schritt(&c, &gefunden(r"C:\b.TP", Some(B)), &mut m);
        schritt(&c, &TpSuche::KeinTurnierOffen, &mut m);
        assert!(m.frage.is_none());
    }

    #[test]
    fn suche_serialisiert_mit_art_tag() {
        let json = serde_json::to_value(TpSuche::KeinZugriff).unwrap();
        assert_eq!(json, serde_json::json!({ "art": "kein_zugriff" }));
        let json = serde_json::to_value(TpSuche::Vorpruefung {
            name: "a.tp".into(),
            info: TpFileInfo::KeineTpDatei,
        })
        .unwrap();
        assert_eq!(json["art"], "vorpruefung");
        assert_eq!(json["info"]["art"], "keine_tp_datei");
    }
}
