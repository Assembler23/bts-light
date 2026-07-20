// Typdeklaration für die Satz-/Matchball-Logik (gamePoint.mjs).

export interface GamePointCourt {
  sets: [number, number][];
  best_of: number;
  target_score: number;
  cap_score: number;
  match_id: number;
}

export declare function setDecided(
  a: number,
  b: number,
  target: number,
  cap: number,
): boolean;

export declare function setsToWin(bestOf: number): number;

export declare function gamePointKind(
  c: GamePointCourt,
): "match" | "set" | null;
