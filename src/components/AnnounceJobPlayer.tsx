import { useEffect, useRef } from "react";
import { pendingAnnounceJobs } from "../api";
import { jobZuAnsage } from "../io/announceJobs";
import { playPrepared } from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
import type { AnnounceConfig, AnnounceJob, AzureTtsConfig } from "../types";

const POLL_MS = 3000;

/**
 * App-weiter Sprecher für Ansage-Aufträge der Turnierleitungs-Seite.
 *
 * Die Seite im Browser spricht nie selbst: Sie steht im Zweifel im Büro,
 * hat keine Verbindung zur Anlage und kennt weder die eingestellte Stimme
 * noch die Namenskorrekturen. Sie beauftragt nur — gesprochen wird hier, mit
 * demselben Code wie bei einem Aufruf aus der Desktop-App. So klingt ein
 * Aufruf gleich, egal wer ihn ausgelöst hat.
 *
 * Erste Runde ist nur Bestandsaufnahme: Was schon vor dem Start dieser
 * Instanz beauftragt wurde, wird nicht nachträglich gerufen. Der Turnier-PC
 * lässt Aufträge ohnehin nach einer Minute verfallen — was länger liegt,
 * gehört zu Spielen, die längst laufen.
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
  const lastIdRef = useRef(0);
  const baselineRef = useRef(false);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      // Sind Ansagen aus, wird hier auch nicht abgefragt: Der Abruf gilt dem
      // Turnier-PC als Lebenszeichen eines Ansage-Geräts. Fragte dieses Gerät
      // trotzdem, meldete die Turnierleitungs-Seite „Aufruf ausgelöst",
      // während in der Halle nichts erklingt — die schlimmste Art von
      // Rückmeldung, weil sie beruhigt.
      if (!cfgRef.current.enabled) return;
      pendingAnnounceJobs(lastIdRef.current)
        .then(async (jobs) => {
          if (!alive) return;
          if (jobs.length > 0) {
            lastIdRef.current = jobs.reduce(
              (m, j) => Math.max(m, j.id),
              lastIdRef.current,
            );
          }
          // Erste Runde: nur merken, nicht sprechen.
          if (!baselineRef.current) {
            baselineRef.current = true;
            return;
          }
          if (jobs.length === 0) return;
          for (const job of jobs) {
            if (!alive) return;
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
