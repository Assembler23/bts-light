// Satzpunkt-Ansage im Schiri-Modus (src/io/satzpunkt.mjs): Das Wort steht
// nur beim ERSTEN Satzpunkt des Satzes und an der Obergrenze in der Ansage — für 21er-, 15er- und
// 11er-Sätze samt Verlängerung und Obergrenze. Dazu ein Durchlauf durch die
// echten Ansage-Funktionen aus tablet.html (Stand, Undo-Stapel, Seiten-
// wechsel), damit die Verdrahtung im Tablet mitgeprüft ist.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { satzpunktSeiten, punkteTeams, satzpunktAnsagen } from "../src/io/satzpunkt.mjs";

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
ok("Teams: a rechts", punkteTeams({ left: 20, right: 15 }, { a: "right", b: "left" }), { a: 15, b: 20 });

const T = (a, b) => ({ a, b });
ok("erster Satzpunkt → ansagen", satzpunktAnsagen(T(20, 15), T(19, 15), 21, 30), true);
ok("besteht weiter → nicht", satzpunktAnsagen(T(20, 16), T(20, 15), 21, 30), false);
ok("Verlängerung 21:20 → nicht", satzpunktAnsagen(T(21, 20), T(20, 20), 21, 30), false);
ok("andere Mannschaft 20:21 → nicht", satzpunktAnsagen(T(20, 21), T(20, 20), 21, 30), false);
ok("29:28 → ansagen", satzpunktAnsagen(T(29, 28), T(28, 28), 21, 30), true);
ok("29:29 → ansagen", satzpunktAnsagen(T(29, 29), T(29, 28), 21, 30), true);
ok("kein Satzpunkt → nicht", satzpunktAnsagen(T(20, 20), null, 21, 30), false);
ok("vorher unbekannt → ansagen", satzpunktAnsagen(T(21, 20), null, 21, 30), true);

// Ballwechsel für Ballwechsel: welche Stände sagen das Wort an?
function verlauf(ziel, cap, punkte) {
  let st = { left: 0, right: 0 };
  const angesagt = [];
  for (const p of punkte) {
    const neu = { ...st, [p]: st[p] + 1 };
    if (satzpunktAnsagen(punkteTeams(neu), punkteTeams(st), ziel, cap)) angesagt.push(`${neu.left}:${neu.right}`);
    st = neu;
  }
  return angesagt;
}
const L = (n) => Array(n).fill("left"), R = (n) => Array(n).fill("right");
ok("21er: 20:15 → 20:19 nur einmal", verlauf(21, 30, [...R(15), ...L(20), ...R(4)]), ["20:15"]);
ok("21er: Verlängerung ohne erneutes Wort",
  verlauf(21, 30, [...L(19), ...R(19), "left", "right", "left", "right", "right"]),
  ["20:19"]);
ok("21er: bis 29:29 nur der erste, dann 29:28 und 29:29",
  verlauf(21, 30, [...L(19), ...R(19), ...Array(10).fill(0).flatMap(() => ["left", "right"])]),
  ["20:19", "29:28", "29:29"]);
ok("21er: 28:29 → 29:29 angesagt",
  verlauf(21, 30, [...L(19), ...R(19), ...Array(9).fill(0).flatMap(() => ["left", "right"]), "right", "left"]).slice(-2),
  ["28:29", "29:29"]);
ok("15er: 20:20 angesagt",
  verlauf(15, 21, [...L(13), ...R(13), ...Array(7).fill(0).flatMap(() => ["left", "right"])]),
  ["14:13", "20:19", "20:20"]);
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
  "satzpunktSeiten", "punkteTeams", "satzpunktAnsagen", "umpVorigerStand", "umpPunktLage", "umpScoreSpoken"];
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
  ok("Tablet: Marke bleibt, solange der Satzpunkt besteht", f.umpPunktLage().wort, "Spielpunkt");
  // Undo zurück auf 14:9 → der Satzpunkt ist wieder der neu erreichte.
  STATE.currentSet = STATE.history.pop().currentSet;
  STATE.servingSide = "left";
  ok("Tablet: nach Undo wieder angesagt", f.umpScoreSpoken(), "14 Spielpunkt 9");
}
{
  // Verlängerung: nach dem ersten Satzpunkt nur noch der Stand.
  const { ansagen } = spiel(21, 30, 3, [], [...L(19), ...R(19), "left", "right", "left", "right", "right"]);
  ok("Tablet 21er: Verlängerung", ansagen.slice(-5), ["20 Satzpunkt 19", "20 beide", "21:20", "21 beide", "22:21"]);
}
{
  const { ansagen } = spiel(21, 30, 1, [], [...L(19), ...R(19), ...Array(10).fill(0).flatMap(() => ["left", "right"])]);
  ok("Tablet 21er: an der Obergrenze", ansagen.slice(-2), ["29 Spielpunkt 28", "29 Spielpunkt beide"]);
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

{
  // „Mannschaften tauschen" bei 29:28 erfindet keinen Satzpunkt.
  const { STATE, f } = spiel(21, 30, 3, [], []);
  STATE.history.push({ currentSet: { left: 29, right: 28 }, setsCompleted: [], teamOnSide: { a: "left", b: "right" },
    korrektur: { art: "ab" } });
  STATE.teamOnSide = { a: "right", b: "left" };
  STATE.currentSet = { left: 29, right: 28 };
  ok("Tablet: Korrektur ohne neues Wort", f.umpScoreSpoken(), "29:28");
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
