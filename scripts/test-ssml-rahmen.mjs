// Testet Tempo und Pausen im Azure-SSML (src/io/ssmlRahmen.mjs): Der
// Tempo-Regler muss auch bei Azure wirken, und `<lang>` darf nie in einem
// `<prosody>` stehen (Azure lehnt das ab → Ansage fiele auf die
// Windows-Stimme zurück).
import {
  tempoProzent,
  mitTempo,
  langAbsetzen,
  ssmlDokument,
  BLOCK_PAUSE_MS,
  LANG_PAUSE_MS,
} from "../src/io/ssmlRahmen.mjs";

let failures = 0;
function eq(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${JSON.stringify(want)}, war ${JSON.stringify(got)}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}
function ok(name, cond) {
  eq(name, Boolean(cond), true);
}

eq("Tempo 0,8", tempoProzent(0.8), "-20%");
eq("Tempo 1 = kein prosody", tempoProzent(1), null);
eq("Tempo 1,2", tempoProzent(1.2), "+20%");
eq("Tempo geklemmt unten", tempoProzent(0.1), "-50%");
eq("Tempo geklemmt oben", tempoProzent(9), "+50%");
eq("Tempo fehlt", tempoProzent(undefined), null);
eq("Tempo NaN", tempoProzent(NaN), null);

eq("ohne Tempo unverändert", mitTempo("Feld 3.", 1), "Feld 3.");
eq("Text mit Tempo", mitTempo("Feld 3.", 0.8), '<prosody rate="-20%">Feld 3.</prosody>');

const lp = `<break time="${LANG_PAUSE_MS}ms"/>`;
const bp = `<break time="${BLOCK_PAUSE_MS}ms"/>`;
const mitLang = `Anna und <lang xml:lang="vi-VN">Nguyen Van An</lang>.`;
eq(
  "lang-Span: Tempo innen, nie außen herum",
  mitTempo(mitLang, 0.8),
  '<prosody rate="-20%">Anna und </prosody><lang xml:lang="vi-VN"><prosody rate="-20%">Nguyen Van An</prosody></lang><prosody rate="-20%">.</prosody>',
);
eq(
  "lang-Span abgesetzt",
  langAbsetzen(mitLang),
  `Anna und ${lp}<lang xml:lang="vi-VN">Nguyen Van An</lang>${lp}.`,
);

const doc = ssmlDokument(
  "de",
  "de-DE-SeraphinaMultilingualNeural",
  ["Feld 3.", "Herreneinzel.", `<lang xml:lang="pl-PL">Kowalski</lang>.`, "", "gegen Meier."],
  0.8,
);
ok("Blockpausen zwischen den Teilen", doc.split(bp).length - 1 === 3);
ok("leerer Teil erzeugt keine Doppelpause", !doc.includes(`${bp}${bp}`));
ok("speak de-DE", doc.startsWith('<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="de-DE">'));
ok("Stimme gesetzt", doc.includes('<voice name="de-DE-SeraphinaMultilingualNeural">'));
// Strukturwächter: zwischen einem <prosody> und seinem </prosody> nie ein <lang.
const innen = [...doc.matchAll(/<prosody[^>]*>([\s\S]*?)<\/prosody>/g)].map((m) => m[1]);
ok("kein <lang> in <prosody>", innen.every((t) => !t.includes("<lang")));
eq("Englisch", ssmlDokument("en", "v", ["Court 1."], 1).includes('xml:lang="en-US"'), true);
eq("Stimme escapt", ssmlDokument("de", 'a"b', ["x"], 1).includes('name="a&quot;b"'), true);

if (failures > 0) {
  console.error(`${failures} Fehler`);
  process.exit(1);
}
console.log("Alles grün.");
