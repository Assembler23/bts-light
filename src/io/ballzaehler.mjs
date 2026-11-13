/** Ballzähler am Zähl-Tablet (Spec `docs/features/tablet-ballzaehler.md`,
 *  ADR 0093).
 *
 *  Am Tablet werden die verbrauchten Federbälle eines Spiels gezählt; die
 *  Zahl geht mit dem Ergebnis an den Turnier-PC und von dort als
 *  `Match.Shuttles` nach BTP. Hier steht nur die reine Zähl-Logik:
 *  - Bereich 0–99, **+** und **−** bleiben an den Rändern stehen;
 *  - der Zähler ist unabhängig vom Punkt-Undo (er liegt nicht im
 *    Undo-Stapel) — ein falscher Tipp wird mit **−** korrigiert;
 *  - ab dem Übermitteln ist die Zahl eingefroren, bis das Ergebnis wieder
 *    geöffnet wird: Der Turnier-PC vergleicht ein erneut gesendetes Ergebnis
 *    nicht auf die Ballzahl, eine spätere Änderung käme nie an;
 *  - der Ergebnis-Body trägt die Zahl nur, wenn der Turnier-PC den Zähler
 *    anbietet (`MatchBrief.shuttleCounter`).
 *
 *  Kanonische Fassung. `tablet.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam; die
 *  Drift prüft `scripts/test-inline-kopien.mjs`.
 */

/** Höchste Ballzahl — derselbe Deckel wie `relay_proto::MAX_SHUTTLES`. */
export const BALL_MAX = 99;

/**
 * Eine Ballzahl aus fremder Hand (gespeicherter oder übernommener Stand):
 * ganze Zahl im Bereich, alles andere zählt als 0.
 * @param {unknown} wert
 * @returns {number}
 */
export function ballzahlAus(wert) {
  if (typeof wert !== "number" || !Number.isFinite(wert)) return 0;
  const n = Math.floor(wert);
  if (n < 0) return 0;
  return n > BALL_MAX ? BALL_MAX : n;
}

/** Ein Ball mehr; bei 99 bleibt es bei 99. */
export function ballMehr(zahl) {
  const n = ballzahlAus(zahl);
  return n >= BALL_MAX ? BALL_MAX : n + 1;
}

/** Ein Ball weniger; bei 0 bleibt es bei 0. */
export function ballWeniger(zahl) {
  const n = ballzahlAus(zahl);
  return n <= 0 ? 0 : n - 1;
}

/**
 * Zeigt das Tablet den Zähler? Nur wenn der Turnier-PC ihn anbietet und
 * ein Spiel geladen ist.
 * @param {{hostZaehlt?: boolean, hatSpiel?: boolean}} lage
 */
export function zaehlerSichtbar(lage) {
  return !!(lage && lage.hostZaehlt && lage.hatSpiel);
}

/**
 * Sind die Knöpfe **+** und **−** gesperrt? Ab dem Übermitteln (Auftrag unterwegs oder
 * angenommen) bis zum Wiederöffnen; ebenso an einem Spiel, das der
 * Turnier-PC schon von Hand gewertet hat.
 * @param {{unterwegs?: boolean, angenommen?: boolean, finalisiert?: boolean}} lage
 */
export function zaehlerGesperrt(lage) {
  return !!(lage && (lage.unterwegs || lage.angenommen || lage.finalisiert));
}

/**
 * Die Ballzahl für den Ergebnis-Body: die Zahl, wenn der Zähler angeboten
 * war — sonst `undefined` (das Feld bleibt dann ganz weg, und der
 * Turnier-PC lässt stehen, was BTP hat).
 * @param {boolean} hostZaehlt
 * @param {unknown} zahl
 * @returns {number|undefined}
 */
export function ballzahlFuerErgebnis(hostZaehlt, zahl) {
  return hostZaehlt ? ballzahlAus(zahl) : undefined;
}
