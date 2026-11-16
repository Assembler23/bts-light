import type { SeitenInfo, UpdateInfo } from "../types";

export type SeitenKnopf = "uebernehmen" | "zurueck" | "schliessen";

export const KNOPF: Record<SeitenKnopf, string>;

export interface SeitenZeile {
  art: "angebot" | "uebernommen" | "zurueckgenommen";
  text: string;
  zusatz: string[];
  knoepfe: SeitenKnopf[];
}

export function seitenZeile(seiten: SeitenInfo | null | undefined): SeitenZeile | null;

export function wartungSeiten(info: UpdateInfo | null | undefined): {
  stand: string;
  hinweise: string[];
  rueckweg: boolean;
} | null;
