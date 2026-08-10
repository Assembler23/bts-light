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

// ── Vertikale Nummerierung (spaltenweise) ──────────────────────────────
// Dieselben 6 Felder wie oben (3 Spalten, unten-links), aber jetzt
// spaltenweise gezählt: Feld 1 unten-links, Feld 2 darüber, dann die
// nächste Spalte.
assert.deepEqual(
  gridPositions(6, {
    columns: 3,
    origin: "bottom_left",
    serpentine: false,
    vertical: true,
  }),
  [
    { col: 0, row: 1 }, { col: 0, row: 0 },
    { col: 1, row: 1 }, { col: 1, row: 0 },
    { col: 2, row: 1 }, { col: 2, row: 0 },
  ],
);

// Schlange spaltenweise: jede zweite Nummerierungs-SPALTE läuft rückwärts —
// hier Spalte 2 (Index 1, 0-basiert): Feld 3 startet oben statt unten.
assert.deepEqual(
  gridPositions(6, {
    columns: 3,
    origin: "bottom_left",
    serpentine: true,
    vertical: true,
  }),
  [
    { col: 0, row: 1 }, { col: 0, row: 0 },
    { col: 1, row: 0 }, { col: 1, row: 1 },
    { col: 2, row: 1 }, { col: 2, row: 0 },
  ],
);

// Teilspalte: 5 Felder in 2 Spalten (→ 3 Reihen je Spalte). Start oben-links,
// spaltenweise: Spalte 0 wird voll (3 Felder), Spalte 1 (die von der
// Start-Ecke aus entferntere) bekommt nur 2 — die angebrochene Zelle liegt
// am fernen Ende (unten rechts), symmetrisch zur horizontalen Teilreihe
// oben (dort ebenfalls top_left, dort bricht die entferntere REIHE ab).
assert.deepEqual(
  gridPositions(5, {
    columns: 2,
    origin: "top_left",
    serpentine: false,
    vertical: true,
  }),
  [
    { col: 0, row: 0 }, { col: 0, row: 1 }, { col: 0, row: 2 },
    { col: 1, row: 0 }, { col: 1, row: 1 },
  ],
);

// Ohne `vertical` (bzw. `vertical: false`) bleibt das horizontale Verhalten
// unverändert — Default-Kompatibilität für ältere Aufrufer/Konfigurationen.
assert.deepEqual(
  gridPositions(6, { columns: 3, origin: "bottom_left", serpentine: false, vertical: false }),
  gridPositions(6, { columns: 3, origin: "bottom_left", serpentine: false }),
);

console.log("hallGrid: alle Fälle bestanden");
