// Ein Ansage-Auftrag der Turnierleitung → eine fertige Ansage.
//
// Eine Stelle für zwei Verbraucher: den Lautsprecher am PC
// (`AnnounceJobPlayer`) und den Vertoner der Ansage-Seiten im Browser
// (`AnsageVertoner`, Spec `ansage-im-browser`). So klingt ein TL-Aufruf auf
// der Seite Wort für Wort wie am PC.
//
// Der Auftrag trägt bewusst keine Namen (Struktur statt Text) — sie kommen
// frisch aus dem aktuellen Feld-Stand bzw. der Kandidatenliste. Steht dort
// inzwischen ein anderes Spiel, wäre der Aufruf falsch: dann `null`.
import { preparationCandidates, tabletOverview } from "../api";
import {
  courtAnsage,
  officialsAnsage,
  scorekeeperAnsage,
  startPlayAnsage,
  ansageOptionen,
} from "./announceCourt";
import {
  preparePreparationAnnouncement,
  resolveAnnouncementLanguage,
  type PreparedAnnouncement,
} from "./announcer";
import type { AnsageZiel } from "./ansageTeilen";
import type {
  AnnounceConfig,
  AnnounceJob,
  AzureTtsConfig,
  Discipline,
} from "../types";

export interface JobAnsage {
  ansage: PreparedAnnouncement;
  ziel: AnsageZiel;
}

/** Übersetzt einen Auftrag; `null` = schweigen. */
export async function jobZuAnsage(
  job: AnnounceJob,
  cfg: AnnounceConfig,
  azureTts?: AzureTtsConfig,
): Promise<JobAnsage | null> {
  if (job.kind === "court_call") {
    const info = await tabletOverview().catch(() => null);
    const court = info?.courts.find((c) => c.court_id === job.courtId);
    // Nichts mehr auf dem Feld oder inzwischen ein anderes Spiel: Der
    // Aufruf wäre falsch, also lieber gar keiner.
    if (!court || court.match_id !== job.matchId) return null;
    // Ab dem vierten Aufruf (Option „Aufrufe unbegrenzt") die schlichte
    // Feld-Ansage ohne Stufenwort — „Dritter und letzter Aufruf" noch
    // einmal wäre gelogen, und eine „vierte" Stufe gibt es im Sprachbild
    // der Halle nicht.
    const stage = job.stage >= 4 ? 1 : job.stage >= 3 ? 3 : 2;
    // Fehlt die Partei (Auftrag aus einer älteren Fassung), gilt „beide".
    return {
      ansage: courtAnsage(court, cfg, azureTts, stage, job.side ?? "both"),
      ziel: { art: "court_call", hall: job.hall, courtId: job.courtId, matchId: job.matchId },
    };
  }

  if (job.kind === "officials") {
    // Nur die Besetzung: Namen stehen im aktuellen Feld-Stand.
    const info = await tabletOverview().catch(() => null);
    const court = info?.courts.find((c) => c.court_id === job.courtId);
    if (!court) return null;
    const ansage = officialsAnsage(court, cfg, azureTts);
    return ansage
      ? { ansage, ziel: { art: "officials", hall: job.hall, courtId: job.courtId } }
      : null;
  }

  if (job.kind === "start_play") {
    // „Bitte mit dem Spielen beginnen": wie beim Feld-Aufruf gegen den
    // aktuellen Stand geprüft — sonst ginge die Aufforderung an die Falschen.
    const info = await tabletOverview().catch(() => null);
    const court = info?.courts.find((c) => c.court_id === job.courtId);
    if (!court || court.match_id !== job.matchId) return null;
    return {
      ansage: startPlayAnsage(court, cfg, azureTts),
      ziel: { art: "start_play", hall: job.hall, courtId: job.courtId, matchId: job.matchId },
    };
  }

  if (job.kind === "scorekeeper_call") {
    // Der zugewiesene Bediener steht im aktuellen Feld-Stand; steht dort
    // inzwischen ein anderes Spiel, ist der Nachruf gegenstandslos.
    const info = await tabletOverview().catch(() => null);
    const court = info?.courts.find((c) => c.court_id === job.courtId);
    if (!court || court.match_id !== job.matchId) return null;
    const ansage = scorekeeperAnsage(
      court,
      court.scorekeeper ?? [],
      (job.stage >= 3 ? 3 : job.stage >= 2 ? 2 : 1) as 1 | 2 | 3,
      cfg,
      azureTts,
    );
    return ansage
      ? {
          ansage,
          ziel: { art: "scorekeeper_call", hall: job.hall, courtId: job.courtId, matchId: job.matchId },
        }
      : null;
  }

  if (job.kind !== "prep_call") {
    // Ansageart aus einem neueren Turnier-PC, die dieser Stand nicht kennt:
    // schweigen. Ohne diese Weiche fiele sie unten in den
    // Vorbereitungs-Zweig und die Halle hörte einen FALSCHEN Aufruf —
    // schlimmer als gar keiner.
    return null;
  }

  // Vorbereitungs-Aufruf: Kandidatenliste hat Namen, Disziplin und die
  // Halle des Aufrufs.
  const view = await preparationCandidates().catch(() => null);
  const c = view?.candidates.find((x) => x.match_id === job.matchId);
  if (!c) return null;
  const beideParteien = job.side === "both";
  const ersteParteiGemeint = job.side !== "team2";
  const namen = beideParteien ? c.team1 : ersteParteiGemeint ? c.team1 : c.team2;
  const nats = beideParteien
    ? [...c.team1_nationalities, ...c.team2_nationalities]
    : ersteParteiGemeint
      ? c.team1_nationalities
      : c.team2_nationalities;
  const lang = resolveAnnouncementLanguage(nats, cfg.language_mode);
  const ansage = preparePreparationAnnouncement(
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
    ansageOptionen(cfg, lang, azureTts),
  );
  // Kein `matchId`: Ein Spiel in Vorbereitung steht auf keinem Feld, die
  // Belegungs-Kontrolle der Seite griffe ins Leere.
  return { ansage, ziel: { art: "prep_call", hall: job.hall } };
}
