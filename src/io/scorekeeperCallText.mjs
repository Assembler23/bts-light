// Textbausteine des Zähltafelbediener-Nachrufs (Spec `tl-sicht-feinschliff`,
// Punkt 2; der seit ADR 0007 offene Baustein).
//
// Eigene Datei aus demselben Grund wie `nameCorrection.mjs` und
// `disciplineVoice.mjs`: Der Wortlaut muss in BEIDEN Synthese-Pfaden
// identisch sein — Web Speech baut Segmente, Azure baut SSML —, und in
// `announcer.ts` gäbe es dafür keinen Test. Hier liegt er an einer Stelle
// und wird von `scripts/test-scorekeeper-call-text.mjs` in der CI geprüft.
//
// **Kein XML-Escaping hier.** Das macht der SSML-Bauer am Verwendungsort,
// genau wie bei den Schiedsrichter-Segmenten.

/** Stufenwort vor dem Nachruf. Stufe 1 = ohne, wie beim ersten Aufruf. */
function stufenWort(stage, lang) {
  if (stage >= 3) {
    return lang === "en" ? "Third and final call." : "Dritter und letzter Aufruf.";
  }
  if (stage === 2) return lang === "en" ? "Second call." : "Zweiter Aufruf.";
  return null;
}

/**
 * Segmente des Nachrufs: Feld, ggf. Stufenwort, dann die Aufforderung mit
 * den Namen.
 *
 * Die Namen laufen **ohne** Aussprache-Korrektur — es ist eine
 * Zuständigkeits-Ansage, keine Spieler-Vorstellung. Dieselbe Entscheidung
 * wie bei „Tabletbedienung: {Name}" am Ende der Feld-Ansage und bei den
 * Schiedsrichtern.
 *
 * @param {string} courtPhrase Fertige Feld-Nennung („Feld 3" / „Court 3").
 * @param {string[]} names Zugewiesene Bediener.
 * @param {1|2|3} stage
 * @param {"de"|"en"} lang
 * @returns {string[]} Segmente in Sprechreihenfolge; leer, wenn Feld oder
 *   Namen fehlen — ein Nachruf ohne Adressat wäre ein Gong ins Leere.
 */
export function scorekeeperCallSegments(courtPhrase, names, stage, lang) {
  const court = (courtPhrase || "").trim();
  const wer = (names || []).map((n) => (n || "").trim()).filter(Boolean);
  if (!court || wer.length === 0) return [];
  const segmente = [`${court}.`];
  const stufe = stufenWort(stage, lang);
  if (stufe) segmente.push(stufe);
  segmente.push(
    lang === "en"
      ? `${wer.join(" / ")}, please report as scoreboard operator.`
      : `${wer.join(" / ")}, bitte als Tabletbedienung melden.`,
  );
  return segmente;
}
