// Welche Felder bietet der Wunschfeld-Wähler der Turnierleitung an?
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-wunschfeld.mjs` prüft beide auf gleiches Verhalten.

/** Gleicher Hallenname? Ohne Rand-Leerzeichen und Groß-/Kleinschreibung —
 *  so vergleicht auch der Turnier-PC (`hall_allows_match`). */
export function gleicheHalle(a, b) {
  return String(a || "").trim().toLowerCase() === String(b || "").trim().toLowerCase();
}

/**
 * Die Felder, die als Wunschfeld für dieses Spiel zur Wahl stehen.
 *
 * Eingeschränkt wird **nur**, wenn eine Hallen-Regel das Spiel bindet
 * (`hall_source` = `"rule"`): Dann lehnt der Turnier-PC jedes Feld einer
 * anderen Halle ab, und ein Eintrag, der nur eine Absage auslöst, gehört
 * nicht in die Liste. Jede andere Hallenangabe (von Hand, aus BTP, aus dem
 * Aufruf, vorverteilt) ist nur der bisherige Stand: Das Wunschfeld legt die
 * Halle neu fest („die Halle folgt dem Feld", Spec `tl-wunschfeld`) — ein
 * Endspiel muss sich aus der vorverteilten Halle aufs Hauptfeld der anderen
 * holen lassen.
 *
 * Ein Feld ohne Halle bleibt immer wählbar (der Turnier-PC blockt es nicht).
 * Das schon gewählte Feld bleibt ebenfalls stehen: Fiele es heraus, zeigte
 * der Wähler „–", obwohl ein Wunsch gilt.
 */
export function wunschfelder(courts, m) {
  const alle = (courts || []).filter(Boolean);
  if (!m) return alle;
  const halle = String(m.hall || "").trim();
  if (m.hall_source !== "rule" || !halle) return alle;
  return alle.filter((c) =>
    !String(c.location || "").trim()
    || gleicheHalle(c.location, halle)
    || c.court_id === m.wish_court);
}

/**
 * Dieselben Felder nach Halle gebündelt, in der Reihenfolge ihres ersten
 * Auftretens. Feldnamen wiederholen sich über Hallen hinweg („1" gibt es in
 * jeder) — ohne die Halle daneben wäre die Wahl ein Ratespiel. Bei nur einer
 * Gruppe braucht es keine Überschrift.
 */
export function wunschfeldGruppen(felder) {
  const gruppen = [];
  for (const c of felder || []) {
    const halle = String(c.location || "").trim();
    let g = gruppen.find((x) => gleicheHalle(x.halle, halle));
    if (!g) gruppen.push((g = { halle, felder: [] }));
    g.felder.push(c);
  }
  return gruppen;
}
