// Welche Felder sind seit dem letzten Abruf neu belegt worden? — die
// Entscheidung hinter der automatischen Feld-Ansage (MatchAnnouncer).
//
// Bewusst hier und nicht im React-Bauteil: Es ist reine Datenverarbeitung,
// und sie hatte einen Fehler, den man nur mit einem Test dauerhaft draußen
// hält (siehe scripts/test-announce-baseline.mjs).

/**
 * @typedef {{ court_id: number, match_id: number, location?: string }} Feld
 * @typedef {{ baseline: Map<number, number>, hatBaseline: boolean }} Stand
 */

/**
 * Vergleicht den aktuellen Feld-Stand mit dem zuletzt gesehenen.
 *
 * **Die Baseline.** Beim ersten brauchbaren Abruf wird nichts angesagt —
 * sonst riefe ein App-Start mitten im Turnier alle laufenden Spiele erneut
 * aus. Brauchbar heißt: Es sind überhaupt Felder dabei. Ein **leerer** Stand
 * taugt nicht, denn genau den liefert der Turnier-PC in den ersten Sekunden,
 * bevor der Sync-Lauf seinen ersten BTP-Schnappschuss hat — würde er als
 * Baseline gelten, wären beim nächsten Abruf alle belegten Felder „neu"
 * (gemeldeter Fehler, 14.08.2026). Felder ohne Spiel sind dagegen ein
 * gültiger Anfangsstand: Am Turniermorgen ist das der Normalfall, und der
 * erste echte Aufruf soll ja angesagt werden.
 *
 * **Der Hallenfilter** wirkt nur auf die Ansage, nicht auf die Baseline: Ein
 * Feld der fremden Halle wird trotzdem gemerkt, damit ein Umschalten der
 * Halle nicht alles Verpasste nachholt.
 *
 * @param {Stand} stand   Stand aus dem vorigen Aufruf
 * @param {Feld[]} felder aktueller Stand aller Felder
 * @param {string} halle  nur diese Halle ansagen (leer = alle)
 * @returns {{ neue: Feld[], stand: Stand }}
 */
export function diffOccupiedCourts(stand, felder, halle) {
  const baseline = new Map(stand.baseline);
  const gefiltert = (halle || "").trim();
  const neue = [];

  for (const feld of felder) {
    const vorher = baseline.get(feld.court_id) ?? 0;
    baseline.set(feld.court_id, feld.match_id);
    const halleOk =
      !gefiltert || (feld.location || "").trim() === gefiltert;
    if (stand.hatBaseline && feld.match_id !== 0 && feld.match_id !== vorher && halleOk) {
      neue.push(feld);
    }
  }

  // Ohne Felder ist nichts zu sehen — und nichts zu merken.
  const hatBaseline = stand.hatBaseline || felder.length > 0;

  return { neue: hatBaseline ? neue : [], stand: { baseline, hatBaseline } };
}
