// Monitoransicht „Begegnung“ (Spec punktspiel-monitoransicht): reine Logik
// aus src/io/begegnungAnsicht.mjs und die wortgleiche Inline-Kopie in
// src-tauri/assets/begegnung.html.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  MAX_ZEILEN, PLATZ_OFFEN, initialen, namenListe, zustandText, nebenzeile, sichtbareZeilen, zeilenMass,
} from "../src/io/begegnungAnsicht.mjs";

let geprueft = 0;
const ok = (name) => { geprueft++; console.log(`✓ ${name}`); };

// ── Initialen ──
assert.equal(initialen("1. BC Muster 2"), "BM");
assert.equal(initialen("TSV Ölbach 1890"), "TÖ");
assert.equal(initialen("SG Nord-Süd"), "SN");
assert.equal(initialen("Berliner"), "B");
assert.equal(initialen("  "), "?");
assert.equal(initialen(null), "?");
assert.equal(initialen("1234 5"), "?");
ok("Initialen: zwei Buchstaben, Ziffern übersprungen, nie leer");

// ── Namen ──
assert.deepEqual(namenListe(["Anna A"], false), [{ text: "Anna A", offen: false }]);
assert.deepEqual(namenListe([], false), [{ text: PLATZ_OFFEN, offen: true }]);
assert.deepEqual(namenListe(["Anna A"], true), [
  { text: "Anna A", offen: false }, { text: PLATZ_OFFEN, offen: true },
]);
assert.equal(namenListe(["A", "B", "C"], true).length, 2);
assert.deepEqual(namenListe(undefined, true).map((n) => n.offen), [true, true]);
assert.deepEqual(namenListe([{ name: "x" }, "  "], true).map((n) => n.offen), [true, true]);
assert.equal(namenListe(["***"], false)[0].text, "***");
ok("Namen: Teilaufstellung sichtbar, fehlender Platz benannt, anonymisiert bleibt");

// ── Zustand ──
assert.equal(zustandText({ zustand: "offen" }), "");
assert.equal(zustandText({ zustand: "laeuft", feld: 2 }), "Feld 2");
assert.equal(zustandText({ zustand: "laeuft" }), "läuft");
assert.equal(zustandText({ zustand: "wird_uebermittelt", feld: 1 }), "Ergebnis wird übermittelt");
assert.equal(zustandText({ zustand: "beendet", ausgang: "normal" }), "");
assert.equal(zustandText({ zustand: "beendet", ausgang: "kampflos" }), "kampflos");
assert.equal(zustandText({ zustand: "beendet", ausgang: "aufgabe" }), "Aufgabe");
assert.equal(zustandText({ zustand: "beendet", ausgang: "disqualifikation" }), "Disqualifikation");
assert.equal(zustandText({ zustand: "offen", ausgang: "kampflos" }), "");
assert.equal(zustandText(null), "");
ok("Zustandstext je Zustand und Ausgang");

// ── Nebenzeile ──
assert.equal(
  nebenzeile({ heim: { spiele: 5, saetze: 11, punkte: 352 }, gast: { spiele: 3, saetze: 7, punkte: 318 } }),
  "11 : 7 Sätze · 352 : 318 Punkte",
);
assert.equal(nebenzeile(undefined), "0 : 0 Sätze · 0 : 0 Punkte");
ok("Nebenzeile");

// ── Zeilen und Maße ──
assert.equal(sichtbareZeilen(new Array(16).fill({})).length, MAX_ZEILEN);
assert.equal(sichtbareZeilen(null).length, 0);
for (const n of [1, 8, 10, 12, 16]) {
  const m = zeilenMass(780, n, true);
  const zeilen = Math.max(4, Math.min(MAX_ZEILEN, n));
  // Zeilen und Abstände füllen die Höhe genau, nie darüber.
  const summe = m.zeile * zeilen + m.abstand * (zeilen - 1);
  assert.ok(Math.abs(summe - 780) < 0.001, `Höhe bei ${n} Zeilen: ${summe}`);
  // Zwei Namenszeilen (Zeilenhöhe 1,14) passen in die Zeile.
  assert.ok(m.name * 1.14 * 2 <= m.zeile, `Doppelnamen passen bei ${n} Zeilen`);
  assert.ok(m.satz <= m.zeile);
}
assert.ok(zeilenMass(780, 8, false).name > zeilenMass(780, 8, true).name, "ohne Doppel größere Namen");
// Zwölf Zeilen auf 1080p: Namen bleiben lesbar (mindestens 18 px).
assert.ok(zeilenMass(1080 * 0.74 - 34, 12, true).name >= 18);
assert.deepEqual(zeilenMass(0, 0, true), { zeile: 0, abstand: 0, name: 0, satz: 0 });
ok("Zeilen teilen sich die Höhe, Schrift passt in die Zeile");

// ── Inline-Kopie in begegnung.html ──
const hier = dirname(fileURLToPath(import.meta.url));
const zwischen = (text, datei) => {
  const m = text.match(/\/\/ >>> begegnungAnsicht\n([\s\S]*?)\n *\/\/ <<< begegnungAnsicht/);
  assert.ok(m, `${datei}: Marken fehlen`);
  return m[1].split("\n").map((z) => z.replace(/^ {2}/, "")).join("\n");
};
const modul = readFileSync(join(hier, "..", "src", "io", "begegnungAnsicht.mjs"), "utf8");
const seite = readFileSync(join(hier, "..", "src-tauri", "assets", "begegnung.html"), "utf8");
const ausModul = modul.match(/\/\/ >>> begegnungAnsicht\n([\s\S]*?)\n\/\/ <<< begegnungAnsicht/)[1]
  .replace(/^export /gm, "");
assert.equal(zwischen(seite, "begegnung.html"), ausModul, "Inline-Kopie weicht vom Modul ab");
ok("begegnung.html trägt die Kopie wortgleich");

// Die Seite setzt Namen nie als HTML und kennt den Hinweistext.
const skript = seite.slice(seite.indexOf("<script>"));
assert.ok(!/innerHTML|insertAdjacentHTML|document\.write/.test(skript), "kein HTML aus Daten");
assert.match(seite, /Diese Ansicht gibt es nur bei Punktspielen\./);
assert.match(skript, /info\/club-logo\?id=/);
assert.match(skript, /'If-None-Match'/);
ok("Seite: Text statt HTML, Hinweis, Logo per ID, Marke");

console.log(`OK — ${geprueft} Prüfungen`);
