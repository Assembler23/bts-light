/** Kurz-, deutsche und englische Bezeichnung einer Disziplin; bei einer
 *  Jugend-Altersklasse („U15") als Jungen-/Mädchen-Disziplin. Unbekannte
 *  Disziplin → leere Texte. */
export function disziplinName(
  key: string | undefined,
  klasse: string | undefined,
): { kurz: string; de: string; en: string };
