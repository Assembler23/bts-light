// Testet die Ziel-Bestimmung für das Umsortieren der GERUFENEN Spiele
// (src/io/gerufeneReihe.mjs, Spec meetingpoint-reihenfolge) und prüft, dass
// die Inline-Kopie in tl.html sich genauso verhält. Ein falsches Ziel
// brächte die falsche Partie zuerst aufs Feld.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as g from "../src/io/gerufeneReihe.mjs";

let failures = 0;
function ok(name, got, want) {
  const a = JSON.stringify(got), b = JSON.stringify(want);
  if (a !== b) { console.error(`✗ ${name}: erwartet ${b}, war ${a}`); failures++; }
  else console.log(`✓ ${name}`);
}

const R = (id, hall) => ({ match_id: id, prep_call: { hall } });
const W = (id) => ({ match_id: id, prep_call: null });
// Gerufen: 1 (A), 2 (B), 3 (A), 4 (A); dann wartend 5, 6.
const L = [R(1, "A"), R(2, "B"), R(3, "A"), R(4, "A"), W(5), W(6)];

ok("Reihe der Halle A", g.rufReihe(L, 3), [1, 3, 4]);
ok("Reihe der Halle B", g.rufReihe(L, 2), [2]);
ok("wartendes Spiel hat keine Reihe", g.rufReihe(L, 5), []);

ok("Zug vor 1", g.rufZielZug(L, 4, 1), 1);
ok("Zug vor 2 (Halle B) → nächstes A danach", g.rufZielZug(L, 4, 2), 3);
ok("Zug auf wartende Zeile → ans Ende", g.rufZielZug(L, 1, 5), null);
ok("Zug ans Listenende → ans Ende", g.rufZielZug(L, 1, null), null);
ok("Zug an die eigene Stelle → keine Bewegung", g.rufZielZug(L, 3, 4), undefined);
ok("schon am Ende → keine Bewegung", g.rufZielZug(L, 4, 5), undefined);
ok("wartend → kein Ziel", g.rufZielZug(L, 5, 1), undefined);

ok("Knopf top", g.rufZielKnopf(L, 4, "top"), 1);
ok("Knopf up", g.rufZielKnopf(L, 4, "up"), 3);
ok("Knopf down", g.rufZielKnopf(L, 1, "down"), 4);
ok("Knopf down vorletztes → ans Ende", g.rufZielKnopf(L, 3, "down"), null);
ok("Knopf end", g.rufZielKnopf(L, 1, "end"), null);
ok("top am Anfang → nichts", g.rufZielKnopf(L, 1, "top"), undefined);
ok("end am Ende → nichts", g.rufZielKnopf(L, 4, "end"), undefined);
ok("allein in der Halle → nichts", g.rufZielKnopf(L, 2, "up"), undefined);

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
const NAMEN = ["rufReihe", "rufZielZug", "rufZielKnopf"];
const quellen = NAMEN.map((n) => funktion(tl, n));
ok("tl.html trägt die Kopien", quellen.map((q) => q !== null), NAMEN.map(() => true));
if (quellen.every(Boolean)) {
  // eslint-disable-next-line no-new-func
  const k = new Function(quellen.join("\n") + "\nreturn {" + NAMEN.join(",") + "};")();
  const ids = [1, 2, 3, 4, 5, 6, 99, null];
  for (const m of ids) {
    ok(`Kopie rufReihe(${m})`, k.rufReihe(L, m), g.rufReihe(L, m));
    for (const b of ids) ok(`Kopie rufZielZug(${m},${b})`, k.rufZielZug(L, m, b), g.rufZielZug(L, m, b));
    for (const a of ["top", "up", "down", "end", "x"]) {
      ok(`Kopie rufZielKnopf(${m},${a})`, k.rufZielKnopf(L, m, a), g.rufZielKnopf(L, m, a));
    }
  }
}

if (failures) { console.error(`\n${failures} Fehler`); process.exit(1); }
console.log("\nalle Tests grün");
