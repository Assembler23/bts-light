// Bildet die Feld-Reihenfolge einer Halle auf Raster-Zellen ab.
// Bildschirm-Koordinaten: row 0 = oben, col 0 = links. Die Start-Ecke
// beschreibt, wo Feld 1 aus Sicht der Turnierleitung liegt.
export function gridPositions(count, { columns, origin, serpentine }) {
  const cols = Math.max(1, columns | 0);
  const rows = Math.max(1, Math.ceil(count / cols));
  const fromBottom = origin === "bottom_left" || origin === "bottom_right";
  const fromRight = origin === "bottom_right" || origin === "top_right";
  const out = [];
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
