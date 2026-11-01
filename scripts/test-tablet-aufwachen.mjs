// Testet, was das Zähl-Tablet beim Aufwachen tut (src/io/tabletAufwachen.mjs),
// und dass die Inline-Kopie in tablet.html wortgleich ist.
//
// Warum das wichtig ist: Falsch entschieden, baut das Tablet entweder eine
// zweite Verbindung neben der ersten auf (Feld „belegt", Bediener muss
// übernehmen) oder bleibt nach dem Aufklappen an einer toten Verbindung
// hängen und bekommt sein nächstes Spiel erst nach dem Wächter-Takt.
import { readFileSync } from "node:fs";
import { aufwachAktion, probeGescheitert, AUFWACH_STILLE_MS } from "../src/io/tabletAufwachen.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got);
  const w = JSON.stringify(want);
  if (g !== w) {
    console.error(`✗ ${name}: erwartet ${w}, war ${g}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const T = 1_700_000_000_000;

// ── Entscheidung beim Aufwachen ───────────────────────────────────────────
ok("kein Socket → sofort verbinden", aufwachAktion(null, 0, T), "verbinden");
ok("Socket zu → sofort verbinden", aufwachAktion(3, T - 1000, T), "verbinden");
// Nicht anfassen, was gerade aufgebaut oder abgebaut wird: sonst zwei Sockets.
ok("Socket verbindet → warten", aufwachAktion(0, 0, T), "warten");
ok("Socket schließt → warten", aufwachAktion(2, T - 60_000, T), "warten");
// Die typische Lage nach zugeklappter Hülle: offen gemeldet, aber lange still.
ok("offen, aber lange still → ersetzen", aufwachAktion(1, T - 5 * 60_000, T), "ersetzen");
ok("offen, genau an der Schwelle → noch prüfen",
  aufwachAktion(1, T - AUFWACH_STILLE_MS, T), "pruefen");
ok("offen, knapp über der Schwelle → ersetzen",
  aufwachAktion(1, T - AUFWACH_STILLE_MS - 1, T), "ersetzen");
ok("offen, nie Kontakt gehabt → ersetzen", aufwachAktion(1, 0, T), "ersetzen");
// Kurz verdeckt (Wisch, Benachrichtigung): Verbindung ist frisch → nur proben.
ok("offen und frisch → prüfen", aufwachAktion(1, T - 2000, T), "pruefen");

// ── Probe nach dem Ping ───────────────────────────────────────────────────
ok("Antwort nach dem Ping → lebt", probeGescheitert(T + 200, T), false);
ok("Kontakt genau beim Ping → lebt", probeGescheitert(T, T), false);
ok("letzter Kontakt vor dem Ping → tot", probeGescheitert(T - 1, T), true);
ok("nie Kontakt → tot", probeGescheitert(0, T), true);

// ── Inline-Kopie wortgleich ───────────────────────────────────────────────
const START = "/* <tabletAufwachen.mjs> */";
const ENDE = "/* </tabletAufwachen.mjs> */";
function abschnitt(text, wo) {
  const a = text.indexOf(START);
  const b = text.indexOf(ENDE);
  if (a < 0 || b < a) {
    console.error(`✗ Marken fehlen in ${wo}`);
    failures++;
    return "";
  }
  return text.slice(a, b).split("\n").map((z) => z.trim()).join("\n");
}
const modul = abschnitt(readFileSync(new URL("../src/io/tabletAufwachen.mjs", import.meta.url), "utf8"), "Modul");
const kopie = abschnitt(readFileSync(new URL("../src-tauri/assets/tablet.html", import.meta.url), "utf8"), "tablet.html");
// Der Abschnitt muss die Logik wirklich tragen — sonst wären zwei leere
// (oder gleich abgeschnittene) Abschnitte auch „wortgleich".
ok("Abschnitt im Modul trägt beide Funktionen",
  modul.includes("function aufwachAktion(") && modul.includes("function probeGescheitert("), true);
ok("Inline-Kopie in tablet.html wortgleich", kopie === modul, true);
// Genau eine Kopie: eine zweite, veraltete würde die erste still überschreiben.
const tabletHtml = readFileSync(new URL("../src-tauri/assets/tablet.html", import.meta.url), "utf8");
ok("genau eine Kopie in tablet.html", tabletHtml.split(START).length - 1, 1);

if (failures > 0) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
