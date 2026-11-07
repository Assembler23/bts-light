// Testet die Logik der Meetingpoint-Seite (src/io/meetingpoint.mjs) und
// prüft, dass die Inline-Kopie in src-tauri/assets/meetingpoint.html sich
// genauso verhält (Spec meetingpoint-spielbereit). Die Kopie wird aus der
// Seite geschnitten, ausgeführt und auf denselben Eingaben verglichen.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as mp from "../src/io/meetingpoint.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const P = (seite, pos, name, present) => ({ platz: { art: "spieler", seite, pos }, name, present });
const SR = (name, present) => ({ platz: { art: "sr" }, name, present });

// ── Modul ────────────────────────────────────────────────────────────────
ok("Halle aus Query", mp.mpHalleAusQuery("?halle=Halle%20A"), "Halle A");
ok("Halle mit Plus", mp.mpHalleAusQuery("?x=1&halle=Halle+B"), "Halle B");
ok("keine Halle", mp.mpHalleAusQuery("?x=1"), "");
ok("kaputte Kodierung → leer", mp.mpHalleAusQuery("?halle=%E0%A4%A"), "");
ok("Sortierung nach Aufruf, dann ID",
  mp.mpSortiert([{ matchId: 3, calledAtMs: 20 }, { matchId: 2, calledAtMs: 10 }, { matchId: 1, calledAtMs: 20 }]).map((p) => p.matchId),
  [2, 1, 3]);
const partie = { matchId: 7, plaetze: [P(1, 0, "Anna", true), P(1, 1, "Bea", false), P(2, 0, "Cem", true), SR("Dora", false)] };
const g = mp.mpGruppen(partie);
ok("Gruppen Seite 1", g.team1.map((p) => p.name), ["Anna", "Bea"]);
ok("Gruppen Seite 2", g.team2.map((p) => p.name), ["Cem"]);
ok("Gruppen SR", g.sr && g.sr.name, "Dora");
ok("unbekannter Platz fällt heraus", mp.mpGruppen({ plaetze: [{ platz: { art: "unbekannt" }, name: "X" }] }), { team1: [], team2: [], sr: null });
ok("Zähler ohne SR-Pflicht zählt den SR nicht", mp.mpZaehler(partie), { da: 2, gesamt: 3 });
ok("Zähler mit SR-Pflicht", mp.mpZaehler({ ...partie, srNoetig: true }), { da: 2, gesamt: 4 });
ok("Check-in-Body", mp.mpCheckinBody("Halle A", 7, { art: "sr" }, 1),
  { halle: "Halle A", matchId: 7, platz: { art: "sr" }, present: true, ort: "meetingpoint" });
ok("Uhrzeit leer ohne Wert", mp.mpUhrzeit(0), "");
const J = 1_000_000_000;
ok("Feld frei: ohne Prognose leer", mp.mpFeldFrei(undefined, false, J), "");
ok("Feld frei: angebrochene Minute zählt voll", mp.mpFeldFrei(J + 5 * 60000 + 1, false, J), "Feld frei in ca. 6 min");
ok("Feld frei: unsicher → grob", mp.mpFeldFrei(J + 6 * 60000, true, J), "Feld frei in ca. 6 min (grob geschätzt)");
ok("Feld frei: überfällig → jetzt", mp.mpFeldFrei(J - 60000, false, J), "Feld wird jetzt frei");
ok("Feld frei: genau jetzt", mp.mpFeldFrei(J, false, J), "Feld wird jetzt frei");
ok("Feld frei: Stunden", mp.mpFeldFrei(J + 75 * 60000, false, J), "Feld frei in ca. 1 h 15 min");
ok("Feld frei: volle Stunde", mp.mpFeldFrei(J + 120 * 60000, true, J), "Feld frei in ca. 2 h (grob geschätzt)");

// ── Inline-Kopie ─────────────────────────────────────────────────────────
const hier = dirname(fileURLToPath(import.meta.url));
const seite = readFileSync(join(hier, "../src-tauri/assets/meetingpoint.html"), "utf8");
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
const NAMEN = ["mpHalleAusQuery", "mpSortiert", "mpGruppen", "mpZaehler", "mpCheckinBody", "mpUhrzeit", "mpFeldFrei"];
const quellen = NAMEN.map((n) => funktion(seite, n));
ok("alle Funktionen in der Seite", quellen.map((q) => q !== null), NAMEN.map(() => true));
// eslint-disable-next-line no-new-func
const kopie = new Function(quellen.join("\n") + "\nreturn {" + NAMEN.join(",") + "};")();
const FAELLE = {
  mpHalleAusQuery: [["?halle=Halle%20A"], ["?x=1&halle=B+C"], [""], ["?halle=%E0%A4%A"]],
  mpSortiert: [[[{ matchId: 3, calledAtMs: 20 }, { matchId: 2, calledAtMs: 10 }, { matchId: 1, calledAtMs: 20 }]], [null]],
  mpGruppen: [[partie], [null], [{ plaetze: [SR("Z", true)] }]],
  mpZaehler: [[partie], [{}], [{ ...partie, srNoetig: true }]],
  mpCheckinBody: [["A", 1, { art: "sr" }, true], [null, 2, { art: "spieler", seite: 1, pos: 0 }, 0]],
  mpUhrzeit: [[0], [Date.UTC(2026, 10, 2, 9, 5)]],
  mpFeldFrei: [[0, false, J], [J + 5 * 60000 + 1, false, J], [J + 6 * 60000, true, J], [J - 1, false, J], [J + 75 * 60000, false, J], [J + 120 * 60000, true, J]],
};
for (const n of NAMEN) {
  for (const args of FAELLE[n]) {
    ok(`Kopie ${n}(${JSON.stringify(args).slice(0, 40)})`, kopie[n](...args), mp[n](...args));
  }
}

if (failures) { console.error(`\n${failures} Fehler`); process.exit(1); }
console.log("\nalle Meetingpoint-Tests grün");
