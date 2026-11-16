// Testet, wann eine Zeile der TL-Warteliste grün statt gelb wird
// (src/io/spielbereitFarbe.mjs), und vergleicht die Inline-Kopie in tl.html
// auf denselben Eingaben. An der Farbe liest die Turnierleitung ab, welche
// Partie aufs Feld kann — eine abweichende Kopie färbte die falschen Zeilen.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spielbereitGruen } from "../src/io/spielbereitFarbe.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const ruf = { called_at_ms: 1 };
const spiel = (extra = {}) => ({ prep_call: ruf, meetingpoint: { bereit: true }, highlight: 1, ...extra });

ok("gerufen, gelb, alle da → grün", spielbereitGruen(spiel()), true);
ok("gerufen, noch ohne Farbe, alle da → grün", spielbereitGruen(spiel({ highlight: 0 })), true);
ok("Farbe fehlt ganz (alter Turnier-PC) → grün", spielbereitGruen(spiel({ highlight: undefined })), true);
ok("noch nicht alle da → bleibt gelb",
  spielbereitGruen(spiel({ meetingpoint: { bereit: false } })), false);
ok("ohne Meetingpoint → bleibt gelb", spielbereitGruen(spiel({ meetingpoint: undefined })), false);
ok("nicht gerufen → nie grün", spielbereitGruen(spiel({ prep_call: null })), false);
for (const h of [2, 3, 4, 5, 6]) {
  ok(`Handfarbe ${h} bleibt stehen`, spielbereitGruen(spiel({ highlight: h })), false);
}
ok("kein Spiel", spielbereitGruen(undefined), false);

// ── Inline-Kopie in tl.html ──
function funktion(text, name) {
  const start = text.indexOf("function " + name + "(");
  if (start < 0) return null;
  let tiefe = 0;
  for (let i = text.indexOf("{", start); i < text.length; i++) {
    if (text[i] === "{") tiefe++;
    else if (text[i] === "}" && --tiefe === 0) return text.slice(start, i + 1);
  }
  return null;
}
const hier = dirname(fileURLToPath(import.meta.url));
const tl = readFileSync(join(hier, "..", "src-tauri", "assets", "tl.html"), "utf8");
const quelle = funktion(tl, "spielbereitGruen");
ok("tl.html trägt die Kopie", !!quelle, true);
if (quelle) {
  // eslint-disable-next-line no-new-func
  const kopie = new Function(quelle + "\nreturn spielbereitGruen;")();
  const faelle = [undefined, null, {}];
  for (const prep_call of [null, ruf])
    for (const meetingpoint of [undefined, { bereit: false }, { bereit: true }])
      for (const highlight of [undefined, 0, 1, 2, 3, 4, 5, 6, 7])
        faelle.push({ prep_call, meetingpoint, highlight });
  ok("Kopie verhält sich wie das Modul",
    faelle.map((m) => kopie(m)), faelle.map((m) => spielbereitGruen(m)));
  // Die Zeile muss die Regel auch anwenden — und die Farbe muss es geben.
  ok("die Listenzeile fragt spielbereitGruen",
    /spielbereitGruen\(m\) \? " spielbereit"/.test(tl), true);
  ok("CSS kennt .row.spielbereit", tl.includes(".row.spielbereit"), true);
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
