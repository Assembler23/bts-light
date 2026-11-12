// Keine „Deutsch / Englisch"-Leiste auf den Anzeigen.
//
// Chromium hält eine Seite voller Spielernamen gern für englisch und bietet
// oben die Übersetzung an — am Hallen-TV drückt das niemand weg. Die
// Startschalter am Pi haben das zweimal nicht dauerhaft verhindert (v0.9.44
// und davor). Verlässlich ist nur, dass die Seite selbst die Übersetzung
// ablehnt. Dieser Test hält das für jede ausgelieferte Seite fest, damit eine
// neue Seite die Marke nicht vergisst.
import { readFileSync, readdirSync } from "node:fs";
import assert from "node:assert/strict";

const ordner = new URL("../src-tauri/assets/", import.meta.url);
const seiten = readdirSync(ordner).filter((n) => n.endsWith(".html"));
assert.ok(seiten.length >= 10, "Seiten-Ordner nicht gefunden");

for (const name of seiten) {
  const kopf = readFileSync(new URL(name, ordner), "utf8").split("</head>")[0];
  assert.match(kopf, /<html lang="de" translate="no">/, `${name}: translate="no" fehlt am <html>`);
  assert.match(kopf, /<meta name="google" content="notranslate"\s*\/?>/, `${name}: notranslate-Meta fehlt`);
  console.log(`✓ ${name}`);
}

// Die Hinweisseite des Pi („noch keine Adresse") und die Richtlinie.
const pi = readFileSync(new URL("../pi/setup-monitor.sh", import.meta.url), "utf8");
assert.match(pi, /<html lang="de" translate="no">/, "Pi-Hinweisseite ohne translate=\"no\"");
assert.match(pi, /"TranslateEnabled": false/, "Pi-Richtlinie TranslateEnabled fehlt");
console.log(`\n${seiten.length} Seiten + Pi-Skript geprüft.`);
