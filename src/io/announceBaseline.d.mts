// Typdeklaration für die Baseline-Logik der Feld-Ansage (announceBaseline.mjs).

/** Ein Feld, so viel davon, wie der Vergleich braucht. */
export interface BaselineCourt {
  court_id: number;
  match_id: number;
  location?: string;
}

/** Stand zwischen zwei Abrufen: gesehene Match-IDs je Feld + ob die
 *  Baseline schon auf einem brauchbaren (nicht leeren) Stand steht. */
export interface Stand {
  baseline: Map<number, number>;
  hatBaseline: boolean;
}

export declare function diffOccupiedCourts<T extends BaselineCourt>(
  stand: Stand,
  felder: T[],
  halle: string,
): { neue: T[]; stand: Stand };
