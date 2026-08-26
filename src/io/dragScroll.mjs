/** Auto-Scrollen während einer Umsortier-Ziehgeste
 *  (`enableReorderDrag` in `assets/tl.html`, Spec
 *  `docs/features/spielliste-manuelle-reihenfolge.md`).
 *
 *  **Warum es das gibt.** Die erste Fassung addierte eine feste Pixelzahl
 *  pro `pointermove`. Das hat einen Konstruktionsfehler, der bei kurzen
 *  Listen kaum auffällt und bei fünfzig bis sechzig Spielen die ganze Geste
 *  unbrauchbar macht: Wer den Zeiger am unteren Rand **still hält** — genau
 *  das, was man tut, während man aufs Weiterscrollen wartet — erzeugt keine
 *  Move-Ereignisse mehr, und das Scrollen bleibt stehen. Man muss am Rand
 *  wackeln, damit die Liste weiterläuft. Deshalb ist das Tempo hier eine
 *  Funktion der **Zeigerposition**, nicht der Ereignisse; ein Bildtakt
 *  (`requestAnimationFrame`) ruft sie auf, solange der Zug läuft.
 *
 *  **Warum quadratisch.** Eine lineare Rampe zwingt zur Wahl zwischen
 *  „schnell genug, um sechzig Zeilen zu überbrücken" und „langsam genug, um
 *  am Rand noch genau einsortieren zu können". Das Quadrat der Randnähe gibt
 *  beides: knapp innerhalb der Zone kriecht es, am Rand fährt es.
 *
 *  Kanonische Fassung. `assets/tl.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build und können keine Module laden) — Änderungen hier
 *  und dort gemeinsam.
 */

/**
 * Mindestbewegung, ab der aus einem Druck ein Zug wird — in Pixeln.
 *
 * **Warum es das braucht.** Seit das Auto-Scrollen im Bildtakt aus der
 * Zeigerposition läuft, ist ein bloßes Gedrückthalten nicht mehr folgenlos:
 * Wer den Griff der obersten sichtbaren Zeile antippt, hat den Zeiger schon
 * mitten in der oberen Scroll-Zone. Gemessen am echten Code: 300 ms Halten
 * ohne jede Bewegung scrollten 101 px, verschoben die Zeile um drei Plätze
 * und schickten die neue Reihenfolge an den Turnier-PC. Vor der
 * Bildtakt-Umstellung war ein bewegungsloser Druck garantiert wirkungslos,
 * weil ohne `pointermove` gar nichts passierte.
 *
 * Der Wert ist knapp gewählt: groß genug gegen Zittern und den Wackler beim
 * Absetzen des Fingers, klein genug, dass ein gewollter Zug sofort anspricht.
 */
export const ZUG_SCHWELLE_PX = 6;

/**
 * Hat sich der Zeiger seit dem Aufsetzen weit genug bewegt, dass es ein Zug
 * ist und kein Tipp?
 *
 * Maximumnorm statt Luftlinie: eine reine Auf-ab-Bewegung ist der Normalfall
 * beim Umsortieren, und die Wurzel spart man sich.
 *
 * @param {number} x0 Zeigerposition beim Aufsetzen.
 * @param {number} y0 Zeigerposition beim Aufsetzen.
 * @param {number} x Aktuelle Zeigerposition.
 * @param {number} y Aktuelle Zeigerposition.
 * @param {number} [schwelle] Mindestbewegung.
 * @returns {boolean} Bei unbrauchbaren Eingaben `false` — im Zweifel KEIN Zug.
 */
export function ueberSchwelle(x0, y0, x, y, schwelle = ZUG_SCHWELLE_PX) {
  if (![x0, y0, x, y, schwelle].every(Number.isFinite)) return false;
  return Math.abs(x - x0) >= schwelle || Math.abs(y - y0) >= schwelle;
}

/** Breite der Scroll-Zone an jedem Rand, in Pixeln. */
export const SCROLL_ZONE_PX = 60;

/** Höchsttempo direkt am Rand, in Pixeln pro Sekunde. Bei 60 Zeilen à ~44 px
 *  überstreicht das die ganze Liste in gut zweieinhalb Sekunden — schnell
 *  genug, um nicht zu nerven, langsam genug, um das Ziel vorbeiziehen zu
 *  sehen. */
export const MAX_SCROLL_PX_S = 900;

/**
 * Wie schnell soll gerade gescrollt werden?
 *
 * @param {number} y Zeigerposition (clientY).
 * @param {number} oben Oberkante des scrollbaren Kastens (clientY).
 * @param {number} unten Unterkante des scrollbaren Kastens (clientY).
 * @param {number} [margin] Breite der Scroll-Zone je Rand.
 * @param {number} [maxTempo] Höchsttempo am Rand.
 * @returns {number} Pixel pro Sekunde; negativ = nach oben, 0 = nicht scrollen.
 */
export function ziehScrollTempo(y, oben, unten, margin = SCROLL_ZONE_PX, maxTempo = MAX_SCROLL_PX_S) {
  if (![y, oben, unten, margin, maxTempo].every(Number.isFinite)) return 0;
  const hoehe = unten - oben;
  if (hoehe <= 0 || margin <= 0 || maxTempo <= 0) return 0;
  // Bei einem sehr flachen Panel dürfen sich die beiden Zonen nicht
  // überlappen — sonst zöge die Mitte gleichzeitig nach oben und unten.
  const zone = Math.min(margin, hoehe / 2);

  // Der Zeiger darf den Kasten verlassen (Abstand wird negativ): Wer nach
  // oben aus dem Panel herausfährt, meint eindeutig „weiter nach oben".
  const abstandOben = y - oben;
  const abstandUnten = unten - y;
  if (abstandOben < abstandUnten) {
    const naehe = Math.min(1, Math.max(0, (zone - abstandOben) / zone));
    return -maxTempo * naehe * naehe;
  }
  const naehe = Math.min(1, Math.max(0, (zone - abstandUnten) / zone));
  return maxTempo * naehe * naehe;
}

/**
 * Pixel für DIESEN Bildtakt aus dem Tempo.
 *
 * Der Zeitschritt wird gedeckelt: Lag die Seite im Hintergrund oder hat der
 * Browser einen Takt verschluckt, käme sonst ein Sprung über die halbe Liste
 * heraus, obwohl der Nutzer nichts getan hat.
 *
 * @param {number} tempo Pixel pro Sekunde (aus `ziehScrollTempo`).
 * @param {number} dtMs Vergangene Zeit seit dem letzten Takt, in Millisekunden.
 * @param {number} [maxDtMs] Obergrenze für den Zeitschritt.
 * @returns {number} Pixel, um die zu scrollen ist (Vorzeichen wie `tempo`).
 */
export function scrollSchritt(tempo, dtMs, maxDtMs = 50) {
  if (!Number.isFinite(tempo) || !Number.isFinite(dtMs)) return 0;
  const dt = Math.min(Math.max(dtMs, 0), maxDtMs);
  return (tempo * dt) / 1000;
}
