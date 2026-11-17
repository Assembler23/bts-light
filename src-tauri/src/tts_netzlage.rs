// Ausfall-Merker der Cloud-Sprachdienste (Spec `ansage-offline-stimme`, E1,
// ADR 0105).
//
// Scheitert eine Vertonung an der Leitung (Klasse „Netz"), merkt sich der Kern
// den Dienst für höchstens zehn Minuten. Solange fragt keine Ansage mehr an,
// die nicht vollständig im Cache liegt — die Ersatzstimme spricht sofort,
// statt jedes Mal die harte Frist abzuwarten. Ein Hintergrund-Prüfer baut alle
// 15 s eine Verbindung zum Host des Dienstes auf (ohne Schlüssel, ohne Text,
// also ohne Zeichenverbrauch) und löscht den Merker, sobald der Host antwortet.
//
// Schlüssel ist der **Host** des Dienstes: So sperrt ein Azure-Ausfall nach
// einem Wechsel zu ElevenLabs nicht dessen Anfragen (Kriterium 9). Der Merker
// lebt im `AppState` und nicht in der `SyncEngine`, weil die bei jedem
// Speichern der Einstellungen neu gebaut wird (Kriterium 8).

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Harte Frist um die ganze Vertonung einer Ansage (vom Nutzer entschieden
/// am 17.11.2026 nach der Windows-Messung, ADR 0105).
pub const HARTE_FRIST: Duration = Duration::from_secs(4);

/// Harte Frist für Gemini: Gemini braucht für eine Ansage oft länger als 4 s
/// und fiele sonst auch bei gutem Internet ständig aus (vom Nutzer
/// entschieden am 17.11.2026). Halb so lang wie seine Anfragefrist von 20 s.
pub const GEMINI_FRIST: Duration = Duration::from_secs(10);

/// Abstand der Hintergrundprüfungen (Ausgangswert).
pub const PRUEFTAKT: Duration = Duration::from_secs(15);

/// Ohne gelungene Prüfung läuft der Merker nach dieser Zeit ab (Ausgangswert).
pub const HOECHSTDAUER_MS: u64 = 10 * 60_000;

/// Merker je Host; die Uhr wird gereicht, damit die Tests sie führen können.
#[derive(Debug, Default)]
pub struct Netzlage {
    /// Host → Zeitpunkt des letzten Netz-Fehlers (ms seit Epoche).
    gestoert: BTreeMap<String, u64>,
    /// Läuft gerade ein Prüfer? Verhindert einen zweiten.
    pruefer_laeuft: bool,
}

impl Netzlage {
    /// Ist der Host gesperrt? Abgelaufene Merker zählen nicht; eine Uhr, die
    /// zurückgesprungen ist, ebenfalls nicht (lieber einmal zu viel anfragen
    /// als auf unbestimmte Zeit stumm bleiben).
    pub fn gestoert(&self, host: &str, jetzt_ms: u64) -> bool {
        self.gestoert
            .get(host)
            .is_some_and(|&seit| jetzt_ms >= seit && jetzt_ms - seit < HOECHSTDAUER_MS)
    }

    /// Ein Netz-Fehler: setzt den Merker für die volle Höchstdauer. `true`,
    /// wenn der Aufrufer jetzt einen Prüfer starten muss.
    pub fn netz_fehler(&mut self, host: &str, jetzt_ms: u64) -> bool {
        self.gestoert.insert(host.to_string(), jetzt_ms);
        let starten = !self.pruefer_laeuft;
        self.pruefer_laeuft = true;
        starten
    }

    /// Der Host antwortet wieder: Merker weg.
    pub fn erreichbar(&mut self, host: &str) -> bool {
        self.gestoert.remove(host).is_some()
    }

    /// Räumt abgelaufene Merker und nennt die noch zu prüfenden Hosts. Ist
    /// nichts mehr gesperrt, endet der Prüfer — im selben kritischen
    /// Abschnitt, damit ein gleichzeitiger Netz-Fehler einen neuen startet.
    pub fn zu_pruefen(&mut self, jetzt_ms: u64) -> Vec<String> {
        self.gestoert
            .retain(|_, &mut seit| jetzt_ms >= seit && jetzt_ms - seit < HOECHSTDAUER_MS);
        if self.gestoert.is_empty() {
            self.pruefer_laeuft = false;
        }
        self.gestoert.keys().cloned().collect()
    }
}

/// Hintergrund-Prüfer: läuft nur, solange ein Merker gesetzt ist (K7).
pub async fn pruefer_lauf<U, P, Fut>(lage: Arc<Mutex<Netzlage>>, uhr: U, pruefe: P)
where
    U: Fn() -> u64,
    P: Fn(String) -> Fut,
    Fut: Future<Output = bool>,
{
    loop {
        tokio::time::sleep(PRUEFTAKT).await;
        let hosts = lage
            .lock()
            .expect("netzlage-Mutex nicht vergiftet")
            .zu_pruefen(uhr());
        if hosts.is_empty() {
            tracing::info!("Cloud-Stimme: kein Dienst mehr gesperrt, Prüfung endet");
            return;
        }
        for host in hosts {
            if pruefe(host.clone()).await {
                let geloescht = lage
                    .lock()
                    .expect("netzlage-Mutex nicht vergiftet")
                    .erreichbar(&host);
                if geloescht {
                    tracing::info!(
                        dienst_host = host.as_str(),
                        "Cloud-Stimme: Dienst wieder erreichbar, Sperre aufgehoben"
                    );
                }
            }
        }
    }
}

/// Nur Hostnamen, wie sie aus Konstanten und der Azure-Region entstehen:
/// Buchstaben, Ziffern, Bindestrich, Punkt.
fn host_gueltig(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['.', '-'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Kostenlose Prüfung: ein HEAD auf die Wurzel des Hosts, ohne Schlüssel und
/// ohne Körper. Jede Antwort unter 500 zählt als erreichbar — auch 401/404,
/// denn dann steht die Leitung. 5xx zählt wie bei der Vertonung als Störung
/// (schützt auch vor einem Proxy, der 502 antwortet).
pub async fn host_erreichbar(host: &str) -> bool {
    if !host_gueltig(host) {
        return false;
    }
    erreichbar_unter(&format!("https://{host}/")).await
}

async fn erreichbar_unter(url: &str) -> bool {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(crate::tts_text::netz::VERBINDUNGSFRIST)
            .timeout(HARTE_FRIST)
            // Jede Prüfung baut neu auf: eine wiederverwendete Verbindung
            // sagte nichts über die Leitung von jetzt.
            .pool_max_idle_per_host(0)
            .build()
            .ok()
    });
    let Some(client) = client else {
        return false;
    };
    match client.head(url).send().await {
        Ok(resp) => resp.status().as_u16() < 500,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::badhub::testhilfe::{starten, Antwort};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const A: &str = "westeurope.tts.speech.microsoft.com";
    const B: &str = "api.elevenlabs.io";

    /// K6: Ein Netz-Fehler sperrt für die volle Höchstdauer, danach läuft der
    /// Merker ab; ein erneuter Fehler setzt ihn wieder voll.
    #[test]
    fn merker_laeuft_nach_der_hoechstdauer_ab_und_wird_neu_gesetzt() {
        let mut l = Netzlage::default();
        assert!(!l.gestoert(A, 1_000));
        assert!(l.netz_fehler(A, 1_000));
        assert!(l.gestoert(A, 1_000));
        assert!(l.gestoert(A, 1_000 + HOECHSTDAUER_MS - 1));
        assert!(!l.gestoert(A, 1_000 + HOECHSTDAUER_MS));
        // Erneuter Fehler kurz vor Ablauf: wieder volle Höchstdauer.
        l.netz_fehler(A, 1_000 + HOECHSTDAUER_MS - 10);
        assert!(l.gestoert(A, 1_000 + 2 * HOECHSTDAUER_MS - 20));
    }

    /// Rücksprung der Uhr hebt die Sperre auf statt sie zu verewigen.
    #[test]
    fn zurueckgesprungene_uhr_sperrt_nicht() {
        let mut l = Netzlage::default();
        l.netz_fehler(A, 10_000);
        assert!(!l.gestoert(A, 5_000));
    }

    /// K9: Der Merker gilt je Host.
    #[test]
    fn merker_gilt_je_dienst() {
        let mut l = Netzlage::default();
        l.netz_fehler(A, 1_000);
        assert!(l.gestoert(A, 2_000));
        assert!(!l.gestoert(B, 2_000));
    }

    /// K6: Eine gelungene Prüfung löscht den Merker.
    #[test]
    fn erreichbar_loescht_den_merker() {
        let mut l = Netzlage::default();
        l.netz_fehler(A, 1_000);
        assert!(l.erreichbar(A));
        assert!(!l.gestoert(A, 1_001));
        assert!(!l.erreichbar(A));
    }

    /// Nur ein Prüfer: der zweite Fehler startet keinen weiteren; ist alles
    /// geräumt, startet der nächste Fehler wieder einen.
    #[test]
    fn pruefer_startet_nur_einmal_und_wieder_nach_dem_ende() {
        let mut l = Netzlage::default();
        assert!(l.netz_fehler(A, 1_000));
        assert!(!l.netz_fehler(B, 1_000));
        assert_eq!(l.zu_pruefen(2_000), vec![B.to_string(), A.to_string()]);
        assert!(l.zu_pruefen(1_000 + HOECHSTDAUER_MS).is_empty());
        assert!(l.netz_fehler(A, 1_000 + HOECHSTDAUER_MS));
    }

    /// K6/K7: Der Prüfer fragt im Takt, löscht beim ersten Erfolg und endet,
    /// sobald nichts mehr gesperrt ist.
    #[tokio::test(start_paused = true)]
    async fn pruefer_loescht_nach_erfolg_und_endet() {
        let lage = Arc::new(Mutex::new(Netzlage::default()));
        assert!(lage.lock().unwrap().netz_fehler(A, 0));
        let versuche = Arc::new(AtomicUsize::new(0));
        let v = versuche.clone();
        let start = tokio::time::Instant::now();
        pruefer_lauf(
            lage.clone(),
            || 1_000,
            move |host| {
                let v = v.clone();
                async move {
                    assert_eq!(host, A);
                    v.fetch_add(1, Ordering::SeqCst) >= 2
                }
            },
        )
        .await;
        assert_eq!(versuche.load(Ordering::SeqCst), 3);
        assert!(!lage.lock().unwrap().gestoert(A, 1_000));
        // Drei Prüfungen bis zum Erfolg, ein Takt bis zum Ende.
        assert_eq!(start.elapsed(), PRUEFTAKT * 4);
        assert!(
            lage.lock().unwrap().netz_fehler(A, 2_000),
            "Prüfer muss neu starten können"
        );
    }

    /// Ohne Erfolg endet der Prüfer mit dem Ablauf der Höchstdauer.
    #[tokio::test(start_paused = true)]
    async fn pruefer_endet_mit_der_hoechstdauer() {
        let lage = Arc::new(Mutex::new(Netzlage::default()));
        lage.lock().unwrap().netz_fehler(A, 0);
        let start = tokio::time::Instant::now();
        let uhr_start = start;
        pruefer_lauf(
            lage.clone(),
            move || uhr_start.elapsed().as_millis() as u64,
            |_| async { false },
        )
        .await;
        let ms = start.elapsed().as_millis() as u64;
        assert!(
            (HOECHSTDAUER_MS..HOECHSTDAUER_MS + 20_000).contains(&ms),
            "{ms}"
        );
    }

    #[test]
    fn nur_echte_hostnamen_werden_geprueft() {
        assert!(host_gueltig(A));
        assert!(host_gueltig("api.openai.com"));
        assert!(!host_gueltig(""));
        assert!(!host_gueltig("evil.com/pfad"));
        assert!(!host_gueltig("a@b"));
        assert!(!host_gueltig("host:8080"));
        assert!(!host_gueltig(".a"));
        assert!(!host_gueltig("-a"));
        assert!(!host_gueltig(&"a".repeat(254)));
    }

    /// Antwort unter 500 = erreichbar, ohne Körper (K7: kein Ansagetext, also
    /// keine Zeichen).
    #[tokio::test]
    async fn pruefung_zaehlt_jede_antwort_unter_500_und_sendet_nichts() {
        let mock = starten(vec![Antwort::status("404 Not Found")]).await;
        assert!(erreichbar_unter(&mock.url).await);
        assert_eq!(mock.anzahl(), 1);
        assert!(mock.anfragen()[0].koerper.is_empty());
    }

    #[tokio::test]
    async fn pruefung_wertet_5xx_als_gestoert() {
        let mock = starten(vec![Antwort::status("503 Service Unavailable")]).await;
        assert!(!erreichbar_unter(&mock.url).await);
    }

    #[tokio::test]
    async fn pruefung_ohne_gegenstelle_ist_gestoert() {
        let frei = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = frei.local_addr().unwrap();
        drop(frei);
        assert!(!erreichbar_unter(&format!("http://{addr}/")).await);
    }

    #[tokio::test]
    async fn ungueltiger_host_wird_nicht_angefragt() {
        assert!(!host_erreichbar("127.0.0.1:1/").await);
    }
}
