/** Segmente der Ansage „Bitte mit dem Spielen beginnen" (Feld, dann
 *  Aufforderung). Ohne Paarung und ohne Stufenwort — sie ist kein Aufruf.
 *  Leeres Ergebnis, wenn keine Feld-Nennung vorliegt. */
export function startPlaySegments(
  courtPhrase: string,
  lang: "de" | "en",
): string[];
