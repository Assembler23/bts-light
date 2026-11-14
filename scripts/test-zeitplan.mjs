// Testet die Texte zur geplanten Zeit (src/io/zeitplan.mjs, Spec tl-zeitplan)
// und schneidet die Inline-Kopien aus tl.html, um sie auf denselben Eingaben
// gegen das Modul laufen zu lassen. Eine abweichende Kopie zeigte der
// Turnierleitung eine falsche Planzeit — und daran liest sie ab, ob das
// Turnier im Zeitplan liegt.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as modul from "../src/io/zeitplan.mjs";

const { planZeitText, startAbweichungText } = modul;

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── planZeitText ──
// 15.11.2026 ist ein Sonntag, 14.11.2026 ein Samstag.
ok("heute nur Uhrzeit", planZeitText(202611141020, 20261114), "10:20");
ok("führende Nullen", planZeitText(202611140905, 20261114), "09:05");
ok("morgen mit Wochentag", planZeitText(202611150900, 20261114), "So 09:00");
ok("gestern mit Wochentag", planZeitText(202611131830, 20261114), "Fr 18:30");
ok("ohne heute nur Uhrzeit", planZeitText(202611150900, 0), "09:00");
ok("heute fehlt", planZeitText(202611150900, undefined), "09:00");
ok("ohne Planzeit", planZeitText(null, 20261114), "");
ok("fehlende Planzeit", planZeitText(undefined, 20261114), "");
ok("Unsinn", planZeitText("abc", 20261114), "");
ok("zu kurz", planZeitText(2026111410, 20261114), "");
ok("Stunde 24", planZeitText(202611142400, 20261114), "");
ok("Monat 0", planZeitText(202600141020, 20261114), "");
ok("Mitternacht", planZeitText(202611140000, 20261114), "00:00");

// ── startAbweichungText ──
ok("später", startAbweichungText(18), "+18 min");
ok("früher", startAbweichungText(-4), "−4 min");
ok("pünktlich", startAbweichungText(0), "±0 min");
ok("fehlt (null)", startAbweichungText(null), "");
ok("fehlt (undefined)", startAbweichungText(undefined), "");
ok("Unsinn", startAbweichungText("x"), "");

// ── Inline-Kopien in tl.html ──
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

function kopieVon(name) {
  const quelle = funktion(tl, name);
  ok(`tl.html trägt die Kopie von ${name}`, !!quelle, true);
  // eslint-disable-next-line no-new-func
  return quelle ? new Function(quelle + `\nreturn ${name};`)() : null;
}

const planKopie = kopieVon("planZeitText");
if (planKopie) {
  const faelle = [];
  for (const heute of [20261114, 20261115, 20270101, 0, undefined, "x"]) {
    for (const pt of [202611141020, 202611150900, 202612312359, 202701010000,
      202611142400, 202613011000, 2026, null, undefined, "abc", NaN]) {
      faelle.push([pt, heute]);
    }
  }
  ok("planZeitText: Kopie verhält sich wie das Modul",
    faelle.map(([p, h]) => planKopie(p, h)),
    faelle.map(([p, h]) => planZeitText(p, h)));
}
const abwKopie = kopieVon("startAbweichungText");
if (abwKopie) {
  const faelle = [-721, -60, -5, -1, 0, 1, 5, 18, 720, 1.9, -1.9, null, undefined, "", "x", NaN];
  ok("startAbweichungText: Kopie verhält sich wie das Modul",
    faelle.map((m) => abwKopie(m)),
    faelle.map((m) => startAbweichungText(m)));
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
