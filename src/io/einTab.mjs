// Ein Tab statt zwei: Der Knopf „Turnierleitung" in bts-light öffnet die
// Turnierleitungs-Oberfläche im Browser. Von außen lässt sich ein schon
// offener Tab nicht ansteuern — der Browser bekommt nur eine Adresse und
// macht dafür immer einen neuen Tab auf. Aufräumen können deshalb nur die
// Seiten selbst: Sie sprechen sich über einen `BroadcastChannel` ab, und
// eine von beiden schließt sich.
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-ein-tab.mjs` prüft beide auf gleiches Verhalten.
//
// Grenzen, bewusst hingenommen: Ein Tab darf sich nur schließen, wenn der
// Browser es erlaubt (Chrome/Edge: ja, solange der Tab direkt über die
// Adresse geöffnet wurde; Firefox: nein). Greift es nicht, stehen zwei Tabs
// nebeneinander — beide funktionieren, wie vor dieser Absprache.

/** Name des Kanals. Eindeutig, weil die Cloud-Fassung der Seite sich ihre
 *  Herkunft (badhub.de) mit anderen Seiten teilt. */
export const EIN_TAB_KANAL = "bts-light-tl-ein-tab";

/** So alt darf eine Meldung höchstens sein. Ein schlafender Hintergrund-Tab
 *  arbeitet Nachrichten erst beim Aufwachen ab — ohne die Frist schlösse er
 *  dann einen Tab, in dem längst gearbeitet wird. */
export const EIN_TAB_FRIST_MS = 5000;

/**
 * Hat der Knopf in bts-light diese Seite geöffnet?
 *
 * Nur dann meldet sich die Seite bei den anderen. Ein per QR-Code gekoppeltes
 * Gerät oder ein von Hand geöffneter zweiter Tab trägt die Marke nicht und
 * bleibt, wie er ist — wer zwei Fenster nebeneinander will, bekommt sie.
 */
export function vonAppGeoeffnet(hash) {
  return /(?:^|[#&])app=1(?:&|$)/.test(String(hash || ""));
}

/**
 * Was folgt aus einer Nachricht auf dem Kanal?
 *
 * `lage`: `ich` (eigene Kennung), `jetzt` (ms), `sichtbar` (Tab im
 * Vordergrund eines sichtbaren Fensters), `beschaeftigt` (ein Dialog steht
 * offen), `gemeldetUm` (ms, wann sich dieser Tab als neu gemeldet hat; 0 =
 * nie).
 *
 * Antwort: `"melden"` (ich bleibe, der neue Tab soll gehen), `"schliessen"`
 * (ich gehe), `"nichts"`.
 *
 * Die Regel für den schon offenen Tab:
 * - **sichtbar** (etwa auf dem zweiten Bildschirm): Er bleibt, wo er ist —
 *   der neue Tab geht. Sonst sprünge die Oberfläche bei jedem Klick in ein
 *   anderes Fenster.
 * - **verdeckt und unbeschäftigt**: Er geht, der neue steht ja schon vorn.
 *   Das ist der übliche Fall „zweiter Klick auf den Knopf".
 * - **verdeckt, aber mitten in einer Eingabe** (Ergebnis-Dialog offen):
 *   Nichts — lieber zwei Tabs als eine verlorene Eingabe.
 */
export function einTabSchritt(nachricht, lage) {
  if (!nachricht || typeof nachricht !== "object" || !lage) return "nichts";
  if (!nachricht.von || nachricht.von === lage.ich) return "nichts";
  if (nachricht.typ === "neu") {
    const alter = lage.jetzt - nachricht.um;
    if (!(alter >= -EIN_TAB_FRIST_MS && alter <= EIN_TAB_FRIST_MS)) return "nichts";
    if (lage.sichtbar) return "melden";
    return lage.beschaeftigt ? "nichts" : "schliessen";
  }
  if (nachricht.typ === "bleibe") {
    // Nur die Antwort auf die EIGENE Meldung zählt, und nur kurz danach.
    if (nachricht.an !== lage.ich || !lage.gemeldetUm) return "nichts";
    const seit = lage.jetzt - lage.gemeldetUm;
    return seit >= 0 && seit <= EIN_TAB_FRIST_MS ? "schliessen" : "nichts";
  }
  return "nichts";
}
