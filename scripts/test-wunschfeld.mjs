// Testet, welche Felder der Wunschfeld-Wähler im TL-Web anbietet
// (src/io/wunschfeld.mjs), und vergleicht die Inline-Kopie in tl.html auf
// denselben Eingaben.
//
// Anlass (16.11.2026): Der Wähler filterte auf `c.hall` — ein Feld, das es
// an `TlCourt` nie gab (die Halle heißt dort `location`). Der Filter griff
// deshalb nie, und niemand merkte es, weil „alle Felder" plausibel aussieht.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gleicheHalle, wunschfelder, wunschfeldGruppen } from "../src/io/wunschfeld.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// Zwei Hallen mit gleichen Feldnamen und ein Feld ohne Halle — so sieht ein
// Mehr-Hallen-Turnier im Zustand aus (`TlCourt`: court_id, court, location).
const A1 = { court_id: 11, court: "1", location: "Halle A" };
const A2 = { court_id: 12, court: "2", location: "Halle A" };
const B1 = { court_id: 21, court: "1", location: "Halle B" };
const B2 = { court_id: 22, court: "2", location: "Halle B", locked: true };
const OHNE = { court_id: 99, court: "Center" };
const ALLE = [A1, A2, B1, B2, OHNE];
const ids = (liste) => liste.map((c) => c.court_id);

console.log("— Welche Felder stehen zur Wahl? —");
ok("Hallen-Regel: nur die Halle des Spiels (und Felder ohne Halle)",
  ids(wunschfelder(ALLE, { hall: "Halle A", hall_source: "rule" })), [11, 12, 99]);
ok("Hallen-Regel: Schreibweise und Rand-Leerzeichen sind egal",
  ids(wunschfelder(ALLE, { hall: " halle b ", hall_source: "rule" })), [21, 22, 99]);
for (const quelle of ["manual", "btp", "call", "auto", "none", undefined]) {
  ok(`Halle aus „${quelle}" bindet nicht — das Wunschfeld legt sie neu fest`,
    ids(wunschfelder(ALLE, { hall: "Halle A", hall_source: quelle })), [11, 12, 21, 22, 99]);
}
ok("Spiel ohne Halle: alle Felder",
  ids(wunschfelder(ALLE, { hall: "", hall_source: "none" })), [11, 12, 21, 22, 99]);
ok("Regel ohne Hallenname bindet nicht",
  ids(wunschfelder(ALLE, { hall: "  ", hall_source: "rule" })), [11, 12, 21, 22, 99]);
ok("das schon gewählte Feld bleibt stehen, auch wenn die Regel es jetzt ausschlösse",
  ids(wunschfelder(ALLE, { hall: "Halle A", hall_source: "rule", wish_court: 21 })), [11, 12, 21, 99]);
ok("gesperrte Felder bleiben in der Liste (der Turnier-PC erklärt die Absage)",
  ids(wunschfelder(ALLE, { hall: "Halle B", hall_source: "rule" })).includes(22), true);
ok("ein Feld namens `hall` zählt nicht — die Halle steht in `location`",
  ids(wunschfelder([{ court_id: 1, court: "1", hall: "Halle A", location: "Halle B" }],
    { hall: "Halle A", hall_source: "rule" })), []);
ok("Ein-Hallen-Turnier ohne Hallen: alle Felder",
  ids(wunschfelder([{ court_id: 1, court: "1" }, { court_id: 2, court: "2" }],
    { hall: "", hall_source: "none" })), [1, 2]);
ok("keine Felder", wunschfelder(undefined, { hall: "Halle A", hall_source: "rule" }), []);
ok("kein Spiel: alle Felder", ids(wunschfelder(ALLE, undefined)), [11, 12, 21, 22, 99]);
ok("Lücken in der Liste stören nicht", ids(wunschfelder([null, A1, undefined], {})), [11]);

console.log("— Bündelung nach Halle —");
const gruppen = (felder) => wunschfeldGruppen(felder).map((g) => [g.halle, ids(g.felder)]);
ok("zwei Hallen und ein Feld ohne Halle",
  gruppen(ALLE), [["Halle A", [11, 12]], ["Halle B", [21, 22]], ["", [99]]]);
ok("Reihenfolge des ersten Auftretens, auch durcheinander",
  gruppen([B1, A1, B2, A2]), [["Halle B", [21, 22]], ["Halle A", [11, 12]]]);
ok("eine Halle: eine Gruppe", gruppen([A1, A2]), [["Halle A", [11, 12]]]);
ok("abweichende Schreibweise ist dieselbe Halle",
  gruppen([A1, { court_id: 13, court: "3", location: "halle a " }]), [["Halle A", [11, 13]]]);
ok("ohne Hallen: eine namenlose Gruppe",
  gruppen([{ court_id: 1, court: "1" }, { court_id: 2, court: "2" }]), [["", [1, 2]]]);
ok("leer", gruppen([]), []);
ok("gar nichts", gruppen(undefined), []);

console.log("— gleicheHalle —");
ok("gleich", gleicheHalle("Halle A", "Halle A"), true);
ok("Schreibweise", gleicheHalle("HALLE A", " halle a"), true);
ok("verschieden", gleicheHalle("Halle A", "Halle B"), false);
ok("leer und fehlend sind gleich", gleicheHalle("", undefined), true);

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
console.log("— Inline-Kopie in tl.html —");
const hier = dirname(fileURLToPath(import.meta.url));
const tl = readFileSync(join(hier, "..", "src-tauri", "assets", "tl.html"), "utf8");
const namen = ["gleicheHalle", "wunschfelder", "wunschfeldGruppen"];
const quellen = namen.map((n) => funktion(tl, n));
ok("tl.html trägt alle drei Kopien", quellen.map(Boolean), [true, true, true]);
if (quellen.every(Boolean)) {
  // eslint-disable-next-line no-new-func
  const kopie = new Function(quellen.join("\n") + `\nreturn { ${namen.join(", ")} };`)();
  const spiele = [undefined, {}];
  for (const hall of ["", "Halle A", "halle b ", "Halle C"])
    for (const hall_source of ["rule", "manual", "btp", "call", "auto", "none", undefined])
      for (const wish_court of [undefined, 21, 99, 7])
        spiele.push({ hall, hall_source, wish_court });
  const listen = [undefined, [], ALLE, [B1, A1, B2, A2], [OHNE], [null, A1]];
  const lauf = (f) => listen.map((l) => spiele.map((m) => ids(f.wunschfelder(l, m))));
  ok("Kopie wählt dieselben Felder wie das Modul",
    lauf(kopie), lauf({ wunschfelder }));
  ok("Kopie bündelt wie das Modul",
    listen.map((l) => kopie.wunschfeldGruppen(l && l.filter(Boolean))),
    listen.map((l) => wunschfeldGruppen(l && l.filter(Boolean))));
  // Der Wähler muss die Regel auch anwenden — und der alte Fehler darf
  // nicht zurückkommen.
  ok("der Wähler fragt wunschfelder", /wunschfelder\(state\.courts, m\)/.test(tl), true);
  // Nur im Baustein des Wählers suchen: Anderswo in der Seite darf ein
  // `c` mit einem Feld `hall` jederzeit entstehen, ohne dass es ein Fehler
  // wäre.
  const von = tl.indexOf("if (state.can_set_wish_court)");
  const bis = tl.indexOf('aria-label="Wunschfeld"', von);
  ok("der Baustein des Wählers ist auffindbar", von >= 0 && bis > von, true);
  ok("der Wähler liest kein `c.hall` (TlCourt kennt nur `location`)",
    /\bc\.hall\b/.test(tl.slice(von, bis)), false);
}

if (failures) { console.error(`\n${failures} Test(s) fehlgeschlagen`); process.exit(1); }
console.log("\nalle Tests grün");
