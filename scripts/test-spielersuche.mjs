// Testet die Entscheidungs- und Textlogik der Spielersuche
// (src/io/spielersuche.mjs, Spec tl-spielersuche) und lässt die
// Inline-Kopien aus tl.html auf denselben Eingaben gegen das Modul laufen.
// Eine abweichende Kopie schickte die Turnierleitung an eine Zeile, die gar
// nicht in der Liste steht — oder stellte still einen Filter um.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { suchbereit, sprungZiel, rollenText, fehlerText, mehrText } from "../src/io/spielersuche.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── suchbereit ──
ok("ein Zeichen", suchbereit("m"), false);
ok("zwei Zeichen", suchbereit("mü"), true);
ok("nur Trenner", suchbereit(" -- "), false);
ok("Trenner zählen nicht", suchbereit("a-"), false);
ok("65 Zeichen", suchbereit("x".repeat(65)), false);
ok("leer/undefined", suchbereit(undefined), false);

// ── sprungZiel ──
const basis = { panelSichtbar: true, panelZu: false, inListe: true, platz: 7, halle: "", hallFilter: "", mpVerborgen: false };
const fall = (gruppe, mehr) => sprungZiel(gruppe, { ...basis, ...mehr });
ok("Zeile erreichbar", fall("waiting", {}), { art: "zeile" });
ok("Panel ausgeblendet", fall("waiting", { panelSichtbar: false }), { art: "unsichtbar", platz: 7 });
ok("hinter dem Deckel", fall("finished", { inListe: false, platz: 41 }), { art: "unsichtbar", platz: 41 });
ok("ausgeblendet schlägt Filter", fall("court", { panelSichtbar: false, halle: "B", hallFilter: "A" }), { art: "unsichtbar", platz: 7 });
ok("andere Halle (Feld)", fall("court", { halle: "B", hallFilter: "A" }), { art: "halle", halle: "B" });
ok("andere Halle (Warteliste)", fall("waiting", { halle: "B", hallFilter: "A" }), { art: "halle", halle: "B" });
ok("hallenloses Wartespiel passt überall", fall("waiting", { halle: "", hallFilter: "A" }), { art: "zeile" });
ok("Beendet kennt keinen Hallenfilter", fall("finished", { halle: "B", hallFilter: "A" }), { art: "zeile" });
ok("Filter nur spielbereite", fall("waiting", { mpVerborgen: true }), { art: "filterMp" });
ok("mp-Filter betrifft Felder nicht", fall("court", { mpVerborgen: true }), { art: "zeile" });
ok("Halle vor mp-Filter", fall("waiting", { mpVerborgen: true, halle: "B", hallFilter: "A" }), { art: "halle", halle: "B" });
ok("Panel zugeklappt", fall("finished", { panelZu: true }), { art: "panelZu" });
ok("Filter vor Aufklappen", fall("court", { panelZu: true, halle: "B", hallFilter: "A" }), { art: "halle", halle: "B" });

// ── Texte ──
ok("Rollen", rollenText(["player", "sr", "ar", "candidate", "x"]), ["Spieler", "SR", "AR", "möglicher Teilnehmer"]);
ok("400", fehlerText(400), "Bitte anders suchen – 2 bis 64 Zeichen, höchstens 6 Wörter.");
ok("401", fehlerText(401), "Dieses Gerät ist nicht mehr freigegeben.");
ok("429", fehlerText(429), "Zu viele Anfragen – kurz warten.");
ok("503", fehlerText(503), "Turnier-PC nicht erreichbar.");
ok("Netz", fehlerText(0), "Turnier-PC nicht erreichbar.");
ok("mehr 0", mehrText(0), "");
ok("mehr 70", mehrText(70), "weitere 70 – bitte genauer suchen");

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
const modul = { suchbereit, sprungZiel, rollenText, fehlerText, mehrText };
const eingaben = {
  suchbereit: [["m"], ["mü"], [" -- "], ["a-"], ["x".repeat(64)], ["x".repeat(65)], [undefined], ["Max Müller"]],
  sprungZiel: [],
  rollenText: [[["player", "sr"]], [["candidate"]], [[]], [undefined]],
  fehlerText: [[400], [401], [429], [503], [0], [500]],
  mehrText: [[0], [1], [70]],
};
for (const gruppe of ["court", "waiting", "finished"]) {
  for (const panelSichtbar of [true, false]) for (const panelZu of [true, false])
    for (const inListe of [true, false]) for (const halle of ["", "A", "B"])
      for (const hallFilter of ["", "A"]) for (const mpVerborgen of [true, false]) {
        eingaben.sprungZiel.push([gruppe, { panelSichtbar, panelZu, inListe, platz: 3, halle, hallFilter, mpVerborgen }]);
      }
}
for (const name of Object.keys(modul)) {
  const quelle = funktion(tl, name);
  ok(`tl.html trägt ${name}`, !!quelle, true);
  if (!quelle) continue;
  // eslint-disable-next-line no-new-func
  const kopie = new Function(quelle + `\nreturn ${name};`)();
  ok(`Kopie ${name} verhält sich wie das Modul`,
    eingaben[name].map((args) => kopie(...args)),
    eingaben[name].map((args) => modul[name](...args)));
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
