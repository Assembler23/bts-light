// Prüft die Inline-Kopien von src/io/monitorSeq.mjs und src/io/pushHealth.mjs
// in den Anzeige-Seiten gegen die Module (Spec monitor-stile-und-stream E2,
// ADR 0069; offener Punkt aus dem Security-Review #328).
//
// Die Kopien sind in ES5 umgeschrieben (andere Parameternamen, `var`), ein
// Wortvergleich ginge also nie auf. Stattdessen wird jede Kopie aus der Seite
// geschnitten, ausgeführt und auf denselben Eingaben gegen das Modul
// verglichen. Weicht eine Kopie im Verhalten ab, zeigt eine Anzeige Stände
// in falscher Ordnung oder hält einen toten Kanal für gesund — beides fällt
// in der Halle erst auf, wenn ein TV einfriert.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { anwenden } from "../src/io/monitorSeq.mjs";
import * as ph from "../src/io/pushHealth.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const SEITEN = ["monitor.html", "tafel.html", "overview.html", "stream.html"];
const hier = dirname(fileURLToPath(import.meta.url));

/** Schneidet `function name(...) { ... }` samt Rumpf aus dem Quelltext. */
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
function konstante(text, name) {
  const m = text.match(new RegExp("var " + name + " = (\\d+);"));
  return m ? Number(m[1]) : null;
}

const SEQ_FAELLE = [];
for (const g of [0, 5, 10, NaN]) for (const n of [0, 5, 6, 10, NaN]) for (const q of ["push", "fetch", "x"]) SEQ_FAELLE.push([g, n, q]);
const JETZT = 100000;
const ZUSTAENDE = [
  null, {}, { wsOpen: false }, { wsOpen: true, lastServerFrameMs: 0 },
  { wsOpen: true, lastServerFrameMs: JETZT - 1000 },
  { wsOpen: true, lastServerFrameMs: JETZT - 25000 },
  { wsOpen: true, lastServerFrameMs: JETZT - 1000, lastFetchOk: false },
  { wsOpen: true, lastServerFrameMs: JETZT - 1000, failures: 2 },
];

for (const seite of SEITEN) {
  const text = readFileSync(join(hier, "..", "src-tauri", "assets", seite), "utf8");
  const quellen = ["seqAnwenden", "pushGesund", "fallbackTakt", "kanalIstTot"].map((n) => funktion(text, n));
  ok(`${seite}: alle vier Kopien vorhanden`, quellen.every(Boolean), true);
  if (!quellen.every(Boolean)) continue;
  const konst = ["HERZSCHLAG_STILL_MS", "FALLBACK_LANGSAM_MS", "FALLBACK_SCHNELL_MS"];
  ok(`${seite}: Konstanten wie im Modul`, konst.map((k) => konstante(text, k)), konst.map((k) => ph[k]));
  const kopf = konst.map((k) => `var ${k} = ${konstante(text, k)};`).join("\n");
  // eslint-disable-next-line no-new-func
  const f = new Function(kopf + "\n" + quellen.join("\n") +
    "\nreturn { seqAnwenden, pushGesund, fallbackTakt, kanalIstTot };")();
  ok(`${seite}: seqAnwenden wie anwenden`,
    SEQ_FAELLE.map(([g, n, q]) => f.seqAnwenden(g, n, q)),
    SEQ_FAELLE.map(([g, n, q]) => anwenden(g, n, q)));
  ok(`${seite}: pushGesund wie Modul`,
    ZUSTAENDE.map((z) => f.pushGesund(z, JETZT)),
    ZUSTAENDE.map((z) => ph.pushGesund(z, JETZT)));
  ok(`${seite}: fallbackTakt wie Modul`,
    [[true, true], [true, false], [false, true], [false, false]].map(([a, b]) => f.fallbackTakt(a, b)),
    [[true, true], [true, false], [false, true], [false, false]].map(([a, b]) => ph.fallbackTakt(a, b)));
  const tot = [[false, 1, JETZT], [true, 0, JETZT], [true, JETZT - 1000, JETZT], [true, JETZT - 25000, JETZT]];
  ok(`${seite}: kanalIstTot wie Modul`,
    tot.map(([a, b, c]) => f.kanalIstTot(a, b, c)),
    tot.map(([a, b, c]) => ph.kanalIstTot(a, b, c)));
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
