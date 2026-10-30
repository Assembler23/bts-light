// Fertige Ansagen an die Ansage-Seiten im Browser geben (Spec
// `ansage-im-browser`). Der Kern entscheidet selbst, ob überhaupt etwas
// entsteht — Schalter „Audio für Ansage-Seiten erzeugen" aus, Slave, oder
// in der Halle hört gerade keine Seite zu: dann nichts, auch kein
// Azure-Aufruf. Deshalb hier kein eigener Schalter-Blick.
import { ansageVertonen } from "../api";
import {
  playPrepared,
  preparePreparationAnnouncement,
  type AnnounceLang,
  type AnnounceOptions,
  type AnnouncePreparationInput,
  type PreparedAnnouncement,
} from "./announcer";
import type { AnsageArt, AnsageAuftrag } from "../types";

export interface AnsageZiel {
  art: AnsageArt;
  /** Halle der Ansage; leer = alle Hallen. */
  hall: string;
  courtId?: number;
  /** Nur bei Ansagen, die einem Spiel auf dem Feld gelten: Die Seite
   *  schweigt, wenn dort inzwischen ein anderes Spiel steht. */
  matchId?: number;
}

/** Baut den Auftrag an den Kern. */
export function ansageAuftrag(
  p: PreparedAnnouncement,
  ziel: AnsageZiel,
): AnsageAuftrag {
  return {
    art: ziel.art,
    hall: ziel.hall,
    courtId: ziel.courtId,
    matchId: ziel.matchId,
    gong: p.gong ?? undefined,
    lang: p.lang,
    rate: p.rate,
    segmente: p.segments,
    ssml: p.ssml,
  };
}

/** Gibt eine Ansage an die Seiten — ohne Warten, ohne Fehler nach außen:
 *  Der Aufruf am PC darf daran nie hängen. */
export function anSeitenGeben(p: PreparedAnnouncement, ziel: AnsageZiel): void {
  ansageVertonen(ansageAuftrag(p, ziel)).catch(() => {});
}

/** Desktop-Knopf „Vorbereitungs-Aufruf"/„-Nachruf": am PC spielen und an die
 *  Ansage-Seiten der Aufruf-Halle geben. */
export function vorbereitungAnsagen(
  input: AnnouncePreparationInput,
  lang: AnnounceLang,
  opts: AnnounceOptions,
): void {
  const p = preparePreparationAnnouncement(input, lang, opts);
  void playPrepared(p, opts.azure?.synthesize);
  anSeitenGeben(p, { art: "desktop", hall: input.hall ?? "" });
}
