// Besetzung in der Vorbereitungs-Ansage (src/io/prepBesetzung.mjs, Spec
// `sr-ar-beim-aufruf`, ADR 0080): Satzbau, Besetzungs-Schalter, Nachrufe
// ohne Besetzung, alte Aufträge ohne Namen.
//
// Ausfuehren: node scripts/test-vorbereitung-besetzung.mjs

import { besetzungSegmente, besetzungAusAuftrag } from "../src/io/prepBesetzung.mjs";

let fehler = 0;
function pruefe(label, bedingung) {
  console.log((bedingung ? "  ok   " : "  FAIL ") + label);
  if (!bedingung) fehler++;
}
const gleich = (a, b) => JSON.stringify(a) === JSON.stringify(b);

console.log("=== Satzbau ===");
pruefe(
  "SR und AR auf Deutsch",
  gleich(besetzungSegmente(["Anna Schulz"], ["Bob Meier"], "de"), [
    "Schiedsrichter: Anna Schulz.",
    "Aufschlagrichter: Bob Meier.",
  ]),
);
pruefe(
  "nur SR auf Englisch",
  gleich(besetzungSegmente(["Anna Schulz"], [], "en"), ["Umpire: Anna Schulz."]),
);
pruefe("leere und Leerzeichen-Namen fallen weg", gleich(besetzungSegmente([" ", ""], undefined, "de"), []));

console.log("=== Auftrag → Ansage ===");
const job = { kind: "meetingpoint_call", sr: ["Anna Schulz"], ar: ["Bob Meier"] };
pruefe(
  "Erst-Aufruf nennt die Besetzung",
  gleich(besetzungAusAuftrag(job, true), {
    umpireNames: ["Anna Schulz"],
    serviceJudgeNames: ["Bob Meier"],
  }),
);
pruefe("Schalter aus ⇒ keine Namen", gleich(besetzungAusAuftrag(job, false), {}));
pruefe(
  "Nachruf (prep_call) nennt keine Besetzung",
  gleich(besetzungAusAuftrag({ kind: "prep_call", sr: ["X"] }, true), {}),
);
pruefe(
  "alter Auftrag ohne sr/ar ⇒ leere Listen, kein Fehler",
  gleich(besetzungAusAuftrag({ kind: "meetingpoint_call" }, true), {
    umpireNames: [],
    serviceJudgeNames: [],
  }),
);

if (fehler > 0) {
  console.error(`\n${fehler} Fehler`);
  process.exit(1);
}
console.log("\nAlles grün.");
