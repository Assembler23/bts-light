#!/usr/bin/env node
// Prueft die nachpruefbaren Behauptungen des Handbuchs gegen den Code.
//
// WARUM DAS ZAEHLT: Beim Schreiben der Handbuch-Kapitel (05./06.09.2026) fand
// die Gegenpruefung in JEDEM Kapitel echte Sachfehler — ueber dreissig
// insgesamt. Darunter "das Ergebnis geht von allein an die Turnierleitung"
// (es muss uebermittelt werden und kann abgelehnt werden), "kampflos kann nur
// die Turnierleitung werten" (es geht am Tablet), "Cloud ist voreingestellt
// an" (ausgeliefert wird nur LAN) und ein falscher Port fuer Liga-Turniere.
//
// Keiner dieser Fehler war beim Schreiben erkennbar — sie sahen alle
// plausibel aus. Und keiner waere je aufgefallen: Ein Handbuch meldet sich
// nicht, wenn es luegt.
//
// WAS DIESER TEST KANN UND WAS NICHT: Er prueft keine Bedeutung. Er prueft,
// dass die Zeichenketten, an denen eine Aussage haengt, im Code noch so
// stehen — Knopfbeschriftungen, Routen, Ports, Standardwerte. Benennt jemand
// einen Knopf um oder aendert einen Standardwert, wird dieser Test rot und
// nennt die Handbuch-Zeile, die nachzuziehen ist.
//
// SO WIRD EINE BEHAUPTUNG VERANKERT — direkt neben dem Satz im Markdown:
//
//   Der Knopf heisst **Court übernehmen**.
//   <!-- pruef: "Court übernehmen" in src-tauri/assets/tablet.html -->
//
//   Der Port ist 8088.
//   <!-- pruef: /TABLET_PORT: u16 = 8088/ in src-tauri/src/tablet/server.rs -->
//
//   Voreingestellt ist nur LAN, nicht Cloud.
//   <!-- pruef-nicht: "#[default]\n    LanCloud" in src-tauri/src/config.rs -->
//
// In Anfuehrungszeichen = woertlich, zwischen Schraegstrichen = regulaerer
// Ausdruck. `pruef-nicht` verlangt, dass etwas NICHT vorkommt.
//
// Aufruf:  node scripts/test-handbuch-fakten.mjs
// Exit:    0 = ok, 1 = mind. eine Behauptung stimmt nicht mehr.

import { readFileSync, existsSync } from "node:fs";

const manifest = JSON.parse(readFileSync("docs/handbuch.json", "utf8"));
const seiten = manifest.gruppen.flatMap((g) => g.seiten).filter((s) => s.datei);

let fehler = 0;
let geprueft = 0;
const pruefe = (ok, text) => {
  console.log(`  ${ok ? "✓" : "✗"} ${text}`);
  if (!ok) fehler++;
};

// <!-- pruef: "text" in pfad -->  /  <!-- pruef-nicht: /regex/ in pfad -->
const MUSTER = /<!--\s*(pruef|pruef-nicht):\s*(?:"([^"]*)"|\/((?:[^/\\]|\\.)*)\/)\s+in\s+(\S+?)\s*-->/g;

const dateiCache = new Map();
function inhalt(pfad) {
  if (!dateiCache.has(pfad)) {
    dateiCache.set(pfad, existsSync(pfad) ? readFileSync(pfad, "utf8") : null);
  }
  return dateiCache.get(pfad);
}

console.log("\nNachprüfbare Behauptungen des Handbuchs");

for (const s of seiten) {
  const md = readFileSync(s.datei, "utf8");
  const zeilen = md.split("\n");

  // Welche Zeilen liegen in einem HTML-Kommentar? Ueber einem Anker steht oft
  // eine mehrzeilige Begruendung; deren Mittelzeilen sehen wie gewoehnlicher
  // Text aus. "Betroffener Satz: <Begruendung>" hilft aber niemandem.
  const imKommentar = [];
  let offen = false;
  for (const zeile of zeilen) {
    const beginnt = zeile.includes("<!--");
    const endet = zeile.includes("-->");
    imKommentar.push(offen || beginnt);
    if (beginnt && !endet) offen = true;
    else if (endet) offen = false;
  }

  let treffer = 0;

  for (const m of md.matchAll(MUSTER)) {
    geprueft++;
    treffer++;
    const [ganz, art, woertlich, regex, ziel] = m;
    const zeile = md.slice(0, m.index).split("\n").length;
    const ort = `${s.datei}:${zeile}`;
    const was = woertlich !== undefined ? `"${woertlich}"` : `/${regex}/`;

    const quelle = inhalt(ziel);
    if (quelle === null) {
      pruefe(false, `${ort} — Zieldatei ${ziel} gibt es nicht`);
      continue;
    }

    let vorhanden;
    if (woertlich !== undefined) {
      // \n in der Anmerkung als echten Zeilenumbruch lesen, damit sich auch
      // mehrzeilige Stellen (etwa #[default] über zwei Zeilen) verankern lassen.
      vorhanden = quelle.includes(woertlich.replace(/\\n/g, "\n"));
    } else {
      try {
        vorhanden = new RegExp(regex, "m").test(quelle);
      } catch (e) {
        pruefe(false, `${ort} — unbrauchbarer regulärer Ausdruck ${was}: ${e.message}`);
        continue;
      }
    }

    const sollDa = art === "pruef";
    const ok = vorhanden === sollDa;
    // Der Satz ueber der Anmerkung — damit die Meldung sagt, WAS falsch wird.
    // Kommentarzeilen und Leerzeilen ueberspringen: Ueber einem Anker steht
    // oft eine Begruendung, und "Betroffener Satz: <Begruendung>" hilft
    // niemandem.
    let satz = "";
    for (let z = zeile - 2; z >= 0 && z > zeile - 15; z--) {
      const kandidat = (zeilen[z] || "").trim();
      if (kandidat === "" || imKommentar[z] || kandidat.startsWith("|---")) continue;
      satz = kandidat.replace(/^[>|*-]\s*/, "").slice(0, 80);
      break;
    }
    pruefe(
      ok,
      ok
        ? `${ort} — ${was} ${sollDa ? "steht" : "fehlt"} in ${ziel}`
        : `${ort} — ${was} ${sollDa ? "steht NICHT MEHR" : "steht wieder"} in ${ziel}` +
            `\n     Betroffener Satz: „${satz}"` +
            `\n     → Handbuch nachziehen oder die Anmerkung anpassen.`
    );
  }

  if (treffer === 0 && s.pruefungen !== false) {
    // Kein harter Fehler: Nicht jedes Kapitel hat verankerbare Fakten
    // (Erklaerstuecke etwa). Sichtbar bleibt es trotzdem.
    console.log(`  · ${s.datei} — keine verankerten Behauptungen`);
  }
}

console.log(`\n${geprueft} Behauptungen geprüft.`);
console.log(fehler === 0 ? "OK" : `${fehler} stimmen nicht mehr`);
process.exit(fehler === 0 ? 0 : 1);
