import type { TpFileInfo, TpFrageSicht, TpSuche } from "../types";

export type TpWahl =
  | { art: "fehler"; text: string }
  | { art: "frage"; dateiGuid: string; eingetragen: string }
  | { art: "uebernehmen"; bindung: string; kennungNeu: string | null };

export function tpInfoText(info: TpFileInfo): string | null;

export function tpWahl(eingabe: {
  info: TpFileInfo;
  name: string;
  eingetragen: string;
  feldLeer: boolean;
  quelle: "dialog" | "suche";
}): TpWahl;

export function tpSucheText(suche: TpSuche): string | null;

export function tpFrageText(frage: TpFrageSicht, kurz: (guid: string) => string): string;
