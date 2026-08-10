// Testet die Doppel-Gruppierung der Check-In-Spielerliste
// (src/io/checkinPairs.mjs). Zwei Spieler derselben Meldung (entry_id)
// werden zu EINER Zeile zusammengefasst; alles andere bleibt einzeln.
import assert from "node:assert/strict";
import { pairEntries } from "../src/io/checkinPairs.mjs";

const p = (player_id, entry_id) => ({ player_id, entry_id });

// Ein Doppel (entry 7) zwischen zwei Einzel-Meldungen: Das Paar rückt an
// die Position seines ERSTEN Partners, der zweite wird dorthin gezogen.
assert.deepEqual(
  pairEntries([p(1, 5), p(2, 7), p(3, 6), p(4, 7)]),
  [[p(1, 5)], [p(2, 7), p(4, 7)], [p(3, 6)]],
);

// Altes badhub ohne entry_id (überall 0): NIEMAND wird gruppiert — sonst
// klumpte die ganze Klasse zu einer einzigen Zeile zusammen.
assert.deepEqual(
  pairEntries([p(1, 0), p(2, 0), p(3, 0)]),
  [[p(1, 0)], [p(2, 0)], [p(3, 0)]],
);

// Unvollständiges Doppel (nur ein Träger der entry_id): Einzelzeile.
assert.deepEqual(pairEntries([p(1, 9)]), [[p(1, 9)]]);

// Datenfehler: DREI Spieler mit derselben entry_id — keine Gruppierung,
// lieber drei ehrliche Einzelzeilen als ein erfundenes „Doppel zu dritt".
assert.deepEqual(
  pairEntries([p(1, 4), p(2, 4), p(3, 4)]),
  [[p(1, 4)], [p(2, 4)], [p(3, 4)]],
);

// Reihenfolge: Paare in der Reihenfolge ihres ersten Partners, Einzelne
// unverändert dazwischen.
assert.deepEqual(
  pairEntries([p(1, 2), p(2, 3), p(3, 2), p(4, 1), p(5, 3)]),
  [[p(1, 2), p(3, 2)], [p(2, 3), p(5, 3)], [p(4, 1)]],
);

// Leere und kaputte Eingaben fallen weich.
assert.deepEqual(pairEntries([]), []);
assert.deepEqual(pairEntries(undefined), []);

console.log("test-checkin-pairs: alle Fälle grün");
