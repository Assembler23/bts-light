//! Die Strecke Turnier-PC → badhub: Fehlerklassen, Backoff, Halt, Drossel,
//! Logo-Regel und Log-Flanken. Spec `badhub-push-entkoppeln`, PR 1
//! (ADR 0086). Bewusst **rein** — kein Netz, keine Uhr außer dem
//! übergebenen `jetzt` —, damit jede Regel ohne HTTP prüfbar ist.
//!
//! Warum es das braucht: Bis v0.9.380 lief nach jedem Fehlschlag im
//! 5-s-Takt ein voller `tset` hinaus, samt Logo (bis ~2,7 MB) — auch bei
//! falschem Passwort. badhub sperrt eine IP nach fünf Fehlversuchen für
//! 15 Minuten und nach 50 am Tag für 24 Stunden; die Halle sperrte sich so
//! binnen Sekunden selbst aus.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Die fünf Nachrichtenarten an den Liveticker-Endpunkt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kanal {
    /// Voller Turnierstand.
    Tset,
    /// Ein einzelner Punktestand.
    Tupdate,
    /// Meldeliste für den Hallen-Check-In (`centry_list`).
    Meldeliste,
    /// Spielplan.
    Sched,
    /// Punktverlauf.
    Verlauf,
}

impl Kanal {
    /// Name fürs Log.
    pub fn name(self) -> &'static str {
        match self {
            Kanal::Tset => "tset",
            Kanal::Tupdate => "tupdate_match",
            Kanal::Meldeliste => "centry_list",
            Kanal::Sched => "sched",
            Kanal::Verlauf => "tverlauf",
        }
    }

    /// Der Liveticker selbst (im Gegensatz zu den additiven Kanälen).
    fn liveticker(self) -> bool {
        matches!(self, Kanal::Tset | Kanal::Tupdate)
    }
}

/// Was eine Sendung ergeben hat — alles, was die Klassifikation braucht,
/// und nichts vom Inhalt (keine Körper, keine Namen).
#[derive(Debug, Clone, Default)]
pub struct Ausgang {
    /// HTTP-Status; `None` = es kam keine Antwort (Transport, Frist, DNS, TLS).
    pub status: Option<u16>,
    /// 401 mit `WWW-Authenticate: Basic` — eine vorgelagerte Anmeldewand,
    /// nicht badhub.
    pub wand: bool,
    /// `Retry-After` in Sekunden, falls genannt und lesbar.
    pub retry_after: Option<u64>,
    /// Sah der Antwortkörper nach badhub aus ([`ist_badhub_antwort`])?
    pub echte_antwort: bool,
    /// Größe des gesendeten Körpers.
    pub bytes: usize,
    /// Dauer der Anfrage.
    pub dauer: Duration,
    /// Transportfehler als Text (ohne Körper, ohne Passwort).
    pub fehler: Option<String>,
}

/// Warum die Strecke angehalten ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltGrund {
    /// badhub lehnt das Passwort ab.
    Passwort,
    /// Ein vorgelagerter Server verlangt eine eigene Anmeldung.
    Wand,
}

/// Fehlerklasse einer Sendung. „Strecke" wirkt auf alle Kanäle, „Kanal" nur
/// auf den betroffenen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Klasse {
    Ok,
    /// Strecke: gestört — Backoff, danach ein voller `tset`.
    Stoerung,
    /// Strecke: badhub drosselt — so lange kein Request.
    Gedrosselt(Duration),
    /// Strecke: angehalten bis zum Speichern oder Neustart der Übertragung.
    Halt(HaltGrund),
    /// Kanal `tset`: abgelehnt — wachsende Pause, kein 5-s-Takt.
    Abgelehnt,
    /// `tset` mit Logo war zu groß: einmal sofort ohne Logo.
    OhneLogoSofort,
    /// `tupdate_match`: dieser Punkt wird nicht wiederholt.
    PunktVerwerfen,
    /// Additiver Kanal: 30 Minuten Pause.
    KanalPause,
    /// Additiver Kanal: Backoff wie die Strecke, je Kanal gezählt.
    KanalBackoff,
}

impl Klasse {
    /// Betrifft die Klasse die ganze Strecke?
    pub fn trifft_strecke(self) -> bool {
        matches!(
            self,
            Klasse::Stoerung | Klasse::Gedrosselt(_) | Klasse::Halt(_)
        )
    }
}

/// Pause eines additiven Kanals, den badhub (noch) nicht kennt oder
/// ablehnt. Lang genug, dass ein altes badhub kein Log vollschreibt; kurz
/// genug, dass ein badhub-Deploy während eines mehrtägigen Turniers noch am
/// selben Tag greift.
pub const KANAL_PAUSE: Duration = Duration::from_secs(30 * 60);

/// Nach einem gescheiterten `tset` MIT Logo geht so lange keines mehr mit.
pub const LOGO_SPERRE: Duration = Duration::from_secs(600);

/// Abstand der Erinnerungszeile im Log, solange ein Fehlerzustand anhält.
pub const ERINNERUNG: Duration = Duration::from_secs(600);

const RETRY_AFTER_OHNE_KOPF: u64 = 60;
const RETRY_AFTER_MIN: u64 = 5;
/// badhub nennt bei der Langzeit-Sperre 86400 s. Wörtlich eingehalten
/// stünde der Liveticker einen Tag still, auch nach korrigiertem Passwort.
/// Ein 429 zählt bei badhub nicht als Fehlversuch — ein Versuch je Stunde
/// verlängert die Sperre also nicht. Dieselbe Grenze wie `tp_takt.rs`.
const RETRY_AFTER_MAX: u64 = 60 * 60;

/// Die Tabelle L5 der Spec: HTTP-Ausgang → Klasse, je Kanal.
pub fn klassifiziere(kanal: Kanal, a: &Ausgang, mit_logo: bool) -> Klasse {
    let Some(status) = a.status else {
        return Klasse::Stoerung;
    };
    match status {
        200 if a.echte_antwort => Klasse::Ok,
        // 200 ohne badhub-Antwort: eine Anmeldeseite (Captive Portal).
        200 => Klasse::Stoerung,
        429 => Klasse::Gedrosselt(retry_after_frist(a.retry_after)),
        401 if a.wand => Klasse::Halt(HaltGrund::Wand),
        // Halt nur, wenn die Ablehnung erkennbar von badhub stammt. Ein 401
        // oder 403 eines Filters im Hallen-WLAN hielte sonst den Liveticker
        // mit „Passwort prüfen" an, obwohl badhub nie gefragt wurde — und
        // ein fremder Server kennt badhubs Fehlversuch-Zähler ohnehin nicht.
        401 | 403 if a.echte_antwort => Klasse::Halt(HaltGrund::Passwort),
        401 | 403 => Klasse::Stoerung,
        300..=399 | 408 => Klasse::Stoerung,
        // 5xx am Liveticker ist eine Strecken-Störung: Der nächste Versuch
        // ist dann ein voller `tset`, und der heilt auch einen kaputten
        // Stand bei badhub.
        500..=599 if kanal.liveticker() => Klasse::Stoerung,
        500..=599 => Klasse::KanalBackoff,
        // 404 am Liveticker heißt: falsche Adresse. An den additiven
        // Kanälen heißt es (wie 400): badhub kennt den Kanal noch nicht.
        404 if kanal.liveticker() => Klasse::Stoerung,
        409 | 413 if kanal == Kanal::Verlauf => Klasse::KanalBackoff,
        413 if kanal == Kanal::Tset && mit_logo => Klasse::OhneLogoSofort,
        _ => match kanal {
            Kanal::Tset => Klasse::Abgelehnt,
            Kanal::Tupdate => Klasse::PunktVerwerfen,
            Kanal::Meldeliste | Kanal::Sched | Kanal::Verlauf => Klasse::KanalPause,
        },
    }
}

/// Pause nach dem n-ten Fehlversuch in Folge: 5, 10, 20, 40, 60, 60 … s.
/// Ohne Zufallsanteil — es gibt genau einen Sender.
pub fn backoff(fehlversuche: u32) -> Duration {
    let stufe = fehlversuche.saturating_sub(1).min(4);
    Duration::from_secs((5u64 << stufe).min(60))
}

/// Pause nach der n-ten Ablehnung des `tset` in Folge: 60, 120, 240, 480,
/// dann 600 s.
pub fn abgelehnt_pause(ablehnungen: u32) -> Duration {
    let stufe = ablehnungen.saturating_sub(1).min(4);
    Duration::from_secs((60u64 << stufe).min(600))
}

/// Wie lange nach einem 429 gewartet wird.
pub fn retry_after_frist(sekunden: Option<u64>) -> Duration {
    let s = sekunden
        .unwrap_or(RETRY_AFTER_OHNE_KOPF)
        .clamp(RETRY_AFTER_MIN, RETRY_AFTER_MAX);
    Duration::from_secs(s)
}

/// Liest `Retry-After` — nur die Sekundenform (badhub sendet keine andere).
pub fn retry_after_lesen(kopf: Option<&str>) -> Option<u64> {
    kopf.and_then(|s| s.trim().parse::<u64>().ok())
}

/// Sieht der Antwortkörper nach badhub aus? Jede Antwort des Endpunkts ist
/// ein JSON-Objekt mit einem Text-Feld `type` (`answer`, `ok`, `error`).
/// Bewusst locker: Eine gute Antwort für fremd zu halten wäre schlimmer als
/// ein unerkanntes Portal. Ein Captive Portal antwortet mit HTML — und galt
/// bisher mit seinem 200 als Erfolg.
pub fn ist_badhub_antwort(koerper: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(koerper)
        .ok()
        .and_then(|v| v.get("type").map(|t| t.is_string()))
        .unwrap_or(false)
}

/// Zustand der Strecke als Ganzes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lage {
    Frei,
    /// Gestört: `versuche` Fehlversuche in Folge; der nächste Versuch (ein
    /// voller `tset`) frühestens `naechster`.
    Gestoert {
        versuche: u32,
        naechster: Instant,
    },
    /// badhub drosselt bis `bis`.
    Gedrosselt {
        bis: Instant,
    },
    /// Angehalten — kein Weg heraus außer einer neuen Strecke.
    Angehalten {
        grund: HaltGrund,
    },
}

/// Was die Statuszeile über die Strecke sagen soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anzeige {
    Frei,
    Gestoert { rest: Duration },
    Gedrosselt { rest: Duration },
    Angehalten { grund: HaltGrund },
    Abgelehnt { status: u16, rest: Duration },
}

/// Pause eines Kanals.
#[derive(Debug, Clone, Copy)]
struct Pause {
    /// Wie oft in Folge.
    n: u32,
    bis: Instant,
    /// Letzter HTTP-Status (nur für die Anzeige beim `tset`).
    status: u16,
}

/// Was den Neustart der Übertragung (Speichern der Einstellungen)
/// überlebt, aber nicht den der App. Liegt in `AppState`.
#[derive(Default)]
pub struct PushMerker {
    /// Ziel, zu dem Logo-Stempel und „war gestört" gehören: Adresse,
    /// Passwort-Marke und Turnier.
    ziel: String,
    /// Adresse, zu der die Drossel-Frist gehört — bewusst NUR die Adresse:
    /// An das Passwort gebunden verfiele die Frist gerade beim Speichern
    /// eines korrigierten Passworts, und Speichern wäre ein Weg an der
    /// Drossel vorbei.
    adresse: String,
    /// Wann das Logo zuletzt erfolgreich hinausging, mit welcher Marke.
    logo: Option<(Instant, String)>,
    /// War die Strecke zuletzt gestört? Dann geht der erste `tset` nach dem
    /// Neustart ohne Logo.
    war_gestoert: bool,
    /// Bis wann badhub drosselt.
    gedrosselt_bis: Option<Instant>,
}

/// Von Hand, ohne `ziel`: Die Kennung trägt eine Hash-Marke des Passworts.
/// Sie verlässt den Arbeitsspeicher nie — und soll auch nicht versehentlich
/// über ein `{:?}` im Log landen.
impl std::fmt::Debug for PushMerker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PushMerker")
            .field("logo", &self.logo.as_ref().map(|(t, _)| t))
            .field("war_gestoert", &self.war_gestoert)
            .field("gedrosselt_bis", &self.gedrosselt_bis)
            .finish_non_exhaustive()
    }
}

/// Die Strecke: Lage, Kanal-Pausen, Logo-Regel, Log-Flanken. Geteilt
/// zwischen dem Sync-Zyklus und den Punkt-Arbeitern der Tablets (das „Tor").
#[derive(Debug)]
pub struct Strecke {
    lage: Lage,
    /// Pause, solange badhub den `tset` ablehnt.
    tset_abgelehnt: Option<Pause>,
    kanal_pausen: HashMap<Kanal, Pause>,
    /// Ausgang des unmittelbar vorigen `tset`-Versuchs; `None` = noch keiner
    /// seit App-Start.
    voriger_tset_ok: Option<bool>,
    logo_sperre_bis: Option<Instant>,
    /// Logo-Marke, die badhub als zu groß abgelehnt hat (413).
    logo_zu_gross: Option<String>,
    flanke: Flanke,
    /// Hat badhub in DIESER Übertragung schon einmal „ok" gesagt? Bis dahin
    /// geht höchstens eine Anfrage zugleich hinaus (siehe [`Self::beginnen`]).
    bestaetigt: bool,
    /// Seit wann die eine unbestätigte Anfrage unterwegs ist.
    probe_seit: Option<Instant>,
    /// Wann zuletzt ein verworfener Punkt gemeldet wurde.
    punkt_gemeldet: Option<Instant>,
    merker: Arc<Mutex<PushMerker>>,
}

/// So lange gilt eine unbestätigte Anfrage als unterwegs — länger als die
/// Gesamtfrist einer Anfrage (15 s). Danach gibt die Strecke den Platz von
/// selbst frei: Ein abgebrochener Task darf sie nicht für immer sperren.
const PROBE_FRIST: Duration = Duration::from_secs(20);

/// Das geteilte Tor.
pub type Tor = Arc<Mutex<Strecke>>;

impl Strecke {
    /// Neue Strecke für ein Ziel. Übernimmt aus dem Merker, was zum selben
    /// Ziel gehört; der Halt lebt bewusst nur hier — eine neue Strecke
    /// (Speichern, Neustart der Übertragung) hebt ihn auf.
    pub fn neu(merker: Arc<Mutex<PushMerker>>, adresse: &str, ziel: &str, jetzt: Instant) -> Self {
        let (lage, voriger) = {
            let mut m = merker.lock().unwrap_or_else(|e| e.into_inner());
            if m.adresse != adresse {
                m.adresse = adresse.to_string();
                m.gedrosselt_bis = None;
            }
            if m.ziel != ziel {
                m.ziel = ziel.to_string();
                m.logo = None;
                m.war_gestoert = false;
            }
            let lage = match m.gedrosselt_bis {
                Some(bis) if bis > jetzt => Lage::Gedrosselt { bis },
                _ => Lage::Frei,
            };
            (lage, m.war_gestoert.then_some(false))
        };
        Self {
            lage,
            tset_abgelehnt: None,
            kanal_pausen: HashMap::new(),
            voriger_tset_ok: voriger,
            logo_sperre_bis: None,
            logo_zu_gross: None,
            flanke: Flanke::default(),
            bestaetigt: false,
            probe_seit: None,
            punkt_gemeldet: None,
            merker,
        }
    }

    /// Strecke ohne Vorgeschichte (Tests, Rückfall).
    pub fn frisch() -> Self {
        Self::neu(
            Arc::new(Mutex::new(PushMerker::default())),
            "",
            "",
            Instant::now(),
        )
    }

    /// Als geteiltes Tor.
    pub fn als_tor(self) -> Tor {
        Arc::new(Mutex::new(self))
    }

    pub fn lage(&self) -> Lage {
        self.lage
    }

    /// Darf dieser Kanal jetzt senden?
    ///
    /// - `tset`: wenn die Strecke frei ist — oder gestört bzw. gedrosselt und
    ///   die Frist um (der eine Probe-Versuch ist immer ein voller `tset`).
    /// - alle anderen: nur bei freier Strecke und ohne eigene Pause. Punkte
    ///   warten zusätzlich, solange badhub den `tset` ablehnt — ohne
    ///   bestätigten Stand hängte ein Punkt dort nur ein halbes Spiel an.
    pub fn darf(&self, kanal: Kanal, jetzt: Instant) -> bool {
        let tset_pause = self.tset_abgelehnt.is_some_and(|p| jetzt < p.bis);
        match kanal {
            Kanal::Tset => {
                !tset_pause
                    && match self.lage {
                        Lage::Frei => true,
                        Lage::Gestoert { naechster, .. } => jetzt >= naechster,
                        Lage::Gedrosselt { bis } => jetzt >= bis,
                        Lage::Angehalten { .. } => false,
                    }
            }
            Kanal::Tupdate => self.lage == Lage::Frei && !tset_pause,
            _ => {
                self.lage == Lage::Frei
                    && !self.kanal_pausen.get(&kanal).is_some_and(|p| jetzt < p.bis)
            }
        }
    }

    /// Wie [`Self::darf`], unmittelbar vor dem Senden — und reserviert dabei
    /// den Platz: Solange badhub das Passwort in dieser Übertragung noch
    /// nicht bestätigt hat, geht nur EINE Anfrage zugleich hinaus.
    ///
    /// Sonst prüften die Punkt-Arbeiter mehrerer Felder und der Zyklus alle
    /// zugleich „frei", und ein falsches Passwort ergäbe einen Schwung von
    /// Fehlversuchen, bevor die erste Ablehnung den Halt setzt — badhub
    /// sperrt die IP nach fünf. Nach dem ersten „ok" entfällt die Bremse;
    /// ein Passwort, das mitten im Turnier ungültig wird, bleibt als
    /// Restrisiko (so viele Fehlversuche wie gerade Anfragen unterwegs sind).
    pub fn beginnen(&mut self, kanal: Kanal, jetzt: Instant) -> bool {
        if !self.darf(kanal, jetzt) {
            return false;
        }
        if self.bestaetigt {
            return true;
        }
        let unterwegs = self
            .probe_seit
            .is_some_and(|t| jetzt.saturating_duration_since(t) < PROBE_FRIST);
        if unterwegs {
            return false;
        }
        self.probe_seit = Some(jetzt);
        true
    }

    /// Darf das Logo in diesem `tset` mitreisen? (Regel L7, Teile b–d; ob es
    /// überhaupt nötig ist, entscheidet der Aufrufer.)
    pub fn logo_erlaubt(&self, marke: &str, jetzt: Instant) -> bool {
        self.lage == Lage::Frei
            && self.voriger_tset_ok != Some(false)
            && !self.logo_sperre_bis.is_some_and(|t| jetzt < t)
            && self.logo_zu_gross.as_deref() != Some(marke)
    }

    /// Wann das Logo zuletzt erfolgreich hinausging (überlebt den Neustart
    /// der Übertragung).
    pub fn logo_stempel(&self) -> Option<(Instant, String)> {
        self.merker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .logo
            .clone()
    }

    /// Stempelt das Logo nach einem geglückten `tset`.
    pub fn logo_stempeln(&mut self, marke: String, jetzt: Instant) {
        self.merker.lock().unwrap_or_else(|e| e.into_inner()).logo = Some((jetzt, marke));
    }

    /// Nimmt den Ausgang einer Sendung auf, schaltet die Lage weiter,
    /// schreibt die Log-Flanken und sagt dem Aufrufer die Klasse.
    /// `logo` = Marke des mitgesendeten Logos, falls eines dabei war.
    pub fn melden(
        &mut self,
        kanal: Kanal,
        a: &Ausgang,
        logo: Option<&str>,
        jetzt: Instant,
    ) -> Klasse {
        let klasse = klassifiziere(kanal, a, logo.is_some());
        let status = a.status.unwrap_or(0);
        self.probe_seit = None;
        match klasse {
            Klasse::Ok => {
                self.bestaetigt = true;
                self.kanal_pausen.remove(&kanal);
                if kanal == Kanal::Tset {
                    self.lage = Lage::Frei;
                    self.tset_abgelehnt = None;
                    self.voriger_tset_ok = Some(true);
                    let mut m = self.merker.lock().unwrap_or_else(|e| e.into_inner());
                    m.war_gestoert = false;
                    m.gedrosselt_bis = None;
                }
            }
            Klasse::Stoerung => {
                // Nur der `tset` zählt den Backoff hoch. Fällt das Netz aus,
                // melden die Punkt-Arbeiter aller Felder fast gleichzeitig
                // eine Störung — zwanzig Felder machten sonst aus der ersten
                // Pause von 5 s gleich 60 s. Ein Nebenkanal stößt die Störung
                // nur an (aus „frei") und lässt eine bestehende, wie sie ist.
                match (self.lage, kanal) {
                    (Lage::Gestoert { versuche, .. }, Kanal::Tset) => {
                        self.lage = Lage::Gestoert {
                            versuche: versuche + 1,
                            naechster: jetzt + backoff(versuche + 1),
                        };
                    }
                    (Lage::Gestoert { .. }, _) => {}
                    (Lage::Angehalten { .. }, _) => {}
                    (Lage::Gedrosselt { .. }, k) if k != Kanal::Tset => {}
                    _ => {
                        self.lage = Lage::Gestoert {
                            versuche: 1,
                            naechster: jetzt + backoff(1),
                        };
                    }
                }
                self.tset_gescheitert(kanal, logo, jetzt);
                self.merker
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .war_gestoert = true;
            }
            Klasse::Gedrosselt(frist) => {
                let bis = jetzt + frist;
                self.lage = Lage::Gedrosselt { bis };
                self.tset_gescheitert(kanal, logo, jetzt);
                let mut m = self.merker.lock().unwrap_or_else(|e| e.into_inner());
                m.gedrosselt_bis = Some(bis);
                m.war_gestoert = true;
            }
            Klasse::Halt(grund) => {
                self.lage = Lage::Angehalten { grund };
                self.tset_gescheitert(kanal, logo, jetzt);
            }
            Klasse::Abgelehnt => {
                // badhub hat geantwortet — die Leitung steht also. Eine
                // Störung davor ist damit vorbei; was bleibt, ist die
                // Ablehnung (sonst zeigte die Statuszeile „gestört, nächster
                // Versuch in 0 s").
                if matches!(self.lage, Lage::Gestoert { .. } | Lage::Gedrosselt { .. }) {
                    self.lage = Lage::Frei;
                }
                let n = self.tset_abgelehnt.map(|p| p.n + 1).unwrap_or(1);
                self.tset_abgelehnt = Some(Pause {
                    n,
                    bis: jetzt + abgelehnt_pause(n),
                    status,
                });
                self.tset_gescheitert(kanal, logo, jetzt);
            }
            Klasse::OhneLogoSofort => {
                self.logo_zu_gross = logo.map(str::to_string);
                self.voriger_tset_ok = Some(false);
            }
            Klasse::PunktVerwerfen => {
                // Höchstens eine Zeile je zehn Minuten: Lehnt badhub jeden
                // Punkt ab, wären es sonst mehrere je Sekunde.
                let faellig = self
                    .punkt_gemeldet
                    .is_none_or(|t| jetzt.saturating_duration_since(t) >= ERINNERUNG);
                if faellig {
                    self.punkt_gemeldet = Some(jetzt);
                    tracing::warn!(
                        kanal = kanal.name(),
                        status,
                        "badhub lehnt einen Punktestand ab — er wird nicht wiederholt, der naechste volle Stand traegt ihn"
                    );
                }
            }
            Klasse::KanalPause => {
                let lief_schon = self.kanal_pausen.get(&kanal).is_some_and(|p| jetzt < p.bis);
                self.kanal_pausen.insert(
                    kanal,
                    Pause {
                        n: 1,
                        bis: jetzt + KANAL_PAUSE,
                        status,
                    },
                );
                if !lief_schon {
                    tracing::info!(
                        kanal = kanal.name(),
                        status,
                        "badhub nimmt diesen Kanal nicht an — naechster Versuch in {} Minuten",
                        KANAL_PAUSE.as_secs() / 60
                    );
                }
            }
            Klasse::KanalBackoff => {
                let n = self.kanal_pausen.get(&kanal).map(|p| p.n + 1).unwrap_or(1);
                self.kanal_pausen.insert(
                    kanal,
                    Pause {
                        n,
                        bis: jetzt + backoff(n),
                        status,
                    },
                );
                if n == 1 {
                    tracing::warn!(
                        kanal = kanal.name(),
                        status,
                        "badhub antwortet auf diesem Kanal mit einem Fehler — Wiederholung mit wachsendem Abstand"
                    );
                }
            }
        }
        self.flanke_schreiben(kanal, klasse, a, jetzt);
        klasse
    }

    /// Ein `tset`-Versuch ist gescheitert: kein Logo im nächsten, und nach
    /// einem Versuch MIT Logo eine Logo-Sperre.
    fn tset_gescheitert(&mut self, kanal: Kanal, logo: Option<&str>, jetzt: Instant) {
        if kanal != Kanal::Tset {
            return;
        }
        self.voriger_tset_ok = Some(false);
        if logo.is_some() {
            self.logo_sperre_bis = Some(jetzt + LOGO_SPERRE);
        }
    }

    /// Lässt in Tests Zeit vergehen: Alle Fristen rücken um `d` näher.
    /// (Die Aufrufer im Betrieb nehmen `Instant::now()` — die Uhr lässt
    /// sich dort nicht anhalten.)
    #[cfg(test)]
    pub fn vorspulen(&mut self, d: Duration) {
        let frueher = |t: Instant| t.checked_sub(d).unwrap_or(t);
        self.lage = match self.lage {
            Lage::Gestoert {
                versuche,
                naechster,
            } => Lage::Gestoert {
                versuche,
                naechster: frueher(naechster),
            },
            Lage::Gedrosselt { bis } => Lage::Gedrosselt { bis: frueher(bis) },
            andere => andere,
        };
        if let Some(p) = &mut self.tset_abgelehnt {
            p.bis = frueher(p.bis);
        }
        for p in self.kanal_pausen.values_mut() {
            p.bis = frueher(p.bis);
        }
        self.logo_sperre_bis = self.logo_sperre_bis.map(frueher);
    }

    /// Was die Statuszeile zeigen soll.
    pub fn anzeige(&self, jetzt: Instant) -> Anzeige {
        match self.lage {
            Lage::Angehalten { grund } => Anzeige::Angehalten { grund },
            Lage::Gedrosselt { bis } => Anzeige::Gedrosselt {
                rest: bis.saturating_duration_since(jetzt),
            },
            Lage::Gestoert { naechster, .. } => Anzeige::Gestoert {
                rest: naechster.saturating_duration_since(jetzt),
            },
            Lage::Frei => match self.tset_abgelehnt {
                Some(p) if jetzt < p.bis => Anzeige::Abgelehnt {
                    status: p.status,
                    rest: p.bis.saturating_duration_since(jetzt),
                },
                _ => Anzeige::Frei,
            },
        }
    }

    /// Name des Fehlerzustands fürs Log; `None` = alles gut.
    fn zustand(&self, jetzt: Instant) -> Option<&'static str> {
        match self.anzeige(jetzt) {
            Anzeige::Frei => None,
            Anzeige::Gestoert { .. } => Some("gestoert"),
            Anzeige::Gedrosselt { .. } => Some("gedrosselt"),
            Anzeige::Angehalten {
                grund: HaltGrund::Passwort,
            } => Some("angehalten (Passwort)"),
            Anzeige::Angehalten {
                grund: HaltGrund::Wand,
            } => Some("angehalten (Anmeldewand)"),
            Anzeige::Abgelehnt { .. } => Some("tset abgelehnt"),
        }
    }

    /// Schreibt höchstens eine Zeile je Zustandswechsel, dazu eine
    /// Erinnerung alle zehn Minuten. Nur Kanal, Klasse, Status, Bytes,
    /// Dauer — nie ein Körper, nie ein Name, nie das Passwort.
    fn flanke_schreiben(&mut self, kanal: Kanal, klasse: Klasse, a: &Ausgang, jetzt: Instant) {
        let zustand = self.zustand(jetzt);
        let Some(zeile) = self.flanke.melden(zustand, jetzt) else {
            return;
        };
        let status = a.status.unwrap_or(0);
        let dauer_ms = a.dauer.as_millis() as u64;
        let grund = a.fehler.as_deref().unwrap_or("");
        match zeile {
            FlankenZeile::Beginn | FlankenZeile::Wechsel => {
                tracing::warn!(
                    kanal = kanal.name(),
                    status,
                    bytes = a.bytes,
                    dauer_ms,
                    "badhub-Strecke: {} ({klasse:?}) {grund}",
                    zustand.unwrap_or("frei")
                );
                // Die Anmeldewand sieht aus wie ein falsches Passwort und
                // ist keines — die Erklärung gehört ins Log, sonst sucht man
                // stundenlang am richtigen Token.
                if klasse == Klasse::Halt(HaltGrund::Wand) {
                    tracing::warn!("{}", crate::badhub::push::PushError::BasicAuthWall);
                }
            }
            FlankenZeile::Erinnerung { seit, versuche } => tracing::warn!(
                kanal = kanal.name(),
                status,
                "badhub-Strecke weiter {}: seit {} min, {versuche} Versuche",
                zustand.unwrap_or("frei"),
                seit.as_secs() / 60
            ),
            FlankenZeile::Ende { dauer, versuche } => tracing::info!(
                kanal = kanal.name(),
                bytes = a.bytes,
                dauer_ms,
                "badhub-Strecke wieder frei nach {} s und {versuche} Versuchen",
                dauer.as_secs()
            ),
        }
    }
}

/// Eine Zeile, die die Flanke verlangt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlankenZeile {
    /// Ein Fehlerzustand beginnt.
    Beginn,
    /// Ein Fehlerzustand geht in einen anderen über.
    Wechsel,
    /// Der Zustand hält an (höchstens alle [`ERINNERUNG`]).
    Erinnerung { seit: Duration, versuche: u32 },
    /// Alles wieder gut.
    Ende { dauer: Duration, versuche: u32 },
}

/// Flankengesteuertes Log: je Zustandswechsel genau eine Zeile statt einer
/// je Zyklus. Generisch — die Strecke nutzt es, der Sync-Loop für
/// BTP-Fehler ebenso.
#[derive(Debug, Default)]
pub struct Flanke {
    zustand: Option<String>,
    seit: Option<Instant>,
    versuche: u32,
    letzte_zeile: Option<Instant>,
}

impl Flanke {
    /// Meldet den Zustand nach einem Versuch (`None` = gut). Liefert die
    /// Zeile, die jetzt ins Log gehört — oder `None`.
    pub fn melden(&mut self, zustand: Option<&str>, jetzt: Instant) -> Option<FlankenZeile> {
        match (self.zustand.as_deref(), zustand) {
            (None, None) => None,
            (None, Some(neu)) => {
                self.zustand = Some(neu.to_string());
                self.seit = Some(jetzt);
                self.versuche = 1;
                self.letzte_zeile = Some(jetzt);
                Some(FlankenZeile::Beginn)
            }
            (Some(alt), Some(neu)) if alt == neu => {
                self.versuche += 1;
                let faellig = self
                    .letzte_zeile
                    .is_none_or(|t| jetzt.saturating_duration_since(t) >= ERINNERUNG);
                faellig.then(|| {
                    self.letzte_zeile = Some(jetzt);
                    FlankenZeile::Erinnerung {
                        seit: jetzt.saturating_duration_since(self.seit.unwrap_or(jetzt)),
                        versuche: self.versuche,
                    }
                })
            }
            (Some(_), Some(neu)) => {
                self.zustand = Some(neu.to_string());
                self.versuche += 1;
                self.letzte_zeile = Some(jetzt);
                Some(FlankenZeile::Wechsel)
            }
            (Some(_), None) => {
                let zeile = FlankenZeile::Ende {
                    dauer: jetzt.saturating_duration_since(self.seit.unwrap_or(jetzt)),
                    versuche: self.versuche,
                };
                *self = Self::default();
                Some(zeile)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Antwort von badhub selbst — auch seine Fehler sind JSON mit `type`.
    fn status(code: u16) -> Ausgang {
        Ausgang {
            status: Some(code),
            echte_antwort: true,
            ..Default::default()
        }
    }

    /// Antwort eines fremden Servers (Filter, Proxy, Anmeldeseite).
    fn fremd(code: u16) -> Ausgang {
        Ausgang {
            status: Some(code),
            echte_antwort: false,
            ..Default::default()
        }
    }

    fn transport() -> Ausgang {
        Ausgang {
            fehler: Some("Verbindung abgelehnt".into()),
            ..Default::default()
        }
    }

    const ALLE: [Kanal; 5] = [
        Kanal::Tset,
        Kanal::Tupdate,
        Kanal::Meldeliste,
        Kanal::Sched,
        Kanal::Verlauf,
    ];

    #[test]
    fn klassifikation_je_kanal_folgt_der_tabelle() {
        use Klasse::*;
        // Zeilen der Tabelle L5; Spalten in der Reihenfolge von `ALLE`.
        let html = fremd(200);
        let wand = Ausgang {
            status: Some(401),
            wand: true,
            ..Default::default()
        };
        let gedrosselt = Ausgang {
            retry_after: Some(900),
            ..status(429)
        };
        let d = Gedrosselt(Duration::from_secs(900));
        let pw = Halt(HaltGrund::Passwort);
        let w = Halt(HaltGrund::Wand);
        let tabelle: Vec<(&str, Ausgang, [Klasse; 5])> = vec![
            ("200 echt", status(200), [Ok; 5]),
            ("200 HTML", html, [Stoerung; 5]),
            ("Transport", transport(), [Stoerung; 5]),
            ("302", status(302), [Stoerung; 5]),
            ("408", status(408), [Stoerung; 5]),
            (
                "500",
                status(500),
                [Stoerung, Stoerung, KanalBackoff, KanalBackoff, KanalBackoff],
            ),
            (
                "503",
                status(503),
                [Stoerung, Stoerung, KanalBackoff, KanalBackoff, KanalBackoff],
            ),
            ("429", gedrosselt, [d; 5]),
            ("401 Wand", wand, [w; 5]),
            ("401", status(401), [pw; 5]),
            ("403", status(403), [pw; 5]),
            // Nicht von badhub (Filter im Hallen-WLAN): kein Halt.
            ("401 fremd", fremd(401), [Stoerung; 5]),
            ("403 fremd", fremd(403), [Stoerung; 5]),
            (
                "400",
                status(400),
                [
                    Abgelehnt,
                    PunktVerwerfen,
                    KanalPause,
                    KanalPause,
                    KanalPause,
                ],
            ),
            (
                "404",
                status(404),
                [Stoerung, Stoerung, KanalPause, KanalPause, KanalPause],
            ),
            (
                "409",
                status(409),
                [
                    Abgelehnt,
                    PunktVerwerfen,
                    KanalPause,
                    KanalPause,
                    KanalBackoff,
                ],
            ),
            (
                "413",
                status(413),
                [
                    Abgelehnt,
                    PunktVerwerfen,
                    KanalPause,
                    KanalPause,
                    KanalBackoff,
                ],
            ),
            (
                "422",
                status(422),
                [
                    Abgelehnt,
                    PunktVerwerfen,
                    KanalPause,
                    KanalPause,
                    KanalPause,
                ],
            ),
        ];
        for (name, ausgang, erwartet) in tabelle {
            for (kanal, soll) in ALLE.iter().zip(erwartet) {
                assert_eq!(
                    klassifiziere(*kanal, &ausgang, false),
                    soll,
                    "{name} auf {kanal:?}"
                );
            }
        }
        // 413 mit Logo: einmal sofort ohne Logo — nur beim `tset`.
        assert_eq!(
            klassifiziere(Kanal::Tset, &status(413), true),
            OhneLogoSofort
        );
    }

    #[test]
    fn basic_wand_ist_halt_aber_kein_passwortfehler() {
        let wand = Ausgang {
            status: Some(401),
            wand: true,
            ..Default::default()
        };
        assert_eq!(
            klassifiziere(Kanal::Tset, &wand, false),
            Klasse::Halt(HaltGrund::Wand)
        );
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        s.melden(Kanal::Tset, &wand, None, jetzt);
        assert_eq!(
            s.anzeige(jetzt),
            Anzeige::Angehalten {
                grund: HaltGrund::Wand
            }
        );
    }

    #[test]
    fn backoff_5_10_20_40_60_und_bleibt_bei_60() {
        let folge: Vec<u64> = (1..=7).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(folge, vec![5, 10, 20, 40, 60, 60, 60]);
        let abgelehnt: Vec<u64> = (1..=6).map(|n| abgelehnt_pause(n).as_secs()).collect();
        assert_eq!(abgelehnt, vec![60, 120, 240, 480, 600, 600]);

        // Die Strecke hält die Stufen ein: genau ein Versuch je Stufe.
        let mut s = Strecke::frisch();
        let mut jetzt = Instant::now();
        for sekunden in [5u64, 10, 20, 40, 60, 60] {
            assert!(s.darf(Kanal::Tset, jetzt));
            s.melden(Kanal::Tset, &transport(), None, jetzt);
            assert!(!s.darf(
                Kanal::Tset,
                jetzt + Duration::from_secs(sekunden) - Duration::from_millis(1)
            ));
            jetzt += Duration::from_secs(sekunden);
        }
    }

    #[test]
    fn erfolg_setzt_den_backoff_zurueck() {
        let mut s = Strecke::frisch();
        let mut jetzt = Instant::now();
        for _ in 0..3 {
            s.melden(Kanal::Tset, &transport(), None, jetzt);
            jetzt += Duration::from_secs(60);
        }
        s.melden(Kanal::Tset, &status(200), None, jetzt);
        assert_eq!(s.lage(), Lage::Frei);
        s.melden(Kanal::Tset, &transport(), None, jetzt);
        assert_eq!(
            s.anzeige(jetzt),
            Anzeige::Gestoert {
                rest: Duration::from_secs(5)
            }
        );
    }

    #[test]
    fn gestoerte_strecke_laesst_nur_den_tset_probieren() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        s.melden(Kanal::Tset, &transport(), None, jetzt);
        let spaeter = jetzt + Duration::from_secs(5);
        assert!(s.darf(Kanal::Tset, spaeter));
        for kanal in [
            Kanal::Tupdate,
            Kanal::Meldeliste,
            Kanal::Sched,
            Kanal::Verlauf,
        ] {
            assert!(!s.darf(kanal, spaeter), "{kanal:?}");
        }
    }

    #[test]
    fn halt_laesst_nichts_mehr_durch() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        s.melden(Kanal::Tupdate, &status(401), None, jetzt);
        let viel_spaeter = jetzt + Duration::from_secs(24 * 3600);
        for kanal in ALLE {
            assert!(!s.darf(kanal, viel_spaeter), "{kanal:?}");
        }
        // Auch ein später Erfolg auf einem Nebenkanal hebt den Halt nicht auf.
        s.melden(Kanal::Sched, &status(200), None, viel_spaeter);
        assert!(!s.darf(Kanal::Tset, viel_spaeter));
    }

    #[test]
    fn gedrosselt_haelt_retry_after_ein() {
        let merker = Arc::new(Mutex::new(PushMerker::default()));
        let jetzt = Instant::now();
        let mut s = Strecke::neu(merker.clone(), "https://x/api", "ziel-a", jetzt);
        let a = Ausgang {
            status: Some(429),
            retry_after: Some(900),
            ..Default::default()
        };
        s.melden(Kanal::Tset, &a, None, jetzt);
        assert!(!s.darf(Kanal::Tset, jetzt + Duration::from_secs(899)));
        assert!(s.darf(Kanal::Tset, jetzt + Duration::from_secs(900)));

        // Die Frist überlebt das Speichern — auch mit neuem Passwort (anderes
        // Ziel, gleiche Adresse) …
        let neu = Strecke::neu(merker.clone(), "https://x/api", "ziel-b", jetzt);
        assert!(!neu.darf(Kanal::Tset, jetzt + Duration::from_secs(899)));
        // … aber nicht den Wechsel der Adresse.
        let anders = Strecke::neu(merker, "https://y/api", "ziel-b", jetzt);
        assert!(anders.darf(Kanal::Tset, jetzt));
    }

    #[test]
    fn retry_after_sekunden_untergrenze_obergrenze_und_unlesbar() {
        assert_eq!(retry_after_frist(Some(900)), Duration::from_secs(900));
        assert_eq!(retry_after_frist(Some(1)), Duration::from_secs(5));
        assert_eq!(retry_after_frist(Some(86400)), Duration::from_secs(3600));
        assert_eq!(retry_after_frist(None), Duration::from_secs(60));
        assert_eq!(retry_after_lesen(Some(" 900 ")), Some(900));
        assert_eq!(
            retry_after_lesen(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after_lesen(None), None);
    }

    #[test]
    fn html_mit_200_ist_keine_badhub_antwort() {
        assert!(!ist_badhub_antwort(
            b"<html><body>Bitte anmelden</body></html>"
        ));
        assert!(!ist_badhub_antwort(b""));
        assert!(!ist_badhub_antwort(b"[1,2]"));
        assert!(!ist_badhub_antwort(br#"{"status":"ok"}"#));
        assert!(!ist_badhub_antwort(br#"{"type":7}"#));
    }

    #[test]
    fn antwort_answer_und_ok_gelten_beide() {
        assert!(ist_badhub_antwort(br#"{"type":"answer","status":"ok"}"#));
        assert!(ist_badhub_antwort(
            br#"{"type":"ok","rows":3,"verworfen":0}"#
        ));
        assert!(ist_badhub_antwort(br#"{"type":"error","message":"x"}"#));
    }

    #[test]
    fn logo_nur_beim_ersten_versuch_oder_nach_einem_erfolg() {
        let mut s = Strecke::frisch();
        let mut jetzt = Instant::now();
        // Allererster Versuch: darf.
        assert!(s.logo_erlaubt("m1", jetzt));
        s.melden(Kanal::Tset, &transport(), None, jetzt);
        // Im Fehlerfall: nie — auch nicht beim Probe-Versuch.
        jetzt += Duration::from_secs(5);
        assert!(s.darf(Kanal::Tset, jetzt));
        assert!(!s.logo_erlaubt("m1", jetzt));
        // Der Probe-Versuch glückt (ohne Logo) → der nächste darf wieder.
        s.melden(Kanal::Tset, &status(200), None, jetzt);
        assert!(s.logo_erlaubt("m1", jetzt));
    }

    #[test]
    fn logo_sperre_nach_gescheitertem_logo_tset() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        s.melden(Kanal::Tset, &transport(), Some("m1"), jetzt);
        // Strecke heilt sofort wieder — das Logo bleibt trotzdem 10 min weg.
        let gleich = jetzt + Duration::from_secs(5);
        s.melden(Kanal::Tset, &status(200), None, gleich);
        assert!(!s.logo_erlaubt("m1", gleich));
        assert!(!s.logo_erlaubt("m1", jetzt + LOGO_SPERRE - Duration::from_secs(1)));
        assert!(s.logo_erlaubt("m1", jetzt + LOGO_SPERRE));
    }

    #[test]
    fn logo_nach_413_erst_bei_neuer_marke() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        let klasse = s.melden(Kanal::Tset, &status(413), Some("gross"), jetzt);
        assert_eq!(klasse, Klasse::OhneLogoSofort);
        // Sofortiger zweiter Versuch ohne Logo ist erlaubt …
        assert!(s.darf(Kanal::Tset, jetzt));
        s.melden(Kanal::Tset, &status(200), None, jetzt);
        // … und dieses Logo bleibt draußen, bis es ein anderes ist.
        let viel_spaeter = jetzt + Duration::from_secs(24 * 3600);
        assert!(!s.logo_erlaubt("gross", viel_spaeter));
        assert!(s.logo_erlaubt("kleiner", viel_spaeter));
        // Ohne Logo zu groß: abgelehnt, mit wachsender Pause.
        assert_eq!(
            s.melden(Kanal::Tset, &status(413), None, jetzt),
            Klasse::Abgelehnt
        );
        assert!(!s.darf(Kanal::Tset, jetzt + Duration::from_secs(59)));
        assert!(s.darf(Kanal::Tset, jetzt + Duration::from_secs(60)));
    }

    #[test]
    fn neustart_im_fehler_geht_ohne_logo_und_behaelt_den_stempel() {
        let merker = Arc::new(Mutex::new(PushMerker::default()));
        let jetzt = Instant::now();
        let mut s = Strecke::neu(merker.clone(), "u", "z", jetzt);
        s.melden(Kanal::Tset, &status(200), None, jetzt);
        s.logo_stempeln("m1".into(), jetzt);
        s.melden(Kanal::Tset, &transport(), None, jetzt);

        // Speichern der Einstellungen: neue Strecke, gleiches Ziel.
        let neu = Strecke::neu(merker.clone(), "u", "z", jetzt);
        assert!(neu.darf(Kanal::Tset, jetzt), "der Backoff beginnt neu");
        assert!(!neu.logo_erlaubt("m1", jetzt), "war gestört → ohne Logo");
        assert_eq!(neu.logo_stempel().map(|(_, m)| m), Some("m1".into()));

        // Anderes Ziel: Stempel und Merker gelten nicht mehr.
        let anderes = Strecke::neu(merker, "u", "anderes", jetzt);
        assert!(anderes.logo_stempel().is_none());
        assert!(anderes.logo_erlaubt("m1", jetzt));
    }

    #[test]
    fn abgelehnter_tset_haelt_auch_die_punkte_an() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        s.melden(Kanal::Tset, &status(400), None, jetzt);
        assert_eq!(
            s.anzeige(jetzt),
            Anzeige::Abgelehnt {
                status: 400,
                rest: Duration::from_secs(60)
            }
        );
        assert!(!s.darf(Kanal::Tupdate, jetzt));
        // Die additiven Kanäle hängen nicht daran.
        assert!(s.darf(Kanal::Sched, jetzt));
    }

    #[test]
    fn kanal_pause_trifft_nur_den_kanal() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        assert_eq!(
            s.melden(Kanal::Meldeliste, &status(409), None, jetzt),
            Klasse::KanalPause
        );
        assert!(!s.darf(
            Kanal::Meldeliste,
            jetzt + KANAL_PAUSE - Duration::from_secs(1)
        ));
        assert!(s.darf(Kanal::Meldeliste, jetzt + KANAL_PAUSE));
        assert!(s.darf(Kanal::Tset, jetzt));
        assert!(s.darf(Kanal::Verlauf, jetzt));
        assert_eq!(s.lage(), Lage::Frei);

        // 5xx auf einem additiven Kanal: Backoff je Kanal, Liveticker läuft.
        s.melden(Kanal::Verlauf, &status(500), None, jetzt);
        assert!(!s.darf(Kanal::Verlauf, jetzt + Duration::from_secs(4)));
        assert!(s.darf(Kanal::Verlauf, jetzt + Duration::from_secs(5)));
        assert!(s.darf(Kanal::Tupdate, jetzt));
    }

    #[test]
    fn flanke_eine_zeile_je_wechsel() {
        let mut f = Flanke::default();
        let jetzt = Instant::now();
        assert_eq!(f.melden(None, jetzt), None);
        assert_eq!(f.melden(Some("a"), jetzt), Some(FlankenZeile::Beginn));
        for i in 1..50 {
            assert_eq!(
                f.melden(Some("a"), jetzt + Duration::from_secs(5 * i)),
                None,
                "Zyklus {i}"
            );
        }
        assert_eq!(
            f.melden(Some("b"), jetzt + Duration::from_secs(300)),
            Some(FlankenZeile::Wechsel)
        );
    }

    #[test]
    fn erinnerung_hoechstens_alle_zehn_minuten() {
        let mut f = Flanke::default();
        let jetzt = Instant::now();
        f.melden(Some("a"), jetzt);
        let mut zeilen = 0;
        // Eine Stunde im 5-s-Takt.
        for i in 1..=720u64 {
            if f.melden(Some("a"), jetzt + Duration::from_secs(5 * i))
                .is_some()
            {
                zeilen += 1;
            }
        }
        assert_eq!(zeilen, 6);
    }

    #[test]
    fn rueckkehr_nennt_dauer_und_versuche() {
        let mut f = Flanke::default();
        let jetzt = Instant::now();
        f.melden(Some("a"), jetzt);
        f.melden(Some("a"), jetzt + Duration::from_secs(5));
        f.melden(Some("a"), jetzt + Duration::from_secs(15));
        assert_eq!(
            f.melden(None, jetzt + Duration::from_secs(35)),
            Some(FlankenZeile::Ende {
                dauer: Duration::from_secs(35),
                versuche: 3
            })
        );
        // Danach ist die Flanke wieder leer.
        assert_eq!(f.melden(None, jetzt + Duration::from_secs(40)), None);
    }

    // ── Aus den Reviews von PR 1 ─────────────────────────────────────────

    #[test]
    fn punkt_arbeiter_blaehen_den_backoff_nicht_auf() {
        // Das Netz fällt aus: Die Punkt-Arbeiter von zwanzig Feldern melden
        // fast gleichzeitig eine Störung. Der erste Probe-`tset` kommt
        // trotzdem nach 5 s, nicht nach 60.
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        for _ in 0..20 {
            s.melden(Kanal::Tupdate, &transport(), None, jetzt);
        }
        assert_eq!(
            s.anzeige(jetzt),
            Anzeige::Gestoert {
                rest: Duration::from_secs(5)
            }
        );
        // Erst ein gescheiterter `tset` zählt hoch.
        let spaeter = jetzt + Duration::from_secs(5);
        s.melden(Kanal::Tset, &transport(), None, spaeter);
        s.melden(Kanal::Sched, &transport(), None, spaeter);
        assert_eq!(
            s.anzeige(spaeter),
            Anzeige::Gestoert {
                rest: Duration::from_secs(10)
            }
        );
        // Ein Nebenkanal hebt weder Halt noch Drossel auf.
        let mut halt = Strecke::frisch();
        halt.melden(Kanal::Tset, &status(401), None, jetzt);
        halt.melden(Kanal::Tupdate, &transport(), None, jetzt);
        assert!(matches!(halt.lage(), Lage::Angehalten { .. }));
    }

    #[test]
    fn vor_der_ersten_bestaetigung_geht_nur_eine_anfrage() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        // Zyklus und Punkt-Arbeiter fragen zugleich: nur einer darf.
        assert!(s.beginnen(Kanal::Tset, jetzt));
        assert!(!s.beginnen(Kanal::Tupdate, jetzt));
        assert!(!s.beginnen(Kanal::Tupdate, jetzt));
        // Die Antwort ist eine Ablehnung des Passworts: genau EIN Fehlversuch.
        s.melden(Kanal::Tset, &status(401), None, jetzt);
        assert!(!s.beginnen(Kanal::Tupdate, jetzt));

        // Nach dem ersten „ok" entfällt die Bremse.
        let mut s = Strecke::frisch();
        assert!(s.beginnen(Kanal::Tset, jetzt));
        s.melden(Kanal::Tset, &status(200), None, jetzt);
        assert!(s.beginnen(Kanal::Tupdate, jetzt));
        assert!(s.beginnen(Kanal::Tupdate, jetzt));

        // Ein abgebrochener Task sperrt die Strecke nicht für immer.
        let mut s = Strecke::frisch();
        assert!(s.beginnen(Kanal::Tset, jetzt));
        assert!(!s.beginnen(Kanal::Tset, jetzt + PROBE_FRIST - Duration::from_secs(1)));
        assert!(s.beginnen(Kanal::Tset, jetzt + PROBE_FRIST));
    }

    #[test]
    fn fremdes_401_haelt_die_strecke_nicht_an() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        assert_eq!(
            s.melden(Kanal::Tset, &fremd(403), None, jetzt),
            Klasse::Stoerung
        );
        assert!(s.darf(Kanal::Tset, jetzt + Duration::from_secs(5)));
    }

    #[test]
    fn abgelehnt_nach_stoerung_zeigt_die_ablehnung() {
        let mut s = Strecke::frisch();
        let jetzt = Instant::now();
        s.melden(Kanal::Tset, &transport(), None, jetzt);
        let probe = jetzt + Duration::from_secs(5);
        s.melden(Kanal::Tset, &status(400), None, probe);
        assert_eq!(
            s.anzeige(probe),
            Anzeige::Abgelehnt {
                status: 400,
                rest: Duration::from_secs(60)
            }
        );
        assert_eq!(s.lage(), Lage::Frei);
    }

    #[test]
    fn merker_gibt_das_ziel_nicht_ueber_debug_preis() {
        let merker = Arc::new(Mutex::new(PushMerker::default()));
        let _s = Strecke::neu(
            merker.clone(),
            "https://x/api?key=geheim",
            "ziel|abcdef12",
            Instant::now(),
        );
        let text = format!("{:?}", merker.lock().unwrap());
        assert!(!text.contains("abcdef12"), "{text}");
        assert!(!text.contains("geheim"), "{text}");
    }
}
