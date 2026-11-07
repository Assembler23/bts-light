// Typen zu gerufeneReihe.mjs (Spec meetingpoint-reihenfolge).
export interface RufZeile {
  match_id: number;
  prep_call?: { hall?: string | null } | null;
}
export function rufReihe(liste: RufZeile[], matchId: number): number[];
export function rufZielZug(
  liste: RufZeile[],
  matchId: number,
  beforeId: number | null,
): number | null | undefined;
export function rufZielKnopf(
  liste: RufZeile[],
  matchId: number,
  art: "top" | "up" | "down" | "end",
): number | null | undefined;
