// Satzpunkt-Ansage im Schiri-Modus (src/io/satzpunkt.mjs): Das Wort steht
// nur beim NEU erreichten Satzpunkt in der Ansage — für 21er-, 15er- und
// 11er-Sätze samt Verlängerung und Obergrenze. Dazu ein Durchlauf durch die
// echten Ansage-Funktionen aus tablet.html (Stand, Undo-Stapel, Seiten-
// wechsel), damit die Verdrahtung im Tablet mitgeprüft ist.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { satzpunktSeiten, satzpunktTeams, satzpunktAnsagen } from "../src/io/satzpunkt.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Modul ──
ok("21er: 19:15 kein Satzpunkt", satzpunktSeiten({ left: 19, right: 15 }, 21, 30), { left: false, right: false });
ok("21er: 20:15 links", satzpunktSeiten({ left: 20, right: 15 }, 21, 30), { left: true, right: false });
ok("21er: 20:20 keiner", satzpunktSeiten({ left: 20, right: 20 }, 21, 30), { left: false, right: false });
ok("21er: 29:29 beide", satzpunktSeiten({ left: 29, right: 29 }, 21, 30), { left: true, right: true });
ok("15er (Obergrenze 21): 14:10 links", satzpunktSeiten({ left: 14, right: 10 }, 15, 21), { left: true, right: false });
ok("15er: 14:14 keiner", satzpunktSeiten({ left: 14, right: 14 }, 15, 21), { left: false, right: false });
ok("15er: 20:20 beide", satzpunktSeiten({ left: 20, right: 20 }, 15, 21), { left: true, right: true });
ok("11er (Obergrenze 15): 14:14 beide", satzpunktSeiten({ left: 14, right: 14 }, 11, 15), { left: true, right: true });
ok("ohne Satzende (99) nie", satzpunktSeiten({ left: 98, right: 0 }, 99, 99), { left: false, right: false });
ok("Teams: a rechts", satzpunktTeams({ left: 20, right: 15 }, 21, 30, { a: "right", b: "left" }), { a: false, b: true });

const T = (a, b) => ({ a, b });
ok("neu → ansagen", satzpunktAnsagen(T(true, false), T(false, false)), true);
ok("besteht weiter → nicht", satzpunktAnsagen(T(true, false), T(true, false)), false);
ok("Wechsel zur anderen Mannschaft → ansagen", satzpunktAnsagen(T(false, true), T(true, false)), true);
ok("zweite Mannschaft kommt dazu (29:29) → ansagen", satzpunktAnsagen(T(true, true), T(true, false)), true);
ok("kein Satzpunkt → nicht", satzpunktAnsagen(T(false, false), null), false);
ok("vorher unbekannt → ansagen", satzpunktAnsagen(T(true, false), null), true);

// Ballwechsel für Ballwechsel: welche Stände sagen das Wort an?
function verlauf(ziel, cap, punkte) {
  let st = { left: 0, right: 0 };
  const angesagt = [];
  for (const p of punkte) {
    const neu = { ...st, [p]: st[p] + 1 };
    const jetzt = satzpunktTeams(neu, ziel, cap);
    if (satzpunktAnsagen(jetzt, satzpunktTeams(st, ziel, cap))) angesagt.push(`${neu.left}:${neu.right}`);
    st = neu;
  }
  return angesagt;
}
const L = (n) => Array(n).fill("left"), R = (n) => Array(n).fill("right");
ok("21er: 20:15 → 20:19 nur einmal", verlauf(21, 30, [...R(15), ...L(20), ...R(4)]), ["20:15"]);
ok("21er: Verlängerung je neuer Satzpunkt",
  verlauf(21, 30, [...L(19), ...R(19), "left", "right", "left", "right", "right"]),
  ["20:19", "21:20", "21:22"]);
ok("21er: 28:28 → 29:28 → 29:29 beide angesagt",
  verlauf(21, 30, [...L(20), ...R(20), ...Array(8).fill(0).flatMap(() => ["left", "right"]), "left", "right"]).slice(-2),
  ["29:28", "29:29"]);
ok("15er: 14:10 einmal, 14:11 nicht", verlauf(15, 21, [...R(10), ...L(14), "right"]), ["14:10"]);
ok("11er: 10:5 einmal", verlauf(11, 15, [...R(5), ...L(10), "right", "right"]), ["10:5"]);

// ── Tablet: echte Ansage-Funktionen mit Undo-Stapel ──
const hier = dirname(fileURLToPath(import.meta.url));
const text = readFileSync(join(hier, "..", "src-tauri", "assets", "tablet.html"), "utf8");
function funktion(name) {
  const start = text.indexOf("function " + name + "(");
  if (start < 0) throw new Error("fehlt in tablet.html: " + name);
  let tiefe = 0;
  for (let i = text.indexOf("{", start); i < text.length; i++) {
    if (text[i] === "{") tiefe++;
    else if (text[i] === "}" && --tiefe === 0) return text.slice(start, i + 1);
  }
  throw new Error("Rumpf offen: " + name);
}
const NAMEN = ["capForTarget", "satzObergrenze", "setWinnerSide", "requiredSetsToWin", "satzSieger",
  "satzpunktSeiten", "satzpunktTeams", "satzpunktAnsagen", "umpVorigerSatzpunkt", "umpPunktLage", "umpScoreSpoken"];
// eslint-disable-next-line no-new-func
const tablet = new Function("STATE", NAMEN.map(funktion).join("\n") + "\nreturn { umpScoreSpoken, umpPunktLage };");

function spiel(ziel, cap, bestOf, saetze, punkte, teamOnSide = { a: "left", b: "right" }) {
  const STATE = {
    match: { targetScore: ziel, capScore: cap, bestOfSets: bestOf }, finished: false,
    setsCompleted: saetze, currentSet: { left: 0, right: 0 }, servingSide: "left",
    teamOnSide, history: [],
  };
  const f = tablet(STATE);
  const ansagen = [];
  for (const p of punkte) {
    STATE.history.push({ currentSet: { ...STATE.currentSet }, setsCompleted: STATE.setsCompleted.slice(),
      teamOnSide: { ...STATE.teamOnSide } });
    STATE.currentSet = { ...STATE.currentSet, [p]: STATE.currentSet[p] + 1 };
    STATE.servingSide = p;
    ansagen.push(f.umpScoreSpoken());
  }
  return { STATE, f, ansagen };
}
{
  const { ansagen } = spiel(21, 30, 3, [], [...R(15), ...L(20), "right", "left"]);
  ok("Tablet 21er: 20 Satzpunkt 15", ansagen[34], "20 Satzpunkt 15");
  ok("Tablet 21er: Aufschlagwechsel ohne Wort", ansagen[35], "16:20");
  ok("Tablet 21er: Satzende-Stand", ansagen[36], "21:16");
}
{
  const { STATE, f, ansagen } = spiel(15, 21, 3, [{ left: 15, right: 3 }], [...R(9), ...L(14), "right"]);
  ok("Tablet 15er im 2. Satz: Spielpunkt einmal", ansagen.slice(-2), ["14 Spielpunkt 9", "10:14"]);
  ok("Tablet: Abzeichen bleibt, solange der Satzpunkt besteht", f.umpPunktLage().wort, "Spielpunkt");
  // Undo zurück auf 14:9 → der Satzpunkt ist wieder der neu erreichte.
  STATE.currentSet = STATE.history.pop().currentSet;
  STATE.servingSide = "left";
  ok("Tablet: nach Undo wieder angesagt", f.umpScoreSpoken(), "14 Spielpunkt 9");
}
{
  // Ohne Undo-Stand (Einstieg mitten im Spiel) lieber ansagen.
  const { STATE, f } = spiel(21, 30, 3, [], []);
  STATE.currentSet = { left: 20, right: 17 };
  ok("Tablet: ohne Vorstand angesagt", f.umpScoreSpoken(), "20 Satzpunkt 17");
}
{
  // Seitenwechsel zwischen zwei Ständen erfindet keinen Satzpunkt.
  const { STATE, f } = spiel(21, 30, 3, [], []);
  STATE.history.push({ currentSet: { left: 20, right: 15 }, setsCompleted: [], teamOnSide: { a: "left", b: "right" } });
  STATE.teamOnSide = { a: "right", b: "left" };
  STATE.currentSet = { left: 16, right: 20 };
  STATE.servingSide = "left";
  ok("Tablet: Seitenwechsel ohne neues Wort", f.umpScoreSpoken(), "16:20");
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
