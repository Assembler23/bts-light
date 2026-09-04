// Testet die Seitenlogik der Zähltafel (src/io/tafelSeiten.mjs, Spec
// zaehltafel-anzeige-huelle) — das echte Modul, dessen Inline-Kopie
// tafel.html trägt.
import { tafelSeiten } from "../src/io/tafelSeiten.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}
const seite = (punkte, saetze, aufschlag) => ({ punkte, saetze, aufschlag });

// ── Laufendes Spiel, Team 1 links, Team 1 schlägt auf ────────────────────
const laufend = [{ a: 21, b: 18 }, { a: 7, b: 5 }];
const cs1 = { teamOnSide: { a: "left", b: "right" }, serving: { team: "a", index: 0 } };
ok("laufend: letzter Satz groß, Satzstand 1:0, Punkt links",
  tafelSeiten(laufend, cs1, false),
  { links: seite(7, 1, true), rechts: seite(5, 0, false), entschieden: false });

// ── Seitenwechsel: Team 1 steht rechts ───────────────────────────────────
const cs2 = { teamOnSide: { a: "right", b: "left" }, serving: { team: "b", index: 1 } };
ok("Seitenwechsel: Zahlen tauschen, Aufschlag folgt Team 2 nach links",
  tafelSeiten(laufend, cs2, false),
  { links: seite(5, 0, true), rechts: seite(7, 1, false), entschieden: false });

// ── Spiegeln wirkt NACH der Seitenbestimmung ──────────────────────────────
ok("spiegel: links und rechts vertauscht",
  tafelSeiten(laufend, cs1, true),
  { links: seite(5, 0, false), rechts: seite(7, 1, true), entschieden: false });

// ── Ohne teamOnSide / ohne courtState: Team 1 links, kein Aufschlag ──────
ok("ohne teamOnSide: Team 1 links, kein Punkt",
  tafelSeiten(laufend, { serving: { team: "a" } }, false),
  { links: seite(7, 1, false), rechts: seite(5, 0, false), entschieden: false });
ok("ohne courtState (Papierzettel): Team 1 links, kein Punkt",
  tafelSeiten(laufend, null, false),
  { links: seite(7, 1, false), rechts: seite(5, 0, false), entschieden: false });
ok("ohne courtState, gespiegelt: Team 2 links",
  tafelSeiten(laufend, null, true),
  { links: seite(5, 0, false), rechts: seite(7, 1, false), entschieden: false });

// ── serving null (vor dem ersten Ballwechsel), Fallback servingSide ─────
ok("serving null, servingSide left: Aufschlag auf der linken Seite",
  tafelSeiten([{ a: 0, b: 0 }], { teamOnSide: { a: "left", b: "right" }, serving: null, servingSide: "left" }, false),
  { links: seite(0, 0, true), rechts: seite(0, 0, false), entschieden: false });
ok("weder serving noch servingSide: kein Punkt",
  tafelSeiten([{ a: 0, b: 0 }], { teamOnSide: { a: "left", b: "right" } }, false),
  { links: seite(0, 0, false), rechts: seite(0, 0, false), entschieden: false });

// ── finished: Geistersatz weg, letzter Satz groß UND gezählt, kein Punkt ─
const fertig = [{ a: 21, b: 18 }, { a: 21, b: 15 }, { a: 0, b: 0 }];
ok("finished: 0:0-Geistersatz gestrichen, Satzstand 2:0, kein Aufschlag",
  tafelSeiten(fertig, { teamOnSide: { a: "left", b: "right" }, finished: true, serving: { team: "a" } }, false),
  { links: seite(21, 2, false), rechts: seite(15, 0, false), entschieden: true });
ok("finished ohne Geistersatz: letzter Satz zählt trotzdem",
  tafelSeiten([{ a: 21, b: 18 }, { a: 19, b: 21 }, { a: 21, b: 12 }], { finished: true }, false),
  { links: seite(21, 2, false), rechts: seite(12, 1, false), entschieden: true });

// ── retired: unvollständiger Satz groß, zählt NICHT ──────────────────────
ok("retired: letzter Satz 11:9 groß, Satzstand nur aus fertigen Sätzen",
  tafelSeiten([{ a: 21, b: 18 }, { a: 11, b: 9 }], { finished: true, retired: true, retiredWinner: "a" }, false),
  { links: seite(11, 1, false), rechts: seite(9, 0, false), entschieden: true });

// ── Unfug schadet nicht ───────────────────────────────────────────────────
ok("leere Satzliste: 0:0, Satzstand 0:0",
  tafelSeiten([], null, false),
  { links: seite(0, 0, false), rechts: seite(0, 0, false), entschieden: false });
ok("kein Array: wie leer",
  tafelSeiten(undefined, undefined, undefined),
  { links: seite(0, 0, false), rechts: seite(0, 0, false), entschieden: false });
ok("Punkte als String/NaN werden 0",
  tafelSeiten([{ a: "x", b: null }], null, false),
  { links: seite(0, 0, false), rechts: seite(0, 0, false), entschieden: false });

if (failures > 0) { console.error(`${failures} Fehler`); process.exit(1); }
console.log("alle Tafel-Seiten-Tests grün");
