// Typdeklaration für die Doppel-Gruppierung der Check-In-Liste
// (checkinPairs.mjs).

/** Gruppiert Spieler zu Anzeige-Zeilen: Doppel-Partner (gleiche
 *  `entry_id`, genau zwei Träger) zusammen, alle anderen einzeln. */
export declare function pairEntries<T extends { entry_id: number }>(
  players: readonly T[] | undefined,
): T[][];
