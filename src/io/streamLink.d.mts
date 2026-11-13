// Typdeklaration für den Stream-Link-Baukasten (streamLink.mjs, ADR 0069).

export interface StreamOptionen {
  nation?: boolean;
  verein?: boolean;
  logo?: boolean;
}
export function streamLink(base: string, courtIds: number[], opt?: StreamOptionen): string;

/** Optionen der Stream-Werbung (ADR 0092); Unbekanntes fällt auf die Vorgabe. */
export interface StreamWerbungOptionen {
  modus?: "rotation" | "reihe" | "band";
  hintergrund?: "transparent" | "kachel";
  tempo?: "langsam" | "mittel" | "schnell";
}
export function streamWerbungLink(base: string, opt?: StreamWerbungOptionen): string;
