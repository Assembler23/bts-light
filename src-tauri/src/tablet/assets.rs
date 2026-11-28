//! In die Binärdatei eingebettete statische Assets des Tablet-Servers.
//!
//! Die Geräte-Seiten sind **austauschbar** (Spec `hotfix-ohne-neustart`,
//! ADR 0099): Ausgeliefert wird der *aktive* [`Seitenstand`], der beim Start
//! dem eingebauten entspricht und zur Laufzeit durch ein geprüftes
//! Seiten-Paket ersetzt werden kann. Es gibt bewusst keine `pub const` je
//! Seite mehr — jeder Aufrufer muss über [`aktiv`] gehen.

use std::borrow::Cow;
#[cfg(not(test))]
use std::sync::RwLock;
use std::sync::{Arc, OnceLock};

use axum::body::Bytes;
use include_dir::{include_dir, Dir};

/// Deklariert die tauschbaren Geräte-Seiten an **einer** Stelle: Variante,
/// Dateiname unter `src-tauri/assets/` und Pflicht-Platzhalter. Daraus
/// entstehen `enum Seite`, die Liste `ALLE` und der eingebaute Inhalt — eine
/// neue Seite kann so nicht in nur einer der Listen landen.
macro_rules! seiten {
    ($( $(#[$doku:meta])* $variante:ident => $datei:literal, [$($platzhalter:literal),*]; )*) => {
        /// Eine Geräte-Seite, die der eingebettete Server ausliefert (Spec
        /// `hotfix-ohne-neustart`, ADR 0099).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Seite {
            $( $(#[$doku])* $variante, )*
        }

        impl Seite {
            /// Alle tauschbaren Seiten, in der Reihenfolge der Deklaration.
            pub const ALLE: &'static [Seite] = &[ $( Seite::$variante, )* ];

            /// Dateiname unter `src-tauri/assets/`. Dient nur dem Vergleich
            /// (Paket-Inhalt gegen diese Liste), **nie** als Pfad.
            pub const fn datei(self) -> &'static str {
                match self { $( Seite::$variante => $datei, )* }
            }

            /// Platzhalter, die der Server beim Ausliefern ersetzt. Fehlt
            /// einer in einer nachgeladenen Seite, wäre sie kaputt (z. B.
            /// ohne Marke kein Versionsabgleich) — ein Paket ohne sie wird
            /// abgelehnt.
            pub const fn pflicht_platzhalter(self) -> &'static [&'static str] {
                match self { $( Seite::$variante => &[ $($platzhalter),* ], )* }
            }

            /// Der Inhalt, wie er in dieser Binärdatei steckt.
            const fn eingebaut(self) -> &'static str {
                match self {
                    $( Seite::$variante => include_str!(concat!("../../assets/", $datei)), )*
                }
            }
        }
    };
}

seiten! {
    /// Tablet-Spielzettel (`/court/{id}`, Demofeld `/court/demo`). Über ihren
    /// **unersetzten** Text wird die Marke des Versionsabgleichs gebildet.
    Tablet => "tablet.html",
        ["__COURT_ID__", "__COURT_LABEL__", "__DEMO__", "__TABLET_PIN__", "__SEITEN_MARKE__"];
    /// Turnierleitungs-Oberfläche (`/tl`). Ohne Platzhalter: Die Seite holt
    /// ihren Zugang aus dem Adress-Fragment und leitet die Adresse ihrer
    /// Schnittstelle aus dem eigenen Pfad ab.
    Tl => "tl.html", [];
    /// Court-Monitor (`/court/{id}/display`, `/monitor`).
    Monitor => "monitor.html", ["__MODE__", "__BASE__", "__COURT_LABEL__"];
    /// Zähltafel (`/court/{id}/tafel`, Spec zaehltafel-anzeige-huelle).
    Tafel => "tafel.html", ["__MODE__", "__BASE__", "__COURT_LABEL__"];
    /// Stream-Einblendung für OBS (`/stream`, ADR 0069).
    Stream => "stream.html", ["__BASE__"];
    /// Stream-Werbung für OBS (`/stream/werbung`, ADR 0092).
    StreamWerbung => "stream-werbung.html", ["__BASE__"];
    /// TV-Launcher (`/` und `/tv`): Vollbild-Menü für die Fernbedienung.
    Tv => "tv.html", ["__LIVE_URL__"];
    /// Court-Übersicht (`/info/overview`). Pollt `/health`.
    Overview => "overview.html", [];
    /// Vorbereitungs-Monitor (`/info/preparation`).
    Preparation => "preparation.html", [];
    /// Sieger-Monitor (`/info/winners`).
    Winners => "winners.html", [];
    /// Werbe-Anzeige (`/info/ad`): Vollbild-Werbebild oder Bildschleife.
    Ad => "ad.html", [];
    /// Kombi-Anzeige (`/combo`): mehrere Felder als Bänder.
    Combo => "combo.html", [];
    /// Spielfolge-Anzeige (Spec spielfolge-anzeige).
    Spielfolge => "spielfolge.html", [];
    /// Felder-Lobby (`/felder`): Start-Seite fürs Zähltablett.
    Lobby => "lobby.html", ["__TABLET_PIN__"];
    /// Anzeige-Hülle fürs Tablet (`/anzeige`).
    Anzeige => "anzeige.html", ["__BASE__", "__TABLET_PIN__"];
    /// Ansage-Seite im Browser (`/ansage`, Spec `ansage-im-browser`).
    Ansage => "ansage.html", ["__BASE__"];
    /// Meetingpoint-Seite (`/meetingpoint`, Spec `meetingpoint-spielbereit`).
    Meetingpoint => "meetingpoint.html", ["__BASE__"];
    /// Begegnung eines Punktspiels (`/info/begegnung`, Spec
    /// punktspiel-monitoransicht). Ohne Platzhalter.
    Begegnung => "begegnung.html", [];
}

/// Woher der aktive Seitenstand stammt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quelle {
    /// Die Seiten, wie sie in dieser Binärdatei stecken (Folge 0).
    Eingebaut,
    /// Ein nachgeladenes, geprüftes Seiten-Paket.
    Paket,
}

/// Text einer Seite: entweder fest in der Binärdatei oder nachgeladen. Beide
/// Formen lassen sich **ohne Kopie** ausliefern ([`Seitenstand::koerper`]).
#[derive(Debug)]
enum Text {
    Fest(&'static str),
    Geladen(Arc<str>),
}

impl Text {
    fn als_str(&self) -> &str {
        match self {
            Text::Fest(s) => s,
            Text::Geladen(s) => s,
        }
    }
}

/// Hält einen nachgeladenen Text als Besitzer eines Antwort-Körpers am Leben
/// (`Arc<str>` selbst ist kein `AsRef<[u8]>`).
struct Besitzer(Arc<str>);

impl AsRef<[u8]> for Besitzer {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Ein vollständiger Satz aller Geräte-Seiten samt der Marke des
/// Versionsabgleichs (Spec `tablet-version-abgleich`).
///
/// Seiten und Marke leben **zusammen** in einem Wert: Wer einen Stand in der
/// Hand hält, kann nie eine neue Seite mit alter Marke ausliefern oder
/// umgekehrt. Deshalb holt jeder Abruf **einmal** [`aktiv`] und liest beides
/// daraus.
#[derive(Debug)]
pub struct Seitenstand {
    folge: u32,
    quelle: Quelle,
    /// Je Seite der Text, Index = Position in [`Seite::ALLE`].
    seiten: Vec<Text>,
    marke: String,
}

impl Seitenstand {
    /// Baut einen Stand aus einer Quelle je Seite und berechnet die Marke.
    ///
    /// Die Marke entsteht **einmal** hier und nicht je Lebenszeichen — sonst
    /// würden bei zwanzig Tablets im Sekundentakt jedes Mal die vollen
    /// ~210 KB `tablet.html` gehasht (Review-Fund 24.08.2026). Sie geht über
    /// den **unersetzten** Text: Sonst trüge jedes Feld eine eigene Marke
    /// (der Platzhalter ist Teil des Textes) und keine passte zur anderen.
    pub fn aus_seiten(
        folge: u32,
        quelle: Quelle,
        mut inhalt: impl FnMut(Seite) -> Cow<'static, str>,
    ) -> Self {
        let seiten: Vec<Text> = Seite::ALLE
            .iter()
            .map(|s| match inhalt(*s) {
                Cow::Borrowed(fest) => Text::Fest(fest),
                Cow::Owned(geladen) => Text::Geladen(Arc::from(geladen)),
            })
            .collect();
        let marke = relay_proto::seiten_marke(seiten[Seite::Tablet as usize].als_str());
        Self {
            folge,
            quelle,
            seiten,
            marke,
        }
    }

    /// Der Stand dieser Binärdatei — Folge 0, ohne Kopie der Texte.
    fn eingebaut() -> Self {
        Self::aus_seiten(0, Quelle::Eingebaut, |s| Cow::Borrowed(s.eingebaut()))
    }

    /// Laufende Nummer des Stands; der eingebaute ist 0.
    pub fn folge(&self) -> u32 {
        self.folge
    }

    pub fn quelle(&self) -> Quelle {
        self.quelle
    }

    /// Fingerabdruck der Tablet-Seite dieses Stands.
    pub fn marke(&self) -> &str {
        &self.marke
    }

    /// Der rohe Text einer Seite, Platzhalter unersetzt.
    pub fn seite(&self, seite: Seite) -> &str {
        self.seiten[seite as usize].als_str()
    }

    /// Eine Seite als Antwort-Körper, **ohne den Text zu kopieren** — für
    /// Seiten, die unverändert ausgeliefert werden. Der eingebaute Text ist
    /// statisch (wie vor dem Umbau), ein nachgeladener wird über seinen
    /// Zähler geteilt. Monitore holen ihre Seite selten, aber `tl.html` und
    /// `overview.html` je Abruf zu kopieren wäre Arbeit ohne Nutzen.
    pub fn koerper(&self, seite: Seite) -> Bytes {
        match &self.seiten[seite as usize] {
            Text::Fest(s) => Bytes::from_static(s.as_bytes()),
            Text::Geladen(s) => Bytes::from_owner(Besitzer(s.clone())),
        }
    }

    /// Die Tablet-Seite für ein Feld, mit der Marke **dieses** Stands.
    /// `label_html` muss bereits HTML-maskiert sein.
    pub fn tablet_seite(&self, court_id: i64, label_html: &str, pin: &str) -> String {
        self.seite(Seite::Tablet)
            .replace("__COURT_ID__", &court_id.to_string())
            .replace("__COURT_LABEL__", label_html)
            .replace("__TABLET_PIN__", pin)
            .replace("__SEITEN_MARKE__", &self.marke)
    }

    /// Die Tablet-Seite fürs Demofeld (`/court/demo`), mit der Marke
    /// **dieses** Stands. PIN leer wie beim Relay: Die echte Bedien-PIN
    /// gehört nicht in eine Übungsseite.
    pub fn demo_seite(&self) -> String {
        relay_proto::demo_tablet_seite(self.seite(Seite::Tablet), "", &self.marke)
    }
}

/// Der eingebaute Stand, einmal je Prozess gebaut. Auch das Ziel des
/// Rückwegs, wenn es kein voriges Paket gibt.
pub fn eingebauter_stand() -> Arc<Seitenstand> {
    static EINGEBAUT: OnceLock<Arc<Seitenstand>> = OnceLock::new();
    EINGEBAUT
        .get_or_init(|| Arc::new(Seitenstand::eingebaut()))
        .clone()
}

// Der aktive Stand ist prozessweit (ADR 0099): LAN-Server und Slave-Brücke
// liefern aus derselben Quelle, ohne dass jede freie `render_*`-Funktion
// einen Parameter bräuchte.
#[cfg(not(test))]
static AKTIV: RwLock<Option<Arc<Seitenstand>>> = RwLock::new(None);

// In Tests hat **jeder Test-Thread seinen eigenen** aktiven Stand. `cargo
// test` fährt die Tests eines Crates parallel im selben Prozess; ein Test,
// der den Stand tauscht, würde sonst jedem gleichzeitig laufenden Test die
// Seiten unter den Füßen wegziehen.
//
// Achtung bei künftigen Tests: Das trägt nur, solange Tausch und Abruf auf
// DEMSELBEN Thread laufen. Das gilt für `#[test]` und für `#[tokio::test]`
// in der Standard-Laufzeit (ein Thread). Ein Test mit `flavor =
// "multi_thread"` oder `spawn_blocking` sähe auf den Arbeiter-Threads immer
// den eingebauten Stand — dort den Stand nicht tauschen, sondern die
// Funktion am `Seitenstand` selbst prüfen.
#[cfg(test)]
thread_local! {
    static AKTIV: std::cell::RefCell<Option<Arc<Seitenstand>>> =
        const { std::cell::RefCell::new(None) };
}

/// Schnappschuss des aktiven Seitenstands.
///
/// **Einmal je Abruf holen** und Seite wie Marke daraus lesen — zwei Aufrufe
/// können zwei verschiedene Stände liefern, wenn dazwischen getauscht wird.
pub fn aktiv() -> Arc<Seitenstand> {
    #[cfg(not(test))]
    let stand = AKTIV
        .read()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
        .clone();
    #[cfg(test)]
    let stand = AKTIV.with(|a| a.borrow().clone());
    stand.unwrap_or_else(eingebauter_stand)
}

/// Setzt einen neuen aktiven Stand und gibt den bisherigen zurück.
///
/// Der Wechsel ist ein einziger Zeiger-Tausch: Ein Abruf sieht entweder den
/// alten oder den neuen Stand, nie eine Mischung. Offene Verbindungen sind
/// nicht betroffen; die Tablets bemerken die neue Marke am nächsten Pong.
pub fn tauschen(neu: Arc<Seitenstand>) -> Arc<Seitenstand> {
    #[cfg(not(test))]
    let alt = AKTIV
        .write()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
        .replace(neu);
    #[cfg(test)]
    let alt = AKTIV.with(|a| a.borrow_mut().replace(neu));
    alt.unwrap_or_else(eingebauter_stand)
}

/// Gebündelte SVG-Länderflaggen, je Datei nach IOC-3-Buchstaben-Code
/// (`GER.svg`, `POL.svg`, …) – ausgeliefert unter `/flags/{file}`.
/// Herkunft/Lizenz siehe `NOTICE.md`.
pub static FLAGS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/flags");

/// Liefert den Inhalt einer gebündelten Flaggen-Datei, falls vorhanden.
/// `name` darf nur ein Dateiname sein (keine Pfad-Trennzeichen).
pub fn flag_svg(name: &str) -> Option<&'static [u8]> {
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return None;
    }
    FLAGS.get_file(name).map(|f| f.contents())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Ein Stand, dessen Seiten sich vom eingebauten unterscheiden, die
    /// Platzhalter aber behalten.
    fn anderer_stand(folge: u32) -> Arc<Seitenstand> {
        Arc::new(Seitenstand::aus_seiten(folge, Quelle::Paket, |s| {
            Cow::Owned(format!("<!-- folge {folge} -->{}", s.eingebaut()))
        }))
    }

    /// Jede `*.html` im Assets-Ordner muss in der Liste stehen — sonst bliebe
    /// eine neue Seite still vom Tausch ausgeschlossen (und umgekehrt zeigte
    /// die Liste auf eine Datei, die es nicht mehr gibt).
    #[test]
    fn alle_html_dateien_im_assets_ordner_sind_tauschbar() {
        let ordner = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let auf_platte: BTreeSet<String> = std::fs::read_dir(&ordner)
            .expect("assets-Ordner lesbar")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".html"))
            .collect();
        let in_liste: BTreeSet<String> =
            Seite::ALLE.iter().map(|s| s.datei().to_string()).collect();
        assert_eq!(auf_platte, in_liste);
        assert_eq!(
            in_liste.len(),
            Seite::ALLE.len(),
            "kein Dateiname darf doppelt vorkommen"
        );
    }

    /// Der Index in `seiten` ist die Position in `ALLE` — die Reihenfolge
    /// der Varianten muss dazu passen.
    #[test]
    fn index_einer_seite_ist_ihre_position_in_alle() {
        for (i, s) in Seite::ALLE.iter().enumerate() {
            assert_eq!(*s as usize, i, "{s:?}");
        }
        let stand = eingebauter_stand();
        for s in Seite::ALLE {
            assert_eq!(stand.seite(*s), s.eingebaut(), "{s:?}");
        }
    }

    #[test]
    fn eingebauter_stand_traegt_die_marke_des_tablet_html() {
        let stand = aktiv();
        assert_eq!(stand.folge(), 0);
        assert_eq!(stand.quelle(), Quelle::Eingebaut);
        assert_eq!(
            stand.marke(),
            relay_proto::seiten_marke(include_str!("../../assets/tablet.html")),
            "die Marke geht über das UNERSETZTE tablet.html"
        );
    }

    /// Die Liste der Pflicht-Platzhalter ist exakt die Menge der
    /// `__GROSS__`-Marken in der eingebauten Seite. Ein neuer Platzhalter
    /// ohne Eintrag hier ließe später ein Paket durch, dem er fehlt.
    #[test]
    fn pflicht_platzhalter_stehen_in_jeder_eingebauten_seite() {
        for s in Seite::ALLE {
            let text = s.eingebaut();
            // Jeder zusammenhängende Lauf aus Großbuchstaben, Ziffern und
            // Unterstrichen, der mit `__` beginnt und endet, ist ein
            // Platzhalter — auch `x___FOO__` wird so als `__FOO__` erkannt.
            let gefunden: BTreeSet<String> = text
                .split(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                .filter(|lauf| lauf.len() > 4 && lauf.starts_with("__") && lauf.ends_with("__"))
                .map(|lauf| lauf.trim_matches('_'))
                .filter(|kern| !kern.is_empty())
                .map(|kern| format!("__{kern}__"))
                .collect();
            let erwartet: BTreeSet<String> = s
                .pflicht_platzhalter()
                .iter()
                .map(|p| p.to_string())
                .collect();
            assert_eq!(gefunden, erwartet, "{}", s.datei());
        }
    }

    #[test]
    fn tausch_wechselt_seite_und_marke_gemeinsam() {
        let vorher = aktiv();
        let neu = anderer_stand(3);
        let abgeloest = tauschen(neu.clone());
        assert!(
            Arc::ptr_eq(&abgeloest, &vorher),
            "der alte Stand kommt zurück"
        );

        let jetzt = aktiv();
        assert_eq!(jetzt.folge(), 3);
        assert_eq!(jetzt.quelle(), Quelle::Paket);
        assert_ne!(jetzt.marke(), vorher.marke());
        assert!(jetzt.seite(Seite::Tl).starts_with("<!-- folge 3 -->"));

        // Die ausgelieferte Tablet-Seite trägt die Marke ihres Stands.
        let seite = jetzt.tablet_seite(4, "Feld 4", "");
        assert!(seite.contains(jetzt.marke()));
        assert!(!seite.contains(vorher.marke()));
        assert!(!seite.contains("__SEITEN_MARKE__"));

        // Wer den alten Schnappschuss noch hält, bleibt in sich stimmig.
        let alt = vorher.tablet_seite(4, "Feld 4", "");
        assert!(alt.contains(vorher.marke()));
        assert!(!alt.contains("<!-- folge 3 -->"));

        // Zurück zum vorigen Stand: wieder die alte Marke.
        tauschen(vorher.clone());
        assert_eq!(aktiv().marke(), vorher.marke());
    }

    /// Der Antwort-Körper ist derselbe Text wie `seite()` — fest wie
    /// nachgeladen, und er überlebt den Stand, aus dem er stammt.
    #[test]
    fn koerper_liefert_den_text_der_seite() {
        let fest = eingebauter_stand();
        let geladen = anderer_stand(2);
        for s in Seite::ALLE {
            assert_eq!(&fest.koerper(*s)[..], fest.seite(*s).as_bytes(), "{s:?}");
            assert_eq!(
                &geladen.koerper(*s)[..],
                geladen.seite(*s).as_bytes(),
                "{s:?}"
            );
        }
        let erwartet = geladen.seite(Seite::Tl).to_string();
        let koerper = geladen.koerper(Seite::Tl);
        drop(geladen);
        assert_eq!(&koerper[..], erwartet.as_bytes());
    }

    #[test]
    fn tablet_seite_ersetzt_alle_platzhalter_ausser_demo() {
        let seite = aktiv().tablet_seite(7, "Feld &lt;7&gt;", "1234");
        for p in [
            "__COURT_ID__",
            "__COURT_LABEL__",
            "__TABLET_PIN__",
            "__SEITEN_MARKE__",
        ] {
            assert!(!seite.contains(p), "{p} muss ersetzt sein");
        }
        assert!(seite.contains("Feld &lt;7&gt;"));
        // `__DEMO__` bleibt am echten Feld stehen — daran erkennt die Seite,
        // dass sie NICHT im Übungsmodus läuft.
        assert!(seite.contains("__DEMO__"));
    }

    #[test]
    fn demoseite_nutzt_denselben_schnappschuss() {
        let vorher = aktiv();
        tauschen(anderer_stand(5));
        let jetzt = aktiv();
        let demo = jetzt.demo_seite();
        assert!(demo.starts_with("<!-- folge 5 -->"));
        assert!(demo.contains(jetzt.marke()));
        assert!(!demo.contains(vorher.marke()));
        for p in Seite::Tablet.pflicht_platzhalter() {
            assert!(!demo.contains(p), "{p} muss im Demofeld ersetzt sein");
        }
        tauschen(vorher);
    }
}
