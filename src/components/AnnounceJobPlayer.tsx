import { useEffect, useRef } from "react";
import {
  pendingAnnounceJobs,
  preparationCandidates,
  tabletOverview,
} from "../api";
import {
  announceCourt,
  announceOfficials,
  announceStartPlay,
} from "../io/announceCourt";
import {
  playPreparationAnnouncement,
  resolveAnnouncementLanguage,
} from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
import type {
  AnnounceConfig,
  AnnounceJob,
  AzureTtsConfig,
  Discipline,
} from "../types";

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

    /** Einen Auftrag in eine gesprochene Ansage übersetzen. */
    const speak = async (job: AnnounceJob) => {
      const cfg = cfgRef.current;
      if (job.kind === "court_call") {
        // Die Paarung steht im aktuellen Feld-Stand — der Auftrag trägt sie
        // bewusst nicht mit sich, sonst wären Namen unterwegs, die hier
        // ohnehin frisch vorliegen.
        const info = await tabletOverview().catch(() => null);
        const court = info?.courts.find((c) => c.court_id === job.courtId);
        // Nichts mehr auf dem Feld oder inzwischen ein anderes Spiel: Der
        // Aufruf wäre falsch, also lieber gar keiner.
        if (!court || court.match_id !== job.matchId) return;
        // Ab dem vierten Aufruf (Option „Aufrufe unbegrenzt") die schlichte
        // Feld-Ansage ohne Stufenwort — „Dritter und letzter Aufruf" noch
        // einmal wäre gelogen, und eine „vierte" Stufe gibt es im Sprachbild
        // der Halle nicht.
        const stage = job.stage >= 4 ? 1 : job.stage >= 3 ? 3 : 2;
        // Auch der Feld-Aufruf trägt seit dem gezielten Nachruf je Partei
        // (Plan tl-liste-vereinfachen E1) eine Partei mit sich — wie der
        // Vorbereitungs-Aufruf darunter. Fehlt sie (Auftrag aus einer
        // älteren Fassung), gilt „beide", das bisherige Verhalten.
        announceCourt(court, cfg, azureRef.current, stage, job.side ?? "both");
        return;
      }

      if (job.kind === "officials") {
        // Nur die Besetzung: Namen stehen im aktuellen Feld-Stand, der
        // Auftrag trägt sie bewusst nicht mit sich.
        const info = await tabletOverview().catch(() => null);
        const court = info?.courts.find((c) => c.court_id === job.courtId);
        if (!court) return;
        announceOfficials(court, cfg, azureRef.current);
        return;
      }

      if (job.kind === "start_play") {
        // „Bitte mit dem Spielen beginnen": Feld und Aufforderung, sonst
        // nichts. Wie beim Feld-Aufruf gegen den aktuellen Stand geprüft —
        // steht inzwischen ein anderes Spiel dort, wäre die Aufforderung an
        // die Falschen gerichtet.
        const info = await tabletOverview().catch(() => null);
        const court = info?.courts.find((c) => c.court_id === job.courtId);
        if (!court || court.match_id !== job.matchId) return;
        announceStartPlay(court, cfg, azureRef.current);
        return;
      }

      if (job.kind !== "prep_call") {
        // Ansageart aus einem neueren Turnier-PC, die dieser Stand nicht
        // kennt: schweigen. Ohne diese Weiche fiele sie unten in den
        // Vorbereitungs-Zweig und die Halle hörte einen FALSCHEN Aufruf —
        // schlimmer als gar keiner. Die Aufträge selbst überstehen das
        // schon (`announce_jobs_aus_json` überspringt Unbekanntes), hier
        // geht es um die zweite Hälfte derselben Absicherung.
        return;
      }

      // Vorbereitungs-Aufruf: Kandidatenliste hat Namen, Disziplin und die
      // Halle des Aufrufs.
      const view = await preparationCandidates().catch(() => null);
      const c = view?.candidates.find((x) => x.match_id === job.matchId);
      if (!c) return;
      const beideParteien = job.side === "both";
      const ersteParteiGemeint = job.side !== "team2";
      const namen = beideParteien
        ? c.team1
        : ersteParteiGemeint
          ? c.team1
          : c.team2;
      const nats = beideParteien
        ? [...c.team1_nationalities, ...c.team2_nationalities]
        : ersteParteiGemeint
          ? c.team1_nationalities
          : c.team2_nationalities;
      const lang = resolveAnnouncementLanguage(nats, cfg.language_mode);
      void playPreparationAnnouncement(
        {
          discipline: (c.discipline || "unknown") as Discipline,
          className: c.class_label,
          teamANames: namen,
          // Bei einer einzelnen Partei bleibt die andere ungenannt — genau
          // wie beim Nachruf aus der Desktop-App.
          teamBNames: beideParteien ? c.team2 : [],
          hall: c.call?.hall || undefined,
          // Die Staffelung 2 → 3 zählt der Turnier-PC: Wer dreimal gerufen
          // wird und dreimal „Zweiter Aufruf" hört, erfährt nie, dass es der
          // letzte vor der kampflosen Wertung war.
          callStage: job.stage >= 3 ? 3 : 2,
        },
        lang,
        {
          rate: cfg.rate,
          voiceURI: lang === "de" ? cfg.voice_de : cfg.voice_en,
          gong: cfg.gong,
          nameOverrides: cfg.name_overrides,
          nameOverridesEnabled: cfg.name_overrides_enabled,
          azure: azureOption(azureRef.current),
        },
      );
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
