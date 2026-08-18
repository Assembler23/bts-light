// Testet die Textbausteine des Zähltafelbediener-Nachrufs
// (src/io/scorekeeperCallText.mjs, Spec tl-sicht-feinschliff Punkt 2).
import { scorekeeperCallSegments } from "../src/io/scorekeeperCallText.mjs";

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

// Der Wortlaut ist mit dem Nutzer abgestimmt (18.08.2026).
eq("erster Nachruf, deutsch", scorekeeperCallSegments("Feld 3", ["Meier"], 1, "de"), [
  "Feld 3.",
  "Meier, bitte als Tabletbedienung melden.",
]);
eq("erster Nachruf, englisch", scorekeeperCallSegments("Court 3", ["Meier"], 1, "en"), [
  "Court 3.",
  "Meier, please report as scoreboard operator.",
]);

// Ab Stufe 2 das Stufenwort davor — die Halle soll hören, dass es dringender
// wird.
eq("zweiter Nachruf", scorekeeperCallSegments("Feld 3", ["Meier", "Kraus"], 2, "de"), [
  "Feld 3.",
  "Zweiter Aufruf.",
  "Meier / Kraus, bitte als Tabletbedienung melden.",
]);
eq("dritter Nachruf", scorekeeperCallSegments("Feld 3", ["Meier"], 3, "de"), [
  "Feld 3.",
  "Dritter und letzter Aufruf.",
  "Meier, bitte als Tabletbedienung melden.",
]);
eq("dritter Nachruf, englisch", scorekeeperCallSegments("Court 3", ["Meier"], 3, "en"), [
  "Court 3.",
  "Third and final call.",
  "Meier, please report as scoreboard operator.",
]);

// Ein Doppel bedient zu zweit — beide Namen, mit „/" verbunden wie bei den
// Schiedsrichtern.
eq("zwei Namen", scorekeeperCallSegments("Feld 1", ["Anna Alt", "Bea Bach"], 1, "de"), [
  "Feld 1.",
  "Anna Alt / Bea Bach, bitte als Tabletbedienung melden.",
]);

// Ohne Adressat oder ohne Feld gibt es nichts anzusagen.
eq("ohne Namen leer", scorekeeperCallSegments("Feld 3", [], 1, "de"), []);
eq("nur leere Namen", scorekeeperCallSegments("Feld 3", ["", "  "], 1, "de"), []);
eq("ohne Feld leer", scorekeeperCallSegments("", ["Meier"], 1, "de"), []);
eq("undefined leer", scorekeeperCallSegments(undefined, undefined, 1, "de"), []);

// Leere Einträge zwischen echten Namen fallen weg, statt „Meier / , Kraus"
// zu erzeugen.
eq("Lücken fallen weg", scorekeeperCallSegments("Feld 2", ["Meier", "", "Kraus"], 1, "de"), [
  "Feld 2.",
  "Meier / Kraus, bitte als Tabletbedienung melden.",
]);

// Kein XML-Escaping in diesem Modul — das macht der SSML-Bauer.
eq("kein XML-Escaping", scorekeeperCallSegments("Feld 1", ["A & B"], 1, "de")[1],
  "A & B, bitte als Tabletbedienung melden.");

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests des Bediener-Nachrufs bestanden.");
