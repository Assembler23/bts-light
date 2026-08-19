export declare const MAX_EVENTS: number;
export interface MatchEventLike {
  id: string;
  seq?: number;
  set?: number;
  afterN?: number;
  kind?: string;
  retracts?: string;
}
export declare function sortiere<T extends MatchEventLike>(events: readonly T[]): T[];
export declare function vereinigen<T extends MatchEventLike>(
  vorhanden: readonly T[] | undefined,
  neu: readonly T[] | undefined,
): T[];
export declare function undoSchnitt(
  events: readonly MatchEventLike[] | undefined,
  set: number,
  afterN: number,
): string[];
export declare function istZurueckgenommen(
  events: readonly MatchEventLike[] | undefined,
  id: string,
): boolean;
export declare function ankerNachBuchung(ankerVorher: { set: number; afterN?: number }): {
  set: number;
  afterN: number;
};
export declare function neueKennung(zufall?: (bytes: Uint8Array) => void): string;
