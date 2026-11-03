export type DienstArt = "azure" | "elevenlabs" | "openai" | "gemini";
/** Wählbare Sprachdienste in Anzeige-Reihenfolge. */
export const DIENSTE: ReadonlyArray<{ wert: string; name: string; art: DienstArt }>;
/** Anbieter zu einem `dienst`-Wert; unbekannt/leer → "azure". */
export function dienstArt(dienst: string | undefined | null): DienstArt;
/** Anzeigename; unbekannt → "Azure". */
export function dienstName(dienst: string | undefined | null): string;
/** Ist der gewählte Dienst vollständig eingerichtet? */
export function cloudStimmeVollstaendig(az: {
  enabled: boolean;
  dienst?: string;
  key?: string;
  region?: string;
  elevenlabs_key?: string;
  openai_key?: string;
  gemini_key?: string;
} | undefined | null): boolean;
