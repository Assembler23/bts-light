// Bildet die Feld-Reihenfolge einer Halle auf Raster-Zellen ab.
// Bildschirm-Koordinaten: row 0 = oben, col 0 = links. Die Start-Ecke
// beschreibt, wo Feld 1 aus Sicht der Turnierleitung liegt.
//
// `vertical` (Default aus) wählt spaltenweise statt reihenweise Zählung:
// Feld 1 an der Start-Ecke, Feld 2 in derselben Spalte eine Zelle weiter
// weg von der Start-Reihe, bis die Spalte voll ist, dann die nächste
// Spalte. Die Spaltenzahl (`columns`) bleibt in beiden Modi die
// Breitenvorgabe des Rasters — nur die Zählrichtung dreht sich.
export function gridPositions(count, { columns, origin, serpentine, vertical = false }) {
  const cols = Math.max(1, columns | 0);
  const rows = Math.max(1, Math.ceil(count / cols));
  const fromBottom = origin === "bottom_left" || origin === "bottom_right";
  const fromRight = origin === "bottom_right" || origin === "top_right";
  const out = [];
  if (vertical) {
    for (let i = 0; i < count; i++) {
      let c = Math.floor(i / rows);
      let r = i % rows;
      // Schlange: jede zweite Nummerierungs-SPALTE läuft rückwärts —
      // das vertikale Gegenstück zur reihenweisen Schlange unten.
      if (serpentine && c % 2 === 1) r = rows - 1 - r;
      if (fromRight) c = cols - 1 - c;
      out.push({ col: c, row: fromBottom ? rows - 1 - r : r });
    }
    return out;
  }
  for (let i = 0; i < count; i++) {
    const r = Math.floor(i / cols);
    let c = i % cols;
    // Schlange: jede zweite Reihe läuft rückwärts — gezählt in
    // Nummerierungs-Reihen, nicht in Bildschirm-Reihen.
    if (serpentine && r % 2 === 1) c = cols - 1 - c;
    if (fromRight) c = cols - 1 - c;
    out.push({ col: c, row: fromBottom ? rows - 1 - r : r });
  }
  return out;
}
