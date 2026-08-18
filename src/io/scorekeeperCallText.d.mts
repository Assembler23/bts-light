/** Segmente des Zähltafelbediener-Nachrufs (Feld, ggf. Stufenwort, dann die
 *  Aufforderung mit den Namen). Leeres Ergebnis, wenn Feld oder Namen
 *  fehlen. */
export function scorekeeperCallSegments(
  courtPhrase: string,
  names: string[],
  stage: 1 | 2 | 3,
  lang: "de" | "en",
): string[];
