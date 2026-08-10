// Testet die Hallen-Raster-Zellen-Abbildung (src/io/hallGrid.mjs). Die
// Funktion gridPositions() bildet eine BTP-Feldreihenfolge auf Bildschirm-
// Koordinaten ab, für die Hallen-Ansicht (Task 10, Turnierleitung-Web).
import assert from "node:assert/strict";
import { gridPositions } from "../src/io/hallGrid.mjs";

// 6 Felder, 3 Spalten, Start unten-links, ohne Schlange:
// Feld 1-3 unten (links→rechts), Feld 4-6 darüber.
assert.deepEqual(gridPositions(6, { columns: 3, origin: "bottom_left", serpentine: false }), [
  { col: 0, row: 1 }, { col: 1, row: 1 }, { col: 2, row: 1 },
  { col: 0, row: 0 }, { col: 1, row: 0 }, { col: 2, row: 0 },
]);

// Schlange: zweite Reihe läuft rückwärts (1-2-3 / 6-5-4 an der Wand).
assert.deepEqual(gridPositions(6, { columns: 3, origin: "bottom_left", serpentine: true }), [
  { col: 0, row: 1 }, { col: 1, row: 1 }, { col: 2, row: 1 },
  { col: 2, row: 0 }, { col: 1, row: 0 }, { col: 0, row: 0 },
]);

// Start unten-rechts: Feld 1 liegt rechts.
assert.deepEqual(gridPositions(4, { columns: 2, origin: "bottom_right", serpentine: false }), [
  { col: 1, row: 1 }, { col: 0, row: 1 },
  { col: 1, row: 0 }, { col: 0, row: 0 },
]);

// Teilreihe: 5 Felder in 3 Spalten — die angebrochene Reihe liegt oben,
// denn gezählt wird von der Start-Ecke aus.
assert.deepEqual(gridPositions(5, { columns: 3, origin: "top_left", serpentine: false }), [
  { col: 0, row: 0 }, { col: 1, row: 0 }, { col: 2, row: 0 },
  { col: 0, row: 1 }, { col: 1, row: 1 },
]);

console.log("hallGrid: alle Fälle bestanden");
