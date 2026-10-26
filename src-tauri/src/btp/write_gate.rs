//! Schreib-Gate für den TP-Upload (ADR 0060, Spec `docs/features/tp-datei-upload.md`).
//!
//! Der Upload kopiert die **offene** TP-Datei, während BTP sie hält. Unsere
//! eigenen BTP-Writes (Ergebnis, Felder, Schiedsrichter, Highlight — die vier
//! `write_*_to_btp` in `tablet/server.rs`) kennen wir; Eingaben des
//! Turnierleiters direkt in BTP nicht. Gegen die erste Quelle liefert dieses
//! Gate eine **garantierte Lücke**: kein eigener Write läuft, und seit dem
//! letzten ist die Ruhezeit für den Jet-Nachlauf vergangen. Gegen die zweite
//! stehen der Byte-Vergleich im Client (`lese_stabil`) und die
//! Plausibilitätsprüfung im Server.
//!
//! Mechanik: ein `tokio::sync::RwLock<()>`. Jeder Write hält die **Lese**sperre
//! (Writes blockieren sich nicht gegenseitig), die Kopie nimmt die
//! **Schreib**sperre. Tokios RwLock ist schreiber-bevorzugt: Sobald die Kopie
//! wartet, reihen sich neue Writes hinter ihr ein — genau das erzeugt die
//! Lücke, ohne zu pollen. Die Wartezeiten ehrlich benannt: Ein Write, der
//! während der Kopie eintrifft, wartet die Kopie ab (Ruhe + zweimal Lesen,
//! typisch 5 s, mit Wiederholung bis etwa 9 s). Ein Write, der eintrifft,
//! während die Kopie noch auf einen langen Write wartet, wartet die restliche
//! Wartephase (höchstens `MAX_WARTEZEIT`) plus Kopie. Nichts geht verloren.
//!
//! Prozessweit genau **eine** Instanz (`GATE`), weil die Write-Funktionen nur
//! `&AppConfig` kennen und keinen Zustand gereicht bekommen. Tests bauen eigene
//! Instanzen und laufen mit pausierter Tokio-Zeit.
//!
//! Die Konstanten sind Startwerte bis zur Nachlauf-Messung (Spec, „Offene
//! Fragen“: Datei nach bestätigtem `SENDUPDATE` sekündlich hashen, bis Ruhe).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Ruhe nach dem letzten Write, bevor gelesen wird (Jet-Nachlauf, ungemessen).
pub const RUHE: Duration = Duration::from_secs(3);
/// Längstens so lange wartet die Kopie auf laufende Writes, dann „beschäftigt“.
pub const MAX_WARTEZEIT: Duration = Duration::from_secs(20);

/// Die eine Instanz des Prozesses.
pub static GATE: LazyLock<WriteGate> = LazyLock::new(WriteGate::new);

/// Die Kopie kam nicht an die Reihe: ein Write hielt länger als `max_wartezeit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFehler {
    Beschaeftigt,
}

/// Gate aus Sperre, Zeitstempel des letzten Write-Endes und Zähler der
/// gehaltenen Lesesperren (Testsonde).
pub struct WriteGate {
    lock: RwLock<()>,
    last_end_ms: AtomicU64,
    laufende_writes: AtomicUsize,
}

/// Lesesperre eines laufenden BTP-Writes. Beim Drop wird das Ende gestempelt.
pub struct SchreibGuard<'a> {
    gate: &'a WriteGate,
    _guard: RwLockReadGuard<'a, ()>,
}

impl Drop for SchreibGuard<'_> {
    fn drop(&mut self) {
        self.gate.laufende_writes.fetch_sub(1, Ordering::SeqCst);
        self.gate.last_end_ms.store(jetzt_ms(), Ordering::SeqCst);
    }
}

/// Schreibsperre der Kopie: solange er lebt, beginnt kein neuer Write.
pub struct KopieGuard<'a> {
    _guard: RwLockWriteGuard<'a, ()>,
}

impl Default for WriteGate {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteGate {
    pub fn new() -> Self {
        Self {
            lock: RwLock::new(()),
            last_end_ms: AtomicU64::new(0),
            laufende_writes: AtomicUsize::new(0),
        }
    }

    /// Für die Dauer eines BTP-Writes halten: `let _schreibt = GATE.schreiben().await;`
    /// am Anfang der Write-Funktion. Wartet nur, wenn gerade eine Kopie läuft
    /// oder wartet.
    pub async fn schreiben(&self) -> SchreibGuard<'_> {
        let guard = self.lock.read().await;
        self.laufende_writes.fetch_add(1, Ordering::SeqCst);
        SchreibGuard {
            gate: self,
            _guard: guard,
        }
    }

    /// Lücke für die Kopie: höchstens `max_wartezeit` auf laufende Writes
    /// warten (sonst [`GateFehler::Beschaeftigt`]), dann `ruhe` lang alle neuen
    /// Writes zurückhalten und erst danach den Guard herausgeben. Der Aufrufer
    /// liest unter dem Guard und lässt ihn **vor** dem Senden fallen.
    pub async fn fuer_kopie(
        &self,
        ruhe: Duration,
        max_wartezeit: Duration,
    ) -> Result<KopieGuard<'_>, GateFehler> {
        // `write()` ist abbruchsicher: läuft die Frist ab, verfällt nur unser
        // Platz in der Warteschlange, kein Write bleibt hängen.
        let guard = tokio::time::timeout(max_wartezeit, self.lock.write())
            .await
            .map_err(|_| GateFehler::Beschaeftigt)?;
        tokio::time::sleep(ruhe).await;
        Ok(KopieGuard { _guard: guard })
    }

    /// Testsonde: Zahl der gerade gehaltenen Lesesperren.
    pub fn in_flight(&self) -> usize {
        self.laufende_writes.load(Ordering::SeqCst)
    }

    /// Unix-ms des letzten Write-Endes, 0 = noch keiner.
    pub fn last_end_ms(&self) -> u64 {
        self.last_end_ms.load(Ordering::SeqCst)
    }
}

fn jetzt_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::time::{advance, Instant};

    /// Hält die Lesesperre `dauer` lang in einem eigenen Task und meldet, wann
    /// sie genommen wurde.
    fn write_der_haelt(gate: Arc<WriteGate>, dauer: Duration) -> Arc<AtomicBool> {
        let genommen = Arc::new(AtomicBool::new(false));
        let g = genommen.clone();
        tokio::spawn(async move {
            let _schreibt = gate.schreiben().await;
            g.store(true, Ordering::SeqCst);
            tokio::time::sleep(dauer).await;
        });
        genommen
    }

    #[tokio::test(start_paused = true)]
    async fn kopie_wartet_auf_laufenden_write() {
        let gate = Arc::new(WriteGate::new());
        let genommen = write_der_haelt(gate.clone(), Duration::from_secs(10));
        tokio::task::yield_now().await;
        assert!(genommen.load(Ordering::SeqCst));
        assert_eq!(gate.in_flight(), 1);

        let t0 = Instant::now();
        let guard = gate
            .fuer_kopie(Duration::from_secs(3), Duration::from_secs(20))
            .await
            .expect("Kopie kommt nach dem Write dran");
        // 10 s Write + 3 s Ruhe — die Kopie beginnt nie, solange ein Write läuft.
        assert_eq!(t0.elapsed(), Duration::from_secs(13));
        assert_eq!(gate.in_flight(), 0);
        drop(guard);
    }

    #[tokio::test(start_paused = true)]
    async fn write_in_ruhezeit_startet_erst_nach_kopie_und_geht_nicht_verloren() {
        let gate = Arc::new(WriteGate::new());
        let kopie = gate
            .fuer_kopie(Duration::from_secs(3), Duration::from_secs(20))
            .await
            .unwrap();
        // Ein Write kommt, während die Kopie liest: er muss warten …
        let genommen = write_der_haelt(gate.clone(), Duration::from_millis(100));
        advance(Duration::from_secs(5)).await;
        tokio::task::yield_now().await;
        assert!(!genommen.load(Ordering::SeqCst), "Write darf nicht vor der Kopie laufen");
        // … und läuft, sobald die Kopie fertig ist — nichts verworfen.
        drop(kopie);
        tokio::task::yield_now().await;
        assert!(genommen.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn max_wartezeit_ueberschritten_beschaeftigt() {
        let gate = Arc::new(WriteGate::new());
        let _genommen = write_der_haelt(gate.clone(), Duration::from_secs(25));
        tokio::task::yield_now().await;
        let t0 = Instant::now();
        let ergebnis = gate
            .fuer_kopie(Duration::from_secs(3), Duration::from_secs(20))
            .await
            .err();
        assert_eq!(ergebnis, Some(GateFehler::Beschaeftigt));
        assert_eq!(t0.elapsed(), Duration::from_secs(20));
        // Der Write läuft unbeeindruckt weiter und endet regulär.
        assert_eq!(gate.in_flight(), 1);
        advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn zeitstempel_nach_drop_und_in_flight_zaehlt() {
        let gate = WriteGate::new();
        assert_eq!(gate.last_end_ms(), 0);
        let a = gate.schreiben().await;
        let b = gate.schreiben().await; // Writes blockieren sich nicht
        assert_eq!(gate.in_flight(), 2);
        drop(a);
        assert_eq!(gate.in_flight(), 1);
        assert!(gate.last_end_ms() > 0, "Drop stempelt das Write-Ende");
        drop(b);
        assert_eq!(gate.in_flight(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn kopie_ohne_write_wartet_nur_die_ruhe() {
        let gate = WriteGate::new();
        let t0 = Instant::now();
        let _k = gate
            .fuer_kopie(Duration::from_secs(3), Duration::from_secs(20))
            .await
            .unwrap();
        assert_eq!(t0.elapsed(), Duration::from_secs(3));
    }
}
