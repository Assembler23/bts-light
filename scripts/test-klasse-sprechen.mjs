// Testet, wie die Klasse hinter der Disziplin gesprochen wird
// (src/io/klasseSprechen.mjs): abgesetzt statt angeklebt, damit aus
// „Herreneinzel A" kein „Herreneinzela" wird.
import { klasseText, klasseSsml, KLASSE_PAUSE_MS } from "../src/io/klasseSprechen.mjs";

let failures = 0;
function eq(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${JSON.stringify(want)}, war ${JSON.stringify(got)}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const pause = `<break time="${KLASSE_PAUSE_MS}ms"/>`;

// Gerätestimme: Komma als Pause.
eq("Text mit Klasse", klasseText("Herreneinzel", "A"), "Herreneinzel, A");
eq("Text ohne Klasse", klasseText("Herreneinzel", ""), "Herreneinzel");
eq("Text Klasse nur Leerraum", klasseText("Herreneinzel", "  "), "Herreneinzel");
eq("Text Jugend", klasseText("Jungeneinzel", "U15"), "Jungeneinzel, U15");
// Ohne Disziplin keine nackte Klasse („A." wäre unverständlich).
eq("Text ohne Disziplin", klasseText("", "A"), "");

// Azure: echte Pause, Buchstaben buchstabiert, alles escapt.
eq("SSML Buchstabe", klasseSsml("Herreneinzel", "A"),
  `Herreneinzel${pause}<say-as interpret-as="characters">A</say-as>`);
eq("SSML zwei Buchstaben", klasseSsml("Herreneinzel", "AB"),
  `Herreneinzel${pause}<say-as interpret-as="characters">AB</say-as>`);
eq("SSML Altersklasse nicht buchstabiert", klasseSsml("Jungeneinzel", "U15"),
  `Jungeneinzel${pause}U15`);
eq("SSML ohne Klasse", klasseSsml("Men's Singles", ""), "Men&apos;s Singles");
eq("SSML escapt Disziplin", klasseSsml("Men's Singles", "B"),
  `Men&apos;s Singles${pause}<say-as interpret-as="characters">B</say-as>`);
eq("SSML escapt Klasse", klasseSsml("Herreneinzel", "<A>"), `Herreneinzel${pause}&lt;A&gt;`);
eq("SSML ohne Disziplin", klasseSsml("", "A"), "");

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Prüfungen bestanden");
