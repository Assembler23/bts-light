// Testet den Text der Marke „Zuletzt gespielt" (src/io/zuletztGespielt.mjs,
// Spec tl-zuletzt-gespielt) und schneidet die Inline-Kopie aus tl.html, um
// sie auf denselben Eingaben gegen das Modul laufen zu lassen. Eine
// abweichende Kopie zeigte der Turnierleitung eine falsche Pausenlänge —
// und genau danach entscheidet sie, wen sie aufruft.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { zuletztText } from "../src/io/zuletztGespielt.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const T = 10 * 3600 * 1000;
const M = 60000;
ok("unter einer Minute", zuletztText(T - 59000, T), "gerade eben");
ok("genau eine Minute", zuletztText(T - M, T), "vor 1 min");
ok("42 Minuten", zuletztText(T - 42 * M - 30000, T), "vor 42 min");
ok("59 Minuten", zuletztText(T - 59 * M, T), "vor 59 min");
ok("eine Stunde", zuletztText(T - 60 * M, T), "vor 1 h 00");
ok("1 h 05", zuletztText(T - 65 * M, T), "vor 1 h 05");
ok("3 h 20", zuletztText(T - 200 * M, T), "vor 3 h 20");
ok("Zukunft (Uhrversatz)", zuletztText(T + 5 * M, T), "gerade eben");
ok("Unsinn", zuletztText(NaN, T), "gerade eben");

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
const quelle = funktion(tl, "zuletztText");
ok("tl.html trägt die Kopie", !!quelle, true);
if (quelle) {
  // eslint-disable-next-line no-new-func
  const kopie = new Function(quelle + "\nreturn zuletztText;")();
  const faelle = [];
  for (let d = -3 * M; d <= 5 * 3600 * 1000; d += 17000) faelle.push(T - d);
  faelle.push(NaN);
  ok("Kopie verhält sich wie das Modul",
    faelle.map((s) => kopie(s, T)),
    faelle.map((s) => zuletztText(s, T)));
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
