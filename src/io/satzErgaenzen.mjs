/** Autovervollständigung im Ergebnis-Dialog des Zähl-Tablets
 *  („Ergebnis eintragen", Usability-Durchgang 15.11.2026).
 *
 *  Wer einen Endstand nachträgt, tippt fast immer zuerst die Punkte des
 *  VERLIERERS — die des Siegers stehen damit fest: 11 heißt 21:11, 20 heißt
 *  22:20. Der Dialog füllt das Gegenfeld deshalb selbst aus. Er rät nur,
 *  wo es nichts zu raten gibt:
 *
 *  - unter „Ziel − 1"  → der Sieger hat genau das Ziel (11 → 21),
 *  - genau „Ziel − 1"  → Verlängerung um zwei (20 → 22), höchstens der Deckel,
 *  - ab dem Ziel       → nichts: 21 kann 21:0 bis 21:19 sein, 24 sowohl
 *                        24:22 als auch 24:26.
 *
 *  Ein Vorschlag bleibt ein Vorschlag: Er folgt der Eingabe, solange das
 *  Gegenfeld nicht von Hand angefasst wurde („1" → 21, „11" → 21, „20" →
 *  22), und verschwindet wieder, wenn die Eingabe keinen mehr hergibt
 *  („2" → 21, „21" → leer). Ein von Hand gesetzter Wert wird nie
 *  überschrieben.
 *
 *  Kanonische Fassung. `tablet.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam; die
 *  Drift prüft `scripts/test-inline-kopien.mjs`.
 */

/** Punkte des Siegers zu den Punkten des Verlierers — oder `null`, wenn
 *  sie nicht eindeutig sind. `target` = Satzziel (21/15/11; ab 99 =
 *  Zeitspiel, dort gibt es kein Ziel), `cap` = Obergrenze des Satzes. */
export function satzErgaenzen(punkte, target, cap) {
  if (!Number.isInteger(punkte) || punkte < 0) return null;
  if (!Number.isInteger(target) || target < 2 || target >= 99) return null;
  if (punkte >= target) return null;
  if (punkte < target - 1) return target;
  var deckel = Number.isInteger(cap) && cap >= target ? cap : punkte + 2;
  return Math.min(punkte + 2, deckel);
}

/** Was mit dem GEGENFELD geschieht, nachdem in ein Feld getippt wurde.
 *  `eingabe` = roher Feldtext, `gegenWert`/`gegenAuto` = Stand des
 *  Gegenfelds (Zahl; stammt sie aus einem Vorschlag?). Liefert `null`
 *  (Gegenfeld nicht anfassen) oder die Zahl, die dort stehen soll — `0`
 *  nimmt einen früheren Vorschlag zurück. */
export function gegenfeldNach(eingabe, gegenWert, gegenAuto, target, cap) {
  if (!gegenAuto && gegenWert !== 0) return null;
  var text = String(eingabe == null ? "" : eingabe).trim();
  var vorschlag = /^\d+$/.test(text) ? satzErgaenzen(Number(text), target, cap) : null;
  if (vorschlag === null) return gegenAuto ? 0 : null;
  return vorschlag;
}
