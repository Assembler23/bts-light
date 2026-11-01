// Typdeklaration für den Stream-Link-Baukasten (streamLink.mjs, ADR 0069).

export interface StreamOptionen {
  nation?: boolean;
  verein?: boolean;
  logo?: boolean;
}
export function streamLink(base: string, courtIds: number[], opt?: StreamOptionen): string;
