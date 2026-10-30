import { useEffect, useRef } from "react";
import { pendingAnnounceJobs } from "../api";
import { jobZuAnsage } from "../io/announceJobs";
import { playPrepared } from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
import type { AnnounceConfig, AnnounceJob, AzureTtsConfig } from "../types";

// Seit E2 (Spec `ansage-im-browser`) kommt auch die Erst-Ansage „neues Spiel
// auf dem Feld" als Auftrag — derselbe Takt wie zuvor der MatchAnnouncer,
// damit der Aufruf nicht später erklingt als bisher.
const POLL_MS = 2000;

// Abhol-Stand **modulweit**, nicht im Bauteil: Die App hängt den Sprecher
// beim Wechsel zwischen Einrichtung und Dashboard neu ein. Begänne er dann
// jedes Mal mit einer Bestandsaufnahme, gingen die Aufrufe dieser Sekunden
// verloren — seit E2 auch die Erst-Ansage eines frisch belegten Felds.
let letzteId = 0;
let hatBaseline = false;
// Halle, für die die Bestandsaufnahme gilt.
let baselineHalle = "";

/**
 * App-weiter Sprecher für Ansage-Aufträge: Aufrufe der
 * Turnierleitungs-Seite und — seit E2 — die Erst-Ansage, die der Kern bei
 * jeder neuen Feldbelegung ablegt (`court_assigned`).
 *
 * Die Seite im Browser spricht nie selbst: Sie steht im Zweifel im Büro,
 * hat keine Verbindung zur Anlage und kennt weder die eingestellte Stimme
 * noch die Namenskorrekturen. Sie beauftragt nur — gesprochen wird hier, mit
 * demselben Code wie bei einem Aufruf aus der Desktop-App. So klingt ein
 * Aufruf gleich, egal wer ihn ausgelöst hat.
 *
 * Erste Runde ist nur Bestandsaufnahme: Was schon vor dem Start dieser
 * Instanz beauftragt wurde, wird nicht nachträglich gerufen. Dasselbe gilt
 * nach dem Abschalten der Ansagen und nach einem Wechsel der Ansage-Halle —
 * sonst holte das Wiedereinschalten oder eine hinzugenommene Halle die
 * Aufrufe der letzten Minute nach (Slave-Befund 23.08.2026, damals im
 * MatchAnnouncer). Der Turnier-PC lässt Aufträge ohnehin nach einer Minute
 * verfallen.
 */
export function AnnounceJobPlayer({
  announce,
  azureTts,
}: {
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
}) {
  const cfgRef = useRef(announce);
  cfgRef.current = announce;
  const azureRef = useRef(azureTts);
  azureRef.current = azureTts;

  useEffect(() => {
    let alive = true;
    const tick = () => {
      const cfg = cfgRef.current;
      // Sind Ansagen aus, wird hier auch nicht abgefragt: Der Abruf gilt dem
      // Turnier-PC als Lebenszeichen eines Ansage-Geräts. Fragte dieses Gerät
      // trotzdem, meldete die Turnierleitungs-Seite „Aufruf ausgelöst",
      // während in der Halle nichts erklingt — die schlimmste Art von
      // Rückmeldung, weil sie beruhigt.
      if (!cfg.enabled) {
        hatBaseline = false;
        return;
      }
      const halle = (cfg.announce_hall || "").trim();
      if (halle !== baselineHalle) {
        hatBaseline = false;
        baselineHalle = halle;
      }
      pendingAnnounceJobs(letzteId)
        .then(async (jobs) => {
          if (!alive) return;
          if (jobs.length > 0) {
            letzteId = jobs.reduce((m, j) => Math.max(m, j.id), letzteId);
          }
          // Erste Runde: nur merken, nicht sprechen.
          if (!hatBaseline) {
            hatBaseline = true;
            return;
          }
          for (const job of jobs) {
            if (!alive) {
              // Ausgehängt mitten in der Runde (Wechsel Einrichtung ↔
              // Dashboard): Den Rest nicht verlieren — die nächste Instanz
              // holt ab diesem Auftrag wieder ab.
              letzteId = Math.min(letzteId, job.id - 1);
              return;
            }
            await speak(job);
          }
        })
        .catch(() => {});
    };

    /** Einen Auftrag in eine gesprochene Ansage übersetzen — mit derselben
     *  Übersetzung wie der Vertoner der Ansage-Seiten (`jobZuAnsage`). */
    const speak = async (job: AnnounceJob) => {
      const r = await jobZuAnsage(job, cfgRef.current, azureRef.current);
      if (!r) return;
      void playPrepared(r.ansage, azureOption(azureRef.current)?.synthesize);
    };

    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  return null;
}
