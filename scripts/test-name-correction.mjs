// Testet die Rangfolge der Aussprache-Korrektur auf dem Azure-Pfad
// (src/io/nameCorrection.mjs) — das echte Modul, das `nameSsml` nutzt.
// Kern: die phonetische Ersatzschreibweise `say` greift jetzt auch bei Azure
// (vorher totes Feld), IPA hat aber Vorrang.
import { resolveNameCorrection } from "../src/io/nameCorrection.mjs";

let failures = 0;
function eq(name, got, want) {
  if (JSON.stringify(got) !== JSON.stringify(want)) {
    console.error(`✗ ${name}: erwartet ${JSON.stringify(want)}, war ${JSON.stringify(got)}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const ipa = new Map([["chybych", "xybʏç"]]);
const say = new Map([
  ["chybych", "Chübüch"],
  ["nguyen", "Nwujen"],
]);

// Der reale Fall: ohne IPA gewinnt die Ersatzschreibweise (früher ignoriert).
eq("say allein greift", resolveNameCorrection("nguyen", new Map(), say), {
  kind: "say",
  value: "Nwujen",
});
// IPA schlägt say, wenn beides da ist (präziseste Korrektur).
eq("ipa schlägt say", resolveNameCorrection("chybych", ipa, say), {
  kind: "ipa",
  value: "xybʏç",
});
// Nur IPA vorhanden.
eq("ipa allein", resolveNameCorrection("chybych", ipa, new Map()), {
  kind: "ipa",
  value: "xybʏç",
});
// Kein Treffer → null (Aufrufer fällt auf <lang>-Erkennung zurück).
eq("kein Treffer → null", resolveNameCorrection("mueller", ipa, say), null);
// Fehlende Maps sind unkritisch.
eq("keine Maps → null", resolveNameCorrection("chybych"), null);
eq(
  "nur sayMap übergeben",
  resolveNameCorrection("nguyen", undefined, say),
  { kind: "say", value: "Nwujen" },
);
// Leerer Wert zählt nicht als Treffer.
eq(
  "leerer say kein Treffer",
  resolveNameCorrection("x", new Map(), new Map([["x", ""]])),
  null,
);

if (failures > 0) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle name-correction-Tests grün.");
