// Testet die Zuständigkeitsgrenze des Teil-Patches (src/io/courtPatch.mjs,
// Spec monitor-livestand-push S5) — das echte Modul, dessen Inline-Kopie die
// Feld-Übersicht trägt. Je Bedingung ein Fall.
import {
  istPatchbar,
  sichtSignatur,
  satzstandGleich,
  istLive,
  hatBegonnen,
} from "../src/io/courtPatch.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const TIMER = { enabled: true, secondCallMinutes: 2, thirdCallMinutes: 4 };

/** Ein laufendes Feld im Normalzustand. */
function feld(extra) {
  return Object.assign(
    {
      court_id: 101,
      court: "Feld 1",
      location: "Halle A",
      hall_color: "#eab308",
      match_id: 7,
      match_name: "HE-A G1",
      team1: ["Anna Berg"],
      team2: ["Bea Klar"],
      team1_nationalities: ["GER"],
      team2_nationalities: ["POL"],
      sets: [[11, 8]],
      serving_team: 1,
      injury: false,
      official_call: false,
      on_court_since_ms: 0,
    },
    extra || {},
  );
}

const p = (a, b, brett = true, timer = TIMER) => istPatchbar(a, b, brett, timer);

// ── Der Normalfall: ein gezählter Punkt ───────────────────────────────────
ok("ein Punkt im laufenden Satz ist patchbar", p(feld(), feld({ sets: [[12, 8]] })), true);
ok("unveränderter Stand ist patchbar", p(feld(), feld()), true);

// ── Alles, was den Aufbau der Karte berührt ───────────────────────────────
ok("Satzwechsel", p(feld(), feld({ sets: [[21, 8], [1, 0]] })), false);
ok("Match-Wechsel", p(feld(), feld({ match_id: 8 })), false);
ok("Feld wird frei", p(feld(), feld({ match_id: 0, team1: [] })), false);
ok("Feld wird belegt", p(feld({ match_id: 0, team1: [] }), feld()), false);

// Ein durchgehend freies Feld ist patchbar — es gibt nichts zu tun. Stünde
// hier `false`, wäre die ganze Etappe wirkungslos: Die Übersicht listet ALLE
// Felder, in jeder Halle ist ständig eines zwischen zwei Spielen, und ein
// einziges "nein" zwingt das ganze Brett in den Neubau.
const frei = feld({ match_id: 0, team1: [], team2: [], sets: [] });
ok("durchgehend freies Feld ist patchbar", p(frei, frei), true);
ok(
  "freies Feld mit gewechselter Halle nicht",
  p(frei, Object.assign({}, frei, { location: "Halle B" })),
  false,
);
ok("Behandlungspause beginnt", p(feld(), feld({ injury: true })), false);
ok("Turnierleitung gerufen", p(feld(), feld({ official_call: true })), false);
ok("Feldname geändert", p(feld(), feld({ court: "Feld 2" })), false);
ok("Runde/Gruppe geändert", p(feld(), feld({ match_name: "HE-A G2" })), false);
ok("Halle geändert", p(feld(), feld({ location: "Halle B" })), false);
ok("Hallen-Farbe geändert", p(feld(), feld({ hall_color: "#14b8a6" })), false);
ok("Spielername geändert", p(feld(), feld({ team1: ["Carla Neu"] })), false);
ok("Nation geändert", p(feld(), feld({ team1_nationalities: ["AUT"] })), false);
ok("anderes Feld", p(feld(), feld({ court_id: 102 })), false);

// ── Brett-Anordnung ───────────────────────────────────────────────────────
ok("geänderte Feld-Menge/Reihenfolge", p(feld(), feld({ sets: [[12, 8]] }), false), false);

// ── Aufruf-Uhr ────────────────────────────────────────────────────────────
// Sie erscheint nur in der Wartephase. Der erste gezählte Punkt lässt sie
// verschwinden — dabei ändert sich der Aufbau der Karte.
// Isoliert geprüft: gleiche Satzanzahl, nur der Aufschlag kommt hinzu. So
// hängt das Ergebnis wirklich an der Uhr — mit einem leeren `sets` gegen
// einen ersten Punkt griffe schon die Satz-Bedingung.
const wartend = feld({ sets: [[0, 0]], serving_team: 0, on_court_since_ms: 1_700_000_000_000 });
const beginnt = feld({ sets: [[0, 0]], serving_team: 1, on_court_since_ms: 1_700_000_000_000 });
ok("Spielbeginn lässt die Uhr verschwinden", p(wartend, beginnt), false);
ok("zwei wartende Stände sind patchbar", p(wartend, wartend), true);
ok(
  "ohne eingeschalteten Aufruf-Timer gibt es keine Uhr, also keinen Wechsel",
  p(wartend, beginnt, true, { enabled: false }),
  true,
);
ok("ohne Aufruf-Zeit gibt es keine Uhr", p(feld(), feld({ sets: [[12, 8]] })), true);
// Und der erste Punkt aus dem leeren Satz heraus bleibt ein Neubau — dort
// greift die Satz-Bedingung.
ok(
  "erster Punkt aus dem leeren Satz",
  p(feld({ sets: [], serving_team: 0 }), feld({ sets: [[1, 0]] })),
  false,
);

// ── Grenzfälle ────────────────────────────────────────────────────────────
ok("kein vorheriger Stand", p(null, feld()), false);
ok("kein neuer Stand", p(feld(), null), false);

// ── Hilfsfunktionen ───────────────────────────────────────────────────────
ok("istLive: belegt", istLive(feld()), true);
ok("istLive: ohne Match", istLive(feld({ match_id: 0 })), false);
ok("istLive: ohne Mannschaft", istLive(feld({ team1: [] })), false);
ok("hatBegonnen: mit Aufschlag", hatBegonnen(feld({ sets: [], serving_team: 2 })), true);
ok("hatBegonnen: mit Punkten", hatBegonnen(feld({ sets: [[0, 1]], serving_team: 0 })), true);
ok("hatBegonnen: 0:0 ohne Aufschlag", hatBegonnen(feld({ sets: [[0, 0]], serving_team: 0 })), false);
// Satzstände kommen im LAN als Paare, in der Cloud als Objekte.
ok("hatBegonnen: Objekt-Form", hatBegonnen(feld({ sets: [{ a: 3, b: 2 }], serving_team: 0 })), true);

// ── Nur geänderte Felder anfassen ─────────────────────────────────────────
ok("gleicher Satzstand", satzstandGleich(feld(), feld()), true);
ok("ein Punkt mehr", satzstandGleich(feld(), feld({ sets: [[12, 8]] })), false);
ok("Punkt beim Gegner", satzstandGleich(feld(), feld({ sets: [[11, 9]] })), false);
ok("Satz mehr", satzstandGleich(feld(), feld({ sets: [[11, 8], [0, 0]] })), false);
ok("leer gegen leer", satzstandGleich(feld({ sets: [] }), feld({ sets: [] })), true);
// LAN liefert Paare, die Cloud Objekte — beide Formen müssen vergleichbar sein.
ok(
  "Paar- und Objektform sind vergleichbar",
  satzstandGleich(feld({ sets: [[11, 8]] }), feld({ sets: [{ a: 11, b: 8 }] })),
  true,
);

// ── Sicht-Signatur ────────────────────────────────────────────────────────
// Die Signatur, die auch wirklich ausgeliefert wird: Felder + Rotationsstand
// + Filter. Ohne den Rotationsstand bliebe die Hallen-Rotation stehen.
const brett = [
  { court_id: 101, location: "Halle A" },
  { court_id: 102, location: "Halle A" },
];
const sig = (c, idx, filter) => sichtSignatur(c, idx, filter);
ok("gleiche Anordnung", sig(brett, 0, "") === sig(brett.slice(), 0, ""), true);
ok("vertauschte Reihenfolge fällt auf", sig(brett, 0, "") === sig([brett[1], brett[0]], 0, ""), false);
ok("ein Feld weniger fällt auf", sig(brett, 0, "") === sig([brett[0]], 0, ""), false);
ok(
  "gewechselte Halle fällt auf",
  sig(brett, 0, "") === sig([{ court_id: 101, location: "Halle B" }, brett[1]], 0, ""),
  false,
);
ok("weitergedrehte Rotation fällt auf", sig(brett, 0, "") === sig(brett, 1, ""), false);
ok("gesetzter Hallenfilter fällt auf", sig(brett, 0, "") === sig(brett, 0, "Halle A"), false);

if (failures > 0) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
