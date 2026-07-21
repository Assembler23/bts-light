// Wählt die anzuwendende Aussprache-Korrektur für einen (bereits
// normalisierten) Namensschlüssel auf dem Azure-SSML-Pfad.
//
// Warum eigenes, testbares Modul: `announcer.ts` ist nicht node-importierbar
// (zieht Browser-/React-State), die Rangfolge der Korrekturen ist aber die
// eigentliche Logik, die nicht still kaputtgehen darf.
//
// Rangfolge (höchste zuerst):
//   1. IPA-Lautschrift  → präzises Azure-`<phoneme>`  (kind:"ipa")
//   2. phonetische Ersatzschreibweise `say` → als Text gesprochen (kind:"say")
//   3. kein Treffer → null; der Aufrufer fällt auf die `<lang>`-Erkennung
//      (mehrsprachige Azure-Stimme) bzw. den Rohnamen zurück.
//
// Der `say`-Zweig ist der Kern des Fixes: bislang wirkte die phonetische
// Ersatzschreibweise NUR auf der Web-Speech-Stimme, auf dem Azure-Pfad war sie
// totes Feld. Ein Turnierleiter tippt „Chybych" → „Chübüch" und erwartet, dass
// es überall greift.
export function resolveNameCorrection(key, ipaMap, sayMap) {
  const ipa = ipaMap?.get(key);
  if (ipa) return { kind: "ipa", value: ipa };
  const say = sayMap?.get(key);
  if (say) return { kind: "say", value: say };
  return null;
}
