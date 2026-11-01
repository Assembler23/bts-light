// Typdeklaration für die Monitor-Stile (monitorStil.mjs, ADR 0068).

export interface NamenPlaetze {
  davor: "flagge" | "logo" | "ohne";
  darunter: "verein" | "nation" | "ohne";
}
export function namenPlaetze(cfg: unknown): NamenPlaetze;
export type Stil = "standard" | "international" | "riesen" | "spieler" | "hell" | "stand";
export const STILE: Stil[];
export function stilAusWert(roh: unknown): Stil;
export function wirksamerStil(modus: string, stateStil: unknown, urlStil: unknown): Stil;
export function zielMitStil<T extends { kind: string }>(ziel: T | null, stil: unknown): T | null;
export function kurzname(name: string, given?: string, family?: string): string;
