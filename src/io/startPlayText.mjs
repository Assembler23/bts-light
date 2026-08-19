// Textbausteine der Ansage „Bitte mit dem Spielen beginnen"
// (Spec `tl-sicht-feinschliff`, Punkt 3).
//
// Eigene Datei statt zweier Textstellen in `announcer.ts`: Die Worte müssen
// in BEIDEN Synthese-Pfaden identisch sein — Web Speech baut Segmente,
// Azure baut SSML —, und in `announcer.ts` gäbe es dafür keinen Test. Hier
// liegen sie an einer Stelle und werden von `scripts/test-start-play-text.mjs`
// in der CI geprüft (Muster `disciplineVoice.mjs`, `announceBaseline.mjs`).
//
// **Kein XML-Escaping hier.** Das macht der SSML-Bauer am Verwendungsort,
// genau wie bei den Schiedsrichter-Segmenten — sonst wäre der Text im
// Web-Speech-Pfad doppelt maskiert.

/**
 * Segmente der Ansage: Feld, dann die Aufforderung.
 *
 * Bewusst **ohne Paarung** (Nutzer-Entscheidung 18.08.2026): Auf dem Feld
 * steht genau eine Paarung, und eine volle Durchsage mitten im Betrieb
 * hielte die Halle länger auf, als die Aufforderung wert ist.
 *
 * Bewusst **ohne Stufenwort**: Die Ansage ist kein Aufruf. „Zweiter Aufruf.
 * Bitte mit dem Spielen beginnen." wäre ein Widerspruch — gerufen wurde ja
 * längst, die Spieler stehen da.
 *
 * @param {string} courtPhrase Fertige Feld-Nennung („Feld 3" / „Court 3").
 *   Kommt vom Aufrufer, weil dort schon die Feld-Beschriftung des Turniers
 *   vorliegt.
 * @param {"de"|"en"} lang
 * @returns {string[]} Segmente in Sprechreihenfolge; leer, wenn kein Feld.
 */
export function startPlaySegments(courtPhrase, lang) {
  const court = (courtPhrase || "").trim();
  if (!court) return [];
  return lang === "en"
    ? [`${court}.`, "Please start playing."]
    : [`${court}.`, "Bitte mit dem Spielen beginnen."];
}
