// Testet die Textbausteine der Spielbeginn-Ansage (src/io/startPlayText.mjs,
// Spec tl-sicht-feinschliff Punkt 3) — dasselbe Modul, das beide
// Synthese-Pfade in announcer.ts nutzen.
import { startPlaySegments } from "../src/io/startPlayText.mjs";

let failures = 0;
function eq(name, got, want) {
  const a = JSON.stringify(got), b = JSON.stringify(want);
  if (a !== b) {
    console.error(`✗ ${name}: erwartet ${b}, war ${a}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// Der Wortlaut ist mit dem Nutzer abgestimmt (18.08.2026) und steht so in
// der Spec — er darf sich nicht unbemerkt ändern.
eq("deutsch", startPlaySegments("Feld 3", "de"), [
  "Feld 3.",
  "Bitte mit dem Spielen beginnen.",
]);
eq("englisch", startPlaySegments("Court 3", "en"), [
  "Court 3.",
  "Please start playing.",
]);

// Feld-Beschriftungen sind frei: Turniere nennen ihre Felder auch „Platz A"
// oder „Court 12b". Die Phrase wird 1:1 übernommen, nur der Punkt kommt dazu.
eq("freie Feld-Beschriftung", startPlaySegments("Platz A", "de"), [
  "Platz A.",
  "Bitte mit dem Spielen beginnen.",
]);
eq("Leerraum wird getrimmt", startPlaySegments("  Feld 7  ", "de"), [
  "Feld 7.",
  "Bitte mit dem Spielen beginnen.",
]);

// Ohne Feld gibt es nichts anzusagen — „Bitte mit dem Spielen beginnen"
// allein in die Halle gerufen, wüsste niemand, wer gemeint ist.
eq("ohne Feld leer", startPlaySegments("", "de"), []);
eq("nur Leerraum leer", startPlaySegments("   ", "en"), []);
eq("undefined leer", startPlaySegments(undefined, "de"), []);

// Die Ansage ist KEIN Aufruf: kein Stufenwort, keine Paarung. Das ist
// Akzeptanzkriterium A3.2/A3.3 und hier festgenagelt.
const de = startPlaySegments("Feld 1", "de").join(" ");
eq("kein Stufenwort", /Aufruf/i.test(de), false);
eq("nur zwei Segmente", startPlaySegments("Feld 1", "de").length, 2);

// Kein XML-Escaping in diesem Modul — das macht der SSML-Bauer, sonst wäre
// der Text im Web-Speech-Pfad doppelt maskiert.
eq("kein XML-Escaping", startPlaySegments("Feld & 1", "de")[0], "Feld & 1.");

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests der Spielbeginn-Ansage bestanden.");
