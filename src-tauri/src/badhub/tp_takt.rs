//! Takt-Upload der TP-Datei (Spec `docs/features/tp-takt-upload.md`, ADR 0061):
//! die **reine** Logik ohne Tauri, Netz und Uhr.
//!
//! Drei Fragen beantwortet dieses Modul, der Takt-Schritt in `commands.rs`
//! setzt sie zusammen:
//!
//! 1. [`einordnen`] — was bedeutet ein [`UploadAusgang`] für den Takt:
//!    Bytes sind dem Server bekannt (Merker setzen), vorübergehender Fehler
//!    (Nachversuch), Halt (ein neuer Upload hilft nicht) oder Pause (badhub
//!    kennt den Upload noch nicht).
//! 2. [`TaktZustand`] — Merker, Zähler, Termin, Halt; alle Übergänge als
//!    Methoden mit `jetzt_ms` als Parameter, damit Tests ohne Uhr laufen.
//! 3. [`bewerte_config`] — welche Config-Änderung den Halt aufhebt
//!    (Wiederanlauf), den Schalter kippt oder den Termin vorzieht.
//!
//! Der Zustand lebt in `AppState` (geflattet in `TpUploadStatus`), nie im
//! Takt-Task: Speichern im Setup stoppt und startet den Sync-Task und damit
//! den Takt — ein Termin im Task wäre dann jedes Mal weg.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use super::tp_upload::{LeseFehler, StatusAntwort, UploadAntwort};
use crate::config::AppConfig;

/// Nachversuch nach vorübergehenden Fehlern (BTP beschäftigt, Datei in
/// Bewegung, Netz, 5xx) — kurz genug für heiße Phasen, lang genug, dass BTP
/// mit seiner Eingabe fertig wird.
pub const NACHVERSUCH: Duration = Duration::from_secs(90);
/// Ereignis-Auslöser (ADR 0062): Ruhe nach dem **letzten** Ereignis bis zur
/// Prüfung — eine Serie von BTP-Eingaben (Auslosung mehrerer Konkurrenzen)
/// soll einen Upload auslösen, nicht fünf.
pub const ENTPRELLUNG: Duration = Duration::from_secs(30);
/// Deckel der Entprellung: spätestens so lange nach dem **ersten** Ereignis
/// wird geprüft, auch wenn BTP weiter Eingaben meldet.
pub const MAX_ENTPRELLUNG: Duration = Duration::from_secs(120);
/// So viele Nachversuche in Folge, dann wieder der reguläre Takt.
pub const MAX_NACHVERSUCHE: u8 = 2;
/// „Nicht lesbar“ in Folge, bis es ein Halt wird (Virenscanner, Netzlaufwerk).
pub const MAX_NICHT_LESBAR: u8 = 3;
/// Pause, wenn badhub den Upload nicht kennt (altes badhub, gesperrtes Testsystem).
pub const PAUSE_NICHT_VERFUEGBAR: Duration = Duration::from_secs(30 * 60);
/// Erster Lauf frühestens so lange nach Start, Einschalten oder Wiederanlauf.
pub const STARTVERZUG: Duration = Duration::from_secs(60);
/// So oft schaut der Takt-Task nach, ob der Termin fällig ist.
pub const PRUEFTAKT: Duration = Duration::from_secs(5);
/// Untergrenze für `Retry-After` — kürzer wäre Hämmern.
pub const MIN_RETRY_AFTER: Duration = Duration::from_secs(5);
/// Obergrenze für `Retry-After` — ein absurder Wert vom Server darf den Takt
/// nicht still für Jahre schlafen legen (Security-Review N2).
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

/// Die Bytes, die der Server zuletzt angenommen oder als bekannt quittiert
/// hat. Pfad und GUID gehören dazu: „Datei neu wählen“ oder ein GUID-Wechsel
/// machen den Merker wertlos, ohne dass jemand ihn löschen muss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendeMerker {
    pub pfad: PathBuf,
    pub guid: String,
    pub sha: [u8; 32],
}

impl SendeMerker {
    pub fn passt(&self, pfad: &Path, guid: &str, sha: &[u8; 32]) -> bool {
        self.pfad == pfad && self.guid == guid && &self.sha == sha
    }
}

/// Strukturierter Ausgang von `fuehre_upload_aus` (commands.rs). `sha` ist der
/// SHA-256 der **gesendeten** Bytes (stabil gelesen unter dem Gate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadAusgang {
    /// Gate: ein BTP-Write hielt länger als `MAX_WARTEZEIT`.
    Beschaeftigt,
    /// Lesen unter dem Gate scheiterte.
    Lesefehler(LeseFehler),
    /// Die Datei nennt in `Settings.LinkCode` eine andere Turnier-Kennung als
    /// das Setup (`datei` = kanonische GUID aus der Datei). Nicht gesendet —
    /// der Server würde mit `guid_mismatch` ablehnen; so sieht der Nutzer es
    /// ohne Netzzugriff, auch wenn BTP am Turniertag eine andere Datei öffnet.
    GuidFremd { datei: String },
    /// Jede Antwort außer 202.
    Antwort {
        antwort: UploadAntwort,
        sha: [u8; 32],
    },
    /// 202 und der Worker hat innerhalb der Frist ein `result` geliefert.
    Verarbeitet {
        status: StatusAntwort,
        sha: [u8; 32],
    },
    /// 202, aber kein `result` innerhalb der Frist.
    DauertNoch { sha: [u8; 32] },
}

/// Was der Takt aus einem Ausgang macht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Einordnung {
    /// Der Server kennt diese Bytes — Merker setzen, nächster regulärer Takt.
    /// `nachholen`: beim nächsten Schritt einmal den Status abfragen.
    Gesendet { sha: [u8; 32], nachholen: bool },
    /// Ein neuer Upload hilft nicht; Text = der Knopf-Text.
    Halt(String),
    /// badhub kennt den Upload noch nicht — 30 Minuten warten.
    Pause,
    /// Vorübergehend; `retry_after` vom Server (429/503), sonst 90 s.
    Nachversuch { retry_after: Option<Duration> },
    /// Datei nicht lesbar — eigener Zähler, Halt erst beim dritten Mal.
    NichtLesbar(String),
}

/// Ergebnis-Codes des Workers, die ein neuer Upload derselben Kennung nicht
/// heilt — auch für den nachgeholten Status (Schritt 3a).
pub fn status_fatal(status: &StatusAntwort) -> bool {
    matches!(
        status.result.as_deref(),
        Some("guid_mismatch") | Some("final_imported")
    )
}

/// Einordnungstabelle der Spec. `knopf_text` ist der fertige Dashboard-Text
/// aus `commands.rs` — bei Halt wird er übernommen, damit Takt und Knopf
/// dieselben Sätze sprechen (Handbuch-Anker).
pub fn einordnen(ausgang: &UploadAusgang, knopf_text: &str) -> Einordnung {
    use UploadAntwort as A;
    let halt = || Einordnung::Halt(knopf_text.to_string());
    let nachversuch = |s: Option<u32>| Einordnung::Nachversuch {
        retry_after: s.map(|s| Duration::from_secs(u64::from(s))),
    };
    match ausgang {
        UploadAusgang::Beschaeftigt => nachversuch(None),
        UploadAusgang::Lesefehler(LeseFehler::Unruhig) => nachversuch(None),
        UploadAusgang::Lesefehler(LeseFehler::NichtLesbar { .. }) => {
            Einordnung::NichtLesbar(knopf_text.to_string())
        }
        UploadAusgang::Lesefehler(LeseFehler::KeineTpDatei | LeseFehler::ZuGross) => halt(),
        UploadAusgang::GuidFremd { .. } => halt(),
        UploadAusgang::DauertNoch { sha } => Einordnung::Gesendet {
            sha: *sha,
            nachholen: true,
        },
        UploadAusgang::Verarbeitet { status, sha } => {
            if status_fatal(status) {
                halt()
            } else {
                Einordnung::Gesendet {
                    sha: *sha,
                    nachholen: false,
                }
            }
        }
        UploadAusgang::Antwort { antwort, sha } => match antwort {
            A::Angenommen { .. } => Einordnung::Gesendet {
                sha: *sha,
                nachholen: true,
            },
            A::Unveraendert { .. } | A::Abgelehnt { .. } => Einordnung::Gesendet {
                sha: *sha,
                nachholen: false,
            },
            A::BereitsEndgueltig | A::VerbandFalsch { .. } | A::Unauthorized => halt(),
            A::Serverfehler { code: 413, .. } => halt(),
            A::Serverfehler { .. } => nachversuch(None),
            A::NochNichtVerfuegbar => Einordnung::Pause,
            A::ZuSchnell { retry_after_s } | A::Ueberlastet { retry_after_s } => {
                nachversuch(Some(*retry_after_s))
            }
            A::NichtErreichbar | A::Status { .. } => nachversuch(None),
        },
    }
}

/// Kurzer Grund für die Takt-Texte („BTP beschäftigt — nächster Versuch 14:47“).
pub fn grund_kurz(ausgang: &UploadAusgang) -> String {
    use UploadAntwort as A;
    match ausgang {
        UploadAusgang::Beschaeftigt => "BTP beschäftigt".to_string(),
        UploadAusgang::Lesefehler(LeseFehler::Unruhig) => "Datei in Bewegung".to_string(),
        UploadAusgang::Lesefehler(LeseFehler::NichtLesbar { basisname }) => {
            format!("Datei nicht lesbar: {basisname}")
        }
        UploadAusgang::Antwort { antwort, .. } => match antwort {
            A::NichtErreichbar => "badhub nicht erreichbar".to_string(),
            A::ZuSchnell { .. } | A::Ueberlastet { .. } => "badhub bittet um Geduld".to_string(),
            A::NochNichtVerfuegbar => "badhub kennt den Upload noch nicht".to_string(),
            A::Status { code } => format!("badhub-Fehler {code}"),
            A::Serverfehler { code, .. } => format!("badhub-Fehler {code}"),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Takt-Text für einen Nachversuch oder eine Pause: Grund plus Termin, ohne
/// Handlungsaufruf — niemand hat diesen Lauf ausgelöst. `regulaer` nach dem
/// letzten erlaubten Nachversuch („nächste Prüfung“ statt „nächster Versuch“).
pub fn text_termin(grund: &str, termin_ms: u64, regulaer: bool) -> String {
    let wort = if regulaer {
        "nächste Prüfung"
    } else {
        "nächster Versuch"
    };
    format!("{grund} — {wort} {}", uhrzeit_aus_ms(termin_ms))
}

/// Unix-ms → „HH:MM“ in der Zeitzone des Rechners.
pub fn uhrzeit_aus_ms(ms: u64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_string())
}

/// Zustand des Takts. Die `pub`-Felder reisen (geflattet in `TpUploadStatus`)
/// ans Dashboard; Merker und Zähler bleiben im Prozess.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TaktZustand {
    /// Nächster Termin (Unix-ms); `None` = Takt inaktiv.
    pub naechster_ms: Option<u64>,
    /// Letzte Schnell-Prüfung (auch ohne Upload).
    pub geprueft_ms: Option<u64>,
    /// Halt-Grund; `None` = läuft.
    pub angehalten: Option<String>,
    #[serde(skip)]
    pub merker: Option<SendeMerker>,
    #[serde(skip)]
    pub nachversuche: u8,
    #[serde(skip)]
    pub nicht_lesbar: u8,
    /// Beim nächsten Schritt einmal den Status nachholen (nach „dauert noch“).
    #[serde(skip)]
    pub nachholen: bool,
    /// Ende des letzten Uploads, der den Server erreicht hat (Takt oder
    /// Knopf) — Bezugspunkt für den Mindestabstand der Ereignisse.
    #[serde(skip)]
    pub letzter_upload_ms: Option<u64>,
    /// Grund, warum der nächste Termin vorgezogen wurde („neue Spiele“,
    /// „Paarung“); `None` = regulärer Takt. Reist ans Dashboard.
    pub ereignis_grund: Option<String>,
    /// Frühester Termin, den der Server oder ein Nachversuch verlangt hat
    /// (429/503 `Retry-After`, Pause, 90-s-Nachversuch, Knopf läuft). Ein
    /// Ereignis unterbietet ihn nie — die Bremse beim Server bleibt.
    #[serde(skip)]
    pub sperre_bis_ms: Option<u64>,
    /// Offenes Entprell-Fenster: erstes Ereignis und spätester Termin.
    #[serde(skip)]
    pub ereignis_fenster: Option<(u64, u64)>,
    /// Nach einem Ereignis war die Datei noch unverändert und wurde einmal
    /// nach 90 s nachgeprüft — beim zweiten Mal gilt wieder der Takt.
    #[serde(skip)]
    pub ereignis_nachgeprueft: bool,
    /// Ereignis, das während eines fälligen/laufenden Schritts kam: wird
    /// nach dem Schritt angewendet statt zu verfallen (Grund, Mindestabstand).
    #[serde(skip)]
    pub ereignis_offen: Option<(String, Duration)>,
}

/// Wie der Schritt den neuen Termin gewählt hat — für den Text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminart {
    Regulaer,
    Nachversuch,
    Pause,
    Halt,
}

impl TaktZustand {
    fn ms(d: Duration) -> u64 {
        d.as_millis() as u64
    }

    /// Start des Takt-Tasks: ein bestehender Termin bleibt, ein fehlender
    /// oder zu naher wird auf `jetzt + STARTVERZUG` gesetzt.
    pub fn starten(&mut self, jetzt_ms: u64) {
        let fruehestens = jetzt_ms + Self::ms(STARTVERZUG);
        self.naechster_ms = Some(
            self.naechster_ms
                .map_or(fruehestens, |n| n.max(fruehestens)),
        );
    }

    /// Schalter aus oder Slave-Modus: kein Termin; Halt bleibt gespeichert.
    pub fn ausschalten(&mut self) {
        self.naechster_ms = None;
        self.ereignis_vergessen();
    }

    /// Schalter an (ohne Neustart): wie Start.
    pub fn einschalten(&mut self, jetzt_ms: u64) {
        self.naechster_ms = None;
        self.ereignis_vergessen();
        self.starten(jetzt_ms);
    }

    /// Jede Spur eines Ereignisses weg — der Termin ist dann wieder Takt,
    /// Halt oder Wiederanlauf, und die Dashboard-Zeile sagt nichts anderes.
    fn ereignis_vergessen(&mut self) {
        self.ereignis_grund = None;
        self.ereignis_fenster = None;
        self.ereignis_nachgeprueft = false;
        self.ereignis_offen = None;
    }

    /// Nach einem Schritt: ein währenddessen gekommenes Ereignis anwenden.
    fn offenes_ereignis_anwenden(&mut self, jetzt_ms: u64) {
        if let Some((grund, abstand)) = self.ereignis_offen.take() {
            self.ereignis_fenster = None;
            self.ereignis(jetzt_ms, &grund, abstand);
        }
    }

    /// Minutenwert geändert: einen fernen Termin auf höchstens +60 s vorziehen.
    pub fn minuten_geaendert(&mut self, jetzt_ms: u64) {
        if let Some(n) = self.naechster_ms {
            self.naechster_ms = Some(n.min(jetzt_ms + Self::ms(STARTVERZUG)));
        }
    }

    /// Wiederanlauf (relevante Config-Änderung): Halt weg, Merker weg, Zähler 0.
    pub fn wiederanlauf(&mut self, jetzt_ms: u64) {
        self.angehalten = None;
        self.merker = None;
        self.nachversuche = 0;
        self.nicht_lesbar = 0;
        self.nachholen = false;
        self.sperre_bis_ms = None;
        self.ereignis_vergessen();
        self.naechster_ms = Some(jetzt_ms + Self::ms(STARTVERZUG));
    }

    /// Schnell-Hash gleich dem Merker: nur Stempel und Termin. Kam die
    /// Prüfung durch ein Ereignis und die Datei ist noch unverändert (BTP
    /// meldet die Auslosung im Protokoll, schreibt sie aber erst danach in
    /// die Datei), wird **einmal** nach 90 s nachgeprüft, dann gilt der Takt.
    pub fn unveraendert(&mut self, jetzt_ms: u64, takt: Duration) {
        self.geprueft_ms = Some(jetzt_ms);
        self.nicht_lesbar = 0;
        self.sperre_bis_ms = None;
        self.ereignis_fenster = None;
        if self.ereignis_grund.is_some() && !self.ereignis_nachgeprueft {
            self.ereignis_nachgeprueft = true;
            self.naechster_ms = Some(jetzt_ms + Self::ms(NACHVERSUCH));
        } else {
            self.ereignis_grund = None;
            self.ereignis_nachgeprueft = false;
            self.naechster_ms = Some(jetzt_ms + Self::ms(takt));
        }
        self.offenes_ereignis_anwenden(jetzt_ms);
    }

    /// Knopf-Upload läuft gerade: später noch einmal, ohne Zähler.
    pub fn knopf_laeuft(&mut self, jetzt_ms: u64) {
        self.naechster_ms = Some(jetzt_ms + Self::ms(NACHVERSUCH));
        self.sperre_bis_ms = self.naechster_ms;
    }

    /// Ein Knopf-Upload hat den Server erreicht: hebt den Halt auf und merkt
    /// die Bytes, damit der Takt sie nicht gleich noch einmal schickt.
    /// `nachholen`: der Knopf bekam „dauert noch“ — der Takt fragt beim
    /// nächsten Schritt einmal nach, sonst entginge ihm ein späteres
    /// `guid_mismatch` (dieselben Bytes gehen ja nicht mehr raus).
    pub fn knopf_gesendet(&mut self, pfad: &Path, guid: &str, sha: [u8; 32], nachholen: bool) {
        self.angehalten = None;
        self.nachversuche = 0;
        self.nicht_lesbar = 0;
        self.nachholen = nachholen;
        self.sperre_bis_ms = None;
        self.ereignis_vergessen();
        self.merker = Some(SendeMerker {
            pfad: pfad.to_path_buf(),
            guid: guid.to_string(),
            sha,
        });
    }

    /// Halt aus Vorbedingung oder nachgeholtem Status (ohne Upload).
    pub fn halt(&mut self, text: String, jetzt_ms: u64, takt: Duration) {
        self.angehalten = Some(text);
        self.ereignis_vergessen();
        self.naechster_ms = Some(jetzt_ms + Self::ms(takt));
    }

    /// Einordnung eines Upload-Ausgangs anwenden; liefert die Terminart für
    /// den Text. `pfad`/`guid` gehören zum Merker.
    pub fn anwenden(
        &mut self,
        e: Einordnung,
        jetzt_ms: u64,
        takt: Duration,
        pfad: &Path,
        guid: &str,
    ) -> Terminart {
        self.geprueft_ms = Some(jetzt_ms);
        // Der Schritt ist gelaufen — ein vorgezogener Termin ist verbraucht.
        // Ein währenddessen gekommenes Ereignis (`ereignis_offen`) bleibt.
        self.ereignis_grund = None;
        self.ereignis_fenster = None;
        self.ereignis_nachgeprueft = false;
        let art = self.anwenden_termin(e, jetzt_ms, takt, pfad, guid);
        // Nicht-reguläre Termine sind eine Bremse, die kein Ereignis unterbietet.
        self.sperre_bis_ms = match art {
            Terminart::Nachversuch | Terminart::Pause => self.naechster_ms,
            Terminart::Regulaer | Terminart::Halt => None,
        };
        if art != Terminart::Halt {
            self.offenes_ereignis_anwenden(jetzt_ms);
        }
        art
    }

    fn anwenden_termin(
        &mut self,
        e: Einordnung,
        jetzt_ms: u64,
        takt: Duration,
        pfad: &Path,
        guid: &str,
    ) -> Terminart {
        match e {
            Einordnung::Gesendet { sha, nachholen } => {
                self.merker = Some(SendeMerker {
                    pfad: pfad.to_path_buf(),
                    guid: guid.to_string(),
                    sha,
                });
                self.nachversuche = 0;
                self.nicht_lesbar = 0;
                self.nachholen = nachholen;
                self.letzter_upload_ms = Some(jetzt_ms);
                self.naechster_ms = Some(jetzt_ms + Self::ms(takt));
                Terminart::Regulaer
            }
            Einordnung::Halt(text) => {
                self.halt(text, jetzt_ms, takt);
                Terminart::Halt
            }
            Einordnung::Pause => {
                self.nachversuche = 0;
                self.naechster_ms = Some(jetzt_ms + Self::ms(PAUSE_NICHT_VERFUEGBAR));
                Terminart::Pause
            }
            Einordnung::Nachversuch { retry_after } => {
                // Der Zähler fällt erst mit einem Erfolg, einer Pause oder einem
                // Wiederanlauf: Nach zwei Nachversuchen gehen **alle** weiteren
                // Fehler in Folge auf den regulären Takt (Spec „ab dem dritten
                // in Folge“) — sonst gäbe es bei totem Server 90/90/Takt/90/90/…
                self.nachversuche = self.nachversuche.saturating_add(1);
                if self.nachversuche > MAX_NACHVERSUCHE {
                    self.naechster_ms = Some(jetzt_ms + Self::ms(takt));
                    Terminart::Regulaer
                } else {
                    let warte = retry_after
                        .map_or(NACHVERSUCH, |r| r.clamp(MIN_RETRY_AFTER, MAX_RETRY_AFTER));
                    self.naechster_ms = Some(jetzt_ms + Self::ms(warte));
                    Terminart::Nachversuch
                }
            }
            Einordnung::NichtLesbar(text) => {
                self.nicht_lesbar += 1;
                if self.nicht_lesbar >= MAX_NICHT_LESBAR {
                    self.nicht_lesbar = 0;
                    self.halt(text, jetzt_ms, takt);
                    Terminart::Halt
                } else {
                    self.naechster_ms = Some(jetzt_ms + Self::ms(NACHVERSUCH));
                    Terminart::Nachversuch
                }
            }
        }
    }

    /// Ereignis aus dem Sync-Lauf (ADR 0062). Regeln:
    /// - Takt aus oder angehalten → wirkungslos.
    /// - Termin fällig oder Schritt läuft → als offen merken, nach dem
    ///   Schritt anwenden (sonst ginge eine Auslosung bis zum Takt verloren).
    /// - Untergrenze: `letzter Upload + min_abstand` (Drossel aus
    ///   `TpUploadConfig::ereignis_abstand`) und eine Bremse des Servers
    ///   oder Nachversuchs (`sperre_bis_ms`).
    /// - Erstes Ereignis: Termin auf `jetzt + ENTPRELLUNG`, nur wenn das
    ///   früher ist als der bestehende. Weitere im Fenster schieben auf
    ///   `jetzt + ENTPRELLUNG`, gedeckelt durch `erstes + MAX_ENTPRELLUNG`
    ///   und den ursprünglichen Termin (echte Entprellung).
    ///
    /// Liefert `true`, wenn sich der Termin bewegt hat (für die Log-Zeile).
    pub fn ereignis(&mut self, jetzt_ms: u64, grund: &str, min_abstand: Duration) -> bool {
        let Some(bisher) = self.naechster_ms else {
            return false;
        };
        if self.angehalten.is_some() {
            return false;
        }
        if bisher <= jetzt_ms {
            self.ereignis_offen = Some((grund.to_string(), min_abstand));
            return false;
        }
        let untergrenze = self
            .letzter_upload_ms
            .map_or(0, |t| t + Self::ms(min_abstand))
            .max(self.sperre_bis_ms.unwrap_or(0));
        let ziel = jetzt_ms + Self::ms(ENTPRELLUNG);
        let termin = match self.ereignis_fenster {
            Some((_, spaetestens)) => ziel.min(spaetestens).max(untergrenze),
            None => {
                let termin = ziel.max(untergrenze);
                if termin >= bisher {
                    return false;
                }
                let spaetestens = bisher.min(jetzt_ms + Self::ms(MAX_ENTPRELLUNG));
                self.ereignis_fenster = Some((jetzt_ms, spaetestens));
                self.ereignis_nachgeprueft = false;
                termin
            }
        };
        self.ereignis_grund = Some(grund.to_string());
        let bewegt = termin != bisher;
        self.naechster_ms = Some(termin);
        bewegt
    }

    /// Ist der Termin fällig?
    pub fn faellig(&self, jetzt_ms: u64) -> bool {
        self.naechster_ms.is_some_and(|n| n <= jetzt_ms)
    }
}

/// Was eine gespeicherte Config-Änderung für den Takt bedeutet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigWechsel {
    /// Eines der sechs Felder, hinter denen ein Halt stehen kann, hat sich geändert.
    pub wiederanlauf: bool,
    /// Schalter von aus auf an.
    pub eingeschaltet: bool,
    /// Schalter von an auf aus.
    pub ausgeschaltet: bool,
    /// Minutenwert geändert (bei weiter eingeschaltetem Takt).
    pub minuten_geaendert: bool,
}

/// Vergleich alt/neu (reine Funktion). Die sechs Wiederanlauf-Felder: Pfad,
/// gebundene GUID, Turnier-GUID, Liveticker-Adresse (Quelle des Kürzels —
/// der 403-Text schickt den Nutzer genau dorthin), badhub-URL, Passwort.
pub fn bewerte_config(alt: &AppConfig, neu: &AppConfig) -> ConfigWechsel {
    let wiederanlauf = alt.tp_upload.path != neu.tp_upload.path
        || alt.tp_upload.tournament_uuid != neu.tp_upload.tournament_uuid
        || alt.tournament_uuid != neu.tournament_uuid
        || alt.badhub.live_url != neu.badhub.live_url
        || alt.badhub.url != neu.badhub.url
        || alt.badhub.password != neu.badhub.password;
    let war_an = alt.tp_upload.auto_enabled;
    let ist_an = neu.tp_upload.auto_enabled;
    ConfigWechsel {
        wiederanlauf,
        eingeschaltet: !war_an && ist_an,
        ausgeschaltet: war_an && !ist_an,
        minuten_geaendert: ist_an && alt.tp_upload.auto_minutes != neu.tp_upload.auto_minutes,
    }
}

impl TaktZustand {
    /// Config-Wechsel anwenden (Reihenfolge: Wiederanlauf schlägt alles,
    /// dann Schalter, dann Minuten).
    pub fn config_gewechselt(&mut self, w: ConfigWechsel, jetzt_ms: u64) {
        if w.wiederanlauf {
            self.wiederanlauf(jetzt_ms);
        }
        if w.ausgeschaltet {
            self.ausschalten();
        } else if w.eingeschaltet {
            self.einschalten(jetzt_ms);
        } else if w.minuten_geaendert {
            self.minuten_geaendert(jetzt_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(15 * 60);
    const J: u64 = 1_000_000;
    fn pf() -> PathBuf {
        PathBuf::from(r"C:\t\x.tp")
    }
    fn sha(n: u8) -> [u8; 32] {
        [n; 32]
    }
    fn antwort(a: UploadAntwort) -> UploadAusgang {
        UploadAusgang::Antwort {
            antwort: a,
            sha: sha(1),
        }
    }
    fn status(result: &str) -> UploadAusgang {
        UploadAusgang::Verarbeitet {
            status: StatusAntwort {
                result: Some(result.to_string()),
                matches: None,
                message: None,
            },
            sha: sha(2),
        }
    }

    /// Standard-Mindestabstand der Ereignisse (5 min, `TP_EREIGNIS_STANDARD_MINUTEN`).
    const ABSTAND: Duration = Duration::from_secs(5 * 60);

    #[test]
    fn ereignis_gedrosselt_wartet_den_groesseren_abstand_ab() {
        // Drossel 20 min: 1 min nach einem Upload geht der Termin auf
        // Upload + 20 min — sofern der reguläre Takt nicht ohnehin früher kommt.
        let takt = Duration::from_secs(60 * 60);
        let mut z = TaktZustand::default();
        z.starten(0);
        z.anwenden(
            Einordnung::Gesendet {
                sha: sha(1),
                nachholen: false,
            },
            100_000,
            takt,
            Path::new("x.tp"),
            "G",
        );
        let drossel = Duration::from_secs(20 * 60);
        assert!(z.ereignis(160_000, "neue Spiele", drossel));
        assert_eq!(z.naechster_ms, Some(100_000 + 20 * 60 * 1000));
    }

    #[test]
    fn ereignis_zieht_den_termin_auf_die_entprellung_vor() {
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        // Takt aus → kein Termin, Ereignis wirkungslos.
        assert!(!z.ereignis(1_000, "neue Spiele", ABSTAND));
        assert_eq!(z.naechster_ms, None);
        z.starten(0); // +60 s
        z.unveraendert(10_000, takt); // nächster = 10 s + 15 min
        assert!(z.ereignis(20_000, "neue Spiele", ABSTAND));
        assert_eq!(
            z.naechster_ms,
            Some(20_000 + ENTPRELLUNG.as_millis() as u64)
        );
        assert_eq!(z.ereignis_grund.as_deref(), Some("neue Spiele"));
        // Zweites Ereignis im Fenster: echte Entprellung — der Termin rückt
        // auf 30 s nach dem letzten Ereignis.
        assert!(z.ereignis(25_000, "Paarung", ABSTAND));
        assert_eq!(z.naechster_ms, Some(55_000));
    }

    #[test]
    fn entprellung_ist_nach_oben_gedeckelt() {
        // Eine Auslosung über Minuten darf den Upload nicht ewig verschieben:
        // spätestens MAX_ENTPRELLUNG nach dem ersten Ereignis.
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        z.starten(0);
        z.unveraendert(10_000, takt);
        assert!(z.ereignis(20_000, "neue Spiele", ABSTAND));
        assert!(z.ereignis(40_000, "neue Spiele", ABSTAND));
        assert_eq!(z.naechster_ms, Some(70_000));
        // Alle 20 s eine weitere Konkurrenz — jedes Mal vor dem Termin.
        for t in [60_000, 80_000, 100_000, 120_000, 135_000] {
            z.ereignis(t, "neue Spiele", ABSTAND);
        }
        assert_eq!(
            z.naechster_ms,
            Some(20_000 + MAX_ENTPRELLUNG.as_millis() as u64)
        );
    }

    #[test]
    fn ereignis_unterbietet_keine_server_bremse() {
        // 429 mit Retry-After 300 s: ein Ereignis darf nicht früher senden.
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        z.starten(0);
        z.anwenden(
            Einordnung::Nachversuch {
                retry_after: Some(Duration::from_secs(300)),
            },
            100_000,
            takt,
            Path::new("x.tp"),
            "G",
        );
        assert_eq!(z.naechster_ms, Some(400_000));
        assert!(!z.ereignis(110_000, "Paarung", ABSTAND));
        assert_eq!(z.naechster_ms, Some(400_000));
        // Pause „badhub kennt den Upload noch nicht“ ebenso.
        let mut p = TaktZustand::default();
        p.starten(0);
        p.anwenden(Einordnung::Pause, 100_000, takt, Path::new("x.tp"), "G");
        assert!(!p.ereignis(110_000, "neue Spiele", ABSTAND));
    }

    #[test]
    fn ereignis_waehrend_laufendem_schritt_wird_danach_nachgeholt() {
        // Termin fällig (Schritt läuft): das Ereignis darf nicht verfallen —
        // nach dem Upload der alten Bytes kommt die Prüfung nach Mindestabstand.
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        z.starten(0); // 60 s
        assert!(!z.ereignis(70_000, "neue Spiele", ABSTAND));
        z.anwenden(
            Einordnung::Gesendet {
                sha: sha(1),
                nachholen: false,
            },
            200_000,
            takt,
            Path::new("x.tp"),
            "G",
        );
        assert_eq!(z.naechster_ms, Some(200_000 + 300_000));
        assert_eq!(z.ereignis_grund.as_deref(), Some("neue Spiele"));
    }

    #[test]
    fn unveraenderte_datei_nach_ereignis_wird_einmal_nachgeprueft() {
        // BTP hat die Auslosung im Protokoll schon gemeldet, aber noch nicht
        // in die Datei geschrieben: ein zweiter Blick nach 90 s, dann Takt.
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        z.starten(0);
        z.unveraendert(10_000, takt);
        assert!(z.ereignis(20_000, "neue Spiele", ABSTAND));
        z.unveraendert(50_000, takt);
        assert_eq!(
            z.naechster_ms,
            Some(50_000 + NACHVERSUCH.as_millis() as u64)
        );
        assert!(z.ereignis_grund.is_some());
        z.unveraendert(140_000, takt);
        assert_eq!(z.naechster_ms, Some(140_000 + 15 * 60 * 1000));
        assert_eq!(z.ereignis_grund, None);
    }

    #[test]
    fn grund_verschwindet_bei_halt_wiederanlauf_knopf_und_ausschalten() {
        let takt = Duration::from_secs(15 * 60);
        let vorgezogen = || {
            let mut z = TaktZustand::default();
            z.starten(0);
            z.unveraendert(10_000, takt);
            assert!(z.ereignis(20_000, "Paarung", ABSTAND));
            z
        };
        let mut a = vorgezogen();
        a.halt("K".into(), 21_000, takt);
        assert_eq!(a.ereignis_grund, None);
        let mut b = vorgezogen();
        b.wiederanlauf(21_000);
        assert_eq!(b.ereignis_grund, None);
        let mut c = vorgezogen();
        c.knopf_gesendet(Path::new("x.tp"), "G", sha(1), false);
        assert_eq!(c.ereignis_grund, None);
        let mut d = vorgezogen();
        d.ausschalten();
        assert_eq!(d.ereignis_grund, None);
    }

    #[test]
    fn ereignis_haelt_mindestabstand_zum_letzten_upload() {
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        z.starten(0);
        let p = Path::new("x.tp");
        z.anwenden(
            Einordnung::Gesendet {
                sha: sha(1),
                nachholen: false,
            },
            100_000,
            takt,
            p,
            "G",
        );
        // 10 s nach dem Upload: frühestens 100 s + 5 min, nicht +30 s.
        assert!(z.ereignis(110_000, "Ergebnis", ABSTAND));
        assert_eq!(z.naechster_ms, Some(100_000 + ABSTAND.as_millis() as u64));
        // Ein regulärer Termin, der näher liegt, wird nie nach hinten gezogen.
        let mut nah = TaktZustand::default();
        nah.starten(0);
        nah.naechster_ms = Some(5_000);
        assert!(!nah.ereignis(1_000, "Ergebnis", ABSTAND));
        assert_eq!(nah.naechster_ms, Some(5_000));
    }

    #[test]
    fn ereignis_bei_halt_wirkungslos_und_knopf_zaehlt_als_upload() {
        let takt = Duration::from_secs(15 * 60);
        let mut z = TaktZustand::default();
        z.starten(0);
        z.halt("K".into(), 1_000, takt);
        assert!(!z.ereignis(2_000, "neue Spiele", ABSTAND));
        let mut k = TaktZustand::default();
        k.starten(0);
        // Regulärer Termin weit weg (letzte Prüfung bei 40 s, Takt 15 min).
        k.unveraendert(40_000, takt);
        k.knopf_gesendet(Path::new("x.tp"), "G", sha(1), false);
        k.letzter_upload_ms = Some(50_000);
        assert!(k.ereignis(51_000, "neue Spiele", ABSTAND));
        assert_eq!(k.naechster_ms, Some(50_000 + ABSTAND.as_millis() as u64));
    }

    #[test]
    fn fremde_guid_in_der_datei_ist_ein_halt() {
        // Die Datei nennt eine andere Turnier-Kennung als das Setup — ein
        // neuer Upload derselben Datei hilft nicht, der Server würde sie
        // ohnehin mit guid_mismatch ablehnen.
        let a = UploadAusgang::GuidFremd {
            datei: "99F55FAB-E301-41A2-A165-1D162CB84FC1".into(),
        };
        assert_eq!(einordnen(&a, "K"), Einordnung::Halt("K".into()));
        assert_eq!(grund_kurz(&a), "");
    }

    #[test]
    fn einordnen_tabelle_zeile_fuer_zeile() {
        use UploadAntwort as A;
        let g = |sha: [u8; 32], nachholen: bool| Einordnung::Gesendet { sha, nachholen };
        let nv = |s: Option<u64>| Einordnung::Nachversuch {
            retry_after: s.map(Duration::from_secs),
        };
        let halt = Einordnung::Halt("K".into());
        // Gesendet
        assert_eq!(einordnen(&status("ok"), "K"), g(sha(2), false));
        assert_eq!(
            einordnen(&UploadAusgang::DauertNoch { sha: sha(3) }, "K"),
            g(sha(3), true)
        );
        assert_eq!(einordnen(&status("parse_error"), "K"), g(sha(2), false));
        assert_eq!(einordnen(&status("implausible"), "K"), g(sha(2), false));
        assert_eq!(
            einordnen(
                &antwort(A::Unveraendert {
                    received_at: "x".into()
                }),
                "K"
            ),
            g(sha(1), false)
        );
        assert_eq!(
            einordnen(
                &antwort(A::Abgelehnt {
                    result: "r".into(),
                    message: "m".into()
                }),
                "K"
            ),
            g(sha(1), false)
        );
        // Halt
        assert_eq!(einordnen(&status("guid_mismatch"), "K"), halt);
        assert_eq!(einordnen(&status("final_imported"), "K"), halt);
        assert_eq!(einordnen(&antwort(A::BereitsEndgueltig), "K"), halt);
        assert_eq!(
            einordnen(
                &antwort(A::VerbandFalsch {
                    kuerzel: "bvbb".into()
                }),
                "K"
            ),
            halt
        );
        assert_eq!(einordnen(&antwort(A::Unauthorized), "K"), halt);
        assert_eq!(
            einordnen(
                &antwort(A::Serverfehler {
                    code: 413,
                    message: "zu groß".into()
                }),
                "K"
            ),
            halt
        );
        assert_eq!(
            einordnen(&UploadAusgang::Lesefehler(LeseFehler::KeineTpDatei), "K"),
            halt
        );
        assert_eq!(
            einordnen(&UploadAusgang::Lesefehler(LeseFehler::ZuGross), "K"),
            halt
        );
        // Pause
        assert_eq!(
            einordnen(&antwort(A::NochNichtVerfuegbar), "K"),
            Einordnung::Pause
        );
        // Nachversuch mit Retry-After
        assert_eq!(
            einordnen(&antwort(A::ZuSchnell { retry_after_s: 37 }), "K"),
            nv(Some(37))
        );
        assert_eq!(
            einordnen(&antwort(A::Ueberlastet { retry_after_s: 5 }), "K"),
            nv(Some(5))
        );
        // Nachversuch ohne
        assert_eq!(einordnen(&UploadAusgang::Beschaeftigt, "K"), nv(None));
        assert_eq!(
            einordnen(&UploadAusgang::Lesefehler(LeseFehler::Unruhig), "K"),
            nv(None)
        );
        assert_eq!(einordnen(&antwort(A::NichtErreichbar), "K"), nv(None));
        assert_eq!(einordnen(&antwort(A::Status { code: 502 }), "K"), nv(None));
        assert_eq!(einordnen(&antwort(A::Status { code: 302 }), "K"), nv(None));
        assert_eq!(
            einordnen(
                &antwort(A::Serverfehler {
                    code: 415,
                    message: "kaputt".into()
                }),
                "K"
            ),
            nv(None)
        );
        // Nicht lesbar
        assert_eq!(
            einordnen(
                &UploadAusgang::Lesefehler(LeseFehler::NichtLesbar {
                    basisname: "x.tp".into()
                }),
                "K"
            ),
            Einordnung::NichtLesbar("K".into())
        );
    }

    #[test]
    fn nachversuch_zweimal_90s_dann_takt_und_retry_after_mit_untergrenze() {
        let mut z = TaktZustand::default();
        let nv = Einordnung::Nachversuch { retry_after: None };
        assert_eq!(
            z.anwenden(nv.clone(), J, T, &pf(), "G"),
            Terminart::Nachversuch
        );
        assert_eq!(z.naechster_ms, Some(J + 90_000));
        assert_eq!(
            z.anwenden(nv.clone(), J, T, &pf(), "G"),
            Terminart::Nachversuch
        );
        assert_eq!(
            z.anwenden(nv.clone(), J, T, &pf(), "G"),
            Terminart::Regulaer
        );
        assert_eq!(z.naechster_ms, Some(J + 15 * 60_000));
        assert_eq!(z.nachversuche, 3, "Zähler bleibt bis zum Erfolg (M3)");
        assert_eq!(
            z.anwenden(nv.clone(), J, T, &pf(), "G"),
            Terminart::Regulaer,
            "jeder weitere Fehler in Folge: regulärer Takt"
        );
        z.anwenden(
            Einordnung::Gesendet {
                sha: sha(8),
                nachholen: false,
            },
            J,
            T,
            &pf(),
            "G",
        );
        assert_eq!(z.nachversuche, 0, "Erfolg setzt zurück");
        // Retry-After 7 s → 7 s; 2 s → 5 s (AK 6)
        let ra = |s| Einordnung::Nachversuch {
            retry_after: Some(Duration::from_secs(s)),
        };
        z.anwenden(ra(7), J, T, &pf(), "G");
        assert_eq!(z.naechster_ms, Some(J + 7_000));
        z.anwenden(ra(2), J, T, &pf(), "G");
        assert_eq!(z.naechster_ms, Some(J + 5_000));
        // Deckel: absurdes Retry-After → höchstens 1 h (Security N2); Zähler ist
        // danach > MAX, also erst wieder zurücksetzen.
        z.nachversuche = 0;
        z.anwenden(ra(4_000_000_000), J, T, &pf(), "G");
        assert_eq!(z.naechster_ms, Some(J + 3_600_000));
        // Erfolg setzt den Zähler zurück
        z.anwenden(
            Einordnung::Gesendet {
                sha: sha(9),
                nachholen: false,
            },
            J,
            T,
            &pf(),
            "G",
        );
        assert_eq!(z.nachversuche, 0);
        assert!(z.merker.as_ref().unwrap().passt(&pf(), "G", &sha(9)));
    }

    #[test]
    fn pause_halt_und_nicht_lesbar() {
        let mut z = TaktZustand::default();
        assert_eq!(
            z.anwenden(Einordnung::Pause, J, T, &pf(), "G"),
            Terminart::Pause
        );
        assert_eq!(z.naechster_ms, Some(J + 30 * 60_000));
        assert!(z.angehalten.is_none());

        assert_eq!(
            z.anwenden(Einordnung::Halt("409".into()), J, T, &pf(), "G"),
            Terminart::Halt
        );
        assert_eq!(z.angehalten.as_deref(), Some("409"));
        assert_eq!(
            z.naechster_ms,
            Some(J + 15 * 60_000),
            "Termin läuft fürs Dashboard weiter"
        );

        let mut z = TaktZustand::default();
        let nl = || Einordnung::NichtLesbar("weg".into());
        assert_eq!(z.anwenden(nl(), J, T, &pf(), "G"), Terminart::Nachversuch);
        assert_eq!(z.anwenden(nl(), J, T, &pf(), "G"), Terminart::Nachversuch);
        assert!(z.angehalten.is_none());
        assert_eq!(
            z.anwenden(nl(), J, T, &pf(), "G"),
            Terminart::Halt,
            "3. in Folge"
        );
        assert_eq!(z.angehalten.as_deref(), Some("weg"));
        // Eine erfolgreiche Lesung dazwischen setzt den Zähler zurück (AK 8a).
        let mut z = TaktZustand::default();
        z.anwenden(nl(), J, T, &pf(), "G");
        z.unveraendert(J, T);
        z.anwenden(nl(), J, T, &pf(), "G");
        z.anwenden(nl(), J, T, &pf(), "G");
        assert!(z.angehalten.is_none(), "nach Reset erst zwei");
    }

    #[test]
    fn start_schalter_minuten_und_wiederanlauf() {
        let mut z = TaktZustand::default();
        z.starten(J);
        assert_eq!(z.naechster_ms, Some(J + 60_000), "ohne Termin: +60 s");
        z.naechster_ms = Some(J + 600_000);
        z.starten(J);
        assert_eq!(
            z.naechster_ms,
            Some(J + 600_000),
            "Termin in 10 min bleibt (AK 12)"
        );
        z.naechster_ms = Some(J + 10_000);
        z.starten(J);
        assert_eq!(z.naechster_ms, Some(J + 60_000), "zu naher Termin → +60 s");
        z.minuten_geaendert(J);
        assert_eq!(z.naechster_ms, Some(J + 60_000));
        z.naechster_ms = Some(J + 30 * 60_000);
        z.minuten_geaendert(J);
        assert_eq!(z.naechster_ms, Some(J + 60_000), "ferner Termin vorgezogen");
        z.ausschalten();
        assert_eq!(z.naechster_ms, None);
        z.einschalten(J);
        assert_eq!(z.naechster_ms, Some(J + 60_000));

        z.angehalten = Some("409".into());
        z.merker = Some(SendeMerker {
            pfad: pf(),
            guid: "G".into(),
            sha: sha(1),
        });
        z.nachversuche = 2;
        z.nachholen = true;
        z.ausschalten();
        assert!(z.angehalten.is_some(), "Aus löst den Halt nicht");
        z.wiederanlauf(J);
        assert_eq!(
            z,
            TaktZustand {
                naechster_ms: Some(J + 60_000),
                ..Default::default()
            }
        );
    }

    #[test]
    fn knopf_gesendet_hebt_halt_und_setzt_merker() {
        let mut z = TaktZustand {
            angehalten: Some("409".into()),
            nachversuche: 1,
            ..Default::default()
        };
        z.knopf_gesendet(&pf(), "G", sha(4), true);
        assert!(z.nachholen, "Knopf „dauert noch“ → Takt fragt nach");
        assert!(z.angehalten.is_none());
        assert_eq!(z.nachversuche, 0);
        assert!(z.merker.as_ref().unwrap().passt(&pf(), "G", &sha(4)));
        assert!(
            !z.merker.as_ref().unwrap().passt(&pf(), "H", &sha(4)),
            "andere GUID"
        );
        assert!(!z
            .merker
            .as_ref()
            .unwrap()
            .passt(Path::new(r"C:\t\y.tp"), "G", &sha(4)));
        z.knopf_laeuft(J);
        assert_eq!(z.naechster_ms, Some(J + 90_000));
        assert_eq!(z.nachversuche, 0, "ohne Zähler (AK 10)");
    }

    #[test]
    fn bewerte_config_nur_die_sechs_felder_und_schalter() {
        let basis = {
            let mut c = AppConfig::default();
            c.tp_upload.auto_enabled = true;
            c.tp_upload.auto_minutes = 15;
            c
        };
        assert_eq!(bewerte_config(&basis, &basis), ConfigWechsel::default());
        let mut c = basis.clone();
        c.finished_warning_seconds += 1;
        c.upload_logs = !c.upload_logs;
        assert_eq!(
            bewerte_config(&basis, &c),
            ConfigWechsel::default(),
            "fremde Felder"
        );

        type Aenderung = Box<dyn Fn(&mut AppConfig)>;
        let felder: Vec<Aenderung> = vec![
            Box::new(|c| c.tp_upload.path = "x".into()),
            Box::new(|c| c.tp_upload.tournament_uuid = "x".into()),
            Box::new(|c| c.tournament_uuid = "x".into()),
            Box::new(|c| c.badhub.live_url = "x".into()),
            Box::new(|c| c.badhub.url = "x".into()),
            Box::new(|c| c.badhub.password = "x".into()),
        ];
        for (i, f) in felder.iter().enumerate() {
            let mut c = basis.clone();
            f(&mut c);
            assert!(bewerte_config(&basis, &c).wiederanlauf, "Feld {i}");
        }
        let mut c = basis.clone();
        c.tp_upload.auto_minutes = 5;
        assert_eq!(
            bewerte_config(&basis, &c),
            ConfigWechsel {
                minuten_geaendert: true,
                ..Default::default()
            }
        );
        let mut c = basis.clone();
        c.tp_upload.auto_enabled = false;
        assert!(bewerte_config(&basis, &c).ausgeschaltet);
        assert!(bewerte_config(&c, &basis).eingeschaltet);
        // Minuten + aus zugleich: nur „aus“ zählt.
        let mut c2 = c.clone();
        c2.tp_upload.auto_minutes = 7;
        let w = bewerte_config(&basis, &c2);
        assert!(w.ausgeschaltet && !w.minuten_geaendert);

        // Anwenden: Wiederanlauf schlägt alles; dann Schalter; dann Minuten.
        let mut z = TaktZustand {
            angehalten: Some("x".into()),
            ..Default::default()
        };
        z.config_gewechselt(
            ConfigWechsel {
                wiederanlauf: true,
                ausgeschaltet: true,
                ..Default::default()
            },
            J,
        );
        assert!(z.angehalten.is_none() && z.naechster_ms.is_none());
    }

    #[test]
    fn config_gewechselt_mit_echtem_paar_minuten_und_fremdes_feld() {
        let mut alt = AppConfig::default();
        alt.tp_upload.auto_enabled = true;
        alt.tp_upload.auto_minutes = 30;
        // Nur Minuten geändert: ferner Termin → +60 s (AK 8).
        let mut neu = alt.clone();
        neu.tp_upload.auto_minutes = 5;
        let mut z = TaktZustand {
            naechster_ms: Some(J + 30 * 60_000),
            ..Default::default()
        };
        z.config_gewechselt(bewerte_config(&alt, &neu), J);
        assert_eq!(z.naechster_ms, Some(J + 60_000));
        // Fremdes Feld: Zustand unverändert.
        let mut neu = alt.clone();
        neu.finished_warning_seconds += 1;
        let vorher = TaktZustand {
            naechster_ms: Some(J + 30 * 60_000),
            angehalten: Some("x".into()),
            nachversuche: 1,
            ..Default::default()
        };
        let mut z = vorher.clone();
        z.config_gewechselt(bewerte_config(&alt, &neu), J);
        assert_eq!(z, vorher);
    }

    #[test]
    fn texte_ohne_handlungsaufruf_und_grund_kurz() {
        let t = text_termin("BTP beschäftigt", J, false);
        assert!(t.starts_with("BTP beschäftigt — nächster Versuch "), "{t}");
        assert!(!t.contains("bitte gleich noch einmal"));
        assert!(text_termin("x", J, true).contains("nächste Prüfung"));
        assert_eq!(grund_kurz(&UploadAusgang::Beschaeftigt), "BTP beschäftigt");
        assert_eq!(
            grund_kurz(&antwort(UploadAntwort::ZuSchnell { retry_after_s: 1 })),
            "badhub bittet um Geduld"
        );
        assert_eq!(
            grund_kurz(&antwort(UploadAntwort::Status { code: 502 })),
            "badhub-Fehler 502"
        );
        assert_eq!(uhrzeit_aus_ms(0).len(), 5);
    }

    #[test]
    fn zustand_serialisiert_nur_die_dashboard_felder() {
        let z = TaktZustand {
            naechster_ms: Some(5),
            merker: Some(SendeMerker {
                pfad: pf(),
                guid: "G".into(),
                sha: sha(1),
            }),
            nachversuche: 2,
            ..Default::default()
        };
        let json = serde_json::to_string(&z).unwrap();
        assert!(json.contains("naechster_ms") && json.contains("angehalten"));
        assert!(
            !json.contains("merker") && !json.contains("nachversuche") && !json.contains("x.tp")
        );
    }
}
