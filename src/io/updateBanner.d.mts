import type { SeitenInfo, UpdateInfo } from "../types";

export type SeitenKnopf = "uebernehmen" | "zurueck" | "schliessen";
export type KernKnopf =
  | "wechseln"
  | "neu_starten"
  | "beim_beenden"
  | "doch_nicht"
  | "verstanden"
  | "schliessen"
  | "kern_zurueck";

export const KNOPF: Record<SeitenKnopf | KernKnopf, string>;

export interface SeitenZeile {
  art: "angebot" | "uebernommen" | "zurueckgenommen";
  text: string;
  zusatz: string[];
  knoepfe: SeitenKnopf[];
}

export interface KernZeile {
  art:
    | "zurueckgefallen"
    | "zurueckgenommen"
    | "gewechselt"
    | "laedt"
    | "wechselt"
    | "bereit";
  text: string;
  zusatz: string[];
  knoepfe: KernKnopf[];
  /** Abweichende Beschriftung einzelner Knöpfe („Zurück zu v…"). */
  namen?: Partial<Record<KernKnopf, string>>;
  /** Knöpfe, die sichtbar, aber aus sind. */
  aus?: KernKnopf[];
}

export function seitenZeile(seiten: SeitenInfo | null | undefined): SeitenZeile | null;

export function kernZeile(info: UpdateInfo | null | undefined): KernZeile | null;

export function wartungKern(info: UpdateInfo | null | undefined): {
  /** Die Vorversion, zu der ein Knopf führt — oder `null`. */
  version: string | null;
  erlaubt: boolean;
  knopf: string | null;
  hinweise: string[];
} | null;

export function unterbrechungsHinweis(info: UpdateInfo): string;

export function wechselHinweis(info: UpdateInfo): string;

export function wartungSeiten(info: UpdateInfo | null | undefined): {
  stand: string;
  hinweise: string[];
  rueckweg: boolean;
} | null;
