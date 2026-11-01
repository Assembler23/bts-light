// Typdeklaration für die Stream-Einblendung (streamAnsicht.mjs, ADR 0069).
export const MAX_FELDER: number;
export const DATEN_FRIST_MS: number;
export function courtsAusQuery(roh: unknown): number[];
export function optionenAusQuery(
  nation: string | null,
  verein: string | null,
  logo: string | null,
): { nation: boolean; verein: boolean; logo: boolean };
export function zaehltAlsDaten(state: unknown): boolean;
export function datenFrisch(letzterKontaktMs: number, jetztMs: number): boolean;
export function feldSichtbar(state: unknown): boolean;
