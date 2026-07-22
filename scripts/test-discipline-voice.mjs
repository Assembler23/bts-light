// Testet die Stimmenwahl je Disziplin (src/io/disciplineVoice.mjs) — das echte
// Modul, das der Announcer für den Azure-Pfad nutzt.
import { voiceForDiscipline } from "../src/io/disciplineVoice.mjs";

let failures = 0;
function eq(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const F = "de-DE-FlorianMultilingualNeural"; // männlich
const S = "de-DE-SeraphinaMultilingualNeural"; // weiblich (Standard)
const map = {
  mens_singles: F,
  mens_doubles: F,
  womens_singles: S,
  womens_doubles: S,
  // mixed absichtlich NICHT gesetzt → Standard
};

eq("Herreneinzel → männlich", voiceForDiscipline(S, map, "mens_singles"), F);
eq("Herrendoppel → männlich", voiceForDiscipline(S, map, "mens_doubles"), F);
eq("Dameneinzel → weiblich", voiceForDiscipline(S, map, "womens_singles"), S);
eq("Mixed (nicht gesetzt) → Standard", voiceForDiscipline(S, map, "mixed"), S);
eq("unknown → Standard", voiceForDiscipline(S, map, "unknown"), S);
eq("kein Map → Standard", voiceForDiscipline(S, undefined, "mens_singles"), S);
eq("leeres Map → Standard", voiceForDiscipline(S, {}, "mens_singles"), S);
eq("leerer Wert → Standard", voiceForDiscipline(S, { mens_singles: "" }, "mens_singles"), S);
eq("Whitespace-Wert → Standard", voiceForDiscipline(S, { mens_singles: "  " }, "mens_singles"), S);
eq("keine Disziplin → Standard", voiceForDiscipline(S, map, undefined), S);

if (failures > 0) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle discipline-voice-Tests grün.");
