// Typdeklaration für die Hallen-Raster-Zellen-Abbildung (hallGrid.mjs).

export interface GridOptions {
  columns: number;
  origin: "top_left" | "top_right" | "bottom_left" | "bottom_right";
  serpentine: boolean;
}

export interface GridPosition {
  col: number;
  row: number;
}

export declare function gridPositions(
  count: number,
  options: GridOptions,
): GridPosition[];
